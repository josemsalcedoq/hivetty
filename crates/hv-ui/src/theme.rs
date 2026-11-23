//! Design tokens from docs/ui-spec.md section 1, in a dark and a light variant (F6-01).
//! The tokens are functions so the whole UI follows [`set_light`] at once.

#![allow(non_snake_case)]

use std::sync::atomic::{AtomicBool, Ordering};

use egui::Color32;

static LIGHT: AtomicBool = AtomicBool::new(false);

fn light() -> bool {
    LIGHT.load(Ordering::Relaxed)
}

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

macro_rules! token {
    ($(#[$doc:meta])* $name:ident, $dark:expr, $light:expr) => {
        $(#[$doc])*
        pub(crate) fn $name() -> Color32 {
            if light() { rgb($light) } else { rgb($dark) }
        }
    };
}

token!(WINDOW_BG, 0x0A0A0B, 0xFFFFFF);
token!(SIDEBAR_BG, 0x111113, 0xF6F6F7);
token!(ELEVATED_BG, 0x18181B, 0xFFFFFF);
token!(
    /// Progress bars and the unseen dot (`chart.output`).
    ACCENT,
    0x4EA1FF,
    0x0969DA
);
token!(
    /// Fill of a primary button, under white text.
    ACCENT_STRONG,
    0x1F6FEB,
    0x0969DA
);
token!(SELECTED_BG, 0x232327, 0xE6E6EB);
token!(HOVER_BG, 0x1C1C20, 0xEFEFF2);
token!(BORDER, 0x26262B, 0xDADADF);
token!(
    /// Outline of controls (checkboxes, fields), stronger than [`BORDER`].
    CONTROL_BORDER,
    0x3A3A41,
    0xC4C4CB
);
token!(TEXT_PRIMARY, 0xEDEDEF, 0x18181B);
token!(TEXT_SECONDARY, 0x9A9AA2, 0x52525B);
token!(TEXT_MUTED, 0x6B6B73, 0x7E7E87);
token!(STATE_WORKING, 0x9A9AA2, 0x6E6E77);
token!(STATE_WAITING, 0xE3A008, 0xB7791F);
token!(STATE_DONE, 0x3FB950, 0x1A7F37);
token!(STATE_ERROR, 0xF85149, 0xCF222E);

/// Switches every token (and egui's own widgets) to the light or dark variant.
pub(crate) fn set_light(ctx: &egui::Context, on: bool) {
    LIGHT.store(on, Ordering::Relaxed);
    apply(ctx);
}

/// Applies the current variant to egui.
pub(crate) fn apply(ctx: &egui::Context) {
    ctx.set_visuals(if light() {
        egui::Visuals::light()
    } else {
        egui::Visuals::dark()
    });
    ctx.global_style_mut(|style| {
        style.visuals.panel_fill = SIDEBAR_BG();
        style.visuals.override_text_color = Some(TEXT_PRIMARY());
        // egui's own widgets (buttons, checkboxes, fields, menus) in the same tokens and
        // sizes as the painted parts, so they line up with them.
        let spacing = &mut style.spacing;
        spacing.button_padding = egui::vec2(8.0, 3.0);
        spacing.interact_size.y = 22.0;
        spacing.icon_width = 16.0;
        spacing.icon_width_inner = 9.0;
        spacing.icon_spacing = 8.0;
        // Thin scroll bars that stay faintly visible, so a list that scrolls looks like it.
        spacing.scroll = egui::style::ScrollStyle::thin();
        spacing.scroll.dormant_handle_opacity = 0.45;
        spacing.scroll.dormant_background_opacity = 0.0;
        spacing.scroll.foreground_color = true;
        let visuals = &mut style.visuals;
        visuals.extreme_bg_color = WINDOW_BG();
        visuals.window_fill = ELEVATED_BG();
        visuals.window_stroke = egui::Stroke::new(1.0, BORDER());
        visuals.window_corner_radius = egui::CornerRadius::same(10);
        visuals.menu_corner_radius = egui::CornerRadius::same(8);
        visuals.selection.bg_fill = ACCENT_STRONG().gamma_multiply(0.6);
        visuals.selection.stroke = egui::Stroke::new(1.0, ACCENT());
        let radius = egui::CornerRadius::same(6);
        let widgets = &mut visuals.widgets;
        widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, BORDER());
        widgets.noninteractive.corner_radius = radius;
        for (state, fill, stroke) in [
            (&mut widgets.inactive, SELECTED_BG(), CONTROL_BORDER()),
            (&mut widgets.hovered, HOVER_BG(), TEXT_MUTED()),
            (&mut widgets.active, SELECTED_BG(), ACCENT()),
            (&mut widgets.open, SELECTED_BG(), BORDER()),
        ] {
            state.weak_bg_fill = fill;
            state.bg_fill = fill;
            state.corner_radius = radius;
            state.expansion = 0.0;
            state.bg_stroke = egui::Stroke::new(1.0, stroke);
        }
        widgets.hovered.weak_bg_fill = SELECTED_BG();
    });
}
