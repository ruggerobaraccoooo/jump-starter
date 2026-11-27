//! macOS: the same system calls the native Swift app uses (NSWorkspace / NSRunningApplication).

use super::Running;
use objc2::rc::autoreleasepool;
use objc2_app_kit::{NSApplicationActivationOptions, NSApplicationActivationPolicy, NSRunningApplication, NSWorkspace};
use objc2_foundation::NSString;
use std::process::Command;

pub const APP_EXTENSIONS: &[&str] = &["app"];
pub const DEFAULT_APP_DIR: Option<&str> = Some("/Applications");

/// On macOS an app is identified by its bundle path.
pub fn identity(entry: &str) -> String {
    entry.to_string()
}

pub fn app_name(entry: &str) -> String {
    super::file_stem(entry)
}

fn plist_value(app_path: &str, key: &str) -> Option<String> {
    let out = Command::new("/usr/bin/plutil")
        .args(["-extract", key, "raw", "-o", "-"])
        .arg(format!("{app_path}/Contents/Info.plist"))
        .output()
        .ok()?;
    let value = String::from_utf8(out.stdout).ok()?.trim().to_string();
    (out.status.success() && !value.is_empty()).then_some(value)
}

fn running_instances(app_path: &str) -> Vec<objc2::rc::Retained<NSRunningApplication>> {
    let Some(bundle_id) = plist_value(app_path, "CFBundleIdentifier") else { return vec![] };
    autoreleasepool(|_| {
        NSRunningApplication::runningApplicationsWithBundleIdentifier(&NSString::from_str(&bundle_id))
            .iter()
            .collect()
    })
}

pub fn running() -> Vec<Running> {
    autoreleasepool(|_| {
        NSWorkspace::sharedWorkspace()
            .runningApplications()
            .iter()
            .filter(|app| app.activationPolicy() == NSApplicationActivationPolicy::Regular)
            .filter_map(|app| {
                let path = app.bundleURL()?.path()?.to_string();
                Some(Running { path, hidden: app.isHidden() })
            })
            .collect()
    })
}

/// Opens (or brings to the front) an app, optionally with arguments (browser profile + URLs).
pub fn open(app_path: &str, args: &[String]) -> Result<(), String> {
    for app in running_instances(app_path) {
        app.unhide();
    }
    let mut cmd = Command::new("/usr/bin/open");
    if args.is_empty() {
        cmd.arg(app_path);
    } else {
        // Like `open -na "Google Chrome" --args --profile-directory=… <urls>`.
        cmd.arg("-na").arg(app_path).arg("--args").args(args);
    }
    cmd.spawn().map(|_| ()).map_err(|e| e.to_string())
}

pub fn hide(app_path: &str) {
    for app in running_instances(app_path) {
        app.hide();
    }
}

pub fn unhide(app_path: &str) {
    for app in running_instances(app_path) {
        app.unhide();
        app.activateWithOptions(NSApplicationActivationOptions::ActivateAllWindows);
    }
}

pub fn quit(app_path: &str) {
    for app in running_instances(app_path) {
        app.terminate();
    }
}

/// App icon as PNG (from the bundle's .icns, converted with `sips`).
pub fn icon(app_path: &str, cache_dir: &std::path::Path) -> Option<(Vec<u8>, &'static str)> {
    let mut name = plist_value(app_path, "CFBundleIconFile")?;
    if !name.ends_with(".icns") {
        name.push_str(".icns");
    }
    let icns = format!("{app_path}/Contents/Resources/{name}");
    let _ = std::fs::create_dir_all(cache_dir);
    let out = cache_dir.join(format!("{:x}.png", super::hash(app_path)));
    if !out.exists() {
        let status = Command::new("/usr/bin/sips")
            .args(["-s", "format", "png", "-Z", "128"])
            .arg(&icns)
            .arg("--out")
            .arg(&out)
            .output()
            .ok()?;
        if !status.status.success() {
            return None;
        }
    }
    std::fs::read(out).ok().map(|b| (b, "image/png"))
}
