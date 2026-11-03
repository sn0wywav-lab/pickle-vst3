//! Server rail, channel list, voice status and user panel.

use crate::app::RustcordApp;
use crate::backend::Cmd;
use crate::ui::theme::over;
use crate::ui::widgets::{self, badge, icon_button};
use eframe::egui::{self, Color32, CornerRadius, RichText, Sense, Stroke, Vec2};
use rustcord_discord::{ChannelKind, Snowflake};
use rustcord_voice::VoiceStatus;
use std::sync::atomic::Ordering;
use std::time::Duration;

/// Theme background image, painted behind the (possibly translucent) panels.
pub fn background(app: &mut RustcordApp, ui: &mut egui::Ui) {
    let Some(url) = app.theme.background_image.clone() else { return };
    let key = format!("bg:{url}");
    if let Some(t) = app.images.get(&app.backend, &key, &url, 2560) {
        let rect = ui.max_rect();
        // cover-fit
        let img = t.size_vec2();
        let scale = (rect.width() / img.x).max(rect.height() / img.y);
        let size = img * scale;
        let draw = egui::Rect::from_center_size(rect.center(), size);
        ui.painter().image(t.id(), draw, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
    }
}

pub fn guild_rail(app: &mut RustcordApp, ui: &mut egui::Ui) {
    let k = app.colors;
    egui::Panel::left("guild_rail")
        .exact_size(72.0)
        .resizable(false)
        .show_separator_line(false)
        .frame(egui::Frame::new().fill(k.bg_tertiary).inner_margin(egui::Margin::symmetric(12, 10)))
        .show(ui, |ui| {
            egui::ScrollArea::vertical().scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 8.0;
                // Home / DMs
                let selected = app.selected_guild.is_none();
                let resp = rail_button(ui, selected, false, k, |ui, rect| {
                    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "💬", egui::FontId::proportional(22.0), Color32::WHITE);
                });
                let dm = app.store.dm_unread();
                if dm > 0 {
                    badge(ui.painter(), resp.rect, dm, k.danger);
                }
                if resp.on_hover_text("Direct Messages").clicked() {
                    app.select_guild(None);
                }
                let sep = ui.allocate_exact_size(Vec2::new(48.0, 2.0), Sense::hover()).0;
                ui.painter().rect_filled(sep.shrink2(Vec2::new(10.0, 0.0)), CornerRadius::same(1), k.bg_accent);

                let guilds = app.store.guilds.clone();
                for g in guilds {
                    let selected = app.selected_guild == Some(g.id);
                    let unread = app.store.guild_has_unread(g.id);
                    let icon = g.icon_url(96).and_then(|url| app.images.get(&app.backend, &format!("guild:{}:{}", g.id, g.icon.as_deref().unwrap_or("")), &url, 96));
                    let resp = rail_button(ui, selected, unread, k, |ui, rect| match &icon {
                        Some(t) => {
                            egui::Image::from_texture(t)
                                .corner_radius(CornerRadius::same(if selected { 14 } else { 24 }))
                                .paint_at(ui, rect);
                        }
                        None => {
                            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, g.initials(), egui::FontId::proportional(15.0), k.header);
                        }
                    });
                    let mentions = app.store.guild_mentions(g.id);
                    if mentions > 0 {
                        badge(ui.painter(), resp.rect, mentions, k.danger);
                    }
                    if resp.on_hover_text(&g.name).clicked() {
                        app.select_guild(Some(g.id));
                    }
                }
            });
        });
}

fn rail_button(
    ui: &mut egui::Ui,
    selected: bool,
    unread: bool,
    k: crate::ui::theme::Colors,
    paint: impl FnOnce(&mut egui::Ui, egui::Rect),
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(48.0), Sense::click());
    let round = if selected || resp.hovered() { 14 } else { 24 };
    let fill = if selected { k.brand } else if resp.hovered() { over(k.brand, k.bg_secondary).gamma_multiply(0.9) } else { over(k.bg_secondary, Color32::BLACK) };
    ui.painter().rect_filled(rect, CornerRadius::same(round), fill);
    paint(ui, rect);
    // selection pill on the left edge
    let pill_h = if selected { 36.0 } else if resp.hovered() { 18.0 } else if unread { 8.0 } else { 0.0 };
    if pill_h > 0.0 {
        let pill = egui::Rect::from_center_size(egui::pos2(rect.left() - 10.0, rect.center().y), Vec2::new(4.0, pill_h));
        ui.painter().rect_filled(pill, CornerRadius::same(2), k.header);
    }
    resp
}

pub fn channel_panel(app: &mut RustcordApp, ui: &mut egui::Ui) {
    let k = app.colors;
    egui::Panel::left("channels")
        .default_size(250.0)
        .size_range(190.0..=420.0)
        .resizable(true)
        .show_separator_line(false)
        .frame(egui::Frame::new().fill(k.bg_secondary))
        .show(ui, |ui| {
            // Bottom-up: user panel, voice panel, then the list fills the rest.
            egui::Panel::bottom("user_panel")
                .exact_size(56.0)
                .show_separator_line(false)
                .frame(egui::Frame::new().fill(k.bg_secondary_alt).inner_margin(egui::Margin::symmetric(8, 8)))
                .show(ui, |ui| user_panel(app, ui));
            if app.media.channel.is_some() || app.media.voice.is_some() {
                egui::Panel::bottom("voice_panel")
                    .show_separator_line(false)
                    .frame(egui::Frame::new().fill(k.bg_secondary_alt).inner_margin(egui::Margin::symmetric(10, 8)))
                    .show(ui, |ui| voice_panel(app, ui));
            }
            egui::Panel::top("guild_header")
                .exact_size(48.0)
                .frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(14, 12)))
                .show(ui, |ui| {
                    let title = match app.selected_guild {
                        Some(g) => app.store.guilds.iter().find(|x| x.id == g).map(|g| g.name.clone()).unwrap_or_default(),
                        None => "Direct Messages".into(),
                    };
                    ui.label(RichText::new(title).strong().size(15.0).color(k.header));
                });
            egui::CentralPanel::default().frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(8, 6))).show(ui, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| match app.selected_guild {
                    Some(g) => guild_channels(app, ui, g),
                    None => dm_list(app, ui),
                });
            });
        });
}

fn channel_row(ui: &mut egui::Ui, k: crate::ui::theme::Colors, icon: &str, name: &str, selected: bool, unread: bool, mentions: u32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 32.0), Sense::click());
    if selected {
        ui.painter().rect_filled(rect, CornerRadius::same(k.radius), over(k.bg_selected, k.bg_secondary));
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(k.radius), over(k.bg_hover, k.bg_secondary));
    }
    let color = if selected { k.interactive_active } else if unread { k.header } else if resp.hovered() { k.interactive_hover } else { k.interactive };
    ui.painter().text(rect.left_center() + Vec2::new(10.0, 0.0), egui::Align2::LEFT_CENTER, icon, egui::FontId::proportional(16.0), k.interactive_muted.gamma_multiply(1.6));
    let font = egui::FontId::proportional(14.5);
    let galley = ui.painter().layout(name.to_string(), font, color, rect.width() - 60.0);
    ui.painter().galley(egui::pos2(rect.left() + 32.0, rect.center().y - galley.size().y / 2.0), galley, color);
    if unread && !selected {
        ui.painter().circle_filled(egui::pos2(rect.left() + 2.0, rect.center().y), 3.0, k.header);
    }
    if mentions > 0 {
        badge(ui.painter(), rect.shrink2(Vec2::new(6.0, 6.0)), mentions, k.danger);
    }
    resp
}

fn guild_channels(app: &mut RustcordApp, ui: &mut egui::Ui, guild: Snowflake) {
    let k = app.colors;
    // Collect first: rendering needs &mut app.
    struct Row {
        id: Snowflake,
        kind: ChannelKind,
        name: String,
    }
    type Group = (Option<(Snowflake, String)>, Vec<Row>);
    let groups: Vec<Group> = app
        .store
        .grouped_channels(guild)
        .into_iter()
        .map(|g| {
            (
                g.category.map(|c| (c.id, c.name.clone().unwrap_or_default().to_uppercase())),
                g.channels
                    .iter()
                    .map(|c| Row { id: c.id, kind: c.kind(), name: c.name.clone().unwrap_or_default() })
                    .collect(),
            )
        })
        .collect();
    for (cat, rows) in groups {
        let mut collapsed = false;
        if let Some((cid, cname)) = &cat {
            collapsed = app.collapsed.contains(cid);
            ui.add_space(10.0);
            let arrow = if collapsed { "▸" } else { "▾" };
            let r = ui.add(egui::Label::new(RichText::new(format!("{arrow} {cname}")).size(11.5).strong().color(k.interactive)).sense(Sense::click()));
            if r.clicked() {
                if collapsed {
                    app.collapsed.remove(cid);
                } else {
                    app.collapsed.insert(*cid);
                }
            }
        }
        for row in rows {
            let selected = app.selected_channel == Some(row.id);
            let unread = app.store.unread.contains(&row.id);
            if collapsed && !selected && !unread && !row.kind.is_voice() {
                continue;
            }
            if row.kind.is_voice() {
                let in_this = app.media.channel == Some(row.id);
                let members: Vec<_> = app.store.voice_members(row.id).into_iter().cloned().collect();
                if collapsed && members.is_empty() {
                    continue;
                }
                let resp = channel_row(ui, k, "🔊", &row.name, in_this, false, 0);
                if resp.clicked() {
                    if !in_this {
                        app.join_voice(Some(guild), row.id);
                    }
                    app.select_channel(row.id);
                }
                resp.on_hover_text(if in_this { "Connected — open voice view" } else { "Click to join" });
                for vs in members {
                    voice_member_row(app, ui, &vs);
                }
            } else {
                let icon = match row.kind {
                    ChannelKind::Announcement => "📢",
                    ChannelKind::Forum => "💬",
                    ChannelKind::Thread => "↳",
                    _ => "#",
                };
                if channel_row(ui, k, icon, &row.name, selected, unread, app.store.mention_counts.get(&row.id).copied().unwrap_or(0)).clicked() {
                    app.select_channel(row.id);
                }
            }
        }
    }
}

fn voice_member_row(app: &mut RustcordApp, ui: &mut egui::Ui, vs: &rustcord_discord::VoiceState) {
    let k = app.colors;
    let user = app.store.users.get(&vs.user_id).cloned().unwrap_or(rustcord_discord::User { id: vs.user_id, ..Default::default() });
    let me = app.store.me_id();
    let speaking = match &app.media.engine {
        Some(e) if app.media.channel == vs.channel_id => {
            if vs.user_id == me {
                e.transmitting.load(Ordering::Relaxed) && app.input_meter_db > -45.0
            } else {
                e.is_user_speaking(vs.user_id.0, Duration::from_millis(250))
            }
        }
        _ => false,
    };
    ui.horizontal(|ui| {
        ui.add_space(30.0);
        let ring = speaking.then_some(k.positive);
        widgets::avatar(ui, &mut app.images, &app.backend, &user, 22.0, ring);
        let name = vs.member.as_ref().and_then(|m| m.nick.clone()).unwrap_or_else(|| user.display_name().to_string());
        let resp = ui.add(egui::Label::new(RichText::new(name).size(13.5).color(if speaking { k.header } else { k.interactive })).sense(Sense::click()));
        if vs.self_video {
            ui.label(RichText::new("📷").size(12.0).color(k.interactive));
        }
        if vs.self_stream {
            let watching = app.media.watching.iter().any(|w| w.user == vs.user_id);
            let badge = RichText::new(if watching { " WATCHING " } else { " LIVE " }).size(9.5).strong().color(Color32::WHITE).background_color(k.danger);
            let r = ui.add(egui::Label::new(badge).sense(Sense::click()));
            if vs.user_id != me {
                let r = r.on_hover_text(if watching { "Stop watching" } else { "Watch stream" });
                if r.clicked() {
                    app.backend.send(if watching { Cmd::StopWatching(vs.user_id) } else { Cmd::WatchStream(vs.user_id) });
                }
            }
        }
        if vs.self_deaf || vs.deaf {
            ui.label(RichText::new("🔇").size(12.0).color(k.danger));
        } else if vs.self_mute || vs.mute {
            ui.label(RichText::new("🎤").size(12.0).color(k.danger)).on_hover_text("Muted");
        }
        if vs.user_id != me {
            resp.context_menu(|ui| {
                let uid = vs.user_id.0;
                let mut vol = app.cfg.user_volumes.get(&uid).copied().unwrap_or(1.0) * 100.0;
                ui.label("User volume");
                if ui.add(egui::Slider::new(&mut vol, 0.0..=400.0).suffix("%")).changed() {
                    app.cfg.user_volumes.insert(uid, vol / 100.0);
                    app.backend.send(Cmd::UserVolume(uid, vol / 100.0));
                    app.mark_dirty();
                }
            });
        }
    });
}

fn dm_list(app: &mut RustcordApp, ui: &mut egui::Ui) {
    let k = app.colors;
    ui.label(RichText::new("DIRECT MESSAGES").size(11.5).strong().color(k.interactive));
    ui.add_space(4.0);
    let ids: Vec<Snowflake> = app.store.private_channels.iter().copied().take(300).collect();
    for id in ids {
        let name = app.store.channel_name(id);
        let selected = app.selected_channel == Some(id);
        let unread = app.store.unread.contains(&id);
        let first_user = app
            .store
            .channels
            .get(&id)
            .and_then(|c| c.recipient_ids.first())
            .and_then(|u| app.store.users.get(u))
            .cloned();
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 42.0), Sense::click());
        if selected {
            ui.painter().rect_filled(rect, CornerRadius::same(k.radius), over(k.bg_selected, k.bg_secondary));
        } else if resp.hovered() {
            ui.painter().rect_filled(rect, CornerRadius::same(k.radius), over(k.bg_hover, k.bg_secondary));
        }
        let av = egui::Rect::from_center_size(egui::pos2(rect.left() + 22.0, rect.center().y), Vec2::splat(32.0));
        match &first_user {
            Some(u) => widgets::paint_avatar(ui, &mut app.images, &app.backend, u, av, None),
            None => {
                ui.painter().circle_filled(av.center(), 16.0, k.bg_accent);
            }
        }
        let color = if selected || unread { k.header } else { k.interactive };
        let galley = ui.painter().layout(name, egui::FontId::proportional(14.5), color, rect.width() - 60.0);
        ui.painter().galley(egui::pos2(rect.left() + 46.0, rect.center().y - galley.size().y / 2.0), galley, color);
        let m = app.store.mention_counts.get(&id).copied().unwrap_or(0);
        if m > 0 {
            badge(ui.painter(), rect.shrink(8.0), m, k.danger);
        }
        if resp.clicked() {
            app.select_channel(id);
        }
    }
}

fn voice_panel(app: &mut RustcordApp, ui: &mut egui::Ui) {
    let k = app.colors;
    let status = app.media.voice.as_ref().map(|v| v.status());
    let (text, color) = match &status {
        Some(VoiceStatus::Connected) => ("Voice Connected", k.positive),
        Some(VoiceStatus::Reconnecting(_)) => ("Reconnecting…", k.warning),
        Some(VoiceStatus::Disconnected(_)) => ("Disconnected", k.danger),
        _ => ("Connecting…", k.warning),
    };
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(RichText::new(text).strong().color(color));
            let chan = app.media.channel.map(|c| app.store.channel_name(c)).unwrap_or_default();
            let guild = app.media.guild.and_then(|g| app.store.guilds.iter().find(|x| x.id == g)).map(|g| g.name.clone());
            ui.label(RichText::new(match guild { Some(g) => format!("{chan} / {g}"), None => chan }).size(12.0).color(k.muted));
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if icon_button(ui, "☎", "Disconnect", Some(k.danger)).clicked() {
                app.backend.send(Cmd::LeaveVoice);
            }
        });
    });
    if let Some(v) = &app.media.voice {
        let ping = v.ping_ms.load(Ordering::Relaxed);
        let dave = v.dave_version.load(Ordering::Relaxed);
        let mode = v.mode.lock().map(|m| m.as_str()).unwrap_or("…");
        let s = &app.cfg.audio;
        let line = format!(
            "{} ms · {} kb/s ({:.0} live) · {} · {} kHz",
            ping,
            s.effective_bitrate_kbps(),
            app.bitrate_kbps,
            if s.channels == rustcord_voice::Channels::Stereo { "stereo" } else { "mono" },
            s.bandwidth.sample_rate_khz(),
        );
        let resp = ui.label(RichText::new(line).size(11.0).color(k.muted));
        let code = v.privacy_code.lock().clone();
        resp.on_hover_text(format!(
            "Transport: {mode}\nEnd-to-end encryption: {}\n{}",
            if dave > 0 { format!("DAVE v{dave}") } else { "off".into() },
            code.map(|c| format!("Privacy code: {c}")).unwrap_or_default()
        ));
        if let Some(VoiceStatus::Disconnected(reason)) = &status {
            ui.label(RichText::new(reason).size(11.0).color(k.danger));
        }
    }
    if let Some(live) = app.media.live.clone() {
        let mode = live.stats.as_ref().and_then(|s| s.mode.lock().clone());
        let err = live.stats.as_ref().and_then(|s| s.error.lock().clone());
        let enc = live.stats.as_ref().map(|s| s.encode_us.load(Ordering::Relaxed)).unwrap_or(0);
        let status = live.voice.as_ref().map(|v| v.status());
        let text = match (&err, &mode, &status) {
            (Some(e), _, _) => format!("Stream error: {e}"),
            (_, Some(m), Some(VoiceStatus::Connected)) => format!(
                "LIVE {}×{} @ {} fps · {} Mb/s · encode {:.1} ms",
                m.width,
                m.height,
                m.fps,
                app.cfg.stream.bitrate_kbps / 1000,
                enc as f32 / 1000.0
            ),
            _ => "Starting stream…".into(),
        };
        let audio = live.audio.as_ref().map(|a| format!(" · audio {:.0} dB", a.take_input_peak_db().max(-60.0))).unwrap_or_default();
        ui.label(RichText::new(format!("{text}{audio}")).size(11.0).color(if err.is_some() { k.danger } else { k.danger.gamma_multiply(0.9) }))
            .on_hover_text(format!("Stream key: {}", live.key));
    }
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        if app.media.live.is_some() {
            if ui.add(egui::Button::new(RichText::new("⏹ Stop stream").color(Color32::WHITE)).fill(k.danger)).clicked() {
                app.backend.send(Cmd::GoLive(false));
            }
        } else if ui.button("🖥 Go Live").clicked() {
            app.open_golive();
        }
        let cam = app.media.camera_on;
        let label = if cam { "📷 Camera on" } else { "📷 Camera" };
        let b = egui::Button::new(RichText::new(label).color(if cam { Color32::WHITE } else { k.interactive }))
            .fill(if cam { k.brand } else { over(k.bg_accent, k.bg_secondary_alt) });
        if ui.add(b).clicked() {
            app.backend.send(Cmd::SetCamera(!cam));
        }
        if let Some(ch) = app.media.channel {
            if ui.button("👥 Voice view").clicked() {
                app.select_channel(ch);
            }
        }
    });
}

fn user_panel(app: &mut RustcordApp, ui: &mut egui::Ui) {
    let k = app.colors;
    let me = app.store.me.clone().unwrap_or_default();
    ui.horizontal(|ui| {
        let speaking = app.media.engine.as_ref().map(|e| e.transmitting.load(Ordering::Relaxed) && app.input_meter_db > -45.0).unwrap_or(false);
        widgets::avatar(ui, &mut app.images, &app.backend, &me, 34.0, speaking.then_some(k.positive));
        ui.vertical(|ui| {
            ui.add_space(2.0);
            ui.label(RichText::new(me.display_name()).strong().size(13.5).color(k.header));
            let status = match &app.gateway_status {
                rustcord_discord::GatewayStatus::Connected => ("Online".to_string(), k.positive),
                rustcord_discord::GatewayStatus::Reconnecting(r) => (format!("Reconnecting ({r})"), k.warning),
                rustcord_discord::GatewayStatus::Connecting => ("Connecting…".into(), k.warning),
                rustcord_discord::GatewayStatus::Fatal(e) => (e.clone(), k.danger),
            };
            ui.label(RichText::new(status.0).size(11.0).color(status.1));
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if icon_button(ui, "⚙", "Settings", None).clicked() {
                app.settings.open = !app.settings.open;
            }
            let deaf = app.media.deafened;
            if icon_button(ui, if deaf { "🔇" } else { "🎧" }, if deaf { "Undeafen" } else { "Deafen" }, deaf.then_some(k.danger)).clicked() {
                app.backend.send(Cmd::SetDeaf(!deaf));
            }
            let muted = app.media.muted;
            if icon_button(ui, "🎤", if muted { "Unmute" } else { "Mute" }, muted.then_some(k.danger)).clicked() {
                app.backend.send(Cmd::SetMute(!muted));
            }
        });
    });
    let _ = Stroke::NONE;
}
