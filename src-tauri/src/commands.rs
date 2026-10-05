//! Things Windows can do that are not files: lock, sleep, restart, empty the
//! recycle bin, open a page of Settings. They are indexed like everything
//! else, so they are found, ranked and learned the same way.

use crate::db::IndexEntry;
use crate::win::{COMMAND_PREFIX, SETTINGS_PREFIX};
use std::os::windows::process::CommandExt;
use std::process::Command;
use windows::Win32::System::Power::SetSuspendState;
use windows::Win32::System::Shutdown::LockWorkStation;
use windows::Win32::UI::Shell::{SHEmptyRecycleBinW, SHERB_NOCONFIRMATION, SHERB_NOPROGRESSUI, SHERB_NOSOUND};

/// (id, name). The id is what follows `command:` in the entry's path.
const SYSTEM_COMMANDS: &[(&str, &str)] = &[
    ("lock", "Lock"),
    ("sleep", "Sleep"),
    ("signout", "Sign out"),
    ("restart", "Restart"),
    ("shutdown", "Shut down"),
    ("emptybin", "Empty Recycle Bin"),
];

/// (page, name). The page is what follows `ms-settings:`.
const SETTINGS_PAGES: &[(&str, &str)] = &[
    ("network-wifi", "Wi-Fi settings"),
    ("bluetooth", "Bluetooth settings"),
    ("display", "Display settings"),
    ("sound", "Sound settings"),
    ("notifications", "Notification settings"),
    ("batterysaver", "Battery settings"),
    ("storagesense", "Storage settings"),
    ("appsfeatures", "Installed apps"),
    ("defaultapps", "Default apps"),
    ("windowsupdate", "Windows Update"),
    ("personalization", "Personalization settings"),
    ("personalization-background", "Wallpaper settings"),
    ("mousetouchpad", "Mouse settings"),
    ("keyboard", "Keyboard settings"),
    ("dateandtime", "Date and time settings"),
    ("regionlanguage", "Language settings"),
    ("privacy", "Privacy settings"),
    ("about", "About this PC"),
];

fn entry(path: String, name: &str) -> IndexEntry {
    IndexEntry {
        filename: name.to_string(),
        filepath: path,
        extension: String::new(),
        file_size: 0,
        modified_at: 0,
        file_type: "command".to_string(),
    }
}

/// Every built-in command, as index entries.
pub fn entries() -> Vec<IndexEntry> {
    let system = SYSTEM_COMMANDS.iter().map(|(id, name)| entry(format!("{}{}", COMMAND_PREFIX, id), name));
    let settings = SETTINGS_PAGES.iter().map(|(page, name)| entry(format!("{}{}", SETTINGS_PREFIX, page), name));
    system.chain(settings).collect()
}

/// Run Windows' own shutdown tool, without a console window flashing up.
fn shutdown_tool(flag: &str) -> Result<(), String> {
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let tool = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_string()) + r"\System32\shutdown.exe";
    Command::new(tool)
        .args([flag, "/t", "0"])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Could not run shutdown: {}", e))
}

/// Run the system command with this id (the part after `command:`).
pub fn run(id: &str) -> Result<(), String> {
    match id {
        "lock" => unsafe { LockWorkStation() }.map_err(|e| format!("Could not lock: {}", e)),
        "sleep" => {
            // (hibernate, force, wake events disabled)
            if unsafe { SetSuspendState(false, false, false) }.as_bool() {
                Ok(())
            } else {
                Err("This PC could not go to sleep".to_string())
            }
        }
        "signout" => shutdown_tool("/l"),
        "restart" => shutdown_tool("/r"),
        "shutdown" => shutdown_tool("/s"),
        "emptybin" => {
            let flags = SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND;
            // Fails when the bin is already empty, which is fine
            let _ = unsafe { SHEmptyRecycleBinW(None, None, flags) };
            Ok(())
        }
        _ => Err(format!("Unknown command: {}", id)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_command_is_indexed_with_a_unique_path() {
        let all = entries();
        assert_eq!(all.len(), SYSTEM_COMMANDS.len() + SETTINGS_PAGES.len());
        let paths: std::collections::HashSet<&str> = all.iter().map(|e| e.filepath.as_str()).collect();
        assert_eq!(paths.len(), all.len());
        assert!(all.iter().all(|e| e.file_type == "command" && !e.filename.is_empty()));
        assert!(paths.contains("command:lock"));
        assert!(paths.contains("ms-settings:display"));
    }

    #[test]
    fn every_listed_system_command_is_implemented() {
        // Checked without running them: an unknown id is the only "not implemented" path.
        assert!(run("definitely-not-a-command").unwrap_err().contains("Unknown command"));
        let implemented = ["lock", "sleep", "signout", "restart", "shutdown", "emptybin"];
        for (id, _) in SYSTEM_COMMANDS {
            assert!(implemented.contains(id), "{} has no implementation", id);
        }
    }
}
