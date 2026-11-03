//! Settings window. The Voice tab is where every Opus knob is unlocked.

use crate::app::{RustcordApp, SettingsTab};
use crate::backend::Cmd;
use crate::ui::theme::over;
use eframe::egui::{self, Color32, CornerRadius, RichText, Vec2};
use rustcord_plugins::HostCmd;
use rustcord_voice::settings::*;
use std::sync::atomic::Ordering;

pub fn window(app: &mut RustcordApp, ctx: &egui::Context) {
    if !app.settings.open {
        return;
    }
    if !app.settings.devices_requested {
        app.settings.devices_requested = true;
        app.backend.send(Cmd::ListDevices);
    }
    let mut open = true;
    egui::Window::new("Settings")
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .default_size([860.0, 640.0])
        .min_size([640.0, 420.0])
        .show(ctx, |ui| {
            egui::Panel::left("settings_tabs")
                .exact_size(170.0)
                .resizable(false)
                .show(ui, |ui| {
                    ui.add_space(6.0);
                    for (tab, label) in [
                        (SettingsTab::Voice, "🎤  Voice & Audio"),
                        (SettingsTab::Video, "📷  Video"),
                        (SettingsTab::Streaming, "🖥  Streaming"),
                        (SettingsTab::Appearance, "🎨  Appearance"),
                        (SettingsTab::Plugins, "🧩  Plugins"),
                        (SettingsTab::Account, "👤  Account"),
                        (SettingsTab::About, "ℹ  About"),
                    ] {
                        let sel = app.settings.tab == tab;
                        if ui.add_sized([158.0, 30.0], egui::Button::selectable(sel, label)).clicked() {
                            app.settings.tab = tab;
                        }
                    }
                });
            egui::CentralPanel::default().show(ui, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    ui.set_max_width(640.0);
                    match app.settings.tab {
                        SettingsTab::Voice => voice(app, ui),
                        SettingsTab::Video => video(app, ui),
                        SettingsTab::Streaming => streaming(app, ui),
                        SettingsTab::Appearance => appearance(app, ui),
                        SettingsTab::Plugins => plugins(app, ui),
                        SettingsTab::Account => account(app, ui),
                        SettingsTab::About => about(app, ui),
                    }
                });
            });
        });
    if !open {
        app.settings.open = false;
        if app.media.mic_test {
            app.backend.send(Cmd::MicTest(false));
        }
        if !app.media.camera_on {
            app.backend.send(Cmd::CameraPreview(false));
        }
        app.save_now();
    }
}

fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(14.0);
    ui.label(RichText::new(title.to_uppercase()).size(11.5).strong().color(ui.visuals().widgets.inactive.fg_stroke.color));
    ui.add_space(4.0);
}

fn card<R>(k: crate::ui::theme::Colors, ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::new()
        .fill(over(k.bg_secondary, Color32::BLACK))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(egui::Margin::same(12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

fn device_combo(ui: &mut egui::Ui, id: &str, current: &mut Option<String>, devices: &[rustcord_voice::devices::DeviceInfo]) -> bool {
    let mut changed = false;
    let label = current.clone().unwrap_or_else(|| "System default".into());
    egui::ComboBox::from_id_salt(id).width(380.0).selected_text(label).show_ui(ui, |ui| {
        changed |= ui.selectable_value(current, None, "System default").changed();
        for d in devices {
            let text = format!("{}{}", d.name, if d.is_default { "  (default)" } else { "" });
            changed |= ui.selectable_value(current, Some(d.name.clone()), text).changed();
        }
    });
    changed
}

fn meter(ui: &mut egui::Ui, db: f32, gate: Option<f32>, k: crate::ui::theme::Colors) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width().min(420.0), 10.0), egui::Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(5), k.bg_tertiary);
    let frac = ((db + 60.0) / 60.0).clamp(0.0, 1.0);
    let color = if db > -3.0 { k.danger } else if db > -12.0 { k.warning } else { k.positive };
    p.rect_filled(egui::Rect::from_min_size(rect.min, Vec2::new(rect.width() * frac, rect.height())), CornerRadius::same(5), color);
    if let Some(g) = gate {
        let x = rect.left() + rect.width() * ((g + 60.0) / 60.0).clamp(0.0, 1.0);
        p.line_segment([egui::pos2(x, rect.top() - 2.0), egui::pos2(x, rect.bottom() + 2.0)], egui::Stroke::new(2.0, k.header));
    }
}

fn voice(app: &mut RustcordApp, ui: &mut egui::Ui) {
    let k = app.colors;
    let mut s = app.cfg.audio.clone();
    let before = s.clone();

    ui.horizontal(|ui| {
        ui.heading("Voice & Audio");
        let (txt, col) = if s.is_filterless() { ("TRUE FILTERLESS", k.positive) } else { ("PROCESSING ON", k.warning) };
        ui.label(RichText::new(txt).strong().size(11.0).color(Color32::WHITE).background_color(col));
    });

    section(ui, "Devices");
    card(app.colors, ui, |ui| {
        egui::Grid::new("devices").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
            ui.label("Input");
            device_combo(ui, "in_dev", &mut s.input_device, &app.settings.inputs);
            ui.end_row();
            ui.label("Output");
            device_combo(ui, "out_dev", &mut s.output_device, &app.settings.outputs);
            ui.end_row();
            ui.label("Capture rate");
            let rates: Vec<u32> = app
                .settings
                .inputs
                .iter()
                .find(|d| Some(&d.name) == s.input_device.as_ref() || (s.input_device.is_none() && d.is_default))
                .map(|d| d.sample_rates.clone())
                .unwrap_or_else(|| vec![44_100, 48_000, 96_000]);
            let label = |r: u32| if r == 0 { "Auto (48 kHz)".to_string() } else { format!("{:.1} kHz", r as f32 / 1000.0) };
            egui::ComboBox::from_id_salt("cap_rate").selected_text(label(s.capture_sample_rate)).show_ui(ui, |ui| {
                ui.selectable_value(&mut s.capture_sample_rate, 0, label(0));
                for r in rates {
                    ui.selectable_value(&mut s.capture_sample_rate, r, label(r));
                }
            });
            ui.end_row();
        });
        if ui.small_button("↻ Refresh devices").clicked() {
            app.backend.send(Cmd::ListDevices);
        }
        if let Some(e) = &app.media.engine {
            let fmt = e.input_format.lock().clone();
            if let Some(f) = fmt {
                let transparent = f.sample_rate == 48_000;
                ui.label(
                    RichText::new(format!(
                        "Mic running at {} Hz · {} ch · {}{}",
                        f.sample_rate,
                        f.channels,
                        f.sample_format,
                        if transparent { " · bit-transparent to Opus (no resampling)" } else { " · resampled to 48 kHz for Opus" }
                    ))
                    .size(11.5)
                    .color(k.muted),
                );
            }
        }
    });

    section(ui, "Mic test");
    card(app.colors, ui, |ui| {
        let gate = s.voice_gate.then_some(s.voice_gate_threshold_db);
        meter(ui, app.input_meter_db, gate, k);
        ui.label(RichText::new(format!("{:.1} dBFS peak", app.input_meter_db.max(-99.0))).size(11.5).color(k.muted));
        ui.horizontal(|ui| {
            let on = app.media.mic_test;
            if ui.button(if on { "■ Stop test" } else { "▶ Let's check" }).clicked() {
                app.backend.send(Cmd::MicTest(!on));
            }
            ui.label(RichText::new("Plays your mic back through the exact encoder settings below — what others hear.").size(11.5).color(k.muted));
        });
    });

    section(ui, "Unlocked quality (Opus encoder)");
    card(app.colors, ui, |ui| opus_controls(ui, &mut s, k));

    section(ui, "Processing");
    card(app.colors, ui, |ui| {
        let off = |ui: &mut egui::Ui, name: &str, on: bool| {
            ui.horizontal(|ui| {
                ui.label(name);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(if on { "ON" } else { "OFF" }).strong().color(if on { k.warning } else { k.positive }));
                });
            });
        };
        off(ui, "Noise suppression", false);
        off(ui, "Echo cancellation", false);
        off(ui, "Automatic gain control", false);
        off(ui, "High-pass filter", s.application == Application::Voice);
        ui.separator();
        ui.checkbox(&mut s.voice_gate, "Voice gate (only transmit above a level)");
        if s.voice_gate {
            ui.add(egui::Slider::new(&mut s.voice_gate_threshold_db, -80.0..=-10.0).suffix(" dB").text("threshold"));
        }
        ui.horizontal(|ui| {
            ui.add(egui::Slider::new(&mut s.input_gain_db, -24.0..=24.0).suffix(" dB").text("input gain"));
            if s.input_gain_db != 0.0 && ui.small_button("0 dB").clicked() {
                s.input_gain_db = 0.0;
            }
        });
        ui.label(RichText::new("At 0 dB the samples reach the encoder untouched.").size(11.5).color(k.muted));
    });

    section(ui, "Output");
    card(app.colors, ui, |ui| {
        let mut vol = s.output_volume * 100.0;
        if ui.add(egui::Slider::new(&mut vol, 0.0..=200.0).suffix("%").text("volume")).changed() {
            s.output_volume = vol / 100.0;
        }
        ui.add(egui::Slider::new(&mut s.jitter_ms, 0..=200).suffix(" ms").text("jitter buffer"));
        ui.label(RichText::new("Lower = less latency, higher = smoother on bad networks.").size(11.5).color(k.muted));
    });

    if let Some(e) = app.media.engine.clone() {
        section(ui, "Live stats");
        card(app.colors, ui, |ui| {
            ui.label(format!(
                "Sent {} packets · {:.0} kb/s live · received {} · concealed {}",
                e.packets_sent.load(Ordering::Relaxed),
                app.bitrate_kbps,
                e.packets_received.load(Ordering::Relaxed),
                e.packets_concealed.load(Ordering::Relaxed)
            ));
            if let Some(f) = e.output_format.lock().clone() {
                ui.label(RichText::new(format!("Output: {} @ {} Hz · {} ch", f.name, f.sample_rate, f.channels)).size(11.5).color(k.muted));
            }
            for err in e.errors.lock().iter().rev().take(3) {
                ui.colored_label(k.danger, err);
            }
        });
    }

    if s != before {
        app.set_audio(s);
    }
}


/// Every Opus encoder knob. Shared by the mic and the stream audio.
fn opus_controls(ui: &mut egui::Ui, s: &mut AudioSettings, k: crate::ui::theme::Colors) {
    ui.label(RichText::new("Bitrate").strong());
    ui.add(egui::Slider::new(&mut s.bitrate_kbps, MIN_BITRATE_KBPS..=MAX_BITRATE_KBPS).logarithmic(true).suffix(" kb/s").integer());
    ui.horizontal_wrapped(|ui| {
        for (label, v) in [("64 (Discord)", 64), ("96", 96), ("128 (Nitro)", 128), ("256", 256), ("384", 384), ("510", 510), ("MAX", MAX_BITRATE_KBPS)] {
            if ui.selectable_label(s.bitrate_kbps == v, label).clicked() {
                s.bitrate_kbps = v;
                if v == MAX_BITRATE_KBPS {
                    s.frame = FrameDuration::Ms10;
                    s.channels = Channels::Stereo;
                }
            }
        }
    });
    let eff = s.effective_bitrate_kbps();
    if eff != s.bitrate_kbps {
        ui.label(
            RichText::new(format!(
                "Effective {eff} kb/s — Opus caps at 300 kb/s per channel and 1275 bytes per frame ({} frames → {} kb/s). Use stereo + 10 ms frames for 600 kb/s.",
                s.frame.label(),
                (1275.0 * 8.0 * 48.0 / s.frame.samples_48k().min(960) as f32) as u32
            ))
            .size(11.5)
            .color(k.warning),
        );
    }

    ui.add_space(8.0);
    ui.label(RichText::new("Sample rate (coded bandwidth)").strong());
    let mut idx = s.bandwidth.index();
    let resp = ui.add(
        egui::Slider::new(&mut idx, 0..=4)
            .show_value(false)
            .text(format!("{} kHz  ·  audio up to {} kHz", s.bandwidth.sample_rate_khz(), s.bandwidth.audio_bandwidth_khz())),
    );
    if resp.changed() {
        s.bandwidth = Bandwidth::ALL[idx];
    }

    ui.add_space(8.0);
    egui::Grid::new("opus_grid").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
        ui.label("Channels");
        ui.horizontal(|ui| {
            ui.radio_value(&mut s.channels, Channels::Stereo, "Stereo");
            ui.radio_value(&mut s.channels, Channels::Mono, "Mono");
        });
        ui.end_row();

        ui.label("Frame size");
        ui.horizontal_wrapped(|ui| {
            for f in FrameDuration::ALL {
                ui.radio_value(&mut s.frame, f, f.label());
            }
        });
        ui.end_row();

        ui.label("Complexity");
        ui.add(egui::Slider::new(&mut s.complexity, 0..=10));
        ui.end_row();

        ui.label("Mode");
        ui.vertical(|ui| {
            ui.radio_value(&mut s.application, Application::Music, "Full fidelity (filterless)");
            ui.radio_value(&mut s.application, Application::LowDelay, "Low delay (filterless, CELT only)");
            ui.radio_value(&mut s.application, Application::Voice, "Voice (adds Opus' speech high-pass)");
        });
        ui.end_row();

        ui.label("Signal hint");
        ui.horizontal(|ui| {
            ui.radio_value(&mut s.signal, Signal::Music, "Music");
            ui.radio_value(&mut s.signal, Signal::Auto, "Auto");
            ui.radio_value(&mut s.signal, Signal::Voice, "Voice");
        });
        ui.end_row();

        ui.label("Rate control");
        ui.horizontal(|ui| {
            ui.checkbox(&mut s.vbr, "VBR");
            ui.add_enabled(s.vbr, egui::Checkbox::new(&mut s.constrained_vbr, "Constrained"));
        });
        ui.end_row();

        ui.label("Loss protection");
        ui.horizontal(|ui| {
            ui.checkbox(&mut s.fec, "In-band FEC");
            ui.add_enabled(s.fec, egui::Slider::new(&mut s.expected_packet_loss, 0..=50).suffix("% loss"));
        });
        ui.end_row();

        ui.label("Silence");
        ui.checkbox(&mut s.dtx, "DTX (send nothing during silence)");
        ui.end_row();

        ui.label("Stereo phase");
        ui.checkbox(&mut s.phase_inversion_disabled, "Disable phase inversion (mono-compatible)");
        ui.end_row();
    });
}

fn video(app: &mut RustcordApp, ui: &mut egui::Ui) {
    let k = app.colors;
    let mut s = app.cfg.video.clone();
    let before = s.clone();
    ui.heading("Video");

    section(ui, "Camera");
    card(app.colors, ui, |ui| {
        if !cfg!(feature = "camera") {
            ui.colored_label(k.warning, "This build was compiled without camera support.");
        }
        let current = app.settings.cameras.iter().find(|c| c.index == s.camera_index).map(|c| c.name.clone()).unwrap_or_else(|| format!("Camera {}", s.camera_index));
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("camera").width(320.0).selected_text(current).show_ui(ui, |ui| {
                for c in &app.settings.cameras {
                    if ui.selectable_label(s.camera_index == c.index, &c.name).clicked() {
                        s.camera_index = c.index;
                        s.camera_name = Some(c.name.clone());
                    }
                }
            });
            if ui.button("Detect modes").clicked() {
                app.backend.send(Cmd::ProbeCamera(s.camera_index));
            }
        });
        if let Some(e) = &app.settings.camera_error {
            ui.colored_label(k.danger, e);
        }
        if !app.settings.camera_modes.is_empty() {
            let best = rustcord_voice::video::best_mode(&app.settings.camera_modes);
            if let Some(b) = best {
                ui.label(RichText::new(format!("Best mode: {}", b.label())).size(11.5).color(k.muted));
            }
        }
    });

    section(ui, "Quality");
    card(app.colors, ui, |ui| {
        ui.checkbox(&mut s.max_out, RichText::new("Max out (highest resolution, then highest frame rate the camera supports)").strong());
        ui.add_enabled_ui(!s.max_out, |ui| {
            let mut sizes: Vec<(u32, u32)> = app.settings.camera_modes.iter().map(|m| (m.width, m.height)).collect();
            if sizes.is_empty() {
                sizes = vec![(640, 360), (1280, 720), (1920, 1080), (2560, 1440), (3840, 2160)];
            }
            sizes.dedup();
            egui::ComboBox::from_label("Resolution").selected_text(format!("{}×{}", s.width, s.height)).show_ui(ui, |ui| {
                for (w, h) in sizes {
                    if ui.selectable_label(s.width == w && s.height == h, format!("{w}×{h}")).clicked() {
                        s.width = w;
                        s.height = h;
                    }
                }
            });
            let max_fps = app.settings.camera_modes.iter().map(|m| m.fps).max().unwrap_or(120).max(30);
            ui.add(egui::Slider::new(&mut s.fps, 1..=max_fps).suffix(" fps").text("frame rate"));
            ui.horizontal(|ui| {
                ui.label("Pixel format");
                for (v, l) in [(PixelFormatPref::Auto, "Auto"), (PixelFormatPref::Mjpeg, "MJPEG"), (PixelFormatPref::Yuyv, "YUYV (raw)"), (PixelFormatPref::Nv12, "NV12 (raw)")] {
                    ui.radio_value(&mut s.pixel_format, v, l);
                }
            });
        });
        ui.add_space(6.0);
        ui.add(egui::Slider::new(&mut s.bitrate_kbps, 250..=MAX_VIDEO_BITRATE_KBPS).logarithmic(true).suffix(" kb/s").text("bitrate"));
        ui.horizontal_wrapped(|ui| {
            for (l, v) in [("2.5 Mb", 2_500), ("6 Mb", 6_000), ("12 Mb", 12_000), ("25 Mb", 25_000), ("50 Mb", 50_000), ("MAX", MAX_VIDEO_BITRATE_KBPS)] {
                if ui.selectable_label(s.bitrate_kbps == v, l).clicked() {
                    s.bitrate_kbps = v;
                }
            }
        });
        ui.add(egui::Slider::new(&mut s.keyframe_interval_s, 0.5..=10.0).suffix(" s").text("keyframe interval"));
        ui.horizontal(|ui| {
            ui.label("Encoder effort");
            ui.radio_value(&mut s.effort, EncoderEffort::Fast, "Fast");
            ui.radio_value(&mut s.effort, EncoderEffort::Balanced, "Balanced");
            ui.radio_value(&mut s.effort, EncoderEffort::Quality, "Quality");
        });
        ui.add(egui::Slider::new(&mut s.encoder_threads, 0..=16).text("encoder threads (0 = auto)"));
        ui.add(egui::Slider::new(&mut s.mtu_payload, 500..=1400).suffix(" B").text("RTP payload size"));
        ui.label(
            RichText::new("H.264 with no resolution, frame-rate or bitrate cap. Viewers need the bandwidth and decoder to keep up.")
                .size(11.5)
                .color(k.muted),
        );
    });

    section(ui, "Preview");
    card(app.colors, ui, |ui| {
        #[cfg(feature = "camera")]
        {
            let previewing = app.media.camera.is_some();
            if ui.button(if previewing { "■ Stop preview" } else { "▶ Preview camera" }).clicked() {
                app.backend.send(Cmd::CameraPreview(!previewing));
            }
            if let Some(stats) = app.media.camera.clone() {
                if let Some(e) = stats.error.lock().clone() {
                    ui.colored_label(k.danger, e);
                }
                if let Some(m) = stats.mode.lock().clone() {
                    ui.label(format!("Capturing {}", m.label()));
                }
                if let Some(f) = stats.preview.lock().clone() {
                    let ctx = ui.ctx().clone();
                    let tex = crate::ui::media::texture_for(app, &ctx, u64::MAX - 1, &f);
                    ui.add(egui::Image::from_texture(&tex).max_size(Vec2::new(480.0, 270.0)).corner_radius(CornerRadius::same(8)));
                }
            }
        }
    });

    if s != before {
        app.set_video(s);
    }
}

fn streaming(app: &mut RustcordApp, ui: &mut egui::Ui) {
    let k = app.colors;
    let mut s = app.cfg.stream.clone();
    let before = s.clone();
    if app.settings.screens.is_empty() && !app.settings.screens_requested {
        app.settings.screens_requested = true;
        app.backend.send(Cmd::ListScreens);
    }
    ui.horizontal(|ui| {
        ui.heading("Streaming (Go Live)");
        if app.media.live.is_some() {
            ui.label(RichText::new(" LIVE ").strong().size(11.0).color(Color32::WHITE).background_color(k.danger));
        }
    });
    ui.label(RichText::new("Changes apply instantly, even while you're live.").size(11.5).color(k.muted));

    section(ui, "Source");
    card(app.colors, ui, |ui| {
        crate::ui::golive::source_picker(ui, &mut s, &app.settings.screens, k);
        if ui.small_button("↻ Refresh").clicked() {
            app.backend.send(Cmd::ListScreens);
        }
    });

    section(ui, "Video quality");
    card(app.colors, ui, |ui| {
        if ui.button(RichText::new("⚡ Max out").strong()).on_hover_text("Native resolution, the screen's refresh rate, 50 Mb/s, best encoder effort").clicked() {
            crate::ui::golive::max_out(&mut s, &app.settings.screens);
        }
        crate::ui::golive::quality_controls(ui, &mut s, k);
        ui.add(egui::Slider::new(&mut s.keyframe_interval_s, 0.5..=10.0).suffix(" s").text("keyframe interval"));
        ui.horizontal(|ui| {
            ui.label("Encoder effort");
            ui.radio_value(&mut s.effort, EncoderEffort::Fast, "Fast");
            ui.radio_value(&mut s.effort, EncoderEffort::Balanced, "Balanced");
            ui.radio_value(&mut s.effort, EncoderEffort::Quality, "Quality");
        });
        ui.add(egui::Slider::new(&mut s.encoder_threads, 0..=32).text("encoder threads (0 = auto)"));
        ui.add(egui::Slider::new(&mut s.mtu_payload, 500..=1400).suffix(" B").text("RTP payload size"));
        ui.label(
            RichText::new("H.264 with no resolution, frame-rate or bitrate cap. Frames are captured on the GPU path (DXGI on Windows) and only the newest frame is encoded, so a busy CPU drops frames instead of adding delay.")
                .size(11.5)
                .color(k.muted),
        );
    });

    section(ui, "Stream audio");
    card(app.colors, ui, |ui| {
        ui.checkbox(&mut s.audio_enabled, "Stream my computer's audio");
        ui.add_enabled_ui(s.audio_enabled, |ui| {
            ui.checkbox(&mut s.audio_loopback, "Capture an output device (system sound)")
                .on_hover_text("Uses WASAPI loopback on Windows. On Linux/macOS pick an input such as a monitor or loopback device instead.");
            let devices = if s.audio_loopback { &app.settings.outputs } else { &app.settings.inputs };
            device_combo(ui, "stream_audio_dev", &mut s.audio.input_device, devices);
        });
    });

    section(ui, "Stream audio quality (Opus)");
    card(app.colors, ui, |ui| {
        ui.add_enabled_ui(s.audio_enabled, |ui| opus_controls(ui, &mut s.audio, k));
    });

    section(ui, "Preview");
    card(app.colors, ui, |ui| {
        let previewing = app.media.screen_preview.is_some();
        if ui.button(if previewing { "■ Stop preview" } else { "▶ Preview capture" }).clicked() {
            app.backend.send(Cmd::ScreenPreview(!previewing));
        }
        let stats = app.media.screen_preview.clone().or_else(|| app.media.live.as_ref().and_then(|l| l.stats.clone()));
        if let Some(stats) = stats {
            if let Some(e) = stats.error.lock().clone() {
                ui.colored_label(k.danger, e);
            }
            if let Some(m) = stats.mode.lock().clone() {
                ui.label(format!("{}×{} @ {} fps ({})", m.width, m.height, m.fps, m.format));
            }
            if let Some(f) = stats.preview.lock().clone() {
                let ctx = ui.ctx().clone();
                let tex = crate::ui::media::texture_for(app, &ctx, u64::MAX - 3, &f);
                ui.add(egui::Image::from_texture(&tex).max_size(Vec2::new(480.0, 270.0)).corner_radius(CornerRadius::same(8)));
            }
        }
    });

    if s != before {
        app.set_stream(s);
    }
}

fn open_path(path: &std::path::Path) {
    #[cfg(target_os = "windows")]
    let cmd = "explorer";
    #[cfg(target_os = "macos")]
    let cmd = "open";
    #[cfg(all(unix, not(target_os = "macos")))]
    let cmd = "xdg-open";
    let _ = std::process::Command::new(cmd).arg(path).spawn();
}

fn appearance(app: &mut RustcordApp, ui: &mut egui::Ui) {
    let k = app.colors;
    ui.heading("Appearance");
    section(ui, "Theme");
    card(app.colors, ui, |ui| {
        let entries = app.theme_entries.clone();
        for e in entries {
            let selected = app.cfg.theme == e.id;
            ui.horizontal(|ui| {
                if ui.radio(selected, RichText::new(e.display_name()).strong()).clicked() && !selected {
                    app.set_theme(e.id.clone());
                }
                if let Some(a) = &e.meta.author {
                    ui.label(RichText::new(format!("by {a}")).size(11.5).color(k.muted));
                }
                if e.path.is_some() {
                    ui.label(RichText::new("file").size(10.0).color(k.muted));
                }
            });
            if let Some(d) = &e.meta.description {
                ui.label(RichText::new(d).size(11.5).color(k.muted));
            }
        }
        ui.separator();
        let t = &app.theme;
        ui.label(RichText::new(format!(
            "Active: {} · {} colour tokens mapped{}{}",
            t.meta.name.clone().unwrap_or_else(|| "theme".into()),
            t.mapped_vars,
            if t.imports.is_empty() { String::new() } else { format!(" · {} remote import(s)", t.imports.len()) },
            if t.background_image.is_some() { " · background image" } else { "" }
        )).size(11.5).color(k.muted));
    });

    section(ui, "Add themes from other clients");
    card(app.colors, ui, |ui| {
        ui.label("BetterDiscord .theme.css files, Vencord/Vesktop themes and QuickCSS all work — the colour variables, @imports and background images are applied natively.");
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut app.settings.theme_import).hint_text("https://… .css URL or a local file path").desired_width(360.0));
            if ui.button("Import").clicked() {
                let src = app.settings.theme_import.trim().to_string();
                match import_theme(&app.paths.themes, &src) {
                    Ok(path) => {
                        app.settings.theme_import.clear();
                        app.rescan_themes();
                        app.set_theme(path.to_string_lossy().into_owned());
                        app.toast("Theme imported", "success");
                    }
                    Err(e) => app.toast(e, "error"),
                }
            }
        });
        ui.horizontal(|ui| {
            if ui.button("📂 Open themes folder").clicked() {
                open_path(&app.paths.themes);
            }
            if ui.button("↻ Reload").clicked() {
                app.rescan_themes();
            }
        });
    });

    section(ui, "Layout");
    card(app.colors, ui, |ui| {
        if ui.add(egui::Slider::new(&mut app.cfg.ui_scale, 0.75..=2.0).text("UI scale")).drag_stopped() {
            ui.ctx().set_zoom_factor(app.cfg.ui_scale);
            app.mark_dirty();
        }
        if ui.checkbox(&mut app.cfg.compact_messages, "Compact message layout").changed() {
            app.mark_dirty();
        }
        if ui.checkbox(&mut app.cfg.show_images, "Show image attachments and embeds").changed() {
            app.mark_dirty();
        }
    });
}

fn import_theme(dir: &std::path::Path, src: &str) -> Result<std::path::PathBuf, String> {
    if src.is_empty() {
        return Err("Enter a URL or file path".into());
    }
    if src.starts_with("http://") || src.starts_with("https://") {
        let name = src.rsplit('/').next().unwrap_or("theme").trim_end_matches(".css").trim_end_matches(".theme");
        let name: String = name.chars().filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_').take(40).collect();
        let path = dir.join(format!("{}.theme.css", if name.is_empty() { "imported" } else { &name }));
        let css = format!("/**\n * @name {name}\n * @description Imported from {src}\n */\n@import url(\"{src}\");\n");
        std::fs::write(&path, css).map_err(|e| e.to_string())?;
        return Ok(path);
    }
    let p = std::path::Path::new(src);
    let file = p.file_name().ok_or("Not a file")?;
    let dest = dir.join(file);
    std::fs::copy(p, &dest).map_err(|e| format!("Copy failed: {e}"))?;
    Ok(dest)
}

fn plugins(app: &mut RustcordApp, ui: &mut egui::Ui) {
    let k = app.colors;
    ui.heading("Plugins");
    card(app.colors, ui, |ui| {
        ui.label("Drop plugins into the plugins folder:");
        ui.label(RichText::new("• BetterDiscord  *.plugin.js   • Vencord-style ES modules  *.js   • Rustcord scripts using the Rustcord API").size(12.0).color(k.muted));
        ui.label(
            RichText::new("Plugins run sandboxed in QuickJS. Parts that rewrite Discord's web bundle (webpack patches, React UI, DOM) are skipped because a native client has no web bundle; commands, message hooks, events, toasts, storage, CSS and fetch work.")
                .size(11.5)
                .color(k.muted),
        );
        ui.horizontal(|ui| {
            if ui.button("📂 Open plugins folder").clicked() {
                open_path(&app.paths.plugins);
            }
            if ui.button("↻ Reload all").clicked() {
                app.plugins.send(HostCmd::Rescan);
            }
            ui.toggle_value(&mut app.settings.show_logs, "Console");
        });
    });
    ui.add_space(8.0);
    let list = app.plugins.plugins();
    if list.is_empty() {
        ui.label(RichText::new("No plugins found.").color(k.muted));
    }
    for p in list {
        card(app.colors, ui, |ui| {
            ui.horizontal(|ui| {
                let mut on = p.enabled;
                if ui.checkbox(&mut on, "").changed() {
                    app.plugins.send(HostCmd::SetEnabled { id: p.id.clone(), enabled: on });
                    app.mark_dirty();
                }
                ui.label(RichText::new(&p.name).strong().color(k.header));
                if !p.version.is_empty() {
                    ui.label(RichText::new(format!("v{}", p.version)).size(11.0).color(k.muted));
                }
                ui.label(RichText::new(p.kind.label()).size(10.0).strong().color(Color32::WHITE).background_color(k.brand.gamma_multiply(0.8)));
                if p.running {
                    ui.label(RichText::new("running").size(10.5).color(k.positive));
                }
            });
            if !p.author.is_empty() {
                ui.label(RichText::new(format!("by {}", p.author)).size(11.5).color(k.muted));
            }
            if !p.description.is_empty() {
                ui.label(&p.description);
            }
            if let Some(e) = &p.error {
                ui.colored_label(k.danger, e);
            }
        });
        ui.add_space(6.0);
    }
    if app.settings.show_logs {
        section(ui, "Plugin console");
        card(app.colors, ui, |ui| {
            if ui.small_button("Clear").clicked() {
                app.plugins.clear_logs();
            }
            egui::ScrollArea::vertical().max_height(220.0).stick_to_bottom(true).show(ui, |ui| {
                for l in app.plugins.logs() {
                    let c = match l.level.as_str() {
                        "error" => k.danger,
                        "warn" => k.warning,
                        _ => k.text,
                    };
                    ui.label(RichText::new(format!("[{}] {}", l.plugin, l.text)).monospace().size(11.5).color(c));
                }
            });
        });
    }
}

fn account(app: &mut RustcordApp, ui: &mut egui::Ui) {
    let k = app.colors;
    ui.heading("Account");
    let me = app.store.me.clone().unwrap_or_default();
    card(app.colors, ui, |ui| {
        ui.horizontal(|ui| {
            crate::ui::widgets::avatar(ui, &mut app.images, &app.backend, &me, 64.0, None);
            ui.vertical(|ui| {
                ui.label(RichText::new(me.display_name()).size(18.0).strong().color(k.header));
                ui.label(RichText::new(&me.username).color(k.muted));
            });
        });
        ui.add_space(8.0);
        if ui.checkbox(&mut app.cfg.remember_token, "Stay logged in on this computer").changed() {
            app.mark_dirty();
        }
        if ui.add(egui::Button::new(RichText::new("Log out").color(Color32::WHITE)).fill(k.danger)).clicked() {
            app.logout();
        }
    });
    section(ui, "Advanced");
    card(app.colors, ui, |ui| {
        ui.horizontal(|ui| {
            ui.label("Client build number");
            if ui.add(egui::DragValue::new(&mut app.cfg.client.client_build_number)).changed() {
                app.mark_dirty();
            }
        });
        ui.label(RichText::new("Applies on next login.").size(11.5).color(k.muted));
    });
}

fn about(app: &mut RustcordApp, ui: &mut egui::Ui) {
    let k = app.colors;
    ui.heading("About Rustcord");
    card(app.colors, ui, |ui| {
        ui.label(RichText::new(format!("Rustcord {}", env!("CARGO_PKG_VERSION"))).strong());
        ui.label(format!("Audio codec: {}", rustcord_voice::opus::version()));
        ui.label("Video codec: OpenH264 (Cisco), H.264 constrained baseline");
        ui.label("Voice transport: AES-256-GCM / XChaCha20-Poly1305 · end-to-end encryption: DAVE (MLS)");
        ui.label("Plugins: QuickJS · UI: egui");
        ui.add_space(8.0);
        ui.label(
            RichText::new(
                "Rustcord is an independent project and is not affiliated with or endorsed by Discord Inc. \
                 Using third-party clients and modified voice settings is against Discord's Terms of Service \
                 and can get an account actioned. You use it at your own risk.",
            )
            .size(11.5)
            .color(k.warning),
        );
    });
}
