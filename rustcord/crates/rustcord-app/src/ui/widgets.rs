//! Small shared widgets: avatars, badges, toasts and alert dialogs.

use crate::app::{Images, RustcordApp};
use crate::backend::Backend;
use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, Vec2};
use rustcord_discord::User;

/// Deterministic pleasant color for initials avatars.
pub fn hash_color(id: u64) -> Color32 {
    let h = (id.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 40) as f32 / (1u64 << 24) as f32;
    egui::ecolor::Hsva::new(h, 0.45, 0.62, 1.0).into()
}

/// Round avatar. `ring` draws a speaking indicator.
pub fn avatar(ui: &mut egui::Ui, images: &mut Images, backend: &Backend, user: &User, size: f32, ring: Option<Color32>) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    paint_avatar(ui, images, backend, user, rect, ring);
    resp
}

pub fn paint_avatar(ui: &egui::Ui, images: &mut Images, backend: &Backend, user: &User, rect: Rect, ring: Option<Color32>) {
    let px = (rect.width() * 2.0).ceil() as u32;
    let size = [32u32, 64, 128, 256].into_iter().find(|s| *s >= px).unwrap_or(256);
    let key = format!("avatar:{}:{}:{size}", user.id, user.avatar.as_deref().unwrap_or("default"));
    let painter = ui.painter();
    let radius = CornerRadius::same((rect.width() / 2.0) as u8);
    match (user.id.0 != 0).then(|| images.get(backend, &key, &user.avatar_url(size), size)).flatten() {
        Some(t) => {
            egui::Image::from_texture(&t)
                .corner_radius(radius)
                .paint_at(ui, rect);
        }
        None => {
            painter.circle_filled(rect.center(), rect.width() / 2.0, hash_color(user.id.0));
            let initial = user.display_name().chars().next().unwrap_or('?').to_uppercase().to_string();
            painter.text(rect.center(), egui::Align2::CENTER_CENTER, initial, egui::FontId::proportional(rect.width() * 0.45), Color32::WHITE);
        }
    }
    if let Some(c) = ring {
        painter.circle_stroke(rect.center(), rect.width() / 2.0 + 1.5, Stroke::new(2.5, c));
    }
}

/// Red pill with a number, anchored at the bottom-right of `rect`.
pub fn badge(painter: &egui::Painter, rect: Rect, n: u32, fill: Color32) {
    let text = if n > 99 { "99+".to_string() } else { n.to_string() };
    let w = 10.0 + text.len() as f32 * 7.0;
    let r = Rect::from_min_size(Pos2::new(rect.right() - w + 2.0, rect.bottom() - 15.0), Vec2::new(w, 16.0));
    painter.rect_filled(r, CornerRadius::same(8), fill);
    painter.text(r.center(), egui::Align2::CENTER_CENTER, text, egui::FontId::proportional(11.0), Color32::WHITE);
}

/// Flat icon button with hover background; `active` tints it.
pub fn icon_button(ui: &mut egui::Ui, icon: &str, tooltip: &str, active: Option<Color32>) -> egui::Response {
    let colors = ui.visuals().clone();
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(30.0), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(6), colors.widgets.hovered.weak_bg_fill);
    }
    let color = active.unwrap_or(if resp.hovered() { colors.widgets.hovered.fg_stroke.color } else { colors.widgets.inactive.fg_stroke.color });
    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, icon, egui::FontId::proportional(17.0), color);
    resp.on_hover_text(tooltip)
}

/// Toasts (bottom right) and alert dialogs from plugins.
pub fn overlays(app: &mut RustcordApp, ctx: &egui::Context) {
    let now = std::time::Instant::now();
    app.toasts.retain(|t| t.until > now);
    if !app.toasts.is_empty() {
        let k = app.colors;
        egui::Area::new(egui::Id::unique("toasts"))
            .anchor(egui::Align2::RIGHT_BOTTOM, Vec2::new(-16.0, -16.0))
            .order(egui::Order::Foreground)
            .interactable(false)
            .show(ctx, |ui| {
                for t in app.toasts.iter().rev().take(5) {
                    let accent = match t.kind.as_str() {
                        "error" | "danger" => k.danger,
                        "success" => k.positive,
                        "warning" | "warn" => k.warning,
                        _ => k.brand,
                    };
                    egui::Frame::new()
                        .fill(crate::ui::theme::over(k.bg_floating, Color32::BLACK))
                        .stroke(Stroke::new(1.0, accent))
                        .corner_radius(CornerRadius::same(8))
                        .inner_margin(egui::Margin::symmetric(12, 8))
                        .show(ui, |ui| {
                            ui.set_max_width(360.0);
                            ui.label(egui::RichText::new(&t.text).color(k.text));
                        });
                    ui.add_space(6.0);
                }
            });
    }
    if let Some((title, body)) = app.alerts.front().cloned() {
        let mut close = false;
        egui::Modal::new(egui::Id::unique("plugin_alert")).show(ctx, |ui| {
            ui.set_max_width(420.0);
            ui.heading(&title);
            ui.add_space(6.0);
            ui.label(&body);
            ui.add_space(10.0);
            if ui.button("OK").clicked() {
                close = true;
            }
        });
        if close {
            app.alerts.pop_front();
        }
    }
}
