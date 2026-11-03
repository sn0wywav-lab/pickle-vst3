use super::*;
use std::time::Duration;

fn temp_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("pickle-plugin-test-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("plugins")).unwrap();
    d
}

fn host_with(tag: &str, files: &[(&str, &str)]) -> (PluginHost, PathBuf) {
    let dir = temp_dir(tag);
    let mut enabled = HashSet::new();
    for (name, src) in files {
        std::fs::write(dir.join("plugins").join(name), src).unwrap();
        enabled.insert(name.to_string());
    }
    let host = PluginHost::start(
        HostConfig { plugins_dir: dir.join("plugins"), data_dir: dir.join("data"), enabled },
        None,
    );
    (host, dir)
}

fn wait_for(host: &PluginHost, mut pred: impl FnMut(&PluginOut) -> bool) -> Vec<PluginOut> {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    let mut seen = Vec::new();
    while std::time::Instant::now() < deadline {
        for o in host.drain() {
            let hit = pred(&o);
            seen.push(o);
            if hit {
                return seen;
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("timed out; saw {seen:#?}\nlogs: {:#?}", host.logs());
}

const SHRUG: &str = include_str!("../../../examples/plugins/BetterShrug.plugin.js");
const UWU: &str = include_str!("../../../examples/plugins/UwUTools.js");
const TOOLS: &str = include_str!("../../../examples/plugins/pickle-tools.js");

#[test]
fn example_plugins_all_start() {
    let (host, _dir) = host_with("start", &[("BetterShrug.plugin.js", SHRUG), ("UwUTools.js", UWU), ("pickle-tools.js", TOOLS)]);
    let (mut css, mut changed) = (false, false);
    let seen = wait_for(&host, |o| {
        css |= matches!(o, PluginOut::Css { css: Some(c), .. } if c.contains("--status-positive"));
        changed |= matches!(o, PluginOut::PluginsChanged);
        css && changed
    });
    assert!(seen.iter().any(|o| matches!(o, PluginOut::Toast { text, .. } if text.contains("started (1x)"))));
    let plugins = host.plugins();
    assert_eq!(plugins.len(), 3);
    for p in &plugins {
        assert!(p.running, "{} not running: {:?}", p.name, p.error);
    }
    let kinds: Vec<PluginKind> = plugins.iter().map(|p| p.kind).collect();
    assert!(kinds.contains(&PluginKind::BetterDiscord));
    assert!(kinds.contains(&PluginKind::Vencord));
    assert!(kinds.contains(&PluginKind::Pickle));
    // Vencord metadata comes from the definePlugin object.
    assert!(plugins.iter().any(|p| p.name == "UwUTools" && p.author == "Pickle"));
    let cmds: Vec<String> = host.commands().into_iter().map(|c| c.name).collect();
    for c in ["uwu", "coinflip", "now", "count"] {
        assert!(cmds.contains(&c.to_string()), "missing /{c}: {cmds:?}");
    }
    assert!(host.has_presend_hooks());
    // the Patcher stub warned instead of crashing
    assert!(host.logs().iter().any(|l| l.text.contains("Patcher")));
}

#[test]
fn presend_hooks_chain() {
    let (host, _dir) = host_with("presend", &[("BetterShrug.plugin.js", SHRUG), ("UwUTools.js", UWU)]);
    wait_for(&host, |o| matches!(o, PluginOut::PluginsChanged));
    host.send(HostCmd::PreSend { channel_id: "1".into(), content: "!uwu hello world :shrug:".into() });
    let seen = wait_for(&host, |o| matches!(o, PluginOut::SendMessage { .. }));
    match seen.last().unwrap() {
        PluginOut::SendMessage { channel_id, content } => {
            assert_eq!(channel_id, "1");
            assert_eq!(content, "hewwo wowwd ¯\\_(ツ)_/¯");
        }
        _ => unreachable!(),
    }
}

#[test]
fn commands_run_async_and_local() {
    let (host, _dir) = host_with("cmd", &[("UwUTools.js", UWU), ("pickle-tools.js", TOOLS)]);
    wait_for(&host, |o| matches!(o, PluginOut::PluginsChanged));
    let ctx = r#"{"channel":{"id":"42"},"guild":null}"#;
    host.send(HostCmd::RunCommand {
        name: "uwu".into(),
        args_json: r#"[{"name":"text","value":"really lovely"}]"#.into(),
        ctx_json: ctx.into(),
    });
    let seen = wait_for(&host, |o| matches!(o, PluginOut::SendMessage { .. }));
    assert!(matches!(seen.last(), Some(PluginOut::SendMessage { content, channel_id }) if content == "weawwy wovewy" && channel_id == "42"));

    host.send(HostCmd::Event { name: "MESSAGE_CREATE".into(), json: r#"{"message":{"content":"hi","channel_id":"42"}}"#.into() });
    host.send(HostCmd::RunCommand { name: "count".into(), args_json: "[]".into(), ctx_json: ctx.into() });
    let seen = wait_for(&host, |o| matches!(o, PluginOut::LocalMessage { .. }));
    assert!(matches!(seen.last(), Some(PluginOut::LocalMessage { content, .. }) if content == "Seen 1 messages so far."));
}

#[test]
fn timers_fetch_and_watchdog() {
    let script = r#"
        setTimeout(() => Pickle.toast("timeout fired"), 20);
        let ticks = 0;
        const iv = setInterval(() => { if (++ticks === 3) { clearInterval(iv); Pickle.toast("interval x3"); } }, 5);
        Pickle.commands.register({ name: "get", execute: async () => {
            const r = await fetch("https://example.invalid/data");
            const j = await r.json();
            return { content: "status " + r.status + " value " + j.value };
        }});
        Pickle.commands.register({ name: "spin", execute: () => { for (;;) {} } });
    "#;
    let (host, _dir) = host_with("timers", &[("t.js", script)]);
    let (mut timeout, mut interval) = (false, false);
    wait_for(&host, |o| {
        if let PluginOut::Toast { text, .. } = o {
            timeout |= text == "timeout fired";
            interval |= text == "interval x3";
        }
        timeout && interval
    });

    let ctx = r#"{"channel":{"id":"7"}}"#;
    host.send(HostCmd::RunCommand { name: "get".into(), args_json: "[]".into(), ctx_json: ctx.into() });
    let seen = wait_for(&host, |o| matches!(o, PluginOut::Fetch { .. }));
    let PluginOut::Fetch { plugin, id, url, .. } = seen.last().unwrap().clone() else { unreachable!() };
    assert_eq!(url, "https://example.invalid/data");
    host.send(HostCmd::FetchDone { plugin, id, result: Ok((200, r#"{"value": 5}"#.into())) });
    let seen = wait_for(&host, |o| matches!(o, PluginOut::SendMessage { .. }));
    assert!(matches!(seen.last(), Some(PluginOut::SendMessage { content, .. }) if content == "status 200 value 5"));

    // An infinite loop is interrupted by the watchdog; the host survives.
    host.send(HostCmd::RunCommand { name: "spin".into(), args_json: "[]".into(), ctx_json: ctx.into() });
    host.send(HostCmd::RunCommand { name: "get".into(), args_json: "[]".into(), ctx_json: ctx.into() });
    wait_for(&host, |o| matches!(o, PluginOut::Fetch { .. }));
}

#[test]
fn unsupported_vencord_import_reports_error() {
    let src = "import { Button } from \"@components/Button\";\nexport default { name: \"x\" };";
    let (host, _dir) = host_with("bad", &[("bad.js", src)]);
    wait_for(&host, |o| matches!(o, PluginOut::PluginsChanged));
    let p = &host.plugins()[0];
    assert!(!p.running);
    assert!(p.error.as_deref().unwrap_or("").contains("@components/Button"), "{:?}", p.error);
}


