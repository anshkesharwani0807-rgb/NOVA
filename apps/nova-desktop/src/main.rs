mod app;
mod panels;
mod system_tray;

use app::{AppTab, NovaDesktopApp};
use eframe::egui;

impl eframe::App for NovaDesktopApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if !self.is_initialized {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.heading("NOVA — Windows Desktop Shell");
                ui.label("Initializing kernel...");
                if ui.button("Start NOVA").clicked() {
                    self.initialize();
                }
                if !self.status_message.is_empty() {
                    ui.colored_label(egui::Color32::RED, &self.status_message);
                }
            });
            return;
        }

        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("NOVA");
                ui.separator();
                if ui
                    .selectable_label(self.active_tab == AppTab::Search, "Search")
                    .clicked()
                {
                    self.active_tab = AppTab::Search;
                }
                if ui
                    .selectable_label(self.active_tab == AppTab::Memory, "Memory")
                    .clicked()
                {
                    self.active_tab = AppTab::Memory;
                }
                if ui
                    .selectable_label(self.active_tab == AppTab::Voice, "Voice")
                    .clicked()
                {
                    self.active_tab = AppTab::Voice;
                }
                if ui
                    .selectable_label(self.active_tab == AppTab::Activity, "Activity")
                    .clicked()
                {
                    self.active_tab = AppTab::Activity;
                }
                if ui
                    .selectable_label(self.active_tab == AppTab::Health, "Health")
                    .clicked()
                {
                    self.active_tab = AppTab::Health;
                }
                if ui
                    .selectable_label(self.active_tab == AppTab::Settings, "Settings")
                    .clicked()
                {
                    self.active_tab = AppTab::Settings;
                }
            });
        });

        match self.active_tab {
            AppTab::Search => panels::search_panel::render(self, ctx),
            AppTab::Memory => panels::memory_panel::render(self, ctx),
            AppTab::Voice => panels::voice_panel::render(self, ctx),
            AppTab::Activity => panels::activity_panel::render(self, ctx),
            AppTab::Health => panels::health_panel::render(self, ctx),
            AppTab::Settings => panels::settings_panel::render(self, ctx),
        }
    }
}

fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1024.0, 768.0])
            .with_min_inner_size([640.0, 480.0])
            .with_title("NOVA — Personal AI Assistant"),
        ..Default::default()
    };

    eframe::run_native(
        "NOVA Desktop",
        options,
        Box::new(|_cc| Box::new(NovaDesktopApp::new())),
    )
}
