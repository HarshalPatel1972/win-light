//! Enumerates packaged (Microsoft Store / UWP) apps, which have no shortcut
//! file in the Start Menu folders and so are invisible to the file walker.

use crate::db::IndexEntry;
use crate::win::{ComGuard, APPS_FOLDER_PREFIX};
use log::warn;
use windows::core::PWSTR;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::UI::Shell::{
    BHID_EnumItems, FOLDERID_AppsFolder, IEnumShellItems, IShellItem, SHGetKnownFolderItem,
    KF_FLAG_DEFAULT, SIGDN, SIGDN_NORMALDISPLAY, SIGDN_PARENTRELATIVEPARSING,
};

unsafe fn display_name(item: &IShellItem, kind: SIGDN) -> Option<String> {
    let raw: PWSTR = item.GetDisplayName(kind).ok()?;
    let name = raw.to_string().ok();
    CoTaskMemFree(Some(raw.0 as *const _));
    name
}

/// Packaged apps have an AppUserModelID of the form `PackageFamilyName!AppId`.
/// Classic apps also appear in the Apps folder, but those are already indexed
/// through their Start Menu shortcuts.
fn is_packaged_aumid(aumid: &str) -> bool {
    aumid.contains('!')
}

fn enumerate() -> windows::core::Result<Vec<IndexEntry>> {
    let _com = ComGuard::new();
    let mut apps = Vec::new();
    unsafe {
        let folder: IShellItem =
            SHGetKnownFolderItem(&FOLDERID_AppsFolder, KF_FLAG_DEFAULT, HANDLE::default())?;
        let items: IEnumShellItems = folder.BindToHandler(None, &BHID_EnumItems)?;

        loop {
            let mut slot = [None];
            let mut fetched = 0u32;
            if items.Next(&mut slot, Some(&mut fetched)).is_err() || fetched == 0 {
                break;
            }
            let Some(item) = slot[0].take() else { break };

            let (Some(name), Some(aumid)) = (
                display_name(&item, SIGDN_NORMALDISPLAY),
                display_name(&item, SIGDN_PARENTRELATIVEPARSING),
            ) else {
                continue;
            };
            if name.is_empty() || !is_packaged_aumid(&aumid) {
                continue;
            }

            apps.push(IndexEntry {
                filename: name,
                filepath: format!("{}{}", APPS_FOLDER_PREFIX, aumid),
                extension: String::new(),
                file_size: 0,
                modified_at: 0,
                file_type: "app".to_string(),
            });
        }
    }
    Ok(apps)
}

/// List installed packaged apps as index entries. Returns an empty list on failure.
pub fn list_store_apps() -> Vec<IndexEntry> {
    match enumerate() {
        Ok(apps) => apps,
        Err(e) => {
            warn!("Could not enumerate Store apps: {}", e);
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packaged_aumid_detection() {
        assert!(is_packaged_aumid("Microsoft.WindowsCalculator_8wekyb3d8bbwe!App"));
        assert!(!is_packaged_aumid(r"{6D809377-6AF0-444B-8957-A3773F02200E}\Foo\foo.exe"));
        assert!(!is_packaged_aumid("Microsoft.Office.WINWORD.EXE.15"));
    }

    /// Every Windows install ships packaged apps (Settings at minimum).
    #[test]
    fn enumerates_store_apps_on_this_machine() {
        let apps = enumerate().expect("Apps folder enumeration failed");
        assert!(!apps.is_empty(), "no packaged apps found");
        for app in &apps {
            assert!(app.filepath.starts_with(APPS_FOLDER_PREFIX));
            assert!(!app.filename.is_empty());
        }
    }
}
