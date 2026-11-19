use egui::Ui;

use crate::theme::Theme;
use crate::view_model::{ViewId, ViewModel};

/// The persistent left rail. Selecting a section only changes `vm.active`;
/// every view's state remains in the view model.
pub fn ui(ui: &mut Ui, vm: &mut ViewModel, theme: &Theme) {
    ui.set_width(theme.rail_width);
    ui.vertical(|ui| {
        ui.add_space(theme.space_md);
        ui.heading("Prompt Studio");
        ui.add_space(theme.space_sm);
        ui.label(egui::RichText::new("native · port in progress").color(theme.faint));
        ui.add_space(theme.space_md);
        ui.separator();

        for view in ViewId::ALL {
            let selected = vm.active == view;
            let button = egui::Button::selectable(selected, view.label());
            if ui.add(button).clicked() {
                vm.navigate(view);
            }
        }

        ui.add_space(theme.space_md);
        ui.separator();
        ui.label(egui::RichText::new("Storage: local").color(theme.muted));
        if ui.button("About").clicked() {
            vm.modal_open = true;
        }
    });
}
