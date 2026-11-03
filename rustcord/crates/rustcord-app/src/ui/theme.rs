//! Maps a parsed CSS theme onto egui's style.

use eframe::egui::{self, Color32, CornerRadius, Stroke, Visuals};
use rustcord_themes::{Rgba, Theme};

pub fn c(r: Rgba) -> Color32 {
    Color32::from_rgba_unmultiplied(r.r, r.g, r.b, r.a)
}

/// Palette as egui colors, for custom painting.
#[derive(Clone, Copy)]
pub struct Colors {
    pub bg_primary: Color32,
    pub bg_secondary: Color32,
    pub bg_secondary_alt: Color32,
    pub bg_tertiary: Color32,
    pub bg_floating: Color32,
    pub bg_input: Color32,
    pub bg_hover: Color32,
    pub bg_selected: Color32,
    pub bg_accent: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub header: Color32,
    pub header_secondary: Color32,
    pub interactive: Color32,
    pub interactive_hover: Color32,
    pub interactive_active: Color32,
    pub interactive_muted: Color32,
    pub link: Color32,
    pub brand: Color32,
    pub danger: Color32,
    pub positive: Color32,
    pub warning: Color32,
    pub mention_bg: Color32,
    pub radius: u8,
}

impl Colors {
    pub fn from_theme(t: &Theme) -> Self {
        let p = &t.palette;
        Self {
            bg_primary: c(p.bg_primary),
            bg_secondary: c(p.bg_secondary),
            bg_secondary_alt: c(p.bg_secondary_alt),
            bg_tertiary: c(p.bg_tertiary),
            bg_floating: c(p.bg_floating),
            bg_input: c(p.bg_input),
            bg_hover: c(p.bg_hover),
            bg_selected: c(p.bg_selected),
            bg_accent: c(p.bg_accent),
            text: c(p.text_normal),
            muted: c(p.text_muted),
            header: c(p.header_primary),
            header_secondary: c(p.header_secondary),
            interactive: c(p.interactive_normal),
            interactive_hover: c(p.interactive_hover),
            interactive_active: c(p.interactive_active),
            interactive_muted: c(p.interactive_muted),
            link: c(p.link),
            brand: c(p.brand),
            danger: c(p.danger),
            positive: c(p.positive),
            warning: c(p.warning),
            mention_bg: c(p.mention_bg),
            radius: t.radius.unwrap_or(6.0).clamp(0.0, 24.0) as u8,
        }
    }
}

/// Composite a translucent color over an opaque one (egui widgets want
/// opaque fills to look right).
pub fn over(top: Color32, bottom: Color32) -> Color32 {
    let a = top.a() as f32 / 255.0;
    let mix = |t: u8, b: u8| (t as f32 * a + b as f32 * (1.0 - a)).round() as u8;
    Color32::from_rgb(mix(top.r(), bottom.r()), mix(top.g(), bottom.g()), mix(top.b(), bottom.b()))
}

pub fn apply(ctx: &egui::Context, t: &Theme, scale: f32) -> Colors {
    let k = Colors::from_theme(t);
    let mut v = if t.dark { Visuals::dark() } else { Visuals::light() };
    let r = CornerRadius::same(k.radius);
    let base = over(k.bg_primary, Color32::BLACK);

    v.panel_fill = k.bg_primary;
    v.window_fill = over(k.bg_floating, base);
    v.window_stroke = Stroke::new(1.0, over(k.bg_accent, base));
    v.window_corner_radius = CornerRadius::same(k.radius.saturating_add(4));
    v.menu_corner_radius = r;
    v.extreme_bg_color = over(k.bg_input, base);
    v.text_edit_bg_color = Some(over(k.bg_input, base));
    v.faint_bg_color = over(k.bg_hover, base);
    v.code_bg_color = over(k.bg_tertiary, base);
    v.hyperlink_color = k.link;
    v.error_fg_color = k.danger;
    v.warn_fg_color = k.warning;
    v.selection.bg_fill = k.brand.gamma_multiply(0.55);
    v.selection.stroke = Stroke::new(1.0, k.header);
    v.slider_trailing_fill = true;

    let w = &mut v.widgets;
    w.noninteractive.bg_fill = over(k.bg_floating, base);
    w.noninteractive.weak_bg_fill = over(k.bg_secondary, base);
    w.noninteractive.bg_stroke = Stroke::new(1.0, over(k.bg_accent, base).gamma_multiply(0.8));
    w.noninteractive.fg_stroke = Stroke::new(1.0, k.text);
    w.noninteractive.corner_radius = r;

    w.inactive.bg_fill = over(k.bg_accent, base);
    w.inactive.weak_bg_fill = over(k.bg_hover, over(k.bg_secondary_alt, base));
    w.inactive.bg_stroke = Stroke::NONE;
    w.inactive.fg_stroke = Stroke::new(1.0, k.interactive);
    w.inactive.corner_radius = r;

    w.hovered.bg_fill = over(k.bg_selected, over(k.bg_accent, base));
    w.hovered.weak_bg_fill = over(k.bg_selected, over(k.bg_secondary_alt, base));
    w.hovered.bg_stroke = Stroke::new(1.0, k.brand.gamma_multiply(0.6));
    w.hovered.fg_stroke = Stroke::new(1.5, k.interactive_hover);
    w.hovered.corner_radius = r;

    w.active.bg_fill = k.brand;
    w.active.weak_bg_fill = k.brand.gamma_multiply(0.8);
    w.active.bg_stroke = Stroke::new(1.0, k.brand);
    w.active.fg_stroke = Stroke::new(2.0, k.interactive_active);
    w.active.corner_radius = r;
    w.open = w.hovered;
    w.open.weak_bg_fill = over(k.bg_secondary_alt, base);

    ctx.set_theme(if t.dark { egui::Theme::Dark } else { egui::Theme::Light });
    ctx.set_visuals(v);
    ctx.global_style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(6.0, 4.0);
        s.spacing.button_padding = egui::vec2(10.0, 5.0);
        s.spacing.slider_width = 260.0;
        s.spacing.interact_size.y = 24.0;
        s.spacing.scroll.floating = true;
    });
    if (ctx.zoom_factor() - scale).abs() > 0.001 {
        ctx.set_zoom_factor(scale);
    }
    k
}
