//! The sessions sidebar (F4-11, F4-12): a search box, filters with counts, and the sessions
//! grouped by project.

use egui::{Color32, CornerRadius, RichText, Sense, Stroke, Vec2};

use crate::{
    ArchivedRow, ProjectMenu, RowState, SessionMenu, SessionRow, elide, state_color, strings, theme,
};

/// Which sessions the list shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Filter {
    #[default]
    All,
    NeedsYou,
    Working,
    Done,
}

impl Filter {
    fn keeps(self, state: RowState) -> bool {
        match self {
            Self::All => true,
            Self::NeedsYou => state == RowState::Waiting,
            Self::Working => state == RowState::Working,
            Self::Done => matches!(state, RowState::Done | RowState::Error),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::All => strings::FILTER_ALL,
            Self::NeedsYou => strings::FILTER_NEEDS_YOU,
            Self::Working => strings::FILTER_WORKING,
            Self::Done => strings::FILTER_DONE,
        }
    }
}

/// State kept between frames.
#[derive(Debug, Default)]
pub(crate) struct Sidebar {
    query: String,
    filter: Filter,
    /// Projects whose sessions are hidden (S-14).
    collapsed: std::collections::HashSet<String>,
    /// Put the keyboard in the search field next frame (S-03).
    pub(crate) focus_search: bool,
}

impl Sidebar {
    /// Draws the sidebar from `model`, leaving `dock_height` free at the bottom. Returns the
    /// index (in `model.sessions`) of a clicked row and whether New session was pressed.
    pub(crate) fn show(
        &mut self,
        ui: &mut egui::Ui,
        model: &crate::ChromeModel,
        dock_height: f32,
    ) -> SidebarOutput {
        let sessions: &[SessionRow] = &model.sessions;
        let archived: &[ArchivedRow] = &model.archived;
        let new_session_hint = model.new_session_hint.as_str();
        let search_hint = model.search_hint.as_str();
        let announcement = model.announcement.as_deref();
        let mut out = SidebarOutput::default();
        let mut clicked = None;
        let mut new_session = false;
        let mut peek = None;
        // Header: as tall as the tab bar, with a line under it at the same height as the tab
        // bar's, so the top of the window reads as one band.
        let (band, _) = ui.allocate_exact_size(
            Vec2::new(ui.available_width(), crate::TAB_BAR_HEIGHT),
            Sense::hover(),
        );
        ui.painter().hline(
            ui.clip_rect().x_range(),
            band.bottom() - 0.5,
            Stroke::new(1.0, theme::BORDER()),
        );
        ui.scope_builder(
            egui::UiBuilder::new()
                .max_rect(band)
                .layout(egui::Layout::right_to_left(egui::Align::Center)),
            |ui| {
                let tip = if new_session_hint.is_empty() {
                    strings::NEW_SESSION.to_owned()
                } else {
                    format!("{} ({new_session_hint})", strings::NEW_SESSION)
                };
                let button =
                    egui::Button::new(RichText::new(strings::NEW_SESSION_SHORT).size(12.0))
                        .corner_radius(CornerRadius::same(6))
                        .min_size(Vec2::new(0.0, 24.0));
                if ui.add(button).on_hover_text(tip).clicked() {
                    new_session = true;
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    // The macOS window buttons sit here, at the window's top left corner.
                    let inset = if model.title_bar == crate::TitleBar::MacInset {
                        crate::tabs::MAC_INSET - (band.left() - ui.clip_rect().left())
                    } else {
                        2.0
                    };
                    ui.add_space(inset.max(2.0));
                    ui.label(RichText::new(strings::SESSIONS).size(13.0).strong());
                });
            },
        );
        ui.add_space(10.0);
        let search = egui::TextEdit::singleline(&mut self.query)
            .hint_text(strings::SEARCH_SESSIONS)
            .margin(egui::Margin::symmetric(8, 5))
            .desired_width(f32::INFINITY);
        let search = ui.add(search);
        // The shortcut at the right end of the empty field, when there is room for it.
        if self.query.is_empty() && !search_hint.is_empty() && !search.has_focus() {
            let font = egui::FontId::proportional(11.0);
            let width = |text: &str| {
                ui.fonts_mut(|f| {
                    f.layout_no_wrap(text.to_owned(), font.clone(), Color32::WHITE)
                        .size()
                        .x
                })
            };
            let needed = width(strings::SEARCH_SESSIONS) * 13.0 / 11.0 + width(search_hint) + 32.0;
            if search.rect.width() >= needed {
                ui.painter().text(
                    egui::pos2(search.rect.right() - 8.0, search.rect.center().y),
                    egui::Align2::RIGHT_CENTER,
                    search_hint,
                    font,
                    theme::TEXT_MUTED(),
                );
            }
        }
        if std::mem::take(&mut self.focus_search) {
            search.request_focus();
        }
        // Enter opens the first session that matches; Esc gives the keyboard back.
        let open_first = search.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        ui.add_space(8.0);
        self.filters(ui, sessions);
        ui.add_space(4.0);

        let query = self.query.trim().to_lowercase();
        let visible: Vec<(usize, &SessionRow)> = sessions
            .iter()
            .enumerate()
            .filter(|(_, s)| self.filter.keeps(s.state))
            .filter(|(_, s)| query.is_empty() || matches_query(s, &query))
            .collect();
        // Leave room below for the agent dock.
        let list_height = (ui.available_height() - dock_height).max(40.0);
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .max_height(list_height)
            .show(ui, |ui| {
                // Group by project in order of first appearance.
                let mut projects: Vec<&str> = Vec::new();
                for (_, s) in &visible {
                    if !projects.contains(&s.project.as_str()) {
                        projects.push(&s.project);
                    }
                }
                for project in projects {
                    ui.add_space(6.0);
                    let count = visible.iter().filter(|(_, s)| s.project == project).count();
                    let collapsed = self.collapsed.contains(project);
                    let header = project_header(ui, project, count, collapsed);
                    let first = visible
                        .iter()
                        .find(|(_, s)| s.project == project)
                        .map(|(i, _)| *i);
                    let mut toggle = header.clicked();
                    header.context_menu(|ui| {
                        let fold = if collapsed {
                            strings::MENU_EXPAND
                        } else {
                            strings::MENU_COLLAPSE
                        };
                        if ui.button(strings::MENU_NEW_SESSION_HERE).clicked() {
                            out.project_menu = first.map(|i| (i, ProjectMenu::NewSession));
                            ui.close();
                        }
                        if ui.button(fold).clicked() {
                            toggle = true;
                            ui.close();
                        }
                        if ui.button(strings::MENU_SHOW_FOLDER).clicked() {
                            out.project_menu = first.map(|i| (i, ProjectMenu::ShowFolder));
                            ui.close();
                        }
                        if ui.button(strings::MENU_ARCHIVE_FINISHED).clicked() {
                            out.project_menu = first.map(|i| (i, ProjectMenu::ArchiveFinished));
                            ui.close();
                        }
                    });
                    if toggle && !self.collapsed.remove(project) {
                        self.collapsed.insert(project.to_owned());
                    }
                    if collapsed {
                        continue;
                    }
                    for (index, row) in visible.iter().filter(|(_, s)| s.project == project) {
                        let (click, card) = session_row(ui, row);
                        out.card_shown |= card;
                        match click {
                            RowClick::Open => clicked = Some(*index),
                            RowClick::Peek => peek = Some(*index),
                            RowClick::Menu(choice) => out.menu = Some((*index, choice)),
                            RowClick::None => {}
                        }
                    }
                }
                // The latest change that needs the user, as a live region: screen readers read
                // it out when it changes (F4-25).
                if let Some(text) = announcement {
                    ui.add_space(8.0);
                    let label = ui.label(RichText::new(text).size(11.0).color(theme::TEXT_MUTED()));
                    ui.ctx().accesskit_node_builder(label.id, |node| {
                        node.set_live(egui::accesskit::Live::Polite);
                    });
                }
                if sessions.is_empty() {
                    ui.label(RichText::new(strings::NO_SESSIONS).color(theme::TEXT_MUTED()));
                } else if visible.is_empty() {
                    ui.label(RichText::new(strings::NO_MATCHES).color(theme::TEXT_MUTED()));
                }
                if !archived.is_empty() {
                    ui.add_space(10.0);
                    egui::CollapsingHeader::new(
                        RichText::new(format!("{} {}", strings::ARCHIVED, archived.len()))
                            .size(12.0)
                            .color(theme::TEXT_SECONDARY()),
                    )
                    .id_salt("archived")
                    .default_open(false)
                    .show(ui, |ui| {
                        for (i, row) in archived.iter().enumerate() {
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    ui.label(RichText::new(&row.title).size(12.0));
                                    ui.label(
                                        RichText::new(&row.detail)
                                            .size(11.0)
                                            .color(theme::TEXT_MUTED()),
                                    );
                                });
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui.small_button(strings::DELETE).clicked() {
                                            out.archive_delete = Some(i);
                                        }
                                        if ui.small_button(strings::RESTORE_SESSION).clicked() {
                                            out.archive_restore = Some(i);
                                        }
                                    },
                                );
                            });
                        }
                    });
                }
            });
        if open_first && clicked.is_none() {
            let query = self.query.trim().to_lowercase();
            clicked = sessions
                .iter()
                .enumerate()
                .find(|(_, s)| {
                    self.filter.keeps(s.state) && (query.is_empty() || matches_query(s, &query))
                })
                .map(|(i, _)| i);
        }
        out.clicked = clicked;
        out.new_session = new_session;
        out.peek = peek;
        out
    }
}

impl Sidebar {
    /// The filters, as one segmented control the width of the sidebar. Labels with counts
    /// when they fit; otherwise each state's dot and count (the full label on hover).
    fn filters(&mut self, ui: &mut egui::Ui, sessions: &[SessionRow]) {
        const FILTERS: [Filter; 4] = [Filter::All, Filter::NeedsYou, Filter::Working, Filter::Done];
        let (rect, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 26.0), Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect(
            rect,
            CornerRadius::same(7),
            theme::WINDOW_BG(),
            Stroke::new(1.0, theme::BORDER()),
            egui::StrokeKind::Inside,
        );
        let inner = rect.shrink(2.0);
        let gap = 2.0;
        let font = egui::FontId::proportional(11.0);
        let labels: Vec<(String, usize)> = FILTERS
            .iter()
            .map(|f| {
                let count = sessions.iter().filter(|s| f.keeps(s.state)).count();
                (format!("{} {count}", f.label()), count)
            })
            .collect();
        let text_width = |text: &str| {
            ui.fonts_mut(|f| {
                f.layout_no_wrap(text.to_owned(), font.clone(), Color32::WHITE)
                    .size()
                    .x
            })
        };
        let full: Vec<f32> = labels.iter().map(|(t, _)| text_width(t) + 12.0).collect();
        let room = inner.width() - gap * 3.0;
        let compact = full.iter().sum::<f32>() > room;
        let widths: Vec<f32> = if compact {
            vec![room / 4.0; 4]
        } else {
            // Each as wide as its label, the space left shared out evenly.
            let extra = (room - full.iter().sum::<f32>()) / 4.0;
            full.iter().map(|w| w + extra).collect()
        };
        let mut x = inner.left();
        for (i, filter) in FILTERS.iter().enumerate() {
            let seg = egui::Rect::from_min_size(
                egui::pos2(x, inner.top()),
                Vec2::new(widths[i], inner.height()),
            );
            x += widths[i] + gap;
            let (label, count) = &labels[i];
            let response = ui.interact(seg, ui.scope_id().with(("filter", i)), Sense::click());
            let selected = self.filter == *filter;
            response.widget_info(|| {
                egui::WidgetInfo::selected(egui::Role::RadioButton, true, selected, label.as_str())
            });
            let fill = if selected {
                theme::SELECTED_BG()
            } else if response.hovered() {
                theme::HOVER_BG()
            } else {
                Color32::TRANSPARENT
            };
            painter.rect_filled(seg, CornerRadius::same(5), fill);
            let color = if selected {
                theme::TEXT_PRIMARY()
            } else {
                theme::TEXT_SECONDARY()
            };
            if compact && *filter != Filter::All {
                // A dot in the state's color, then the count, centered together.
                let number = count.to_string();
                let w = 6.0 + 4.0 + text_width(&number);
                let left = seg.center().x - w / 2.0;
                let dot = match filter {
                    Filter::NeedsYou => theme::STATE_WAITING(),
                    Filter::Done => theme::STATE_DONE(),
                    _ => theme::STATE_WORKING(),
                };
                painter.circle_filled(egui::pos2(left + 3.0, seg.center().y), 3.0, dot);
                painter.text(
                    egui::pos2(left + 10.0, seg.center().y),
                    egui::Align2::LEFT_CENTER,
                    number,
                    font.clone(),
                    color,
                );
            } else {
                let text = if compact {
                    filter.label().to_owned()
                } else {
                    label.clone()
                };
                painter.text(
                    seg.center(),
                    egui::Align2::CENTER_CENTER,
                    text,
                    font.clone(),
                    color,
                );
            }
            let response = if compact {
                response.on_hover_text(label.as_str())
            } else {
                response
            };
            if response.clicked() {
                self.filter = *filter;
            }
        }
    }
}

/// A project's heading in the list: fold arrow, name, and the number of sessions at the right.
fn project_header(
    ui: &mut egui::Ui,
    project: &str,
    count: usize,
    collapsed: bool,
) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 22.0), Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::Role::Button, true, format!("{project}, {count}"))
    });
    let painter = ui.painter_at(rect);
    if response.hovered() {
        painter.rect_filled(rect, CornerRadius::same(5), theme::HOVER_BG());
    }
    let color = theme::TEXT_SECONDARY();
    // The fonts have no triangles: painted.
    let c = egui::pos2(rect.left() + 9.0, rect.center().y);
    let points = if collapsed {
        vec![
            c + Vec2::new(-2.0, -4.0),
            c + Vec2::new(-2.0, 4.0),
            c + Vec2::new(3.0, 0.0),
        ]
    } else {
        vec![
            c + Vec2::new(-4.0, -2.0),
            c + Vec2::new(4.0, -2.0),
            c + Vec2::new(0.0, 3.0),
        ]
    };
    painter.add(egui::Shape::convex_polygon(points, color, Stroke::NONE));
    let font = egui::FontId::proportional(12.0);
    let number = painter.text(
        egui::pos2(rect.right() - 8.0, rect.center().y),
        egui::Align2::RIGHT_CENTER,
        count.to_string(),
        font.clone(),
        theme::TEXT_MUTED(),
    );
    let name_left = rect.left() + 20.0;
    let name = elide(
        ui,
        project,
        &font,
        (number.left() - name_left - 8.0).max(10.0),
    );
    painter.text(
        egui::pos2(name_left, rect.center().y),
        egui::Align2::LEFT_CENTER,
        name,
        font,
        color,
    );
    response
}

fn matches_query(row: &SessionRow, query: &str) -> bool {
    [&row.title, &row.subtitle, &row.project, &row.agent]
        .iter()
        .any(|field| field.to_lowercase().contains(query))
}

/// What the sidebar produced this frame.
#[derive(Debug, Default)]
pub(crate) struct SidebarOutput {
    pub(crate) clicked: Option<usize>,
    pub(crate) new_session: bool,
    pub(crate) peek: Option<usize>,
    pub(crate) menu: Option<(usize, SessionMenu)>,
    pub(crate) project_menu: Option<(usize, ProjectMenu)>,
    pub(crate) card_shown: bool,
    pub(crate) archive_restore: Option<usize>,
    pub(crate) archive_delete: Option<usize>,
}

/// What a click on a row asked for.
enum RowClick {
    None,
    Open,
    Peek,
    Menu(SessionMenu),
}

/// One two-line row: dot, title, elapsed time / state or activity. Waiting rows get a Peek
/// button; a right click peeks at any row.
fn session_row(ui: &mut egui::Ui, row: &SessionRow) -> (RowClick, bool) {
    let height = 44.0;
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::click());
    // What screen readers announce for the row (painted by hand, so egui cannot infer it).
    let label = format!("{}, {}, {}", row.title, row.subtitle, row.elapsed);
    response
        .widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, row.focused, &label));
    let painter = ui.painter_at(rect);
    let bg = if row.focused {
        theme::SELECTED_BG()
    } else if response.hovered() {
        theme::HOVER_BG()
    } else {
        Color32::TRANSPARENT
    };
    painter.rect_filled(rect, CornerRadius::same(6), bg);

    let left = rect.left() + 12.0;
    painter.circle_filled(
        egui::pos2(left + 4.0, rect.top() + 15.0),
        4.0,
        state_color(row.state),
    );
    if row.unseen && !row.focused {
        painter.circle_filled(
            egui::pos2(rect.right() - 6.0, rect.top() + 6.0),
            3.0,
            theme::ACCENT(),
        );
    }

    let text_left = left + 16.0;
    let font = egui::FontId::proportional(13.0);
    let small = egui::FontId::proportional(12.0);
    let right = rect.right() - 10.0;
    let time = painter.text(
        egui::pos2(right, rect.top() + 8.0),
        egui::Align2::RIGHT_TOP,
        &row.elapsed,
        small.clone(),
        theme::TEXT_MUTED(),
    );
    let title_width = (time.left() - text_left - 8.0).max(20.0);
    let title = elide(ui, &row.title, &font, title_width);
    painter.text(
        egui::pos2(text_left, rect.top() + 7.0),
        egui::Align2::LEFT_TOP,
        title,
        font,
        theme::TEXT_PRIMARY(),
    );
    let subtitle_color = match row.state {
        RowState::Done => theme::STATE_DONE(),
        RowState::Waiting => theme::STATE_WAITING(),
        RowState::Error => theme::STATE_ERROR(),
        _ => theme::TEXT_SECONDARY(),
    };
    let subtitle = elide(ui, &row.subtitle, &small, right - text_left);
    painter.text(
        egui::pos2(text_left, rect.top() + 25.0),
        egui::Align2::LEFT_TOP,
        subtitle,
        small,
        subtitle_color,
    );
    let mut click = RowClick::None;
    if row.state == RowState::Waiting && !row.focused {
        let button_rect = egui::Rect::from_min_size(
            egui::pos2(rect.right() - 46.0, rect.top() + 24.0),
            Vec2::new(40.0, 16.0),
        );
        let peek = ui.interact(
            button_rect,
            ui.scope_id().with(("peek", &row.title, rect.top() as i32)),
            Sense::click(),
        );
        painter.rect_filled(button_rect, CornerRadius::same(4), theme::BORDER());
        painter.text(
            button_rect.center(),
            egui::Align2::CENTER_CENTER,
            strings::PEEK,
            egui::FontId::proportional(10.0),
            theme::TEXT_PRIMARY(),
        );
        if peek.clicked() {
            click = RowClick::Peek;
        }
    }
    if matches!(click, RowClick::None) && response.clicked() {
        click = RowClick::Open;
    }
    // Hover card (S-15).
    let card_shown = !row.card.is_empty() && response.hovered();
    let response = if row.card.is_empty() {
        response
    } else {
        response.on_hover_ui(|ui| {
            egui::Grid::new("session-card")
                .num_columns(2)
                .spacing([12.0, 4.0])
                .show(ui, |ui| {
                    for (label, value) in &row.card {
                        ui.label(RichText::new(label).size(12.0).color(theme::TEXT_MUTED()));
                        ui.label(RichText::new(value).size(12.0));
                        ui.end_row();
                    }
                });
        })
    };
    // Right click: the session menu (S-13).
    response.context_menu(|ui| {
        for (choice, label) in [
            (SessionMenu::Focus, strings::MENU_FOCUS),
            (SessionMenu::Rename, strings::MENU_RENAME),
            (SessionMenu::Restart, strings::MENU_RESTART),
            (SessionMenu::Peek, strings::MENU_PEEK),
            (SessionMenu::Archive, strings::MENU_ARCHIVE),
            (SessionMenu::CopyId, strings::MENU_COPY_ID),
            (SessionMenu::ShowFolder, strings::MENU_SHOW_FOLDER),
            (SessionMenu::OpenInEditor, strings::MENU_OPEN_IN_EDITOR),
            (SessionMenu::ShowTranscript, strings::MENU_SHOW_TRANSCRIPT),
            (SessionMenu::RemoveWorktree, strings::MENU_REMOVE_WORKTREE),
            (SessionMenu::Close, strings::MENU_CLOSE),
        ] {
            if choice == SessionMenu::RemoveWorktree && !row.worktree
                || choice == SessionMenu::ShowTranscript && !row.transcript
            {
                continue;
            }
            if ui.button(label).clicked() {
                click = RowClick::Menu(choice);
                ui.close();
            }
        }
    });
    (click, card_shown)
}
