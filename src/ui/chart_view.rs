use std::f32::consts::PI;
use eframe::egui::{
    self, Color32, Frame, Margin, Pos2, Rect, RichText, Rounding, Sense, Shape, Stroke, Ui, Vec2,
};
use crate::core::types::{FolderTreeNode, SizeUnit};
use crate::ui::theme::ThemeColors;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChartMode {
    Pie,
    Bar,
}

pub struct ChartItem {
    pub name: String,
    pub bytes: u64,
    pub percentage: f32,
    pub color: Color32,
}

/// Palette of clean, distinct colors for pie chart slices (matching professional tool aesthetics)
const PALETTE: [Color32; 10] = [
    Color32::from_rgb(59, 130, 246),  // Azure Blue
    Color32::from_rgb(16, 185, 129),  // Emerald Green
    Color32::from_rgb(245, 158, 11),  // Amber
    Color32::from_rgb(239, 68, 68),   // Crimson / Red
    Color32::from_rgb(168, 85, 247),  // Purple
    Color32::from_rgb(6, 182, 212),   // Cyan
    Color32::from_rgb(249, 115, 22),  // Orange
    Color32::from_rgb(236, 72, 153),  // Pink
    Color32::from_rgb(132, 204, 22),  // Lime
    Color32::from_rgb(148, 163, 184), // Slate (Others)
];

pub fn render_pro_chart(
    ui: &mut Ui,
    node: &FolderTreeNode,
    unit: SizeUnit,
    chart_mode: &mut ChartMode,
) {
    ui.vertical(|ui| {
        // Toolbar for chart options
        ui.horizontal(|ui| {
            ui.label(RichText::new("Visualization:").color(ThemeColors::TEXT_MUTED).strong());

            if ui.selectable_label(*chart_mode == ChartMode::Pie, "Pie Chart").clicked() {
                *chart_mode = ChartMode::Pie;
            }
            if ui.selectable_label(*chart_mode == ChartMode::Bar, "Bar Breakdown").clicked() {
                *chart_mode = ChartMode::Bar;
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("Total: {}", unit.format(node.total_size)))
                        .color(ThemeColors::ACCENT)
                        .strong(),
                );
            });
        });

        ui.separator();

        let items = build_chart_items(node);
        if items.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(30.0);
                ui.label(RichText::new("Selected folder is empty.").color(ThemeColors::TEXT_MUTED));
            });
            return;
        }

        match chart_mode {
            ChartMode::Pie => render_pie_chart_canvas(ui, &items, unit),
            ChartMode::Bar => render_bar_breakdown(ui, &items, unit),
        }
    });
}

fn build_chart_items(node: &FolderTreeNode) -> Vec<ChartItem> {
    let mut entries: Vec<(String, u64)> = Vec::new();

    for child in &node.children {
        entries.push((child.name.clone(), child.total_size));
    }
    for file in &node.direct_files {
        entries.push((file.name.clone(), file.size));
    }

    entries.sort_by(|a, b| b.1.cmp(&a.1));

    let total: u64 = entries.iter().map(|e| e.1).sum();
    if total == 0 {
        return Vec::new();
    }

    let mut chart_items = Vec::new();
    let mut other_bytes = 0u64;

    for (idx, (name, bytes)) in entries.into_iter().enumerate() {
        let pct = (bytes as f32 / total as f32) * 100.0;
        if idx < 8 && pct >= 1.0 {
            let color = PALETTE[idx % (PALETTE.len() - 1)];
            chart_items.push(ChartItem {
                name,
                bytes,
                percentage: pct,
                color,
            });
        } else {
            other_bytes += bytes;
        }
    }

    if other_bytes > 0 {
        let pct = (other_bytes as f32 / total as f32) * 100.0;
        chart_items.push(ChartItem {
            name: "Others".to_string(),
            bytes: other_bytes,
            percentage: pct,
            color: PALETTE[PALETTE.len() - 1],
        });
    }

    chart_items
}

fn render_pie_chart_canvas(ui: &mut Ui, items: &[ChartItem], unit: SizeUnit) {
    let desired_size = Vec2::new(ui.available_width(), (ui.available_height() - 20.0).max(180.0));
    let (rect, response) = ui.allocate_exact_size(desired_size, Sense::hover());
    let painter = ui.painter_at(rect);

    let center = Pos2::new(rect.min.x + rect.width() * 0.35, rect.min.y + rect.height() * 0.5);
    let radius = (rect.height() * 0.42).min(rect.width() * 0.28).max(45.0);

    let mouse_pos = response.hover_pos();
    let mut hovered_index: Option<usize> = None;

    if let Some(pos) = mouse_pos {
        let delta = pos - center;
        let dist = delta.length();
        if dist <= radius {
            let mut angle = delta.y.atan2(delta.x);
            if angle < 0.0 {
                angle += 2.0 * PI;
            }

            let mut cur_angle = 0.0;
            for (i, item) in items.iter().enumerate() {
                let sweep = (item.percentage / 100.0) * 2.0 * PI;
                if angle >= cur_angle && angle <= cur_angle + sweep {
                    hovered_index = Some(i);
                    break;
                }
                cur_angle += sweep;
            }
        }
    }

    // Draw pie slices
    let mut cur_angle: f32 = 0.0;
    for (i, item) in items.iter().enumerate() {
        let sweep = (item.percentage / 100.0) * 2.0 * PI;
        let is_hovered = hovered_index == Some(i);

        let slice_radius = if is_hovered { radius + 4.0 } else { radius };
        let slice_color = if is_hovered {
            lighten_color(item.color)
        } else {
            item.color
        };

        // Explode hovered slice outwards along its bisecting angle
        let mid_angle = cur_angle + sweep * 0.5;
        let slice_center = if is_hovered {
            Pos2::new(center.x + mid_angle.cos() * 8.0, center.y + mid_angle.sin() * 8.0)
        } else {
            center
        };

        // Tessellate arc
        let steps = ((sweep / (PI / 16.0)).ceil() as usize).max(4);
        let mut points = vec![slice_center];
        for s in 0..=steps {
            let a = cur_angle + sweep * (s as f32 / steps as f32);
            points.push(Pos2::new(slice_center.x + a.cos() * slice_radius, slice_center.y + a.sin() * slice_radius));
        }

        painter.add(Shape::convex_polygon(
            points,
            slice_color,
            Stroke::new(1.0, Color32::from_black_alpha(80)),
        ));

        cur_angle += sweep;
    }

    // Draw Legend on the right side
    let legend_x = rect.min.x + rect.width() * 0.65;
    let mut legend_y = rect.min.y + 12.0;

    for (i, item) in items.iter().enumerate() {
        let is_hovered = hovered_index == Some(i);
        let text_color = if is_hovered { ThemeColors::TEXT_PRIMARY } else { ThemeColors::TEXT_SECONDARY };

        // Color badge
        let badge_rect = Rect::from_min_size(Pos2::new(legend_x, legend_y + 2.0), Vec2::new(10.0, 10.0));
        painter.rect_filled(badge_rect, Rounding::same(2.0), item.color);

        // Label
        let label_text = format!("{}: {:.1}% ({})", truncate_name(&item.name, 18), item.percentage, unit.format(item.bytes));
        painter.text(
            Pos2::new(legend_x + 16.0, legend_y),
            egui::Align2::LEFT_TOP,
            label_text,
            egui::FontId::proportional(11.0),
            text_color,
        );

        legend_y += 18.0;
        if legend_y > rect.max.y - 20.0 {
            break;
        }
    }

    // Hover Callout Tooltip matching the screenshot
    if let Some(idx) = hovered_index {
        let item = &items[idx];
        egui::show_tooltip_at_pointer(
            ui.ctx(),
            ui.layer_id(),
            egui::Id::new("pie_chart_tooltip"),
            |ui| {
                Frame::none()
                    .fill(ThemeColors::BG_CARD)
                    .stroke(Stroke::new(1.0, ThemeColors::ACCENT))
                    .rounding(Rounding::same(4.0))
                    .inner_margin(Margin::same(8.0))
                    .show(ui, |ui| {
                        ui.label(RichText::new(&item.name).strong().color(ThemeColors::TEXT_PRIMARY));
                        ui.label(
                            RichText::new(format!("{:.2}% ({})", item.percentage, unit.format(item.bytes)))
                                .color(ThemeColors::STATUS_INFO)
                                .strong(),
                        );
                    });
            },
        );
    }
}

fn render_bar_breakdown(ui: &mut Ui, items: &[ChartItem], unit: SizeUnit) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        egui::Grid::new("chart_bar_breakdown_grid")
            .striped(true)
            .spacing([12.0, 8.0])
            .min_col_width(100.0)
            .show(ui, |ui| {
                ui.label(RichText::new("Item").strong().color(ThemeColors::TEXT_MUTED));
                ui.label(RichText::new("Size").strong().color(ThemeColors::TEXT_MUTED));
                ui.label(RichText::new("Percentage").strong().color(ThemeColors::TEXT_MUTED));
                ui.label(RichText::new("Visual Share").strong().color(ThemeColors::TEXT_MUTED));
                ui.end_row();

                for item in items {
                    ui.horizontal(|ui| {
                        let (rect, _) = ui.allocate_exact_size(Vec2::new(10.0, 10.0), Sense::hover());
                        ui.painter().rect_filled(rect, Rounding::same(2.0), item.color);
                        ui.label(RichText::new(&item.name).color(ThemeColors::TEXT_PRIMARY));
                    });

                    ui.label(RichText::new(unit.format(item.bytes)).color(ThemeColors::ACCENT).strong());
                    ui.label(RichText::new(format!("{:.2} %", item.percentage)).color(ThemeColors::TEXT_SECONDARY));

                    let fraction = (item.percentage / 100.0).clamp(0.0, 1.0);
                    ui.add(egui::ProgressBar::new(fraction).desired_width(160.0));
                    ui.end_row();
                }
            });
    });
}

fn lighten_color(c: Color32) -> Color32 {
    Color32::from_rgb(
        c.r().saturating_add(30),
        c.g().saturating_add(30),
        c.b().saturating_add(30),
    )
}

fn truncate_name(name: &str, max_len: usize) -> String {
    if name.chars().count() > max_len {
        format!("{}...", name.chars().take(max_len).collect::<String>())
    } else {
        name.to_string()
    }
}
