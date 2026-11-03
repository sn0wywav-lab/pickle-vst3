//! Event-sourced client state. Owned by the UI thread, so no locking at all:
//! gateway events are parsed on the network threads and only the cheap
//! "apply" step runs here.

use crate::gateway::GatewayEvent;
use crate::model::*;
use crate::snowflake::Snowflake;
use std::collections::{HashMap, HashSet, VecDeque};
use std::time::{Duration, Instant};

#[derive(Debug, Default)]
pub struct ChannelMessages {
    /// Sorted oldest -> newest by id.
    pub messages: VecDeque<Message>,
    /// True once a history fetch returned fewer than requested.
    pub reached_start: bool,
    pub loading: bool,
    pub loaded_once: bool,
    pub error: Option<String>,
}

impl ChannelMessages {
    fn position(&self, id: Snowflake) -> Result<usize, usize> {
        self.messages.binary_search_by(|m| m.id.cmp(&id))
    }

    pub fn oldest(&self) -> Option<Snowflake> {
        self.messages
            .iter()
            .find(|m| m.local == LocalMessageState::Delivered)
            .map(|m| m.id)
    }
}

#[derive(Debug)]
pub struct Store {
    pub me: Option<User>,
    pub session_id: Option<String>,
    pub users: HashMap<Snowflake, User>,
    pub guilds: Vec<Guild>,
    pub channels: HashMap<Snowflake, Channel>,
    guild_channels: HashMap<Snowflake, Vec<Snowflake>>,
    pub private_channels: Vec<Snowflake>,
    pub messages: HashMap<Snowflake, ChannelMessages>,
    /// (guild id or 0 for private calls, user id) -> state
    pub voice_states: HashMap<(Snowflake, Snowflake), VoiceState>,
    pub typing: HashMap<Snowflake, Vec<(Snowflake, Instant)>>,
    pub unread: HashSet<Snowflake>,
    pub mention_counts: HashMap<Snowflake, u32>,
    pub max_messages_per_channel: usize,
    pub ready: bool,
}

impl Default for Store {
    fn default() -> Self {
        Self {
            me: None,
            session_id: None,
            users: HashMap::new(),
            guilds: Vec::new(),
            channels: HashMap::new(),
            guild_channels: HashMap::new(),
            private_channels: Vec::new(),
            messages: HashMap::new(),
            voice_states: HashMap::new(),
            typing: HashMap::new(),
            unread: HashSet::new(),
            mention_counts: HashMap::new(),
            max_messages_per_channel: 600,
            ready: false,
        }
    }
}

/// Category with its children, for rendering a channel sidebar.
pub struct ChannelGroup<'a> {
    pub category: Option<&'a Channel>,
    pub channels: Vec<&'a Channel>,
}

impl Store {
    pub fn me_id(&self) -> Snowflake {
        self.me.as_ref().map(|u| u.id).unwrap_or_default()
    }

    fn add_channel(&mut self, mut c: Channel) {
        for u in c.recipients.drain(..) {
            if !c.recipient_ids.contains(&u.id) {
                c.recipient_ids.push(u.id);
            }
            self.users.insert(u.id, u);
        }
        if let Some(g) = c.guild_id {
            let list = self.guild_channels.entry(g).or_default();
            if !list.contains(&c.id) {
                list.push(c.id);
            }
        } else if c.kind().is_private() && !self.private_channels.contains(&c.id) {
            self.private_channels.push(c.id);
        }
        self.channels.insert(c.id, c);
    }

    fn remove_channel(&mut self, id: Snowflake) {
        if let Some(c) = self.channels.remove(&id) {
            if let Some(g) = c.guild_id {
                if let Some(list) = self.guild_channels.get_mut(&g) {
                    list.retain(|x| *x != id);
                }
            }
        }
        self.private_channels.retain(|x| *x != id);
        self.messages.remove(&id);
    }

    fn sort_private(&mut self) {
        let channels = &self.channels;
        self.private_channels.sort_by_key(|id| {
            std::cmp::Reverse(
                channels
                    .get(id)
                    .and_then(|c| c.last_message_id)
                    .unwrap_or(*id),
            )
        });
    }

    fn upsert_guild(&mut self, payload: GuildPayload) {
        if payload.unavailable && payload.name.is_none() && payload.properties.is_none() {
            return;
        }
        let (guild, channels, voice) = payload.into_parts();
        let gid = guild.id;
        match self.guilds.iter_mut().find(|g| g.id == gid) {
            Some(g) => *g = guild,
            None => self.guilds.push(guild),
        }
        if !channels.is_empty() {
            if let Some(old) = self.guild_channels.remove(&gid) {
                for id in old {
                    self.channels.remove(&id);
                }
            }
            for c in channels {
                self.add_channel(c);
            }
        }
        if !voice.is_empty() {
            self.voice_states.retain(|(g, _), _| *g != gid);
            for v in voice {
                self.set_voice_state(v);
            }
        }
    }

    fn set_voice_state(&mut self, v: VoiceState) {
        if let Some(u) = v.member.as_ref().and_then(|m| m.user.clone()) {
            self.users.entry(u.id).or_insert(u);
        }
        let key = (v.guild_id.unwrap_or_default(), v.user_id);
        if v.channel_id.is_none() {
            self.voice_states.remove(&key);
        } else {
            self.voice_states.insert(key, v);
        }
    }

    /// Apply a gateway event. `active_channel` is the channel the user is
    /// looking at (used for unread tracking).
    pub fn apply(&mut self, ev: GatewayEvent, active_channel: Option<Snowflake>) {
        match ev {
            GatewayEvent::Ready(r) => {
                let r = *r;
                *self = Store {
                    max_messages_per_channel: self.max_messages_per_channel,
                    ..Store::default()
                };
                self.users.insert(r.user.id, r.user.clone());
                self.me = Some(r.user);
                self.session_id = Some(r.session_id);
                for u in r.users {
                    self.users.insert(u.id, u);
                }
                for g in r.guilds {
                    self.upsert_guild(g);
                }
                for c in r.private_channels {
                    self.add_channel(c);
                }
                self.sort_private();
                self.ready = true;
            }
            GatewayEvent::ReadySupplemental(s) => {
                for g in s.guilds {
                    for mut v in g.voice_states {
                        v.guild_id = Some(g.id);
                        self.set_voice_state(v);
                    }
                }
            }
            GatewayEvent::GuildCreate(g) | GatewayEvent::GuildUpdate(g) => {
                self.upsert_guild(*g);
            }
            GatewayEvent::GuildDelete(d) => {
                if !d.unavailable {
                    self.guilds.retain(|g| g.id != d.id);
                    if let Some(list) = self.guild_channels.remove(&d.id) {
                        for id in list {
                            self.channels.remove(&id);
                            self.messages.remove(&id);
                        }
                    }
                }
            }
            GatewayEvent::ChannelCreate(c) | GatewayEvent::ChannelUpdate(c) => {
                let private = c.kind().is_private();
                self.add_channel(*c);
                if private {
                    self.sort_private();
                }
            }
            GatewayEvent::ChannelDelete(c) => self.remove_channel(c.id),
            GatewayEvent::MessageCreate(m) => {
                let m = *m;
                let me = self.me_id();
                if Some(m.channel_id) != active_channel && m.author.id != me {
                    self.unread.insert(m.channel_id);
                    if m.mentions_user(me) || self.channels.get(&m.channel_id).map(|c| c.kind().is_private()).unwrap_or(false) {
                        *self.mention_counts.entry(m.channel_id).or_default() += 1;
                    }
                }
                if let Some(t) = self.typing.get_mut(&m.channel_id) {
                    t.retain(|(u, _)| *u != m.author.id);
                }
                if let Some(c) = self.channels.get_mut(&m.channel_id) {
                    c.last_message_id = Some(m.id);
                    if c.kind().is_private() {
                        self.sort_private();
                    }
                }
                self.insert_message(m);
            }
            GatewayEvent::MessageUpdate(u) => {
                if let Some(cm) = self.messages.get_mut(&u.channel_id) {
                    if let Ok(i) = cm.position(u.id) {
                        let m = &mut cm.messages[i];
                        let u = *u;
                        if let Some(c) = u.content {
                            m.content = c;
                        }
                        if u.edited_timestamp.is_some() {
                            m.edited_timestamp = u.edited_timestamp;
                        }
                        if let Some(e) = u.embeds {
                            m.embeds = e;
                        }
                        if let Some(a) = u.attachments {
                            m.attachments = a;
                        }
                    }
                }
            }
            GatewayEvent::MessageDelete(d) => {
                if let Some(cm) = self.messages.get_mut(&d.channel_id) {
                    if let Ok(i) = cm.position(d.id) {
                        cm.messages.remove(i);
                    }
                }
            }
            GatewayEvent::VoiceStateUpdate(v) => self.set_voice_state(*v),
            GatewayEvent::TypingStart(t) => {
                if t.user_id == self.me_id() {
                    return;
                }
                if let Some(u) = t.member.as_ref().and_then(|m| m.user.clone()) {
                    self.users.entry(u.id).or_insert(u);
                }
                let list = self.typing.entry(t.channel_id).or_default();
                list.retain(|(u, _)| *u != t.user_id);
                list.push((t.user_id, Instant::now()));
            }
            GatewayEvent::Status(_) | GatewayEvent::Resumed | GatewayEvent::VoiceServerUpdate(_) => {}
        }
    }

    /// Insert (or replace) a single message keeping the list sorted.
    pub fn insert_message(&mut self, m: Message) {
        self.users.entry(m.author.id).or_insert_with(|| m.author.clone());
        let cap = self.max_messages_per_channel;
        let cm = self.messages.entry(m.channel_id).or_default();
        // Resolve optimistic sends by nonce.
        if let Some(nonce) = m.nonce_snowflake() {
            if let Some(i) = cm
                .messages
                .iter()
                .rposition(|x| x.local == LocalMessageState::Pending && x.id == nonce)
            {
                cm.messages.remove(i);
            }
        }
        match cm.position(m.id) {
            Ok(i) => cm.messages[i] = m,
            Err(i) => cm.messages.insert(i, m),
        }
        while cm.messages.len() > cap {
            cm.messages.pop_front();
            cm.reached_start = false;
        }
    }

    /// Add an optimistic message (shown greyed until the server echoes it).
    pub fn insert_pending(&mut self, channel: Snowflake, nonce: Snowflake, content: String) {
        let author = self.me.clone().unwrap_or_default();
        let m = Message {
            id: nonce,
            channel_id: channel,
            author,
            content,
            nonce: Some(serde_json::Value::String(nonce.to_string())),
            local: LocalMessageState::Pending,
            ..Default::default()
        };
        let cm = self.messages.entry(channel).or_default();
        match cm.position(m.id) {
            Ok(i) => cm.messages[i] = m,
            Err(i) => cm.messages.insert(i, m),
        }
    }

    pub fn mark_failed(&mut self, channel: Snowflake, nonce: Snowflake, error: String) {
        if let Some(cm) = self.messages.get_mut(&channel) {
            if let Some(m) = cm.messages.iter_mut().find(|m| m.id == nonce) {
                m.local = LocalMessageState::Failed(error);
            }
        }
    }

    /// Prepend a page of older history (oldest-first).
    pub fn add_history(&mut self, channel: Snowflake, page: Vec<Message>, requested: usize) {
        for m in &page {
            self.users.entry(m.author.id).or_insert_with(|| m.author.clone());
        }
        let cm = self.messages.entry(channel).or_default();
        cm.loading = false;
        cm.loaded_once = true;
        cm.error = None;
        if page.len() < requested {
            cm.reached_start = true;
        }
        for m in page {
            match cm.position(m.id) {
                Ok(i) => cm.messages[i] = m,
                Err(i) => cm.messages.insert(i, m),
            }
        }
    }

    pub fn push_local(&mut self, channel: Snowflake, author: User, content: String) {
        let id = Snowflake::generate_nonce();
        let m = Message {
            id,
            channel_id: channel,
            author,
            content,
            local: LocalMessageState::Ephemeral,
            ..Default::default()
        };
        let cm = self.messages.entry(channel).or_default();
        match cm.position(id) {
            Ok(i) => cm.messages[i] = m,
            Err(i) => cm.messages.insert(i, m),
        }
    }

    pub fn mark_read(&mut self, channel: Snowflake) {
        self.unread.remove(&channel);
        self.mention_counts.remove(&channel);
    }

    pub fn guild_has_unread(&self, guild: Snowflake) -> bool {
        self.guild_channels
            .get(&guild)
            .map(|l| l.iter().any(|c| self.unread.contains(c)))
            .unwrap_or(false)
    }

    pub fn guild_mentions(&self, guild: Snowflake) -> u32 {
        self.guild_channels
            .get(&guild)
            .map(|l| l.iter().filter_map(|c| self.mention_counts.get(c)).sum())
            .unwrap_or(0)
    }

    pub fn dm_unread(&self) -> u32 {
        self.private_channels
            .iter()
            .filter_map(|c| self.mention_counts.get(c))
            .sum()
    }

    /// Channels of a guild grouped under their categories, Discord-sorted
    /// (text before voice, then by position).
    pub fn grouped_channels(&self, guild: Snowflake) -> Vec<ChannelGroup<'_>> {
        let Some(ids) = self.guild_channels.get(&guild) else {
            return Vec::new();
        };
        let all: Vec<&Channel> = ids.iter().filter_map(|id| self.channels.get(id)).collect();
        let mut cats: Vec<&Channel> = all
            .iter()
            .copied()
            .filter(|c| c.kind() == ChannelKind::Category)
            .collect();
        cats.sort_by_key(|c| (c.position, c.id));
        let sort_key = |c: &&Channel| (c.kind().is_voice() as u8, c.position, c.id);

        let mut groups = Vec::with_capacity(cats.len() + 1);
        let mut top: Vec<&Channel> = all
            .iter()
            .copied()
            .filter(|c| c.parent_id.is_none() && c.kind() != ChannelKind::Category && c.kind() != ChannelKind::Thread)
            .collect();
        top.sort_by_key(sort_key);
        if !top.is_empty() {
            groups.push(ChannelGroup { category: None, channels: top });
        }
        for cat in cats {
            let mut children: Vec<&Channel> = all
                .iter()
                .copied()
                .filter(|c| c.parent_id == Some(cat.id) && c.kind() != ChannelKind::Thread)
                .collect();
            children.sort_by_key(sort_key);
            groups.push(ChannelGroup { category: Some(cat), channels: children });
        }
        groups
    }

    pub fn channel_name(&self, id: Snowflake) -> String {
        let Some(c) = self.channels.get(&id) else {
            return "unknown".into();
        };
        if let Some(n) = c.name.as_deref().filter(|n| !n.is_empty()) {
            return n.to_string();
        }
        let names: Vec<&str> = c
            .recipient_ids
            .iter()
            .filter_map(|u| self.users.get(u))
            .map(|u| u.display_name())
            .collect();
        if names.is_empty() {
            "Direct message".into()
        } else {
            names.join(", ")
        }
    }

    /// Members currently in a voice channel.
    pub fn voice_members(&self, channel: Snowflake) -> Vec<&VoiceState> {
        let mut v: Vec<&VoiceState> = self
            .voice_states
            .values()
            .filter(|v| v.channel_id == Some(channel))
            .collect();
        v.sort_by_key(|v| v.user_id);
        v
    }

    pub fn user_name(&self, id: Snowflake) -> String {
        self.users
            .get(&id)
            .map(|u| u.display_name().to_string())
            .unwrap_or_else(|| format!("user {}", id.0 % 10_000))
    }

    /// Drop stale typing indicators (Discord's are valid for ~10s).
    pub fn prune_typing(&mut self) {
        let now = Instant::now();
        self.typing.retain(|_, list| {
            list.retain(|(_, t)| now.duration_since(*t) < Duration::from_secs(10));
            !list.is_empty()
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(id: u64, ch: u64, content: &str) -> Message {
        Message {
            id: Snowflake(id),
            channel_id: Snowflake(ch),
            content: content.into(),
            ..Default::default()
        }
    }

    #[test]
    fn messages_stay_sorted_and_deduped() {
        let mut s = Store::default();
        s.insert_message(msg(5, 1, "b"));
        s.insert_message(msg(3, 1, "a"));
        s.insert_message(msg(5, 1, "b2"));
        s.add_history(Snowflake(1), vec![msg(1, 1, "x"), msg(2, 1, "y")], 50);
        let ids: Vec<u64> = s.messages[&Snowflake(1)].messages.iter().map(|m| m.id.0).collect();
        assert_eq!(ids, vec![1, 2, 3, 5]);
        assert!(s.messages[&Snowflake(1)].reached_start);
        assert_eq!(s.messages[&Snowflake(1)].messages[3].content, "b2");
    }

    #[test]
    fn pending_is_replaced_by_echo() {
        let mut s = Store::default();
        let nonce = Snowflake::generate_nonce();
        s.insert_pending(Snowflake(9), nonce, "hello".into());
        let mut echo = msg(nonce.0 + 1000, 9, "hello");
        echo.nonce = Some(serde_json::Value::String(nonce.to_string()));
        s.insert_message(echo);
        let list = &s.messages[&Snowflake(9)].messages;
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].local, LocalMessageState::Delivered);
    }

    #[test]
    fn grouped_channels_sort() {
        let mut s = Store::default();
        let ch = |id: u64, kind: u8, pos: i32, parent: Option<u64>| Channel {
            id: Snowflake(id),
            kind_raw: kind,
            guild_id: Some(Snowflake(100)),
            name: Some(format!("c{id}")),
            position: pos,
            parent_id: parent.map(Snowflake),
            ..Default::default()
        };
        for c in [ch(1, 4, 0, None), ch(2, 2, 0, Some(1)), ch(3, 0, 1, Some(1)), ch(4, 0, 0, None)] {
            s.add_channel(c);
        }
        let groups = s.grouped_channels(Snowflake(100));
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].channels[0].id, Snowflake(4));
        // text (3) before voice (2) inside the category
        assert_eq!(groups[1].channels.iter().map(|c| c.id.0).collect::<Vec<_>>(), vec![3, 2]);
    }
}
