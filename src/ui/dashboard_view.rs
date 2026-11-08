use eframe::egui::{self, Color32, Frame, Margin, RichText, Rounding, Stroke, Ui};
use crate::core::security::relaunch_as_admin;
use crate::core::types::format_bytes;
use crate::ui::theme::ThemeColors;

pub struct DashboardMetrics {
    pub total_scanned_bytes: u64,
    pub total_files: usize,
    pub total_dirs: usize,
    pub total_inaccessible: usize,
    pub total_locked: usize,
    pub is_admin: bool,
}

pub fn render_dashboard(
    ui: &mut Ui,
    metrics: &DashboardMetrics,
    on_navigate: &mut impl FnMut(usize),
    on_quick_scan_system_temp: &mut impl FnMut(),
) {
    ui.spacing_mut().item_spacing = egui::vec2(12.0, 16.0);

    // Header & Privilege Banner
    render_header_card(ui, metrics.is_admin);

    // Metrics Overview Grid
    render_metrics_grid(ui, metrics);

    // Quick Actions & System Info
    render_quick_actions(ui, on_navigate, on_quick_scan_system_temp);
}

fn render_header_card(ui: &mut Ui, is_admin: bool) {
    Frame::none()
        .fill(ThemeColors::BG_CARD)
        .rounding(Rounding::same(8.0))
        .stroke(Stroke::new(1.0, ThemeColors::BORDER_QUIET))
        .inner_margin(Margin::same(16.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.heading(
                        RichText::new("Storage diagnostics and cleaner")
                            .color(ThemeColors::TEXT_PRIMARY),
                    );
                    ui.label(
                        RichText::new("Filesystem analysis, process lock inspection, and administrative cleanup")
                            .color(ThemeColors::TEXT_SECONDARY),
                    );
                });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if is_admin {
                        ui.label(
                            RichText::new("Administrator")
                                .color(ThemeColors::STATUS_SUCCESS),
                        );
                    } else {
                        ui.horizontal(|ui| {
                            if ui
                                .button(
                                    RichText::new("Relaunch as administrator")
                                        .color(ThemeColors::TEXT_PRIMARY),
                                )
                                .clicked()
                            {
                                let _ = relaunch_as_admin();
                            }
                            ui.label(
                                RichText::new("Standard user")
                                    .color(ThemeColors::TEXT_MUTED),
                            );
                        });
                    }
                });
            });
        });
}

fn render_metrics_grid(ui: &mut Ui, metrics: &DashboardMetrics) {
    egui::Grid::new("dashboard_stats_grid")
        .num_columns(4)
        .spacing([12.0, 12.0])
        .min_col_width(180.0)
        .show(ui, |ui| {
            render_stat_card(
                ui,
                "Total scanned size",
                &format_bytes(metrics.total_scanned_bytes),
                "Across all analyzed locations",
                ThemeColors::ACCENT,
            );

            render_stat_card(
                ui,
                "Total files",
                &format!("{}", metrics.total_files),
                &format!("In {} directories", metrics.total_dirs),
                ThemeColors::TEXT_PRIMARY,
            );

            render_stat_card(
                ui,
                "Inaccessible or skipped",
                &format!("{}", metrics.total_inaccessible),
                "Access denied or restricted",
                if metrics.total_inaccessible > 0 {
                    ThemeColors::STATUS_WARNING
                } else {
                    ThemeColors::STATUS_SUCCESS
                },
            );

            render_stat_card(
                ui,
                "Locked by processes",
                &format!("{}", metrics.total_locked),
                "Identified via Restart Manager",
                if metrics.total_locked > 0 {
                    ThemeColors::STATUS_ERROR
                } else {
                    ThemeColors::STATUS_SUCCESS
                },
            );

            ui.end_row();
        });
}

fn render_stat_card(ui: &mut Ui, title: &str, value: &str, subtitle: &str, val_color: Color32) {
    Frame::none()
        .fill(ThemeColors::BG_CARD)
        .rounding(Rounding::same(6.0))
        .stroke(Stroke::new(1.0, ThemeColors::BORDER_QUIET))
        .inner_margin(Margin::same(14.0))
        .show(ui, |ui| {
            ui.set_width(200.0);
            ui.vertical(|ui| {
                ui.label(RichText::new(title).color(ThemeColors::TEXT_MUTED));
                ui.add_space(4.0);
                ui.label(
                    RichText::new(value)
                        .color(val_color)
                        .size(20.0),
                );
                ui.add_space(4.0);
                ui.label(RichText::new(subtitle).color(ThemeColors::TEXT_SECONDARY).size(11.0));
            });
        });
}

fn render_quick_actions(
    ui: &mut Ui,
    on_navigate: &mut impl FnMut(usize),
    on_quick_scan_system_temp: &mut impl FnMut(),
) {
    ui.add_space(4.0);
    ui.label(RichText::new("Quick actions").color(ThemeColors::TEXT_PRIMARY));

    ui.horizontal(|ui| {
        if ui
            .button("Open folder analyzer")
            .clicked()
        {
            on_navigate(1);
        }

        if ui
            .button("Open temp cleaner")
            .clicked()
        {
            on_navigate(2);
        }

        if ui
            .button("Scan system temp")
            .clicked()
        {
            on_quick_scan_system_temp();
            on_navigate(2);
        }

        if ui
            .button("View operation log")
            .clicked()
        {
            on_navigate(3);
        }
    });
}
