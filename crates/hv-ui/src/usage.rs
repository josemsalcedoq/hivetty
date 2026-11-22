//! The "Agent usage" dashboard (F5-07…F5-12, docs/ui-spec.md §6) drawn over the terminals.

use egui::{Color32, CornerRadius, Margin, Rect, RichText, Sense, Stroke, Vec2, pos2};
use hv_usage::compact;
use hv_usage::dashboard::{Dashboard, heat_levels};

use crate::{strings, theme};

/// The ranges of the selector (U-02).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsageRange {
    Week,
    Month,
    Quarter,
    All,
}

impl UsageRange {
    /// Days in the range (`None` = everything).
    pub fn days(self) -> Option<u32> {
        match self {
            Self::Week => Some(7),
            Self::Month => Some(30),
            Self::Quarter => Some(90),
            Self::All => None,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Week => strings::RANGE_7,
            Self::Month => strings::RANGE_30,
            Self::Quarter => strings::RANGE_90,
            Self::All => strings::RANGE_ALL,
        }
    }
}

/// What the dashboard shows this frame.
#[derive(Debug, Clone)]
pub enum UsageView {
    /// Logs are being read.
    Loading(UsageRange),
    /// Ready; the last field is the agent the panel is filtered to (display name).
    Ready(UsageRange, std::sync::Arc<Dashboard>, Option<String>),
}

/// What the user did in the dashboard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UsageAction {
    Range(UsageRange),
    /// Show only this agent (id), or everything again with `None` (U-51).
    FilterAgent(Option<String>),
    Close,
}

const INPUT_COLOR: Color32 = Color32::from_rgb(0x5A, 0x5A, 0x62);
const OUTPUT_COLOR: Color32 = Color32::from_rgb(0x4E, 0xA1, 0xFF);
const HEAT: [Color32; 5] = [
    Color32::from_rgb(0x1A, 0x1A, 0x1D),
    Color32::from_rgb(0x4A, 0x4A, 0x50),
    Color32::from_rgb(0x80, 0x80, 0x86),
    Color32::from_rgb(0xB4, 0xB4, 0xB8),
    Color32::from_rgb(0xE6, 0xE6, 0xE6),
];

/// Draws the dashboard filling `area` (points). Returns what the user did.
pub(crate) fn show(
    ctx: &egui::Context,
    area: Rect,
    view: &UsageView,
    limits: &[String],
) -> Option<UsageAction> {
    let mut action = None;
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        return Some(UsageAction::Close);
    }
    let range = match view {
        UsageView::Loading(r) | UsageView::Ready(r, ..) => *r,
    };
    egui::Area::new(egui::Id::unique("usage-dashboard"))
        .order(egui::Order::Middle)
        .fade_in(false)
        .fixed_pos(area.min)
        .show(ctx, |ui| {
            ui.set_min_size(area.size());
            ui.set_max_size(area.size());
            ui.painter()
                .rect_filled(area, CornerRadius::ZERO, theme::WINDOW_BG());
            egui::Frame::new()
                .inner_margin(Margin::same(20))
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            if let Some(a) = header(ui, view, range, limits) {
                                action = Some(a);
                            }
                            ui.add_space(16.0);
                            match view {
                                UsageView::Loading(_) => {
                                    ui.label(
                                        RichText::new(strings::USAGE_LOADING)
                                            .color(theme::TEXT_MUTED()),
                                    );
                                }
                                UsageView::Ready(_, dash, _)
                                    if dash.input == 0 && dash.tasks == 0 =>
                                {
                                    ui.label(
                                        RichText::new(strings::USAGE_EMPTY)
                                            .color(theme::TEXT_SECONDARY()),
                                    );
                                }
                                UsageView::Ready(_, dash, _) => {
                                    if let Some(a) = body(ui, dash) {
                                        action = Some(a);
                                    }
                                }
                            }
                        });
                });
        });
    action
}

fn header(
    ui: &mut egui::Ui,
    view: &UsageView,
    range: UsageRange,
    limits: &[String],
) -> Option<UsageAction> {
    let mut action = None;
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(RichText::new(strings::USAGE_TITLE).size(20.0).strong());
            if let UsageView::Ready(_, dash, filter) = view {
                let first = dash.first_day.strftime("%b %-d").to_string();
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("{first} – {}", strings::TODAY))
                            .size(13.0)
                            .color(theme::TEXT_MUTED()),
                    );
                    if let Some(agent) = filter {
                        let chip =
                            egui::Button::new(RichText::new(format!("{agent}  ×")).size(12.0))
                                .corner_radius(CornerRadius::same(10));
                        if ui
                            .add(chip)
                            .on_hover_text(strings::SHOW_ALL_AGENTS)
                            .clicked()
                        {
                            action = Some(UsageAction::FilterAgent(None));
                        }
                    }
                });
            }
            for limit in limits {
                ui.label(
                    RichText::new(limit)
                        .size(12.0)
                        .color(theme::TEXT_SECONDARY()),
                );
            }
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button(strings::ESC).clicked() {
                action = Some(UsageAction::Close);
            }
            ui.add_space(8.0);
            for r in [
                UsageRange::All,
                UsageRange::Quarter,
                UsageRange::Month,
                UsageRange::Week,
            ] {
                let selected = r == range;
                let text = RichText::new(r.label()).size(12.0).color(if selected {
                    theme::TEXT_PRIMARY()
                } else {
                    theme::TEXT_SECONDARY()
                });
                let button = egui::Button::new(text)
                    .fill(if selected {
                        theme::SELECTED_BG()
                    } else {
                        Color32::TRANSPARENT
                    })
                    .corner_radius(CornerRadius::same(6));
                if ui.add(button).clicked() && !selected {
                    action = Some(UsageAction::Range(r));
                }
            }
        });
    });
    action
}

fn body(ui: &mut egui::Ui, d: &Dashboard) -> Option<UsageAction> {
    let mut action = None;
    let width = ui.available_width();
    let narrow = width < 900.0;
    let gap = 12.0;
    // Row 1: KPI cards.
    let cards = [
        (
            strings::KPI_INPUT,
            compact(d.input),
            match d.cost {
                Some((usd, share)) if share > 0.995 => format!(
                    "{:.0}% {} · {}",
                    d.cache_share * 100.0,
                    strings::FROM_CACHE,
                    strings::api_equivalent(usd)
                ),
                Some((usd, _)) => format!(
                    "{:.0}% {} · {} ({})",
                    d.cache_share * 100.0,
                    strings::FROM_CACHE,
                    strings::api_equivalent(usd),
                    strings::PARTIAL
                ),
                None => format!("{:.0}% {}", d.cache_share * 100.0, strings::FROM_CACHE),
            },
            d.delta_input,
            d.daily.iter().map(|x| x.input).collect::<Vec<_>>(),
        ),
        (
            strings::KPI_OUTPUT,
            compact(d.output),
            if d.reasoning > 0 {
                format!(
                    "{} {} · {} {}",
                    compact(d.output_per_turn),
                    strings::PER_TURN,
                    compact(d.reasoning),
                    strings::REASONING
                )
            } else {
                format!("{} {}", compact(d.output_per_turn), strings::PER_TURN)
            },
            d.delta_output,
            d.daily.iter().map(|x| x.output).collect(),
        ),
        (
            strings::KPI_TASKS,
            group_thousands(d.tasks),
            format!("{} {}", group_thousands(d.turns), strings::TURNS),
            d.delta_tasks,
            d.daily.iter().map(|x| x.tasks).collect(),
        ),
        (
            strings::KPI_AGENT_TIME,
            duration_text(d.agent_seconds),
            match d.observed_seconds {
                Some(seen) => format!(
                    "{} {} {} · {}",
                    strings::UP_TO,
                    d.max_concurrency,
                    strings::AT_ONCE,
                    strings::observed(&duration_text(seen))
                ),
                None => format!(
                    "{} {} {}",
                    strings::UP_TO,
                    d.max_concurrency,
                    strings::AT_ONCE
                ),
            },
            None,
            d.daily.iter().map(|x| x.seconds).collect(),
        ),
    ];
    let per_row = if narrow { 2 } else { 4 };
    let card_w = (width - gap * (per_row as f32 - 1.0)) / per_row as f32;
    for chunk in cards.chunks(per_row) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for (label, value, sub, delta, spark) in chunk {
                card(ui, card_w, 130.0, |ui| {
                    kpi(ui, label, value, sub, *delta, spark);
                });
            }
        });
        ui.add_space(gap);
    }
    // Row 2: tokens by day + heatmap.
    let (w1, w2) = if narrow {
        (width, width)
    } else {
        ((width - gap) * 7.0 / 12.0, (width - gap) * 5.0 / 12.0)
    };
    let row2 = |ui: &mut egui::Ui| {
        card(ui, w1, 230.0, |ui| tokens_by_day(ui, d));
        card(ui, w2, 230.0, |ui| heatmap(ui, d));
    };
    if narrow {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = gap;
            row2(ui);
        });
    } else {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            row2(ui);
        });
    }
    ui.add_space(gap);
    // Row 3: models + agents.
    let half = if narrow { width } else { (width - gap) / 2.0 };
    let mut row3 = |ui: &mut egui::Ui| {
        card(ui, half, 0.0, |ui| models(ui, d));
        card(ui, half, 0.0, |ui| {
            if let Some(id) = agents(ui, d) {
                action = Some(UsageAction::FilterAgent(Some(id)));
            }
        });
    };
    if narrow {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = gap;
            row3(ui);
        });
    } else {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            row3(ui);
        });
    }
    action
}

fn card(ui: &mut egui::Ui, width: f32, min_height: f32, content: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(theme::ELEVATED_BG())
        .stroke(Stroke::new(1.0, theme::BORDER()))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(14))
        .show(ui, |ui| {
            ui.set_width(width - 28.0);
            ui.set_min_height((min_height - 28.0).max(0.0));
            ui.vertical(content);
        });
}

fn section_label(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).size(12.0).color(theme::TEXT_MUTED()));
}

fn kpi(ui: &mut egui::Ui, label: &str, value: &str, sub: &str, delta: Option<f64>, spark: &[u64]) {
    ui.horizontal(|ui| {
        section_label(ui, label);
        if let Some(delta) = delta {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let text = delta_text(delta);
                let color = if delta >= 0.0 {
                    theme::STATE_DONE()
                } else {
                    theme::STATE_ERROR()
                };
                egui::Frame::new()
                    .fill(color.gamma_multiply(0.15))
                    .corner_radius(CornerRadius::same(8))
                    .inner_margin(Margin::symmetric(6, 1))
                    .show(ui, |ui| {
                        ui.label(RichText::new(text).size(11.0).color(color));
                    });
            });
        }
    });
    ui.label(RichText::new(value).size(32.0).color(theme::TEXT_PRIMARY()));
    ui.label(RichText::new(sub).size(13.0).color(theme::TEXT_SECONDARY()));
    sparkline(ui, spark);
}

/// "+42%", capped at "+999%" (U-14).
pub(crate) fn delta_text(delta: f64) -> String {
    let pct = (delta * 100.0).round();
    if pct > 999.0 {
        "+999%".to_owned()
    } else if pct >= 0.0 {
        format!("+{pct:.0}%")
    } else {
        format!("{pct:.0}%")
    }
}

/// Agent time the dashboard way: "347 h", "12 min", "40 s".
pub(crate) fn duration_text(seconds: u64) -> String {
    match seconds {
        0..60 => format!("{seconds} s"),
        60..3600 => format!("{} min", seconds / 60),
        _ => {
            let hours = seconds as f64 / 3600.0;
            if hours < 10.0 {
                format!("{hours:.1} h")
            } else {
                format!("{hours:.0} h")
            }
        }
    }
}

/// `1,110`.
pub(crate) fn group_thousands(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn sparkline(ui: &mut egui::Ui, values: &[u64]) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 24.0), Sense::hover());
    let max = values.iter().copied().max().unwrap_or(0);
    if values.len() < 2 || max == 0 {
        return;
    }
    let points: Vec<egui::Pos2> = values
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let x = rect.left() + rect.width() * i as f32 / (values.len() - 1) as f32;
            let y = rect.bottom() - rect.height() * (*v as f32 / max as f32);
            pos2(x, y)
        })
        .collect();
    ui.painter().add(egui::Shape::line(
        points,
        Stroke::new(1.5, theme::TEXT_SECONDARY()),
    ));
}

fn tokens_by_day(ui: &mut egui::Ui, d: &Dashboard) {
    section_label(ui, strings::TOKENS_BY_DAY);
    ui.add_space(4.0);
    let max_in = d.daily.iter().map(|x| x.input).max().unwrap_or(0).max(1);
    let max_out = d.daily.iter().map(|x| x.output).max().unwrap_or(0).max(1);
    ui.label(
        RichText::new(format!("{} {}", compact(max_in), strings::INPUT))
            .size(11.0)
            .color(theme::TEXT_MUTED()),
    );
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 130.0), Sense::hover());
    let painter = ui.painter_at(rect);
    // Input grows up from the middle line, output down; each has its own scale (U-20).
    let mid = rect.top() + rect.height() * 0.62;
    let n = d.daily.len().max(1);
    let slot = rect.width() / n as f32;
    let bar = (slot * 0.7).max(1.0);
    for (i, day) in d.daily.iter().enumerate() {
        let x = rect.left() + slot * i as f32 + (slot - bar) / 2.0;
        let up = (mid - rect.top()) * day.input as f32 / max_in as f32;
        let down = (rect.bottom() - mid) * day.output as f32 / max_out as f32;
        if day.input > 0 {
            painter.rect_filled(
                Rect::from_min_max(pos2(x, mid - up.max(1.0)), pos2(x + bar, mid)),
                CornerRadius::same(1),
                INPUT_COLOR,
            );
        }
        if day.output > 0 {
            painter.rect_filled(
                Rect::from_min_max(pos2(x, mid), pos2(x + bar, mid + down.max(1.0))),
                CornerRadius::same(1),
                OUTPUT_COLOR,
            );
        }
    }
    painter.line_segment(
        [pos2(rect.left(), mid), pos2(rect.right(), mid)],
        Stroke::new(1.0, theme::BORDER()),
    );
    // Tooltip with the exact values of the day under the pointer (U-23).
    if let Some(pos) = response.hover_pos() {
        let i = (((pos.x - rect.left()) / slot) as usize).min(n - 1);
        if let Some(day) = d.daily.get(i) {
            response.on_hover_ui_at_pointer(|ui| {
                ui.label(day.date.strftime("%a %b %-d").to_string());
                ui.label(format!("{} {}", group_thousands(day.input), strings::INPUT));
                ui.label(format!(
                    "{} {}",
                    group_thousands(day.output),
                    strings::OUTPUT
                ));
            });
        }
    }
    ui.label(
        RichText::new(format!("{} {}", compact(max_out), strings::OUTPUT))
            .size(11.0)
            .color(theme::TEXT_MUTED()),
    );
    // X axis: first day, middle day, today (U-21).
    ui.horizontal(|ui| {
        let first = d.first_day.strftime("%b %-d").to_string();
        let middle = d
            .daily
            .get(d.daily.len() / 2)
            .map(|x| x.date.strftime("%b %-d").to_string())
            .unwrap_or_default();
        ui.label(RichText::new(first).size(11.0).color(theme::TEXT_MUTED()));
        let w = ui.available_width();
        ui.add_space(w / 2.0 - 40.0);
        ui.label(RichText::new(middle).size(11.0).color(theme::TEXT_MUTED()));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                RichText::new(strings::TODAY)
                    .size(11.0)
                    .color(theme::TEXT_MUTED()),
            );
        });
    });
    // "A of N days with agents · X/day" (U-22).
    ui.label(
        RichText::new(format!(
            "{} of {} {} · {}/{}",
            d.active_days,
            d.daily.len(),
            strings::DAYS_WITH_AGENTS,
            compact(d.per_active_day),
            strings::DAY
        ))
        .size(12.0)
        .color(theme::TEXT_SECONDARY()),
    );
}

fn heatmap(ui: &mut egui::Ui, d: &Dashboard) {
    section_label(ui, strings::WHEN_THEY_WORK);
    ui.add_space(6.0);
    let levels = heat_levels(&d.heat);
    let label_w = 30.0;
    let avail = ui.available_width() - label_w;
    let cell = (avail / 24.0).clamp(4.0, 16.0);
    let (rect, _) =
        ui.allocate_exact_size(Vec2::new(label_w + cell * 24.0, cell * 7.0), Sense::hover());
    let painter = ui.painter_at(rect);
    for (w, name) in strings::WEEKDAYS.iter().enumerate() {
        let y = rect.top() + cell * w as f32;
        painter.text(
            pos2(rect.left(), y + cell / 2.0),
            egui::Align2::LEFT_CENTER,
            name,
            egui::FontId::proportional(10.0),
            theme::TEXT_MUTED(),
        );
        for h in 0..24 {
            let x = rect.left() + label_w + cell * h as f32;
            painter.rect_filled(
                Rect::from_min_size(pos2(x + 1.0, y + 1.0), Vec2::splat(cell - 2.0)),
                CornerRadius::same(2),
                HEAT[usize::from(levels[w][h])],
            );
        }
    }
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(strings::LESS)
                .size(11.0)
                .color(theme::TEXT_MUTED()),
        );
        for color in HEAT {
            let (r, _) = ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
            ui.painter().rect_filled(r, CornerRadius::same(2), color);
        }
        ui.label(
            RichText::new(strings::MORE)
                .size(11.0)
                .color(theme::TEXT_MUTED()),
        );
    });
    if let Some((day, hour)) = d.most_active {
        let name = strings::WEEKDAYS[day.to_monday_zero_offset() as usize];
        ui.label(
            RichText::new(format!("{} {name} {hour} h", strings::MOST_ACTIVE))
                .size(12.0)
                .color(theme::TEXT_SECONDARY()),
        );
    }
}

fn provider_color(provider: &str) -> Color32 {
    match provider {
        "anthropic" => Color32::from_rgb(0xD9, 0x77, 0x57),
        "openai" => Color32::from_rgb(0x74, 0xAA, 0x9C),
        "google" => Color32::from_rgb(0x4E, 0xA1, 0xFF),
        "xai" => Color32::from_rgb(0xC9, 0xC9, 0xCC),
        _ => Color32::from_rgb(0x6B, 0x6B, 0x73),
    }
}

fn models(ui: &mut egui::Ui, d: &Dashboard) {
    section_label(ui, &format!("{} · {}", strings::MODELS, d.models.len()));
    ui.add_space(6.0);
    // Stacked bar by provider, share of input (U-40).
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 8.0), Sense::hover());
    let mut x = rect.left();
    for (provider, share) in &d.providers {
        let w = rect.width() * *share as f32;
        ui.painter().rect_filled(
            Rect::from_min_size(pos2(x, rect.top()), Vec2::new(w, rect.height())),
            CornerRadius::ZERO,
            provider_color(provider),
        );
        x += w;
    }
    ui.horizontal_wrapped(|ui| {
        for (provider, share) in &d.providers {
            let (r, _) = ui.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
            ui.painter()
                .circle_filled(r.center(), 4.0, provider_color(provider));
            ui.label(
                RichText::new(format!("{provider} {:.0}%", share * 100.0))
                    .size(11.0)
                    .color(theme::TEXT_SECONDARY()),
            );
        }
    });
    ui.add_space(6.0);
    let max = d.models.first().map_or(1, |m| m.input.max(1));
    let shown = 6;
    for m in d.models.iter().take(shown) {
        table_row(
            ui,
            provider_color(&m.provider),
            &m.model,
            m.input,
            max,
            m.output,
            None,
        );
    }
    if d.models.len() > shown {
        egui::CollapsingHeader::new(
            RichText::new(format!("{} {}", d.models.len() - shown, strings::MORE_ROWS))
                .size(12.0)
                .color(theme::TEXT_MUTED()),
        )
        .id_salt("more-models")
        .show(ui, |ui| {
            for m in d.models.iter().skip(shown) {
                table_row(
                    ui,
                    provider_color(&m.provider),
                    &m.model,
                    m.input,
                    max,
                    m.output,
                    None,
                );
            }
        });
    }
}

/// The agents table; returns the id of a clicked row (U-51).
fn agents(ui: &mut egui::Ui, d: &Dashboard) -> Option<String> {
    let mut clicked = None;
    ui.horizontal(|ui| {
        section_label(ui, &format!("{} · {}", strings::AGENTS, d.agents.len()));
        ui.label(
            RichText::new(strings::CLICK_TO_FILTER)
                .size(11.0)
                .color(theme::TEXT_MUTED()),
        );
    });
    ui.add_space(6.0);
    let max = d.agents.first().map_or(1, |a| a.input.max(1));
    for a in &d.agents {
        let top = ui.cursor().top();
        table_row(
            ui,
            theme::TEXT_SECONDARY(),
            &a.agent,
            a.input,
            max,
            a.output,
            Some(duration_text(a.seconds)),
        );
        let row = Rect::from_min_max(
            pos2(ui.min_rect().left(), top),
            pos2(ui.min_rect().right(), ui.cursor().top()),
        );
        let response = ui.interact(
            row,
            ui.scope_id().with(("agent-row", &a.id)),
            Sense::click(),
        );
        if response.hovered() {
            ui.painter().rect_filled(
                row,
                CornerRadius::same(4),
                theme::HOVER_BG().gamma_multiply(0.5),
            );
        }
        if response.clicked() {
            clicked = Some(a.id.clone());
        }
    }
    clicked
}

/// Name, proportional bar (at least one point wide), input, output and optionally tasks.
fn table_row(
    ui: &mut egui::Ui,
    dot: Color32,
    name: &str,
    input: u64,
    max: u64,
    output: u64,
    extra: Option<String>,
) {
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
        ui.painter().circle_filled(r.center(), 3.5, dot);
        let (name_rect, _) = ui.allocate_exact_size(Vec2::new(150.0, 18.0), Sense::hover());
        let font = egui::FontId::proportional(12.0);
        let shown = crate::elide(ui, name, &font, name_rect.width());
        ui.painter().text(
            name_rect.left_center(),
            egui::Align2::LEFT_CENTER,
            shown,
            font,
            theme::TEXT_PRIMARY(),
        );
        let numbers = if extra.is_some() { 180.0 } else { 120.0 };
        let bar_w = (ui.available_width() - numbers).max(20.0);
        let (bar_rect, _) = ui.allocate_exact_size(Vec2::new(bar_w, 18.0), Sense::hover());
        let w = (bar_w * input as f32 / max as f32).max(1.0);
        ui.painter().rect_filled(
            Rect::from_min_size(
                pos2(bar_rect.left(), bar_rect.center().y - 3.0),
                Vec2::new(w, 6.0),
            ),
            CornerRadius::same(3),
            INPUT_COLOR,
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if let Some(text) = &extra {
                ui.label(
                    RichText::new(text)
                        .size(12.0)
                        .color(theme::TEXT_SECONDARY()),
                );
            }
            ui.label(
                RichText::new(compact(output))
                    .size(12.0)
                    .color(OUTPUT_COLOR),
            );
            ui.label(
                RichText::new(compact(input))
                    .size(12.0)
                    .color(theme::TEXT_SECONDARY()),
            );
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_read_like_the_reference() {
        assert_eq!(delta_text(0.42), "+42%");
        assert_eq!(delta_text(25.0), "+999%");
        assert_eq!(delta_text(-0.3), "-30%");
        assert_eq!(group_thousands(1110), "1,110");
        assert_eq!(group_thousands(2_674_000), "2,674,000");
        assert_eq!(group_thousands(12), "12");
        assert_eq!(duration_text(40), "40 s");
        assert_eq!(duration_text(720), "12 min");
        assert_eq!(duration_text(5400), "1.5 h");
        assert_eq!(duration_text(347 * 3600), "347 h");
    }
}
