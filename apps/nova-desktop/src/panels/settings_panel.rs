use crate::app::NovaDesktopApp;
use eframe::egui;

pub fn render(app: &mut NovaDesktopApp, ctx: &egui::Context) {
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.heading("Settings");

        ui.label("Current config (read-only):");
        egui::ScrollArea::vertical()
            .max_height(200.0)
            .show(ui, |ui| {
                ui.monospace(&app.config_json);
            });

        ui.separator();
        ui.label("Edit config (JSON):");
        egui::ScrollArea::vertical()
            .max_height(300.0)
            .show(ui, |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut app.config_edit)
                        .code_editor()
                        .desired_rows(20),
                );
            });

        if ui.button("Save Config").clicked() {
            app.save_config();
        }

        if !app.status_message.is_empty() {
            ui.label(&app.status_message);
        }
    });
}
