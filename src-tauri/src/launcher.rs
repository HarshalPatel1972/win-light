use log::{error, info, warn};
use std::path::Path;
use std::process::Command;

/// Launch a file or application at the given path using the Windows shell.
/// Handles .exe, .lnk, directories, and documents.
pub fn launch(filepath: &str) -> Result<(), String> {
    let path = Path::new(filepath);

    if !path.exists() {
        return Err(format!("File not found: {}", filepath));
    }

    let extension = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    info!("Launching: {} (type: {})", filepath, extension);

    match extension.as_str() {
        // Direct execution for .exe files
        "exe" => launch_exe(filepath),
        // Resolve and launch .lnk shortcuts
        "lnk" => launch_shortcut(filepath),
        // Open directories in Explorer
        "" if path.is_dir() => open_in_explorer(filepath),
        // Everything else: open with default handler via ShellExecute
        _ => shell_open(filepath),
    }
}

/// Launch an .exe file directly.
fn launch_exe(filepath: &str) -> Result<(), String> {
    let parent = Path::new(filepath)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| ".".to_string());

    Command::new(filepath)
        .current_dir(&parent)
        .spawn()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::PermissionDenied {
                format!(
                    "Permission denied: '{}'. Try running as administrator.",
                    filepath
                )
            } else {
                format!("Failed to launch '{}': {}", filepath, e)
            }
        })?;

    info!("Launched exe: {}", filepath);
    Ok(())
}

/// Open a .lnk shortcut using the Windows shell.
fn launch_shortcut(filepath: &str) -> Result<(), String> {
    // Use cmd /c start to handle .lnk files properly
    shell_open(filepath)
}

/// Open a directory in Windows Explorer.
fn open_in_explorer(filepath: &str) -> Result<(), String> {
    Command::new("explorer.exe")
        .arg(filepath)
        .spawn()
        .map_err(|e| format!("Failed to open explorer for '{}': {}", filepath, e))?;

    info!("Opened directory in Explorer: {}", filepath);
    Ok(())
}

/// Use Windows ShellExecute (via cmd start) to open a file with its default handler.
fn shell_open(filepath: &str) -> Result<(), String> {
    // Use PowerShell's Start-Process for reliable ShellExecute behavior.
    // This handles .lnk, .url, documents, and any registered file types.
    Command::new("cmd")
        .args(["/C", "start", "", filepath])
        .spawn()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::PermissionDenied {
                warn!("Permission denied opening '{}', attempting elevated launch", filepath);
                format!(
                    "Permission denied: '{}'. This file may require administrator privileges.",
                    filepath
                )
            } else {
                error!("Failed to shell open '{}': {}", filepath, e);
                format!("Failed to open '{}': {}", filepath, e)
            }
        })?;

    info!("Shell opened: {}", filepath);
    Ok(())
}

/// Open the containing folder of a file in Explorer, with the file selected.
pub fn open_containing_folder(filepath: &str) -> Result<(), String> {
    let path = Path::new(filepath);
    if !path.exists() {
        return Err(format!("File not found: {}", filepath));
    }

    Command::new("explorer.exe")
        .args(["/select,", filepath])
        .spawn()
        .map_err(|e| format!("Failed to open containing folder: {}", e))?;

    info!("Opened containing folder for: {}", filepath);
    Ok(())
}
