//! Linux: apps are .desktop entries (Name / Exec / Icon) or programs/AppImages.
//! Hide/show works on X11 (and for X11 apps under XWayland); Wayland doesn't let one app
//! control another app's windows, so there it's a no-op. Quit asks the app to terminate.

use super::Running;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use sysinfo::{ProcessRefreshKind, RefreshKind, Signal, System, UpdateKind};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{AtomEnum, ClientMessageEvent, ConnectionExt, EventMask, Window};

pub const APP_EXTENSIONS: &[&str] = &["desktop", "AppImage"];
pub const DEFAULT_APP_DIR: Option<&str> = Some("/usr/share/applications");

// MARK: .desktop entries

struct DesktopEntry {
    name: Option<String>,
    exec: Vec<String>,
    icon: Option<String>,
}

fn parse_desktop(path: &str) -> Option<DesktopEntry> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut in_main = false;
    let (mut name, mut exec, mut icon) = (None, None, None);
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_main = line == "[Desktop Entry]";
            continue;
        }
        if !in_main {
            continue;
        }
        if let Some(v) = line.strip_prefix("Name=") { name.get_or_insert(v.to_string()); }
        if let Some(v) = line.strip_prefix("Exec=") { exec.get_or_insert(v.to_string()); }
        if let Some(v) = line.strip_prefix("Icon=") { icon.get_or_insert(v.to_string()); }
    }
    Some(DesktopEntry { name, exec: split_exec(&exec.unwrap_or_default()), icon })
}

/// Splits an Exec line (with quotes) and drops field codes like %U and `env VAR=x` prefixes.
fn split_exec(exec: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut cur = String::new();
    let (mut quoted, mut escaped) = (false, false);
    for c in exec.chars() {
        match c {
            _ if escaped => { cur.push(c); escaped = false; }
            '\\' => escaped = true,
            '"' => quoted = !quoted,
            ' ' if !quoted => { if !cur.is_empty() { args.push(std::mem::take(&mut cur)); } }
            _ => cur.push(c),
        }
    }
    if !cur.is_empty() { args.push(cur); }
    let mut args: Vec<String> = args.into_iter().filter(|a| !(a.len() == 2 && a.starts_with('%'))).collect();
    if args.first().map(|a| a == "env").unwrap_or(false) {
        args.remove(0);
        while args.first().map(|a| a.contains('=')).unwrap_or(false) { args.remove(0); }
    }
    args
}

fn which(program: &str) -> Option<PathBuf> {
    if program.contains('/') {
        return Some(PathBuf::from(program));
    }
    std::env::var("PATH").ok()?.split(':').map(|dir| Path::new(dir).join(program)).find(|p| p.is_file())
}

fn flatpak_app_id(exec: &[String]) -> Option<String> {
    let bin = exec.first()?;
    if !bin.ends_with("flatpak") { return None; }
    let run = exec.iter().position(|a| a == "run")?;
    exec[run + 1..].iter().find(|a| !a.starts_with('-')).cloned()
}

/// Program + base arguments that start this entry.
fn command_line(entry: &str) -> Vec<String> {
    if entry.ends_with(".desktop") {
        parse_desktop(entry).map(|d| d.exec).unwrap_or_default()
    } else {
        vec![entry.to_string()]
    }
}

/// A running app is identified by its resolved program path, or "flatpak:<app id>".
pub fn identity(entry: &str) -> String {
    let cmd = command_line(entry);
    if let Some(id) = flatpak_app_id(&cmd) {
        return format!("flatpak:{id}");
    }
    cmd.first()
        .and_then(|p| which(p))
        .map(|p| std::fs::canonicalize(&p).unwrap_or(p).to_string_lossy().to_string())
        .unwrap_or_else(|| entry.to_string())
}

pub fn app_name(entry: &str) -> String {
    if entry.ends_with(".desktop") {
        if let Some(name) = parse_desktop(entry).and_then(|d| d.name) {
            return name;
        }
    }
    super::file_stem(entry)
}

// MARK: Processes

fn processes() -> Vec<(u32, String)> {
    let sys = System::new_with_specifics(
        RefreshKind::nothing().with_processes(ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet)),
    );
    sys.processes()
        .iter()
        .filter_map(|(pid, p)| Some((pid.as_u32(), p.exe()?.to_string_lossy().to_string())))
        .collect()
}

fn running_flatpaks() -> Vec<String> {
    if which("flatpak").is_none() { return vec![]; }
    Command::new("flatpak")
        .args(["ps", "--columns=application"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).lines().map(|l| format!("flatpak:{}", l.trim())).collect())
        .unwrap_or_default()
}

pub fn running() -> Vec<Running> {
    let hidden_pids = x11_windows().map(|w| w.hidden_by_pid()).unwrap_or_default();
    let mut by_exe: HashMap<String, (bool, bool)> = HashMap::new(); // (has window, any visible)
    for (pid, exe) in processes() {
        let e = by_exe.entry(exe).or_insert((false, false));
        if let Some(hidden) = hidden_pids.get(&pid) {
            e.0 = true;
            if !hidden { e.1 = true; }
        }
    }
    let mut list: Vec<Running> =
        by_exe.into_iter().map(|(path, (win, vis))| Running { path, hidden: win && !vis }).collect();
    list.extend(running_flatpaks().into_iter().map(|path| Running { path, hidden: false }));
    list
}

fn pids_of(entry: &str) -> HashSet<u32> {
    let id = identity(entry);
    processes().into_iter().filter(|(_, exe)| *exe == id).map(|(pid, _)| pid).collect()
}

// MARK: X11 windows

struct X11 {
    conn: x11rb::rust_connection::RustConnection,
    root: Window,
    windows: Vec<(u32, Window, bool)>, // (pid, window, hidden)
}

fn atom(conn: &impl Connection, name: &[u8]) -> Option<u32> {
    Some(conn.intern_atom(false, name).ok()?.reply().ok()?.atom)
}

fn x11_windows() -> Option<X11> {
    std::env::var("DISPLAY").ok()?;
    let (conn, screen) = x11rb::connect(None).ok()?;
    let root = conn.setup().roots[screen].root;
    let client_list = atom(&conn, b"_NET_CLIENT_LIST")?;
    let wm_pid = atom(&conn, b"_NET_WM_PID")?;
    let wm_state = atom(&conn, b"_NET_WM_STATE")?;
    let state_hidden = atom(&conn, b"_NET_WM_STATE_HIDDEN")?;
    let clients: Vec<u32> = conn
        .get_property(false, root, client_list, AtomEnum::WINDOW, 0, u32::MAX).ok()?.reply().ok()?
        .value32()?.collect();
    let mut windows = Vec::new();
    for w in clients {
        let pid = conn.get_property(false, w, wm_pid, AtomEnum::CARDINAL, 0, 1).ok()
            .and_then(|c| c.reply().ok()).and_then(|r| r.value32().and_then(|mut v| v.next()));
        let hidden = conn.get_property(false, w, wm_state, AtomEnum::ATOM, 0, 64).ok()
            .and_then(|c| c.reply().ok())
            .and_then(|r| r.value32().map(|mut v| v.any(|a| a == state_hidden)))
            .unwrap_or(false);
        if let Some(pid) = pid {
            windows.push((pid, w, hidden));
        }
    }
    Some(X11 { conn, root, windows })
}

impl X11 {
    fn hidden_by_pid(&self) -> HashMap<u32, bool> {
        let mut map: HashMap<u32, bool> = HashMap::new();
        for (pid, _, hidden) in &self.windows {
            let e = map.entry(*pid).or_insert(true);
            *e = *e && *hidden;
        }
        map
    }

    fn windows_of(&self, pids: &HashSet<u32>) -> Vec<Window> {
        self.windows.iter().filter(|(p, _, _)| pids.contains(p)).map(|(_, w, _)| *w).collect()
    }

    fn send(&self, window: Window, message: &[u8], data: [u32; 5]) {
        let Some(kind) = atom(&self.conn, message) else { return };
        let event = ClientMessageEvent::new(32, window, kind, data);
        let _ = self.conn.send_event(false, self.root, EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY, event);
    }

    fn minimize(&self, window: Window) {
        self.send(window, b"WM_CHANGE_STATE", [3, 0, 0, 0, 0]); // IconicState
    }

    fn activate(&self, window: Window) {
        self.send(window, b"_NET_ACTIVE_WINDOW", [2, 0, 0, 0, 0]); // source: pager
    }

    fn flush(&self) {
        let _ = self.conn.flush();
    }
}

// MARK: Actions

pub fn open(entry: &str, args: &[String]) -> Result<(), String> {
    if args.is_empty() {
        let pids = pids_of(entry);
        if let Some(x) = x11_windows() {
            let windows = x.windows_of(&pids);
            if !windows.is_empty() {
                windows.iter().for_each(|w| x.activate(*w));
                x.flush();
                return Ok(());
            }
        }
        if entry.ends_with(".desktop") {
            let launched = Command::new("gio").args(["launch", entry]).spawn().is_ok();
            if launched { return Ok(()); }
            let id = super::file_stem(entry);
            return Command::new("gtk-launch").arg(id).spawn().map(|_| ()).map_err(|e| e.to_string());
        }
    }
    // Programs, and browsers with profile/URL arguments: run the Exec line plus the arguments.
    let mut cmd = command_line(entry);
    if cmd.is_empty() { return Err("No program to start".into()); }
    let program = cmd.remove(0);
    Command::new(program).args(cmd).args(args).spawn().map(|_| ()).map_err(|e| e.to_string())
}

pub fn hide(entry: &str) {
    let pids = pids_of(entry);
    if let Some(x) = x11_windows() {
        x.windows_of(&pids).iter().for_each(|w| x.minimize(*w));
        x.flush();
    }
}

pub fn unhide(entry: &str) {
    let pids = pids_of(entry);
    if let Some(x) = x11_windows() {
        x.windows_of(&pids).iter().for_each(|w| x.activate(*w));
        x.flush();
    }
}

pub fn quit(entry: &str) {
    let id = identity(entry);
    if let Some(app_id) = id.strip_prefix("flatpak:") {
        let _ = Command::new("flatpak").args(["kill", app_id]).spawn();
        return;
    }
    let sys = System::new_with_specifics(
        RefreshKind::nothing().with_processes(ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet)),
    );
    for p in sys.processes().values() {
        if p.exe().map(|e| e.to_string_lossy() == id).unwrap_or(false) {
            p.kill_with(Signal::Term);
        }
    }
}

// MARK: Icons

/// The icon named in the .desktop file, from the usual icon theme folders (PNG or SVG).
pub fn icon(entry: &str, _cache_dir: &Path) -> Option<(Vec<u8>, &'static str)> {
    let name = if entry.ends_with(".desktop") { parse_desktop(entry)?.icon? } else { return None };
    let candidates: Vec<PathBuf> = if name.starts_with('/') {
        vec![PathBuf::from(&name)]
    } else {
        let home = std::env::var("HOME").unwrap_or_default();
        let bases = [
            format!("{home}/.local/share/icons"),
            "/usr/share/icons".into(),
            format!("{home}/.local/share/flatpak/exports/share/icons"),
            "/var/lib/flatpak/exports/share/icons".into(),
        ];
        let mut list = Vec::new();
        for base in &bases {
            for size in ["256x256", "128x128", "96x96", "64x64", "48x48"] {
                list.push(PathBuf::from(format!("{base}/hicolor/{size}/apps/{name}.png")));
            }
            list.push(PathBuf::from(format!("{base}/hicolor/scalable/apps/{name}.svg")));
        }
        list.push(PathBuf::from(format!("/usr/share/pixmaps/{name}.png")));
        list.push(PathBuf::from(format!("/usr/share/pixmaps/{name}.svg")));
        list
    };
    let path = candidates.into_iter().find(|p| p.is_file())?;
    let mime = if path.extension().map(|e| e == "svg").unwrap_or(false) { "image/svg+xml" } else { "image/png" };
    Some((std::fs::read(path).ok()?, mime))
}
