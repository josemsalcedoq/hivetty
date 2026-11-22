//! Design tokens from docs/ui-spec.md section 1 (dark theme) applied to egui.

use egui::Color32;

pub(crate) const SIDEBAR_BG: Color32 = Color32::from_rgb(0x11, 0x11, 0x13);
pub(crate) const ELEVATED_BG: Color32 = Color32::from_rgb(0x18, 0x18, 0x1B);
/// Progress bars and the unseen dot (`chart.output`).
pub(crate) const ACCENT: Color32 = Color32::from_rgb(0x4E, 0xA1, 0xFF);
pub(crate) const SELECTED_BG: Color32 = Color32::from_rgb(0x23, 0x23, 0x27);
pub(crate) const HOVER_BG: Color32 = Color32::from_rgb(0x1C, 0x1C, 0x20);
pub(crate) const BORDER: Color32 = Color32::from_rgb(0x26, 0x26, 0x2B);
pub(crate) const TEXT_PRIMARY: Color32 = Color32::from_rgb(0xED, 0xED, 0xEF);
pub(crate) const TEXT_SECONDARY: Color32 = Color32::from_rgb(0x9A, 0x9A, 0xA2);
pub(crate) const TEXT_MUTED: Color32 = Color32::from_rgb(0x6B, 0x6B, 0x73);
pub(crate) const STATE_WORKING: Color32 = Color32::from_rgb(0x9A, 0x9A, 0xA2);
pub(crate) const STATE_WAITING: Color32 = Color32::from_rgb(0xE3, 0xA0, 0x08);
pub(crate) const STATE_DONE: Color32 = Color32::from_rgb(0x3F, 0xB9, 0x50);
pub(crate) const STATE_ERROR: Color32 = Color32::from_rgb(0xF8, 0x51, 0x49);

/// Applies the dark theme to egui.
pub(crate) fn apply(ctx: &egui::Context) {
    ctx.set_visuals(egui::Visuals::dark());
    ctx.global_style_mut(|style| {
        style.visuals.panel_fill = SIDEBAR_BG;
        style.visuals.override_text_color = Some(TEXT_PRIMARY);
    });
}
