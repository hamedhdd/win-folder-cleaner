use std::path::PathBuf;
use eframe::egui::{
    self, Color32, ComboBox, Frame, Margin, RichText, Rounding, ScrollArea, Stroke, Ui,
};

use crate::core::types::{AgeFilter, TempLocation};
use crate::ui::theme::ThemeColors;

pub struct CleanerViewState {
    pub locations: Vec<TempLocation>,
    pub selected_age_filter: AgeFilter,
    pub is_operating: bool,
    pub current_operation: String,
    pub operation_progress_item: String,
}

impl Default for CleanerViewState {
    fn default() -> Self {
        Self {
            locations: Vec::new(),
            selected_age_filter: AgeFilter::All,
            is_operating: false,
            current_operation: String::new(),
            operation_progress_item: String::new(),
        }
    }
}

pub fn render_cleaner_view(
    ui: &mut Ui,
    state: &mut CleanerViewState,
    on_scan_locations: &mut impl FnMut(Vec<PathBuf>),
    on_take_ownership_selected: &mut impl FnMut(Vec<PathBuf>),
    on_fix_permissions_selected: &mut impl FnMut(Vec<PathBuf>),
    on_clean_selected: &mut impl FnMut(Vec<PathBuf>, AgeFilter, bool), // (paths, age_filter, is_dry_run)
    on_add_custom_directory: &mut impl FnMut(PathBuf),
) {
    ui.spacing_mut().item_spacing = egui::vec2(8.0, 10.0);

    // Controls & Filter Bar
    render_control_toolbar(
        ui,
        state,
        on_scan_locations,
        on_take_ownership_selected,
        on_fix_permissions_selected,
        on_clean_selected,
        on_add_custom_directory,
    );

    // Active Operation Banner
    if state.is_operating {
        render_operation_banner(ui, state);
    }

    // Temp Locations Table
    render_locations_table(ui, state);
}

fn render_control_toolbar(
    ui: &mut Ui,
    state: &mut CleanerViewState,
    on_scan_locations: &mut impl FnMut(Vec<PathBuf>),
    on_take_ownership_selected: &mut impl FnMut(Vec<PathBuf>),
    on_fix_permissions_selected: &mut impl FnMut(Vec<PathBuf>),
    on_clean_selected: &mut impl FnMut(Vec<PathBuf>, AgeFilter, bool),
    on_add_custom_directory: &mut impl FnMut(PathBuf),
) {
    Frame::none()
        .fill(ThemeColors::BG_CARD)
        .rounding(Rounding::same(6.0))
        .stroke(Stroke::new(1.0, ThemeColors::BORDER_QUIET))
        .inner_margin(Margin::same(12.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Select All").clicked() {
                    for loc in &mut state.locations {
                        if loc.exists {
                            loc.selected = true;
                        }
                    }
                }

                if ui.button("Deselect All").clicked() {
                    for loc in &mut state.locations {
                        loc.selected = false;
                    }
                }

                ui.add_space(8.0);
                ui.label(RichText::new("Age Filter:").color(ThemeColors::TEXT_MUTED));

                ComboBox::from_id_salt("cleaner_age_filter_combo")
                    .selected_text(state.selected_age_filter.as_str())
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut state.selected_age_filter, AgeFilter::All, AgeFilter::All.as_str());
                        ui.selectable_value(&mut state.selected_age_filter, AgeFilter::OlderThan7Days, AgeFilter::OlderThan7Days.as_str());
                        ui.selectable_value(&mut state.selected_age_filter, AgeFilter::OlderThan30Days, AgeFilter::OlderThan30Days.as_str());
                        ui.selectable_value(&mut state.selected_age_filter, AgeFilter::OlderThan60Days, AgeFilter::OlderThan60Days.as_str());
                        ui.selectable_value(&mut state.selected_age_filter, AgeFilter::OlderThan90Days, AgeFilter::OlderThan90Days.as_str());
                    });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let selected_paths: Vec<PathBuf> = state
                        .locations
                        .iter()
                        .filter(|l| l.selected && l.exists)
                        .map(|l| l.path.clone())
                        .collect();

                    let has_selection = !selected_paths.is_empty();

                    if ui
                        .add_enabled(
                            has_selection && !state.is_operating,
                            egui::Button::new(
                                RichText::new("Clean Eligible Contents")
                                    .color(Color32::WHITE)
                                    .strong(),
                            ),
                        )
                        .clicked()
                    {
                        on_clean_selected(selected_paths.clone(), state.selected_age_filter, false);
                    }

                    if ui
                        .add_enabled(
                            has_selection && !state.is_operating,
                            egui::Button::new(
                                RichText::new("Dry Run Simulation")
                                    .color(ThemeColors::STATUS_INFO)
                                    .strong(),
                            ),
                        )
                        .clicked()
                    {
                        on_clean_selected(selected_paths.clone(), state.selected_age_filter, true);
                    }

                    if ui
                        .add_enabled(
                            has_selection && !state.is_operating,
                            egui::Button::new("Fix Permissions"),
                        )
                        .clicked()
                    {
                        on_fix_permissions_selected(selected_paths.clone());
                    }

                    if ui
                        .add_enabled(
                            has_selection && !state.is_operating,
                            egui::Button::new("Take Ownership"),
                        )
                        .clicked()
                    {
                        on_take_ownership_selected(selected_paths.clone());
                    }

                    if ui
                        .add_enabled(
                            has_selection && !state.is_operating,
                            egui::Button::new("Scan Selected"),
                        )
                        .clicked()
                    {
                        on_scan_locations(selected_paths);
                    }

                    if ui.button("+ Add Folder...").clicked() {
                        if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                            on_add_custom_directory(folder);
                        }
                    }
                });
            });
        });
}

fn render_operation_banner(ui: &mut Ui, state: &CleanerViewState) {
    Frame::none()
        .fill(ThemeColors::BG_CARD)
        .rounding(Rounding::same(6.0))
        .stroke(Stroke::new(1.0, ThemeColors::ACCENT))
        .inner_margin(Margin::same(12.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(&state.current_operation).color(ThemeColors::ACCENT).strong());
                ui.label(
                    RichText::new(&state.operation_progress_item)
                        .color(ThemeColors::TEXT_SECONDARY)
                        .monospace(),
                );
            });
        });
}

fn render_locations_table(ui: &mut Ui, state: &mut CleanerViewState) {
    ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        egui::Grid::new("temp_locations_grid")
            .striped(true)
            .spacing([12.0, 8.0])
            .min_col_width(80.0)
            .show(ui, |ui| {
                ui.label(RichText::new("Select").strong().color(ThemeColors::TEXT_MUTED));
                ui.label(RichText::new("Name").strong().color(ThemeColors::TEXT_MUTED));
                ui.label(RichText::new("Total Size").strong().color(ThemeColors::TEXT_MUTED));
                ui.label(RichText::new("Files").strong().color(ThemeColors::TEXT_MUTED));
                ui.label(RichText::new("Inaccessible").strong().color(ThemeColors::TEXT_MUTED));
                ui.label(RichText::new("Status").strong().color(ThemeColors::TEXT_MUTED));
                ui.label(RichText::new("Filesystem Path").strong().color(ThemeColors::TEXT_MUTED));
                ui.end_row();

                for loc in &mut state.locations {
                    ui.checkbox(&mut loc.selected, "");

                    let name_color = if loc.is_system {
                        ThemeColors::ACCENT
                    } else {
                        ThemeColors::TEXT_PRIMARY
                    };
                    ui.label(RichText::new(&loc.name).color(name_color).strong());

                    if let Some(summary) = &loc.summary {
                        ui.label(RichText::new(summary.formatted_total_size()).color(ThemeColors::STATUS_INFO).strong());
                        ui.label(RichText::new(format!("{}", summary.total_files)).color(ThemeColors::TEXT_SECONDARY));

                        let inacc_color = if summary.inaccessible_files > 0 {
                            ThemeColors::STATUS_WARNING
                        } else {
                            ThemeColors::TEXT_MUTED
                        };
                        ui.label(RichText::new(format!("{}", summary.inaccessible_files)).color(inacc_color));
                    } else if loc.exists {
                        ui.label(RichText::new("Unscanned").color(ThemeColors::TEXT_MUTED));
                        ui.label(RichText::new("-").color(ThemeColors::TEXT_MUTED));
                        ui.label(RichText::new("-").color(ThemeColors::TEXT_MUTED));
                    } else {
                        ui.label(RichText::new("Not Present").color(ThemeColors::TEXT_MUTED));
                        ui.label(RichText::new("-").color(ThemeColors::TEXT_MUTED));
                        ui.label(RichText::new("-").color(ThemeColors::TEXT_MUTED));
                    }

                    if loc.exists {
                        ui.label(RichText::new("Available").color(ThemeColors::STATUS_SUCCESS));
                    } else {
                        ui.label(RichText::new("Directory Not Found").color(ThemeColors::TEXT_MUTED));
                    }

                    ui.label(RichText::new(loc.path.to_string_lossy()).color(ThemeColors::TEXT_MUTED).size(11.0));
                    ui.end_row();
                }
            });
    });
}
