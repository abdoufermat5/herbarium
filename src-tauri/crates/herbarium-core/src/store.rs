// SQLite index for pages. Rebuildable at any time from the vault files
// (see `crate::vault::index_vault`). Not the source of truth, only an index.

use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;

use rusqlite::{Connection, OptionalExtension, Params, Row, params};

use crate::models::{PageMeta, SCHEMA_VERSION, SearchHit, TagCount};
use crate::time::now_secs;

const COLUMNS: &str = "id,title,tags,folder,note,created_at,updated_at,interval_minutes,next_review,last_review,allow_cdn,ext,source_title";

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS pages (
  id            TEXT PRIMARY KEY,
  title         TEXT NOT NULL DEFAULT '',
  tags          TEXT NOT NULL DEFAULT '[]',
  folder        TEXT,
  note          TEXT NOT NULL DEFAULT '',
  created_at    INTEGER NOT NULL,
  updated_at    INTEGER NOT NULL,
  interval_minutes INTEGER,
  next_review   INTEGER,
  last_review   INTEGER,
  allow_cdn     INTEGER NOT NULL DEFAULT 0,
  text_content  TEXT NOT NULL DEFAULT '',
  mtime         INTEGER NOT NULL DEFAULT 0,
  ext           TEXT NOT NULL DEFAULT '{}',
  source_title  TEXT,
  fingerprint   TEXT NOT NULL DEFAULT ''
);
CREATE VIRTUAL TABLE IF NOT EXISTS pages_fts USING fts5(
  id UNINDEXED, title, tags, folder, note, text, tokenize='porter'
);
CREATE TABLE IF NOT EXISTS links (
  src TEXT NOT NULL,
  dst TEXT NOT NULL,
  PRIMARY KEY (src, dst)
);
CREATE INDEX IF NOT EXISTS links_dst ON links(dst);
CREATE TABLE IF NOT EXISTS link_scans (
  src         TEXT PRIMARY KEY,
  fingerprint TEXT NOT NULL
);
";

pub struct Store {
    pub vault: PathBuf,
    conn: Connection,
}

fn row_to_meta(row: &Row) -> rusqlite::Result<PageMeta> {
    Ok(PageMeta {
        schema_version: SCHEMA_VERSION,
        id: row.get(0)?,
        title: row.get(1)?,
        tags: serde_json::from_str::<Vec<String>>(&row.get::<_, String>(2)?).unwrap_or_default(),
        folder: row.get(3)?,
        note: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
        interval_minutes: row.get(7)?,
        legacy_interval_days: None,
        next_review: row.get(8)?,
        last_review: row.get(9)?,
        allow_cdn: row.get::<_, i64>(10)? != 0,
        ext: serde_json::from_str(&row.get::<_, String>(11)?).unwrap_or_default(),
        source_title: row.get(12)?,
    })
}

impl Store {
    pub fn open(vault: PathBuf) -> rusqlite::Result<Store> {
        let dir = vault.join(".herbarium");
        std::fs::create_dir_all(&dir).ok();
        let conn = Connection::open(dir.join("index.sqlite"))?;
        conn.pragma_update(None, "journal_mode", "WAL").ok();
        conn.pragma_update(None, "busy_timeout", 5000).ok();
        conn.execute_batch(SCHEMA)?;
        migrate(&conn)?;
        Ok(Store { vault, conn })
    }

    /// Insert or refresh one page (meta, text content and file mtime).
    pub fn upsert(&self, meta: &PageMeta, text: &str, mtime: i64) -> rusqlite::Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        let tags = serde_json::to_string(&meta.tags).unwrap_or_else(|_| "[]".into());
        let ext = serde_json::to_string(&meta.ext).unwrap_or_else(|_| "{}".into());
        let fingerprint =
            crate::vault::page_fingerprint(&self.vault, &meta.id, meta.folder.as_deref());
        tx.execute(
            &format!(
                "INSERT INTO pages ({COLUMNS}, text_content, mtime, fingerprint)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)
                 ON CONFLICT(id) DO UPDATE SET
                   title=excluded.title, tags=excluded.tags, folder=excluded.folder,
                   note=excluded.note, updated_at=excluded.updated_at,
                   interval_minutes=excluded.interval_minutes, next_review=excluded.next_review,
                   last_review=excluded.last_review, allow_cdn=excluded.allow_cdn,
                   ext=excluded.ext, source_title=excluded.source_title,
                   text_content=excluded.text_content, mtime=excluded.mtime,
                   fingerprint=excluded.fingerprint"
            ),
            params![
                meta.id,
                meta.title,
                tags,
                meta.folder,
                meta.note,
                meta.created_at,
                meta.updated_at,
                meta.interval_minutes,
                meta.next_review,
                meta.last_review,
                if meta.allow_cdn { 1 } else { 0 },
                ext,
                meta.source_title,
                text,
                mtime,
                fingerprint,
            ],
        )?;
        // FTS rows share the `pages` rowid (stable across ON CONFLICT updates),
        // so replacing one is an indexed lookup instead of a full FTS scan.
        tx.execute(
            "INSERT OR REPLACE INTO pages_fts (rowid, id, title, tags, folder, note, text)
             SELECT rowid, ?1, ?2, ?3, ?4, ?5, ?6 FROM pages WHERE id = ?1",
            params![
                meta.id,
                meta.title,
                tags,
                meta.folder.as_deref().unwrap_or_default(),
                fts_note(meta),
                text,
            ],
        )?;
        tx.commit()
    }

    pub fn fingerprint_for(&self, id: &str) -> rusqlite::Result<Option<String>> {
        self.conn
            .query_row(
                "SELECT fingerprint FROM pages WHERE id = ?1",
                params![id],
                |r| r.get(0),
            )
            .optional()
    }

    pub fn delete(&self, id: &str) -> rusqlite::Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        delete_row(&tx, id)?;
        tx.commit()
    }

    pub fn get_meta(&self, id: &str) -> rusqlite::Result<Option<PageMeta>> {
        self.conn
            .query_row(
                &format!("SELECT {COLUMNS} FROM pages WHERE id = ?1"),
                params![id],
                row_to_meta,
            )
            .optional()
    }

    pub fn text_for(&self, id: &str) -> rusqlite::Result<Option<String>> {
        self.conn
            .query_row(
                "SELECT text_content FROM pages WHERE id = ?1",
                params![id],
                |r| r.get(0),
            )
            .optional()
    }

    pub fn mtime_for(&self, id: &str) -> rusqlite::Result<Option<i64>> {
        self.conn
            .query_row("SELECT mtime FROM pages WHERE id = ?1", params![id], |r| {
                r.get(0)
            })
            .optional()
    }

    fn query_metas(&self, sql: &str, params: impl Params) -> rusqlite::Result<Vec<PageMeta>> {
        let mut stmt = self.conn.prepare(sql)?;
        stmt.query_map(params, row_to_meta)?.collect()
    }

    pub fn all(&self) -> rusqlite::Result<Vec<PageMeta>> {
        self.query_metas(
            &format!("SELECT {COLUMNS} FROM pages ORDER BY title COLLATE NOCASE"),
            [],
        )
    }

    /// Pages whose `ext` JSON contains `needle`, ignoring ASCII case: a cheap
    /// prefilter before looking at one `ext` field properly.
    pub fn with_ext_containing(&self, needle: &str) -> rusqlite::Result<Vec<PageMeta>> {
        self.query_metas(
            &format!(
                "SELECT {COLUMNS} FROM pages WHERE instr(lower(ext), lower(?1)) > 0
                 ORDER BY title COLLATE NOCASE"
            ),
            params![needle],
        )
    }

    pub fn search(&self, query: &str, limit: usize) -> rusqlite::Result<Vec<SearchHit>> {
        let limit_param = if limit == usize::MAX {
            i64::MAX
        } else {
            limit as i64
        };
        let expr = fts_expr(query);
        if expr.is_empty() {
            let sql = format!("SELECT {COLUMNS} FROM pages ORDER BY title COLLATE NOCASE LIMIT ?1");
            let mut stmt = self.conn.prepare(&sql)?;
            let rows = stmt.query_map(params![limit_param], |r| {
                let meta = row_to_meta(r)?;
                Ok(SearchHit {
                    meta,
                    snippet: None,
                })
            })?;
            return rows.collect();
        }
        let page_cols = COLUMNS
            .split(',')
            .map(|col| format!("p.{col}"))
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "SELECT {page_cols}, snippet(pages_fts, 5, '[', ']', '…', 12)
             FROM pages_fts f JOIN pages p ON p.id = f.id
             WHERE pages_fts MATCH ?1
             ORDER BY bm25(pages_fts, 0, 10, 4, 2, 3, 1), p.title COLLATE NOCASE
             LIMIT ?2"
        );
        let fts_hits = self.conn.prepare(&sql).and_then(|mut stmt| {
            let rows = stmt.query_map(params![expr, limit_param], |r| {
                let meta = row_to_meta(r)?;
                let snip: Option<String> = r.get(13)?;
                let snippet = snip.filter(|s| s.contains('[') && s.contains(']'));
                Ok(SearchHit { meta, snippet })
            })?;
            rows.collect::<rusqlite::Result<Vec<_>>>()
        });
        match fts_hits {
            Ok(hits) if !hits.is_empty() => Ok(hits),
            _ => self.like_search(query, limit),
        }
    }

    fn like_search(&self, query: &str, limit: usize) -> rusqlite::Result<Vec<SearchHit>> {
        let like = like_pattern(query);
        let limit_param = if limit == usize::MAX {
            i64::MAX
        } else {
            limit as i64
        };
        let sql = format!(
            "SELECT {COLUMNS} FROM pages
             WHERE title LIKE ?1 ESCAPE '\\' OR tags LIKE ?1 ESCAPE '\\'
                OR folder LIKE ?1 ESCAPE '\\' OR note LIKE ?1 ESCAPE '\\'
                OR text_content LIKE ?1 ESCAPE '\\'
                OR source_title LIKE ?1 ESCAPE '\\'
             ORDER BY title COLLATE NOCASE LIMIT ?2"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![like, limit_param], |r| {
            let meta = row_to_meta(r)?;
            Ok(SearchHit {
                meta,
                snippet: None,
            })
        })?;
        rows.collect()
    }

    /// Bring the link index up to date: rescan every page whose files changed
    /// since its last scan (`html` reads a page's HTML; `None` skips it) and
    /// forget pages that are gone. Cheap when nothing changed.
    pub fn refresh_links(
        &self,
        html: impl Fn(&PageMeta) -> Option<String>,
    ) -> rusqlite::Result<()> {
        let stale: Vec<(String, String)> = self
            .conn
            .prepare(
                "SELECT p.id, p.fingerprint FROM pages p
                 LEFT JOIN link_scans s ON s.src = p.id
                 WHERE s.fingerprint IS NULL OR s.fingerprint != p.fingerprint",
            )?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "DELETE FROM links WHERE src NOT IN (SELECT id FROM pages)",
            [],
        )?;
        tx.execute(
            "DELETE FROM link_scans WHERE src NOT IN (SELECT id FROM pages)",
            [],
        )?;
        for (id, fingerprint) in stale {
            let Some(meta) = self.get_meta(&id)? else {
                continue;
            };
            let Some(doc) = html(&meta) else {
                continue;
            };
            tx.execute("DELETE FROM links WHERE src = ?1", params![id])?;
            for dst in crate::content::extract_page_links(&doc) {
                if dst != id {
                    tx.execute(
                        "INSERT OR IGNORE INTO links (src, dst) VALUES (?1, ?2)",
                        params![id, dst],
                    )?;
                }
            }
            tx.execute(
                "INSERT INTO link_scans (src, fingerprint) VALUES (?1, ?2)
                 ON CONFLICT(src) DO UPDATE SET fingerprint = excluded.fingerprint",
                params![id, fingerprint],
            )?;
        }
        tx.commit()
    }

    /// Every link between two pages that both exist, as `(from, to)`.
    pub fn all_links(&self) -> rusqlite::Result<Vec<(String, String)>> {
        self.conn
            .prepare(
                "SELECT l.src, l.dst FROM links l
                 JOIN pages a ON a.id = l.src JOIN pages b ON b.id = l.dst
                 ORDER BY l.src, l.dst",
            )?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect()
    }

    /// Ids `id` links to (existing or not), in id order.
    pub fn links_from(&self, id: &str) -> rusqlite::Result<Vec<String>> {
        self.conn
            .prepare("SELECT dst FROM links WHERE src = ?1 ORDER BY dst")?
            .query_map(params![id], |r| r.get(0))?
            .collect()
    }

    /// Pages that link to `id`, by title.
    pub fn links_to(&self, id: &str) -> rusqlite::Result<Vec<PageMeta>> {
        let cols = COLUMNS
            .split(',')
            .map(|c| format!("p.{c}"))
            .collect::<Vec<_>>()
            .join(",");
        self.query_metas(
            &format!(
                "SELECT {cols} FROM links l JOIN pages p ON p.id = l.src
                 WHERE l.dst = ?1 ORDER BY p.title COLLATE NOCASE"
            ),
            params![id],
        )
    }

    /// Count of every page due for review at or before `now_ms`, with no cap.
    pub fn due_count(&self, now_ms: i64) -> rusqlite::Result<usize> {
        self.conn
            .query_row(
                "SELECT COUNT(*) FROM pages WHERE next_review IS NOT NULL AND next_review <= ?1",
                params![now_ms],
                |r| r.get(0),
            )
            .map(|n: i64| n as usize)
    }

    /// Pages due for review at or before `now_ms`, earliest first.
    pub fn due(&self, now_ms: i64) -> rusqlite::Result<Vec<PageMeta>> {
        self.query_metas(
            &format!(
                "SELECT {COLUMNS} FROM pages WHERE next_review IS NOT NULL AND next_review <= ?1
                 ORDER BY next_review ASC"
            ),
            params![now_ms],
        )
    }

    pub fn tag_counts(&self) -> rusqlite::Result<Vec<TagCount>> {
        let mut stmt = self.conn.prepare("SELECT tags FROM pages")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        let mut counts = BTreeMap::new();
        for row in rows {
            let tags = serde_json::from_str::<Vec<String>>(&row?).unwrap_or_default();
            for tag in tags {
                *counts.entry(tag).or_default() += 1;
            }
        }
        Ok(counts
            .into_iter()
            .map(|(tag, count)| TagCount { tag, count })
            .collect())
    }

    pub fn folders(&self) -> rusqlite::Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT DISTINCT folder FROM pages WHERE folder IS NOT NULL AND folder != '' ORDER BY folder COLLATE NOCASE")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        rows.collect()
    }

    /// Remove every indexed page whose id is not in `keep`.
    pub fn remove_all_except(&self, keep: &HashSet<String>) -> rusqlite::Result<usize> {
        let all: Vec<String> = self
            .conn
            .prepare("SELECT id FROM pages")?
            .query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        let mut removed = 0;
        let tx = self.conn.unchecked_transaction()?;
        for id in all {
            if !keep.contains(&id) {
                delete_row(&tx, &id)?;
                removed += 1;
            }
        }
        tx.commit()?;
        Ok(removed)
    }

    /// Number of pages in the index.
    pub fn count(&self) -> rusqlite::Result<usize> {
        self.conn
            .query_row("SELECT COUNT(*) FROM pages", [], |r| r.get(0))
            .map(|n: i64| n as usize)
    }

    /// Last successful rescan, for the sidebar sync note.
    pub fn set_synced_at(&self) {
        let _ = self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS sync (n INTEGER PRIMARY KEY, at INTEGER NOT NULL);",
        );
        let _ = self.conn.execute(
            "INSERT INTO sync(n, at) VALUES (1, ?1) ON CONFLICT(n) DO UPDATE SET at=?1",
            params![now_secs()],
        );
    }

    pub fn synced_at(&self) -> Option<i64> {
        self.conn
            .query_row("SELECT at FROM sync WHERE n=1", [], |r| r.get(0))
            .optional()
            .ok()
            .flatten()
    }
}

/// Remove one page from both tables. The FTS row goes first: it is found
/// through the `pages` rowid.
fn delete_row(conn: &Connection, id: &str) -> rusqlite::Result<()> {
    conn.execute(
        "DELETE FROM pages_fts WHERE rowid = (SELECT rowid FROM pages WHERE id = ?1)",
        params![id],
    )?;
    conn.execute("DELETE FROM pages WHERE id = ?1", params![id])?;
    Ok(())
}

/// `PRAGMA user_version` once FTS rows are keyed by the `pages` rowid.
/// `PRAGMA user_version` once FTS has the `note` column and rowid mapping.
const FTS_ROWID_VERSION: i64 = 2;

/// Bring an index created by an older build up to the current columns.
/// The index is rebuildable, so adding columns with defaults is enough.
fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    let has_ext = conn
        .prepare("SELECT 1 FROM pragma_table_info('pages') WHERE name = 'ext'")?
        .exists([])?;
    if !has_ext {
        conn.execute_batch("ALTER TABLE pages ADD COLUMN ext TEXT NOT NULL DEFAULT '{}'")?;
    }
    // Schema 1 counted whole days.
    let has_days = conn
        .prepare("SELECT 1 FROM pragma_table_info('pages') WHERE name = 'interval_days'")?
        .exists([])?;
    if has_days {
        conn.execute_batch(
            "ALTER TABLE pages RENAME COLUMN interval_days TO interval_minutes;
             UPDATE pages SET interval_minutes = interval_minutes * 1440 WHERE interval_minutes IS NOT NULL;",
        )?;
    }
    let has_source_title = conn
        .prepare("SELECT 1 FROM pragma_table_info('pages') WHERE name = 'source_title'")?
        .exists([])?;
    if !has_source_title {
        conn.execute_batch("ALTER TABLE pages ADD COLUMN source_title TEXT;")?;
    }
    let has_fingerprint = conn
        .prepare("SELECT 1 FROM pragma_table_info('pages') WHERE name = 'fingerprint'")?
        .exists([])?;
    if !has_fingerprint {
        conn.execute_batch("ALTER TABLE pages ADD COLUMN fingerprint TEXT NOT NULL DEFAULT '';")?;
    }
    let has_note = conn
        .prepare("SELECT 1 FROM pragma_table_info('pages') WHERE name = 'note'")?
        .exists([])?;
    if !has_note {
        conn.execute_batch("ALTER TABLE pages ADD COLUMN note TEXT NOT NULL DEFAULT '';")?;
    }
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version < FTS_ROWID_VERSION {
        conn.execute_batch(&format!(
            "BEGIN;
             DROP TABLE IF EXISTS pages_fts;
             CREATE VIRTUAL TABLE pages_fts USING fts5(
               id UNINDEXED, title, tags, folder, note, text, tokenize='porter'
             );
             INSERT INTO pages_fts (rowid, id, title, tags, folder, note, text)
               SELECT rowid, id, title, tags, COALESCE(folder, ''), note, text_content FROM pages;
             PRAGMA user_version = {FTS_ROWID_VERSION};
             COMMIT;"
        ))?;
    }
    Ok(())
}

/// Turn a user query into an FTS5 expression: every token quoted and prefix-starred.
fn fts_expr(query: &str) -> String {
    query
        .split_whitespace()
        .map(|tok| {
            let esc = tok.replace('"', "\"\"");
            format!("\"{}\"*", esc)
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn like_pattern(query: &str) -> String {
    let esc: String = query
        .chars()
        .map(|c| match c {
            '%' | '_' | '\\' => format!("\\{c}"),
            _ => c.to_string(),
        })
        .collect();
    format!("%{esc}%")
}

/// The note as indexed for search: the page's source tool and prompt, and its
/// highlights and their notes, are searchable alongside it.
fn fts_note(meta: &PageMeta) -> String {
    let mut out = meta.note.clone();
    if let Some(source) = crate::models::PageSource::of(meta) {
        for part in [source.tool, source.prompt].into_iter().flatten() {
            out.push('\n');
            out.push_str(&part);
        }
    }
    // Highlighted passages and their notes are searchable too.
    for h in crate::builtin::highlights_of(meta) {
        out.push('\n');
        out.push_str(&h.quote);
        if !h.note.is_empty() {
            out.push('\n');
            out.push_str(&h.note);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memory_store() -> Store {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA).unwrap();
        Store {
            vault: PathBuf::new(),
            conn,
        }
    }

    #[test]
    fn tag_counts_track_occurrences_updates_and_deletions() {
        let store = memory_store();
        assert!(store.tag_counts().unwrap().is_empty());

        let mut first = PageMeta::new("first".into());
        first.tags = vec!["rust".into(), "web".into(), "rust".into(), "été".into()];
        store.upsert(&first, "", 0).unwrap();
        let mut second = PageMeta::new("second".into());
        second.tags = vec!["web".into(), "rust".into()];
        store.upsert(&second, "", 0).unwrap();
        store
            .upsert(&PageMeta::new("untagged".into()), "", 0)
            .unwrap();
        store
            .upsert(&PageMeta::new("corrupt".into()), "", 0)
            .unwrap();
        store
            .conn
            .execute(
                "UPDATE pages SET tags = 'invalid json' WHERE id = 'corrupt'",
                [],
            )
            .unwrap();

        let counts: Vec<_> = store
            .tag_counts()
            .unwrap()
            .into_iter()
            .map(|entry| (entry.tag, entry.count))
            .collect();
        assert_eq!(
            counts,
            vec![("rust".into(), 3), ("web".into(), 2), ("été".into(), 1)]
        );

        first.tags = vec!["web".into()];
        store.upsert(&first, "", 0).unwrap();
        store.delete("second").unwrap();
        let counts: Vec<_> = store
            .tag_counts()
            .unwrap()
            .into_iter()
            .map(|entry| (entry.tag, entry.count))
            .collect();
        assert_eq!(counts, vec![("web".into(), 1)]);
    }

    #[test]
    fn migrate_turns_a_day_count_column_into_minutes() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(&format!(
            "CREATE TABLE pages (id TEXT PRIMARY KEY, title TEXT NOT NULL DEFAULT '', tags TEXT NOT NULL DEFAULT '[]',
               folder TEXT, text_content TEXT NOT NULL DEFAULT '', interval_days INTEGER);
             INSERT INTO pages (id, interval_days) VALUES ('a', 3), ('b', NULL);
             {SCHEMA}"
        ))
        .unwrap();
        migrate(&conn).unwrap();
        let got: Vec<(String, Option<i64>)> = conn
            .prepare("SELECT id, interval_minutes FROM pages ORDER BY id")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(got, vec![("a".into(), Some(4320)), ("b".into(), None)]);
    }

    #[test]
    fn migrate_rekeys_legacy_fts_rows_so_deleted_pages_leave_search() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA).unwrap();
        let store = Store {
            vault: PathBuf::new(),
            conn,
        };
        for id in ["gone", "kept"] {
            let mut meta = PageMeta::new(id.into());
            meta.title = format!("{id} semver");
            store.upsert(&meta, "", 0).unwrap();
        }
        // Older builds inserted FTS rows with their own rowids, keyed by `id` only, and lacked `note`.
        store
            .conn
            .execute_batch(
                "DROP TABLE pages_fts;
             CREATE VIRTUAL TABLE pages_fts USING fts5(id UNINDEXED, title, tags, folder, text);
             INSERT INTO pages_fts (rowid, id, title, tags, folder, text) VALUES
               (90, 'kept', 'kept semver', '[]', '', ''), (91, 'gone', 'gone semver', '[]', '', '');
             PRAGMA user_version = 0;",
            )
            .unwrap();

        migrate(&store.conn).unwrap();
        store.delete("gone").unwrap();
        store
            .upsert(&store.get_meta("kept").unwrap().unwrap(), "", 0)
            .unwrap();

        let ids: Vec<_> = store
            .search("semver", 50)
            .unwrap()
            .into_iter()
            .map(|m| m.meta.id)
            .collect();
        assert_eq!(ids, vec!["kept"]);
        let fts_rows: i64 = store
            .conn
            .query_row("SELECT COUNT(*) FROM pages_fts", [], |r| r.get(0))
            .unwrap();
        assert_eq!(fts_rows, 1);
    }

    #[test]
    fn search_fallback_treats_like_wildcards_as_literal_text() {
        let store = memory_store();
        for (id, title) in [
            ("literal", "100%_ready\\today"),
            ("decoy", "100XXready\\today"),
        ] {
            let mut meta = PageMeta::new(id.into());
            meta.title = title.into();
            store.upsert(&meta, "", 0).unwrap();
        }
        store.conn.execute_batch("DROP TABLE pages_fts").unwrap();

        let ids: Vec<_> = store
            .search("%_ready\\", 50)
            .unwrap()
            .into_iter()
            .map(|hit| hit.meta.id)
            .collect();
        assert_eq!(ids, vec!["literal"]);
    }

    #[test]
    fn source_title_roundtrips_and_allow_cdn_defaults_to_false() {
        let store = memory_store();
        let mut meta = PageMeta::new("p1".into());
        meta.title = "Renamed Title".into();
        meta.source_title = Some("Original HTML Title".into());
        store.upsert(&meta, "content", 100).unwrap();

        let loaded = store.get_meta("p1").unwrap().unwrap();
        assert_eq!(loaded.source_title, Some("Original HTML Title".into()));
        assert_eq!(loaded.title, "Renamed Title");
        assert!(!loaded.allow_cdn, "allow_cdn defaults to false");
    }

    #[test]
    fn due_count_counts_all_due_pages_without_cap() {
        let store = memory_store();
        for i in 0..10 {
            let mut meta = PageMeta::new(format!("page-{i}"));
            meta.next_review = Some(1000 + i);
            store.upsert(&meta, "", 0).unwrap();
        }
        assert_eq!(store.due_count(1005).unwrap(), 6);
        assert_eq!(store.due_count(2000).unwrap(), 10);
    }

    #[test]
    fn search_ranks_notes_and_returns_snippets_when_in_text() {
        let store = memory_store();
        let mut p1 = PageMeta::new("text-hit".into());
        p1.title = "Alpha Document".into();
        store
            .upsert(&p1, "Here is rustacean programming in full text", 0)
            .unwrap();

        let mut p2 = PageMeta::new("title-hit".into());
        p2.title = "Rustacean Guide".into();
        store.upsert(&p2, "other body", 0).unwrap();

        let mut p3 = PageMeta::new("note-hit".into());
        p3.title = "Beta Document".into();
        p3.note = "A special note about rustacean".into();
        store.upsert(&p3, "plain text", 0).unwrap();

        let hits = store.search("rustacean", usize::MAX).unwrap();
        assert_eq!(hits.len(), 3);
        // title match ranked highest due to bm25 weight 10
        assert_eq!(hits[0].meta.id, "title-hit");
        assert_eq!(hits[0].snippet, None, "no snippet when match not in text");

        let text_hit = hits.iter().find(|h| h.meta.id == "text-hit").unwrap();
        assert!(text_hit.snippet.is_some(), "snippet present for text match");
        let snip = text_hit.snippet.as_ref().unwrap();
        assert!(snip.contains('[') && snip.contains(']'));
    }
}
