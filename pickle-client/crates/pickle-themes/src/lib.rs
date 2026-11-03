//! Theme engine.
//!
//! Themes are plain CSS, so themes made for BetterDiscord (`*.theme.css`),
//! Vencord / Vesktop (QuickCSS or theme files) and similar web-based clients
//! load directly. A native client has no DOM, so selector-level rules can't
//! apply; what *does* carry over is the design-token layer every serious
//! theme is built on: CSS custom properties (`--background-primary`,
//! `--text-normal`, `--brand-experiment`, ... including the newer
//! `--background-base-*` / `--text-default` names), `@import`ed remote
//! stylesheets, `var()`, `calc()`, `hsl()`, `color-mix()` and background
//! images.

pub mod color;
pub mod css;

pub use color::{parse_color, Rgba};
pub use css::{Meta, Vars};

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palette {
    /// Chat area.
    pub bg_primary: Rgba,
    /// Channel list.
    pub bg_secondary: Rgba,
    /// User / voice panel.
    pub bg_secondary_alt: Rgba,
    /// Server rail.
    pub bg_tertiary: Rgba,
    /// Popups, menus, modals.
    pub bg_floating: Rgba,
    pub bg_input: Rgba,
    pub bg_hover: Rgba,
    pub bg_selected: Rgba,
    pub bg_accent: Rgba,
    pub text_normal: Rgba,
    pub text_muted: Rgba,
    pub header_primary: Rgba,
    pub header_secondary: Rgba,
    pub interactive_normal: Rgba,
    pub interactive_hover: Rgba,
    pub interactive_active: Rgba,
    pub interactive_muted: Rgba,
    pub link: Rgba,
    pub brand: Rgba,
    pub danger: Rgba,
    pub positive: Rgba,
    pub warning: Rgba,
    pub mention_bg: Rgba,
    pub scrollbar: Rgba,
}

impl Default for Palette {
    fn default() -> Self {
        ThemeBuilder::new().add_css(BUILTIN[0].1).build().palette
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    pub meta: Meta,
    pub palette: Palette,
    pub dark: bool,
    pub background_image: Option<String>,
    /// Corner radius in points (from `--pickle-radius`).
    pub radius: Option<f32>,
    /// Remote stylesheets referenced by the theme (fetched by the app).
    pub imports: Vec<String>,
    /// How many variables were recognised (shown in the UI as a hint for
    /// how much of a theme applies).
    pub mapped_vars: usize,
}

impl Default for Theme {
    fn default() -> Self {
        ThemeBuilder::new().add_css(BUILTIN[0].1).build()
    }
}

/// Variable names tried in order for each palette slot.
const MAP: &[(&str, &[&str])] = &[
    ("bg_primary", &["--pickle-bg-primary", "--background-primary", "--background-base-lower", "--bg-base-primary", "--chat-background"]),
    ("bg_secondary", &["--pickle-bg-secondary", "--background-secondary", "--background-base-low", "--bg-base-secondary"]),
    ("bg_secondary_alt", &["--pickle-bg-secondary-alt", "--background-secondary-alt", "--background-base-lowest", "--bg-base-tertiary"]),
    ("bg_tertiary", &["--pickle-bg-tertiary", "--background-tertiary", "--background-base-lowest", "--bg-base-tertiary"]),
    ("bg_floating", &["--pickle-bg-floating", "--background-floating", "--background-surface-high", "--modal-background", "--background-surface-higher"]),
    ("bg_input", &["--pickle-bg-input", "--channeltextarea-background", "--input-background", "--background-surface-high"]),
    ("bg_hover", &["--pickle-bg-hover", "--background-modifier-hover", "--interactive-background-hover", "--background-mod-subtle"]),
    ("bg_selected", &["--pickle-bg-selected", "--background-modifier-selected", "--interactive-background-selected", "--background-mod-normal"]),
    ("bg_accent", &["--pickle-bg-accent", "--background-accent", "--background-mod-strong"]),
    ("text_normal", &["--pickle-text", "--text-normal", "--text-default", "--text-primary"]),
    ("text_muted", &["--pickle-text-muted", "--text-muted", "--text-subtle", "--text-tertiary"]),
    ("header_primary", &["--pickle-header", "--header-primary", "--text-strong"]),
    ("header_secondary", &["--pickle-header-secondary", "--header-secondary", "--text-secondary"]),
    ("interactive_normal", &["--interactive-normal", "--interactive-text-default", "--channels-default", "--channel-icon"]),
    ("interactive_hover", &["--interactive-hover", "--interactive-text-hover"]),
    ("interactive_active", &["--interactive-active", "--interactive-text-active"]),
    ("interactive_muted", &["--interactive-muted"]),
    ("link", &["--pickle-link", "--text-link", "--link-color"]),
    ("brand", &["--pickle-accent", "--brand-experiment", "--brand-500", "--brand-560", "--accent-color", "--accent", "--main-color"]),
    ("danger", &["--status-danger", "--text-danger", "--red-400", "--red-360"]),
    ("positive", &["--status-positive", "--text-positive", "--green-360", "--green-400"]),
    ("warning", &["--status-warning", "--text-warning", "--yellow-300"]),
    ("mention_bg", &["--background-mentioned", "--background-message-highlight"]),
    ("scrollbar", &["--scrollbar-auto-thumb", "--scrollbar-thin-thumb"]),
];

const BG_IMAGE_VARS: &[&str] = &["--pickle-background-image", "--background-image", "--app-background-image", "--bg-image"];

/// Accumulates stylesheets in cascade order and produces a [`Theme`].
#[derive(Default)]
pub struct ThemeBuilder {
    vars: Vars,
    meta: Option<Meta>,
    imports: Vec<String>,
}

impl ThemeBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a stylesheet. The first one with a metadata header names the theme.
    pub fn add_css(mut self, css: &str) -> Self {
        self.push_css(css);
        self
    }

    pub fn push_css(&mut self, css: &str) {
        let meta = css::parse_meta(css);
        if self.meta.is_none() && meta.name.is_some() {
            self.meta = Some(meta);
        }
        self.imports.extend(css::imports(css));
        self.vars.add_stylesheet(css);
    }

    pub fn vars(&self) -> &Vars {
        &self.vars
    }

    pub fn build(&self) -> Theme {
        let v = &self.vars;
        let lookup = |slot: &str| -> Option<Rgba> {
            let names = MAP.iter().find(|(s, _)| *s == slot)?.1;
            names.iter().find_map(|n| v.get(n).and_then(|val| parse_color(&val)))
        };
        let mut mapped = 0;
        let mut get = |slot: &str, fallback: Rgba| -> Rgba {
            match lookup(slot) {
                Some(c) => {
                    mapped += 1;
                    c
                }
                None => fallback,
            }
        };

        // Fallbacks derive from whatever the theme does define, so partial
        // themes still look coherent.
        let bg_primary = get("bg_primary", Rgba::rgb(0x1b, 0x21, 0x1d));
        let dark = v.get("--pickle-light").is_none() && bg_primary.luminance() < 0.5;
        let shade = |c: Rgba, by: f32| if dark { c.mix(Rgba::rgb(0, 0, 0), by) } else { c.mix(Rgba::rgb(0, 0, 0), by * 0.5) };
        let bg_secondary = get("bg_secondary", shade(bg_primary, 0.18));
        let bg_secondary_alt = get("bg_secondary_alt", shade(bg_secondary, 0.12));
        let bg_tertiary = get("bg_tertiary", shade(bg_secondary, 0.25));
        let bg_floating = get("bg_floating", shade(bg_tertiary, 0.1));
        let text_normal = get("text_normal", if dark { Rgba::rgb(0xdb, 0xde, 0xe1) } else { Rgba::rgb(0x31, 0x33, 0x38) });
        let bg_input = get("bg_input", bg_primary.mix(text_normal, 0.06));
        let bg_hover = get("bg_hover", text_normal.with_alpha(0.06));
        let bg_selected = get("bg_selected", text_normal.with_alpha(0.12));
        let bg_accent = get("bg_accent", bg_primary.mix(text_normal, 0.2));
        let text_muted = get("text_muted", text_normal.mix(bg_primary, 0.4));
        let header_primary = get("header_primary", text_normal.mix(if dark { Rgba::rgb(255, 255, 255) } else { Rgba::rgb(0, 0, 0) }, 0.5));
        let header_secondary = get("header_secondary", text_muted);
        let interactive_normal = get("interactive_normal", text_muted.mix(text_normal, 0.5));
        let interactive_hover = get("interactive_hover", text_normal);
        let interactive_active = get("interactive_active", header_primary);
        let interactive_muted = get("interactive_muted", text_muted.mix(bg_primary, 0.5));
        let brand = get("brand", Rgba::rgb(0x7a, 0xc4, 0x5c));
        let link = get("link", brand.mix(Rgba::rgb(0x00, 0xa8, 0xfc), 0.6));
        let danger = get("danger", Rgba::rgb(0xf2, 0x3f, 0x43));
        let positive = get("positive", Rgba::rgb(0x23, 0xa5, 0x5a));
        let warning = get("warning", Rgba::rgb(0xf0, 0xb2, 0x32));
        let mention_bg = get("mention_bg", warning.with_alpha(0.1));
        let scrollbar = get("scrollbar", bg_tertiary.mix(text_normal, 0.15));

        let background_image = BG_IMAGE_VARS.iter().find_map(|n| v.get(n)).and_then(|val| extract_url(&val));
        let radius = v
            .get("--pickle-radius")
            .and_then(|r| css::eval_calcs(&r).trim_end_matches("px").trim().parse::<f32>().ok());

        Theme {
            meta: self.meta.clone().unwrap_or_default(),
            dark,
            palette: Palette {
                bg_primary,
                bg_secondary,
                bg_secondary_alt,
                bg_tertiary,
                bg_floating,
                bg_input,
                bg_hover,
                bg_selected,
                bg_accent,
                text_normal,
                text_muted,
                header_primary,
                header_secondary,
                interactive_normal,
                interactive_hover,
                interactive_active,
                interactive_muted,
                link,
                brand,
                danger,
                positive,
                warning,
                mention_bg,
                scrollbar,
            },
            background_image,
            radius,
            imports: {
                let mut i = self.imports.clone();
                i.dedup();
                i
            },
            mapped_vars: mapped,
        }
    }
}

/// `url("...")` → `...` (only http(s) URLs are honoured).
pub fn extract_url(v: &str) -> Option<String> {
    let start = v.find("url(")? + 4;
    let len = css::matching_paren(&v[start..])?;
    let url = v[start..start + len].trim().trim_matches(|c| c == '"' || c == '\'');
    (url.starts_with("https://") || url.starts_with("http://")).then(|| url.to_string())
}

/// Built-in themes: (id, css).
pub const BUILTIN: &[(&str, &str)] = &[
    ("builtin:pickle-dark", include_str!("builtin/pickle-dark.theme.css")),
    ("builtin:midnight", include_str!("builtin/midnight.theme.css")),
    ("builtin:graphite", include_str!("builtin/graphite.theme.css")),
    ("builtin:daylight", include_str!("builtin/daylight.theme.css")),
];

#[derive(Debug, Clone)]
pub struct ThemeEntry {
    /// `builtin:<name>` or the file path.
    pub id: String,
    pub meta: Meta,
    pub path: Option<PathBuf>,
}

impl ThemeEntry {
    pub fn display_name(&self) -> String {
        self.meta.name.clone().unwrap_or_else(|| {
            self.path
                .as_ref()
                .and_then(|p| p.file_name())
                .map(|f| f.to_string_lossy().trim_end_matches(".css").trim_end_matches(".theme").to_string())
                .unwrap_or_else(|| self.id.clone())
        })
    }
}

/// List built-in themes plus every `*.css` file in `dir`.
pub fn scan(dir: &Path) -> Vec<ThemeEntry> {
    let mut out: Vec<ThemeEntry> = BUILTIN
        .iter()
        .map(|(id, css)| ThemeEntry { id: id.to_string(), meta: css::parse_meta(css), path: None })
        .collect();
    if let Ok(rd) = std::fs::read_dir(dir) {
        let mut files: Vec<PathBuf> = rd
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().map(|e| e == "css").unwrap_or(false))
            .collect();
        files.sort();
        for p in files {
            let meta = std::fs::read_to_string(&p).map(|s| css::parse_meta(&s)).unwrap_or_default();
            out.push(ThemeEntry { id: p.to_string_lossy().into_owned(), meta, path: Some(p) });
        }
    }
    out
}

/// Raw CSS for a theme id (built-in or file).
pub fn load_css(id: &str) -> std::io::Result<String> {
    if let Some((_, css)) = BUILTIN.iter().find(|(b, _)| *b == id) {
        return Ok(css.to_string());
    }
    std::fs::read_to_string(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_parse_fully() {
        for (id, css) in BUILTIN {
            let t = ThemeBuilder::new().add_css(css).build();
            assert!(t.meta.name.is_some(), "{id}");
            assert!(t.mapped_vars >= 18, "{id}: {}", t.mapped_vars);
        }
        assert!(!ThemeBuilder::new().add_css(BUILTIN[3].1).build().dark);
    }

    /// A typical BetterDiscord theme: meta header, remote @import, variables
    /// on `.theme-dark`, a background image and selector rules we ignore.
    #[test]
    fn betterdiscord_style_theme() {
        let css = r#"/**
 * @name Frosted Pickle
 * @author tester
 * @version 1.2.3
 * @description test theme
 */
@import url("https://themes.example.com/frosted/main.css");
:root {
  --saturation-factor: 1;
  --primary-630-hsl: 220 calc(var(--saturation-factor, 1) * 7.7%) 22.9%;
  --background-image: url("https://images.example.com/bg.png");
  --main-color: #ff66aa;
}
.theme-dark {
  --background-primary: hsl(var(--primary-630-hsl) / 1);
  --background-secondary: rgba(0, 0, 0, .35);
  --text-normal: #fafafa;
}
.theme-light { --background-primary: white; }
.chat-3bRxxu .messages { background: red !important; }
"#;
        let t = ThemeBuilder::new().add_css(css).build();
        assert_eq!(t.meta.name.as_deref(), Some("Frosted Pickle"));
        assert_eq!(t.imports, vec!["https://themes.example.com/frosted/main.css"]);
        assert_eq!(t.background_image.as_deref(), Some("https://images.example.com/bg.png"));
        assert_eq!(t.palette.brand, Rgba::rgb(0xff, 0x66, 0xaa));
        assert_eq!(t.palette.bg_secondary.a, 89);
        assert!(t.dark);
        // hsl(220 7.7% 22.9%) is a dark blue-grey
        let p = t.palette.bg_primary;
        assert!(p.b > p.r && p.luminance() < 0.3, "{p:?}");
    }

    #[test]
    fn plugin_css_layers_override() {
        let mut b = ThemeBuilder::new();
        b.push_css(BUILTIN[0].1);
        b.push_css(":root { --brand-experiment: #123456; }");
        assert_eq!(b.build().palette.brand, Rgba::rgb(0x12, 0x34, 0x56));
    }

    #[test]
    fn new_refresh_variable_names() {
        let t = ThemeBuilder::new()
            .add_css(":root { --background-base-lower: #101010; --text-default: #eeeeee; --text-strong: #ffffff; }")
            .build();
        assert_eq!(t.palette.bg_primary, Rgba::rgb(16, 16, 16));
        assert_eq!(t.palette.text_normal, Rgba::rgb(0xee, 0xee, 0xee));
        assert_eq!(t.palette.header_primary, Rgba::rgb(255, 255, 255));
    }
}

#[cfg(test)]
mod example_tests {
    use super::*;

    #[test]
    fn shipped_example_theme_maps_everything() {
        let css = include_str!("../../../examples/themes/FrostedBrine.theme.css");
        let t = ThemeBuilder::new().add_css(css).build();
        assert_eq!(t.meta.name.as_deref(), Some("Frosted Brine"));
        assert!(t.mapped_vars >= 18, "{}", t.mapped_vars);
        assert_eq!(t.radius, Some(10.0));
        // hsl(168 18% 11%) -> dark teal
        let p = t.palette.bg_primary;
        assert!(p.g > p.r && p.luminance() < 0.15, "{p:?}");
        // color-mix(in srgb, #3ee6c4 85%, white)
        assert_eq!(t.palette.brand, Rgba::rgb(0x5b, 0xea, 0xcd));
    }
}
