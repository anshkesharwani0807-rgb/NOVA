use crate::app::NovaDesktopApp;
use eframe::egui;

pub fn render(app: &mut NovaDesktopApp, ctx: &egui::Context) {
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.heading("Voice System");

        ui.horizontal(|ui| {
            ui.label("Status:");
            ui.colored_label(
                if app.voice_status == "Listening" {
                    egui::Color32::GREEN
                } else {
                    egui::Color32::GRAY
                },
                &app.voice_status,
            );
        });

        ui.horizontal(|ui| {
            ui.label("Wake Word:");
            ui.add(egui::TextEdit::singleline(&mut app.wake_word).desired_width(150.0));
        });

        ui.separator();
        ui.label("Voice commands appear here when the pipeline is active.");
        ui.label("Currently in demo/simulation mode — connect a microphone for live use.");
    });
}
