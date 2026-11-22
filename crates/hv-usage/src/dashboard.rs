//! Everything the usage dashboard shows for a range of days (F5-06, docs/requirements.md §6).
//! Pure: records in, numbers out, with the formulas checked against the reference screenshot.

use std::collections::BTreeMap;

use jiff::Timestamp;
use jiff::civil::{Date, Weekday};
use jiff::tz::TimeZone;

use crate::{Task, UsageRecord};

/// One day of the "tokens by day" chart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Day {
    pub date: Date,
    /// All input (uncached + cache read + cache write).
    pub input: u64,
    pub output: u64,
    pub tasks: u64,
}

/// A row of the models table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelRow {
    pub provider: String,
    pub model: String,
    pub input: u64,
    pub output: u64,
}

/// A row of the agents table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRow {
    pub agent: String,
    pub input: u64,
    pub output: u64,
    pub tasks: u64,
}

/// The dashboard for one range.
#[derive(Debug, Clone, PartialEq)]
pub struct Dashboard {
    /// First and last day shown (the last one is today).
    pub first_day: Date,
    pub last_day: Date,
    /// INPUT TOKENS (U-10).
    pub input: u64,
    /// Share of input read from the cache, 0–1 (U-10).
    pub cache_share: f64,
    /// OUTPUT TOKENS (U-11).
    pub output: u64,
    /// Assistant turns.
    pub turns: u64,
    /// Output per turn (U-11).
    pub output_per_turn: u64,
    /// TASKS (U-12).
    pub tasks: u64,
    /// Change against the previous period of the same length (U-14); `None` without data or
    /// for "All".
    pub delta_input: Option<f64>,
    pub delta_output: Option<f64>,
    pub delta_tasks: Option<f64>,
    /// One entry per day of the range, oldest first (U-20).
    pub daily: Vec<Day>,
    /// Days with any usage, and (input + output) per such day (U-22).
    pub active_days: u32,
    pub per_active_day: u64,
    /// Tokens by local weekday (Monday first) and hour (U-30, U-33).
    pub heat: [[u64; 24]; 7],
    /// Busiest weekday and hour (U-32).
    pub most_active: Option<(Weekday, u8)>,
    /// Models by input, largest first (U-41).
    pub models: Vec<ModelRow>,
    /// Share of input per provider, largest first (U-40).
    pub providers: Vec<(String, f64)>,
    /// Agents by input, largest first (U-50).
    pub agents: Vec<AgentRow>,
}

/// A timestamp in local time: day, Monday-based weekday index and hour.
fn local(ts: &str, tz: &TimeZone) -> Option<(Date, usize, u8)> {
    let ts: Timestamp = ts.parse().ok()?;
    let zoned = ts.to_zoned_datetime(tz.clone());
    let weekday = zoned.weekday().to_monday_zero_offset() as usize;
    let hour = u8::try_from(zoned.hour()).ok()?;
    Some((zoned.date(), weekday, hour))
}

/// `(current - previous) / previous`, or `None` when there is nothing to compare with.
fn delta(current: u64, previous: u64) -> Option<f64> {
    (previous > 0).then(|| (current as f64 - previous as f64) / previous as f64)
}

/// Builds the dashboard for the last `days` days including `today` (`None` = everything).
pub fn build(
    records: &[UsageRecord],
    tasks: &[Task],
    days: Option<u32>,
    today: Date,
    tz: &TimeZone,
) -> Dashboard {
    let located: Vec<(&UsageRecord, (Date, usize, u8))> = records
        .iter()
        .filter_map(|r| local(&r.timestamp, tz).map(|l| (r, l)))
        .collect();
    let task_days: Vec<(&Task, Date)> = tasks
        .iter()
        .filter_map(|t| local(&t.timestamp, tz).map(|(d, ..)| (t, d)))
        .collect();
    let earliest = located
        .iter()
        .map(|(_, (d, ..))| *d)
        .chain(task_days.iter().map(|(_, d)| *d))
        .min()
        .unwrap_or(today);
    let length = match days {
        Some(n) => i64::from(n.max(1)),
        None => (today - earliest).get_days() as i64 + 1,
    };
    let first_day = today
        .checked_sub(jiff::Span::new().days(length - 1))
        .unwrap_or(today);
    let in_range = |d: Date| d >= first_day && d <= today;

    // Previous period of the same length, for the deltas.
    let previous = days.map(|_| {
        let end = first_day.yesterday().unwrap_or(first_day);
        let start = first_day
            .checked_sub(jiff::Span::new().days(length))
            .unwrap_or(first_day);
        (start, end)
    });
    let in_previous = |d: Date| previous.is_some_and(|(s, e)| d >= s && d <= e);

    let mut input = 0;
    let mut output = 0;
    let mut cache_read = 0;
    let mut turns = 0;
    let (mut prev_input, mut prev_output) = (0, 0);
    let mut by_day: BTreeMap<Date, Day> = BTreeMap::new();
    let mut heat = [[0u64; 24]; 7];
    let mut models: BTreeMap<(String, String), ModelRow> = BTreeMap::new();
    let mut agents: BTreeMap<String, AgentRow> = BTreeMap::new();
    for (r, (date, weekday, hour)) in &located {
        if in_previous(*date) {
            prev_input += r.total_input();
            prev_output += r.output_tokens;
        }
        if !in_range(*date) {
            continue;
        }
        input += r.total_input();
        output += r.output_tokens;
        cache_read += r.cache_read_tokens;
        turns += 1;
        let day = by_day.entry(*date).or_insert(Day {
            date: *date,
            input: 0,
            output: 0,
            tasks: 0,
        });
        day.input += r.total_input();
        day.output += r.output_tokens;
        heat[*weekday][usize::from(*hour)] += r.total_input() + r.output_tokens;
        let model = models
            .entry((r.provider.to_owned(), r.model.clone()))
            .or_insert_with(|| ModelRow {
                provider: r.provider.to_owned(),
                model: r.model.clone(),
                input: 0,
                output: 0,
            });
        model.input += r.total_input();
        model.output += r.output_tokens;
        let agent = agents
            .entry(r.agent.to_owned())
            .or_insert_with(|| AgentRow {
                agent: r.agent.to_owned(),
                input: 0,
                output: 0,
                tasks: 0,
            });
        agent.input += r.total_input();
        agent.output += r.output_tokens;
    }
    let mut task_count = 0;
    let mut prev_tasks = 0;
    for (t, date) in &task_days {
        if in_previous(*date) {
            prev_tasks += 1;
        }
        if !in_range(*date) {
            continue;
        }
        task_count += 1;
        by_day
            .entry(*date)
            .or_insert(Day {
                date: *date,
                input: 0,
                output: 0,
                tasks: 0,
            })
            .tasks += 1;
        agents
            .entry(t.agent.to_owned())
            .or_insert_with(|| AgentRow {
                agent: t.agent.to_owned(),
                input: 0,
                output: 0,
                tasks: 0,
            })
            .tasks += 1;
    }

    // Every day of the range, including empty ones.
    let mut daily = Vec::new();
    let mut d = first_day;
    while d <= today {
        daily.push(by_day.get(&d).copied().unwrap_or(Day {
            date: d,
            input: 0,
            output: 0,
            tasks: 0,
        }));
        let Ok(next) = d.tomorrow() else { break };
        d = next;
    }
    let active_days = daily.iter().filter(|d| d.input + d.output > 0).count() as u32;
    let per_active_day = (input + output)
        .checked_div(u64::from(active_days))
        .unwrap_or(0);

    let most_active = heat
        .iter()
        .enumerate()
        .flat_map(|(w, hours)| hours.iter().enumerate().map(move |(h, v)| (w, h, *v)))
        .filter(|(.., v)| *v > 0)
        .max_by_key(|(.., v)| *v)
        .map(|(w, h, _)| {
            (
                Weekday::from_monday_zero_offset(w as i8).unwrap_or(Weekday::Monday),
                h as u8,
            )
        });

    let mut models: Vec<ModelRow> = models.into_values().collect();
    models.sort_by(|a, b| b.input.cmp(&a.input).then(b.output.cmp(&a.output)));
    let mut providers: BTreeMap<String, u64> = BTreeMap::new();
    for m in &models {
        *providers.entry(m.provider.clone()).or_default() += m.input;
    }
    let mut providers: Vec<(String, f64)> = providers
        .into_iter()
        .map(|(p, i)| {
            (
                p,
                if input == 0 {
                    0.0
                } else {
                    i as f64 / input as f64
                },
            )
        })
        .collect();
    providers.sort_by(|a, b| b.1.total_cmp(&a.1));
    let mut agents: Vec<AgentRow> = agents.into_values().collect();
    agents.sort_by(|a, b| b.input.cmp(&a.input).then(b.tasks.cmp(&a.tasks)));

    Dashboard {
        first_day,
        last_day: today,
        input,
        cache_share: if input == 0 {
            0.0
        } else {
            cache_read as f64 / input as f64
        },
        output,
        turns,
        output_per_turn: output.checked_div(turns).unwrap_or(0),
        tasks: task_count,
        delta_input: previous.and_then(|_| delta(input, prev_input)),
        delta_output: previous.and_then(|_| delta(output, prev_output)),
        delta_tasks: previous.and_then(|_| delta(task_count, prev_tasks)),
        daily,
        active_days,
        per_active_day,
        heat,
        most_active,
        models,
        providers,
        agents,
    }
}

/// The 5 intensity levels of the heatmap (U-31): 0 for empty cells, 1–4 by quartiles of the
/// non-empty ones.
pub fn heat_levels(heat: &[[u64; 24]; 7]) -> [[u8; 24]; 7] {
    let mut values: Vec<u64> = heat.iter().flatten().copied().filter(|v| *v > 0).collect();
    values.sort_unstable();
    let quantile = |q: f64| {
        values
            .get(((values.len() as f64 - 1.0) * q).round() as usize)
            .copied()
            .unwrap_or(0)
    };
    let cuts = [quantile(0.25), quantile(0.5), quantile(0.75)];
    let mut out = [[0u8; 24]; 7];
    for (w, hours) in heat.iter().enumerate() {
        for (h, v) in hours.iter().enumerate() {
            out[w][h] = if *v == 0 {
                0
            } else {
                1 + cuts.iter().filter(|c| *v > **c).count() as u8
            };
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;

    fn rec(
        ts: &str,
        model: &str,
        provider: &'static str,
        input: u64,
        cache: u64,
        output: u64,
    ) -> UsageRecord {
        UsageRecord {
            agent: "claude-code",
            provider,
            model: model.into(),
            timestamp: ts.into(),
            session_id: None,
            project: None,
            input_tokens: input,
            output_tokens: output,
            cache_read_tokens: cache,
            cache_write_tokens: 0,
        }
    }

    fn task(ts: &str) -> Task {
        Task {
            agent: "claude-code",
            timestamp: ts.into(),
            session_id: None,
        }
    }

    #[test]
    fn thirty_days_include_today_and_kpis_follow_the_formulas() {
        let tz = TimeZone::UTC;
        let today = date(2026, 11, 21);
        let records = vec![
            rec(
                "2026-11-21T10:00:00Z",
                "claude-opus-5-5",
                "anthropic",
                100,
                900,
                50,
            ),
            rec(
                "2026-10-23T09:00:00Z",
                "claude-opus-5-5",
                "anthropic",
                10,
                90,
                10,
            ),
            rec(
                "2026-10-22T09:00:00Z",
                "claude-opus-5-5",
                "anthropic",
                1,
                0,
                1,
            ), // previous period
            rec("2026-11-20T23:00:00Z", "gpt-6", "openai", 100, 0, 40),
        ];
        let tasks = vec![task("2026-11-21T09:59:00Z"), task("2026-10-22T08:00:00Z")];
        let d = build(&records, &tasks, Some(30), today, &tz);
        assert_eq!(d.first_day, date(2026, 10, 23), "Oct 23 – today is 30 days");
        assert_eq!(d.daily.len(), 30);
        assert_eq!(d.input, 1000 + 100 + 100);
        assert_eq!(d.output, 100);
        assert_eq!(d.turns, 3);
        assert_eq!(d.output_per_turn, 33);
        assert!((d.cache_share - 990.0 / 1200.0).abs() < 1e-9);
        assert_eq!(d.tasks, 1);
        assert_eq!(d.active_days, 3);
        assert_eq!(d.per_active_day, 1300 / 3);
        // Previous 30 days held 1 input token and 1 task.
        assert_eq!(d.delta_input, Some(1199.0));
        assert_eq!(d.delta_tasks, Some(0.0));
        // Provider share by input tokens (U-40).
        assert_eq!(d.providers[0].0, "anthropic");
        assert!((d.providers[0].1 - 1100.0 / 1200.0).abs() < 1e-9);
        // Rows add up to the totals (U-43).
        assert_eq!(d.models.iter().map(|m| m.output).sum::<u64>(), d.output);
        assert_eq!(d.most_active, Some((Weekday::Saturday, 10)));
    }

    #[test]
    fn all_starts_at_the_first_record_and_has_no_deltas() {
        let tz = TimeZone::UTC;
        let records = vec![rec("2026-11-01T10:00:00Z", "m", "anthropic", 1, 0, 1)];
        let d = build(&records, &[], None, date(2026, 11, 3), &tz);
        assert_eq!(d.first_day, date(2026, 11, 1));
        assert_eq!(d.daily.len(), 3);
        assert_eq!(d.delta_input, None);
    }

    #[test]
    fn heat_levels_use_quartiles() {
        let mut heat = [[0u64; 24]; 7];
        for (i, v) in [1u64, 2, 3, 4, 100].iter().enumerate() {
            heat[0][i] = *v;
        }
        let levels = heat_levels(&heat);
        assert_eq!(&levels[0][..6], &[1, 1, 2, 3, 4, 0]);
    }
}
