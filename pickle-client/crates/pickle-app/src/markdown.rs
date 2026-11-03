//! Discord-flavoured markdown → flat list of styled segments.

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Style {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub code: bool,
    pub spoiler: bool,
    pub heading: u8,
    pub subtext: bool,
    pub quote: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Seg {
    Text(String, Style),
    Link(String),
    UserMention(u64),
    ChannelMention(u64),
    RoleMention,
    Everyone(String),
    Emoji { name: String, id: u64, animated: bool },
    Timestamp(i64, char),
    CodeBlock { lang: String, code: String },
    Newline,
}

pub fn parse(src: &str) -> Vec<Seg> {
    let mut out = Vec::new();
    let mut rest = src;
    while let Some(start) = rest.find("```") {
        if let Some(end) = rest[start + 3..].find("```") {
            inline_block(&rest[..start], &mut out);
            let body = &rest[start + 3..start + 3 + end];
            let (lang, code) = match body.split_once('\n') {
                Some((l, c)) if !l.contains(' ') && l.len() < 20 => (l.to_string(), c.to_string()),
                _ => (String::new(), body.to_string()),
            };
            out.push(Seg::CodeBlock { lang, code: code.trim_end_matches('\n').to_string() });
            rest = &rest[start + 3 + end + 3..];
            if let Some(r) = rest.strip_prefix('\n') {
                rest = r;
            }
        } else {
            break;
        }
    }
    inline_block(rest, &mut out);
    while matches!(out.last(), Some(Seg::Newline)) {
        out.pop();
    }
    out
}

fn inline_block(text: &str, out: &mut Vec<Seg>) {
    if text.is_empty() {
        return;
    }
    let lines: Vec<&str> = text.split('\n').collect();
    for (i, line) in lines.iter().enumerate() {
        let mut style = Style::default();
        let mut l = *line;
        if let Some(r) = l.strip_prefix("> ") {
            style.quote = true;
            l = r;
        }
        for (prefix, level) in [("### ", 3u8), ("## ", 2), ("# ", 1)] {
            if let Some(r) = l.strip_prefix(prefix) {
                style.heading = level;
                l = r;
                break;
            }
        }
        if let Some(r) = l.strip_prefix("-# ") {
            style.subtext = true;
            l = r;
        }
        inline(l, style, out);
        if i + 1 < lines.len() {
            out.push(Seg::Newline);
        }
    }
}

const MARKERS: [&str; 7] = ["**", "__", "~~", "||", "`", "*", "_"];

fn push_text(out: &mut Vec<Seg>, s: &str, st: Style) {
    if s.is_empty() {
        return;
    }
    if let Some(Seg::Text(prev, ps)) = out.last_mut() {
        if *ps == st {
            prev.push_str(s);
            return;
        }
    }
    out.push(Seg::Text(s.to_string(), st));
}

fn inline(line: &str, base: Style, out: &mut Vec<Seg>) {
    let mut st = base;
    let mut buf = String::new();
    let mut i = 0;
    while i < line.len() {
        let rest = &line[i..];
        // escapes
        if rest.starts_with('\\') && rest.len() > 1 {
            let next = rest[1..].chars().next().unwrap();
            if "*_~|`\\<>#-".contains(next) {
                buf.push(next);
                i += 1 + next.len_utf8();
                continue;
            }
        }
        // inline code: no formatting inside
        if !st.code && rest.starts_with('`') {
            if let Some(end) = rest[1..].find('`') {
                push_text(out, &buf, st);
                buf.clear();
                let mut cs = st;
                cs.code = true;
                push_text(out, &rest[1..1 + end], cs);
                i += end + 2;
                continue;
            }
        }
        // links
        if rest.starts_with("https://") || rest.starts_with("http://") {
            let end = rest.find(|c: char| c.is_whitespace() || c == '>' || c == '<').unwrap_or(rest.len());
            let mut url = &rest[..end];
            while url.ends_with(['.', ',', ')', '!', '?', ';', ':']) && !url.ends_with("/)") {
                url = &url[..url.len() - 1];
            }
            push_text(out, &buf, st);
            buf.clear();
            out.push(Seg::Link(url.to_string()));
            i += url.len();
            continue;
        }
        // <@id> <#id> <:emoji:id> <t:ts:f> <https://...>
        if rest.starts_with('<') {
            if let Some(end) = rest.find('>') {
                let inner = &rest[1..end];
                let seg = if let Some(id) = inner.strip_prefix("@&") {
                    id.parse::<u64>().ok().map(|_| Seg::RoleMention)
                } else if let Some(id) = inner.strip_prefix("@!").or_else(|| inner.strip_prefix('@')) {
                    id.parse().ok().map(Seg::UserMention)
                } else if let Some(id) = inner.strip_prefix('#') {
                    id.parse().ok().map(Seg::ChannelMention)
                } else if let Some(t) = inner.strip_prefix("t:") {
                    let (ts, fmt) = t.split_once(':').unwrap_or((t, "f"));
                    ts.parse().ok().map(|v| Seg::Timestamp(v, fmt.chars().next().unwrap_or('f')))
                } else if inner.starts_with("http") {
                    Some(Seg::Link(inner.to_string()))
                } else {
                    let (animated, e) = match inner.strip_prefix("a:") {
                        Some(e) => (true, e),
                        None => (false, inner.strip_prefix(':').unwrap_or("")),
                    };
                    e.split_once(':').and_then(|(name, id)| {
                        id.parse().ok().map(|id| Seg::Emoji { name: name.to_string(), id, animated })
                    })
                };
                if let Some(seg) = seg {
                    push_text(out, &buf, st);
                    buf.clear();
                    out.push(seg);
                    i += end + 1;
                    continue;
                }
            }
        }
        if rest.starts_with("@everyone") || rest.starts_with("@here") {
            let word = if rest.starts_with("@everyone") { "@everyone" } else { "@here" };
            push_text(out, &buf, st);
            buf.clear();
            out.push(Seg::Everyone(word.into()));
            i += word.len();
            continue;
        }
        // emphasis markers: toggle only if balanced on this line
        let mut matched = false;
        for m in MARKERS {
            if m == "`" || !rest.starts_with(m) {
                continue;
            }
            let flag_on = match m {
                "**" => st.bold,
                "__" => st.underline,
                "~~" => st.strike,
                "||" => st.spoiler,
                _ => st.italic,
            };
            // single `_` only at word boundaries (snake_case stays intact)
            if m == "_" {
                let prev_alnum = line[..i].chars().next_back().map(|c| c.is_alphanumeric()).unwrap_or(false);
                let next_alnum = rest[1..].chars().next().map(|c| c.is_alphanumeric()).unwrap_or(false);
                if (!flag_on && prev_alnum) || (flag_on && next_alnum) {
                    continue;
                }
            }
            if !flag_on && !rest[m.len()..].contains(m) {
                continue;
            }
            push_text(out, &buf, st);
            buf.clear();
            match m {
                "**" => st.bold = !st.bold,
                "__" => st.underline = !st.underline,
                "~~" => st.strike = !st.strike,
                "||" => st.spoiler = !st.spoiler,
                _ => st.italic = !st.italic,
            }
            i += m.len();
            matched = true;
            break;
        }
        if matched {
            continue;
        }
        let ch = rest.chars().next().unwrap();
        buf.push(ch);
        i += ch.len_utf8();
    }
    push_text(out, &buf, st);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_formatting() {
        let s = parse("**bold** and *it* `co*de*` snake_case_name");
        assert!(matches!(&s[0], Seg::Text(t, st) if t == "bold" && st.bold));
        assert!(s.iter().any(|x| matches!(x, Seg::Text(t, st) if t == "it" && st.italic)));
        assert!(s.iter().any(|x| matches!(x, Seg::Text(t, st) if t == "co*de*" && st.code)));
        assert!(s.iter().any(|x| matches!(x, Seg::Text(t, _) if t.contains("snake_case_name"))));
    }

    #[test]
    fn mentions_links_emoji_codeblocks() {
        let s = parse("hey <@123> see https://x.dev/a. <:pickle:42>\n```rust\nfn main() {}\n```done");
        assert!(s.contains(&Seg::UserMention(123)));
        assert!(s.contains(&Seg::Link("https://x.dev/a".into())));
        assert!(s.contains(&Seg::Emoji { name: "pickle".into(), id: 42, animated: false }));
        assert!(s.contains(&Seg::CodeBlock { lang: "rust".into(), code: "fn main() {}".into() }));
        assert!(matches!(s.last(), Some(Seg::Text(t, _)) if t == "done"));
    }

    #[test]
    fn unbalanced_markers_are_literal() {
        let s = parse("2 * 3 = 6");
        assert_eq!(s, vec![Seg::Text("2 * 3 = 6".into(), Style::default())]);
    }
}
