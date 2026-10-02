// SQLite index for pages. Rebuildable at any time from the vault files
// (see `crate::vault::index_vault`). Not the source of truth, only an index.

use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;

use rusqlite::{params, Connection, OptionalExtension, Params, Row};

use crate::models::{PageMeta, TagCount};
use crate::time::now_secs;

const COLUMNS: &str = "id,title,tags,folder,note,created_at,updated_at,interval_days,next_review,last_review,allow_cdn";

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS pages (
  id            TEXT PRIMARY KEY,
  title         TEXT NOT NULL DEFAULT '',
  tags          TEXT NOT NULL DEFAULT '[]',
  folder        TEXT,
  note          TEXT NOT NULL DEFAULT '',
  created_at    INTEGER NOT NULL,
  updated_at    INTEGER NOT NULL,
  interval_days INTEGER,
  next_review   INTEGER,
  last_review   INTEGER,
  allow_cdn     INTEGER NOT NULL DEFAULT 1,
  text_content  TEXT NOT NULL DEFAULT '',
  mtime         INTEGER NOT NULL DEFAULT 0
);
CREATE VIRTUAL TABLE IF NOT EXISTS pages_fts USING fts5(
  id UNINDEXED, title, tags, folder, text, tokenize='porter'
);
";

pub struct Store {
    pub vault: PathBuf,
    conn: Connection,
}

fn row_to_meta(row: &Row) -> rusqlite::Result<PageMeta> {
    Ok(PageMeta {
        id: row.get(0)?,
        title: row.get(1)?,
        tags: serde_json::from_str::<Vec<String>>(&row.get::<_, String>(2)?).unwrap_or_default(),
        folder: row.get(3)?,
        note: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
        interval_days: row.get(7)?,
        next_review: row.get(8)?,
        last_review: row.get(9)?,
        allow_cdn: row.get::<_, i64>(10)? != 0,
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
        Ok(Store { vault, conn })
    }

    /// Insert or refresh one page (meta, text content and file mtime).
    pub fn upsert(&self, meta: &PageMeta, text: &str, mtime: i64) -> rusqlite::Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        let tags = serde_json::to_string(&meta.tags).unwrap_or_else(|_| "[]".into());
        tx.execute(
            &format!(
                "INSERT INTO pages ({COLUMNS}, text_content, mtime)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)
                 ON CONFLICT(id) DO UPDATE SET
                   title=excluded.title, tags=excluded.tags, folder=excluded.folder,
                   note=excluded.note, updated_at=excluded.updated_at,
                   interval_days=excluded.interval_days, next_review=excluded.next_review,
                   last_review=excluded.last_review, allow_cdn=excluded.allow_cdn,
                   text_content=excluded.text_content, mtime=excluded.mtime"
            ),
            params![
                meta.id,
                meta.title,
                tags,
                meta.folder,
                meta.note,
                meta.created_at,
                meta.updated_at,
                meta.interval_days,
                meta.next_review,
                meta.last_review,
                if meta.allow_cdn { 1 } else { 0 },
                text,
                mtime,
            ],
        )?;
        tx.execute(
            "DELETE FROM pages_fts WHERE id = ?1",
            params![meta.id],
        )?;
        tx.execute(
            "INSERT INTO pages_fts (id, title, tags, folder, text) VALUES (?1,?2,?3,?4,?5)",
            params![
                meta.id,
                meta.title,
                tags,
                meta.folder.as_deref().unwrap_or_default(),
                text,
            ],
        )?;
        tx.commit()
    }

    pub fn delete(&self, id: &str) -> rusqlite::Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM pages WHERE id = ?1", params![id])?;
        tx.execute("DELETE FROM pages_fts WHERE id = ?1", params![id])?;
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
            .query_row(
                "SELECT mtime FROM pages WHERE id = ?1",
                params![id],
                |r| r.get(0),
            )
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

    pub fn search(&self, query: &str) -> rusqlite::Result<Vec<PageMeta>> {
        let expr = fts_expr(query);
        if expr.is_empty() {
            return self.all();
        }
        let sql = format!(
            "SELECT p.{COLUMNS} FROM pages_fts f JOIN pages p ON p.id = f.id
             WHERE pages_fts MATCH ?1 ORDER BY bm25(pages_fts), p.title COLLATE NOCASE LIMIT 400"
        );
        self.query_metas(&sql, params![expr])
            .or_else(|_| self.like_search(query))
    }

    fn like_search(&self, query: &str) -> rusqlite::Result<Vec<PageMeta>> {
        let like = like_pattern(query);
        let sql = format!(
            "SELECT {COLUMNS} FROM pages
             WHERE title LIKE ?1 ESCAPE '\\' OR tags LIKE ?1 ESCAPE '\\'
                OR folder LIKE ?1 ESCAPE '\\' OR text_content LIKE ?1 ESCAPE '\\'
             ORDER BY title COLLATE NOCASE LIMIT 400"
        );
        self.query_metas(&sql, params![like])
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
        Ok(counts.into_iter().map(|(tag, count)| TagCount { tag, count }).collect())
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
                tx.execute("DELETE FROM pages WHERE id = ?1", params![id])?;
                tx.execute("DELETE FROM pages_fts WHERE id = ?1", params![id])?;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn memory_store() -> Store {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA).unwrap();
        Store { vault: PathBuf::new(), conn }
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
        store.upsert(&PageMeta::new("untagged".into()), "", 0).unwrap();
        store.upsert(&PageMeta::new("corrupt".into()), "", 0).unwrap();
        store.conn.execute("UPDATE pages SET tags = 'invalid json' WHERE id = 'corrupt'", []).unwrap();

        let counts: Vec<_> = store.tag_counts().unwrap().into_iter()
            .map(|entry| (entry.tag, entry.count)).collect();
        assert_eq!(counts, vec![("rust".into(), 3), ("web".into(), 2), ("été".into(), 1)]);

        first.tags = vec!["web".into()];
        store.upsert(&first, "", 0).unwrap();
        store.delete("second").unwrap();
        let counts: Vec<_> = store.tag_counts().unwrap().into_iter()
            .map(|entry| (entry.tag, entry.count)).collect();
        assert_eq!(counts, vec![("web".into(), 1)]);
    }

    #[test]
    fn search_fallback_treats_like_wildcards_as_literal_text() {
        let store = memory_store();
        for (id, title) in [("literal", "100%_ready\\today"), ("decoy", "100XXready\\today")] {
            let mut meta = PageMeta::new(id.into());
            meta.title = title.into();
            store.upsert(&meta, "", 0).unwrap();
        }
        store.conn.execute_batch("DROP TABLE pages_fts").unwrap();

        let ids: Vec<_> = store.search("%_ready\\").unwrap().into_iter()
            .map(|meta| meta.id).collect();
        assert_eq!(ids, vec!["literal"]);
    }
}