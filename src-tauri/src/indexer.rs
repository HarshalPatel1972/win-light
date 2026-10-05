use crate::apps;
use crate::db::{Database, IndexEntry};
use crate::searcher::SearchIndex;
use log::{error, info, warn};
use notify::event::{EventKind, ModifyKind};
use notify::{RecursiveMode, Watcher};
use std::collections::HashSet;
use std::fs::Metadata;
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use walkdir::WalkDir;

/// A directory tree that gets indexed.
#[derive(Debug, Clone, PartialEq)]
pub struct Root {
    pub path: PathBuf,
    /// Only executables are indexed (used for Program Files, which otherwise
    /// floods the index with DLLs and data files nobody searches for).
    pub apps_only: bool,
    /// How many folder levels below `path` are indexed.
    pub max_depth: usize,
    /// For the user's profile: outside the folders people actually keep their
    /// things in, only this many levels are indexed. A profile is mostly
    /// tool caches and package stores, and none of that should be searchable.
    pub shallow_depth: Option<usize>,
    /// Folders the user asked to keep out of the index (lowercase paths).
    pub excluded: Arc<Vec<String>>,
}

/// What the user has asked to be indexed beyond, or kept out of, the defaults.
#[derive(Debug, Clone, Default)]
pub struct IndexOptions {
    /// Extra folders to index in full.
    pub include: Vec<String>,
    /// Folders never to index, wherever they are.
    pub exclude: Vec<String>,
    /// Interface language, for the names of built-in commands.
    pub language: String,
}

impl Root {
    fn new(path: PathBuf, apps_only: bool, max_depth: usize) -> Self {
        Root { path, apps_only, max_depth, shallow_depth: None, excluded: Arc::default() }
    }

    /// Whether the user excluded `path` or a folder above it.
    fn is_excluded(&self, path: &Path) -> bool {
        if self.excluded.is_empty() {
            return false;
        }
        let path = path.to_string_lossy().to_lowercase();
        self.excluded.iter().any(|folder| {
            path.strip_prefix(folder.as_str()).is_some_and(|rest| rest.is_empty() || rest.starts_with('\\'))
        })
    }

    /// How deep `rel` (a path relative to this root) may be indexed.
    fn depth_for(&self, rel: &Path) -> usize {
        let Some(shallow) = self.shallow_depth else { return self.max_depth };
        let first = rel.components().next().map(|c| c.as_os_str().to_string_lossy().to_lowercase());
        match first {
            Some(name) if PERSONAL_FOLDERS.contains(&name.as_str()) || name.starts_with("onedrive") => self.max_depth,
            _ => shallow,
        }
    }
}

/// Determines the file_type category from extension and path context.
pub fn classify_file(extension: &str, filepath: &str, is_dir: bool) -> String {
    let ext_lower = extension.to_lowercase();

    // Folders
    if is_dir {
        return "folder".to_string();
    }

    // Application types
    if matches!(ext_lower.as_str(), "exe" | "msi" | "appx" | "msix") {
        return "app".to_string();
    }

    // Shortcuts (often point to applications)
    if ext_lower == "lnk" || ext_lower == "url" {
        return "shortcut".to_string();
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
    if filepath.to_lowercase().contains("start menu") {
        return "app".to_string();
    }

    "other".to_string()
}

/// Collects all directories that should be indexed.
pub fn index_roots(options: &IndexOptions) -> Vec<Root> {
    let mut roots: Vec<Root> = Vec::new();
    let home = dirs::home_dir();
    // `inside_home` folders are skipped when the home scan already covers them.
    let mut add = |path: Option<PathBuf>, apps_only: bool, max_depth: usize, inside_home: bool| {
        let Some(path) = path else { return };
        let covered = inside_home && home.as_ref().is_some_and(|h| path.starts_with(h));
        if path.is_dir() && !covered && !roots.iter().any(|r| r.path == path) {
            roots.push(Root::new(path, apps_only, max_depth));
        }
    };

    // Everything the user keeps: the whole profile, not just a few folders.
    add(home.clone(), false, HOME_DEPTH, false);

    // Desktop, Documents and Downloads are resolved through the Known Folders
    // API in case they were moved out of the profile (another drive, a share).
    add(dirs::desktop_dir(), false, HOME_DEPTH, true);
    add(dirs::document_dir(), false, HOME_DEPTH, true);
    add(dirs::download_dir(), false, HOME_DEPTH, true);
    add(std::env::var_os("PUBLIC").map(|p| PathBuf::from(p).join("Desktop")), false, DEFAULT_DEPTH, false);

    // Start Menu (both user and system). The user's lives under AppData, which
    // the home scan skips, so it is always its own root.
    let start_menu = |base: PathBuf| base.join("Microsoft").join("Windows").join("Start Menu");
    add(dirs::data_dir().map(start_menu), false, DEFAULT_DEPTH, false);
    add(std::env::var_os("ProgramData").map(|p| start_menu(PathBuf::from(p))), false, DEFAULT_DEPTH, false);

    // Program Files
    add(std::env::var_os("ProgramFiles").map(PathBuf::from), true, DEFAULT_DEPTH, false);
    add(std::env::var_os("ProgramFiles(x86)").map(PathBuf::from), true, DEFAULT_DEPTH, false);

    // Other internal drives (D:, E:, ...), where people keep projects and media.
    // The system drive is already covered by the roots above.
    let system_drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".to_string()).to_uppercase();
    for drive in crate::win::fixed_drives() {
        if !drive.to_string_lossy().to_uppercase().starts_with(&system_drive) {
            add(Some(drive), false, DRIVE_DEPTH, false);
        }
    }

    // Folders the user added are indexed in full, wherever they are
    for folder in &options.include {
        add(Some(PathBuf::from(folder)), false, HOME_DEPTH, false);
    }

    if let Some(profile) = roots.iter_mut().find(|root| Some(&root.path) == home.as_ref()) {
        profile.shallow_depth = Some(PROFILE_SHALLOW_DEPTH);
    }

    let excluded = Arc::new(
        options
            .exclude
            .iter()
            .map(|folder| folder.trim_end_matches('\\').to_lowercase())
            .filter(|folder| !folder.is_empty())
            .collect::<Vec<_>>(),
    );
    for root in &mut roots {
        root.excluded = excluded.clone();
    }
    roots
}

/// Folder levels indexed below a root, to keep deeply nested trees out.
const DEFAULT_DEPTH: usize = 6;
/// The user's own folders are worth following further down.
const HOME_DEPTH: usize = 8;
/// ...but only inside the folders that hold their own things. Elsewhere in the
/// profile (tool folders, caches, package stores) just the top levels are kept.
const PROFILE_SHALLOW_DEPTH: usize = 2;
const PERSONAL_FOLDERS: &[&str] = &["desktop", "documents", "downloads", "pictures", "music", "videos"];

/// By-products of tools, never opened by hand.
const NOISE_EXTENSIONS: &[&str] = &["meta", "pyc", "class", "o", "obj", "pdb", "tlog", "tmp", "cache", "map"];

/// Whole drives are indexed shallowly.
const DRIVE_DEPTH: usize = 5;

/// Hard ceiling on the index, so an unusual disk can never exhaust memory.
const MAX_ENTRIES: usize = 400_000;

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
    // Build output and package folders: thousands of files nobody opens by hand
    "target",
    "dist",
    "obj",
    "venv",
    "site-packages",
    "pkg",
    "vendor",
    "packages",
    "packagecache",
    "program files",
    "program files (x86)",
    "programdata",
];

/// Whether a path found elsewhere (e.g. by Windows Search) runs through a
/// folder the index itself would never enter.
pub fn is_in_skipped_dir(path: &str) -> bool {
    let mut parts: Vec<&str> = path.split('\\').collect();
    parts.pop(); // the file name itself is not a folder
    parts.iter().skip(1).any(|part| should_skip_dir(part))
}

/// Check if a directory name should be skipped.
fn should_skip_dir(name: &str) -> bool {
    name.starts_with('.') || SKIP_DIRS.iter().any(|skip| name.eq_ignore_ascii_case(skip))
}

const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
const FILE_ATTRIBUTE_SYSTEM: u32 = 0x4;

/// Decide whether `path` belongs in the index and, if so, describe it.
///
/// This is the single place the indexing rules live; both the full scan and
/// the file watcher go through it, so they can never disagree.
fn make_entry(root: &Root, path: &Path, metadata: &Metadata) -> Option<IndexEntry> {
    let rel = path.strip_prefix(&root.path).ok()?;
    let components: Vec<_> = rel.components().collect();
    if components.len() > root.depth_for(rel) {
        return None;
    }

    if root.is_excluded(path) {
        return None;
    }

    let is_dir = metadata.is_dir();

    // Anything below a skipped directory is out; so is a skipped directory itself.
    let dir_components = if is_dir { components.len() } else { components.len().saturating_sub(1) };
    if components[..dir_components]
        .iter()
        .any(|c| should_skip_dir(&c.as_os_str().to_string_lossy()))
    {
        return None;
    }

    // Hidden and system files (desktop.ini, thumbs.db, ...) are noise.
    if !components.is_empty() && metadata.file_attributes() & (FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM) != 0 {
        return None;
    }

    let filename = path.file_name()?.to_string_lossy().to_string();
    let extension = if is_dir {
        String::new()
    } else {
        path.extension().map(|e| e.to_string_lossy().to_string()).unwrap_or_default()
    };

    if root.apps_only && (is_dir || !extension.eq_ignore_ascii_case("exe")) {
        return None;
    }
    if NOISE_EXTENSIONS.iter().any(|noise| extension.eq_ignore_ascii_case(noise)) {
        return None;
    }

    let filepath = path.to_string_lossy().to_string();
    let modified_at = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    Some(IndexEntry {
        file_type: classify_file(&extension, &filepath, is_dir),
        filename,
        filepath,
        extension,
        file_size: if is_dir { 0 } else { metadata.len() as i64 },
        modified_at,
    })
}

/// Walk `start` (which must be `root.path` or something beneath it) and collect
/// every entry that belongs in the index.
fn scan_tree(root: &Root, start: &Path, out: &mut Vec<IndexEntry>) {
    let depth_of_start = start.strip_prefix(&root.path).map_or(0, |rel| rel.components().count());
    let walker = WalkDir::new(start)
        .max_depth(root.max_depth.saturating_sub(depth_of_start))
        .into_iter()
        .filter_entry(|entry| {
            // Don't descend into skipped or hidden directories (the start itself is always walked)
            if entry.depth() == 0 || !entry.file_type().is_dir() {
                return true;
            }
            let hidden = entry
                .metadata()
                .is_ok_and(|m| m.file_attributes() & (FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM) != 0);
            // No point walking into a folder whose contents would be too deep to keep
            let too_deep = entry
                .path()
                .strip_prefix(&root.path)
                .is_ok_and(|rel| rel.components().count() >= root.depth_for(rel));
            !hidden
                && !too_deep
                && !root.is_excluded(entry.path())
                && !should_skip_dir(&entry.file_name().to_string_lossy())
        });

    for entry in walker {
        if out.len() >= MAX_ENTRIES {
            warn!("Index is full ({} entries); the rest of {} is not indexed", MAX_ENTRIES, start.display());
            break;
        }
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                // Permission denied, locked or vanished files are expected; skip silently.
                let expected = e.io_error().is_some_and(|io| {
                    matches!(
                        io.kind(),
                        std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::NotFound
                    ) || matches!(io.raw_os_error(), Some(5 | 32 | 1920 | 1921))
                });
                if !expected {
                    warn!("Walk error: {}", e);
                }
                continue;
            }
        };

        let Ok(metadata) = entry.metadata() else { continue };
        if let Some(index_entry) = make_entry(root, entry.path(), &metadata) {
            out.push(index_entry);
        }
    }
}

/// Scan every root plus the installed Store apps.
fn scan_all(roots: &[Root], language: &str) -> Vec<IndexEntry> {
    let mut entries = Vec::new();
    for root in roots {
        info!("Indexing directory: {}", root.path.display());
        scan_tree(root, &root.path, &mut entries);
    }
    entries.extend(apps::list_store_apps());
    entries.extend(crate::commands::entries(language));
    drop_programs_that_have_a_shortcut(&mut entries);
    entries
}

/// An app normally shows up twice: as its Start Menu shortcut ("PowerPoint")
/// and as the program file that shortcut starts ("POWERPNT.EXE"). Keep the
/// shortcut, which carries the name people know, and drop the duplicate.
fn drop_programs_that_have_a_shortcut(entries: &mut Vec<IndexEntry>) {
    let _com = crate::win::ComGuard::new();
    let targets: HashSet<String> = entries
        .iter()
        .filter(|e| e.extension.eq_ignore_ascii_case("lnk"))
        .filter_map(|e| crate::win::shortcut_target(&e.filepath))
        .map(|target| target.to_lowercase())
        .collect();
    entries.retain(|e| !(e.extension.eq_ignore_ascii_case("exe") && targets.contains(&e.filepath.to_lowercase())));
}

/// Whether anything at `path` is ruled out by a skipped folder on the way to
/// it. Works on the path alone, so it also answers for files that are gone.
fn is_under_skipped_dir(root: &Root, path: &Path) -> bool {
    root.is_excluded(path) || path.strip_prefix(&root.path).is_ok_and(|rel| {
        rel.components().any(|c| should_skip_dir(&c.as_os_str().to_string_lossy()))
    })
}

/// The installed apps only (Start Menu shortcuts and Store apps), as file
/// entries. Takes well under a second, unlike a full scan.
pub fn quick_app_entries() -> Vec<crate::db::FileEntry> {
    let mut entries = Vec::new();
    for root in index_roots(&IndexOptions::default()) {
        if root.path.to_string_lossy().to_lowercase().contains("start menu") {
            scan_tree(&root, &root.path, &mut entries);
        }
    }
    entries.extend(apps::list_store_apps());
    entries
        .into_iter()
        .filter(|e| e.file_type != "folder")
        .map(|e| crate::db::FileEntry {
            id: 0,
            filename: e.filename,
            filepath: e.filepath,
            extension: e.extension,
            file_size: e.file_size,
            modified_at: e.modified_at,
            file_type: e.file_type,
            click_count: 0,
            last_accessed: 0,
        })
        .collect()
}

/// Number of rows a full scan must remove before the database is compacted.
const VACUUM_THRESHOLD: usize = 10_000;

/// Performs a full scan and makes the database and the in-memory index match
/// it exactly (new files added, vanished files dropped).
/// Returns the number of entries indexed.
pub fn full_index(db: &Database, index: &SearchIndex, options: &IndexOptions) -> Result<usize, String> {
    let roots = index_roots(options);
    info!("Starting full index of {} directories", roots.len());

    let entries = scan_all(&roots, &options.language);
    let removed = db
        .replace_all(&entries)
        .map_err(|e| format!("Failed to store index: {}", e))?;
    let _ = db.set_meta("last_full_index", &chrono::Utc::now().timestamp().to_string());
    index.reload(db)?;

    // A large purge (e.g. the first scan after the index rules got stricter)
    // leaves the database file mostly empty; shrink it.
    if removed > VACUUM_THRESHOLD {
        if let Err(e) = db.vacuum() {
            warn!("Could not compact the index database: {}", e);
        }
    }

    info!("Full index complete: {} entries indexed, {} removed", entries.len(), removed);
    Ok(entries.len())
}

// ────────────────────── Live updates ──────────────────────

/// How long the file system must stay quiet before a batch of changes is applied.
const WATCH_QUIET: Duration = Duration::from_millis(750);
/// Upper bound on how long changes are collected before being applied anyway.
const WATCH_MAX_WAIT: Duration = Duration::from_secs(5);

/// Whether a file system event can change what is in the index. Content
/// modifications are ignored: they fire constantly (e.g. during a download) and
/// only affect size/mtime, which the periodic full scan refreshes.
fn is_structural(kind: &EventKind) -> bool {
    matches!(
        kind,
        EventKind::Create(_) | EventKind::Remove(_) | EventKind::Modify(ModifyKind::Name(_))
    )
}

/// The root a changed path belongs to, unless the change is somewhere the
/// index ignores anyway. The profile contains folders (AppData above all) that
/// change many times a second; this keeps all of that from costing anything.
fn watched_root<'a>(roots: &'a [Root], path: &Path) -> Option<&'a Root> {
    roots
        .iter()
        .filter(|root| path.starts_with(&root.path))
        // The most specific root wins (the Start Menu lives inside the profile)
        .max_by_key(|root| root.path.as_os_str().len())
        .filter(|root| !is_under_skipped_dir(root, path))
}

/// Bring the index up to date for a set of paths that were created, removed or renamed.
fn apply_changes(roots: &[Root], db: &Database, index: &SearchIndex, changed: HashSet<PathBuf>) {
    let mut upserts = Vec::new();
    let mut deletes = Vec::new();

    for path in changed {
        let Some(root) = watched_root(roots, &path) else { continue };
        match std::fs::metadata(&path) {
            // A directory that appeared (or was renamed) brings its contents with it.
            Ok(metadata) if metadata.is_dir() => scan_tree(root, &path, &mut upserts),
            Ok(metadata) => upserts.extend(make_entry(root, &path, &metadata)),
            Err(_) => deletes.push(path.to_string_lossy().to_string()),
        }
    }

    if upserts.is_empty() && deletes.is_empty() {
        return;
    }
    if let Err(e) = db.delete_paths(&deletes) {
        error!("Failed to remove deleted files from index: {}", e);
    }
    if let Err(e) = db.upsert_files_batch(&upserts) {
        error!("Failed to add new files to index: {}", e);
    }
    if let Err(e) = index.reload(db) {
        error!("{}", e);
    }
    info!("Index updated: {} added/changed, {} removed", upserts.len(), deletes.len());
}

/// Watch the indexed directories and apply changes as they happen, so new
/// files are searchable within a second without rescanning the disk.
/// `on_update` is called after each applied batch.
///
/// Watching lasts as long as the returned watcher is kept; dropping it (for
/// example to start a new one after the indexed folders changed) ends it.
pub fn start_watcher(
    db: Arc<Database>,
    index: Arc<SearchIndex>,
    options: &IndexOptions,
    on_update: impl Fn() + Send + 'static,
) -> notify::Result<notify::RecommendedWatcher> {
    let roots = index_roots(options);
    let root_count = roots.len();
    let (tx, rx) = mpsc::channel::<PathBuf>();

    let filter_roots = roots.clone();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(event) = res {
            if is_structural(&event.kind) {
                for path in event.paths {
                    if watched_root(&filter_roots, &path).is_some() {
                        let _ = tx.send(path);
                    }
                }
            }
        }
    })?;

    for root in &roots {
        if let Err(e) = watcher.watch(&root.path, RecursiveMode::Recursive) {
            warn!("Cannot watch {}: {}", root.path.display(), e);
        }
    }

    std::thread::Builder::new()
        .name("index-watcher".to_string())
        .spawn(move || {
            // Ends by itself when the watcher (which holds the sending side) is dropped
            while let Ok(first) = rx.recv() {
                let started = Instant::now();
                let mut changed = HashSet::from([first]);
                while started.elapsed() < WATCH_MAX_WAIT {
                    match rx.recv_timeout(WATCH_QUIET) {
                        Ok(path) => {
                            changed.insert(path);
                        }
                        Err(_) => break,
                    }
                }
                apply_changes(&roots, &db, &index, changed);
                on_update();
            }
        })
        .map_err(notify::Error::io)?;

    info!("Watching {} directories for changes", root_count);
    Ok(watcher)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn names(entries: &[IndexEntry]) -> Vec<String> {
        let mut n: Vec<String> = entries.iter().map(|e| e.filename.clone()).collect();
        n.sort();
        n
    }

    fn scan(root: &Root) -> Vec<IndexEntry> {
        let mut out = Vec::new();
        scan_tree(root, &root.path, &mut out);
        out
    }

    fn open_db(dir: &Path) -> Database {
        Database::open(&dir.join("index.db")).unwrap()
    }

    #[test]
    fn excluded_folders_are_left_out_with_everything_inside_them() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path();
        fs::create_dir_all(base.join("private").join("deep")).unwrap();
        fs::create_dir_all(base.join("private-not")).unwrap();
        fs::write(base.join("private").join("secret.txt"), "x").unwrap();
        fs::write(base.join("private").join("deep").join("more.txt"), "x").unwrap();
        fs::write(base.join("private-not").join("visible.txt"), "x").unwrap();
        fs::write(base.join("open.txt"), "x").unwrap();

        let mut root = Root::new(base.to_path_buf(), false, 6);
        // Stored lowercase, matched whatever the case on disk
        root.excluded = Arc::new(vec![base.join("PRIVATE").to_string_lossy().to_lowercase()]);
        let found = names(&scan(&root));

        assert!(found.contains(&"open.txt".to_string()));
        assert!(found.contains(&"visible.txt".to_string()), "a folder that merely starts the same is not excluded");
        assert!(!found.contains(&"private".to_string()));
        assert!(!found.contains(&"secret.txt".to_string()));
        assert!(!found.contains(&"more.txt".to_string()));
    }

    #[test]
    fn recognises_paths_inside_skipped_folders() {
        assert!(is_in_skipped_dir(r"C:\Users\me\proj\Library\PackageCache\com.x\CHANGELOG.md"));
        assert!(is_in_skipped_dir(r"C:\Users\me\app\node_modules\pkg\README.md"));
        assert!(is_in_skipped_dir(r"C:\Users\me\.config\notes.txt"));
        assert!(!is_in_skipped_dir(r"C:\Users\me\Documents\CHANGELOG.md"));
        // A file may be called like a skipped folder
        assert!(!is_in_skipped_dir(r"C:\Users\me\Documents\target"));
    }

    #[test]
    fn classifies_by_extension_and_kind() {
        assert_eq!(classify_file("EXE", r"C:\x\a.EXE", false), "app");
        assert_eq!(classify_file("lnk", r"C:\x\a.lnk", false), "shortcut");
        assert_eq!(classify_file("pdf", r"C:\x\a.pdf", false), "document");
        assert_eq!(classify_file("", r"C:\x\my.folder", true), "folder");
        assert_eq!(classify_file("xyz", r"C:\x\a.xyz", false), "other");
    }

    #[test]
    fn scan_skips_ignored_directories_and_respects_depth() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path();
        fs::create_dir_all(base.join("project/node_modules/pkg")).unwrap();
        fs::create_dir_all(base.join(".hidden")).unwrap();
        fs::create_dir_all(base.join("a/b/c/d/e/f/g")).unwrap();
        fs::write(base.join("project/readme.md"), "x").unwrap();
        fs::write(base.join("project/node_modules/pkg/index.js"), "x").unwrap();
        fs::write(base.join(".hidden/secret.txt"), "x").unwrap();
        fs::write(base.join("a/b/c/d/e/deep.txt"), "x").unwrap();
        fs::write(base.join("a/b/c/d/e/f/g/too-deep.txt"), "x").unwrap();

        let root = Root::new(base.to_path_buf(), false, 6);
        let found = names(&scan(&root));

        assert!(found.contains(&"readme.md".to_string()));
        assert!(found.contains(&"deep.txt".to_string()));
        assert!(!found.contains(&"index.js".to_string()));
        assert!(!found.contains(&"node_modules".to_string()));
        assert!(!found.contains(&"secret.txt".to_string()));
        assert!(!found.contains(&"too-deep.txt".to_string()));
    }

    #[test]
    fn apps_only_roots_index_executables_only() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("Acme/App")).unwrap();
        fs::write(tmp.path().join("Acme/App/app.exe"), "x").unwrap();
        fs::write(tmp.path().join("Acme/App/library.dll"), "x").unwrap();
        fs::write(tmp.path().join("Acme/App/readme.txt"), "x").unwrap();

        let root = Root::new(tmp.path().to_path_buf(), true, 6);

        assert_eq!(names(&scan(&root)), vec!["app.exe"]);
    }

    #[test]
    fn only_structural_events_trigger_updates() {
        use notify::event::{CreateKind, DataChange, RemoveKind, RenameMode};
        assert!(is_structural(&EventKind::Create(CreateKind::Any)));
        assert!(is_structural(&EventKind::Remove(RemoveKind::Any)));
        assert!(is_structural(&EventKind::Modify(ModifyKind::Name(RenameMode::To))));
        assert!(!is_structural(&EventKind::Modify(ModifyKind::Data(DataChange::Any))));
        assert!(!is_structural(&EventKind::Modify(ModifyKind::Any)));
    }

    #[test]
    fn apply_changes_adds_new_files_and_removes_deleted_trees() {
        let tmp = tempfile::tempdir().unwrap();
        let files = tmp.path().join("files");
        let (old, new, modules) = (files.join("old"), files.join("new"), files.join("node_modules"));
        let indexed = |index: &SearchIndex, path: PathBuf| index.contains_path(&path.to_string_lossy());
        fs::create_dir_all(&old).unwrap();
        fs::write(old.join("gone.txt"), "x").unwrap();

        let roots = vec![Root::new(files.clone(), false, 6)];
        let db = open_db(tmp.path());
        let index = SearchIndex::new();
        db.replace_all(&scan(&roots[0])).unwrap();
        index.reload(&db).unwrap();
        assert!(indexed(&index, old.join("gone.txt")));

        // Delete a directory tree, add a new directory with a file in it.
        fs::remove_dir_all(&old).unwrap();
        fs::create_dir_all(&new).unwrap();
        fs::write(new.join("fresh.txt"), "x").unwrap();
        fs::create_dir_all(&modules).unwrap();
        fs::write(modules.join("ignored.js"), "x").unwrap();

        let changed = HashSet::from([old.clone(), new.clone(), modules.clone()]);
        apply_changes(&roots, &db, &index, changed);

        assert!(!indexed(&index, old.clone()));
        assert!(!indexed(&index, old.join("gone.txt")));
        assert!(indexed(&index, new.clone()));
        assert!(indexed(&index, new.join("fresh.txt")));
        assert!(!indexed(&index, modules.clone()));
        assert!(!indexed(&index, modules.join("ignored.js")));
    }
}
