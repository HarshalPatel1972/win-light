mod db;
mod indexer;
mod launcher;
mod searcher;

use db::Database;
use log::{error, info};
use searcher::SearchResult;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{
    image::Image,
    menu::{MenuBuilder, MenuItemBuilder},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager,
};

/// Application state shared across all Tauri commands.
pub struct AppState {
    pub db: Arc<Database>,
    pub indexing: std::sync::atomic::AtomicBool,
}

/// Get the database file path in the app data directory.
fn get_db_path() -> PathBuf {
    let mut path = dirs::data_local_dir().unwrap_or_else(|| PathBuf::from("."));
    path.push("AnCheck");
    std::fs::create_dir_all(&path).ok();
    path.push("ancheck_index.db");
    path
}

// ────────────────────── Tauri Commands ──────────────────────

/// Perform a search query and return ranked results.
#[tauri::command]
async fn search(state: tauri::State<'_, AppState>, query: String) -> Result<Vec<SearchResult>, String> {
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || searcher::search(&db, &query, 15))
        .await
        .map_err(|e| format!("Search task failed: {}", e))?
}

/// Evaluate a math expression. Returns None-equivalent empty string if not a math expression.
#[tauri::command]
async fn eval_math(query: String) -> Result<Option<String>, String> {
    Ok(searcher::evaluate_math(&query))
}

/// Launch a file/app at the given path and record the click.
#[tauri::command]
async fn launch_file(state: tauri::State<'_, AppState>, filepath: String) -> Result<(), String> {
    // Record the click for usage boosting
    let db = state.db.clone();
    let fp = filepath.clone();
    tokio::task::spawn_blocking(move || {
        if let Err(e) = db.record_click(&fp) {
            error!("Failed to record click: {}", e);
        }
    })
    .await
    .ok();

    launcher::launch(&filepath)
}

/// Open the containing folder of a file in Explorer.
#[tauri::command]
async fn open_containing_folder(filepath: String) -> Result<(), String> {
    launcher::open_containing_folder(&filepath)
}

/// Trigger a full re-index of the file system.
#[tauri::command]
async fn rebuild_index(
    state: tauri::State<'_, AppState>,
    app: AppHandle,
) -> Result<usize, String> {
    let is_indexing = &state.indexing;

    // Prevent concurrent indexing
    if is_indexing.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return Err("Indexing is already in progress".to_string());
    }

    let db = state.db.clone();
    let result = tokio::task::spawn_blocking(move || indexer::full_index(&db))
        .await
        .map_err(|e| format!("Index task failed: {}", e))?;

    is_indexing.store(false, std::sync::atomic::Ordering::SeqCst);

    // Notify frontend that indexing is complete
    let _ = app.emit("indexing-complete", ());

    result
}

/// Get the total number of indexed files.
#[tauri::command]
async fn get_index_count(state: tauri::State<'_, AppState>) -> Result<i64, String> {
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || db.file_count().map_err(|e| format!("Count error: {}", e)))
        .await
        .map_err(|e| format!("Task failed: {}", e))?
}

/// Check if indexing is currently in progress.
#[tauri::command]
async fn is_indexing(state: tauri::State<'_, AppState>) -> Result<bool, String> {
    Ok(state.indexing.load(std::sync::atomic::Ordering::SeqCst))
}

// ────────────────────── App Setup ──────────────────────

/// Toggle window visibility: show if hidden, hide if visible.
fn toggle_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        if window.is_visible().unwrap_or(false) {
            let _ = window.hide();
        } else {
            let _ = window.show();
            let _ = window.set_focus();
            // Notify frontend to focus the search input
            let _ = app.emit("focus-search", ());
        }
    }
}

/// Set up the system tray icon and menu.
fn setup_tray(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let show_item = MenuItemBuilder::with_id("show", "Show Launcher (Ctrl+Space)").build(app)?;
    let rebuild_item = MenuItemBuilder::with_id("rebuild", "Rebuild Index").build(app)?;
    let separator = MenuItemBuilder::with_id("sep", "────────────").enabled(false).build(app)?;
    let exit_item = MenuItemBuilder::with_id("exit", "Exit").build(app)?;

    let menu = MenuBuilder::new(app)
        .item(&show_item)
        .item(&rebuild_item)
        .item(&separator)
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
        .tooltip("AnCheck - Quick Launcher")
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => toggle_window(app),
            "rebuild" => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    let state = app.state::<AppState>();
                    let db = state.db.clone();
                    let is_indexing = &state.indexing;
                    if !is_indexing.swap(true, std::sync::atomic::Ordering::SeqCst) {
                        let _ = app.emit("indexing-started", ());
                        let result = tokio::task::spawn_blocking(move || indexer::full_index(&db)).await;
                        is_indexing.store(false, std::sync::atomic::Ordering::SeqCst);
                        let _ = app.emit("indexing-complete", ());
                        match result {
                            Ok(Ok(count)) => info!("Tray rebuild: indexed {} files", count),
                            Ok(Err(e)) => error!("Tray rebuild error: {}", e),
                            Err(e) => error!("Tray rebuild task error: {}", e),
                        }
                    }
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

/// Register the global Ctrl+Space hotkey.
fn setup_global_shortcut(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

    let shortcut: Shortcut = "Ctrl+Space".parse().map_err(|e| {
        format!("Failed to parse shortcut: {:?}", e)
    })?;

    app.global_shortcut().on_shortcut(shortcut, move |app, _shortcut, event| {
        if event.state == ShortcutState::Pressed {
            toggle_window(app);
        }
    }).map_err(|e| format!("Failed to register global shortcut: {}", e))?;

    info!("Global shortcut Ctrl+Space registered");
    Ok(())
}

/// Spawn the background incremental indexing loop.
fn start_background_indexer(app: &AppHandle) {
    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        // Wait 2 minutes before first incremental index
        tokio::time::sleep(tokio::time::Duration::from_secs(120)).await;

        loop {
            let state = app_handle.state::<AppState>();
            let is_indexing = &state.indexing;

            if !is_indexing.swap(true, std::sync::atomic::Ordering::SeqCst) {
                let db = state.db.clone();
                let result =
                    tokio::task::spawn_blocking(move || indexer::incremental_index(&db)).await;

                is_indexing.store(false, std::sync::atomic::Ordering::SeqCst);

                match result {
                    Ok(Ok((indexed, removed))) => {
                        info!(
                            "Background index: {} files indexed, {} removed",
                            indexed, removed
                        );
                    }
                    Ok(Err(e)) => error!("Background index error: {}", e),
                    Err(e) => error!("Background index task error: {}", e),
                }
            }

            // Re-index every 5 minutes
            tokio::time::sleep(tokio::time::Duration::from_secs(300)).await;
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let db_path = get_db_path();
    info!("Database path: {}", db_path.display());

    let db = Database::open(&db_path).expect("Failed to open database");
    let db = Arc::new(db);

    let app_state = AppState {
        db: db.clone(),
        indexing: std::sync::atomic::AtomicBool::new(false),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            search,
            eval_math,
            launch_file,
            open_containing_folder,
            rebuild_index,
            get_index_count,
            is_indexing,
        ])
        .setup(|app| {
            let handle = app.handle().clone();

            // Set up system tray
            if let Err(e) = setup_tray(&handle) {
                error!("Failed to setup tray: {}", e);
            }

            // Register global shortcut
            if let Err(e) = setup_global_shortcut(&handle) {
                error!("Failed to setup global shortcut: {}", e);
            }

            // Hide window on focus lost
            if let Some(window) = app.get_webview_window("main") {
                let win = window.clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::Focused(false) = event {
                        let _ = win.hide();
                    }
                });
            }

            // Run initial indexing in background
            let db_clone = {
                let state = handle.state::<AppState>();
                state.db.clone()
            };
            let handle_for_index = handle.clone();
            tauri::async_runtime::spawn(async move {
                let state = handle_for_index.state::<AppState>();
                let is_indexing = &state.indexing;
                is_indexing.store(true, std::sync::atomic::Ordering::SeqCst);
                let _ = handle_for_index.emit("indexing-started", ());

                let result = tokio::task::spawn_blocking(move || indexer::full_index(&db_clone)).await;

                is_indexing.store(false, std::sync::atomic::Ordering::SeqCst);
                let _ = handle_for_index.emit("indexing-complete", ());

                match result {
                    Ok(Ok(count)) => info!("Initial index complete: {} files", count),
                    Ok(Err(e)) => error!("Initial index error: {}", e),
                    Err(e) => error!("Initial index task error: {}", e),
                }
            });

            // Start background incremental indexer
            start_background_indexer(&handle);

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
