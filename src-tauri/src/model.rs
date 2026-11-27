//! Groups, apps and shortcuts, stored as JSON in the app's data folder.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Shortcuts {
    /// Accelerator strings, e.g. "Command+1" or "Control+Alt+1".
    pub open: Option<String>,
    pub hide: Option<String>,
    pub quit: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ProfileLaunch {
    pub id: String,
    /// Browser profile ("Profile 1" for Chromium, profile name for Firefox); None = last used.
    pub profile: Option<String>,
    pub profile_name: Option<String>,
    #[serde(default)]
    pub urls: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AppEntry {
    pub id: String,
    pub name: String,
    /// .app bundle (macOS), .exe/.lnk (Windows) or .desktop/binary (Linux).
    pub path: String,
    #[serde(default)]
    pub profiles: Vec<ProfileLaunch>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub apps: Vec<AppEntry>,
    #[serde(default)]
    pub shortcuts: Shortcuts,
}

pub struct Store {
    file: PathBuf,
    pub groups: Vec<Group>,
}

impl Store {
    pub fn load(data_dir: PathBuf) -> Store {
        let _ = std::fs::create_dir_all(&data_dir);
        let file = data_dir.join("groups.json");
        let groups = std::fs::read(&file)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .or_else(import_from_mac_app)
            .unwrap_or_default();
        let store = Store { file, groups };
        store.save();
        store
    }

    pub fn save(&self) {
        if let Ok(json) = serde_json::to_vec_pretty(&self.groups) {
            let _ = std::fs::write(&self.file, json);
        }
    }

    pub fn group(&self, id: &str) -> Option<&Group> {
        self.groups.iter().find(|g| g.id == id)
    }
}

pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// First launch on a Mac: take over the groups of the native Swift app, if present.
#[cfg(target_os = "macos")]
fn import_from_mac_app() -> Option<Vec<Group>> {
    let home = std::env::var("HOME").ok()?;
    let path = PathBuf::from(home).join("Library/Application Support/AppGroups/groups.json");
    let value: serde_json::Value = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    let groups = value.as_array()?;
    Some(
        groups
            .iter()
            .map(|g| Group {
                id: new_id(),
                name: g["name"].as_str().unwrap_or("Group").to_string(),
                apps: g["apps"]
                    .as_array()
                    .map(|apps| {
                        apps.iter()
                            .map(|a| AppEntry {
                                id: new_id(),
                                name: a["name"].as_str().unwrap_or("").to_string(),
                                path: a["path"].as_str().unwrap_or("").to_string(),
                                profiles: a["profileLaunches"]
                                    .as_array()
                                    .map(|ls| {
                                        ls.iter()
                                            .map(|l| ProfileLaunch {
                                                id: new_id(),
                                                profile: l["profileDir"].as_str().map(String::from),
                                                profile_name: l["profileName"].as_str().map(String::from),
                                                urls: l["urlList"]
                                                    .as_array()
                                                    .map(|u| u.iter().filter_map(|x| x.as_str().map(String::from)).collect())
                                                    .unwrap_or_default(),
                                            })
                                            .collect()
                                    })
                                    .unwrap_or_default(),
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                shortcuts: Shortcuts {
                    open: carbon_to_accelerator(&g["hotkey"]),
                    hide: carbon_to_accelerator(&g["hideHotkey"]),
                    quit: carbon_to_accelerator(&g["quitHotkey"]),
                },
            })
            .collect(),
    )
}

#[cfg(not(target_os = "macos"))]
fn import_from_mac_app() -> Option<Vec<Group>> {
    None
}

/// The Swift app stores Carbon modifier masks; convert to "Control+Alt+Shift+Command+1".
#[cfg(target_os = "macos")]
fn carbon_to_accelerator(hotkey: &serde_json::Value) -> Option<String> {
    let mods = hotkey["modifiers"].as_u64()?;
    let key = hotkey["key"].as_str()?;
    let mut parts = Vec::new();
    if mods & 4096 != 0 { parts.push("Control"); }
    if mods & 2048 != 0 { parts.push("Alt"); }
    if mods & 512 != 0 { parts.push("Shift"); }
    if mods & 256 != 0 { parts.push("Command"); }
    if parts.is_empty() { return None; }
    Some(format!("{}+{}", parts.join("+"), key))
}
