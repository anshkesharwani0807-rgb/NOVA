use crate::app::NovaDesktopApp;
use eframe::egui;

pub fn render(app: &mut NovaDesktopApp, ctx: &egui::Context) {
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.heading("Activity Trail & Egress Log");

        ui.horizontal(|ui| {
            if ui.button("Refresh").clicked() {
                app.refresh_activity_trail();
                app.refresh_egress_log();
            }
        });

        ui.separator();
        ui.label("Activity Trail:");
        egui::ScrollArea::vertical()
            .max_height(200.0)
            .show(ui, |ui| {
                for entry in &app.activity_trail {
                    ui.label(entry);
                }
                if app.activity_trail.is_empty() {
                    ui.label("(no entries)");
                }
            });

        ui.separator();
        ui.label("Egress Log:");
        egui::ScrollArea::vertical()
            .max_height(200.0)
            .show(ui, |ui| {
                for entry in &app.egress_log {
                    ui.label(entry);
                }
                if app.egress_log.is_empty() {
                    ui.label("(no entries)");
                }
            });
    });
}
