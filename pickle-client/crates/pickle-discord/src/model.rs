//! Data model. Every struct is deliberately lenient: unknown fields are ignored,
//! missing or `null` fields fall back to defaults. Discord ships slightly
//! different shapes to user clients depending on gateway capabilities, so a
//! strict model would break on the first payload change.

use crate::snowflake::Snowflake;
use serde::{Deserialize, Deserializer, Serialize};

/// Treat `null` the same as a missing field.
pub(crate) fn nullable<'de, D, T>(d: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}

pub const CDN: &str = "https://cdn.discordapp.com";

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct User {
    pub id: Snowflake,
    #[serde(default, deserialize_with = "nullable")]
    pub username: String,
    #[serde(default)]
    pub global_name: Option<String>,
    #[serde(default)]
    pub avatar: Option<String>,
    #[serde(default)]
    pub discriminator: Option<String>,
    #[serde(default, deserialize_with = "nullable")]
    pub bot: bool,
}

impl User {
    pub fn display_name(&self) -> &str {
        self.global_name
            .as_deref()
            .filter(|s| !s.is_empty())
            .unwrap_or(&self.username)
    }

    pub fn avatar_url(&self, size: u32) -> String {
        match &self.avatar {
            Some(hash) => {
                let ext = if hash.starts_with("a_") { "gif" } else { "png" };
                // Animated avatars are requested as PNG (first frame) to save bandwidth.
                let _ = ext;
                format!("{CDN}/avatars/{}/{hash}.png?size={size}", self.id)
            }
            None => {
                let idx = (self.id.0 >> 22) % 6;
                format!("{CDN}/embed/avatars/{idx}.png")
            }
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Member {
    #[serde(default)]
    pub user: Option<User>,
    #[serde(default)]
    pub nick: Option<String>,
    #[serde(default, deserialize_with = "nullable")]
    pub roles: Vec<Snowflake>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChannelKind {
    Text,
    Dm,
    Voice,
    GroupDm,
    Category,
    Announcement,
    Thread,
    Stage,
    Forum,
    Other(u8),
}

impl ChannelKind {
    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::Text,
            1 => Self::Dm,
            2 => Self::Voice,
            3 => Self::GroupDm,
            4 => Self::Category,
            5 => Self::Announcement,
            10..=12 => Self::Thread,
            13 => Self::Stage,
            15 | 16 => Self::Forum,
            o => Self::Other(o),
        }
    }

    pub fn is_voice(self) -> bool {
        matches!(self, Self::Voice | Self::Stage)
    }

    pub fn is_text_like(self) -> bool {
        matches!(
            self,
            Self::Text | Self::Dm | Self::GroupDm | Self::Announcement | Self::Thread | Self::Voice
        )
    }

    pub fn is_private(self) -> bool {
        matches!(self, Self::Dm | Self::GroupDm)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Channel {
    pub id: Snowflake,
    #[serde(rename = "type", default)]
    pub kind_raw: u8,
    #[serde(default)]
    pub guild_id: Option<Snowflake>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default, deserialize_with = "nullable")]
    pub position: i32,
    #[serde(default)]
    pub parent_id: Option<Snowflake>,
    #[serde(default)]
    pub topic: Option<String>,
    #[serde(default)]
    pub last_message_id: Option<Snowflake>,
    #[serde(default, deserialize_with = "nullable")]
    pub recipients: Vec<User>,
    #[serde(default, deserialize_with = "nullable")]
    pub recipient_ids: Vec<Snowflake>,
    #[serde(default)]
    pub bitrate: Option<u32>,
    #[serde(default)]
    pub user_limit: Option<u32>,
    #[serde(default, deserialize_with = "nullable")]
    pub nsfw: bool,
    #[serde(default)]
    pub icon: Option<String>,
}

impl Channel {
    pub fn kind(&self) -> ChannelKind {
        ChannelKind::from_u8(self.kind_raw)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Attachment {
    pub id: Snowflake,
    #[serde(default, deserialize_with = "nullable")]
    pub filename: String,
    #[serde(default, deserialize_with = "nullable")]
    pub url: String,
    #[serde(default)]
    pub proxy_url: Option<String>,
    #[serde(default, deserialize_with = "nullable")]
    pub size: u64,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
    #[serde(default)]
    pub content_type: Option<String>,
}

impl Attachment {
    pub fn is_image(&self) -> bool {
        self.width.is_some()
            && self
                .content_type
                .as_deref()
                .map(|c| c.starts_with("image/"))
                .unwrap_or(true)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct EmbedMedia {
    #[serde(default, deserialize_with = "nullable")]
    pub url: String,
    #[serde(default)]
    pub proxy_url: Option<String>,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct EmbedField {
    #[serde(default, deserialize_with = "nullable")]
    pub name: String,
    #[serde(default, deserialize_with = "nullable")]
    pub value: String,
    #[serde(default, deserialize_with = "nullable")]
    pub inline: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct EmbedText {
    #[serde(default, alias = "name", deserialize_with = "nullable")]
    pub text: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Embed {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub color: Option<u32>,
    #[serde(default)]
    pub author: Option<EmbedText>,
    #[serde(default)]
    pub footer: Option<EmbedText>,
    #[serde(default)]
    pub image: Option<EmbedMedia>,
    #[serde(default)]
    pub thumbnail: Option<EmbedMedia>,
    #[serde(default, deserialize_with = "nullable")]
    pub fields: Vec<EmbedField>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Message {
    pub id: Snowflake,
    pub channel_id: Snowflake,
    #[serde(default)]
    pub guild_id: Option<Snowflake>,
    #[serde(default)]
    pub author: User,
    #[serde(default, deserialize_with = "nullable")]
    pub content: String,
    #[serde(default, deserialize_with = "nullable")]
    pub timestamp: String,
    #[serde(default)]
    pub edited_timestamp: Option<String>,
    #[serde(default, deserialize_with = "nullable")]
    pub attachments: Vec<Attachment>,
    #[serde(default, deserialize_with = "nullable")]
    pub embeds: Vec<Embed>,
    #[serde(default, deserialize_with = "nullable")]
    pub mentions: Vec<User>,
    #[serde(default, deserialize_with = "nullable")]
    pub mention_everyone: bool,
    #[serde(default)]
    pub referenced_message: Option<Box<Message>>,
    #[serde(rename = "type", default, deserialize_with = "nullable")]
    pub kind: u8,
    #[serde(default)]
    pub nonce: Option<serde_json::Value>,
    #[serde(default)]
    pub member: Option<Member>,

    /// Local-only state (never sent by Discord).
    #[serde(skip)]
    pub local: LocalMessageState,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub enum LocalMessageState {
    #[default]
    Delivered,
    /// Optimistically inserted; waiting for MESSAGE_CREATE with the same nonce.
    Pending,
    Failed(String),
    /// Shown only locally (plugin bot messages, system notices).
    Ephemeral,
}

impl Message {
    pub fn nonce_snowflake(&self) -> Option<Snowflake> {
        match self.nonce.as_ref()? {
            serde_json::Value::String(s) => s.parse().ok(),
            serde_json::Value::Number(n) => n.as_u64().map(Snowflake),
            _ => None,
        }
    }

    pub fn author_name(&self) -> &str {
        self.member
            .as_ref()
            .and_then(|m| m.nick.as_deref())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| self.author.display_name())
    }

    pub fn mentions_user(&self, id: Snowflake) -> bool {
        self.mention_everyone || self.mentions.iter().any(|u| u.id == id)
    }
}

/// Partial message update (MESSAGE_UPDATE only carries changed fields).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct MessageUpdate {
    pub id: Snowflake,
    pub channel_id: Snowflake,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub edited_timestamp: Option<String>,
    #[serde(default)]
    pub embeds: Option<Vec<Embed>>,
    #[serde(default)]
    pub attachments: Option<Vec<Attachment>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct MessageDelete {
    pub id: Snowflake,
    pub channel_id: Snowflake,
    #[serde(default)]
    pub guild_id: Option<Snowflake>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct VoiceState {
    #[serde(default)]
    pub guild_id: Option<Snowflake>,
    #[serde(default)]
    pub channel_id: Option<Snowflake>,
    pub user_id: Snowflake,
    #[serde(default, deserialize_with = "nullable")]
    pub session_id: String,
    #[serde(default, deserialize_with = "nullable")]
    pub self_mute: bool,
    #[serde(default, deserialize_with = "nullable")]
    pub self_deaf: bool,
    #[serde(default, deserialize_with = "nullable")]
    pub self_video: bool,
    #[serde(default, deserialize_with = "nullable")]
    pub self_stream: bool,
    #[serde(default, deserialize_with = "nullable")]
    pub mute: bool,
    #[serde(default, deserialize_with = "nullable")]
    pub deaf: bool,
    #[serde(default)]
    pub member: Option<Member>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct VoiceServerUpdate {
    #[serde(default, deserialize_with = "nullable")]
    pub token: String,
    #[serde(default)]
    pub guild_id: Option<Snowflake>,
    #[serde(default)]
    pub channel_id: Option<Snowflake>,
    #[serde(default)]
    pub endpoint: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct TypingStart {
    pub channel_id: Snowflake,
    #[serde(default)]
    pub guild_id: Option<Snowflake>,
    pub user_id: Snowflake,
    #[serde(default)]
    pub member: Option<Member>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct GuildProperties {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
}

/// A guild as delivered by READY / GUILD_CREATE. Handles both the legacy
/// shape and the `CLIENT_STATE_V2` shape (where name/icon live in `properties`).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct GuildPayload {
    pub id: Snowflake,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub properties: Option<GuildProperties>,
    #[serde(default, deserialize_with = "nullable")]
    pub channels: Vec<Channel>,
    #[serde(default, deserialize_with = "nullable")]
    pub threads: Vec<Channel>,
    #[serde(default, deserialize_with = "nullable")]
    pub voice_states: Vec<VoiceState>,
    #[serde(default, deserialize_with = "nullable")]
    pub unavailable: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Guild {
    pub id: Snowflake,
    pub name: String,
    pub icon: Option<String>,
}

impl Guild {
    pub fn icon_url(&self, size: u32) -> Option<String> {
        self.icon
            .as_ref()
            .map(|h| format!("{CDN}/icons/{}/{h}.png?size={size}", self.id))
    }

    pub fn initials(&self) -> String {
        self.name
            .split_whitespace()
            .filter_map(|w| w.chars().next())
            .take(3)
            .collect()
    }
}

impl GuildPayload {
    /// Split into the compact guild record plus its channels / voice states.
    pub fn into_parts(self) -> (Guild, Vec<Channel>, Vec<VoiceState>) {
        let props = self.properties.unwrap_or_default();
        let guild = Guild {
            id: self.id,
            name: self.name.or(props.name).unwrap_or_else(|| "Unknown server".into()),
            icon: self.icon.or(props.icon),
        };
        let mut channels = self.channels;
        channels.extend(self.threads);
        for c in &mut channels {
            c.guild_id = Some(self.id);
        }
        let mut voice = self.voice_states;
        for v in &mut voice {
            v.guild_id = Some(self.id);
        }
        (guild, channels, voice)
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Ready {
    pub user: User,
    pub session_id: String,
    #[serde(default)]
    pub resume_gateway_url: Option<String>,
    #[serde(default, deserialize_with = "nullable")]
    pub guilds: Vec<GuildPayload>,
    #[serde(default, deserialize_with = "nullable")]
    pub private_channels: Vec<Channel>,
    /// Present when the DEDUPE_USER_OBJECTS capability is used.
    #[serde(default, deserialize_with = "nullable")]
    pub users: Vec<User>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SupplementalGuild {
    pub id: Snowflake,
    #[serde(default, deserialize_with = "nullable")]
    pub voice_states: Vec<VoiceState>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ReadySupplemental {
    #[serde(default, deserialize_with = "nullable")]
    pub guilds: Vec<SupplementalGuild>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct GuildDelete {
    pub id: Snowflake,
    #[serde(default, deserialize_with = "nullable")]
    pub unavailable: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guild_v2_properties() {
        let j = r#"{"id":"1","properties":{"name":"Pickles","icon":null},
            "channels":[{"id":"2","type":0,"name":"general","position":0}],
            "voice_states":[{"user_id":"3","channel_id":"4","session_id":"x"}]}"#;
        let g: GuildPayload = serde_json::from_str(j).unwrap();
        let (g, ch, vs) = g.into_parts();
        assert_eq!(g.name, "Pickles");
        assert_eq!(ch[0].guild_id, Some(Snowflake(1)));
        assert_eq!(vs[0].guild_id, Some(Snowflake(1)));
    }

    #[test]
    fn message_with_nulls() {
        let j = r#"{"id":"10","channel_id":"11","author":{"id":"12","username":"a","global_name":null},
            "content":"hi","timestamp":"2024-01-01T00:00:00+00:00","attachments":null,"embeds":[],
            "mentions":[],"type":0,"nonce":"1234"}"#;
        let m: Message = serde_json::from_str(j).unwrap();
        assert_eq!(m.author_name(), "a");
        assert_eq!(m.nonce_snowflake(), Some(Snowflake(1234)));
    }
}
