//! Message list, rich text rendering and the composer.

use crate::app::RustcordApp;
use crate::backend::Cmd;
use crate::markdown::{self, Seg};
use crate::ui::theme::over;
use crate::ui::{media, widgets};
use eframe::egui::{self, Color32, CornerRadius, RichText, Stroke, TextFormat, Vec2};
use rustcord_discord::{LocalMessageState, Message, Snowflake};

pub fn central(app: &mut RustcordApp, ui: &mut egui::Ui) {
    let k = app.colors;
    egui::CentralPanel::default().frame(egui::Frame::new().fill(k.bg_primary)).show(ui, |ui| {
        let Some(channel) = app.selected_channel else {
            ui.centered_and_justified(|ui| {
                ui.label(RichText::new("Pick a channel").size(18.0).color(k.muted));
            });
            return;
        };
        let ch = app.store.channels.get(&channel).cloned();
        let is_voice = ch.as_ref().map(|c| c.kind().is_voice()).unwrap_or(false);

        egui::Panel::top("chat_header")
            .exact_size(48.0)
            .frame(egui::Frame::new().fill(k.bg_primary).inner_margin(egui::Margin::symmetric(16, 12)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let icon = if is_voice { "🔊" } else if ch.as_ref().map(|c| c.kind().is_private()).unwrap_or(false) { "@" } else { "#" };
                    ui.label(RichText::new(icon).size(18.0).color(k.interactive_muted.gamma_multiply(1.6)));
                    ui.label(RichText::new(app.store.channel_name(channel)).strong().size(15.0).color(k.header));
                    if let Some(topic) = ch.as_ref().and_then(|c| c.topic.clone()).filter(|t| !t.is_empty()) {
                        ui.separator();
                        ui.add(egui::Label::new(RichText::new(topic).size(13.0).color(k.muted)).truncate());
                    }
                });
            });

        // Voice view: video grid on top when you're in this call.
        if is_voice && app.media.channel == Some(channel) {
            egui::Panel::top("voice_view")
                .resizable(true)
                .default_size(ui.available_height() * 0.6)
                .frame(egui::Frame::new().fill(over(k.bg_tertiary, Color32::BLACK)).inner_margin(egui::Margin::same(10)))
                .show(ui, |ui| media::voice_grid(app, ui, channel));
        } else if app.media.channel.is_some() && media::has_video(app) {
            egui::Panel::top("video_strip")
                .exact_size(150.0)
                .frame(egui::Frame::new().fill(over(k.bg_tertiary, Color32::BLACK)).inner_margin(egui::Margin::same(6)))
                .show(ui, |ui| media::video_strip(app, ui));
        }

        egui::Panel::bottom("composer")
            .show_separator_line(false)
            .frame(egui::Frame::new().fill(k.bg_primary).inner_margin(egui::Margin { left: 16, right: 16, top: 2, bottom: 16 }))
            .show(ui, |ui| composer(app, ui, channel));

        egui::CentralPanel::default().frame(egui::Frame::new()).show(ui, |ui| messages(app, ui, channel));
    });
}

fn format_time(ts_ms: u64, with_date: bool) -> String {
    use chrono::{Local, TimeZone};
    match Local.timestamp_millis_opt(ts_ms as i64) {
        chrono::LocalResult::Single(t) => {
            let today = Local::now().date_naive();
            if !with_date {
                t.format("%H:%M").to_string()
            } else if t.date_naive() == today {
                t.format("Today at %H:%M").to_string()
            } else {
                t.format("%d/%m/%Y %H:%M").to_string()
            }
        }
        _ => String::new(),
    }
}

fn messages(app: &mut RustcordApp, ui: &mut egui::Ui, channel: Snowflake) {
    let k = app.colors;
    let me = app.store.me_id();
    let compact = app.cfg.compact_messages;
    let show_images = app.cfg.show_images;

    // Borrow the channel's history out of the store for the duration of the
    // frame instead of cloning it; it goes back in at the end.
    let cm = app.store.messages.remove(&channel).unwrap_or_default();
    let (loading, reached_start) = (cm.loading, cm.reached_start);

    // Row heights depend on wrapping width: drop the cache when it changes.
    let width = ui.available_width();
    if (width - app.row_width).abs() > 0.5 {
        app.row_width = width;
        app.row_heights.clear();
    }

    let stick = std::mem::take(&mut app.scroll_to_bottom);
    let area = egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .stick_to_bottom(true)
        .id_salt(("messages", channel.0));
    let out = area.show_viewport(ui, |ui, viewport| {
        // Rows carry their own padding; zero spacing keeps measured heights
        // and placeholder heights identical.
        ui.spacing_mut().item_spacing.y = 0.0;
        ui.add_space(8.0);
        if reached_start {
            ui.add_space(20.0);
            ui.label(RichText::new(format!("Welcome to #{}", app.store.channel_name(channel))).size(24.0).strong().color(k.header));
            ui.label(RichText::new("This is the start of the channel.").color(k.muted));
            ui.add_space(12.0);
        } else if loading {
            ui.vertical_centered(|ui| ui.spinner());
        }
        if let Some(e) = &cm.error {
            ui.colored_label(k.danger, format!("Couldn't load messages: {e}"));
        }
        // Only lay out rows near the viewport; everything else is a spacer
        // of its last measured height (virtualised list).
        let origin = ui.min_rect().top();
        let keep = viewport.expand2(egui::vec2(0.0, 600.0));
        let mut prev: Option<&Message> = None;
        for m in &cm.messages {
            let grouped = !compact
                && prev.is_some_and(|p| {
                    p.author.id == m.author.id
                        && m.referenced_message.is_none()
                        && m.id.timestamp_ms().saturating_sub(p.id.timestamp_ms()) < 7 * 60 * 1000
                        && p.local != LocalMessageState::Ephemeral
                });
            prev = Some(m);
            let top = ui.cursor().top();
            let rel = top - origin;
            let cached = app.row_heights.get(&(m.id, grouped)).copied();
            match cached {
                Some(h) if rel + h < keep.min.y || rel > keep.max.y => ui.add_space(h),
                _ => {
                    message_row(app, ui, m, grouped, me, show_images);
                    let h = ui.cursor().top() - top;
                    app.row_heights.insert((m.id, grouped), h);
                }
            }
        }
        ui.add_space(12.0);
        if stick {
            ui.scroll_to_cursor(Some(egui::Align::BOTTOM));
        }
    });
    let near_top = out.state.clamped_offset().y < 80.0;
    let has_rows = !cm.messages.is_empty();
    // Put the history back (merging anything that arrived meanwhile).
    match app.store.messages.remove(&channel) {
        Some(added) => {
            let mut cm = cm;
            for m in added.messages {
                if let Err(i) = cm.messages.binary_search_by(|x| x.id.cmp(&m.id)) {
                    cm.messages.insert(i, m);
                }
            }
            app.store.messages.insert(channel, cm);
        }
        None => {
            app.store.messages.insert(channel, cm);
        }
    }
    if near_top && !reached_start && !loading && has_rows {
        app.load_older(channel);
    }
    if app.row_heights.len() > 20_000 {
        app.row_heights.clear();
    }
}

fn message_row(app: &mut RustcordApp, ui: &mut egui::Ui, m: &Message, grouped: bool, me: Snowflake, show_images: bool) {
    let k = app.colors;
    let mentioned = m.author.id != me && m.mentions_user(me);
    let pending = m.local == LocalMessageState::Pending;
    let failed = matches!(m.local, LocalMessageState::Failed(_));
    let frame = egui::Frame::new()
        .inner_margin(egui::Margin { left: 16, right: 48, top: if grouped { 1 } else { 8 }, bottom: 1 })
        .fill(if mentioned { over(k.mention_bg, k.bg_primary) } else { Color32::TRANSPARENT });
    let resp = frame
        .show(ui, |ui| {
            // The list itself runs with zero spacing (see `messages`).
            ui.spacing_mut().item_spacing.y = 4.0;
            ui.set_width(ui.available_width());
            if let Some(r) = &m.referenced_message {
                ui.horizontal(|ui| {
                    ui.add_space(44.0);
                    ui.label(RichText::new("╭").color(k.interactive_muted));
                    ui.label(RichText::new(format!("@{}", r.author_name())).size(12.0).strong().color(k.interactive));
                    let preview: String = r.content.chars().take(90).collect();
                    ui.add(egui::Label::new(RichText::new(preview.replace('\n', " ")).size(12.0).color(k.muted)).truncate());
                });
            }
            ui.horizontal_top(|ui| {
                if grouped {
                    // avatar (40) + gap (8) + item spacing, so text lines up
                    ui.add_space(40.0 + 8.0 + ui.spacing().item_spacing.x);
                } else {
                    widgets::avatar(ui, &mut app.images, &app.backend, &m.author, 40.0, None);
                    ui.add_space(8.0);
                }
                ui.vertical(|ui| {
                    if !grouped {
                        ui.horizontal(|ui| {
                            let name_color = if m.local == LocalMessageState::Ephemeral { k.brand } else { k.header };
                            ui.label(RichText::new(m.author_name()).strong().color(name_color));
                            if m.author.bot {
                                ui.label(RichText::new(if m.local == LocalMessageState::Ephemeral { "PLUGIN" } else { "BOT" }).size(9.5).strong().background_color(k.brand).color(Color32::WHITE));
                            }
                            let ts = if m.local == LocalMessageState::Delivered { m.id.timestamp_ms() } else { chrono::Utc::now().timestamp_millis() as u64 };
                            ui.label(RichText::new(format_time(ts, true)).size(11.5).color(k.muted));
                        });
                    }
                    let text_color = if pending { k.muted } else if failed { k.danger } else { k.text };
                    if !m.content.is_empty() {
                        rich_text(app, ui, &m.content, text_color, m.edited_timestamp.is_some());
                    }
                    if let LocalMessageState::Failed(e) = &m.local {
                        ui.label(RichText::new(format!("Failed to send: {e}")).size(11.0).color(k.danger));
                    }
                    for a in &m.attachments {
                        if show_images && a.is_image() {
                            let key = format!("att:{}", a.id);
                            let url = a.proxy_url.clone().unwrap_or_else(|| a.url.clone());
                            let w = a.width.unwrap_or(400).min(400) as f32;
                            let h = a.height.unwrap_or(300) as f32 * (w / a.width.unwrap_or(400).max(1) as f32);
                            match app.images.get(&app.backend, &key, &url, 800) {
                                Some(t) => {
                                    let r = ui.add(egui::Image::from_texture(&t).max_size(Vec2::new(w, h.min(400.0))).corner_radius(CornerRadius::same(6)).sense(egui::Sense::click()));
                                    if r.clicked() {
                                        ui.ctx().open_url(egui::OpenUrl::new_tab(&a.url));
                                    }
                                }
                                None => {
                                    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, h.min(400.0)), egui::Sense::hover());
                                    ui.painter().rect_filled(rect, CornerRadius::same(6), k.bg_secondary);
                                }
                            }
                        } else {
                            egui::Frame::new().fill(k.bg_secondary).corner_radius(CornerRadius::same(6)).inner_margin(egui::Margin::same(8)).show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label("📄");
                                    ui.hyperlink_to(&a.filename, &a.url);
                                    ui.label(RichText::new(human_size(a.size)).size(11.0).color(k.muted));
                                });
                            });
                        }
                    }
                    for e in &m.embeds {
                        embed(app, ui, e, show_images);
                    }
                });
            });
        })
        .response;

    // Hover actions
    let hovered = ui.rect_contains_pointer(resp.rect);
    if hovered && m.local == LocalMessageState::Delivered {
        ui.painter().rect_filled(resp.rect, CornerRadius::ZERO, k.bg_hover.gamma_multiply(0.5));
        let bar = egui::Rect::from_min_size(resp.rect.right_top() + Vec2::new(-130.0, -10.0), Vec2::new(116.0, 28.0));
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(bar).layout(egui::Layout::right_to_left(egui::Align::Center)));
        egui::Frame::new().fill(over(k.bg_floating, Color32::BLACK)).corner_radius(CornerRadius::same(6)).stroke(Stroke::new(1.0, k.bg_accent)).show(&mut child, |ui| {
            ui.horizontal(|ui| {
                if m.author.id == me {
                    if widgets::icon_button(ui, "🗑", "Delete", Some(k.danger)).clicked() {
                        app.backend.send(Cmd::DeleteMessage { channel: m.channel_id, message: m.id });
                    }
                    if widgets::icon_button(ui, "✏", "Edit", None).clicked() {
                        app.editing = Some((m.id, m.content.clone()));
                    }
                }
                if widgets::icon_button(ui, "📋", "Copy text", None).clicked() {
                    ui.ctx().copy_text(m.content.clone());
                }
                if widgets::icon_button(ui, "↩", "Reply", None).clicked() {
                    app.reply_to = Some(m.id);
                }
            });
        });
    }
}

fn human_size(n: u64) -> String {
    match n {
        0..=1023 => format!("{n} B"),
        1024..=1_048_575 => format!("{:.1} KB", n as f64 / 1024.0),
        _ => format!("{:.1} MB", n as f64 / 1_048_576.0),
    }
}

fn embed(app: &mut RustcordApp, ui: &mut egui::Ui, e: &rustcord_discord::Embed, show_images: bool) {
    let k = app.colors;
    let bar = e.color.map(|c| Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8)).unwrap_or(k.bg_accent);
    let r = egui::Frame::new()
        .fill(over(k.bg_secondary, k.bg_primary))
        .corner_radius(CornerRadius::same(4))
        .inner_margin(egui::Margin { left: 14, right: 12, top: 8, bottom: 10 })
        .show(ui, |ui| {
            ui.set_max_width(520.0);
            if let Some(a) = &e.author {
                ui.label(RichText::new(&a.text).size(12.0).strong().color(k.header));
            }
            if let Some(t) = &e.title {
                match &e.url {
                    Some(u) => {
                        ui.hyperlink_to(RichText::new(t).strong(), u);
                    }
                    None => {
                        ui.label(RichText::new(t).strong().color(k.header));
                    }
                }
            }
            if let Some(d) = &e.description {
                rich_text(app, ui, d, k.text, false);
            }
            for f in &e.fields {
                ui.label(RichText::new(&f.name).strong().size(12.5).color(k.header));
                rich_text(app, ui, &f.value, k.text, false);
            }
            if show_images {
                if let Some(img) = e.image.as_ref().or(e.thumbnail.as_ref()) {
                    let url = img.proxy_url.clone().unwrap_or_else(|| img.url.clone());
                    if let Some(t) = app.images.get(&app.backend, &format!("embed:{url}"), &url, 640) {
                        ui.add(egui::Image::from_texture(&t).max_size(Vec2::new(400.0, 300.0)).corner_radius(CornerRadius::same(4)));
                    }
                }
            }
            if let Some(f) = &e.footer {
                ui.label(RichText::new(&f.text).size(11.0).color(k.muted));
            }
        });
    let rect = r.response.rect;
    ui.painter().rect_filled(egui::Rect::from_min_max(rect.left_top(), egui::pos2(rect.left() + 4.0, rect.bottom())), CornerRadius::same(2), bar);
}

/// Render Discord markdown inline with wrapping.
pub fn rich_text(app: &mut RustcordApp, ui: &mut egui::Ui, text: &str, color: Color32, edited: bool) {
    let k = app.colors;
    let segs = markdown::parse(text);
    let job_style = |st: &markdown::Style| -> TextFormat {
        let size = match st.heading {
            1 => 22.0,
            2 => 19.0,
            3 => 16.5,
            _ if st.subtext => 12.0,
            _ => 15.0,
        };
        let mut f = TextFormat {
            font_id: if st.code { egui::FontId::monospace(13.5) } else { egui::FontId::proportional(size) },
            color: if st.subtext || st.quote { k.muted } else { color },
            ..Default::default()
        };
        if st.bold || st.heading > 0 {
            f.color = if st.heading > 0 { k.header } else { over(k.header, color).gamma_multiply(1.0) };
        }
        f.italics = st.italic;
        if st.underline {
            f.underline = Stroke::new(1.0, f.color);
        }
        if st.strike {
            f.strikethrough = Stroke::new(1.0, f.color);
        }
        if st.code {
            f.background = over(k.bg_tertiary, k.bg_primary);
        }
        if st.spoiler {
            f.background = k.bg_accent;
            f.color = k.bg_accent;
        }
        f
    };

    let mut job = egui::text::LayoutJob::default();
    let flush = |ui: &mut egui::Ui, job: &mut egui::text::LayoutJob| {
        if !job.sections.is_empty() {
            let j = std::mem::take(job);
            ui.label(j);
        }
    };
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for seg in &segs {
            match seg {
                Seg::Text(t, st) => {
                    let fmt = job_style(st);
                    job.append(t, 0.0, fmt);
                }
                Seg::Newline => {
                    flush(ui, &mut job);
                    ui.end_row();
                }
                Seg::Link(url) => {
                    flush(ui, &mut job);
                    ui.hyperlink_to(RichText::new(url).color(k.link), url);
                }
                Seg::UserMention(id) => {
                    flush(ui, &mut job);
                    let name = app.store.user_name(Snowflake(*id));
                    ui.label(RichText::new(format!("@{name}")).color(k.link).background_color(k.link.gamma_multiply(0.18)));
                }
                Seg::ChannelMention(id) => {
                    flush(ui, &mut job);
                    let name = app.store.channel_name(Snowflake(*id));
                    if ui.add(egui::Label::new(RichText::new(format!("#{name}")).color(k.link).background_color(k.link.gamma_multiply(0.18))).sense(egui::Sense::click())).clicked()
                        && app.store.channels.contains_key(&Snowflake(*id)) {
                            app.select_channel(Snowflake(*id));
                        }
                }
                Seg::RoleMention => {
                    flush(ui, &mut job);
                    ui.label(RichText::new("@role").color(k.link));
                }
                Seg::Everyone(w) => {
                    flush(ui, &mut job);
                    ui.label(RichText::new(w).color(k.link).background_color(k.link.gamma_multiply(0.18)));
                }
                Seg::Emoji { name, id, animated } => {
                    flush(ui, &mut job);
                    let url = format!("https://cdn.discordapp.com/emojis/{id}.{}?size=48", if *animated { "gif" } else { "png" });
                    match app.images.get(&app.backend, &format!("emoji:{id}"), &url, 48) {
                        Some(t) => {
                            ui.add(egui::Image::from_texture(&t).fit_to_exact_size(Vec2::splat(22.0))).on_hover_text(format!(":{name}:"));
                        }
                        None => {
                            ui.label(RichText::new(format!(":{name}:")).color(k.muted));
                        }
                    }
                }
                Seg::Timestamp(ts, _) => {
                    flush(ui, &mut job);
                    ui.label(RichText::new(format_time((*ts as u64) * 1000, true)).background_color(k.bg_accent).color(k.text));
                }
                Seg::CodeBlock { code, .. } => {
                    flush(ui, &mut job);
                    ui.end_row();
                    egui::Frame::new()
                        .fill(over(k.bg_tertiary, k.bg_primary))
                        .corner_radius(CornerRadius::same(4))
                        .inner_margin(egui::Margin::same(8))
                        .show(ui, |ui| {
                            ui.set_min_width(ui.available_width().min(560.0));
                            ui.label(RichText::new(code).monospace().size(13.0).color(k.text));
                        });
                    ui.end_row();
                }
            }
        }
        if edited {
            job.append(" (edited)", 0.0, TextFormat { font_id: egui::FontId::proportional(10.5), color: k.muted, ..Default::default() });
        }
        flush(ui, &mut job);
    });
}

fn composer(app: &mut RustcordApp, ui: &mut egui::Ui, channel: Snowflake) {
    let k = app.colors;
    // typing indicator
    let typing: Vec<String> = app
        .store
        .typing
        .get(&channel)
        .map(|l| l.iter().map(|(u, _)| app.store.user_name(*u)).collect())
        .unwrap_or_default();
    ui.horizontal(|ui| {
        ui.set_min_height(18.0);
        if !typing.is_empty() {
            let text = match typing.len() {
                1 => format!("{} is typing…", typing[0]),
                2 => format!("{} and {} are typing…", typing[0], typing[1]),
                n => format!("{n} people are typing…"),
            };
            ui.label(RichText::new(text).size(12.0).strong().color(k.text));
        }
    });

    if let Some(reply) = app.reply_to {
        let who = app
            .store
            .messages
            .get(&channel)
            .and_then(|c| c.messages.iter().find(|m| m.id == reply))
            .map(|m| m.author_name().to_string())
            .unwrap_or_default();
        egui::Frame::new().fill(k.bg_secondary).corner_radius(CornerRadius { nw: 8, ne: 8, sw: 0, se: 0 }).inner_margin(egui::Margin::symmetric(12, 6)).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("Replying to {who}")).size(12.5).color(k.muted));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("✖").clicked() {
                        app.reply_to = None;
                    }
                });
            });
        });
    }

    // editing an existing message
    if let Some((mid, mut text)) = app.editing.take() {
        let mut keep = true;
        egui::Frame::new().fill(k.bg_input).corner_radius(CornerRadius::same(8)).inner_margin(egui::Margin::symmetric(12, 8)).show(ui, |ui| {
            ui.label(RichText::new("Editing message — Enter to save, Esc to cancel").size(11.5).color(k.muted));
            let r = ui.add(egui::TextEdit::multiline(&mut text).desired_rows(1).desired_width(f32::INFINITY).frame(egui::Frame::NONE));
            r.request_focus();
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                keep = false;
            } else if r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift) {
                app.backend.send(Cmd::EditMessage { channel, message: mid, content: text.trim().to_string() });
                keep = false;
            }
        });
        if keep {
            app.editing = Some((mid, text));
        }
        return;
    }

    let mut draft = app.drafts.remove(&channel).unwrap_or_default();
    let name = app.store.channel_name(channel);
    let mut submit = false;
    egui::Frame::new().fill(k.bg_input).corner_radius(CornerRadius::same(8)).inner_margin(egui::Margin::symmetric(14, 10)).show(ui, |ui| {
        let resp = ui.add(
            egui::TextEdit::multiline(&mut draft)
                .hint_text(format!("Message #{name}"))
                .desired_rows(1)
                .desired_width(f32::INFINITY)
                .frame(egui::Frame::NONE)
                .text_color(k.text)
                .lock_focus(true),
        );
        if resp.changed() && !draft.starts_with('/') && !draft.is_empty() {
            app.typing(channel);
        }
        if resp.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift) {
            // TextEdit already inserted the newline; drop it and send.
            if draft.ends_with('\n') {
                draft.pop();
            }
            submit = true;
        }
        if app.selected_channel.is_some() && !resp.has_focus() && app.editing.is_none() && !app.settings.open {
            // Typing anywhere focuses the composer.
            let typed = ui.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Text(_))));
            if typed {
                resp.request_focus();
            }
        }
    });

    // Slash command suggestions (built-ins + plugins)
    if let Some(prefix) = draft.strip_prefix('/').filter(|p| !p.contains(' ')) {
        let mut list: Vec<(String, String, String)> = vec![
            ("shrug".into(), "Appends ¯\\_(ツ)_/¯".into(), "built-in".into()),
            ("tableflip".into(), "Appends (╯°□°)╯︵ ┻━┻".into(), "built-in".into()),
            ("unflip".into(), "Appends ┬─┬ノ( º _ ºノ)".into(), "built-in".into()),
            ("spoiler".into(), "Marks your message as a spoiler".into(), "built-in".into()),
        ];
        let plugins = app.plugins.plugins();
        for c in app.plugins.commands() {
            let pname = plugins.iter().find(|p| p.id == c.plugin).map(|p| p.name.clone()).unwrap_or(c.plugin.clone());
            list.push((c.name, c.description, pname));
        }
        list.retain(|(n, _, _)| n.starts_with(prefix));
        if !list.is_empty() {
            egui::Frame::new().fill(over(k.bg_floating, Color32::BLACK)).corner_radius(CornerRadius::same(8)).inner_margin(egui::Margin::same(6)).show(ui, |ui| {
                for (n, d, src) in list.into_iter().take(8) {
                    let r = ui.add(egui::Button::new(RichText::new(format!("/{n}  ")).strong()).frame(false));
                    ui.horizontal(|ui| {
                        ui.add_space(14.0);
                        ui.label(RichText::new(format!("{d} · {src}")).size(11.5).color(k.muted));
                    });
                    if r.clicked() {
                        draft = format!("/{n} ");
                    }
                }
            });
        }
    }

    if submit {
        let text = std::mem::take(&mut draft);
        app.submit(channel, text);
    }
    app.drafts.insert(channel, draft);
}
