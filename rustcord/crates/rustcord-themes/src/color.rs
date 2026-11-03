//! CSS color parsing: hex, rgb[a](), hsl[a](), color-mix(), named colors.

use crate::css::{eval_calcs, matching_paren};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

#[allow(clippy::self_named_constructors)]
impl Rgba {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub fn with_alpha(self, a: f32) -> Self {
        Self { a: (a.clamp(0.0, 1.0) * 255.0).round() as u8, ..self }
    }

    pub fn luminance(self) -> f32 {
        (0.2126 * self.r as f32 + 0.7152 * self.g as f32 + 0.0722 * self.b as f32) / 255.0
    }

    pub fn mix(self, other: Rgba, t: f32) -> Rgba {
        let l = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round().clamp(0.0, 255.0) as u8;
        Rgba { r: l(self.r, other.r), g: l(self.g, other.g), b: l(self.b, other.b), a: l(self.a, other.a) }
    }
}

const NAMED: &[(&str, [u8; 3])] = &[
    ("black", [0, 0, 0]),
    ("white", [255, 255, 255]),
    ("red", [255, 0, 0]),
    ("green", [0, 128, 0]),
    ("lime", [0, 255, 0]),
    ("blue", [0, 0, 255]),
    ("yellow", [255, 255, 0]),
    ("cyan", [0, 255, 255]),
    ("aqua", [0, 255, 255]),
    ("magenta", [255, 0, 255]),
    ("fuchsia", [255, 0, 255]),
    ("gray", [128, 128, 128]),
    ("grey", [128, 128, 128]),
    ("silver", [192, 192, 192]),
    ("maroon", [128, 0, 0]),
    ("olive", [128, 128, 0]),
    ("purple", [128, 0, 128]),
    ("teal", [0, 128, 128]),
    ("navy", [0, 0, 128]),
    ("orange", [255, 165, 0]),
    ("pink", [255, 192, 203]),
    ("hotpink", [255, 105, 180]),
    ("deeppink", [255, 20, 147]),
    ("crimson", [220, 20, 60]),
    ("tomato", [255, 99, 71]),
    ("coral", [255, 127, 80]),
    ("gold", [255, 215, 0]),
    ("khaki", [240, 230, 140]),
    ("violet", [238, 130, 238]),
    ("indigo", [75, 0, 130]),
    ("orchid", [218, 112, 214]),
    ("plum", [221, 160, 221]),
    ("salmon", [250, 128, 114]),
    ("skyblue", [135, 206, 235]),
    ("steelblue", [70, 130, 180]),
    ("royalblue", [65, 105, 225]),
    ("dodgerblue", [30, 144, 255]),
    ("slateblue", [106, 90, 205]),
    ("slategray", [112, 128, 144]),
    ("darkslategray", [47, 79, 79]),
    ("dimgray", [105, 105, 105]),
    ("darkgray", [169, 169, 169]),
    ("lightgray", [211, 211, 211]),
    ("gainsboro", [220, 220, 220]),
    ("whitesmoke", [245, 245, 245]),
    ("ghostwhite", [248, 248, 255]),
    ("darkred", [139, 0, 0]),
    ("darkgreen", [0, 100, 0]),
    ("darkblue", [0, 0, 139]),
    ("darkviolet", [148, 0, 211]),
    ("rebeccapurple", [102, 51, 153]),
    ("springgreen", [0, 255, 127]),
    ("seagreen", [46, 139, 87]),
    ("limegreen", [50, 205, 50]),
    ("chartreuse", [127, 255, 0]),
    ("turquoise", [64, 224, 208]),
    ("midnightblue", [25, 25, 112]),
    ("beige", [245, 245, 220]),
    ("ivory", [255, 255, 240]),
    ("lavender", [230, 230, 250]),
];

fn parse_hex(h: &str) -> Option<Rgba> {
    let h = h.trim_start_matches('#');
    if !h.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let d = |i: usize| u8::from_str_radix(&h[i..i + 1], 16).ok().map(|v| v * 17);
    let dd = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok();
    match h.len() {
        3 => Some(Rgba::rgb(d(0)?, d(1)?, d(2)?)),
        4 => Some(Rgba::rgba(d(0)?, d(1)?, d(2)?, d(3)?)),
        6 => Some(Rgba::rgb(dd(0)?, dd(2)?, dd(4)?)),
        8 => Some(Rgba::rgba(dd(0)?, dd(2)?, dd(4)?, dd(6)?)),
        _ => None,
    }
}

/// Split function arguments on commas, whitespace and `/`.
fn args(s: &str) -> Vec<String> {
    s.replace(['/', ','], " ").split_whitespace().map(String::from).collect()
}

fn num(s: &str, percent_scale: f64) -> Option<f64> {
    let s = s.trim();
    if s == "none" {
        return Some(0.0);
    }
    if let Some(p) = s.strip_suffix('%') {
        return p.parse::<f64>().ok().map(|v| v / 100.0 * percent_scale);
    }
    s.parse().ok()
}

fn hue(s: &str) -> Option<f64> {
    let s = s.trim();
    if let Some(v) = s.strip_suffix("deg") {
        v.parse().ok()
    } else if let Some(v) = s.strip_suffix("turn") {
        v.parse::<f64>().ok().map(|t| t * 360.0)
    } else if let Some(v) = s.strip_suffix("rad") {
        v.parse::<f64>().ok().map(f64::to_degrees)
    } else {
        s.parse().ok()
    }
}

fn hsl_to_rgb(h: f64, s: f64, l: f64) -> (u8, u8, u8) {
    let h = ((h % 360.0) + 360.0) % 360.0 / 360.0;
    let s = s.clamp(0.0, 1.0);
    let l = l.clamp(0.0, 1.0);
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    let f = |mut t: f64| {
        if t < 0.0 {
            t += 1.0;
        }
        if t > 1.0 {
            t -= 1.0;
        }
        let v = if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        };
        (v * 255.0).round().clamp(0.0, 255.0) as u8
    };
    (f(h + 1.0 / 3.0), f(h), f(h - 1.0 / 3.0))
}

fn alpha(a: Option<&String>) -> Option<u8> {
    match a {
        None => Some(255),
        Some(a) => num(a, 1.0).map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8),
    }
}

/// Parse a CSS color (after `var()` substitution).
pub fn parse_color(input: &str) -> Option<Rgba> {
    let s = eval_calcs(input.trim().trim_end_matches("!important").trim());
    let lower = s.to_ascii_lowercase();
    let lower = lower.trim();
    if lower == "transparent" {
        return Some(Rgba::rgba(0, 0, 0, 0));
    }
    if lower.starts_with('#') {
        return parse_hex(lower);
    }
    if let Some((_, c)) = NAMED.iter().find(|(n, _)| *n == lower) {
        return Some(Rgba::rgb(c[0], c[1], c[2]));
    }
    if let Some(open) = lower.find('(') {
        let func = lower[..open].trim();
        let len = matching_paren(&lower[open + 1..])?;
        let inner = &lower[open + 1..open + 1 + len];
        match func {
            "rgb" | "rgba" => {
                let a = args(inner);
                if a.len() < 3 {
                    return None;
                }
                let c = |i: usize| num(&a[i], 255.0).map(|v| v.round().clamp(0.0, 255.0) as u8);
                return Some(Rgba::rgba(c(0)?, c(1)?, c(2)?, alpha(a.get(3))?));
            }
            "hsl" | "hsla" => {
                let a = args(inner);
                if a.len() < 3 {
                    return None;
                }
                let (r, g, b) = hsl_to_rgb(hue(&a[0])?, num(&a[1], 1.0)?, num(&a[2], 1.0)?);
                return Some(Rgba::rgba(r, g, b, alpha(a.get(3))?));
            }
            "color-mix" => return color_mix(inner),
            _ => {}
        }
    }
    // Gradients and other complex values: use the first color inside.
    first_embedded_color(lower)
}

fn color_mix(inner: &str) -> Option<Rgba> {
    // color-mix(in srgb, A p%, B q%)
    let (_, rest) = crate::css::split_top_level_comma(inner)?;
    let (a, b) = crate::css::split_top_level_comma(rest)?;
    let split_pct = |s: &str| -> (String, Option<f64>) {
        let s = s.trim();
        if let Some(idx) = s.rfind(' ') {
            let (c, p) = s.split_at(idx);
            if let Some(v) = p.trim().strip_suffix('%').and_then(|v| v.parse::<f64>().ok()) {
                return (c.trim().to_string(), Some(v / 100.0));
            }
        }
        (s.to_string(), None)
    };
    let (ca, pa) = split_pct(a);
    let (cb, pb) = split_pct(b);
    let ca = parse_color(&ca)?;
    let cb = parse_color(&cb)?;
    let wa = match (pa, pb) {
        (Some(x), _) => x,
        (None, Some(y)) => 1.0 - y,
        _ => 0.5,
    };
    Some(ca.mix(cb, (1.0 - wa) as f32))
}

fn first_embedded_color(s: &str) -> Option<Rgba> {
    let mut best: Option<(usize, Rgba)> = None;
    for pat in ["#", "rgb", "hsl"] {
        let mut from = 0;
        while let Some(i) = s[from..].find(pat) {
            let i = from + i;
            from = i + 1;
            let cand = if pat == "#" {
                let end = s[i + 1..]
                    .find(|c: char| !c.is_ascii_hexdigit())
                    .map(|e| i + 1 + e)
                    .unwrap_or(s.len());
                parse_hex(&s[i..end])
            } else {
                let Some(open) = s[i..].find('(') else { continue };
                let Some(len) = matching_paren(&s[i + open + 1..]) else { continue };
                parse_color(&s[i..i + open + 1 + len + 1])
            };
            if let Some(c) = cand {
                if best.map(|(bi, _)| i < bi).unwrap_or(true) {
                    best = Some((i, c));
                }
                break;
            }
        }
    }
    best.map(|(_, c)| c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats() {
        assert_eq!(parse_color("#fff"), Some(Rgba::rgb(255, 255, 255)));
        assert_eq!(parse_color("#11223344"), Some(Rgba::rgba(0x11, 0x22, 0x33, 0x44)));
        assert_eq!(parse_color("rgba(10, 20, 30, 0.5)"), Some(Rgba::rgba(10, 20, 30, 128)));
        assert_eq!(parse_color("rgb(10 20 30 / 50%)"), Some(Rgba::rgba(10, 20, 30, 128)));
        assert_eq!(parse_color("hsl(0, 100%, 50%)"), Some(Rgba::rgb(255, 0, 0)));
        assert_eq!(parse_color("hsl(120deg 100% 25% / 1)"), Some(Rgba::rgb(0, 128, 0)));
        assert_eq!(parse_color("hsl(220 calc(1 * 7.7%) 22.9% / 1)").map(|c| c.b > c.r), Some(true));
        assert_eq!(parse_color("Crimson"), Some(Rgba::rgb(220, 20, 60)));
        assert_eq!(parse_color("transparent").map(|c| c.a), Some(0));
        assert_eq!(parse_color("color-mix(in srgb, #000 50%, #fff)"), Some(Rgba::rgb(128, 128, 128)));
        assert_eq!(
            parse_color("linear-gradient(45deg, rgba(1,2,3,1) 0%, #fff 100%)"),
            Some(Rgba::rgb(1, 2, 3))
        );
        assert_eq!(parse_color("url(x.png)"), None);
    }
}
