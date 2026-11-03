//! Persistent settings (JSON in the platform config directory).

use pickle_discord::ClientProperties;
use pickle_voice::{AudioSettings, VideoSettings};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    /// Stored only when "remember me" is ticked.
    pub token: Option<String>,
    pub remember_token: bool,
    pub audio: AudioSettings,
    pub video: VideoSettings,
    pub theme: String,
    pub enabled_plugins: Vec<String>,
    pub user_volumes: HashMap<u64, f32>,
    pub ui_scale: f32,
    pub compact_messages: bool,
    pub show_images: bool,
    pub client: ClientProperties,
    pub last_guild: Option<u64>,
    pub last_channel: Option<u64>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            token: None,
            remember_token: true,
            audio: AudioSettings::default(),
            video: VideoSettings::default(),
            theme: "builtin:pickle-dark".into(),
            enabled_plugins: Vec::new(),
            user_volumes: HashMap::new(),
            ui_scale: 1.0,
            compact_messages: false,
            show_images: true,
            client: ClientProperties::default(),
            last_guild: None,
            last_channel: None,
        }
    }
}

pub struct Paths {
    pub config_file: PathBuf,
    pub themes: PathBuf,
    pub plugins: PathBuf,
    pub plugin_data: PathBuf,
}

impl Paths {
    pub fn resolve() -> Self {
        let base = std::env::var_os("PICKLE_HOME")
            .map(PathBuf::from)
            .or_else(|| directories::ProjectDirs::from("dev", "pickle", "Pickle").map(|d| d.config_dir().to_path_buf()))
            .unwrap_or_else(|| PathBuf::from(".pickle"));
        let p = Self {
            config_file: base.join("settings.json"),
            themes: base.join("themes"),
            plugins: base.join("plugins"),
            plugin_data: base.join("plugin-data"),
        };
        for d in [&p.themes, &p.plugins, &p.plugin_data] {
            let _ = std::fs::create_dir_all(d);
        }
        p
    }
}

impl AppConfig {
    pub fn load(paths: &Paths) -> Self {
        std::fs::read_to_string(&paths.config_file)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, paths: &Paths) {
        let mut copy = self.clone();
        if !copy.remember_token {
            copy.token = None;
        }
        let Ok(json) = serde_json::to_vec_pretty(&copy) else { return };
        let tmp = paths.config_file.with_extension("json.tmp");
        if std::fs::write(&tmp, json).is_ok() {
            restrict_permissions(&tmp);
            let _ = std::fs::rename(&tmp, &paths.config_file);
        }
    }
}

/// The settings file may contain the account token: keep it user-only.
fn restrict_permissions(path: &std::path::Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    #[cfg(not(unix))]
    let _ = path;
}
