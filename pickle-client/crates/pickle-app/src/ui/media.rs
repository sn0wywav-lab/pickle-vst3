//! Voice call view: participant tiles with live camera video.

use crate::app::PickleApp;
use crate::ui::theme::over;
use crate::ui::widgets;
use eframe::egui::{self, Color32, CornerRadius, RichText, Stroke, Vec2};
use pickle_discord::{Snowflake, User};
use pickle_voice::video::VideoFrame;
use std::sync::atomic::Ordering;
use std::time::Duration;

/// Key used for our own camera preview in the texture map.
const SELF_KEY: u64 = u64::MAX;

pub fn has_video(app: &PickleApp) -> bool {
    let remote = app.media.voice.as_ref().map(|v| !v.remote_video.lock().is_empty()).unwrap_or(false);
    remote || local_preview(app).is_some()
}

fn local_preview(app: &PickleApp) -> Option<VideoFrame> {
    #[cfg(feature = "camera")]
    return app.media.camera.as_ref().and_then(|c| c.preview.lock().clone());
    #[cfg(not(feature = "camera"))]
    {
        let _ = app;
        None
    }
}

/// Upload a frame if it changed and return its texture.
pub fn texture_for(app: &mut PickleApp, ctx: &egui::Context, key: u64, frame: &VideoFrame) -> egui::TextureHandle {
    let size = [frame.width as usize, frame.height as usize];
    match app.video_tex.get_mut(&key) {
        Some((serial, tex)) if *serial == frame.serial => tex.clone(),
        Some((serial, tex)) => {
            tex.set(egui::ColorImage::from_rgba_unmultiplied(size, &frame.rgba), egui::TextureOptions::LINEAR);
            *serial = frame.serial;
            tex.clone()
        }
        None => {
            let tex = ctx.load_texture(format!("video:{key}"), egui::ColorImage::from_rgba_unmultiplied(size, &frame.rgba), egui::TextureOptions::LINEAR);
            app.video_tex.insert(key, (frame.serial, tex.clone()));
            tex
        }
    }
}

struct Tile {
    user: User,
    key: u64,
    frame: Option<VideoFrame>,
    speaking: bool,
    muted: bool,
}

fn tiles(app: &PickleApp, channel: Snowflake) -> Vec<Tile> {
    let me = app.store.me_id();
    let remote = app.media.voice.as_ref().map(|v| v.remote_video.lock().clone()).unwrap_or_default();
    let mut out = Vec::new();
    for vs in app.store.voice_members(channel) {
        let user = app.store.users.get(&vs.user_id).cloned().unwrap_or(User { id: vs.user_id, ..Default::default() });
        let is_me = vs.user_id == me;
        let frame = if is_me { local_preview(app) } else { remote.get(&vs.user_id.0).cloned() };
        let speaking = match &app.media.engine {
            Some(e) if is_me => e.transmitting.load(Ordering::Relaxed) && app.input_meter_db > -45.0,
            Some(e) => e.is_user_speaking(vs.user_id.0, Duration::from_millis(250)),
            None => false,
        };
        out.push(Tile {
            key: if is_me { SELF_KEY } else { vs.user_id.0 },
            user,
            frame,
            speaking,
            muted: vs.self_mute || vs.mute,
        });
    }
    // If our own voice state hasn't arrived yet, still show ourselves.
    if !out.iter().any(|t| t.key == SELF_KEY) {
        if let Some(me_user) = app.store.me.clone() {
            out.insert(0, Tile { key: SELF_KEY, user: me_user, frame: local_preview(app), speaking: false, muted: app.media.muted });
        }
    }
    // Video first.
    out.sort_by_key(|t| t.frame.is_none());
    out
}

fn draw_tile(app: &mut PickleApp, ui: &mut egui::Ui, t: &Tile, size: Vec2) {
    let k = app.colors;
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, CornerRadius::same(10), over(k.bg_secondary, Color32::BLACK));
    match &t.frame {
        Some(f) => {
            let tex = texture_for(app, ui.ctx(), t.key, f);
            let img = Vec2::new(f.width as f32, f.height as f32);
            let scale = (rect.width() / img.x).min(rect.height() / img.y);
            let draw = egui::Rect::from_center_size(rect.center(), img * scale);
            // mirror our own preview like a mirror
            let uv = if t.key == SELF_KEY {
                egui::Rect::from_min_max(egui::pos2(1.0, 0.0), egui::pos2(0.0, 1.0))
            } else {
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0))
            };
            painter.image(tex.id(), draw, uv, Color32::WHITE);
        }
        None => {
            let d = (size.y * 0.38).min(96.0);
            let av = egui::Rect::from_center_size(rect.center(), Vec2::splat(d));
            widgets::paint_avatar(ui, &mut app.images, &app.backend, &t.user, av, None);
        }
    }
    if t.speaking {
        ui.painter().rect_stroke(rect, CornerRadius::same(10), Stroke::new(3.0, k.positive), egui::StrokeKind::Inside);
    }
    let label = format!("{}{}", t.user.display_name(), if t.muted { "  🎤✖" } else { "" });
    let galley = ui.painter().layout_no_wrap(label, egui::FontId::proportional(13.0), Color32::WHITE);
    let bg = egui::Rect::from_min_size(rect.left_bottom() + Vec2::new(8.0, -30.0), galley.size() + Vec2::new(14.0, 8.0));
    ui.painter().rect_filled(bg, CornerRadius::same(6), Color32::from_black_alpha(160));
    ui.painter().galley(bg.min + Vec2::new(7.0, 4.0), galley, Color32::WHITE);
    if let (Some(f), true) = (&t.frame, resp.hovered()) {
        ui.painter().text(rect.right_top() + Vec2::new(-8.0, 8.0), egui::Align2::RIGHT_TOP, format!("{}×{}", f.width, f.height), egui::FontId::proportional(11.0), Color32::from_white_alpha(200));
    }
}

pub fn voice_grid(app: &mut PickleApp, ui: &mut egui::Ui, channel: Snowflake) {
    let list = tiles(app, channel);
    let n = list.len().max(1);
    let avail = ui.available_size();
    // choose the column count that maximises 16:9 tile size
    let mut best = (1usize, Vec2::ZERO);
    for cols in 1..=n {
        let rows = n.div_ceil(cols);
        let w = (avail.x - 8.0 * (cols as f32 - 1.0)) / cols as f32;
        let h = (avail.y - 8.0 * (rows as f32 - 1.0) - 40.0) / rows as f32;
        let (tw, th) = if w / h > 16.0 / 9.0 { (h * 16.0 / 9.0, h) } else { (w, w * 9.0 / 16.0) };
        if tw * th > best.1.x * best.1.y {
            best = (cols, Vec2::new(tw.max(80.0), th.max(45.0)));
        }
    }
    let (cols, size) = best;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        egui::Grid::new("voice_tiles").spacing(Vec2::splat(8.0)).show(ui, |ui| {
            for (i, t) in list.iter().enumerate() {
                draw_tile(app, ui, t, size);
                if (i + 1) % cols == 0 {
                    ui.end_row();
                }
            }
        });
        ui.add_space(6.0);
        if let Some(c) = &app.media.camera_on.then_some(()).and(camera_line(app)) {
            ui.label(RichText::new(c).size(11.5).color(app.colors.muted));
        }
    });
}

fn camera_line(app: &PickleApp) -> Option<String> {
    #[cfg(feature = "camera")]
    {
        let stats = app.media.camera.as_ref()?;
        if let Some(e) = stats.error.lock().clone() {
            return Some(format!("Camera error: {e}"));
        }
        let m = stats.mode.lock().clone()?;
        Some(format!(
            "Sending {} · target {} kb/s · encode {:.1} ms",
            m.label(),
            app.cfg.video.bitrate_kbps,
            stats.encode_us.load(Ordering::Relaxed) as f32 / 1000.0
        ))
    }
    #[cfg(not(feature = "camera"))]
    {
        let _ = app;
        None
    }
}

/// Compact strip shown above other channels while in a call with video.
pub fn video_strip(app: &mut PickleApp, ui: &mut egui::Ui) {
    let Some(ch) = app.media.channel else { return };
    let list: Vec<Tile> = tiles(app, ch).into_iter().filter(|t| t.frame.is_some()).collect();
    egui::ScrollArea::horizontal().show(ui, |ui| {
        ui.horizontal(|ui| {
            for t in &list {
                draw_tile(app, ui, t, Vec2::new(240.0, 135.0));
            }
        });
    });
}
