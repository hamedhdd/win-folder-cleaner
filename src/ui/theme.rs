use eframe::egui::{
    self, Color32, Margin, Rounding, Stroke, Vec2, Visuals,
};

/// Color palette adhering to the Unslop and Impeccable "Operate" design standards.
/// Quiet dark graphite neutral ramp with purposeful cobalt/azure accent and semantic status indicators.
pub struct ThemeColors;

impl ThemeColors {
    pub const BG_APP: Color32 = Color32::from_rgb(16, 18, 22);
    pub const BG_PANEL: Color32 = Color32::from_rgb(22, 25, 31);
    pub const BG_CARD: Color32 = Color32::from_rgb(28, 32, 40);
    pub const BG_CARD_HOVER: Color32 = Color32::from_rgb(34, 39, 49);
    pub const BG_INPUT: Color32 = Color32::from_rgb(20, 23, 29);

    pub const BORDER_QUIET: Color32 = Color32::from_rgb(44, 50, 62);
    pub const BORDER_FOCUS: Color32 = Color32::from_rgb(59, 130, 246);

    pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(240, 243, 246);
    pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(154, 163, 178);
    pub const TEXT_MUTED: Color32 = Color32::from_rgb(100, 110, 126);

    pub const ACCENT: Color32 = Color32::from_rgb(59, 130, 246);       // Azure / Cobalt
    pub const ACCENT_HOVER: Color32 = Color32::from_rgb(96, 165, 250);
    pub const STATUS_SUCCESS: Color32 = Color32::from_rgb(34, 197, 94);  // Green
    pub const STATUS_WARNING: Color32 = Color32::from_rgb(245, 158, 11); // Amber
    pub const STATUS_ERROR: Color32 = Color32::from_rgb(239, 68, 68);    // Rose / Red
    pub const STATUS_INFO: Color32 = Color32::from_rgb(14, 165, 233);    // Cyan
}

/// Applies deliberate, clean styling to the egui Context.
pub fn apply_clean_theme(ctx: &egui::Context) {
    let mut visuals = Visuals::dark();

    visuals.dark_mode = true;
    visuals.override_text_color = Some(ThemeColors::TEXT_PRIMARY);

    visuals.panel_fill = ThemeColors::BG_PANEL;
    visuals.window_fill = ThemeColors::BG_CARD;
    visuals.extreme_bg_color = ThemeColors::BG_INPUT;
    visuals.faint_bg_color = ThemeColors::BG_APP;

    // Window & Card Rounding
    visuals.window_rounding = Rounding::same(8.0);
    visuals.menu_rounding = Rounding::same(6.0);
    visuals.window_stroke = Stroke::new(1.0, ThemeColors::BORDER_QUIET);

    // Widget states
    visuals.widgets.noninteractive.bg_fill = ThemeColors::BG_CARD;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, ThemeColors::BORDER_QUIET);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, ThemeColors::TEXT_SECONDARY);
    visuals.widgets.noninteractive.rounding = Rounding::same(6.0);

    visuals.widgets.inactive.bg_fill = ThemeColors::BG_CARD;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, ThemeColors::BORDER_QUIET);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, ThemeColors::TEXT_PRIMARY);
    visuals.widgets.inactive.rounding = Rounding::same(6.0);

    visuals.widgets.hovered.bg_fill = ThemeColors::BG_CARD_HOVER;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, ThemeColors::ACCENT);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, ThemeColors::TEXT_PRIMARY);
    visuals.widgets.hovered.rounding = Rounding::same(6.0);

    visuals.widgets.active.bg_fill = ThemeColors::ACCENT;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, ThemeColors::ACCENT_HOVER);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    visuals.widgets.active.rounding = Rounding::same(6.0);

    visuals.widgets.open.bg_fill = ThemeColors::BG_CARD_HOVER;
    visuals.widgets.open.bg_stroke = Stroke::new(1.0, ThemeColors::BORDER_FOCUS);

    ctx.set_visuals(visuals);

    // Spacing configuration on 4px/8px rhythm
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = Vec2::new(8.0, 8.0);
    style.spacing.button_padding = Vec2::new(12.0, 6.0);
    style.spacing.window_margin = Margin::same(12.0);
    style.spacing.indent = 16.0;

    ctx.set_style(style);
}
