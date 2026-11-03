//! Go Live: streaming your screen and watching other people's streams.
//!
//! A stream is a second media connection next to the voice one: the main
//! gateway hands out a stream key, an RTC server and a token, and the
//! media flows over a voice-protocol connection of the `screen` kind.

use rustcord_discord::{stream_key, GatewayCommand, GatewayHandle, Snowflake, StreamCreate, StreamServerUpdate};
use rustcord_voice::connection::{VideoAttrs, VoiceShared};
use rustcord_voice::engine::{EngineShared, IncomingSender};
use rustcord_voice::video::{CameraMode, CaptureStats};
use rustcord_voice::{AudioEngine, ConnKind, EngineMode, StreamSettings, VoiceConnectInfo, VoiceConnection};
use std::sync::Arc;
use tracing::info;

#[cfg(feature = "screen")]
use rustcord_voice::video::screen::ScreenStream;

pub type Repaint = Arc<dyn Fn() + Send + Sync>;

/// What the UI needs to show our own stream.
#[derive(Clone)]
pub struct LiveHandles {
    pub key: String,
    pub voice: Option<Arc<VoiceShared>>,
    pub stats: Option<Arc<CaptureStats>>,
    pub audio: Option<Arc<EngineShared>>,
}

/// A stream we're watching.
#[derive(Clone)]
pub struct WatchHandles {
    pub user: Snowflake,
    pub voice: Option<Arc<VoiceShared>>,
}

#[derive(Default)]
struct Rtc {
    server_id: Option<u64>,
    endpoint: Option<String>,
    token: String,
}

impl Rtc {
    fn ready(&self) -> Option<(u64, String, String)> {
        Some((self.server_id?, self.endpoint.clone().filter(|e| !e.is_empty())?, self.token.clone()))
    }
}

struct Live {
    key: String,
    channel: Snowflake,
    rtc: Rtc,
    conn: Option<VoiceConnection>,
    #[cfg(feature = "screen")]
    screen: Option<ScreenStream>,
    audio: Option<AudioEngine>,
}

struct Watch {
    key: String,
    user: Snowflake,
    channel: Snowflake,
    rtc: Rtc,
    conn: Option<VoiceConnection>,
}

#[derive(Default)]
pub struct StreamMgr {
    pub settings: StreamSettings,
    live: Option<Live>,
    watches: Vec<Watch>,
    #[cfg(feature = "screen")]
    preview: Option<ScreenStream>,
}

impl StreamMgr {
    pub fn new(settings: StreamSettings) -> Self {
        Self { settings, ..Default::default() }
    }

    /// Ask Discord for a Go Live slot in the voice channel we're in.
    pub fn start(&mut self, gw: &GatewayHandle, guild: Option<Snowflake>, channel: Snowflake, me: Snowflake) -> Result<(), String> {
        if !cfg!(feature = "screen") {
            return Err("This build has no screen capture support.".into());
        }
        if self.live.is_some() {
            return Ok(());
        }
        let key = stream_key(guild, channel, me);
        info!("go live: requesting {key}");
        gw.send(GatewayCommand::StreamCreate { guild_id: guild, channel_id: channel });
        gw.send(GatewayCommand::StreamSetPaused { stream_key: key.clone(), paused: false });
        self.live = Some(Live {
            key,
            channel,
            rtc: Rtc::default(),
            conn: None,
            #[cfg(feature = "screen")]
            screen: None,
            audio: None,
        });
        Ok(())
    }

    pub fn stop(&mut self, gw: Option<&GatewayHandle>) {
        if let Some(live) = self.live.take() {
            if let Some(gw) = gw {
                gw.send(GatewayCommand::StreamDelete { stream_key: live.key.clone() });
            }
            if let Some(c) = &live.conn {
                c.disconnect();
            }
        }
    }

    pub fn watch(&mut self, gw: &GatewayHandle, guild: Option<Snowflake>, channel: Snowflake, user: Snowflake) {
        if self.watches.iter().any(|w| w.user == user) {
            return;
        }
        let key = stream_key(guild, channel, user);
        gw.send(GatewayCommand::StreamWatch { stream_key: key.clone() });
        self.watches.push(Watch { key, user, channel, rtc: Rtc::default(), conn: None });
    }

    pub fn unwatch(&mut self, gw: Option<&GatewayHandle>, user: Snowflake) {
        if let Some(i) = self.watches.iter().position(|w| w.user == user) {
            let w = self.watches.remove(i);
            if let Some(gw) = gw {
                gw.send(GatewayCommand::StreamDelete { stream_key: w.key });
            }
            if let Some(c) = w.conn {
                c.disconnect();
            }
        }
    }

    /// Leaving voice ends our stream and every stream we watch.
    pub fn leave_all(&mut self, gw: Option<&GatewayHandle>) {
        self.stop(gw);
        let users: Vec<Snowflake> = self.watches.iter().map(|w| w.user).collect();
        for u in users {
            self.unwatch(gw, u);
        }
    }

    fn rtc_for(&mut self, key: &str) -> Option<&mut Rtc> {
        if let Some(l) = self.live.as_mut().filter(|l| l.key == key) {
            return Some(&mut l.rtc);
        }
        self.watches.iter_mut().find(|w| w.key == key).map(|w| &mut w.rtc)
    }

    pub fn on_create(&mut self, sc: &StreamCreate) {
        if let Some(rtc) = self.rtc_for(&sc.stream_key) {
            rtc.server_id = sc.rtc_server_id.as_deref().and_then(|s| s.parse().ok());
        }
    }

    pub fn on_server(&mut self, ssu: &StreamServerUpdate) {
        let key = ssu.stream_key.clone();
        if let Some(rtc) = self.rtc_for(&key) {
            rtc.endpoint = ssu.endpoint.clone();
            rtc.token = ssu.token.clone();
        }
        // A new server means any existing connection for this key is stale.
        if let Some(l) = self.live.as_mut().filter(|l| l.key == key) {
            l.conn = None;
        }
        if let Some(w) = self.watches.iter_mut().find(|w| w.key == key) {
            w.conn = None;
        }
    }

    /// The server ended a stream. Returns a message for the user.
    pub fn on_delete(&mut self, key: &str, reason: Option<&str>) -> Option<String> {
        if self.live.as_ref().is_some_and(|l| l.key == key) {
            self.stop(None);
            return Some(format!("Your stream ended{}", reason.map(|r| format!(" ({r})")).unwrap_or_default()));
        }
        if let Some(i) = self.watches.iter().position(|w| w.key == key) {
            self.watches.remove(i);
            return Some("The stream you were watching ended".into());
        }
        None
    }

    /// Open media connections that have everything they need. Returns the
    /// capture stats of a freshly started stream so the caller can announce
    /// its real resolution once known.
    pub fn try_connect(
        &mut self,
        me: Snowflake,
        session_id: Option<&str>,
        voice_audio: Option<IncomingSender>,
        repaint: Repaint,
    ) -> Option<Arc<CaptureStats>> {
        let session = session_id?.to_string();
        let mut started = None;
        if let Some(live) = self.live.as_mut() {
            if live.conn.is_none() {
                if let Some((server_id, endpoint, token)) = live.rtc.ready() {
                    let info = VoiceConnectInfo {
                        endpoint,
                        token,
                        session_id: session.clone(),
                        server_id,
                        channel_id: live.channel.0,
                        user_id: me.0,
                        kind: ConnKind::Stream,
                    };
                    let conn = VoiceConnection::connect(info, None, Some(repaint.clone()));
                    let s = &self.settings;
                    // Announce the requested size now; the real one follows once capture starts.
                    conn.set_video(
                        Some(VideoAttrs { width: 1920, height: 1080, fps: s.fps, bitrate_kbps: s.bitrate_kbps }),
                        s.mtu_payload,
                    );
                    #[cfg(feature = "screen")]
                    {
                        let screen = ScreenStream::start(s.clone(), Some(conn.video_sink()), Some(repaint.clone()));
                        started = Some(screen.stats.clone());
                        live.screen = Some(screen);
                    }
                    live.audio = s.audio_enabled.then(|| {
                        let mode = EngineMode { playback: false, loopback: s.audio_loopback };
                        let e = AudioEngine::start_with(s.audio.clone(), mode);
                        e.set_sink(Some(conn.audio_sink()));
                        e
                    });
                    info!("go live: connected stream {}", live.key);
                    live.conn = Some(conn);
                }
            }
        }
        for w in &mut self.watches {
            if w.conn.is_none() {
                if let Some((server_id, endpoint, token)) = w.rtc.ready() {
                    let info = VoiceConnectInfo {
                        endpoint,
                        token,
                        session_id: session.clone(),
                        server_id,
                        channel_id: w.channel.0,
                        user_id: me.0,
                        kind: ConnKind::Stream,
                    };
                    // Stream audio is mixed into the normal voice output.
                    w.conn = Some(VoiceConnection::connect(info, voice_audio.clone(), Some(repaint.clone())));
                }
            }
        }
        started
    }

    /// Tell the stream server the real output size once capture knows it.
    pub fn announce_mode(&self, mode: &CameraMode) {
        if let Some(c) = self.live.as_ref().and_then(|l| l.conn.as_ref()) {
            c.set_video(
                Some(VideoAttrs { width: mode.width, height: mode.height, fps: mode.fps, bitrate_kbps: self.settings.bitrate_kbps }),
                self.settings.mtu_payload,
            );
        }
    }

    /// Apply new settings; restarts capture/encoding if we're live.
    pub fn set_settings(&mut self, s: StreamSettings, repaint: Repaint) -> Option<Arc<CaptureStats>> {
        self.settings = s;
        let mut started = None;
        if let Some(live) = self.live.as_mut() {
            if let Some(conn) = &live.conn {
                #[cfg(feature = "screen")]
                {
                    live.screen = None;
                    let screen = ScreenStream::start(self.settings.clone(), Some(conn.video_sink()), Some(repaint.clone()));
                    started = Some(screen.stats.clone());
                    live.screen = Some(screen);
                }
                live.audio = self.settings.audio_enabled.then(|| {
                    let mode = EngineMode { playback: false, loopback: self.settings.audio_loopback };
                    let e = AudioEngine::start_with(self.settings.audio.clone(), mode);
                    e.set_sink(Some(conn.audio_sink()));
                    e
                });
            }
        }
        #[cfg(feature = "screen")]
        if self.preview.is_some() {
            self.preview = Some(ScreenStream::start(self.settings.clone(), None, Some(repaint)));
        }
        #[cfg(not(feature = "screen"))]
        let _ = repaint;
        started
    }

    pub fn set_preview(&mut self, on: bool, repaint: Repaint) {
        #[cfg(feature = "screen")]
        {
            self.preview = on.then(|| ScreenStream::start(self.settings.clone(), None, Some(repaint)));
        }
        #[cfg(not(feature = "screen"))]
        let _ = (on, repaint);
    }

    pub fn preview_stats(&self) -> Option<Arc<CaptureStats>> {
        #[cfg(feature = "screen")]
        return self.preview.as_ref().map(|p| p.stats.clone());
        #[cfg(not(feature = "screen"))]
        None
    }

    pub fn live_handles(&self) -> Option<LiveHandles> {
        let l = self.live.as_ref()?;
        #[cfg(feature = "screen")]
        let stats = l.screen.as_ref().map(|s| s.stats.clone());
        #[cfg(not(feature = "screen"))]
        let stats = None;
        Some(LiveHandles {
            key: l.key.clone(),
            voice: l.conn.as_ref().map(|c| c.shared.clone()),
            stats,
            audio: l.audio.as_ref().map(|a| a.shared.clone()),
        })
    }

    pub fn watch_handles(&self) -> Vec<WatchHandles> {
        self.watches
            .iter()
            .map(|w| WatchHandles { user: w.user, voice: w.conn.as_ref().map(|c| c.shared.clone()) })
            .collect()
    }
}
