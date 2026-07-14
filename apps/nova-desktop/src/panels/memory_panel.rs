use crate::app::NovaDesktopApp;
use eframe::egui;

pub fn render(app: &mut NovaDesktopApp, ctx: &egui::Context) {
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.heading("Memory");

        ui.horizontal(|ui| {
            ui.label("Filter:");
            let changed = ui
                .add(
                    egui::TextEdit::singleline(&mut app.memory_filter)
                        .hint_text("Search memories...")
                        .desired_width(300.0),
                )
                .changed();
            if changed {
                app.refresh_memory_list();
            }
        });

        ui.horizontal(|ui| {
            ui.label("New memory:");
            ui.add(
                egui::TextEdit::singleline(&mut app.new_memory_text)
                    .hint_text("Enter text...")
                    .desired_width(400.0),
            );
            if ui.button("Add").clicked() {
                app.add_memory();
            }
        });

        ui.separator();

        let detail_id = {
            let mut detail_id: Option<String> = None;
            egui::ScrollArea::vertical().show(ui, |ui| {
                for rec in &app.memory_records {
                    ui.group(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(format!("[{:?}]", rec.category));
                            ui.label(&rec.content);
                            if ui.small_button("View").clicked() {
                                detail_id = Some(rec.id.clone());
                            }
                        });
                    });
                }
            });
            detail_id
        };

        if let Some(id) = detail_id {
            if let Some(m) = &app.memory {
                if let Ok(Some(rec)) = m.find_by_id(&id) {
                    app.memory_detail = Some(rec);
                }
            }
        }

        let detail = app.memory_detail.clone();
        if let Some(rec) = detail {
            egui::Window::new("Memory Detail").show(ctx, |ui| {
                ui.label(format!("ID: {}", rec.id));
                ui.label(format!("Category: {:?}", rec.category));
                ui.label(format!("Content: {}", rec.content));
                ui.label(format!("Created: {}", rec.created_at));
                ui.label(format!("Updated: {}", rec.updated_at));
                if ui.button("Close").clicked() {
                    app.memory_detail = None;
                }
            });
        }
    });
}
