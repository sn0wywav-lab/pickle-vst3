//! Go Live picker dialog and the stream-quality controls shared with the
//! Streaming settings tab.

use crate::app::RustcordApp;
use crate::backend::{Cmd, ScreenInfo};
use crate::ui::theme::{over, Colors};
use eframe::egui::{self, Color32, CornerRadius, RichText};
use rustcord_voice::settings::*;

/// Source picker (screens first, then windows).
pub fn source_picker(ui: &mut egui::Ui, s: &mut StreamSettings, screens: &[ScreenInfo], k: Colors) {
    if screens.is_empty() {
        ui.label(RichText::new("Looking for screens…").color(k.muted));
        return;
    }
    let selected = s.source.clone().or_else(|| screens.iter().find(|x| x.primary).or(screens.first()).map(|x| x.id.clone()));
    egui::ScrollArea::vertical().max_height(190.0).id_salt("sources").show(ui, |ui| {
        for (title, monitors) in [("SCREENS", true), ("WINDOWS", false)] {
            let list: Vec<&ScreenInfo> = screens.iter().filter(|x| x.is_monitor == monitors).collect();
            if list.is_empty() {
                continue;
            }
            ui.label(RichText::new(title).size(11.0).strong().color(k.interactive));
            for src in list {
                let sel = selected.as_deref() == Some(src.id.as_str());
                let detail = if src.is_monitor {
                    let hz = if src.refresh_hz > 0.0 { format!(" · {:.0} Hz", src.refresh_hz) } else { String::new() };
                    format!("{}×{}{hz}{}", src.width, src.height, if src.primary { " · primary" } else { "" })
                } else {
                    format!("{}×{}", src.width, src.height)
                };
                let label = format!("{}  {}   {}", if src.is_monitor { "🖥" } else { "🗗" }, src.name, detail);
                if ui.add(egui::Button::selectable(sel, label)).clicked() {
                    s.source = Some(src.id.clone());
                }
            }
        }
    });
}

/// The best the selected source can do: native size, its refresh rate.
pub fn max_out(s: &mut StreamSettings, screens: &[ScreenInfo]) {
    let src = s
        .source
        .as_ref()
        .and_then(|id| screens.iter().find(|x| &x.id == id))
        .or_else(|| screens.iter().find(|x| x.primary))
        .or(screens.first());
    s.max_height = 0;
    s.fps = src.map(|x| x.refresh_hz.round() as u32).filter(|f| *f >= 30).unwrap_or(60).min(MAX_STREAM_FPS);
    s.bitrate_kbps = 50_000;
    s.effort = EncoderEffort::Quality;
    s.audio_enabled = true;
    s.audio.bitrate_kbps = 510;
    s.audio.channels = Channels::Stereo;
    s.audio.bandwidth = Bandwidth::Full;
}

pub fn quality_controls(ui: &mut egui::Ui, s: &mut StreamSettings, k: Colors) {
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("Resolution").strong());
        for (label, h) in [("Source", 0), ("2160p", 2160), ("1440p", 1440), ("1080p", 1080), ("720p", 720)] {
            if ui.selectable_label(s.max_height == h, label).clicked() {
                s.max_height = h;
            }
        }
    });
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("Frame rate").strong());
        for f in [30, 60, 120, 144, 165, 240] {
            if ui.selectable_label(s.fps == f, format!("{f}")).clicked() {
                s.fps = f;
            }
        }
    });
    ui.add(egui::Slider::new(&mut s.fps, 1..=MAX_STREAM_FPS).suffix(" fps").text("custom fps"));
    ui.add(egui::Slider::new(&mut s.bitrate_kbps, 500..=MAX_VIDEO_BITRATE_KBPS).logarithmic(true).suffix(" kb/s").text("video bitrate"));
    ui.horizontal_wrapped(|ui| {
        for (l, v) in [("6 Mb", 6_000), ("12 Mb", 12_000), ("25 Mb", 25_000), ("50 Mb", 50_000), ("MAX", MAX_VIDEO_BITRATE_KBPS)] {
            if ui.selectable_label(s.bitrate_kbps == v, l).clicked() {
                s.bitrate_kbps = v;
            }
        }
    });
    ui.horizontal(|ui| {
        ui.label("Optimise for");
        ui.radio_value(&mut s.content, StreamContent::Clarity, "Clarity (text, UI)");
        ui.radio_value(&mut s.content, StreamContent::Motion, "Motion (games, video)");
    });
    ui.horizontal(|ui| {
        ui.checkbox(&mut s.audio_enabled, "Stream audio");
        if s.audio_enabled {
            ui.label(RichText::new(format!("{} kb/s {}", s.audio.effective_bitrate_kbps(), if s.audio.channels == Channels::Stereo { "stereo" } else { "mono" })).color(k.muted));
        }
    });
}

/// Modal shown by the "Go Live" button.
pub fn dialog(app: &mut RustcordApp, ctx: &egui::Context) {
    if !app.golive_open {
        return;
    }
    let k = app.colors;
    let mut s = app.cfg.stream.clone();
    let before = s.clone();
    let mut close = false;
    let mut go = false;
    egui::Modal::new(egui::Id::unique("golive")).show(ctx, |ui| {
        ui.set_width(560.0);
        ui.horizontal(|ui| {
            ui.heading("Go Live");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("✖").clicked() {
                    close = true;
                }
                if ui.small_button("↻").on_hover_text("Refresh screens and windows").clicked() {
                    app.backend.send(Cmd::ListScreens);
                }
            });
        });
        ui.add_space(4.0);
        egui::Frame::new().fill(over(k.bg_secondary, Color32::BLACK)).corner_radius(CornerRadius::same(8)).inner_margin(egui::Margin::same(10)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            source_picker(ui, &mut s, &app.settings.screens, k);
        });
        ui.add_space(8.0);
        egui::Frame::new().fill(over(k.bg_secondary, Color32::BLACK)).corner_radius(CornerRadius::same(8)).inner_margin(egui::Margin::same(10)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            quality_controls(ui, &mut s, k);
        });
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if ui.add(egui::Button::new(RichText::new("⚡ Max out").strong())).on_hover_text("Native resolution, the screen's refresh rate, 50 Mb/s, best encoder effort, 510 kb/s stereo audio").clicked() {
                max_out(&mut s, &app.settings.screens);
            }
            if ui.button("More options…").clicked() {
                app.settings.open = true;
                app.settings.tab = crate::app::SettingsTab::Streaming;
                close = true;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let label = RichText::new("Go Live").strong().color(Color32::WHITE);
                if ui.add_sized([110.0, 32.0], egui::Button::new(label).fill(k.brand)).clicked() {
                    go = true;
                }
            });
        });
        ui.label(RichText::new("Viewers need the bandwidth and a decoder that keeps up with what you pick.").size(11.5).color(k.muted));
    });
    if s != before {
        app.set_stream(s);
    }
    if go {
        app.backend.send(Cmd::GoLive(true));
        close = true;
    }
    if close {
        app.golive_open = false;
    }
}
