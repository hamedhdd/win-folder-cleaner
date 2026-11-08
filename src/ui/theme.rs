use eframe::egui::{
    self, Color32, Margin, Rounding, Stroke, Vec2, Visuals,
};

/// Color palette adhering to the Unslop and Impeccable "Operate" design standards.
/// Quiet dark graphite neutral ramp with purposeful cobalt/azure accent and semantic status indicators.
pub struct ThemeColors;

impl ThemeColors {
    // 1-Ramp Graphite Neutrals
    pub const BG_APP: Color32 = Color32::from_rgb(15, 17, 20);
    pub const BG_PANEL: Color32 = Color32::from_rgb(20, 23, 28);
    pub const BG_CARD: Color32 = Color32::from_rgb(26, 30, 36);
    pub const BG_CARD_HOVER: Color32 = Color32::from_rgb(33, 38, 46);
    pub const BG_INPUT: Color32 = Color32::from_rgb(18, 21, 26);

    // Hairline Borders (6-8% perceived opacity on dark background)
    pub const BORDER_QUIET: Color32 = Color32::from_rgb(40, 46, 56);
    pub const BORDER_FOCUS: Color32 = Color32::from_rgb(59, 130, 246);

    // Three Strict Text Roles
    pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(240, 243, 246);
    pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(156, 163, 175);
    pub const TEXT_MUTED: Color32 = Color32::from_rgb(107, 114, 128);

    // One Intentional Accent
    pub const ACCENT: Color32 = Color32::from_rgb(59, 130, 246);       // Cobalt Azure
    pub const ACCENT_HOVER: Color32 = Color32::from_rgb(96, 165, 250);

    // Toned-Down Semantic Colors (Carrying Meaning Only)
    pub const STATUS_SUCCESS: Color32 = Color32::from_rgb(34, 197, 94);  // Green
    pub const STATUS_WARNING: Color32 = Color32::from_rgb(234, 179, 8);  // Amber
    pub const STATUS_ERROR: Color32 = Color32::from_rgb(239, 68, 68);    // Rose / Red
    pub const STATUS_INFO: Color32 = Color32::from_rgb(56, 189, 248);    // Muted Sky Blue
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

    // Nested Radii: Window (8px) -> Card (6px) -> Control (4px)
    visuals.window_rounding = Rounding::same(8.0);
    visuals.menu_rounding = Rounding::same(4.0);
    visuals.window_stroke = Stroke::new(1.0, ThemeColors::BORDER_QUIET);

    // Widget states with calm chrome
    visuals.widgets.noninteractive.bg_fill = ThemeColors::BG_CARD;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, ThemeColors::BORDER_QUIET);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, ThemeColors::TEXT_SECONDARY);
    visuals.widgets.noninteractive.rounding = Rounding::same(4.0);

    visuals.widgets.inactive.bg_fill = ThemeColors::BG_CARD;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, ThemeColors::BORDER_QUIET);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, ThemeColors::TEXT_PRIMARY);
    visuals.widgets.inactive.rounding = Rounding::same(4.0);

    visuals.widgets.hovered.bg_fill = ThemeColors::BG_CARD_HOVER;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, ThemeColors::ACCENT);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, ThemeColors::TEXT_PRIMARY);
    visuals.widgets.hovered.rounding = Rounding::same(4.0);

    visuals.widgets.active.bg_fill = ThemeColors::ACCENT;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, ThemeColors::ACCENT_HOVER);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, ThemeColors::TEXT_PRIMARY);
    visuals.widgets.active.rounding = Rounding::same(4.0);

    visuals.widgets.open.bg_fill = ThemeColors::BG_CARD_HOVER;
    visuals.widgets.open.bg_stroke = Stroke::new(1.0, ThemeColors::BORDER_FOCUS);

    ctx.set_visuals(visuals);

    // Spacing configuration strictly on 4px grid
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = Vec2::new(8.0, 8.0);
    style.spacing.button_padding = Vec2::new(12.0, 6.0);
    style.spacing.window_margin = Margin::same(12.0);
    style.spacing.indent = 16.0;

    ctx.set_style(style);
}
