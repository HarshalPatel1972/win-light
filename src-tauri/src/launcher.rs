use crate::win::{is_virtual, to_wide, ComGuard, COMMAND_PREFIX};
use log::info;
use std::path::Path;
use windows::core::PCWSTR;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Shell::{ILCreateFromPathW, ILFree, SHOpenFolderAndSelectItems, ShellExecuteW};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

/// How to open something.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Verb {
    /// The target's default action.
    Default,
    /// Elevated, through a UAC prompt.
    RunAsAdmin,
    /// Let the user pick the app ("Open with…").
    OpenWith,
}

impl Verb {
    fn name(self) -> Option<&'static str> {
        match self {
            Verb::Default => None,
            Verb::RunAsAdmin => Some("runas"),
            Verb::OpenWith => Some("openas"),
        }
    }
}

/// Launch a file, folder, app or Store app through the Windows shell.
///
/// The target is handed to `ShellExecuteW` as a single argument, so nothing in
/// a file name is ever interpreted as a command. The shell also takes care of
/// shortcuts, file associations and UAC elevation.
pub fn launch(target: &str, verb: Verb) -> Result<(), String> {
    if let Some(command) = target.strip_prefix(COMMAND_PREFIX) {
        return crate::commands::run(command);
    }
    // Store apps and Settings pages are addresses the shell understands
    if is_virtual(target) {
        return shell_execute(target, None, verb);
    }

    let path = Path::new(target);
    if !path.exists() {
        return Err(format!("File not found: {}", target));
    }

    let working_dir = if path.is_dir() {
        None
    } else {
        path.parent().map(|p| p.to_string_lossy().to_string())
    };
    shell_execute(target, working_dir.as_deref(), verb)
}

/// Open a web address in the default browser. Only http(s) is accepted, so
/// this can never be used to start a program.
pub fn open_url(url: &str) -> Result<(), String> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err(format!("Not a web address: {}", url));
    }
    shell_execute(url, None, Verb::Default)
}

fn shell_execute(target: &str, working_dir: Option<&str>, verb: Verb) -> Result<(), String> {
    let target_w = to_wide(target);
    let dir_w = working_dir.map(to_wide);
    let dir_ptr = dir_w.as_ref().map_or(PCWSTR::null(), |d| PCWSTR(d.as_ptr()));
    let verb_w = verb.name().map(to_wide);
    // A null verb runs the target's default action.
    let verb_ptr = verb_w.as_ref().map_or(PCWSTR::null(), |v| PCWSTR(v.as_ptr()));

    let result = unsafe {
        ShellExecuteW(
            HWND::default(),
            verb_ptr,
            PCWSTR(target_w.as_ptr()),
            PCWSTR::null(),
            dir_ptr,
            SW_SHOWNORMAL,
        )
    };

    // ShellExecute reports success as any value greater than 32.
    let code = result.0 as isize;
    if code > 32 {
        info!("Launched: {}", target);
        return Ok(());
    }

    Err(match code {
        2 | 3 => format!("File not found: {}", target),
        // Also what a declined UAC prompt reports
        5 => format!("Access denied: {}", target),
        31 => format!("No app is associated with this file type: {}", target),
        _ => format!("Failed to open '{}' (error {})", target, code),
    })
}

/// Open the containing folder of a file in Explorer, with the file selected.
pub fn open_containing_folder(filepath: &str) -> Result<(), String> {
    if is_virtual(filepath) {
        return Err("This item has no containing folder".to_string());
    }
    if !Path::new(filepath).exists() {
        return Err(format!("File not found: {}", filepath));
    }

    let _com = ComGuard::new();
    let path_w = to_wide(filepath);
    unsafe {
        let pidl = ILCreateFromPathW(PCWSTR(path_w.as_ptr()));
        if pidl.is_null() {
            return Err(format!("Failed to resolve path: {}", filepath));
        }
        let result = SHOpenFolderAndSelectItems(pidl, None, 0);
        ILFree(Some(pidl));
        result.map_err(|e| format!("Failed to open containing folder: {}", e))?;
    }

    info!("Opened containing folder for: {}", filepath);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_web_addresses_can_be_opened_as_urls() {
        for bad in ["calc.exe", "file:///C:/Windows/System32/calc.exe", "javascript:alert(1)", "shell:AppsFolder\\X!App", ""] {
            assert!(open_url(bad).is_err(), "{} should be rejected", bad);
        }
    }

    #[test]
    fn missing_files_are_reported_not_launched() {
        let error = launch(r"C:\definitely\not\here.txt", Verb::Default).unwrap_err();
        assert!(error.contains("File not found"));
    }
}
