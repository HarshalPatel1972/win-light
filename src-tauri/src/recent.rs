//! Files the user opened recently in any app, read from the "Recent" folder
//! Windows keeps for Explorer's Quick access. A new install has no history of
//! its own, so this is what the home view offers until it does.

use crate::win::{shortcut_target, ComGuard};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// Most recent shortcuts looked at; each one costs a little to resolve.
const MAX_SHORTCUTS: usize = 40;

fn recent_folder() -> Option<PathBuf> {
    Some(dirs::data_dir()?.join("Microsoft").join("Windows").join("Recent"))
}

/// The shortcuts in `folder`, newest first, with when each was last touched.
fn newest_shortcuts(folder: &Path) -> Vec<(PathBuf, i64)> {
    let Ok(entries) = std::fs::read_dir(folder) else { return Vec::new() };
    let mut shortcuts: Vec<(PathBuf, i64)> = entries
        .flatten()
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext.eq_ignore_ascii_case("lnk")))
        .filter_map(|entry| {
            let modified = entry.metadata().ok()?.modified().ok()?;
            let seconds = modified.duration_since(UNIX_EPOCH).ok()?.as_secs() as i64;
            Some((entry.path(), seconds))
        })
        .collect();
    shortcuts.sort_by(|a, b| b.1.cmp(&a.1));
    shortcuts.truncate(MAX_SHORTCUTS);
    shortcuts
}

/// Whether `target` is something worth offering: a file on a local drive that
/// is still there. Network paths are skipped so a dead share cannot stall us.
fn is_offerable(target: &str) -> bool {
    if target.starts_with(r"\\") {
        return false;
    }
    let path = Path::new(target);
    path.is_file()
}

/// Up to `limit` recently opened files: (path, when it was opened), newest first.
pub fn recent_files(limit: usize) -> Vec<(String, i64)> {
    let Some(folder) = recent_folder() else { return Vec::new() };
    let _com = ComGuard::new();
    let mut seen = std::collections::HashSet::new();
    newest_shortcuts(&folder)
        .into_iter()
        .filter_map(|(shortcut, opened)| Some((shortcut_target(&shortcut.to_string_lossy())?, opened)))
        .filter(|(target, _)| is_offerable(target) && seen.insert(target.to_lowercase()))
        .take(limit)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_local_existing_files_are_offered() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("report.txt");
        std::fs::write(&file, "x").unwrap();

        assert!(is_offerable(&file.to_string_lossy()));
        assert!(!is_offerable(&dir.path().to_string_lossy()), "folders are not files");
        assert!(!is_offerable(&dir.path().join("gone.txt").to_string_lossy()));
        assert!(!is_offerable(r"\\server\share\file.txt"));
    }

    #[test]
    fn shortcuts_are_listed_newest_first_and_other_files_ignored() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.lnk"), "x").unwrap();
        std::fs::write(dir.path().join("notes.txt"), "x").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        std::fs::write(dir.path().join("b.LNK"), "x").unwrap();

        let names: Vec<String> = newest_shortcuts(dir.path())
            .into_iter()
            .map(|(path, _)| path.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert_eq!(names, vec!["b.LNK", "a.lnk"]);
        assert!(newest_shortcuts(&dir.path().join("missing")).is_empty());
    }

    #[test]
    fn reads_this_machines_recent_files_without_error() {
        for (path, opened) in recent_files(6) {
            assert!(Path::new(&path).is_file());
            assert!(opened > 0);
        }
    }
}
