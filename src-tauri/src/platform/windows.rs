//! Windows: apps are .exe files or Start menu shortcuts (.lnk, resolved to their target).
//! Hide = minimize the app's windows, show = restore them, quit = ask each window to close
//! (WM_CLOSE, like clicking ✕, so apps can still ask to save).

use super::Running;
use std::collections::{HashMap, HashSet};
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::Command;
use std::sync::Mutex;
use sysinfo::{ProcessRefreshKind, RefreshKind, System, UpdateKind};
use windows::core::{Interface, HSTRING, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
};
use windows::Win32::Storage::FileSystem::WIN32_FIND_DATAW;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, IPersistFile, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, STGM_READ,
};
use windows::Win32::UI::Shell::{IShellLinkW, ShellExecuteW, ShellLink};
use windows::Win32::UI::WindowsAndMessaging::{
    DestroyIcon, EnumWindows, GetIconInfo, GetWindow, GetWindowLongW, GetWindowThreadProcessId, IsIconic,
    IsWindowVisible, PostMessageW, PrivateExtractIconsW, SetForegroundWindow, ShowWindow, GWL_EXSTYLE, GW_OWNER,
    HICON, ICONINFO, SW_MINIMIZE, SW_RESTORE, SW_SHOWNORMAL, WM_CLOSE, WS_EX_TOOLWINDOW,
};

pub const APP_EXTENSIONS: &[&str] = &["exe", "lnk"];
pub const DEFAULT_APP_DIR: Option<&str> = Some(r"C:\ProgramData\Microsoft\Windows\Start Menu\Programs");

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

// MARK: Identity (.lnk → .exe)

static LNK_CACHE: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);

/// A running app is identified by its lower-cased .exe path.
pub fn identity(entry: &str) -> String {
    executable(entry).to_lowercase()
}

fn executable(entry: &str) -> String {
    if !entry.to_lowercase().ends_with(".lnk") {
        return entry.to_string();
    }
    let mut cache = LNK_CACHE.lock().unwrap();
    let cache = cache.get_or_insert_with(HashMap::new);
    if let Some(target) = cache.get(entry) {
        return target.clone();
    }
    let target = resolve_lnk(entry).unwrap_or_else(|| entry.to_string());
    cache.insert(entry.to_string(), target.clone());
    target
}

fn resolve_lnk(path: &str) -> Option<String> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).ok()?;
        let file: IPersistFile = link.cast().ok()?;
        file.Load(&HSTRING::from(path), STGM_READ).ok()?;
        let mut buf = [0u16; 260];
        let mut data = WIN32_FIND_DATAW::default();
        link.GetPath(&mut buf, &mut data, 0).ok()?;
        let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        let target = String::from_utf16_lossy(&buf[..len]);
        (!target.is_empty()).then_some(target)
    }
}

pub fn app_name(entry: &str) -> String {
    super::file_stem(entry)
}

// MARK: Processes and windows

fn processes() -> Vec<(u32, String)> {
    let sys = System::new_with_specifics(
        RefreshKind::nothing().with_processes(ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet)),
    );
    sys.processes()
        .iter()
        .filter_map(|(pid, p)| Some((pid.as_u32(), p.exe()?.to_string_lossy().to_lowercase())))
        .collect()
}

/// Visible top-level windows (no owner, not tool windows) per process id.
fn top_level_windows() -> Vec<(u32, HWND)> {
    unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> windows::core::BOOL {
        unsafe {
            let out = &mut *(lparam.0 as *mut Vec<(u32, HWND)>);
            let owned = GetWindow(hwnd, GW_OWNER).map(|o| !o.is_invalid()).unwrap_or(false);
            let tool = (GetWindowLongW(hwnd, GWL_EXSTYLE) as u32) & WS_EX_TOOLWINDOW.0 != 0;
            if IsWindowVisible(hwnd).as_bool() && !owned && !tool {
                let mut pid = 0u32;
                GetWindowThreadProcessId(hwnd, Some(&mut pid));
                out.push((pid, hwnd));
            }
        }
        true.into()
    }
    let mut out: Vec<(u32, HWND)> = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(collect), LPARAM(&mut out as *mut _ as isize));
    }
    out
}

fn windows_of(entry: &str) -> Vec<HWND> {
    let exe = identity(entry);
    let pids: HashSet<u32> = processes().into_iter().filter(|(_, e)| *e == exe).map(|(pid, _)| pid).collect();
    top_level_windows().into_iter().filter(|(pid, _)| pids.contains(pid)).map(|(_, h)| h).collect()
}

/// Running apps by .exe; "hidden" when all of an app's windows are minimized.
pub fn running() -> Vec<Running> {
    let windows = top_level_windows();
    let mut by_exe: HashMap<String, (bool, bool)> = HashMap::new(); // (has window, any visible)
    for (pid, exe) in processes() {
        let entry = by_exe.entry(exe).or_insert((false, false));
        for (_, hwnd) in windows.iter().filter(|(p, _)| *p == pid) {
            entry.0 = true;
            if !unsafe { IsIconic(*hwnd) }.as_bool() {
                entry.1 = true;
            }
        }
    }
    by_exe
        .into_iter()
        .map(|(path, (has_window, visible))| Running { path, hidden: has_window && !visible })
        .collect()
}

// MARK: Actions

/// Brings a running app's windows to the front, or launches it (with arguments for browsers).
pub fn open(entry: &str, args: &[String]) -> Result<(), String> {
    if args.is_empty() {
        let windows = windows_of(entry);
        if !windows.is_empty() {
            unhide_windows(&windows);
            return Ok(());
        }
    }
    let params = args.iter().map(|a| quote(a)).collect::<Vec<_>>().join(" ");
    let target = if args.is_empty() { entry.to_string() } else { executable(entry) };
    let result = unsafe {
        ShellExecuteW(
            None,
            &HSTRING::from("open"),
            &HSTRING::from(target.as_str()),
            if params.is_empty() { PCWSTR::null() } else { PCWSTR(HSTRING::from(params.as_str()).as_ptr()) },
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    if result.0 as isize > 32 {
        Ok(())
    } else {
        // Fallback, e.g. for shortcuts ShellExecute can't open.
        Command::new("cmd")
            .args(["/C", "start", "", &target])
            .args(args)
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

fn quote(arg: &str) -> String {
    if arg.contains(' ') { format!("\"{arg}\"") } else { arg.to_string() }
}

pub fn hide(entry: &str) {
    for hwnd in windows_of(entry) {
        unsafe {
            let _ = ShowWindow(hwnd, SW_MINIMIZE);
        }
    }
}

pub fn unhide(entry: &str) {
    unhide_windows(&windows_of(entry));
}

fn unhide_windows(windows: &[HWND]) {
    for hwnd in windows {
        unsafe {
            if IsIconic(*hwnd).as_bool() {
                let _ = ShowWindow(*hwnd, SW_RESTORE);
            }
            let _ = SetForegroundWindow(*hwnd);
        }
    }
}

pub fn quit(entry: &str) {
    for hwnd in windows_of(entry) {
        unsafe {
            let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
    }
}

// MARK: Icons

/// 64×64 icon from the program, as PNG.
pub fn icon(entry: &str, cache_dir: &Path) -> Option<(Vec<u8>, &'static str)> {
    let exe = executable(entry);
    let _ = std::fs::create_dir_all(cache_dir);
    let cached = cache_dir.join(format!("{:x}.png", super::hash(&exe)));
    if let Ok(bytes) = std::fs::read(&cached) {
        return Some((bytes, "image/png"));
    }
    let rgba = extract_icon_rgba(&exe, 64)?;
    let mut png_bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png_bytes, 64, 64);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().ok()?;
        writer.write_image_data(&rgba).ok()?;
    }
    let _ = std::fs::write(&cached, &png_bytes);
    Some((png_bytes, "image/png"))
}

fn extract_icon_rgba(exe: &str, size: i32) -> Option<Vec<u8>> {
    unsafe {
        let mut path = [0u16; 260];
        for (i, c) in exe.encode_utf16().take(259).enumerate() {
            path[i] = c;
        }
        let mut icons = [HICON::default()];
        let count = PrivateExtractIconsW(&path, 0, size, size, Some(&mut icons), None, 0);
        if count == 0 || icons[0].is_invalid() {
            return None;
        }
        let hicon = icons[0];
        let mut info = ICONINFO::default();
        GetIconInfo(hicon, &mut info).ok()?;

        let dc = CreateCompatibleDC(None);
        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: size,
                biHeight: -size, // top-down
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut pixels = vec![0u8; (size * size * 4) as usize];
        let lines = GetDIBits(dc, info.hbmColor, 0, size as u32, Some(pixels.as_mut_ptr() as *mut _), &mut bmi, DIB_RGB_COLORS);
        let _ = DeleteDC(dc);
        let _ = DeleteObject(info.hbmColor.into());
        let _ = DeleteObject(info.hbmMask.into());
        let _ = DestroyIcon(hicon);
        if lines == 0 {
            return None;
        }
        // BGRA → RGBA; icons without an alpha channel are fully opaque.
        let has_alpha = pixels.chunks(4).any(|p| p[3] != 0);
        for p in pixels.chunks_mut(4) {
            p.swap(0, 2);
            if !has_alpha {
                p[3] = 255;
            }
        }
        Some(pixels)
    }
}
