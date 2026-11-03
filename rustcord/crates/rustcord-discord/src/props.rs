use base64::Engine;
use serde::{Deserialize, Serialize};

/// The "super properties" a user client identifies itself with. They are sent
/// both in the gateway IDENTIFY and (base64 encoded) as `X-Super-Properties`
/// on every REST request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ClientProperties {
    pub os: String,
    pub browser: String,
    pub device: String,
    pub system_locale: String,
    pub browser_user_agent: String,
    pub browser_version: String,
    pub os_version: String,
    pub release_channel: String,
    pub client_build_number: u64,
}

impl Default for ClientProperties {
    fn default() -> Self {
        let os = if cfg!(target_os = "windows") {
            "Windows"
        } else if cfg!(target_os = "macos") {
            "Mac OS X"
        } else {
            "Linux"
        };
        let ua_platform = if cfg!(target_os = "windows") {
            "Windows NT 10.0; Win64; x64"
        } else if cfg!(target_os = "macos") {
            "Macintosh; Intel Mac OS X 10_15_7"
        } else {
            "X11; Linux x86_64"
        };
        let browser_version = "140.0.0.0".to_string();
        Self {
            os: os.into(),
            browser: "Chrome".into(),
            device: String::new(),
            system_locale: "en-US".into(),
            browser_user_agent: format!(
                "Mozilla/5.0 ({ua_platform}) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/{browser_version} Safari/537.36"
            ),
            browser_version,
            os_version: if cfg!(target_os = "windows") { "10".into() } else { String::new() },
            release_channel: "stable".into(),
            client_build_number: 460_000,
        }
    }
}

impl ClientProperties {
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "os": self.os,
            "browser": self.browser,
            "device": self.device,
            "system_locale": self.system_locale,
            "browser_user_agent": self.browser_user_agent,
            "browser_version": self.browser_version,
            "os_version": self.os_version,
            "referrer": "",
            "referring_domain": "",
            "referrer_current": "",
            "referring_domain_current": "",
            "release_channel": self.release_channel,
            "client_build_number": self.client_build_number,
            "client_event_source": serde_json::Value::Null,
        })
    }

    pub fn super_properties_b64(&self) -> String {
        base64::engine::general_purpose::STANDARD.encode(self.to_json().to_string())
    }
}
