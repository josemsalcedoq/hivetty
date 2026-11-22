//! The tab bar (F2-03, F2-04): select, close, `+`, double-click to rename, drag to reorder, and
//! the indicators (state dot or spinner, unseen dot, bell, OSC 9;4 progress).

use egui::{Color32, CornerRadius, Key, Sense, Stroke, Vec2};

use crate::{PaletteItem, RowState, elide, state_color, strings, theme};

/// Tab bar height in points (docs/ui-spec.md 1).
pub const TAB_BAR_HEIGHT: f32 = 36.0;
const TAB_MIN_WIDTH: f32 = 120.0;
const TAB_MAX_WIDTH: f32 = 240.0;
const PLUS_WIDTH: f32 = 32.0;
const MENU_WIDTH: f32 = 32.0;
/// Spinner: 8 frames at 12 fps (docs/ui-spec.md 1, movement).
const SPINNER_FPS: f64 = 12.0;

/// One tab in the tab bar.
#[derive(Debug, Clone, PartialEq)]
pub struct TabItem {
    /// Title (the focused pane's title or the user's name for the tab).
    pub title: String,
    /// The most urgent state among its panes, for the dot.
    pub state: RowState,
    /// Whether it is the active tab.
    pub active: bool,
    /// Something finished or changed here while the user looked elsewhere.
    pub unseen: bool,
    /// The bell rang here and nobody has looked yet.
    pub bell: bool,
    /// OSC 9;4 progress, 0..=1, or `None`.
    pub progress: Option<f32>,
    /// OSC 9;4 error state (red bar).
    pub progress_error: bool,
}

impl TabItem {
    /// A plain tab with just a title and state.
    pub fn new(title: impl Into<String>, state: RowState, active: bool) -> Self {
        Self {
            title: title.into(),
            state,
            active,
            unseen: false,
            bell: false,
            progress: None,
            progress_error: false,
        }
    }
}

/// What the user did in the tab bar this frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TabAction {
    /// Activate the tab at this index.
    Select(usize),
    /// Close the tab at this index.
    Close(usize),
    /// Open a new tab.
    New,
    /// Rename the tab (an empty name restores the automatic title).
    Rename(usize, String),
    /// Move the tab at `from` to position `to`.
    Move {
        /// Old index.
        from: usize,
        /// New index.
        to: usize,
    },
    /// An entry of the ☰ menu (its [`PaletteItem::id`]).
    Menu(usize),
    /// The empty part of the bar was dragged: move the window (custom title bar).
    DragWindow,
    /// Maximize or restore the window (button or double click on the bar).
    ToggleMaximize,
    /// Minimize the window.
    Minimize,
    /// Close the window.
    CloseWindow,
}

/// How the tab bar doubles as the window's title bar (F1-02).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TitleBar {
    /// The OS draws its own title bar above.
    #[default]
    Native,
    /// No OS title bar: the bar drags the window and draws minimize / maximize / close.
    Custom {
        /// Whether the window is maximized (picks the maximize or restore icon).
        maximized: bool,
    },
    /// macOS transparent title bar: leave room on the left for the traffic lights.
    MacInset,
}

const CONTROL_WIDTH: f32 = 46.0;
const MAC_INSET: f32 = 78.0;

/// State kept between frames.
#[derive(Debug, Default)]
pub(crate) struct TabBar {
    /// Tab being renamed, its text, and whether the field still needs focus.
    renaming: Option<(usize, String, bool)>,
    /// Tab being dragged.
    dragging: Option<usize>,
    /// Tab count last frame.
    last_len: usize,
}

impl TabBar {
    /// Whether a text field in the bar has the keyboard.
    pub(crate) fn editing(&self) -> bool {
        self.renaming.is_some()
    }

    pub(crate) fn show(
        &mut self,
        ui: &mut egui::Ui,
        tabs: &[TabItem],
        menu: &[Vec<PaletteItem>],
        title_bar: TitleBar,
    ) -> Option<TabAction> {
        let mut action = None;
        let menu_width = if menu.is_empty() { 0.0 } else { MENU_WIDTH };
        let chrome_width = match title_bar {
            TitleBar::Native => 0.0,
            // Room to grab the window, plus the three buttons.
            TitleBar::Custom { .. } => CONTROL_WIDTH * 3.0 + 80.0,
            TitleBar::MacInset => MAC_INSET + 80.0,
        };
        let available =
            (ui.available_width() - PLUS_WIDTH - menu_width - chrome_width).max(TAB_MIN_WIDTH);
        let width = (available / tabs.len().max(1) as f32).clamp(TAB_MIN_WIDTH, TAB_MAX_WIDTH);
        // A tab closing or opening shifts indices: drop a rename or drag in progress.
        if tabs.len() != self.last_len {
            self.renaming = None;
            self.dragging = None;
            self.last_len = tabs.len();
        }
        if tabs.iter().any(|t| t.state == RowState::Working) {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_secs_f64(1.0 / SPINNER_FPS));
        }
        let mut rects = Vec::with_capacity(tabs.len());
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            if title_bar == TitleBar::MacInset {
                ui.add_space(MAC_INSET);
            }
            if !menu.is_empty()
                && let Some(id) = hamburger(ui, menu)
            {
                action = Some(TabAction::Menu(id));
            }
            for (i, tab) in tabs.iter().enumerate() {
                let (rect, a) = self.tab(ui, tab, width, i);
                rects.push(rect);
                if a.is_some() {
                    action = a;
                }
            }
            let (rect, response) =
                ui.allocate_exact_size(Vec2::new(PLUS_WIDTH, TAB_BAR_HEIGHT - 8.0), Sense::click());
            response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::Role::Button, true, strings::NEW_TAB)
            });
            let painter = ui.painter_at(rect);
            if response.hovered() {
                painter.rect_filled(rect, CornerRadius::same(6), theme::HOVER_BG());
            }
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "+",
                egui::FontId::proportional(16.0),
                theme::TEXT_SECONDARY(),
            );
            if response.on_hover_text(strings::NEW_TAB).clicked() {
                action = Some(TabAction::New);
            }
            if title_bar != TitleBar::Native {
                let controls = match title_bar {
                    TitleBar::Custom { .. } => CONTROL_WIDTH * 3.0,
                    _ => 0.0,
                };
                // The empty rest of the bar moves the window and maximizes on double click.
                let free = (ui.available_width() - controls).max(0.0);
                let (_, drag) = ui
                    .allocate_exact_size(Vec2::new(free, TAB_BAR_HEIGHT), Sense::click_and_drag());
                if drag.double_clicked() {
                    action = Some(TabAction::ToggleMaximize);
                } else if drag.drag_started() {
                    action = Some(TabAction::DragWindow);
                }
            }
            if let TitleBar::Custom { maximized } = title_bar
                && let Some(a) = window_controls(ui, maximized)
            {
                action = Some(a);
            }
        });
        // Finish a drag: the drop position decides the new index.
        if let Some(from) = self.dragging {
            let released = ui.input(|i| i.pointer.any_released());
            if released {
                self.dragging = None;
                let x = ui.input(|i| i.pointer.interact_pos()).map(|p| p.x);
                if let Some(x) = x {
                    // Insert before the first tab whose middle is right of the pointer; removing
                    // the dragged tab first shifts later positions left by one.
                    let before = rects
                        .iter()
                        .position(|r| x < r.center().x)
                        .unwrap_or(rects.len());
                    let to = if before > from { before - 1 } else { before };
                    if to != from {
                        action = Some(TabAction::Move { from, to });
                    }
                }
            }
        }
        action
    }

    /// One tab. Returns its rectangle and what was clicked.
    fn tab(
        &mut self,
        ui: &mut egui::Ui,
        tab: &TabItem,
        width: f32,
        index: usize,
    ) -> (egui::Rect, Option<TabAction>) {
        let (rect, response) = ui.allocate_exact_size(
            Vec2::new(width, TAB_BAR_HEIGHT - 8.0),
            Sense::click_and_drag(),
        );
        let label = tab.title.clone();
        response
            .widget_info(|| egui::WidgetInfo::selected(egui::Role::Tab, true, tab.active, &label));
        let painter = ui.painter_at(rect);
        let dragged = self.dragging == Some(index);
        let bg = if tab.active || dragged {
            theme::SELECTED_BG()
        } else if response.hovered() {
            theme::HOVER_BG()
        } else {
            Color32::TRANSPARENT
        };
        painter.rect_filled(rect, CornerRadius::same(6), bg);
        if dragged {
            painter.rect_stroke(
                rect,
                CornerRadius::same(6),
                Stroke::new(1.0, theme::BORDER()),
                egui::StrokeKind::Inside,
            );
        }

        // Left indicator: spinner while working, otherwise the state dot.
        let center = egui::pos2(rect.left() + 12.0, rect.center().y);
        if tab.state == RowState::Working {
            spinner(&painter, ui.input(|i| i.time), center);
        } else {
            painter.circle_filled(center, 3.5, state_color(tab.state));
        }

        let close_rect = egui::Rect::from_center_size(
            egui::pos2(rect.right() - 14.0, rect.center().y),
            Vec2::splat(18.0),
        );
        let show_close = tab.active || response.hovered();
        let mut text_right = if show_close {
            close_rect.left() - 4.0
        } else {
            rect.right() - 8.0
        };
        // Right indicators: bell and unseen dot.
        if tab.bell {
            painter.text(
                egui::pos2(text_right, rect.center().y),
                egui::Align2::RIGHT_CENTER,
                "🔔",
                egui::FontId::proportional(11.0),
                theme::STATE_WAITING(),
            );
            text_right -= 16.0;
        }
        if tab.unseen && !tab.active {
            painter.circle_filled(
                egui::pos2(text_right - 3.0, rect.center().y),
                3.0,
                theme::ACCENT(),
            );
            text_right -= 10.0;
        }

        let mut action = None;
        let text_left = rect.left() + 22.0;
        let renaming_this = self.renaming.as_ref().is_some_and(|(i, ..)| *i == index);
        if renaming_this {
            if let Some((_, text, needs_focus)) = self.renaming.as_mut() {
                let field = egui::Rect::from_min_max(
                    egui::pos2(text_left, rect.top() + 4.0),
                    egui::pos2(text_right, rect.bottom() - 4.0),
                );
                let edit = ui.put(
                    field,
                    egui::TextEdit::singleline(text)
                        .font(egui::FontId::proportional(13.0))
                        .frame(egui::Frame::NONE),
                );
                if *needs_focus {
                    edit.request_focus();
                    *needs_focus = false;
                }
                let (enter, escape) =
                    ui.input(|i| (i.key_pressed(Key::Enter), i.key_pressed(Key::Escape)));
                if escape {
                    self.renaming = None;
                } else if enter || edit.lost_focus() {
                    action = Some(TabAction::Rename(index, text.trim().to_owned()));
                    self.renaming = None;
                }
            }
        } else {
            let font = egui::FontId::proportional(13.0);
            let title = elide(ui, &tab.title, &font, text_right - text_left);
            let color = if tab.active {
                theme::TEXT_PRIMARY()
            } else {
                theme::TEXT_SECONDARY()
            };
            painter.text(
                egui::pos2(text_left, rect.center().y),
                egui::Align2::LEFT_CENTER,
                title,
                font,
                color,
            );
        }

        // OSC 9;4 progress: a thin bar along the bottom.
        if let Some(progress) = tab.progress {
            let bar = egui::Rect::from_min_size(
                egui::pos2(rect.left() + 6.0, rect.bottom() - 3.0),
                Vec2::new((rect.width() - 12.0) * progress.clamp(0.0, 1.0), 2.0),
            );
            let color = if tab.progress_error {
                theme::STATE_ERROR()
            } else {
                theme::ACCENT()
            };
            painter.rect_filled(bar, CornerRadius::same(1), color);
        }

        if show_close && !renaming_this {
            let close = ui.interact(
                close_rect,
                ui.scope_id().with(("close-tab", index)),
                Sense::click(),
            );
            close.widget_info(|| {
                egui::WidgetInfo::labeled(egui::Role::Button, true, strings::CLOSE_TAB)
            });
            if close.hovered() {
                painter.rect_filled(close_rect, CornerRadius::same(4), theme::BORDER());
            }
            painter.text(
                close_rect.center(),
                egui::Align2::CENTER_CENTER,
                "×",
                egui::FontId::proportional(14.0),
                theme::TEXT_MUTED(),
            );
            if close.clicked() {
                action = Some(TabAction::Close(index));
            }
        }
        if action.is_none() && !renaming_this {
            if response.double_clicked() {
                self.renaming = Some((index, tab.title.clone(), true));
            } else if response.clicked() {
                action = Some(TabAction::Select(index));
            } else if response.drag_started() {
                self.dragging = Some(index);
            }
        }
        if response.middle_clicked() {
            action = Some(TabAction::Close(index));
        }
        (rect, action)
    }
}

/// Minimize, maximize / restore and close, drawn as lines (the UI font has no such glyphs).
fn window_controls(ui: &mut egui::Ui, maximized: bool) -> Option<TabAction> {
    let mut action = None;
    ui.spacing_mut().item_spacing.x = 0.0;
    for (kind, label) in [
        (0, strings::MINIMIZE),
        (
            1,
            if maximized {
                strings::RESTORE
            } else {
                strings::MAXIMIZE
            },
        ),
        (2, strings::CLOSE_WINDOW),
    ] {
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(CONTROL_WIDTH, TAB_BAR_HEIGHT), Sense::click());
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label));
        let painter = ui.painter_at(rect);
        let hovered = response.hovered();
        if hovered {
            let bg = if kind == 2 {
                theme::STATE_ERROR()
            } else {
                theme::HOVER_BG()
            };
            painter.rect_filled(rect, CornerRadius::ZERO, bg);
        }
        let color = if hovered && kind == 2 {
            egui::Color32::WHITE
        } else {
            theme::TEXT_SECONDARY()
        };
        let stroke = Stroke::new(1.0, color);
        let c = rect.center();
        match kind {
            0 => {
                painter.line_segment([c + Vec2::new(-5.0, 0.0), c + Vec2::new(5.0, 0.0)], stroke);
            }
            1 if maximized => {
                painter.rect_stroke(
                    egui::Rect::from_center_size(c + Vec2::new(-1.0, 1.0), Vec2::splat(8.0)),
                    CornerRadius::ZERO,
                    stroke,
                    egui::StrokeKind::Inside,
                );
                painter.line_segment(
                    [c + Vec2::new(-2.0, -4.0), c + Vec2::new(5.0, -4.0)],
                    stroke,
                );
                painter.line_segment([c + Vec2::new(5.0, -4.0), c + Vec2::new(5.0, 3.0)], stroke);
            }
            1 => {
                painter.rect_stroke(
                    egui::Rect::from_center_size(c, Vec2::splat(10.0)),
                    CornerRadius::ZERO,
                    stroke,
                    egui::StrokeKind::Inside,
                );
            }
            _ => {
                painter.line_segment([c + Vec2::new(-5.0, -5.0), c + Vec2::new(5.0, 5.0)], stroke);
                painter.line_segment([c + Vec2::new(-5.0, 5.0), c + Vec2::new(5.0, -5.0)], stroke);
            }
        }
        if response.on_hover_text(label).clicked() {
            action = Some(match kind {
                0 => TabAction::Minimize,
                1 => TabAction::ToggleMaximize,
                _ => TabAction::CloseWindow,
            });
        }
    }
    action
}

/// The ☰ button and its menu (Windows and Linux; macOS uses the system menu bar).
fn hamburger(ui: &mut egui::Ui, sections: &[Vec<PaletteItem>]) -> Option<usize> {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(MENU_WIDTH, TAB_BAR_HEIGHT - 8.0), Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, strings::MENU));
    let painter = ui.painter_at(rect);
    if response.hovered() {
        painter.rect_filled(rect, CornerRadius::same(6), theme::HOVER_BG());
    }
    // Three lines: the UI font has no ☰ glyph.
    for dy in [-4.0, 0.0, 4.0] {
        let y = rect.center().y + dy;
        painter.line_segment(
            [
                egui::pos2(rect.center().x - 6.0, y),
                egui::pos2(rect.center().x + 6.0, y),
            ],
            Stroke::new(1.5, theme::TEXT_SECONDARY()),
        );
    }
    let response = response.on_hover_text(strings::MENU);
    let mut chosen = None;
    egui::Popup::menu(&response).show(|ui| {
        ui.set_min_width(260.0);
        for (i, section) in sections.iter().enumerate() {
            if i > 0 {
                ui.separator();
            }
            for item in section {
                let button = egui::Button::new(&item.label).shortcut_text(&item.hint);
                if ui.add(button).clicked() {
                    chosen = Some(item.id);
                    ui.close();
                }
            }
        }
    });
    chosen
}

/// Eight dots around a circle, one bright, rotating at 12 fps.
fn spinner(painter: &egui::Painter, time: f64, center: egui::Pos2) {
    let frame = (time * SPINNER_FPS) as usize % 8;
    for i in 0..8 {
        let angle = i as f32 / 8.0 * std::f32::consts::TAU;
        let pos = center + Vec2::new(angle.cos(), angle.sin()) * 4.0;
        let age = (i + 8 - frame) % 8;
        let alpha = (255 - age as u32 * 28).max(40) as u8;
        painter.circle_filled(
            pos,
            1.1,
            Color32::from_rgba_unmultiplied(0xED, 0xED, 0xEF, alpha),
        );
    }
}
