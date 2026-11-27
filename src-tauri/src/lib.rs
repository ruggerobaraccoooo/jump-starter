//! Jump Starter (Tauri): tray panel + editor window, global shortcuts per group.

mod model;
mod platform;

use base64::Engine;
use model::{Group, Store};
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder, WindowEvent};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
enum Action {
    Open,
    Hide,
    Quit,
}

struct AppState {
    store: Mutex<Store>,
    bindings: Mutex<HashMap<Shortcut, (String, Action)>>,
    /// "groupId:action" for shortcuts that couldn't be registered (taken by another app).
    failed: Mutex<Vec<String>>,
}

const PANEL_WIDTH: f64 = 420.0; // 380 panel + room for its shadow

// MARK: - Group actions

fn group_paths(group: &Group) -> Vec<String> {
    group.apps.iter().map(|a| a.path.clone()).collect()
}

fn all_entry_paths(app: &AppHandle) -> Vec<String> {
    let state = app.state::<AppState>();
    let store = state.store.lock().unwrap();
    store.groups.iter().flat_map(group_paths).collect()
}

fn browser_family(path: &str) -> Option<&'static str> {
    let p = path.to_lowercase();
    if ["chrome", "edge", "brave", "vivaldi", "chromium"].iter().any(|k| p.contains(k)) {
        Some("chromium")
    } else if ["firefox", "zen"].iter().any(|k| p.contains(k)) {
        Some("firefox")
    } else {
        None
    }
}

fn open_group(group: &Group) {
    let running: Vec<String> = platform::running_entries(&group_paths(group)).into_iter().map(|r| r.path).collect();
    for app in &group.apps {
        let launches: Vec<_> = app.profiles.iter().filter(|l| l.profile.is_some() || !l.urls.is_empty()).collect();
        let family = browser_family(&app.path);
        if launches.is_empty() || family.is_none() {
            let _ = platform::open(&app.path, &[]);
            continue;
        }
        // One launch per profile; if the browser is still starting, later ones wait a moment.
        let was_running = running.iter().any(|r| r == &app.path);
        for (i, launch) in launches.into_iter().enumerate() {
            let mut args = Vec::new();
            if let Some(profile) = &launch.profile {
                if family == Some("chromium") {
                    args.push(format!("--profile-directory={profile}"));
                } else {
                    args.push("-P".into());
                    args.push(profile.clone());
                }
            }
            args.extend(launch.urls.iter().map(|u| if u.contains("://") { u.clone() } else { format!("https://{u}") }));
            let path = app.path.clone();
            let delay = std::time::Duration::from_millis(i as u64 * if was_running { 300 } else { 1500 });
            std::thread::spawn(move || {
                std::thread::sleep(delay);
                let _ = platform::open(&path, &args);
            });
        }
    }
}

/// Hide shortcut: hides the group's open apps; if they're all hidden, shows them again.
fn toggle_hidden(group: &Group) {
    let open = platform::running_entries(&group_paths(group));
    if open.iter().any(|r| !r.hidden) {
        open.iter().for_each(|r| platform::hide(&r.path));
    } else {
        open.iter().for_each(|r| platform::unhide(&r.path));
    }
}

fn hide_group(group: &Group) {
    group_paths(group).iter().for_each(|p| platform::hide(p));
}

fn quit_group(group: &Group) {
    group_paths(group).iter().for_each(|p| platform::quit(p));
}

fn with_group(app: &AppHandle, id: &str, f: impl FnOnce(&Group)) {
    let state = app.state::<AppState>();
    let group = state.store.lock().unwrap().group(id).cloned();
    if let Some(group) = group {
        f(&group);
    }
}

// MARK: - Shortcuts

fn register_shortcuts(app: &AppHandle) {
    let state = app.state::<AppState>();
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    let mut bindings = HashMap::new();
    let mut failed = Vec::new();
    for g in state.store.lock().unwrap().groups.iter() {
        for (accel, action) in [(&g.shortcuts.open, Action::Open), (&g.shortcuts.hide, Action::Hide), (&g.shortcuts.quit, Action::Quit)] {
            let Some(accel) = accel else { continue };
            let key = format!("{}:{}", g.id, serde_json::to_value(action).unwrap().as_str().unwrap());
            match accel.parse::<Shortcut>() {
                Ok(shortcut) if !bindings.contains_key(&shortcut) && gs.register(shortcut).is_ok() => {
                    bindings.insert(shortcut, (g.id.clone(), action));
                }
                _ => failed.push(key),
            }
        }
    }
    *state.bindings.lock().unwrap() = bindings;
    *state.failed.lock().unwrap() = failed;
}

fn on_shortcut(app: &AppHandle, shortcut: &Shortcut) {
    let binding = app.state::<AppState>().bindings.lock().unwrap().get(shortcut).cloned();
    if let Some((id, action)) = binding {
        with_group(app, &id, |g| match action {
            Action::Open => open_group(g),
            Action::Hide => toggle_hidden(g),
            Action::Quit => quit_group(g),
        });
    }
}

// MARK: - Panel and editor windows

fn toggle_panel(app: &AppHandle, anchor: Option<(f64, f64, f64, f64)>) {
    let Some(panel) = app.get_webview_window("panel") else { return };
    if panel.is_visible().unwrap_or(false) {
        let _ = panel.hide();
        return;
    }
    let scale = panel.scale_factor().unwrap_or(1.0);
    let width = PANEL_WIDTH * scale;
    // Tray icon rectangle (physical px); without one, use the top-right corner of the main monitor.
    let monitor = anchor
        .and_then(|(x, y, _, _)| app.monitor_from_point(x / scale, y / scale).ok().flatten())
        .or_else(|| panel.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else { return };
    let work = monitor.work_area();
    let (wx, wy) = (work.position.x as f64, work.position.y as f64);
    let (ww, wh) = (work.size.width as f64, work.size.height as f64);
    let (ix, iy, iw, ih) = anchor.unwrap_or((wx + ww - width, wy, width, 0.0));

    // Menu bar at the top (macOS, most Linux): panel hangs below the icon.
    // Taskbar at the bottom (Windows): panel sits above the icon.
    let at_bottom = iy > wy + wh / 2.0;
    let height = wh - 8.0 * scale;
    let x = (ix + iw / 2.0 - width / 2.0).clamp(wx, wx + ww - width);
    let y = if at_bottom { iy - height } else { wy + 2.0 * scale };
    let _ = panel.set_size(PhysicalSize::new(width, height));
    let _ = panel.set_position(PhysicalPosition::new(x, y));
    let _ = panel.emit("panel-shown", if at_bottom { "bottom" } else { "top" });
    let _ = panel.show();
    let _ = panel.set_focus();
    let _ = ih;
}

fn open_editor_window(app: &AppHandle, group_id: Option<String>) {
    if let Some(panel) = app.get_webview_window("panel") {
        let _ = panel.hide();
    }
    let window = match app.get_webview_window("editor") {
        Some(w) => w,
        None => {
            #[allow(unused_mut)]
            let mut builder = WebviewWindowBuilder::new(app, "editor", WebviewUrl::App("editor.html".into()))
                .title("Jump Starter")
                .inner_size(900.0, 640.0)
                .min_inner_size(720.0, 480.0)
                .center();
            #[cfg(target_os = "macos")]
            {
                builder = builder.title_bar_style(tauri::TitleBarStyle::Overlay).hidden_title(true);
            }
            match builder.build() {
                Ok(w) => w,
                Err(_) => return,
            }
        }
    };
    let _ = window.show();
    let _ = window.set_focus();
    if let Some(id) = group_id {
        let _ = window.emit("select-group", id);
    }
}

// MARK: - Commands called from the UI

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct PlatformInfo {
    os: &'static str,
    app_extensions: Vec<&'static str>,
    default_app_dir: Option<&'static str>,
}

#[tauri::command]
fn platform_info() -> PlatformInfo {
    PlatformInfo {
        os: std::env::consts::OS,
        app_extensions: platform::APP_EXTENSIONS.to_vec(),
        default_app_dir: platform::DEFAULT_APP_DIR,
    }
}

#[tauri::command]
fn get_groups(state: tauri::State<AppState>) -> Vec<Group> {
    state.store.lock().unwrap().groups.clone()
}

#[tauri::command]
fn save_groups(app: AppHandle, state: tauri::State<AppState>, groups: Vec<Group>) -> Vec<String> {
    {
        let mut store = state.store.lock().unwrap();
        store.groups = groups;
        store.save();
    }
    register_shortcuts(&app);
    let _ = app.emit("groups-changed", ());
    state.failed.lock().unwrap().clone()
}

#[tauri::command]
fn failed_shortcuts(state: tauri::State<AppState>) -> Vec<String> {
    state.failed.lock().unwrap().clone()
}

/// While the editor records a shortcut, free all global ones so the keys reach the editor.
#[tauri::command]
fn suspend_shortcuts(app: AppHandle) {
    let _ = app.global_shortcut().unregister_all();
}

#[tauri::command]
fn resume_shortcuts(app: AppHandle) {
    register_shortcuts(&app);
}

/// Which of the groups' apps are running (keyed by entry path).
#[tauri::command]
fn running_apps(app: AppHandle) -> Vec<platform::Running> {
    platform::running_entries(&all_entry_paths(&app))
}

#[tauri::command]
fn app_icon(app: AppHandle, path: String) -> Option<String> {
    let cache = app.path().app_cache_dir().ok()?.join("icons");
    let (bytes, mime) = platform::icon(&path, &cache)?;
    Some(format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)))
}

#[tauri::command]
fn new_app_entry(path: String) -> model::AppEntry {
    model::AppEntry { id: model::new_id(), name: platform::app_name(&path), path, profiles: vec![] }
}

#[tauri::command]
fn group_action(app: AppHandle, id: String, action: String) {
    with_group(&app, &id, |g| match action.as_str() {
        "open" => open_group(g),
        "hide" => hide_group(g),
        "toggleHidden" => toggle_hidden(g),
        "quit" => quit_group(g),
        _ => {}
    });
}

#[tauri::command]
fn app_action(path: String, action: String) {
    match action.as_str() {
        "open" => {
            let _ = platform::open(&path, &[]);
        }
        "hide" => platform::hide(&path),
        "quit" => platform::quit(&path),
        _ => {}
    }
}

#[tauri::command]
fn open_editor(app: AppHandle, group_id: Option<String>) {
    open_editor_window(&app, group_id);
}

#[tauri::command]
fn hide_panel(app: AppHandle) {
    if let Some(panel) = app.get_webview_window("panel") {
        let _ = panel.hide();
    }
}

#[tauri::command]
fn quit_jump_starter(app: AppHandle) {
    app.exit(0);
}

// MARK: - App

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        on_shortcut(app, shortcut);
                    }
                })
                .build(),
        )
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory); // menu bar only, no Dock icon

            let data_dir = app.path().app_data_dir()?;
            app.manage(AppState {
                store: Mutex::new(Store::load(data_dir)),
                bindings: Mutex::new(HashMap::new()),
                failed: Mutex::new(Vec::new()),
            });
            register_shortcuts(app.handle());

            let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray.png"))?;
            TrayIconBuilder::with_id("tray")
                .icon(icon)
                .icon_as_template(true)
                .tooltip("Jump Starter")
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, rect, .. } = event {
                        let app = tray.app_handle();
                        let scale = app.get_webview_window("panel").and_then(|w| w.scale_factor().ok()).unwrap_or(1.0);
                        let p = rect.position.to_physical::<f64>(scale);
                        let s = rect.size.to_physical::<f64>(scale);
                        toggle_panel(app, Some((p.x, p.y, s.width, s.height)));
                    }
                })
                .build(app)?;

            // The panel hides when it loses focus, like a menu.
            if let Some(panel) = app.get_webview_window("panel") {
                let handle = panel.clone();
                let keep_open = std::env::var("JUMP_KEEP_OPEN").is_ok();
                panel.on_window_event(move |event| {
                    if let WindowEvent::Focused(false) = event {
                        if !keep_open {
                            let _ = handle.hide();
                        }
                    }
                });
            }

            // Debug helpers for screenshots: JUMP_OPEN_PANEL=1 / JUMP_OPEN_EDITOR=1.
            if std::env::var("JUMP_OPEN_PANEL").is_ok() {
                toggle_panel(app.handle(), None);
            }
            // JUMP_DEMO_EXPAND=1: slow (10×) expand of the 3rd group after 2 s, collapse after 6 s.
            if std::env::var("JUMP_DEMO_EXPAND").is_ok() {
                if let Some(panel) = app.get_webview_window("panel") {
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_secs(2));
                        let _ = panel.eval("document.documentElement.style.setProperty('--dur','2400ms');document.querySelectorAll('.g-head')[2].click()");
                        std::thread::sleep(std::time::Duration::from_secs(4));
                        let _ = panel.eval("document.querySelectorAll('.g-head')[2].click()");
                    });
                }
            }
            if std::env::var("JUMP_OPEN_EDITOR").is_ok() {
                open_editor_window(app.handle(), None);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            platform_info,
            get_groups,
            save_groups,
            failed_shortcuts,
            suspend_shortcuts,
            resume_shortcuts,
            running_apps,
            app_icon,
            new_app_entry,
            group_action,
            app_action,
            open_editor,
            hide_panel,
            quit_jump_starter
        ])
        .run(tauri::generate_context!())
        .expect("error while running Jump Starter");
}
