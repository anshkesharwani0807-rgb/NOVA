use crate::app::NovaDesktopApp;
use eframe::egui;

pub fn render(app: &mut NovaDesktopApp, ctx: &egui::Context) {
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.heading("System Health");

        if ui.button("Refresh Health").clicked() {
            app.refresh_health();
        }

        ui.separator();
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.monospace(&app.health_report);
        });

        ui.separator();
        ui.label(format!("Status: {}", app.status_message));
    });
}
