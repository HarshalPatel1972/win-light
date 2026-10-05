//! Small helpers shared by the modules that talk to the Windows shell.

use std::path::PathBuf;
use windows::core::{Interface, PCWSTR};
use windows::Win32::Foundation::{HANDLE, HWND};
use windows::Win32::Storage::FileSystem::{GetDriveTypeW, GetLogicalDrives};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, IPersistFile, CLSCTX_INPROC_SERVER,
    COINIT_APARTMENTTHREADED, STGM_READ,
};
use windows::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::UI::Shell::{
    FileOpenDialog, IFileOpenDialog, IShellLinkW, ShellLink, FOS_FORCEFILESYSTEM, FOS_PICKFOLDERS,
    SIGDN_FILESYSPATH,
};

/// Prefix of the launch target stored for packaged (Store) apps.
pub const APPS_FOLDER_PREFIX: &str = "shell:AppsFolder\\";

/// Prefix of a built-in system command (lock, restart, ...).
pub const COMMAND_PREFIX: &str = "command:";

/// Prefix of a page in Windows Settings.
pub const SETTINGS_PREFIX: &str = "ms-settings:";

/// Whether a stored path names something other than a file on disk: a Store
/// app, a system command or a Settings page.
pub fn is_virtual(path: &str) -> bool {
    is_shell_target(path) || path.starts_with(COMMAND_PREFIX) || path.starts_with(SETTINGS_PREFIX)
}

/// Whether a stored path is a Store app.
pub fn is_shell_target(path: &str) -> bool {
    path.starts_with(APPS_FOLDER_PREFIX)
}

/// NUL-terminated UTF-16 copy of `s` for passing to Win32 APIs.
pub fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Initializes COM on the current thread for as long as the guard lives.
pub struct ComGuard {
    initialized: bool,
}

impl ComGuard {
    pub fn new() -> Self {
        // Fails if the thread already uses a different apartment model; COM is
        // usable in that case too, we just must not balance it with an uninit.
        let initialized = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.is_ok();
        ComGuard { initialized }
    }
}

impl Drop for ComGuard {
    fn drop(&mut self) {
        if self.initialized {
            unsafe { CoUninitialize() };
        }
    }
}

/// The file a `.lnk` shortcut points at. Needs COM on the calling thread.
pub fn shortcut_target(lnk_path: &str) -> Option<String> {
    let path_w = to_wide(lnk_path);
    unsafe {
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).ok()?;
        let file: IPersistFile = link.cast().ok()?;
        file.Load(PCWSTR(path_w.as_ptr()), STGM_READ).ok()?;

        let mut buffer = [0u16; 1024];
        link.GetPath(&mut buffer, std::ptr::null_mut(), 0).ok()?;
        let len = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
        (len > 0).then(|| String::from_utf16_lossy(&buffer[..len]))
    }
}

/// Let the user choose a folder with the standard Windows dialog.
/// Returns None if they cancel.
pub fn pick_folder() -> Option<String> {
    let _com = ComGuard::new();
    unsafe {
        let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
        let options = dialog.GetOptions().ok()?;
        dialog.SetOptions(options | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM).ok()?;
        // Cancelling is reported as an error
        dialog.Show(HWND::default()).ok()?;
        let raw = dialog.GetResult().ok()?.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let path = raw.to_string().ok();
        CoTaskMemFree(Some(raw.0 as *const _));
        path
    }
}

/// Root folders of the fixed (internal) drives, e.g. `C:\`, `D:\`.
pub fn fixed_drives() -> Vec<PathBuf> {
    const DRIVE_FIXED: u32 = 3;
    let mask = unsafe { GetLogicalDrives() };
    (0..26u32)
        .filter(|bit| mask & (1 << bit) != 0)
        .map(|bit| format!("{}:\\", (b'A' + bit as u8) as char))
        .filter(|root| unsafe { GetDriveTypeW(PCWSTR(to_wide(root).as_ptr())) } == DRIVE_FIXED)
        .map(PathBuf::from)
        .collect()
}

/// Put text on the Windows clipboard.
pub fn copy_to_clipboard(text: &str) -> Result<(), String> {
    const CF_UNICODETEXT: u32 = 13;
    let wide = to_wide(text);
    let bytes = wide.len() * std::mem::size_of::<u16>();
    unsafe {
        OpenClipboard(HWND::default()).map_err(|e| format!("Clipboard is busy: {}", e))?;
        let result = (|| -> windows::core::Result<()> {
            EmptyClipboard()?;
            let memory = GlobalAlloc(GMEM_MOVEABLE, bytes)?;
            let target = GlobalLock(memory) as *mut u16;
            if target.is_null() {
                return Err(windows::core::Error::from_win32());
            }
            std::ptr::copy_nonoverlapping(wide.as_ptr(), target, wide.len());
            let _ = GlobalUnlock(memory);
            // On success the clipboard owns the memory
            SetClipboardData(CF_UNICODETEXT, HANDLE(memory.0))?;
            Ok(())
        })();
        let _ = CloseClipboard();
        result.map_err(|e| format!("Could not copy: {}", e))
    }
}