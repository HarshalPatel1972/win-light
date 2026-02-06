use crate::db::Database;
use log::{error, info, warn};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;
use walkdir::WalkDir;

/// Determines the file_type category from extension and path context.
fn classify_file(extension: &str, filepath: &str) -> String {
    let ext_lower = extension.to_lowercase();
    let path_lower = filepath.to_lowercase();

    // Application types
    if matches!(ext_lower.as_str(), "exe" | "msi" | "appx" | "msix") {
        return "app".to_string();
    }

    // Shortcuts (often point to applications)
    if ext_lower == "lnk" || ext_lower == "url" {
        return "shortcut".to_string();
    }

    // Folders
    if Path::new(filepath).is_dir() {
        return "folder".to_string();
    }

    // Documents
    if matches!(
        ext_lower.as_str(),
        "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx"
            | "txt" | "md" | "csv" | "rtf" | "odt" | "ods" | "odp"
    ) {
        return "document".to_string();
    }

    // Images
    if matches!(
        ext_lower.as_str(),
        "png" | "jpg" | "jpeg" | "gif" | "bmp" | "svg" | "webp" | "ico"
    ) {
        return "image".to_string();
    }

    // Code files
    if matches!(
        ext_lower.as_str(),
        "rs" | "py" | "js" | "ts" | "jsx" | "tsx" | "java" | "c" | "cpp"
            | "h" | "cs" | "go" | "rb" | "php" | "html" | "css" | "json"
            | "xml" | "yaml" | "yml" | "toml"
    ) {
        return "code".to_string();
    }

    // Start Menu items are apps even if they don't have .exe extension
    if path_lower.contains("start menu") {
        return "app".to_string();
    }

    "other".to_string()
}

/// Collects all directories that should be indexed.
fn get_index_directories() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    // User profile directories
    if let Some(home) = dirs::home_dir() {
        dirs.push(home.join("Desktop"));
        dirs.push(home.join("Documents"));
        dirs.push(home.join("Downloads"));
    }

    // Start Menu (both user and system)
    if let Some(data) = dirs::data_dir() {
        // %APPDATA%\Microsoft\Windows\Start Menu
        dirs.push(data.join("Microsoft").join("Windows").join("Start Menu"));
    }
    // System-wide Start Menu
    let system_start_menu = PathBuf::from(r"C:\ProgramData\Microsoft\Windows\Start Menu");
    if system_start_menu.exists() {
        dirs.push(system_start_menu);
    }

    // Program Files
    if let Ok(pf) = std::env::var("ProgramFiles") {
        dirs.push(PathBuf::from(pf));
    }
    if let Ok(pf86) = std::env::var("ProgramFiles(x86)") {
        dirs.push(PathBuf::from(pf86));
    }

    // Only keep directories that actually exist
    dirs.retain(|d| d.exists());
    dirs
}

/// Maximum directory depth to prevent scanning deeply nested node_modules etc.
const MAX_DEPTH: usize = 6;

/// Directories to skip during indexing (case-insensitive check).
const SKIP_DIRS: &[&str] = &[
    "node_modules",
    ".git",
    ".svn",
    "__pycache__",
    ".cache",
    "cache",
    ".tmp",
    "temp",
    "$recycle.bin",
    "system volume information",
    "windows",
    "appdata",
];

/// Check if a directory name should be skipped.
fn should_skip_dir(name: &str) -> bool {
    let lower = name.to_lowercase();
    SKIP_DIRS.iter().any(|&skip| lower == skip)
}

/// Performs a full index scan of all configured directories.
/// Returns the number of files indexed.
pub fn full_index(db: &Arc<Database>) -> Result<usize, String> {
    let directories = get_index_directories();
    info!("Starting full index of {} directories", directories.len());

    let mut total_indexed = 0usize;
    let mut batch: Vec<(String, String, String, i64, i64, String)> = Vec::with_capacity(1000);

    for dir in &directories {
        info!("Indexing directory: {}", dir.display());

        let walker = WalkDir::new(dir)
            .max_depth(MAX_DEPTH)
            .follow_links(true)
            .into_iter()
            .filter_entry(|entry| {
                // Skip hidden/system directories
                if entry.file_type().is_dir() {
                    if let Some(name) = entry.file_name().to_str() {
                        if name.starts_with('.') || should_skip_dir(name) {
                            return false;
                        }
                    }
                }
                true
            });

        for entry in walker {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    // Permission denied, inaccessible files, or broken symlinks - skip silently
                    if let Some(io_err) = e.io_error() {
                        let kind = io_err.kind();
                        if kind == std::io::ErrorKind::PermissionDenied
                            || kind == std::io::ErrorKind::NotFound
                        {
                            continue;
                        }
                        // Windows-specific: OS error 1920 (file cannot be accessed),
                        // OS error 5 (access denied), and similar
                        if let Some(code) = io_err.raw_os_error() {
                            if matches!(code, 5 | 32 | 1920 | 1921) {
                                continue;
                            }
                        }
                    }
                    warn!("Walk error: {}", e);
                    continue;
                }
            };

            let path = entry.path();
            let filepath = path.to_string_lossy().to_string();

            let filename = match path.file_name() {
                Some(name) => name.to_string_lossy().to_string(),
                None => continue,
            };

            let extension = path
                .extension()
                .map(|e| e.to_string_lossy().to_string())
                .unwrap_or_default();

            let metadata = match entry.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };

            let file_size = if metadata.is_file() {
                metadata.len() as i64
            } else {
                0
            };

            let modified_at = metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);

            let file_type = classify_file(&extension, &filepath);

            batch.push((filename, filepath, extension, file_size, modified_at, file_type));

            // Flush batch every 500 entries
            if batch.len() >= 500 {
                if let Err(e) = db.upsert_files_batch(&batch) {
                    error!("Failed to upsert batch: {}", e);
                }
                total_indexed += batch.len();
                batch.clear();
            }
        }
    }

    // Flush remaining entries
    if !batch.is_empty() {
        if let Err(e) = db.upsert_files_batch(&batch) {
            error!("Failed to upsert final batch: {}", e);
        }
        total_indexed += batch.len();
    }

    // Record indexing time
    let now = chrono::Utc::now().timestamp().to_string();
    let _ = db.set_meta("last_full_index", &now);

    info!("Full index complete: {} files indexed", total_indexed);
    Ok(total_indexed)
}

/// Perform an incremental re-index: remove missing files and re-scan directories.
pub fn incremental_index(db: &Arc<Database>) -> Result<(usize, usize), String> {
    info!("Starting incremental index...");

    // Remove files that no longer exist
    let removed = db.remove_missing_files().map_err(|e| format!("Remove missing failed: {}", e))?;
    if removed > 0 {
        info!("Removed {} missing files from index", removed);
    }

    // Re-scan and upsert
    let indexed = full_index(db)?;

    let now = chrono::Utc::now().timestamp().to_string();
    let _ = db.set_meta("last_incremental_index", &now);

    Ok((indexed, removed))
}
