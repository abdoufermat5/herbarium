// SQLite index for pages. Rebuildable at any time from the vault files
// (see `crate::vault::index_vault`). Not the source of truth, only an index.

use std::collections::HashSet;
use std::path::PathBuf;

use rusqlite::{params, Connection, OptionalExtension, Row};

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
                serde_json::to_string(&meta.tags).unwrap_or_else(|_| "[]".into()),
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
                serde_json::to_string(&meta.tags).unwrap_or_else(|_| "[]".into()),
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

    pub fn all(&self) -> rusqlite::Result<Vec<PageMeta>> {
        let mut stmt = self
            .conn
            .prepare(&format!("SELECT {COLUMNS} FROM pages ORDER BY title COLLATE NOCASE"))?;
        let rows = stmt.query_map([], row_to_meta)?;
        rows.collect()
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
        let mut stmt = match self.conn.prepare(&sql) {
            Ok(s) => s,
            Err(_) => return self.like_search(query),
        };
        match stmt.query_map(params![expr], row_to_meta) {
            Ok(rows) => match rows.collect::<rusqlite::Result<Vec<_>>>() {
                Ok(v) => Ok(v),
                Err(_) => self.like_search(query),
            },
            Err(_) => self.like_search(query),
        }
    }

    fn like_search(&self, query: &str) -> rusqlite::Result<Vec<PageMeta>> {
        let like = like_pattern(query);
        let sql = format!(
            "SELECT {COLUMNS} FROM pages
             WHERE title LIKE ?1 ESCAPE '\\' OR tags LIKE ?1 ESCAPE '\\'
                OR folder LIKE ?1 ESCAPE '\\' OR text_content LIKE ?1 ESCAPE '\\'
             ORDER BY title COLLATE NOCASE LIMIT 400"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![like], row_to_meta)?;
        rows.collect()
    }

    /// Pages due for review at or before `now_ms`, earliest first.
    pub fn due(&self, now_ms: i64) -> rusqlite::Result<Vec<PageMeta>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {COLUMNS} FROM pages WHERE next_review IS NOT NULL AND next_review <= ?1
             ORDER BY next_review ASC"
        ))?;
        let rows = stmt.query_map(params![now_ms], row_to_meta)?;
        rows.collect()
    }

    pub fn tag_counts(&self) -> rusqlite::Result<Vec<TagCount>> {
        let metas = self.all()?;
        let mut map: std::collections::BTreeMap<String, usize> = Default::default();
        for m in metas {
            for t in m.tags {
                *map.entry(t).or_default() += 1;
            }
        }
        Ok(map.into_iter().map(|(tag, count)| TagCount { tag, count }).collect())
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