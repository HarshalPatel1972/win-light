//! The windows that are open right now, so the launcher can switch to
//! something already running instead of starting a second copy.

use windows::core::PWSTR;
use windows::Win32::Foundation::{BOOL, CloseHandle, HWND, LPARAM};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowLongW, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsIconic,
    IsWindowVisible, SetForegroundWindow, ShowWindow, GWL_EXSTYLE, SW_RESTORE, WS_EX_TOOLWINDOW,
};

/// A window the user could switch to.
#[derive(Debug, Clone, PartialEq)]
pub struct OpenWindow {
    pub handle: isize,
    pub title: String,
    /// Full path of the program that owns it; empty if it could not be read.
    pub program: String,
}

impl OpenWindow {
    /// The program's name without folder or extension, e.g. "chrome".
    pub fn program_name(&self) -> &str {
        let file = self.program.rsplit('\\').next().unwrap_or("");
        file.rsplit_once('.').map_or(file, |(stem, _)| stem)
    }
}

unsafe fn program_of(window: HWND) -> (u32, String) {
    let mut pid = 0u32;
    GetWindowThreadProcessId(window, Some(&mut pid));
    let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
        return (pid, String::new());
    };
    let mut buffer = [0u16; 1024];
    let mut len = buffer.len() as u32;
    let path = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, PWSTR(buffer.as_mut_ptr()), &mut len)
        .map(|_| String::from_utf16_lossy(&buffer[..len as usize]))
        .unwrap_or_default();
    let _ = CloseHandle(process);
    (pid, path)
}

/// Whether this is a window a person would recognise as "open": visible,
/// titled, not a tool palette, and not one of the invisible placeholder
/// windows Windows keeps for suspended Store apps.
unsafe fn is_switchable(window: HWND) -> bool {
    if !IsWindowVisible(window).as_bool() || GetWindowTextLengthW(window) == 0 {
        return false;
    }
    if GetWindowLongW(window, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW.0 != 0 {
        return false;
    }
    let mut cloaked = 0u32;
    let _ = DwmGetWindowAttribute(
        window,
        DWMWA_CLOAKED,
        &mut cloaked as *mut u32 as *mut _,
        std::mem::size_of::<u32>() as u32,
    );
    cloaked == 0
}

unsafe extern "system" fn collect(window: HWND, param: LPARAM) -> BOOL {
    let windows = &mut *(param.0 as *mut Vec<OpenWindow>);
    if is_switchable(window) {
        let mut title = vec![0u16; GetWindowTextLengthW(window) as usize + 1];
        let len = GetWindowTextW(window, &mut title) as usize;
        let (pid, program) = program_of(window);
        if pid != std::process::id() {
            windows.push(OpenWindow {
                handle: window.0 as isize,
                title: String::from_utf16_lossy(&title[..len]),
                program,
            });
        }
    }
    true.into()
}

/// Every window the user could switch to, front to back.
pub fn list() -> Vec<OpenWindow> {
    let mut windows: Vec<OpenWindow> = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(collect), LPARAM(&mut windows as *mut _ as isize));
    }
    windows
}

/// The windows whose title or program matches every word of `query`.
pub fn matching(windows: Vec<OpenWindow>, query: &str, limit: usize) -> Vec<OpenWindow> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    if words.is_empty() || query.trim().chars().count() < 2 {
        return Vec::new();
    }
    windows
        .into_iter()
        .filter(|window| {
            let haystack = format!("{} {}", window.title, window.program_name()).to_lowercase();
            words.iter().all(|word| haystack.contains(word))
        })
        .take(limit)
        .collect()
}

/// Bring a window to the front, restoring it first if it is minimised.
pub fn activate(handle: isize) -> Result<(), String> {
    let window = HWND(handle as *mut _);
    unsafe {
        if !IsWindowVisible(window).as_bool() {
            return Err("That window is no longer open".to_string());
        }
        if IsIconic(window).as_bool() {
            let _ = ShowWindow(window, SW_RESTORE);
        }
        if !SetForegroundWindow(window).as_bool() {
            return Err("Windows did not allow switching to that window".to_string());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(title: &str, program: &str) -> OpenWindow {
        OpenWindow { handle: 1, title: title.to_string(), program: program.to_string() }
    }

    #[test]
    fn matches_on_title_or_program_with_every_word() {
        let open = vec![
            window("Inbox - Gmail - Google Chrome", r"C:\Program Files\Google\Chrome\Application\chrome.exe"),
            window("report.docx - Word", r"C:\Program Files\Microsoft Office\WINWORD.EXE"),
            window("Untitled - Notepad", r"C:\Windows\notepad.exe"),
        ];
        let titles = |q: &str| -> Vec<String> { matching(open.clone(), q, 5).into_iter().map(|w| w.title).collect() };

        assert_eq!(titles("chrome"), vec!["Inbox - Gmail - Google Chrome"]);
        assert_eq!(titles("winword"), vec!["report.docx - Word"]);
        assert_eq!(titles("gmail inbox"), vec!["Inbox - Gmail - Google Chrome"]);
        assert!(titles("gmail report").is_empty());
        assert!(titles("n").is_empty(), "one letter would match almost everything");
        assert_eq!(matching(open.clone(), "o", 5).len(), 0);
    }

    #[test]
    fn program_name_drops_folder_and_extension() {
        assert_eq!(window("x", r"C:\Apps\Code.exe").program_name(), "Code");
        assert_eq!(window("x", "").program_name(), "");
    }

    #[test]
    fn lists_real_windows_without_crashing() {
        for open in list() {
            assert!(!open.title.is_empty());
            assert_ne!(open.handle, 0);
        }
    }

    #[test]
    fn activating_a_dead_window_is_an_error_not_a_crash() {
        assert!(activate(0x7FFF_FFF0).is_err());
    }
}
