use rusqlite::{params, Connection, Result as SqlResult};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;

/// Represents a single indexed file entry stored in SQLite.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub id: i64,
    pub filename: String,
    pub filepath: String,
    pub extension: String,
    pub file_size: i64,
    pub modified_at: i64,
    pub file_type: String, // "app", "document", "folder", "shortcut", "other"
    pub click_count: i64,
    pub last_accessed: i64,
    pub icon_path: Option<String>,
}

/// Thread-safe database wrapper.
pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    /// Open or create the SQLite database at the given path.
    pub fn open(db_path: &PathBuf) -> SqlResult<Self> {
        let conn = Connection::open(db_path)?;

        // Performance tunings for search-heavy workload
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA cache_size = -64000;
             PRAGMA temp_store = MEMORY;
             PRAGMA mmap_size = 268435456;",
        )?;

        let db = Database {
            conn: Mutex::new(conn),
        };
        db.create_tables()?;
        Ok(db)
    }

    /// Create tables and indexes if they don't already exist.
    fn create_tables(&self) -> SqlResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS files (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                filename TEXT NOT NULL,
                filepath TEXT NOT NULL UNIQUE,
                extension TEXT NOT NULL DEFAULT '',
                file_size INTEGER NOT NULL DEFAULT 0,
                modified_at INTEGER NOT NULL DEFAULT 0,
                file_type TEXT NOT NULL DEFAULT 'other',
                click_count INTEGER NOT NULL DEFAULT 0,
                last_accessed INTEGER NOT NULL DEFAULT 0,
                icon_path TEXT
            );

            CREATE INDEX IF NOT EXISTS idx_filename ON files(filename);
            CREATE INDEX IF NOT EXISTS idx_filepath ON files(filepath);
            CREATE INDEX IF NOT EXISTS idx_extension ON files(extension);
            CREATE INDEX IF NOT EXISTS idx_file_type ON files(file_type);
            CREATE INDEX IF NOT EXISTS idx_click_count ON files(click_count DESC);
            CREATE INDEX IF NOT EXISTS idx_modified_at ON files(modified_at DESC);

            CREATE TABLE IF NOT EXISTS index_meta (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );",
        )?;
        Ok(())
    }

    /// Insert or update a file entry (upsert based on filepath).
    pub fn upsert_file(
        &self,
        filename: &str,
        filepath: &str,
        extension: &str,
        file_size: i64,
        modified_at: i64,
        file_type: &str,
    ) -> SqlResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO files (filename, filepath, extension, file_size, modified_at, file_type)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(filepath) DO UPDATE SET
                filename = excluded.filename,
                file_size = excluded.file_size,
                modified_at = excluded.modified_at,
                file_type = excluded.file_type",
            params![filename, filepath, extension, file_size, modified_at, file_type],
        )?;
        Ok(())
    }

    /// Batch insert/upsert multiple file entries in a single transaction.
    pub fn upsert_files_batch(&self, entries: &[(String, String, String, i64, i64, String)]) -> SqlResult<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare_cached(
                "INSERT INTO files (filename, filepath, extension, file_size, modified_at, file_type)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(filepath) DO UPDATE SET
                    filename = excluded.filename,
                    file_size = excluded.file_size,
                    modified_at = excluded.modified_at,
                    file_type = excluded.file_type",
            )?;
            for (filename, filepath, extension, file_size, modified_at, file_type) in entries {
                stmt.execute(params![filename, filepath, extension, file_size, modified_at, file_type])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Search files using SQL LIKE for prefix/substring matching.
    /// Returns up to `limit` results sorted by relevance.
    pub fn search_files(&self, query: &str, limit: usize) -> SqlResult<Vec<FileEntry>> {
        let conn = self.conn.lock().unwrap();
        let like_pattern = format!("%{}%", query.replace('%', "\\%").replace('_', "\\_"));
        let prefix_pattern = format!("{}%", query.replace('%', "\\%").replace('_', "\\_"));

        // Union query: exact matches first, then prefix, then substring,
        // all boosted by click_count and recency.
        let sql = "
            SELECT id, filename, filepath, extension, file_size, modified_at,
                   file_type, click_count, last_accessed, icon_path,
                   CASE
                       WHEN LOWER(filename) = LOWER(?1) THEN 100
                       WHEN LOWER(filename) LIKE LOWER(?2) ESCAPE '\\' THEN 75
                       WHEN LOWER(filename) LIKE LOWER(?3) ESCAPE '\\' THEN 50
                       WHEN LOWER(filepath) LIKE LOWER(?3) ESCAPE '\\' THEN 25
                       ELSE 0
                   END AS match_score
            FROM files
            WHERE LOWER(filename) LIKE LOWER(?3) ESCAPE '\\'
               OR LOWER(filepath) LIKE LOWER(?3) ESCAPE '\\'
            ORDER BY
                match_score DESC,
                CASE file_type
                    WHEN 'app' THEN 5
                    WHEN 'shortcut' THEN 4
                    WHEN 'document' THEN 3
                    WHEN 'folder' THEN 2
                    ELSE 1
                END DESC,
                click_count DESC,
                last_accessed DESC,
                modified_at DESC
            LIMIT ?4
        ";

        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map(params![query, prefix_pattern, like_pattern, limit as i64], |row| {
            Ok(FileEntry {
                id: row.get(0)?,
                filename: row.get(1)?,
                filepath: row.get(2)?,
                extension: row.get(3)?,
                file_size: row.get(4)?,
                modified_at: row.get(5)?,
                file_type: row.get(6)?,
                click_count: row.get(7)?,
                last_accessed: row.get(8)?,
                icon_path: row.get(9)?,
            })
        })?;

        let mut results = Vec::new();
        for row in rows {
            if let Ok(entry) = row {
                results.push(entry);
            }
        }
        Ok(results)
    }

    /// Increment the click count and update last_accessed time for a file.
    pub fn record_click(&self, filepath: &str) -> SqlResult<()> {
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now().timestamp();
        conn.execute(
            "UPDATE files SET click_count = click_count + 1, last_accessed = ?1 WHERE filepath = ?2",
            params![now, filepath],
        )?;
        Ok(())
    }

    /// Remove entries whose files no longer exist on disk.
    pub fn remove_missing_files(&self) -> SqlResult<usize> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT filepath FROM files")?;
        let paths: Vec<String> = stmt
            .query_map([], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();

        let mut removed = 0usize;
        for path in &paths {
            if !std::path::Path::new(path).exists() {
                conn.execute("DELETE FROM files WHERE filepath = ?1", params![path])?;
                removed += 1;
            }
        }
        Ok(removed)
    }

    /// Get the total number of indexed files.
    pub fn file_count(&self) -> SqlResult<i64> {
        let conn = self.conn.lock().unwrap();
        conn.query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0))
    }

    /// Set a metadata key/value pair.
    pub fn set_meta(&self, key: &str, value: &str) -> SqlResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO index_meta (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    /// Get a metadata value by key.
    pub fn get_meta(&self, key: &str) -> SqlResult<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT value FROM index_meta WHERE key = ?1")?;
        let result = stmt.query_row(params![key], |row| row.get(0));
        match result {
            Ok(val) => Ok(Some(val)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Get all file entries (for fuzzy matching in memory).
    pub fn get_all_filenames(&self) -> SqlResult<Vec<(i64, String, String, String, i64, i64, i64)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, filename, filepath, file_type, click_count, last_accessed, modified_at FROM files"
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
            ))
        })?;
        let mut result = Vec::new();
        for row in rows {
            if let Ok(entry) = row {
                result.push(entry);
            }
        }
        Ok(result)
    }

    /// Get a single file entry by id.
    pub fn get_file_by_id(&self, id: i64) -> SqlResult<Option<FileEntry>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, filename, filepath, extension, file_size, modified_at,
                    file_type, click_count, last_accessed, icon_path
             FROM files WHERE id = ?1",
        )?;
        let result = stmt.query_row(params![id], |row| {
            Ok(FileEntry {
                id: row.get(0)?,
                filename: row.get(1)?,
                filepath: row.get(2)?,
                extension: row.get(3)?,
                file_size: row.get(4)?,
                modified_at: row.get(5)?,
                file_type: row.get(6)?,
                click_count: row.get(7)?,
                last_accessed: row.get(8)?,
                icon_path: row.get(9)?,
            })
        });
        match result {
            Ok(entry) => Ok(Some(entry)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}
