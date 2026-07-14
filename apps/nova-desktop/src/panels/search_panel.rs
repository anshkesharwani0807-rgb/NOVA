use crate::app::{NovaDesktopApp, SearchMode};
use eframe::egui;

pub fn render(app: &mut NovaDesktopApp, ctx: &egui::Context) {
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.heading("Search");

        ui.horizontal(|ui| {
            ui.label("Mode:");
            if ui
                .selectable_label(app.search_mode == SearchMode::Text, "Text")
                .clicked()
            {
                app.search_mode = SearchMode::Text;
            }
            if ui
                .selectable_label(app.search_mode == SearchMode::NaturalLanguage, "NL")
                .clicked()
            {
                app.search_mode = SearchMode::NaturalLanguage;
            }
        });

        ui.horizontal(|ui| {
            let resp = ui.add(
                egui::TextEdit::singleline(&mut app.search_query)
                    .hint_text("Type to search...")
                    .desired_width(f32::INFINITY),
            );
            if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                app.do_search();
            }
            if ui.button("Search").clicked() {
                app.do_search();
            }
        });

        ui.separator();

        if app.search_results.is_empty() {
            ui.label("No results. Try a search above.");
        } else {
            ui.label(format!("{} result(s)", app.search_results.len()));
            egui::ScrollArea::vertical().show(ui, |ui| {
                for result in &app.search_results {
                    ui.group(|ui| {
                        ui.label(format!("Score: {:.2}", result.score));
                        ui.label(&result.document.content);
                    });
                }
            });
        }
    });
}
