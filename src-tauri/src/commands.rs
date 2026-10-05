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

/// The ids that follow `command:` in an entry's path. Their names are in `strings`.
const SYSTEM_COMMANDS: &[&str] = &["lock", "sleep", "signout", "restart", "shutdown", "emptybin"];

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

/// Every built-in command, as index entries, named in `language`.
/// (Settings pages keep their English names for now.)
pub fn entries(language: &str) -> Vec<IndexEntry> {
    let system = SYSTEM_COMMANDS.iter().map(|id| {
        let name = crate::strings::text(&format!("command.{}", id), language);
        entry(format!("{}{}", COMMAND_PREFIX, id), name)
    });
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

/// Environment variable that turns system commands into log entries.
pub const DRY_RUN_VARIABLE: &str = "MATCHSTICK_DRY_RUN";

/// Run the system command with this id (the part after `command:`).
pub fn run(id: &str) -> Result<(), String> {
    // Testing the launcher must never restart the machine it is tested on.
    if is_dry_run() {
        return log_instead(id);
    }
    execute(id)
}

/// Whether commands are to be logged instead of carried out.
fn is_dry_run() -> bool {
    std::env::var_os(DRY_RUN_VARIABLE).is_some()
}

/// The dry-run stand-in for `execute`: says what would have happened.
fn log_instead(id: &str) -> Result<(), String> {
    if !SYSTEM_COMMANDS.contains(&id) {
        return Err(format!("Unknown command: {}", id));
    }
    log::info!("dry run: would run system command '{}'", id);
    Ok(())
}

/// Actually carry out a system command.
fn execute(id: &str) -> Result<(), String> {
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
        let all = entries("en");
        assert_eq!(all.len(), SYSTEM_COMMANDS.len() + SETTINGS_PAGES.len());
        let paths: std::collections::HashSet<&str> = all.iter().map(|e| e.filepath.as_str()).collect();
        assert_eq!(paths.len(), all.len());
        assert!(all.iter().all(|e| e.file_type == "command" && !e.filename.is_empty()));
        assert!(paths.contains("command:lock"));
        assert!(paths.contains("ms-settings:display"));
    }

    #[test]
    fn dry_run_logs_instead_of_executing() {
        // Deliberately never calls `run` or `execute` with a real id: a
        // mistake here must not be able to restart the machine running the tests.
        for id in SYSTEM_COMMANDS {
            assert_eq!(log_instead(id), Ok(()));
        }
        assert!(log_instead("nonsense").is_err());

        std::env::set_var(DRY_RUN_VARIABLE, "1");
        assert!(is_dry_run());
        std::env::remove_var(DRY_RUN_VARIABLE);
        assert!(!is_dry_run());
    }

    #[test]
    fn every_listed_system_command_is_implemented() {
        // Checked without running them: an unknown id is the only "not implemented" path.
        assert!(execute("definitely-not-a-command").unwrap_err().contains("Unknown command"));
        let implemented = ["lock", "sleep", "signout", "restart", "shutdown", "emptybin"];
        for id in SYSTEM_COMMANDS {
            assert!(implemented.contains(id), "{} has no implementation", id);
        }
        // ...and has a name in another language
        assert!(entries("de").iter().any(|e| e.filename == "Sperren" && e.filepath == "command:lock"));
    }
}
