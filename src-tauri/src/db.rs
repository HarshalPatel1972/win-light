use rusqlite::{params, Connection, Result as SqlResult};
use std::path::Path;
use std::sync::Mutex;

/// A file discovered by the indexer, ready to be written to the database.
#[derive(Debug, Clone, PartialEq)]
pub struct IndexEntry {
    pub filename: String,
    pub filepath: String,
    pub extension: String,
    pub file_size: i64,
    pub modified_at: i64,
    pub file_type: String, // "app", "document", "folder", "shortcut", "other"
}

/// Represents a single indexed file entry stored in SQLite.
#[derive(Debug, Clone)]
pub struct FileEntry {
    pub id: i64,
    pub filename: String,
    pub filepath: String,
    pub extension: String,
    pub file_size: i64,
    pub modified_at: i64,
    pub file_type: String,
    pub click_count: i64,
    pub last_accessed: i64,
}

/// Thread-safe database wrapper.
///
/// The database is the persistent store only; searches run against the
/// in-memory `SearchIndex`, so nothing on the search path waits on this lock.
pub struct Database {
    conn: Mutex<Connection>,
}

const UPSERT_SQL: &str =
    "INSERT INTO files (filename, filepath, extension, file_size, modified_at, file_type)
     VALUES (?1, ?2, ?3, ?4, ?5, ?6)
     ON CONFLICT(filepath) DO UPDATE SET
        filename = excluded.filename,
        extension = excluded.extension,
        file_size = excluded.file_size,
        modified_at = excluded.modified_at,
        file_type = excluded.file_type";

impl Database {
    /// Open or create the SQLite database at the given path.
    pub fn open(db_path: &Path) -> SqlResult<Self> {
        let conn = Connection::open(db_path)?;

        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA temp_store = MEMORY;",
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

            CREATE TABLE IF NOT EXISTS index_meta (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            -- Left over from when searches ran in SQL; they only slow down writes now.
            DROP INDEX IF EXISTS idx_filename;
            DROP INDEX IF EXISTS idx_filepath;
            DROP INDEX IF EXISTS idx_extension;
            DROP INDEX IF EXISTS idx_file_type;
            DROP INDEX IF EXISTS idx_click_count;
            DROP INDEX IF EXISTS idx_modified_at;",
        )?;
        Ok(())
    }

    /// Insert or update file entries in a single transaction.
    /// Usage data (click count, last accessed) of existing rows is kept.
    pub fn upsert_files_batch(&self, entries: &[IndexEntry]) -> SqlResult<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare_cached(UPSERT_SQL)?;
            for e in entries {
                stmt.execute(params![
                    e.filename,
                    e.filepath,
                    e.extension,
                    e.file_size,
                    e.modified_at,
                    e.file_type
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Make the table match a complete scan: upsert every entry and drop
    /// every row the scan did not see. Returns the number of rows removed.
    pub fn replace_all(&self, entries: &[IndexEntry]) -> SqlResult<usize> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let removed;
        {
            tx.execute_batch(
                "CREATE TEMP TABLE IF NOT EXISTS seen (filepath TEXT PRIMARY KEY) WITHOUT ROWID;
                 DELETE FROM seen;",
            )?;
            let mut upsert = tx.prepare_cached(UPSERT_SQL)?;
            let mut mark = tx.prepare_cached("INSERT OR IGNORE INTO seen (filepath) VALUES (?1)")?;
            for e in entries {
                upsert.execute(params![
                    e.filename,
                    e.filepath,
                    e.extension,
                    e.file_size,
                    e.modified_at,
                    e.file_type
                ])?;
                mark.execute(params![e.filepath])?;
            }
            removed = tx.execute(
                "DELETE FROM files WHERE filepath NOT IN (SELECT filepath FROM seen)",
                [],
            )?;
            tx.execute("DELETE FROM seen", [])?;
        }
        tx.commit()?;
        Ok(removed)
    }

    /// Give the space of deleted rows back to the file system.
    pub fn vacuum(&self) -> SqlResult<()> {
        self.conn.lock().unwrap().execute_batch("VACUUM")
    }

    /// Remove the given paths and everything beneath them.
    pub fn delete_paths(&self, paths: &[String]) -> SqlResult<usize> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let mut removed = 0usize;
        {
            let mut stmt = tx.prepare_cached(
                "DELETE FROM files
                 WHERE filepath = ?1 OR substr(filepath, 1, length(?2)) = ?2",
            )?;
            for path in paths {
                let prefix = format!("{}\\", path.trim_end_matches('\\'));
                removed += stmt.execute(params![path, prefix])?;
            }
        }
        tx.commit()?;
        Ok(removed)
    }

    /// Increment the click count and update last_accessed time for a file.
    pub fn record_click(&self, filepath: &str, now: i64) -> SqlResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE files SET click_count = click_count + 1, last_accessed = ?1 WHERE filepath = ?2",
            params![now, filepath],
        )?;
        Ok(())
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

    /// Load every indexed entry (used to build the in-memory search index).
    pub fn load_all(&self) -> SqlResult<Vec<FileEntry>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, filename, filepath, extension, file_size, modified_at,
                    file_type, click_count, last_accessed
             FROM files",
        )?;
        let rows = stmt.query_map([], |row| {
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
            })
        })?;
        rows.collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str) -> IndexEntry {
        IndexEntry {
            filename: path.rsplit('\\').next().unwrap().to_string(),
            filepath: path.to_string(),
            extension: String::new(),
            file_size: 1,
            modified_at: 1,
            file_type: "other".to_string(),
        }
    }

    fn open_temp() -> (tempfile::TempDir, Database) {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("test.db")).unwrap();
        (dir, db)
    }

    fn paths(db: &Database) -> Vec<String> {
        let mut p: Vec<String> = db.load_all().unwrap().into_iter().map(|e| e.filepath).collect();
        p.sort();
        p
    }

    #[test]
    fn replace_all_removes_unseen_and_keeps_usage() {
        let (_dir, db) = open_temp();
        db.replace_all(&[entry(r"C:\a\one.txt"), entry(r"C:\a\two.txt")]).unwrap();
        db.record_click(r"C:\a\one.txt", 42).unwrap();

        let removed = db.replace_all(&[entry(r"C:\a\one.txt"), entry(r"C:\a\three.txt")]).unwrap();

        assert_eq!(removed, 1);
        assert_eq!(paths(&db), vec![r"C:\a\one.txt", r"C:\a\three.txt"]);
        let one = db.load_all().unwrap().into_iter().find(|e| e.filename == "one.txt").unwrap();
        assert_eq!((one.click_count, one.last_accessed), (1, 42));
    }

    #[test]
    fn delete_paths_removes_children_but_not_siblings() {
        let (_dir, db) = open_temp();
        db.upsert_files_batch(&[
            entry(r"C:\a\dir"),
            entry(r"C:\a\dir\child.txt"),
            entry(r"C:\a\dir2\other.txt"),
        ])
        .unwrap();

        let removed = db.delete_paths(&[r"C:\a\dir".to_string()]).unwrap();

        assert_eq!(removed, 2);
        assert_eq!(paths(&db), vec![r"C:\a\dir2\other.txt"]);
    }
}
