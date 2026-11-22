//! `hivetty usage`: token usage per day from the agents' logs, as a table, CSV or JSON, with
//! optional project and range filters (F5-14, U-61, U-62).

use std::fmt::Write as _;

/// Output format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Format {
    Table,
    Csv,
    Json,
}

/// Options of `hivetty usage`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UsageArgs {
    pub(crate) format: Format,
    /// Keep only sessions whose directory name contains this (case-insensitive).
    pub(crate) project: Option<String>,
    /// Last N days including today.
    pub(crate) days: Option<u32>,
}

impl UsageArgs {
    pub(crate) fn parse(args: &[String]) -> Result<Self, String> {
        let mut out = Self {
            format: Format::Table,
            project: None,
            days: None,
        };
        let mut it = args.iter();
        while let Some(arg) = it.next() {
            match arg.as_str() {
                "--json" => out.format = Format::Json,
                "--csv" => out.format = Format::Csv,
                "--project" => {
                    out.project = Some(it.next().ok_or("--project needs a name")?.clone());
                }
                "--days" => {
                    let n = it.next().ok_or("--days needs a number")?;
                    out.days = Some(n.parse().map_err(|_| format!("invalid --days '{n}'"))?);
                }
                other => return Err(format!("unknown usage option '{other}'")),
            }
        }
        Ok(out)
    }
}

/// One day of the report.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Row {
    date: String,
    input: u64,
    output: u64,
    cache_read: u64,
    cache_write: u64,
    turns: u64,
    tasks: u64,
}

pub(crate) fn run(args: &UsageArgs) -> String {
    let data = crate::app::UsageData::read();
    let (records, tasks) = data.parts();
    if records.is_empty() && tasks.is_empty() {
        return "No usage found. hivetty reads Claude Code's transcripts (~/.claude/projects).\n"
            .to_owned();
    }
    report(records, tasks, args, today())
}

fn today() -> jiff::civil::Date {
    jiff::Timestamp::now()
        .to_zoned_datetime(jiff::tz::TimeZone::system())
        .date()
}

fn matches_project(project: Option<&str>, wanted: &str) -> bool {
    project.is_some_and(|p| {
        let name = p.rsplit(['/', '\\']).next().unwrap_or(p);
        name.to_lowercase().contains(&wanted.to_lowercase())
    })
}

fn report(
    records: &[hv_usage::UsageRecord],
    tasks: &[hv_usage::Task],
    args: &UsageArgs,
    today: jiff::civil::Date,
) -> String {
    let first = args.days.map(|d| {
        today
            .checked_sub(jiff::Span::new().days(i64::from(d.max(1)) - 1))
            .unwrap_or(today)
    });
    let keep_day = |day: jiff::civil::Date| first.is_none_or(|f| day >= f) && day <= today;
    let records: Vec<_> = records
        .iter()
        .filter(|r| {
            args.project
                .as_deref()
                .is_none_or(|p| matches_project(r.project.as_deref(), p))
        })
        .filter(|r| r.local_day().is_some_and(keep_day))
        .cloned()
        .collect();
    // Tasks carry no directory: with a project filter, keep those of the matching sessions.
    let sessions: std::collections::HashSet<&str> = records
        .iter()
        .filter_map(|r| r.session_id.as_deref())
        .collect();
    let mut task_days: std::collections::BTreeMap<jiff::civil::Date, u64> = Default::default();
    for t in tasks {
        let Some(day) = t
            .timestamp
            .parse::<jiff::Timestamp>()
            .ok()
            .map(|ts| ts.to_zoned_datetime(jiff::tz::TimeZone::system()).date())
        else {
            continue;
        };
        let in_project = args.project.is_none()
            || t.session_id
                .as_deref()
                .is_some_and(|s| sessions.contains(s));
        if keep_day(day) && in_project {
            *task_days.entry(day).or_default() += 1;
        }
    }
    let daily = hv_usage::daily(&records);
    let mut days: std::collections::BTreeSet<jiff::civil::Date> = daily.keys().copied().collect();
    days.extend(task_days.keys().copied());
    let rows: Vec<Row> = days
        .into_iter()
        .map(|day| {
            let t = daily.get(&day).copied().unwrap_or_default();
            Row {
                date: day.to_string(),
                input: t.input,
                output: t.output,
                cache_read: t.cache_read,
                cache_write: t.cache_write,
                turns: t.turns,
                tasks: task_days.get(&day).copied().unwrap_or(0),
            }
        })
        .collect();
    match args.format {
        Format::Csv => csv(&rows),
        Format::Json => json(&rows),
        Format::Table => table(&rows),
    }
}

fn csv(rows: &[Row]) -> String {
    let mut out = String::from("date,input,output,cache_read,cache_write,turns,tasks\n");
    for r in rows {
        let _ = writeln!(
            out,
            "{},{},{},{},{},{},{}",
            r.date, r.input, r.output, r.cache_read, r.cache_write, r.turns, r.tasks
        );
    }
    out
}

fn json(rows: &[Row]) -> String {
    let daily: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "date": r.date,
                "input": r.input,
                "output": r.output,
                "cacheRead": r.cache_read,
                "cacheWrite": r.cache_write,
                "turns": r.turns,
                "tasks": r.tasks,
            })
        })
        .collect();
    serde_json::json!({ "daily": daily }).to_string() + "\n"
}

fn table(rows: &[Row]) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{:<12}{:>14}{:>12}{:>14}{:>14}{:>8}{:>8}",
        "Date", "Input", "Output", "Cache read", "Cache write", "Turns", "Tasks"
    );
    let mut total = Row {
        date: "Total".to_owned(),
        input: 0,
        output: 0,
        cache_read: 0,
        cache_write: 0,
        turns: 0,
        tasks: 0,
    };
    for r in rows {
        let _ = writeln!(
            out,
            "{:<12}{:>14}{:>12}{:>14}{:>14}{:>8}{:>8}",
            r.date, r.input, r.output, r.cache_read, r.cache_write, r.turns, r.tasks
        );
        total.input += r.input;
        total.output += r.output;
        total.cache_read += r.cache_read;
        total.cache_write += r.cache_write;
        total.turns += r.turns;
        total.tasks += r.tasks;
    }
    let _ = writeln!(
        out,
        "{:<12}{:>14}{:>12}{:>14}{:>14}{:>8}{:>8}",
        total.date,
        total.input,
        total.output,
        total.cache_read,
        total.cache_write,
        total.turns,
        total.tasks
    );
    let all_input = total.input + total.cache_read + total.cache_write;
    let _ = writeln!(
        out,
        "\nInput {} ({:.0}% from cache) · Output {} · {} tasks",
        hv_usage::compact(all_input),
        if all_input == 0 {
            0.0
        } else {
            total.cache_read as f64 * 100.0 / all_input as f64
        },
        hv_usage::compact(total.output),
        total.tasks
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;

    fn rec(ts: &str, project: &str, session: &str) -> hv_usage::UsageRecord {
        hv_usage::UsageRecord {
            agent: "claude-code",
            provider: "anthropic",
            model: "m".into(),
            timestamp: ts.into(),
            session_id: Some(session.into()),
            project: Some(project.into()),
            input_tokens: 10,
            output_tokens: 5,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
        }
    }

    #[test]
    fn options_parse() {
        let args = |s: &str| s.split_whitespace().map(String::from).collect::<Vec<_>>();
        let a = UsageArgs::parse(&args("--csv --project hivetty --days 7")).unwrap();
        assert_eq!(a.format, Format::Csv);
        assert_eq!(a.project.as_deref(), Some("hivetty"));
        assert_eq!(a.days, Some(7));
        assert!(UsageArgs::parse(&args("--days x")).is_err());
        assert!(UsageArgs::parse(&args("--bogus")).is_err());
    }

    #[test]
    fn csv_filters_by_project_and_days() {
        let records = vec![
            rec("2026-11-21T12:00:00Z", "/home/me/hivetty", "a"),
            rec("2026-11-21T12:00:00Z", "/home/me/other", "b"),
            rec("2026-10-01T12:00:00Z", "/home/me/hivetty", "a"),
        ];
        let tasks = vec![
            hv_usage::Task {
                agent: "claude-code",
                timestamp: "2026-11-21T11:59:00Z".into(),
                session_id: Some("a".into()),
            },
            hv_usage::Task {
                agent: "claude-code",
                timestamp: "2026-11-21T11:59:00Z".into(),
                session_id: Some("b".into()),
            },
        ];
        let args = UsageArgs {
            format: Format::Csv,
            project: Some("HIVETTY".into()),
            days: Some(7),
        };
        let out = report(&records, &tasks, &args, date(2026, 11, 22));
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 2, "{out}");
        assert!(lines[1].ends_with(",10,5,0,0,1,1"), "{out}");
    }
}
