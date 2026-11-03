//! First screen: token login.

use crate::app::{LoginState, RustcordApp};
use eframe::egui::{self, RichText};

pub fn show(app: &mut RustcordApp, ui: &mut egui::Ui) {
    let k = app.colors;
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(crate::ui::theme::over(k.bg_tertiary, egui::Color32::BLACK)))
        .show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space((ui.available_height() * 0.18).max(20.0));
                egui::Frame::new()
                    .fill(crate::ui::theme::over(k.bg_primary, egui::Color32::BLACK))
                    .corner_radius(egui::CornerRadius::same(14))
                    .inner_margin(egui::Margin::same(28))
                    .show(ui, |ui| {
                        ui.set_width(440.0);
                        ui.label(RichText::new("Rustcord").size(30.0).strong().color(k.brand));
                        ui.label(RichText::new("A fast native client for Discord").color(k.muted));
                        ui.add_space(18.0);

                        match app.login.clone() {
                            LoginState::Checking => {
                                ui.horizontal(|ui| {
                                    ui.spinner();
                                    ui.label("Signing in…");
                                });
                                return;
                            }
                            LoginState::Error(e) => {
                                ui.colored_label(k.danger, e);
                                if let Some(saved) = app.cfg.token.clone() {
                                    if ui.button("Retry with saved token").clicked() {
                                        app.login_with(saved);
                                        return;
                                    }
                                }
                                ui.add_space(8.0);
                            }
                            _ => {}
                        }

                        ui.label(RichText::new("ACCOUNT TOKEN").size(11.0).strong().color(k.header_secondary));
                        let resp = ui.add(
                            egui::TextEdit::singleline(&mut app.token_input)
                                .password(true)
                                .hint_text("Paste your token")
                                .desired_width(f32::INFINITY),
                        );
                        ui.checkbox(&mut app.cfg.remember_token, "Remember me on this computer");
                        ui.add_space(8.0);
                        let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        let go = ui
                            .add_sized([ui.available_width(), 36.0], egui::Button::new(RichText::new("Log in").strong()).fill(k.brand))
                            .clicked();
                        if (go || enter) && !app.token_input.trim().is_empty() {
                            let t = std::mem::take(&mut app.token_input);
                            app.login_with(t);
                        }
                        ui.add_space(12.0);
                        ui.label(
                            RichText::new(
                                "Your token is only sent to discord.com and stored locally (owner-only file permissions) \
                                 when \"Remember me\" is ticked. Third-party clients are against Discord's Terms of \
                                 Service; use at your own risk.",
                            )
                            .size(11.0)
                            .color(k.muted),
                        );
                    });
            });
        });
}
