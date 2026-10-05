//! What differs when Matchstick runs as a Microsoft Store package.
//!
//! The same program is shipped two ways: as a classic installer, and as an
//! MSIX package in the Store. Inside a package Windows changes two things we
//! rely on: registry writes are private to the app (so the usual "run at
//! login" entry is never seen by Windows), and updates are the Store's job.

use windows::core::{HSTRING, PWSTR};
use windows::ApplicationModel::Activation::ActivationKind;
use windows::ApplicationModel::{AppInstance, StartupTask, StartupTaskState};
use windows::Win32::Foundation::APPMODEL_ERROR_NO_PACKAGE;
use windows::Win32::Storage::Packaging::Appx::GetCurrentPackageFullName;

/// Id of the startup task declared in the package manifest (packaging/AppxManifest.xml).
const STARTUP_TASK_ID: &str = "MatchstickStartup";

/// Whether this process runs inside a Store (MSIX) package.
pub fn is_packaged() -> bool {
    let mut length = 0u32;
    // Asking for the name with no buffer: an unpackaged process gets
    // "no package", a packaged one gets "buffer too small".
    unsafe { GetCurrentPackageFullName(&mut length, PWSTR::null()) != APPMODEL_ERROR_NO_PACKAGE }
}

/// Run a blocking Windows Runtime call off the UI thread, where waiting on
/// an asynchronous operation is not allowed.
fn on_worker<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    std::thread::spawn(work).join().ok()
}

fn startup_task() -> windows::core::Result<StartupTask> {
    StartupTask::GetAsync(&HSTRING::from(STARTUP_TASK_ID))?.get()
}

fn is_enabled(state: StartupTaskState) -> bool {
    state == StartupTaskState::Enabled || state == StartupTaskState::EnabledByPolicy
}

/// Whether Windows will start the app at login.
pub fn starts_at_login() -> bool {
    on_worker(|| startup_task().and_then(|task| task.State()).map(is_enabled).unwrap_or(false)).unwrap_or(false)
}

/// Turn starting at login on or off.
pub fn set_starts_at_login(enabled: bool) -> Result<(), String> {
    on_worker(move || -> Result<(), String> {
        let task = startup_task().map_err(|e| format!("Startup setting unavailable: {}", e))?;
        if !enabled {
            return task.Disable().map_err(|e| format!("Could not turn off startup: {}", e));
        }
        let state = task
            .RequestEnableAsync()
            .and_then(|request| request.get())
            .map_err(|e| format!("Could not turn on startup: {}", e))?;
        if is_enabled(state) {
            Ok(())
        } else {
            // The user (or their organisation) switched it off in Windows; only they can undo that
            Err("Windows has startup turned off for Matchstick. Turn it on in Settings > Apps > Startup.".to_string())
        }
    })
    .unwrap_or_else(|| Err("Startup setting unavailable".to_string()))
}

/// Whether this launch was Windows starting the app at login (as opposed to
/// the user opening it), in which case it should stay in the tray.
pub fn launched_at_login() -> bool {
    on_worker(|| {
        AppInstance::GetActivatedEventArgs()
            .and_then(|args| args.Kind())
            .map(|kind| kind == ActivationKind::StartupTask)
            .unwrap_or(false)
    })
    .unwrap_or(false)
}

/// The four-part version a Store package needs for app version `major.minor.patch`.
///
/// The Store does not accept versions that start with 0 and reserves the last
/// part for itself, so 0.2.2 becomes 1.2.2.0 and 1.4.0 becomes 2.4.0.0.
/// (The packaging script applies the same rule; this is here to test it.)
#[cfg(test)]
fn package_version(app_version: &str) -> Option<String> {
    let parts: Vec<u32> = app_version.split('.').map(|p| p.parse().ok()).collect::<Option<_>>()?;
    let [major, minor, patch] = parts[..] else { return None };
    Some(format!("{}.{}.{}.0", major + 1, minor, patch))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tests_do_not_run_inside_a_package() {
        assert!(!is_packaged());
        // Outside a package these must simply say "no", not fail
        assert!(!starts_at_login());
        assert!(!launched_at_login());
    }

    #[test]
    fn package_versions_never_start_with_zero() {
        assert_eq!(package_version("0.2.2"), Some("1.2.2.0".to_string()));
        assert_eq!(package_version("1.4.0"), Some("2.4.0.0".to_string()));
        assert_eq!(package_version("nonsense"), None);
    }
}
