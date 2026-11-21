//! Open Headphones Connect: a connect card, battery readout and noise
//! control for Sony headphones on Windows. Unofficial; not affiliated with
//! Sony.

pub mod bluetooth;
pub mod flyout;
pub mod logger;
pub mod protocol;
pub mod session;
pub mod settings;
pub mod tray;

use std::path::PathBuf;
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::image::Image;
use tauri::menu::MenuBuilder;
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow, WindowEvent};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};
use tauri_plugin_notification::NotificationExt;
use windows::UI::ViewManagement::{UIColorType, UISettings};

use protocol::{NoiseMode, NoiseState, Switch, Transport};
use session::{Command, Phase, Snapshot};
use settings::Settings;

const LOW_BATTERY: u8 = 20;
const TRAY_ID: &str = "main";
const APP_NAME: &str = "Open Headphones Connect";

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
enum Mode {
    /// The card that appears when the headphones connect.
    Card,
    /// The control panel opened from the tray.
    Panel,
}

#[derive(Serialize, Clone)]
struct ShowRequest {
    mode: Mode,
    nonce: u64,
    /// Play the card with sample values (the "Preview" button in settings).
    preview: bool,
}

struct Ui {
    mode: Mode,
    visible: bool,
    /// What was last asked for and not dismissed since, shown yet or not.
    wanted: Option<Mode>,
    wanted_at: Instant,
    /// The panel has held keyboard focus since it was shown. Only then does
    /// losing focus close it: a panel Windows refused to bring to the front
    /// must not vanish the moment it appears.
    focused: bool,
    nonce: u64,
    frontend_ready: bool,
    /// A request made before the page finished loading.
    queued: Option<ShowRequest>,
    hidden_at: Option<Instant>,
    motion: flyout::Motion,
}

struct Alerts {
    low_battery_sent: bool,
}

/// A panel opened this recently is the user's own doing (starting the app,
/// clicking the tray icon). A connection that completes in that moment does
/// not take the panel away from them; any later one shows the card over it.
const PANEL_GRACE: Duration = Duration::from_secs(4);

/// Whether a snapshot should bring up the connect card. `panel_opened` is how
/// long ago the panel was asked for, if it is the thing currently wanted.
fn card_wanted(new_connection: bool, enabled: bool, panel_opened: Option<Duration>) -> bool {
    new_connection && enabled && !panel_opened.is_some_and(|ago| ago < PANEL_GRACE)
}

struct AppState {
    snapshot: Mutex<Snapshot>,
    settings: Mutex<Settings>,
    settings_path: PathBuf,
    ui: Mutex<Ui>,
    alerts: Mutex<Alerts>,
    tray_shown: Mutex<Option<(tray::Icon, bool, u32)>>,
}

fn main_window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window("main")
}

// ----- showing and hiding the flyout ---------------------------------------

/// Asks the page to lay out `mode`; it answers with `present` once it knows
/// how tall it needs to be.
fn request_show(app: &AppHandle, mode: Mode, preview: bool) {
    let state = app.state::<AppState>();
    let mut ui = state.ui.lock().unwrap();
    ui.nonce += 1;
    ui.wanted = Some(mode);
    ui.wanted_at = Instant::now();
    let request = ShowRequest { mode, nonce: ui.nonce, preview };
    if ui.frontend_ready {
        let _ = app.emit("show", &request);
    } else {
        ui.queued = Some(request);
    }
}

fn hide_flyout(app: &AppHandle) {
    let state = app.state::<AppState>();
    let mut ui = state.ui.lock().unwrap();
    ui.wanted = None;
    ui.queued = None;
    ui.focused = false;
    // Trust the screen over our own bookkeeping: a window that is somehow
    // still showing must never be left behind.
    let window = main_window(app);
    let on_screen = window.as_ref().is_some_and(flyout::is_on_screen);
    if !ui.visible && !on_screen {
        return;
    }
    crate::log!("Hiding {:?}", ui.mode);
    ui.visible = false;
    ui.nonce += 1; // invalidates a `present` that is still on its way
    ui.hidden_at = Some(Instant::now());
    if let Some(window) = window {
        flyout::hide(&window, &ui.motion);
    }
    let _ = app.emit("hidden", ());
}

fn toggle_panel(app: &AppHandle) {
    let state = app.state::<AppState>();
    let (open, just_closed) = {
        let ui = state.ui.lock().unwrap();
        (
            ui.visible && ui.mode == Mode::Panel,
            // Clicking the tray icon first takes focus away, which closes the
            // panel; without this the same click would reopen it.
            ui.hidden_at.is_some_and(|at| at.elapsed() < Duration::from_millis(300)),
        )
    };
    if open {
        hide_flyout(app);
    } else if !just_closed {
        request_show(app, Mode::Panel, false);
    }
}

// ----- reacting to the headset ---------------------------------------------

fn on_snapshot(app: &AppHandle, snapshot: &Snapshot) {
    let state = app.state::<AppState>();
    let previous = std::mem::replace(&mut *state.snapshot.lock().unwrap(), snapshot.clone());
    let _ = app.emit("state", snapshot);
    update_tray(app, snapshot);

    let settings = state.settings.lock().unwrap().clone();
    let mut alerts = state.alerts.lock().unwrap();
    match snapshot.phase {
        Phase::Disconnected | Phase::NoDevice | Phase::BluetoothOff => {
            alerts.low_battery_sent = false;
        }
        Phase::Connected => {
            // The session counts fresh connections; the card follows that
            // count, so it appears every time the headset is switched on.
            let new_connection = snapshot.connections != previous.connections;
            let panel_opened = {
                let ui = state.ui.lock().unwrap();
                (ui.wanted == Some(Mode::Panel)).then(|| ui.wanted_at.elapsed())
            };
            if card_wanted(new_connection, settings.show_connect_card, panel_opened) {
                request_show(app, Mode::Card, false);
            }
            if let Some(battery) = snapshot.battery.headline() {
                if battery.level > LOW_BATTERY + 5 || battery.charging {
                    alerts.low_battery_sent = false;
                } else if battery.level <= LOW_BATTERY && !alerts.low_battery_sent {
                    alerts.low_battery_sent = true;
                    if settings.low_battery_alert {
                        let name = snapshot.device_name.as_deref().unwrap_or("Headphones");
                        let _ = app
                            .notification()
                            .builder()
                            .title(format!("{name} battery is at {}%", battery.level))
                            .body("Plug them in soon to keep listening.")
                            .show();
                    }
                }
            }
        }
        Phase::Starting | Phase::Connecting => {}
    }
}

fn noise_label(mode: NoiseMode) -> &'static str {
    match mode {
        NoiseMode::NoiseCancelling => "Noise cancelling",
        NoiseMode::Wind => "Wind noise reduction",
        NoiseMode::Ambient => "Ambient sound",
        NoiseMode::Off => "Noise control off",
    }
}

fn tooltip(snapshot: &Snapshot) -> String {
    let name = snapshot.device_name.as_deref().unwrap_or("Sony headphones");
    match snapshot.phase {
        Phase::Starting => APP_NAME.into(),
        Phase::BluetoothOff => format!("{APP_NAME}\nBluetooth is off"),
        Phase::NoDevice => format!("{APP_NAME}\nNo Sony headphones paired"),
        Phase::Disconnected => format!("{name}\nNot connected"),
        Phase::Connecting => format!("{name}\nConnecting…"),
        Phase::Connected => {
            let mut lines = vec![name.to_string()];
            if let Some(battery) = snapshot.battery.headline() {
                let charging = if battery.charging { ", charging" } else { "" };
                lines.push(format!("Battery {}%{charging}", battery.level));
            }
            if let Some(noise) = snapshot.noise {
                lines.push(noise_label(noise.mode).into());
            }
            lines.join("\n")
        }
    }
}

fn update_tray(app: &AppHandle, snapshot: &Snapshot) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else { return };
    let icon = match (snapshot.phase, snapshot.battery.headline()) {
        (Phase::Connected, Some(b)) => tray::Icon::Battery { level: b.level, charging: b.charging },
        (Phase::Connected, None) => tray::Icon::Connected,
        _ => tray::Icon::Idle,
    };
    let key = (icon, tray::taskbar_is_light(), tray::icon_size());
    let state = app.state::<AppState>();
    let mut shown = state.tray_shown.lock().unwrap();
    if *shown != Some(key) {
        let rendered = tray::render(key.0, key.2, key.1);
        let _ = tray.set_icon(Some(Image::new_owned(rendered.rgba, rendered.size, rendered.size)));
        *shown = Some(key);
    }
    let _ = tray.set_tooltip(Some(tooltip(snapshot)));
}

// ----- commands the page can call ------------------------------------------

#[tauri::command]
fn get_state(state: State<AppState>) -> Snapshot {
    state.snapshot.lock().unwrap().clone()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SettingsView {
    #[serde(flatten)]
    settings: Settings,
    start_with_windows: bool,
    version: &'static str,
}

#[tauri::command]
fn get_settings(app: AppHandle, state: State<AppState>) -> SettingsView {
    SettingsView {
        settings: state.settings.lock().unwrap().clone(),
        start_with_windows: app.autolaunch().is_enabled().unwrap_or(false),
        version: env!("CARGO_PKG_VERSION"),
    }
}

#[tauri::command]
fn update_settings(state: State<AppState>, settings: Settings) {
    settings.save(&state.settings_path);
    *state.settings.lock().unwrap() = settings;
}

#[tauri::command]
fn set_start_with_windows(app: AppHandle, enabled: bool) -> Result<(), String> {
    let launcher = app.autolaunch();
    let result = if enabled { launcher.enable() } else { launcher.disable() };
    result.map_err(|e| format!("Couldn't change the startup setting: {e}"))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Accent {
    /// The user's Windows accent colour, plus the lighter and darker steps
    /// Fluent uses on dark and light surfaces.
    base: String,
    on_dark: String,
    on_light: String,
}

#[tauri::command]
fn get_accent() -> Option<Accent> {
    let ui = UISettings::new().ok()?;
    let hex = |kind: UIColorType| {
        ui.GetColorValue(kind).ok().map(|c| format!("#{:02x}{:02x}{:02x}", c.R, c.G, c.B))
    };
    Some(Accent {
        base: hex(UIColorType::Accent)?,
        on_dark: hex(UIColorType::AccentLight2)?,
        on_light: hex(UIColorType::AccentDark1)?,
    })
}

#[tauri::command]
fn set_noise(session: State<session::Handle>, mode: NoiseMode, ambient_level: u8, focus_on_voice: bool) {
    session.send(Command::SetNoise(NoiseState { mode, ambient_level, focus_on_voice }));
}

#[tauri::command]
fn set_eq_preset(session: State<session::Handle>, preset: u8) {
    session.send(Command::SetEqPreset(preset));
}

#[tauri::command]
fn set_eq_bands(session: State<session::Handle>, clear_bass: Option<i8>, bands: Vec<i8>) {
    session.send(Command::SetEqBands { clear_bass, bands });
}

#[tauri::command]
fn set_volume(session: State<session::Handle>, volume: u8) {
    session.send(Command::SetVolume(volume));
}

#[tauri::command]
fn transport(session: State<session::Handle>, control: Transport) {
    session.send(Command::Transport(control));
}

/// Flips one of the headset's on/off settings.
#[tauri::command]
fn set_switch(session: State<session::Handle>, id: Switch, on: bool) {
    session.send(Command::SetSwitch(id, on));
}

#[tauri::command]
fn set_auto_power_off(session: State<session::Handle>, choice: u8) {
    session.send(Command::SetAutoPowerOff(choice));
}

#[tauri::command]
fn connect(session: State<session::Handle>) {
    session.send(Command::Connect);
}

#[tauri::command]
fn disconnect(session: State<session::Handle>) {
    session.send(Command::Disconnect);
}

#[tauri::command]
fn power_off(session: State<session::Handle>) {
    session.send(Command::PowerOff);
}

#[tauri::command]
fn open_bluetooth_settings() {
    let _ = std::process::Command::new("explorer.exe").arg("ms-settings:bluetooth").spawn();
}

/// Files a user may place in the app's settings folder to replace the built-in
/// headphones on the connect card with a 3D model of their own.
const MODEL_FILE: &str = "model.glb";
const MODEL_SETTINGS_FILE: &str = "model.json";
/// Holds models for particular headsets, each named after its headset.
const MODELS_DIR: &str = "models";

/// The user's own 3D model, as raw bytes. Empty when there is none.
#[tauri::command]
fn custom_model(app: AppHandle, name: Option<String>) -> tauri::ipc::Response {
    let bytes = app
        .path()
        .app_config_dir()
        .ok()
        .and_then(|dir| model_path(&dir, name.as_deref()))
        .and_then(|path| std::fs::read(path).ok())
        .unwrap_or_default();
    tauri::ipc::Response::new(bytes)
}

/// One of the user's own 3D models: `name` is the headset it is for, or
/// `None` for the general one, and `settings` its optional placement notes.
#[derive(Serialize, PartialEq, Debug)]
struct ModelInfo {
    name: Option<String>,
    settings: Option<String>,
}

/// Which of the user's own models to show for `headset`, if any.
#[tauri::command]
fn custom_model_info(app: AppHandle, headset: Option<String>) -> Option<ModelInfo> {
    find_model(&app.path().app_config_dir().ok()?, headset.as_deref())
}

/// A headset's model name as a file name: letters, digits, spaces, hyphens
/// and underscores only, so that it cannot point outside the models folder.
fn model_file_stem(headset: &str) -> Option<String> {
    let stem: String = headset
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '-' | '_'))
        .collect();
    let stem = stem.trim();
    (!stem.is_empty() && stem.len() <= 64).then(|| stem.to_string())
}

/// Where a model lives: `models\<headset>.glb`, or `model.glb` for the
/// general one.
fn model_path(dir: &std::path::Path, name: Option<&str>) -> Option<PathBuf> {
    match name {
        Some(name) => Some(dir.join(MODELS_DIR).join(format!("{}.glb", model_file_stem(name)?))),
        None => Some(dir.join(MODEL_FILE)),
    }
}

/// A model named after the connected headset wins over the general one.
fn find_model(dir: &std::path::Path, headset: Option<&str>) -> Option<ModelInfo> {
    if let Some(stem) = headset.and_then(model_file_stem) {
        let models = dir.join(MODELS_DIR);
        if models.join(format!("{stem}.glb")).is_file() {
            let settings = std::fs::read_to_string(models.join(format!("{stem}.json"))).ok();
            return Some(ModelInfo { name: Some(stem), settings });
        }
    }
    dir.join(MODEL_FILE).is_file().then(|| ModelInfo {
        name: None,
        settings: std::fs::read_to_string(dir.join(MODEL_SETTINGS_FILE)).ok(),
    })
}

#[tauri::command]
fn log_note(message: String) {
    crate::log!("{message}");
}

#[tauri::command]
fn report_error(message: String) {
    crate::log!("Page error: {message}");
}

/// The page finished loading and is listening for `show`.
#[tauri::command]
fn frontend_ready(app: AppHandle, state: State<AppState>) {
    let queued = {
        let mut ui = state.ui.lock().unwrap();
        ui.frontend_ready = true;
        ui.queued.take()
    };
    if let Some(request) = queued {
        let _ = app.emit("show", &request);
    }
}

/// The page has laid out the requested mode and reports its height.
#[tauri::command]
fn present(app: AppHandle, state: State<AppState>, mode: Mode, nonce: u64, height: f64) -> bool {
    let Some(window) = main_window(&app) else { return false };
    let mut ui = state.ui.lock().unwrap();
    if nonce != ui.nonce {
        return false;
    }
    let was_visible = ui.visible;
    ui.mode = mode;
    ui.visible = true;
    crate::log!("Showing {mode:?}, {height} px tall");
    flyout::place(
        &window,
        &ui.motion,
        flyout::Placement { height, slide: !was_visible, activate: mode == Mode::Panel },
    );
    true
}

/// The content changed height while showing (a section expanded, say).
#[tauri::command]
fn resize(app: AppHandle, state: State<AppState>, height: f64) {
    let Some(window) = main_window(&app) else { return };
    let ui = state.ui.lock().unwrap();
    if ui.visible {
        flyout::place(&window, &ui.motion, flyout::Placement { height, slide: false, activate: false });
    }
}

#[tauri::command]
fn dismiss(app: AppHandle) {
    hide_flyout(&app);
}

#[tauri::command]
fn open_panel(app: AppHandle) {
    request_show(&app, Mode::Panel, false);
}

#[tauri::command]
fn preview_card(app: AppHandle) {
    request_show(&app, Mode::Card, true);
}

#[tauri::command]
fn quit(app: AppHandle) {
    app.exit(0);
}

// ----- startup ---------------------------------------------------------------

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let menu = MenuBuilder::new(app).text("open", "Open").separator().text("quit", "Quit").build()?;
    let first = tray::render(tray::Icon::Idle, tray::icon_size(), tray::taskbar_is_light());
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(Image::new_owned(first.rgba, first.size, first.size))
        .tooltip(APP_NAME)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => request_show(app, Mode::Panel, false),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle_panel(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

pub fn run() {
    let launched_at_login = std::env::args().any(|arg| arg == "--autostart");

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // A second launch just opens the panel of the running copy.
            request_show(app, Mode::Panel, false);
        }))
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, Some(vec!["--autostart"])))
        .plugin(tauri_plugin_notification::init())
        .invoke_handler(tauri::generate_handler![
            get_state,
            get_settings,
            update_settings,
            set_start_with_windows,
            get_accent,
            set_noise,
            set_eq_preset,
            set_eq_bands,
            set_volume,
            transport,
            set_switch,
            set_auto_power_off,
            connect,
            disconnect,
            power_off,
            open_bluetooth_settings,
            report_error,
            log_note,
            custom_model,
            custom_model_info,
            frontend_ready,
            present,
            resize,
            dismiss,
            open_panel,
            preview_card,
            quit,
        ])
        .setup(move |app| {
            let handle = app.handle().clone();
            if let Ok(dir) = app.path().app_log_dir() {
                logger::init(&dir);
            }
            crate::log!("{APP_NAME} {} starting", env!("CARGO_PKG_VERSION"));

            let settings_path = app
                .path()
                .app_config_dir()
                .map(|dir| dir.join("settings.json"))
                .unwrap_or_else(|_| PathBuf::from("settings.json"));
            app.manage(AppState {
                snapshot: Mutex::new(Snapshot::default()),
                settings: Mutex::new(Settings::load(&settings_path)),
                settings_path,
                ui: Mutex::new(Ui {
                    mode: Mode::Panel,
                    visible: false,
                    wanted: None,
                    wanted_at: Instant::now(),
                    focused: false,
                    nonce: 0,
                    frontend_ready: false,
                    queued: None,
                    hidden_at: None,
                    motion: flyout::Motion::default(),
                }),
                alerts: Mutex::new(Alerts { low_battery_sent: false }),
                tray_shown: Mutex::new(None),
            });

            build_tray(&handle)?;

            let for_session = handle.clone();
            app.manage(session::spawn(move |snapshot| on_snapshot(&for_session, snapshot)));

            if !launched_at_login {
                request_show(&handle, Mode::Panel, false);
            }
            Ok(())
        })
        .on_window_event(|window, event| match event {
            WindowEvent::Focused(true) => {
                window.state::<AppState>().ui.lock().unwrap().focused = true;
            }
            WindowEvent::Focused(false) => {
                if !window.state::<AppState>().ui.lock().unwrap().focused {
                    return;
                }
                // Focus moves around inside the window too (into the web view,
                // onto a drop-down), so look at who actually has the
                // foreground a moment later before closing the panel.
                let app = window.app_handle().clone();
                thread::spawn(move || {
                    thread::sleep(Duration::from_millis(120));
                    let state = app.state::<AppState>();
                    let panel_open = {
                        let ui = state.ui.lock().unwrap();
                        ui.visible && ui.mode == Mode::Panel
                    };
                    let still_ours = main_window(&app).is_some_and(|w| flyout::has_foreground(&w));
                    if panel_open && !still_ours {
                        hide_flyout(&app);
                    }
                });
            }
            WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                hide_flyout(window.app_handle());
            }
            _ => {}
        })
        .build(tauri::generate_context!())
        .expect("failed to start the app")
        .run(|_app, _event| {});
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::Battery;
    use crate::session::Batteries;

    #[test]
    fn a_model_named_after_the_headset_wins_over_the_general_one() {
        let dir = std::env::temp_dir().join(format!("ohc-models-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("models")).unwrap();
        assert_eq!(find_model(&dir, Some("WH-1000XM5")), None);

        std::fs::write(dir.join("model.glb"), b"general").unwrap();
        std::fs::write(dir.join("model.json"), "{}").unwrap();
        let general = Some(ModelInfo { name: None, settings: Some("{}".into()) });
        assert_eq!(find_model(&dir, Some("WH-1000XM5")), general);
        assert_eq!(find_model(&dir, None), general);

        std::fs::write(dir.join("models").join("WH-1000XM5.glb"), b"xm5").unwrap();
        assert_eq!(
            find_model(&dir, Some("WH-1000XM5")),
            Some(ModelInfo { name: Some("WH-1000XM5".into()), settings: None })
        );
        // Other headsets still get the general one.
        assert_eq!(find_model(&dir, Some("WH-1000XM3")), general);
        assert_eq!(model_path(&dir, Some("WH-1000XM5")), Some(dir.join("models").join("WH-1000XM5.glb")));
        assert_eq!(model_path(&dir, None), Some(dir.join("model.glb")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_headset_name_cannot_reach_outside_the_models_folder() {
        assert_eq!(model_file_stem("WH-1000XM5").as_deref(), Some("WH-1000XM5"));
        assert_eq!(model_file_stem(" LinkBuds Clip ").as_deref(), Some("LinkBuds Clip"));
        assert_eq!(model_file_stem("..\\..\\secrets").as_deref(), Some("secrets"));
        assert_eq!(model_file_stem("../../x/y.glb").as_deref(), Some("xyglb"));
        assert_eq!(model_file_stem("///"), None);
        assert_eq!(model_file_stem(""), None);
    }

    #[test]
    fn tooltip_summarises_a_connected_headset() {
        let snapshot = Snapshot {
            phase: Phase::Connected,
            device_name: Some("WH-1000XM3".into()),
            battery: Batteries { single: Some(Battery { level: 80, charging: false }), ..Default::default() },
            noise: Some(NoiseState { mode: NoiseMode::NoiseCancelling, ambient_level: 10, focus_on_voice: false }),
            ..Snapshot::default()
        };
        assert_eq!(tooltip(&snapshot), "WH-1000XM3\nBattery 80%\nNoise cancelling");
        let off = Snapshot { phase: Phase::Disconnected, device_name: Some("WH-1000XM3".into()), ..Snapshot::default() };
        assert_eq!(tooltip(&off), "WH-1000XM3\nNot connected");
    }

    #[test]
    fn the_card_follows_every_new_connection() {
        // nothing else on screen
        assert!(card_wanted(true, true, None));
        // a panel that has been open for a while gives way to the card
        assert!(card_wanted(true, true, Some(Duration::from_secs(30))));
        // a panel the user has only just opened is left alone
        assert!(!card_wanted(true, true, Some(Duration::from_secs(1))));
        // no new connection, or the card switched off in settings
        assert!(!card_wanted(false, true, None));
        assert!(!card_wanted(true, false, None));
    }
}
