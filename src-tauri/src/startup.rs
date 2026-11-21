//! Starting with Windows.
//!
//! The ordinary installer puts the app on the registry's startup list (through
//! tauri-plugin-autostart). The Microsoft Store version runs as an MSIX package,
//! which cannot use that list; it declares a startup task in its package
//! manifest instead, and the user switches that on and off from here or from
//! Settings > Apps > Startup.

use std::path::PathBuf;

use tauri::{AppHandle, Manager};
use tauri_plugin_autostart::ManagerExt;
use windows::core::HSTRING;
use windows::ApplicationModel::Activation::ActivationKind;
use windows::ApplicationModel::{AppInstance, Package, StartupTask, StartupTaskState};

/// The `TaskId` of the startup task in `packaging/AppxManifest.xml`.
pub const TASK_ID: &str = "OpenHeadphonesConnectStartup";

/// Whether this copy runs as an MSIX package (the Microsoft Store version).
pub fn packaged() -> bool {
    Package::Current().is_ok()
}

/// The packaged app's own data folder (`…\Packages\<package>\LocalState`).
///
/// A packaged app that uses the usual AppData folders gets a private,
/// redirected view of them: it cannot see files the user puts there, and what
/// it writes may land somewhere the user cannot find. Its own folder is a
/// real place that both sides see.
fn package_folder() -> Option<PathBuf> {
    let family = Package::Current().ok()?.Id().ok()?.FamilyName().ok()?;
    let local = std::env::var_os("LOCALAPPDATA")?;
    Some(PathBuf::from(local).join("Packages").join(family.to_string()).join("LocalState"))
}

/// Where settings and the user's own 3D models live.
pub fn config_dir(app: &AppHandle) -> Option<PathBuf> {
    package_folder().or_else(|| app.path().app_config_dir().ok())
}

/// Where the log is written.
pub fn log_dir(app: &AppHandle) -> Option<PathBuf> {
    package_folder().map(|dir| dir.join("logs")).or_else(|| app.path().app_log_dir().ok())
}

fn task() -> Result<StartupTask, String> {
    StartupTask::GetAsync(&HSTRING::from(TASK_ID))
        .and_then(|op| op.get())
        .map_err(|e| format!("Couldn't read the startup setting: {}", e.message()))
}

pub fn is_enabled(app: &AppHandle) -> bool {
    if !packaged() {
        return app.autolaunch().is_enabled().unwrap_or(false);
    }
    task()
        .and_then(|task| task.State().map_err(|e| e.message()))
        .is_ok_and(|state| state == StartupTaskState::Enabled || state == StartupTaskState::EnabledByPolicy)
}

pub fn set(app: &AppHandle, enabled: bool) -> Result<(), String> {
    if !packaged() {
        let launcher = app.autolaunch();
        let result = if enabled { launcher.enable() } else { launcher.disable() };
        return result.map_err(|e| format!("Couldn't change the startup setting: {e}"));
    }
    let task = task()?;
    if !enabled {
        return task.Disable().map_err(|e| format!("Couldn't change the startup setting: {}", e.message()));
    }
    let state = task
        .RequestEnableAsync()
        .and_then(|op| op.get())
        .map_err(|e| format!("Couldn't change the startup setting: {}", e.message()))?;
    match state {
        StartupTaskState::Enabled | StartupTaskState::EnabledByPolicy => Ok(()),
        StartupTaskState::DisabledByUser => Err(
            "Windows has starting this app turned off. Turn it on in Settings > Apps > Startup.".into(),
        ),
        StartupTaskState::DisabledByPolicy => {
            Err("Your organisation doesn't allow this app to start with Windows.".into())
        }
        _ => Err("Windows didn't let the app start with Windows.".into()),
    }
}

/// Whether Windows started the app at sign-in rather than the user starting
/// it: the ordinary installer passes `--autostart`, the packaged startup task
/// passes nothing but says so in how it activated the app.
pub fn launched_at_login(args: &[String]) -> bool {
    args.iter().any(|arg| arg == "--autostart")
        || (packaged()
            && AppInstance::GetActivatedEventArgs()
                .and_then(|activation| activation.Kind())
                .is_ok_and(|kind| kind == ActivationKind::StartupTask))
}
