//! Offline demo data (`RUSTCORD_DEMO=1`): lets you preview themes, layout
//! and settings without logging in. No account is used; only public default
//! avatar images are downloaded.

use crate::app::{LoginState, RustcordApp, SettingsTab};
use rustcord_discord::{GatewayEvent, Message, Snowflake};

const READY: &str = r##"{
  "user": {"id": "1000", "username": "rustcord_dev", "global_name": "You"},
  "session_id": "demo",
  "guilds": [
    {"id": "2000", "name": "Oxide Lab", "channels": [
      {"id": "2100", "type": 4, "name": "Text Channels", "position": 0},
      {"id": "2101", "type": 0, "name": "general", "position": 0, "parent_id": "2100", "topic": "Zero-cost chat"},
      {"id": "2102", "type": 0, "name": "showcase", "position": 1, "parent_id": "2100"},
      {"id": "2103", "type": 5, "name": "announcements", "position": 2, "parent_id": "2100"},
      {"id": "2200", "type": 4, "name": "Voice Channels", "position": 1},
      {"id": "2201", "type": 2, "name": "Studio (510 kbps)", "position": 0, "parent_id": "2200"},
      {"id": "2202", "type": 2, "name": "Lounge", "position": 1, "parent_id": "2200"}
    ], "voice_states": [
      {"user_id": "1000", "channel_id": "2201", "session_id": "a", "self_video": true},
      {"user_id": "1001", "channel_id": "2201", "session_id": "b", "self_stream": true},
      {"user_id": "1002", "channel_id": "2201", "session_id": "c", "self_mute": true},
      {"user_id": "1003", "channel_id": "2202", "session_id": "d", "self_deaf": true}
    ]},
    {"id": "3000", "name": "Audio Nerds", "channels": [{"id": "3001", "type": 0, "name": "dsp", "position": 0}]},
    {"id": "4000", "name": "Night Owls", "channels": [{"id": "4001", "type": 0, "name": "chat", "position": 0}]}
  ],
  "private_channels": [
    {"id": "5001", "type": 1, "recipients": [{"id": "1001", "username": "ferris", "global_name": "Ferris"}], "last_message_id": "5"},
    {"id": "5002", "type": 3, "name": "Mix crew", "recipients": [{"id": "1002", "username": "crabby"}, {"id": "1003", "username": "oxide"}]}
  ],
  "users": [
    {"id": "1001", "username": "ferris", "global_name": "Ferris"},
    {"id": "1002", "username": "crabby", "global_name": "Crabby"},
    {"id": "1003", "username": "oxide", "global_name": "Oxide"}
  ]
}"##;

fn msg(id: u64, author: (u64, &str), content: &str) -> Message {
    serde_json::from_value(serde_json::json!({
        "id": id.to_string(),
        "channel_id": "2101",
        "guild_id": "2000",
        "author": {"id": author.0.to_string(), "username": author.1, "global_name": author.1},
        "content": content,
        "timestamp": "",
    }))
    .expect("demo message")
}

pub fn load(app: &mut RustcordApp) {
    let ready: rustcord_discord::Ready = serde_json::from_str(READY).expect("demo ready");
    app.store.apply(GatewayEvent::Ready(Box::new(ready)), None);
    app.login = LoginState::Ready;
    app.gateway_status = rustcord_discord::GatewayStatus::Connected;
    let base = (1_730_000_000_000u64 - rustcord_discord::snowflake::DISCORD_EPOCH_MS) << 22;
    let lines = [
        ((1001, "Ferris"), "just bumped my mic to **510 kb/s stereo** — no noise suppression, no AGC. it's *wild* how much better it sounds"),
        ((1001, "Ferris"), "going live at native 1440p 165 fps, 50 Mb/s — hit WATCH on my name"),
        ((1001, "Ferris"), "fullband 48 kHz too"),
        ((1002, "Crabby"), "<@1000> can you check the camera at 1080p60? I'm getting `H.264 level 4.2` in the stats"),
        ((1000, "You"), "yep, max-out picked 1920x1080 @ 60 fps (MJPEG). here's the encoder config:\n```rust\nEncoderConfig::new()\n    .bitrate(BitRate::from_bps(12_000_000))\n    .max_frame_rate(FrameRate::from_hz(60.0))\n```"),
        ((1003, "Oxide"), "theme ported straight from BetterDiscord btw 🎨 ~~no~~ zero changes needed"),
        ((1003, "Oxide"), "> plugins too?\nthe Vencord-style /uwu command works, just dropped the file in the plugins folder"),
    ];
    for (i, (who, text)) in lines.iter().enumerate() {
        let id = base + (((i as u64) * 90_000) << 22);
        app.store.insert_message(msg(id, *who, text));
    }
    let cm = app.store.messages.entry(Snowflake(2101)).or_default();
    cm.loaded_once = true;
    cm.reached_start = true;
    app.selected_guild = Some(Snowflake(2000));
    app.selected_channel = Some(Snowflake(std::env::var("RUSTCORD_DEMO_CHANNEL").ok().and_then(|c| c.parse().ok()).unwrap_or(2101)));
    app.media.guild = Some(Snowflake(2000));
    app.media.channel = Some(Snowflake(2201));
    app.media.camera_on = true;
    app.store.typing.insert(Snowflake(2101), vec![(Snowflake(1001), std::time::Instant::now() + std::time::Duration::from_secs(3600))]);
    if std::env::var_os("RUSTCORD_DEMO_GOLIVE").is_some() {
        app.open_golive();
    }
    if let Ok(tab) = std::env::var("RUSTCORD_DEMO_SETTINGS") {
        app.settings.open = true;
        app.settings.tab = match tab.as_str() {
            "video" => SettingsTab::Video,
            "streaming" => SettingsTab::Streaming,
            "appearance" => SettingsTab::Appearance,
            "plugins" => SettingsTab::Plugins,
            "account" => SettingsTab::Account,
            "about" => SettingsTab::About,
            _ => SettingsTab::Voice,
        };
    }
}
