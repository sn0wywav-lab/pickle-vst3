//! Just enough CSS to read themes written for the web-based clients:
//! metadata headers, `@import`s, custom properties, `var()`, `calc()`.

use std::collections::HashMap;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Meta {
    pub name: Option<String>,
    pub author: Option<String>,
    pub version: Option<String>,
    pub description: Option<String>,
}

/// Parse the `/** @name ... */` header used by BetterDiscord themes/plugins.
pub fn parse_meta(src: &str) -> Meta {
    let mut meta = Meta::default();
    let Some(start) = src.find("/**") else { return meta };
    let Some(len) = src[start..].find("*/") else { return meta };
    for line in src[start + 3..start + len].lines() {
        let line = line.trim().trim_start_matches('*').trim();
        let Some(rest) = line.strip_prefix('@') else { continue };
        let (key, value) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
        let value = value.trim().to_string();
        match key {
            "name" => meta.name = Some(value),
            "author" => meta.author = Some(value),
            "version" => meta.version = Some(value),
            "description" => meta.description = Some(value),
            _ => {}
        }
    }
    meta
}

pub fn strip_comments(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut rest = src;
    while let Some(i) = rest.find("/*") {
        out.push_str(&rest[..i]);
        match rest[i + 2..].find("*/") {
            Some(j) => rest = &rest[i + 2 + j + 2..],
            None => {
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    out
}

/// URLs referenced by `@import url(...)` / `@import "..."`.
pub fn imports(src: &str) -> Vec<String> {
    let src = strip_comments(src);
    let mut out = Vec::new();
    for part in src.split("@import").skip(1) {
        let part = part.trim_start();
        let url = if let Some(r) = part.strip_prefix("url(") {
            r.split(')').next().unwrap_or("")
        } else {
            part.split(';').next().unwrap_or("")
        };
        let url = url.trim().trim_matches(|c| c == '"' || c == '\'').trim();
        if url.starts_with("http://") || url.starts_with("https://") {
            out.push(url.to_string());
        }
    }
    out
}

/// Custom properties collected from a stylesheet, split by whether the rule
/// targets the light theme.
#[derive(Debug, Clone, Default)]
pub struct Vars {
    pub main: HashMap<String, String>,
    pub light: HashMap<String, String>,
}

impl Vars {
    /// Add every `--custom: value` declaration from a stylesheet. Later
    /// stylesheets override earlier ones (cascade order).
    pub fn add_stylesheet(&mut self, css: &str) {
        let css = strip_comments(css);
        let bytes = css.as_bytes();
        let mut stack: Vec<String> = Vec::new();
        let mut buf = String::new();
        let mut i = 0;
        let mut quote: Option<u8> = None;
        let mut paren = 0i32;
        while i < bytes.len() {
            let c = bytes[i];
            if let Some(q) = quote {
                if c == q {
                    quote = None;
                }
                buf.push(c as char);
                i += 1;
                continue;
            }
            match c {
                b'"' | b'\'' => {
                    quote = Some(c);
                    buf.push(c as char);
                }
                b'(' => {
                    paren += 1;
                    buf.push('(');
                }
                b')' => {
                    paren -= 1;
                    buf.push(')');
                }
                b'{' if paren <= 0 => {
                    stack.push(buf.trim().to_string());
                    buf.clear();
                }
                b'}' if paren <= 0 => {
                    self.declaration(&stack, &buf);
                    buf.clear();
                    stack.pop();
                }
                b';' if paren <= 0 => {
                    if stack.is_empty() {
                        buf.clear(); // top-level at-rule like @import
                    } else {
                        self.declaration(&stack, &buf);
                        buf.clear();
                    }
                }
                _ => {
                    // multi-byte UTF-8 passes through byte-wise below
                    if c < 0x80 {
                        buf.push(c as char);
                    } else {
                        let ch_len = utf8_len(c);
                        if let Ok(s) = std::str::from_utf8(&bytes[i..(i + ch_len).min(bytes.len())]) {
                            buf.push_str(s);
                        }
                        i += ch_len;
                        continue;
                    }
                }
            }
            i += 1;
        }
    }

    fn declaration(&mut self, stack: &[String], decl: &str) {
        let Some(selector) = stack.last() else { return };
        if stack.iter().any(|s| s.starts_with("@keyframes") || s.starts_with("@font-face")) {
            return;
        }
        let decl = decl.trim();
        let Some(rest) = decl.strip_prefix("--") else { return };
        let Some((name, value)) = rest.split_once(':') else { return };
        let name = format!("--{}", name.trim());
        let value = value.trim().trim_end_matches("!important").trim().to_string();
        let light = selector.contains("theme-light") && !selector.contains("theme-dark");
        if light {
            self.light.insert(name, value);
        } else {
            self.main.insert(name, value);
        }
    }

    pub fn raw(&self, name: &str) -> Option<&str> {
        self.main.get(name).or_else(|| self.light.get(name)).map(String::as_str)
    }

    /// Value with all `var()` references substituted.
    pub fn get(&self, name: &str) -> Option<String> {
        self.raw(name).map(|v| self.substitute(v, 0))
    }

    pub fn substitute(&self, value: &str, depth: u32) -> String {
        if depth > 16 || !value.contains("var(") {
            return value.to_string();
        }
        let mut out = String::with_capacity(value.len());
        let mut rest = value;
        while let Some(i) = rest.find("var(") {
            out.push_str(&rest[..i]);
            let inner_start = i + 4;
            let Some(len) = matching_paren(&rest[inner_start..]) else {
                out.push_str(&rest[i..]);
                return out;
            };
            let inner = &rest[inner_start..inner_start + len];
            let (name, fallback) = match split_top_level_comma(inner) {
                Some((n, f)) => (n.trim(), Some(f.trim())),
                None => (inner.trim(), None),
            };
            let resolved = match self.raw(name) {
                Some(v) => self.substitute(v, depth + 1),
                None => fallback.map(|f| self.substitute(f, depth + 1)).unwrap_or_default(),
            };
            out.push_str(&resolved);
            rest = &rest[inner_start + len + 1..];
        }
        out.push_str(rest);
        out
    }
}

fn utf8_len(first: u8) -> usize {
    match first {
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF7 => 4,
        _ => 1,
    }
}

/// Length of the text up to the `)` that closes an already-opened paren.
pub fn matching_paren(s: &str) -> Option<usize> {
    let mut depth = 1;
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

pub fn split_top_level_comma(s: &str) -> Option<(&str, &str)> {
    let mut depth = 0;
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => return Some((&s[..i], &s[i + 1..])),
            _ => {}
        }
    }
    None
}

/// Evaluate every `calc(...)` in a value (numbers, %, deg, px; + - * /).
pub fn eval_calcs(value: &str) -> String {
    let mut out = String::new();
    let mut rest = value;
    while let Some(i) = rest.find("calc(") {
        out.push_str(&rest[..i]);
        let start = i + 5;
        let Some(len) = matching_paren(&rest[start..]) else {
            out.push_str(&rest[i..]);
            return out;
        };
        let inner = eval_calcs(&rest[start..start + len]);
        match calc(&inner) {
            Some((v, unit)) => out.push_str(&format!("{}{}", trim_float(v), unit)),
            None => out.push_str(&inner),
        }
        rest = &rest[start + len + 1..];
    }
    out.push_str(rest);
    out
}

fn trim_float(v: f64) -> String {
    let s = format!("{v:.4}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Tiny recursive-descent evaluator. Returns (value, unit).
pub fn calc(expr: &str) -> Option<(f64, String)> {
    let tokens = tokenize(expr)?;
    let mut pos = 0;
    let r = parse_sum(&tokens, &mut pos)?;
    (pos == tokens.len()).then_some(r)
}

#[derive(Debug, Clone)]
enum Tok {
    Num(f64, String),
    Op(char),
    Open,
    Close,
}

fn tokenize(s: &str) -> Option<Vec<Tok>> {
    let mut out = Vec::new();
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        let prev_is_value = matches!(out.last(), Some(Tok::Num(..)) | Some(Tok::Close));
        if c.is_ascii_digit() || c == '.' || (c == '-' && !prev_is_value && i + 1 < chars.len() && (chars[i + 1].is_ascii_digit() || chars[i + 1] == '.')) {
            let start = i;
            i += 1;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                i += 1;
            }
            let num: f64 = chars[start..i].iter().collect::<String>().parse().ok()?;
            let ustart = i;
            while i < chars.len() && (chars[i].is_alphabetic() || chars[i] == '%') {
                i += 1;
            }
            out.push(Tok::Num(num, chars[ustart..i].iter().collect()));
            continue;
        }
        match c {
            '+' | '-' | '*' | '/' => out.push(Tok::Op(c)),
            '(' => out.push(Tok::Open),
            ')' => out.push(Tok::Close),
            _ => return None,
        }
        i += 1;
    }
    Some(out)
}

fn parse_sum(t: &[Tok], pos: &mut usize) -> Option<(f64, String)> {
    let (mut v, mut u) = parse_product(t, pos)?;
    while let Some(Tok::Op(op @ ('+' | '-'))) = t.get(*pos) {
        let op = *op;
        *pos += 1;
        let (r, ru) = parse_product(t, pos)?;
        if u.is_empty() {
            u = ru;
        }
        v = if op == '+' { v + r } else { v - r };
    }
    Some((v, u))
}

fn parse_product(t: &[Tok], pos: &mut usize) -> Option<(f64, String)> {
    let (mut v, mut u) = parse_atom(t, pos)?;
    while let Some(Tok::Op(op @ ('*' | '/'))) = t.get(*pos) {
        let op = *op;
        *pos += 1;
        let (r, ru) = parse_atom(t, pos)?;
        if u.is_empty() {
            u = ru;
        }
        v = if op == '*' { v * r } else if r != 0.0 { v / r } else { return None };
    }
    Some((v, u))
}

fn parse_atom(t: &[Tok], pos: &mut usize) -> Option<(f64, String)> {
    match t.get(*pos)? {
        Tok::Num(v, u) => {
            *pos += 1;
            Some((*v, u.clone()))
        }
        Tok::Open => {
            *pos += 1;
            let r = parse_sum(t, pos)?;
            matches!(t.get(*pos), Some(Tok::Close)).then(|| {
                *pos += 1;
                r
            })
        }
        Tok::Op('-') => {
            *pos += 1;
            let (v, u) = parse_atom(t, pos)?;
            Some((-v, u))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meta_and_imports() {
        let css = "/**\n * @name Fancy\n * @author someone\n * @version 2.1\n */\n@import url('https://example.com/a.css');\n@import \"https://x.dev/b.css\";";
        let m = parse_meta(css);
        assert_eq!(m.name.as_deref(), Some("Fancy"));
        assert_eq!(m.version.as_deref(), Some("2.1"));
        assert_eq!(imports(css), vec!["https://example.com/a.css", "https://x.dev/b.css"]);
    }

    #[test]
    fn vars_cascade_and_var_resolution() {
        let mut v = Vars::default();
        v.add_stylesheet(
            ":root { --a: #111; --b: var(--a); }
             .theme-dark, .theme-darker { --a: #222 !important; }
             .theme-light { --a: #eee; }
             @media (min-width: 1px) { :root { --c: var(--missing, rgb(1, 2, 3)); } }
             @keyframes x { from { --a: red; } }",
        );
        assert_eq!(v.get("--b").as_deref(), Some("#222"));
        assert_eq!(v.light.get("--a").map(String::as_str), Some("#eee"));
        assert_eq!(v.get("--c").as_deref(), Some("rgb(1, 2, 3)"));
    }

    #[test]
    fn calc_eval() {
        assert_eq!(eval_calcs("hsl(220 calc(1 * 7.7%) 22.9%)"), "hsl(220 7.7% 22.9%)");
        assert_eq!(eval_calcs("calc((2 + 3) * 4px)"), "20px");
        assert_eq!(eval_calcs("calc(var-free - 1)"), "var-free - 1");
    }
}
