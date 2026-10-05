mod apps;
mod calc;
mod commands;
mod db;
mod icons;
mod indexer;
mod intro;
mod launcher;
mod searcher;
mod settings;
mod win;
mod windows_list;
mod winsearch;

use db::Database;
use icons::IconCache;
use log::{error, info, warn};
use searcher::{SearchIndex, SearchResult};
use serde::Serialize;
use settings::Settings;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{
    image::Image,
    menu::{MenuBuilder, MenuItemBuilder},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager,
};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
use tauri_plugin_updater::UpdaterExt;

/// Command-line flag passed when Windows starts the app at login.
const HIDDEN_ARG: &str = "--hidden";

/// How often the whole index is rebuilt from disk. The file watcher keeps it
/// current in between; this only catches anything the watcher missed.
const FULL_REINDEX_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);

/// How often to look for a new release.
const UPDATE_CHECK_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);

/// Application state shared across all Tauri commands.
pub struct AppState {
    pub db: Arc<Database>,
    pub index: Arc<SearchIndex>,
    pub icons: Arc<IconCache>,
    pub indexing: AtomicBool,
    pub settings: Mutex<Settings>,
    pub settings_path: PathBuf,
    /// Whether the global hotkey is currently registered with Windows.
    pub hotkey_registered: AtomicBool,
    /// Version of a newer release, once one has been found.
    pub available_update: Mutex<Option<String>>,
    /// Files found through Windows Search, which may be opened although they
    /// are not in our own index.
    pub found_paths: Mutex<HashSet<String>>,
    /// Exchange rates for currency conversion.
    pub rates: calc::Rates,
}

/// Directory holding the index database and settings.
/// `MATCHSTICK_DATA_DIR` overrides the location (portable installs, testing).
fn app_data_dir() -> PathBuf {
    let path = match std::env::var_os("MATCHSTICK_DATA_DIR") {
        Some(dir) => PathBuf::from(dir),
        None => dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Matchstick"),
    };
    std::fs::create_dir_all(&path).ok();
    path
}

/// The app used to be called AnCheck. On the first run under the new name,
/// carry its index over so the user's launch history is not lost.
fn migrate_legacy_index(db_path: &std::path::Path) {
    if db_path.exists() || std::env::var_os("MATCHSTICK_DATA_DIR").is_some() {
        return;
    }
    let Some(local) = dirs::data_local_dir() else { return };
    let legacy = local.join("AnCheck").join("ancheck_index.db");
    if legacy.exists() {
        match std::fs::copy(&legacy, db_path) {
            Ok(_) => info!("Imported the index from the previous AnCheck install"),
            Err(e) => warn!("Could not import the AnCheck index: {}", e),
        }
    }
}

// ────────────────────── Search & launch commands ──────────────────────

/// Perform a search query and return ranked results.
#[tauri::command]
async fn search(state: tauri::State<'_, AppState>, query: String) -> Result<Vec<SearchResult>, String> {
    let index = state.index.clone();
    tokio::task::spawn_blocking(move || searcher::search(&index, &query, 15))
        .await
        .map_err(|e| format!("Search task failed: {}", e))
}

/// The items the user opens most, for the home view shown before typing.
#[tauri::command]
async fn get_suggestions(state: tauri::State<'_, AppState>) -> Result<Vec<SearchResult>, String> {
    Ok(searcher::suggestions(&state.index, 6))
}

/// The user's own apps and files, for the first-run story.
#[tauri::command]
async fn get_intro(state: tauri::State<'_, AppState>) -> Result<intro::Intro, String> {
    let (index, icons) = (state.index.clone(), state.icons.clone());
    tokio::task::spawn_blocking(move || intro::build(&index, &icons))
        .await
        .map_err(|e| format!("Intro task failed: {}", e))
}

/// Answer a calculation or conversion, if the query is one.
#[tauri::command]
async fn eval_math(state: tauri::State<'_, AppState>, query: String) -> Result<Option<calc::Answer>, String> {
    // Exchange rates are only fetched when someone actually asks about money
    if calc::needs_rates(&query) {
        state.rates.refresh_if_stale().await;
    }
    Ok(calc::evaluate(&query, state.rates.table().as_ref()))
}

/// How many documents a full-text search returns.
const CONTENT_RESULTS: usize = 6;

/// Find documents by what is written inside them, using the Windows Search index.
#[tauri::command]
async fn search_content(state: tauri::State<'_, AppState>, query: String) -> Result<Vec<SearchResult>, String> {
    // Ask for more than are shown, since some are filtered out below
    let hits = tokio::task::spawn_blocking(move || winsearch::search(&query, CONTENT_RESULTS * 4))
        .await
        .map_err(|e| format!("Content search failed: {}", e))?;

    // These files are not in our own index; remember them so they can be opened
    let mut found = state.found_paths.lock().unwrap();
    if found.len() > 5000 {
        found.clear();
    }
    // Windows indexes package and build folders too; leave those out, and list
    // each file name once, as the name search does
    let mut names = HashSet::new();
    Ok(hits
        .into_iter()
        .filter(|hit| !indexer::is_in_skipped_dir(&hit.path) && names.insert(hit.name.to_lowercase()))
        .take(CONTENT_RESULTS)
        .enumerate()
        .map(|(i, hit)| {
            found.insert(hit.path.clone());
            let extension = std::path::Path::new(&hit.path)
                .extension()
                .map(|e| e.to_string_lossy().to_string())
                .unwrap_or_default();
            SearchResult {
                // Negative ids keep these apart from indexed entries
                id: -(i as i64) - 1,
                file_type: indexer::classify_file(&extension, &hit.path, false),
                filename: hit.name,
                filepath: hit.path,
                extension,
                file_size: 0,
                modified_at: 0,
                click_count: 0,
                last_accessed: 0,
                score: 0.0,
                match_type: "content".to_string(),
                matched_indices: Vec::new(),
                snippet: hit.summary,
            }
        })
        .collect())
}

/// Only things Matchstick itself listed may be opened; the webview cannot
/// make the backend run or reveal arbitrary paths.
fn ensure_known(state: &AppState, filepath: &str) -> Result<(), String> {
    if state.index.contains_path(filepath) || state.found_paths.lock().unwrap().contains(filepath) {
        Ok(())
    } else {
        Err(format!("Not a search result: {}", filepath))
    }
}

/// Launch a file/app at the given path and record the click.
/// `mode` is "admin" to run elevated or "open_with" to choose the app.
#[tauri::command]
async fn launch_file(
    state: tauri::State<'_, AppState>,
    filepath: String,
    mode: Option<String>,
) -> Result<(), String> {
    ensure_known(&state, &filepath)?;
    let verb = match mode.as_deref() {
        Some("admin") => launcher::Verb::RunAsAdmin,
        Some("open_with") => launcher::Verb::OpenWith,
        _ => launcher::Verb::Default,
    };

    let db = state.db.clone();
    let index = state.index.clone();
    tokio::task::spawn_blocking(move || {
        launcher::launch(&filepath, verb)?;

        // Record the click for usage boosting
        let now = chrono::Utc::now().timestamp();
        index.record_click(&filepath, now);
        if let Err(e) = db.record_click(&filepath, now) {
            error!("Failed to record click: {}", e);
        }
        Ok(())
    })
    .await
    .map_err(|e| format!("Launch task failed: {}", e))?
}

/// Open the containing folder of a file in Explorer.
#[tauri::command]
async fn open_containing_folder(state: tauri::State<'_, AppState>, filepath: String) -> Result<(), String> {
    ensure_known(&state, &filepath)?;
    tokio::task::spawn_blocking(move || launcher::open_containing_folder(&filepath))
        .await
        .map_err(|e| format!("Task failed: {}", e))?
}

/// Get the shell icon of a result as a PNG data URL.
#[tauri::command]
async fn get_icon(state: tauri::State<'_, AppState>, filepath: String) -> Result<Option<String>, String> {
    ensure_known(&state, &filepath)?;
    let icons = state.icons.clone();
    tokio::task::spawn_blocking(move || icons.get(&filepath))
        .await
        .map_err(|e| format!("Icon task failed: {}", e))
}

/// What the preview pane shows for a result.
#[derive(Serialize)]
struct Preview {
    /// Thumbnail (or large icon) as a PNG data URL.
    image: Option<String>,
    /// Whether `image` shows the file's contents; if not, it is an icon and
    /// must not be enlarged.
    is_thumbnail: bool,
    size: i64,
    /// Unix time of the last change; 0 if unknown.
    modified: i64,
    is_folder: bool,
    /// The opening lines of a text file, shown when there is no thumbnail.
    text: Option<String>,
}

/// File types whose first lines are worth showing as a preview.
const TEXT_EXTENSIONS: &[&str] = &[
    "txt", "md", "log", "csv", "json", "xml", "yaml", "yml", "toml", "ini", "cfg", "rs", "py", "js", "ts", "jsx",
    "tsx", "java", "c", "cpp", "h", "cs", "go", "rb", "php", "html", "css", "sql", "sh", "bat", "ps1",
];

/// The first lines of a text file, or None if it is not text.
fn text_preview(filepath: &str) -> Option<String> {
    use std::io::Read;
    const MAX_BYTES: usize = 4096;
    const MAX_LINES: usize = 40;

    let extension = std::path::Path::new(filepath).extension()?.to_string_lossy().to_lowercase();
    if !TEXT_EXTENSIONS.contains(&extension.as_str()) {
        return None;
    }
    let mut bytes = Vec::with_capacity(MAX_BYTES);
    std::fs::File::open(filepath).ok()?.take(MAX_BYTES as u64).read_to_end(&mut bytes).ok()?;
    // A NUL byte means this is binary data wearing a text extension
    if bytes.is_empty() || bytes.contains(&0) {
        return None;
    }
    let text = String::from_utf8_lossy(&bytes);
    Some(text.lines().take(MAX_LINES).collect::<Vec<_>>().join("\n"))
}

/// A large picture and the basic facts of a result, read fresh from disk.
#[tauri::command]
async fn get_preview(state: tauri::State<'_, AppState>, filepath: String) -> Result<Preview, String> {
    ensure_known(&state, &filepath)?;
    let icons = state.icons.clone();
    tokio::task::spawn_blocking(move || {
        let metadata = std::fs::metadata(&filepath).ok();
        let modified = metadata
            .as_ref()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_secs() as i64);
        let (image, is_thumbnail) = icons.preview(&filepath).unzip();
        Preview {
            image,
            is_thumbnail: is_thumbnail.unwrap_or(false),
            size: metadata.as_ref().map_or(0, |m| if m.is_dir() { 0 } else { m.len() as i64 }),
            modified,
            is_folder: metadata.is_some_and(|m| m.is_dir()),
            text: text_preview(&filepath),
        }
    })
    .await
    .map_err(|e| format!("Preview task failed: {}", e))
}

/// How many open windows a search offers to switch to.
const WINDOW_RESULTS: usize = 3;

/// Windows that are already open and match the query.
#[tauri::command]
async fn search_windows(state: tauri::State<'_, AppState>, query: String) -> Result<Vec<SearchResult>, String> {
    let open = tokio::task::spawn_blocking(move || {
        windows_list::matching(windows_list::list(), &query, WINDOW_RESULTS)
    })
    .await
    .map_err(|e| format!("Window search failed: {}", e))?;

    // The owning program lends its icon, so its path must be allowed
    let mut found = state.found_paths.lock().unwrap();
    Ok(open
        .into_iter()
        .map(|window| {
            if !window.program.is_empty() {
                found.insert(window.program.clone());
            }
            SearchResult {
                // The window handle doubles as the id used to switch to it
                id: window.handle as i64,
                snippet: window.program_name().to_string(),
                filename: window.title,
                filepath: window.program,
                extension: String::new(),
                file_size: 0,
                modified_at: 0,
                file_type: "window".to_string(),
                click_count: 0,
                last_accessed: 0,
                score: 0.0,
                match_type: "window".to_string(),
                matched_indices: Vec::new(),
            }
        })
        .collect())
}

/// Bring an open window to the front.
#[tauri::command]
async fn activate_window(handle: i64) -> Result<(), String> {
    windows_list::activate(handle as isize)
}

/// Search a website through one of the user's keyword shortcuts.
#[tauri::command]
async fn open_quick_link(state: tauri::State<'_, AppState>, keyword: String, query: String) -> Result<(), String> {
    let link = state
        .settings
        .lock()
        .unwrap()
        .quick_links
        .iter()
        .find(|link| link.keyword.eq_ignore_ascii_case(&keyword))
        .cloned()
        .ok_or_else(|| format!("No shortcut called '{}'", keyword))?;
    launcher::open_url(&link.url.replace("{query}", &encode_query(query.trim())))
}

/// Copy text (a path, an answer) to the clipboard.
#[tauri::command]
async fn copy_text(text: String) -> Result<(), String> {
    win::copy_to_clipboard(&text)
}

/// Percent-encode a search query for use in a URL.
fn encode_query(query: &str) -> String {
    query
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            b' ' => "+".to_string(),
            _ => format!("%{:02X}", b),
        })
        .collect()
}

/// Search the web for `query` in the default browser, with the chosen engine.
#[tauri::command]
async fn web_search(state: tauri::State<'_, AppState>, query: String) -> Result<(), String> {
    let engine = state.settings.lock().unwrap().search_engine.clone();
    let base = match engine.as_str() {
        "bing" => "https://www.bing.com/search?q=",
        "duckduckgo" => "https://duckduckgo.com/?q=",
        _ => "https://www.google.com/search?q=",
    };
    launcher::open_url(&format!("{}{}", base, encode_query(query.trim())))
}

/// Open a web address typed into the search box.
#[tauri::command]
async fn open_url(url: String) -> Result<(), String> {
    launcher::open_url(&url)
}

// ────────────────────── Indexing ──────────────────────

/// Run a full index unless one is already running, notifying the frontend.
async fn run_full_index(app: &AppHandle) -> Result<usize, String> {
    let state = app.state::<AppState>();

    // Prevent concurrent indexing
    if state.indexing.swap(true, Ordering::SeqCst) {
        return Err("Indexing is already in progress".to_string());
    }
    let _ = app.emit("indexing-started", ());

    let (db, index) = (state.db.clone(), state.index.clone());
    let result = tokio::task::spawn_blocking(move || indexer::full_index(&db, &index))
        .await
        .map_err(|e| format!("Index task failed: {}", e))
        .and_then(|r| r);

    state.indexing.store(false, Ordering::SeqCst);
    let _ = app.emit("indexing-complete", ());

    match &result {
        Ok(count) => info!("Index complete: {} entries", count),
        Err(e) => error!("Index error: {}", e),
    }
    result
}

/// Trigger a full re-index of the file system.
#[tauri::command]
async fn rebuild_index(app: AppHandle) -> Result<usize, String> {
    run_full_index(&app).await
}

/// Get the total number of indexed files.
#[tauri::command]
async fn get_index_count(state: tauri::State<'_, AppState>) -> Result<usize, String> {
    Ok(state.index.len())
}

/// Check if indexing is currently in progress.
#[tauri::command]
async fn is_indexing(state: tauri::State<'_, AppState>) -> Result<bool, String> {
    Ok(state.indexing.load(Ordering::SeqCst))
}

/// Index at startup, then rebuild periodically as a safety net for the watcher.
fn start_background_indexer(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            let _ = run_full_index(&app).await;
            tokio::time::sleep(FULL_REINDEX_INTERVAL).await;
        }
    });
}

// ────────────────────── Settings ──────────────────────

/// Everything the settings screen shows.
#[derive(Serialize)]
struct SettingsView {
    hotkey: String,
    hotkey_registered: bool,
    theme: String,
    language: String,
    search_engine: String,
    quick_links: Vec<settings::QuickLink>,
    launch_at_login: bool,
    version: String,
}

#[tauri::command]
async fn get_settings(app: AppHandle, state: tauri::State<'_, AppState>) -> Result<SettingsView, String> {
    let settings = state.settings.lock().unwrap().clone();
    Ok(SettingsView {
        hotkey: settings.hotkey,
        hotkey_registered: state.hotkey_registered.load(Ordering::SeqCst),
        theme: settings.theme,
        language: settings.language,
        search_engine: settings.search_engine,
        quick_links: settings.quick_links,
        launch_at_login: app.autolaunch().is_enabled().unwrap_or(false),
        version: app.package_info().version.to_string(),
    })
}

fn parse_hotkey(hotkey: &str) -> Result<Shortcut, String> {
    hotkey
        .parse()
        .map_err(|e| format!("'{}' is not a valid shortcut: {:?}", hotkey, e))
}

/// Register `hotkey` as the global launcher shortcut.
fn register_hotkey(app: &AppHandle, hotkey: &str) -> Result<(), String> {
    let shortcut = parse_hotkey(hotkey)?;
    app.global_shortcut()
        .register(shortcut)
        .map_err(|e| format!("{} is already in use by another app ({})", hotkey, e))
}

/// Change the global shortcut. If the new one cannot be registered, the
/// previous one stays active and an error is returned.
#[tauri::command]
async fn set_hotkey(app: AppHandle, state: tauri::State<'_, AppState>, hotkey: String) -> Result<(), String> {
    let new_shortcut = parse_hotkey(&hotkey)?;
    let old_hotkey = state.settings.lock().unwrap().hotkey.clone();
    let old_registered = state.hotkey_registered.load(Ordering::SeqCst);

    if old_registered {
        if let Ok(old_shortcut) = parse_hotkey(&old_hotkey) {
            if old_shortcut == new_shortcut {
                return Ok(());
            }
            let _ = app.global_shortcut().unregister(old_shortcut);
        }
    }

    if let Err(e) = register_hotkey(&app, &hotkey) {
        if old_registered {
            let _ = register_hotkey(&app, &old_hotkey);
        }
        return Err(e);
    }

    state.hotkey_registered.store(true, Ordering::SeqCst);
    let mut settings = state.settings.lock().unwrap();
    settings.hotkey = hotkey;
    settings.save(&state.settings_path)
}

#[tauri::command]
async fn set_launch_at_login(app: AppHandle, enabled: bool) -> Result<(), String> {
    let autolaunch = app.autolaunch();
    let result = if enabled { autolaunch.enable() } else { autolaunch.disable() };
    result.map_err(|e| format!("Failed to change startup setting: {}", e))
}

#[tauri::command]
async fn set_appearance(state: tauri::State<'_, AppState>, theme: String, language: String) -> Result<(), String> {
    let mut settings = state.settings.lock().unwrap();
    settings.theme = theme;
    settings.language = language;
    settings.save(&state.settings_path)
}

#[tauri::command]
async fn set_search_engine(state: tauri::State<'_, AppState>, engine: String) -> Result<(), String> {
    let mut settings = state.settings.lock().unwrap();
    settings.search_engine = engine;
    settings.save(&state.settings_path)
}

/// Replace the keyword shortcuts. Incomplete rows and addresses that are not
/// web addresses are dropped rather than stored.
#[tauri::command]
async fn set_quick_links(
    state: tauri::State<'_, AppState>,
    links: Vec<settings::QuickLink>,
) -> Result<(), String> {
    let mut settings = state.settings.lock().unwrap();
    settings.quick_links = links
        .into_iter()
        .map(|link| settings::QuickLink {
            keyword: link.keyword.trim().to_lowercase(),
            name: link.name.trim().to_string(),
            url: link.url.trim().to_string(),
        })
        .filter(|link| {
            !link.keyword.is_empty()
                && !link.keyword.contains(char::is_whitespace)
                && (link.url.starts_with("https://") || link.url.starts_with("http://"))
        })
        .collect();
    settings.save(&state.settings_path)
}

// ────────────────────── Updates ──────────────────────

/// Ask the release server whether a newer version exists.
async fn find_update(app: &AppHandle) -> Result<Option<tauri_plugin_updater::Update>, String> {
    let update = app
        .updater()
        .map_err(|e| format!("Updater unavailable: {}", e))?
        .check()
        .await
        .map_err(|e| format!("Update check failed: {}", e))?;

    let version = update.as_ref().map(|u| u.version.clone());
    *app.state::<AppState>().available_update.lock().unwrap() = version.clone();
    if let Some(version) = version {
        info!("Update available: {}", version);
        let _ = app.emit("update-available", version);
    }
    Ok(update)
}

/// Check for a newer release. Returns its version if there is one.
#[tauri::command]
async fn check_for_update(app: AppHandle) -> Result<Option<String>, String> {
    Ok(find_update(&app).await?.map(|u| u.version))
}

/// Version of an update found by an earlier check, if any.
#[tauri::command]
async fn get_available_update(state: tauri::State<'_, AppState>) -> Result<Option<String>, String> {
    Ok(state.available_update.lock().unwrap().clone())
}

/// Download and install the latest release, then restart into it.
#[tauri::command]
async fn install_update(app: AppHandle) -> Result<(), String> {
    let Some(update) = find_update(&app).await? else {
        return Err("No update available".to_string());
    };
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|e| format!("Update failed: {}", e))?;
    app.restart()
}

/// Look for updates shortly after startup and then periodically.
fn start_update_checker(app: &AppHandle) {
    // Development builds have no release to update to.
    if cfg!(debug_assertions) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(30)).await;
        loop {
            if let Err(e) = find_update(&app).await {
                warn!("{}", e);
            }
            tokio::time::sleep(UPDATE_CHECK_INTERVAL).await;
        }
    });
}

// ────────────────────── App Setup ──────────────────────

/// Show the launcher and put the cursor in the search box.
fn show_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
        // Notify frontend to focus the search input
        let _ = app.emit("focus-search", ());
    }
}

/// Toggle window visibility: show if hidden, hide if visible.
fn toggle_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        if window.is_visible().unwrap_or(false) {
            let _ = window.hide();
        } else {
            show_window(app);
        }
    }
}

/// Set up the system tray icon and menu.
fn setup_tray(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let show_item = MenuItemBuilder::with_id("show", "Show Launcher").build(app)?;
    let settings_item = MenuItemBuilder::with_id("settings", "Settings…").build(app)?;
    let rebuild_item = MenuItemBuilder::with_id("rebuild", "Rebuild Index").build(app)?;
    let exit_item = MenuItemBuilder::with_id("exit", "Exit").build(app)?;

    let menu = MenuBuilder::new(app)
        .item(&show_item)
        .item(&settings_item)
        .item(&rebuild_item)
        .separator()
        .item(&exit_item)
        .build()?;

    let _tray = TrayIconBuilder::new()
        .icon(Image::from_path("icons/32x32.png").unwrap_or_else(|_| {
            // Fallback: use the app icon from resources
            app.default_window_icon().cloned().unwrap_or_else(|| {
                Image::from_bytes(include_bytes!("../icons/32x32.png"))
                    .expect("Failed to load tray icon")
            })
        }))
        .menu(&menu)
        .tooltip("Matchstick - Quick Launcher")
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => show_window(app),
            "settings" => {
                show_window(app);
                let _ = app.emit("open-settings", ());
            }
            "rebuild" => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = run_full_index(&app).await;
                });
            }
            "exit" => {
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let tauri::tray::TrayIconEvent::Click { button, .. } = event {
                if button == tauri::tray::MouseButton::Left {
                    toggle_window(tray.app_handle());
                }
            }
        })
        .build(app)?;

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let data_dir = app_data_dir();
    let db_path = data_dir.join("index.db");
    migrate_legacy_index(&db_path);
    let settings_path = data_dir.join("settings.json");
    info!("Database path: {}", db_path.display());

    let first_run = !settings_path.exists();
    let settings = Settings::load(&settings_path);
    if first_run {
        let _ = settings.save(&settings_path);
    }

    let db = Arc::new(Database::open(&db_path).expect("Failed to open database"));

    // Load the last session's index so searches work before the first scan finishes.
    let index = Arc::new(SearchIndex::new());
    if let Err(e) = index.reload(&db) {
        error!("{}", e);
    }

    let app_state = AppState {
        db,
        index,
        icons: Arc::new(IconCache::default()),
        indexing: AtomicBool::new(false),
        settings: Mutex::new(settings),
        settings_path,
        hotkey_registered: AtomicBool::new(false),
        available_update: Mutex::new(None),
        found_paths: Mutex::new(HashSet::new()),
        rates: calc::Rates::load(data_dir.join("rates.json")),
    };

    tauri::Builder::default()
        // Must be registered first: a second launch just surfaces the running instance.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_window(app);
        }))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state == ShortcutState::Pressed {
                        toggle_window(app);
                    }
                })
                .build(),
        )
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![HIDDEN_ARG]),
        ))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            search,
            get_suggestions,
            get_intro,
            eval_math,
            launch_file,
            open_containing_folder,
            get_icon,
            get_preview,
            search_content,
            copy_text,
            web_search,
            open_url,
            set_search_engine,
            set_quick_links,
            open_quick_link,
            search_windows,
            activate_window,
            rebuild_index,
            get_index_count,
            is_indexing,
            get_settings,
            set_hotkey,
            set_launch_at_login,
            set_appearance,
            check_for_update,
            get_available_update,
            install_update,
        ])
        .setup(move |app| {
            let handle = app.handle().clone();
            let state = handle.state::<AppState>();

            // Set up system tray
            if let Err(e) = setup_tray(&handle) {
                error!("Failed to setup tray: {}", e);
            }

            // Register global shortcut
            let hotkey = state.settings.lock().unwrap().hotkey.clone();
            match register_hotkey(&handle, &hotkey) {
                Ok(()) => {
                    state.hotkey_registered.store(true, Ordering::SeqCst);
                    info!("Global shortcut {} registered", hotkey);
                }
                Err(e) => error!("Failed to setup global shortcut: {}", e),
            }

            // A launcher is only useful if it is running, so installed builds
            // start with Windows by default; the user can turn this off in Settings.
            if first_run && !cfg!(debug_assertions) {
                if let Err(e) = handle.autolaunch().enable() {
                    warn!("Could not enable launch at login: {}", e);
                }
            }

            if let Some(window) = app.get_webview_window("main") {
                // Hide window on focus lost
                let win = window.clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::Focused(false) = event {
                        let _ = win.hide();
                    }
                });
            }

            // Stay in the tray when started by Windows at login
            if !std::env::args().any(|arg| arg == HIDDEN_ARG) {
                show_window(&handle);
            }

            // Keep the index current as files come and go
            let watcher_handle = handle.clone();
            if let Err(e) = indexer::start_watcher(state.db.clone(), state.index.clone(), move || {
                let _ = watcher_handle.emit("index-updated", ());
            }) {
                error!("Failed to start file watcher: {}", e);
            }

            start_background_indexer(&handle);
            start_update_checker(&handle);

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
