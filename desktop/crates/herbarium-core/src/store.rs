// SQLite index for pages. Rebuildable at any time from the vault files
// (see `crate::vault::index_vault`). Not the source of truth, only an index.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, Params, Row, params};

use crate::models::{PageMeta, PageSource, SCHEMA_VERSION, SearchHit, TagCount};
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
  source_url    TEXT,
  search_note   TEXT NOT NULL DEFAULT '',
  fingerprint   TEXT NOT NULL DEFAULT ''
);
CREATE VIRTUAL TABLE IF NOT EXISTS pages_fts USING fts5(
  id UNINDEXED, title, tags, folder, search_note, text_content,
  content='pages', tokenize='porter'
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
CREATE TABLE IF NOT EXISTS sync (
  n  INTEGER PRIMARY KEY,
  at INTEGER NOT NULL
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
        conn.pragma_update(None, "busy_timeout", 5000).ok();
        let wal: Result<String, _> = conn.query_row("PRAGMA journal_mode = WAL", [], |r| r.get(0));
        if !matches!(wal.as_deref(), Ok("wal")) {
            eprintln!("herbarium: journal_mode is not WAL ({wal:?}); writer contention may be higher");
        }
        conn.pragma_update(None, "synchronous", "NORMAL").ok();
        migrate(&conn)?;
        Ok(Store { vault, conn })
    }

    /// A read-only connection to the same index, for hot read paths (protocol
    /// serving, native-host lookups) that must never contend with the app's
    /// write connection. Falls back to a read/write handle (still used only for
    /// reads) when a read-only WAL handle cannot be opened on this platform.
    pub fn open_read_only(vault: PathBuf) -> rusqlite::Result<Store> {
        let db = vault.join(".herbarium").join("index.sqlite");
        let flags = rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
            | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let conn = Connection::open_with_flags(&db, flags).or_else(|_| Connection::open(&db))?;
        conn.pragma_update(None, "busy_timeout", 5000).ok();
        Ok(Store { vault, conn })
    }

    /// Insert or refresh one page (meta, text content and file mtime).
    pub fn upsert(&self, meta: &PageMeta, text: &str, mtime: i64) -> rusqlite::Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        let tags = serde_json::to_string(&meta.tags).unwrap_or_else(|_| "[]".into());
        let ext = serde_json::to_string(&meta.ext).unwrap_or_else(|_| "{}".into());
        let source_url = PageSource::of(meta)
            .and_then(|s| s.url)
            .map(|u| normalize_url(&u));
        let search_note = fts_note(meta);
        let fingerprint =
            crate::vault::page_fingerprint(&self.vault, &meta.id, meta.folder.as_deref());
        tx.execute(
            &format!(
                "INSERT INTO pages ({COLUMNS}, text_content, mtime, source_url, search_note, fingerprint)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18)
                 ON CONFLICT(id) DO UPDATE SET
                   title=excluded.title, tags=excluded.tags, folder=excluded.folder,
                   note=excluded.note, updated_at=excluded.updated_at,
                   interval_minutes=excluded.interval_minutes, next_review=excluded.next_review,
                   last_review=excluded.last_review, allow_cdn=excluded.allow_cdn,
                   ext=excluded.ext, source_title=excluded.source_title,
                   source_url=excluded.source_url, search_note=excluded.search_note,
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
                source_url,
                search_note,
                fingerprint,
            ],
        )?;
        // The `pages_ai`/`pages_au` triggers keep `pages_fts` (an external-content
        // table) in step with `pages`, so no separate FTS insert is needed here.
        tx.commit()
    }

    /// Update a page's metadata columns (and its searchable note) without
    /// touching its indexed text. Used by metadata-only saves (`Ctx::save`):
    /// tags, notes, review schedule, source, highlights. The FTS triggers keep
    /// the index in step. `mtime`/`fingerprint` still change because the
    /// sidecar was rewritten on disk.
    pub fn update_meta(&self, meta: &PageMeta) -> rusqlite::Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        update_meta_on(&tx, &self.vault, meta)?;
        tx.commit()
    }

    /// Run `f` inside a single transaction, committing on success. Exposed so a
    /// caller (e.g. the rescan indexer) can batch many writes without paying
    /// one transaction per row. Returns `f`'s result.
    pub fn batch<F, T>(&self, f: F) -> rusqlite::Result<T>
    where
        F: FnOnce(&rusqlite::Transaction) -> rusqlite::Result<T>,
    {
        let tx = self.conn.unchecked_transaction()?;
        let out = f(&tx)?;
        tx.commit()?;
        Ok(out)
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

    /// Pages whose metadata still carries a legacy `ext.storage` blob.
    pub fn with_legacy_storage(&self) -> rusqlite::Result<Vec<PageMeta>> {
        self.query_metas(
            &format!("SELECT {COLUMNS} FROM pages WHERE json_type(ext, '$.storage') IS NOT NULL"),
            [],
        )
    }

    /// List page metadata, optionally limited to a folder (including its
    /// subfolders) or an exact tag, in the same `title COLLATE NOCASE` order
    /// as `all`. Folder/tag/limit are pushed into SQL rather than filtering in
    /// memory.
    pub fn list_filtered(
        &self,
        folder: Option<&str>,
        tag: Option<&str>,
        limit: usize,
    ) -> rusqlite::Result<Vec<PageMeta>> {
        let limit_param = if limit == usize::MAX {
            i64::MAX
        } else {
            limit as i64
        };
        let mut where_sql = String::new();
        let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        let mut n = 1;
        if let Some(f) = folder {
            where_sql.push_str(&format!(
                "WHERE (folder = ?{n} OR folder LIKE ?{} ESCAPE '\\')",
                n + 1
            ));
            args.push(Box::new(f.to_string()));
            args.push(Box::new(format!("{}/%", like_escape(f))));
            n += 2;
        }
        if let Some(t) = tag {
            where_sql.push_str(if where_sql.is_empty() { "WHERE " } else { " AND " });
            where_sql.push_str(&format!(
                "EXISTS (SELECT 1 FROM json_each(tags) WHERE json_each.value = ?{n})"
            ));
            args.push(Box::new(t.to_string()));
            n += 1;
        }
        let sql = format!(
            "SELECT {COLUMNS} FROM pages {where_sql} ORDER BY title COLLATE NOCASE LIMIT ?{n}"
        );
        args.push(Box::new(limit_param));
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(args.iter().map(|a| a.as_ref())), row_to_meta)?;
        rows.collect()
    }

    /// Pages whose normalized source URL equals `url` (lowercased scheme/host,
    /// no fragment, no trailing slash), in title order.
    pub fn find_by_url(&self, url: &str) -> rusqlite::Result<Vec<PageMeta>> {
        self.query_metas(
            &format!(
                "SELECT {COLUMNS} FROM pages WHERE source_url = ?1
                 ORDER BY title COLLATE NOCASE"
            ),
            params![url],
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
        match self.fts_search(&expr, None, None, limit_param) {
            Ok(hits) => Ok(hits),
            // The query couldn't be parsed as an FTS expression: retry it as a
            // sanitized phrase so it still matches literally instead of erroring.
            Err(_) => self.fts_search(&fts_phrase(query), None, None, limit_param),
        }
    }

    /// Like `search`, but with a folder (incl. subfolders) and/or tag narrowed
    /// in SQL. Used for filtered searches so the folder/tag predicates never
    /// load and discard every page in memory.
    pub fn search_filtered(
        &self,
        text: &str,
        folder: Option<&str>,
        tag: Option<&str>,
        limit: usize,
    ) -> rusqlite::Result<Vec<SearchHit>> {
        let limit_param = if limit == usize::MAX {
            i64::MAX
        } else {
            limit as i64
        };
        let expr = fts_expr(text);
        if expr.is_empty() {
            let metas = self.list_filtered(folder, tag, limit)?;
            return Ok(metas
                .into_iter()
                .map(|meta| SearchHit { meta, snippet: None })
                .collect());
        }
        match self.fts_search(&expr, folder, tag, limit_param) {
            Ok(hits) => Ok(hits),
            Err(_) => self.fts_search(&fts_phrase(text), folder, tag, limit_param),
        }
    }

    /// One FTS query, optionally narrowed to a folder (incl. subfolders) and/or
    /// a case-insensitive tag. Returns hits in bm25 rank order, then title.
    fn fts_search(
        &self,
        expr: &str,
        folder: Option<&str>,
        tag: Option<&str>,
        limit_param: i64,
    ) -> rusqlite::Result<Vec<SearchHit>> {
        let page_cols = COLUMNS
            .split(',')
            .map(|col| format!("p.{col}"))
            .collect::<Vec<_>>()
            .join(", ");
        let mut where_sql = String::from("pages_fts MATCH ?");
        let mut args: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(expr.to_string())];
        let mut n = 2;
        if let Some(f) = folder {
            where_sql.push_str(&format!(
                " AND (p.folder = ?{n} OR p.folder LIKE ?{} ESCAPE '\\')",
                n + 1
            ));
            args.push(Box::new(f.to_string()));
            args.push(Box::new(format!("{}/%", like_escape(f))));
            n += 2;
        }
        if let Some(t) = tag {
            where_sql.push_str(&format!(
                " AND EXISTS (SELECT 1 FROM json_each(p.tags) WHERE json_each.value = ?{n} COLLATE NOCASE)"
            ));
            args.push(Box::new(t.to_string()));
            n += 1;
        }
        let sql = format!(
            "SELECT {page_cols}, snippet(pages_fts, 5, '[', ']', '…', 12)
             FROM pages_fts f JOIN pages p ON p.id = f.id
             WHERE {where_sql}
             ORDER BY bm25(pages_fts, 0, 10, 4, 2, 3, 1), p.title COLLATE NOCASE
             LIMIT ?{n}"
        );
        args.push(Box::new(limit_param));
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(args.iter().map(|a| a.as_ref())), |r| {
            let meta = row_to_meta(r)?;
            let snip: Option<String> = r.get(13)?;
            let snippet = snip.filter(|s| s.contains('[') && s.contains(']'));
            Ok(SearchHit { meta, snippet })
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

    /// Every indexed page id.
    pub fn ids(&self) -> rusqlite::Result<HashSet<String>> {
        self.conn
            .prepare("SELECT id FROM pages")?
            .query_map([], |r| r.get(0))?
            .collect()
    }

    /// Remove the pages in `indexed_before` whose id is not in `keep`. Pages
    /// indexed after `indexed_before` was taken (by another process, while a
    /// rescan was walking the vault) are never touched.
    pub fn remove_missing(
        &self,
        indexed_before: &HashSet<String>,
        keep: &HashSet<String>,
    ) -> rusqlite::Result<usize> {
        let mut removed = 0;
        let tx = self.conn.unchecked_transaction()?;
        for id in indexed_before {
            if !keep.contains(id) {
                delete_row(&tx, id)?;
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

/// Remove one page from the index. The `pages_ad` trigger removes its FTS row
/// (keyed by the shared `pages` rowid) automatically.
fn delete_row(conn: &Connection, id: &str) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM pages WHERE id = ?1", params![id])?;
    Ok(())
}

/// Write `meta`'s metadata columns (not `text_content`) to the index, on any
/// connection (or transaction). The `pages_au` trigger refreshes its FTS row.
/// `mtime`/`fingerprint` are recomputed because the sidecar changed on disk.
pub(crate) fn update_meta_on(conn: &Connection, vault: &Path, meta: &PageMeta) -> rusqlite::Result<()> {
    let tags = serde_json::to_string(&meta.tags).unwrap_or_else(|_| "[]".into());
    let ext = serde_json::to_string(&meta.ext).unwrap_or_else(|_| "{}".into());
    let source_url = PageSource::of(meta)
        .and_then(|s| s.url)
        .map(|u| normalize_url(&u));
    let search_note = fts_note(meta);
    let fingerprint = crate::vault::page_fingerprint(vault, &meta.id, meta.folder.as_deref());
    let mtime = crate::vault::page_mtime(vault, meta);
    conn.execute(
        "UPDATE pages SET
           title=?2, tags=?3, folder=?4, note=?5, updated_at=?6,
           interval_minutes=?7, next_review=?8, last_review=?9, allow_cdn=?10,
           ext=?11, source_title=?12, source_url=?13, search_note=?14,
           mtime=?15, fingerprint=?16
         WHERE id=?1",
        params![
            meta.id,
            meta.title,
            tags,
            meta.folder,
            meta.note,
            meta.updated_at,
            meta.interval_minutes,
            meta.next_review,
            meta.last_review,
            if meta.allow_cdn { 1 } else { 0 },
            ext,
            meta.source_title,
            source_url,
            search_note,
            mtime,
            fingerprint,
        ],
    )?;
    Ok(())
}

/// `PRAGMA user_version` once FTS rows are keyed by the `pages` rowid and the
/// FTS table is an external-content table maintained by triggers.
const FTS_ROWID_VERSION: i64 = 3;

/// Bring the schema up to date. All steps run in one `BEGIN IMMEDIATE`
/// transaction: a second process opening the same index waits (busy_timeout)
/// for the first to finish, then finds every step already applied instead of
/// racing it into "duplicate column". Append new idempotent steps to
/// `migrate_steps`.
fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    in_immediate_tx(conn, |conn| {
        conn.execute_batch(SCHEMA)?;
        migrate_steps(conn)
    })
}

fn in_immediate_tx(
    conn: &Connection,
    steps: impl FnOnce(&Connection) -> rusqlite::Result<()>,
) -> rusqlite::Result<()> {
    conn.execute_batch("BEGIN IMMEDIATE")?;
    match steps(conn) {
        Ok(()) => conn.execute_batch("COMMIT"),
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(e)
        }
    }
}

/// Bring an index created by an older build up to the current columns.
/// The index is rebuildable, so adding columns with defaults is enough.
fn migrate_steps(conn: &Connection) -> rusqlite::Result<()> {
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
    // `source_url` is the normalized page URL (lowercased scheme/host, no
    // fragment, no trailing slash), used for exact `find_by_url` lookups.
    let has_source_url = conn
        .prepare("SELECT 1 FROM pragma_table_info('pages') WHERE name = 'source_url'")?
        .exists([])?;
    if !has_source_url {
        conn.execute_batch("ALTER TABLE pages ADD COLUMN source_url TEXT;")?;
    }
    // `search_note` is the denormalized note indexed by FTS (note + source
    // tool/prompt + highlight quotes/notes). External-content FTS maps its
    // columns to `pages` columns by name, so the indexed note lives here.
    let has_search_note = conn
        .prepare("SELECT 1 FROM pragma_table_info('pages') WHERE name = 'search_note'")?
        .exists([])?;
    if !has_search_note {
        conn.execute_batch("ALTER TABLE pages ADD COLUMN search_note TEXT NOT NULL DEFAULT '';")?;
    }
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS pages_folder ON pages(folder);
         CREATE INDEX IF NOT EXISTS pages_next_review ON pages(next_review);
         CREATE INDEX IF NOT EXISTS pages_created_at ON pages(created_at);
         CREATE INDEX IF NOT EXISTS pages_source_url ON pages(source_url);",
    )?;
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version < FTS_ROWID_VERSION {
        // Backfill the two derived columns before the FTS rebuild reads them.
        // `index_vault` skips unchanged pages, so this must not wait for the
        // next rescan.
        let rows: Vec<(String, String, String)> = conn
            .prepare("SELECT id, note, ext FROM pages")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<rusqlite::Result<_>>()?;
        for (id, note, ext) in rows {
            let mut meta = PageMeta::new(id.clone());
            meta.note = note;
            meta.ext = serde_json::from_str(&ext).unwrap_or_default();
            let source_url = PageSource::of(&meta)
                .and_then(|s| s.url)
                .map(|u| normalize_url(&u));
            let search_note = fts_note(&meta);
            conn.execute(
                "UPDATE pages SET source_url = ?1, search_note = ?2 WHERE id = ?3",
                params![source_url, search_note, id],
            )?;
        }
        conn.execute_batch(&format!(
            "DROP TRIGGER IF EXISTS pages_ai;
             DROP TRIGGER IF EXISTS pages_ad;
             DROP TRIGGER IF EXISTS pages_au;
             DROP TABLE IF EXISTS pages_fts;
             CREATE VIRTUAL TABLE pages_fts USING fts5(
               id UNINDEXED, title, tags, folder, search_note, text_content,
               content='pages', tokenize='porter'
             );
             CREATE TRIGGER pages_ai AFTER INSERT ON pages BEGIN
               INSERT INTO pages_fts(rowid, id, title, tags, folder, search_note, text_content)
               VALUES (new.rowid, new.id, new.title, new.tags,
                       COALESCE(new.folder, ''), new.search_note, new.text_content);
             END;
             CREATE TRIGGER pages_ad AFTER DELETE ON pages BEGIN
               INSERT INTO pages_fts(pages_fts, rowid, id, title, tags, folder, search_note, text_content)
               VALUES ('delete', old.rowid, old.id, old.title, old.tags,
                       COALESCE(old.folder, ''), old.search_note, old.text_content);
             END;
             CREATE TRIGGER pages_au AFTER UPDATE ON pages
             WHEN old.title IS NOT new.title OR old.tags IS NOT new.tags
               OR old.folder IS NOT new.folder OR old.search_note IS NOT new.search_note
               OR old.text_content IS NOT new.text_content
             BEGIN
               INSERT INTO pages_fts(pages_fts, rowid, id, title, tags, folder, search_note, text_content)
               VALUES ('delete', old.rowid, old.id, old.title, old.tags,
                       COALESCE(old.folder, ''), old.search_note, old.text_content);
               INSERT INTO pages_fts(rowid, id, title, tags, folder, search_note, text_content)
               VALUES (new.rowid, new.id, new.title, new.tags,
                       COALESCE(new.folder, ''), new.search_note, new.text_content);
             END;
             INSERT INTO pages_fts (rowid, id, title, tags, folder, search_note, text_content)
               SELECT rowid, id, title, tags, COALESCE(folder, ''), search_note, text_content FROM pages;
             PRAGMA user_version = {FTS_ROWID_VERSION};"
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

/// A sanitized FTS5 phrase query: the raw query treated as one literal phrase
/// (quotes escaped), used when the prefix expression fails to parse.
fn fts_phrase(query: &str) -> String {
    format!("\"{}\"", query.replace('"', "\"\""))
}

/// Escape LIKE metacharacters so a value is matched literally with `ESCAPE '\\'`.
fn like_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(c, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// A URL reduced to what identifies a page: scheme and host lowercased, no
/// fragment, no trailing slash. Used for the `source_url` column and
/// `pages.find_by_url`.
pub fn normalize_url(url: &str) -> String {
    let url = url.trim();
    let url = url.split('#').next().unwrap_or(url);
    let (scheme_host, rest) = match url.find("://") {
        Some(i) => {
            let after = &url[i + 3..];
            let end = after.find(['/', '?']).map_or(url.len(), |j| i + 3 + j);
            (url[..end].to_ascii_lowercase(), &url[end..])
        }
        None => (url.to_string(), ""),
    };
    let rest = rest.strip_suffix('/').unwrap_or(rest);
    format!("{scheme_host}{rest}")
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

/// The few fields a resource listing needs for one page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListingRow {
    pub id: String,
    pub title: String,
    pub tags: Vec<String>,
    pub folder: Option<String>,
}

impl Store {
    /// One window of the library in title order (ties by id, so a window is
    /// stable) without loading the pages' metadata: `limit` rows from `offset`.
    pub fn listing_window(&self, offset: usize, limit: usize) -> rusqlite::Result<Vec<ListingRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id,title,tags,folder FROM pages
             ORDER BY title COLLATE NOCASE, id LIMIT ?1 OFFSET ?2",
        )?;
        stmt.query_map(params![limit as i64, offset as i64], |r| {
            Ok(ListingRow {
                id: r.get(0)?,
                title: r.get(1)?,
                tags: serde_json::from_str(&r.get::<_, String>(2)?).unwrap_or_default(),
                folder: r.get(3)?,
            })
        })?
        .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memory_store() -> Store {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA).unwrap();
        migrate(&conn).unwrap();
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
               folder TEXT, text_content TEXT NOT NULL DEFAULT '', interval_days INTEGER,
               created_at INTEGER, next_review INTEGER);
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
    fn search_matches_by_prefix() {
        let store = memory_store();
        let mut hit = PageMeta::new("rustacean".into());
        hit.title = "Rustacean Guide".into();
        store.upsert(&hit, "the rustacean body text", 0).unwrap();
        let mut other = PageMeta::new("other".into());
        other.title = "Unrelated".into();
        store.upsert(&other, "nothing here", 0).unwrap();

        // A prefix of a token matches, and the full token matches too.
        let ids: Vec<_> = store
            .search("rusta", 50)
            .unwrap()
            .into_iter()
            .map(|hit| hit.meta.id)
            .collect();
        assert_eq!(ids, vec!["rustacean"]);
        let ids: Vec<_> = store
            .search("rustacean", 50)
            .unwrap()
            .into_iter()
            .map(|hit| hit.meta.id)
            .collect();
        assert_eq!(ids, vec!["rustacean"]);
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

    #[test]
    fn find_by_url_matches_normalized_source_url() {
        let store = memory_store();
        let mut meta = PageMeta::new("p1".into());
        meta.title = "Example".into();
        PageSource::set(
            &mut meta,
            Some(PageSource {
                url: Some("https://Example.com/foo/bar#frag".into()),
                ..Default::default()
            }),
        );
        store.upsert(&meta, "", 0).unwrap();

        // The stored URL is normalized (lowercased host, no fragment) and
        // matched exactly on lookup, without scanning `ext` JSON. The caller
        // (pages.find_by_url) normalizes the query before handing it here.
        let found = store.find_by_url("https://example.com/foo/bar").unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, "p1");

        assert!(store.find_by_url("https://example.com/other").unwrap().is_empty());
        assert!(store.find_by_url("https://example.com/foo").unwrap().is_empty());
    }

    #[test]
    fn list_filtered_pushes_folder_tag_and_limit_into_sql() {
        let store = memory_store();
        for (id, folder, tags) in [
            ("alpha", None, vec!["rust"]),
            ("beta", Some("lang"), vec!["web"]),
            ("gamma", Some("lang"), Vec::<&str>::new()),
            ("delta", Some("lang/rust"), vec!["rust", "web"]),
            ("epsilon", Some("lang/rust"), vec!["web"]),
        ] {
            let mut m = PageMeta::new(id.into());
            m.title = id.to_string();
            m.folder = folder.map(str::to_string);
            m.tags = tags.into_iter().map(str::to_string).collect();
            store.upsert(&m, "", 0).unwrap();
        }

        // A folder includes its subfolders, in `title COLLATE NOCASE` order.
        let ids: Vec<String> = store
            .list_filtered(Some("lang"), None, usize::MAX)
            .unwrap()
            .into_iter()
            .map(|m| m.id)
            .collect();
        assert_eq!(ids, vec!["beta", "delta", "epsilon", "gamma"]);

        // A tag filters across folders (exact, case-sensitive).
        let ids: Vec<String> = store
            .list_filtered(None, Some("web"), usize::MAX)
            .unwrap()
            .into_iter()
            .map(|m| m.id)
            .collect();
        assert_eq!(ids, vec!["beta", "delta", "epsilon"]);

        // Folder + tag + limit.
        let ids: Vec<String> = store
            .list_filtered(Some("lang"), Some("web"), 2)
            .unwrap()
            .into_iter()
            .map(|m| m.id)
            .collect();
        assert_eq!(ids, vec!["beta", "delta"]);
    }

    #[test]
    fn batch_updates_meta_atomically() {
        let store = memory_store();
        for id in ["a", "b"] {
            let mut m = PageMeta::new(id.into());
            m.tags = vec!["old".into()];
            store.upsert(&m, "", 0).unwrap();
        }

        store
            .batch(|tx| {
                for id in ["a", "b"] {
                    let mut m = store.get_meta(id).unwrap().unwrap();
                    m.tags = vec!["new".into()];
                    m.updated_at = 1;
                    update_meta_on(tx, &store.vault, &m)?;
                }
                Ok::<_, rusqlite::Error>(())
            })
            .unwrap();
        for id in ["a", "b"] {
            assert_eq!(
                store.get_meta(id).unwrap().unwrap().tags,
                vec!["new".to_string()]
            );
        }

        // A failing batch rolls every write back.
        let err: rusqlite::Result<()> = store.batch(|tx| {
            let mut m = store.get_meta("a").unwrap().unwrap();
            m.tags = vec!["rolled".into()];
            update_meta_on(tx, &store.vault, &m)?;
            Err(rusqlite::Error::InvalidQuery)
        });
        assert!(err.is_err());
        assert_eq!(
            store.get_meta("a").unwrap().unwrap().tags,
            vec!["new".to_string()]
        );
    }
}
