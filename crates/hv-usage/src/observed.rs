//! Agent time measured by hivetty itself (F4-23, A-21): every stretch a pane's agent spent
//! `Working`, appended to `agent-time.jsonl` in the data directory. It complements the time
//! estimated from the agents' logs (ADR 0005) with what the terminal actually saw.

use std::io::Write;
use std::path::Path;

use jiff::civil::Date;
use jiff::tz::TimeZone;
use serde::{Deserialize, Serialize};

/// Largest file read (about 200 000 intervals).
const MAX_BYTES: u64 = 16 << 20;

/// One stretch of work, in Unix seconds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Interval {
    /// Agent profile id ("claude-code").
    pub agent: String,
    pub start: i64,
    pub end: i64,
}

/// Appends one interval. Blocking: call off the UI thread.
pub fn append(path: &Path, interval: &Interval) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut line = serde_json::to_string(interval).map_err(std::io::Error::other)?;
    line.push('\n');
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?
        .write_all(line.as_bytes())
}

/// Reads every interval; bad lines and an oversized file are skipped.
pub fn load(path: &Path) -> Vec<Interval> {
    if std::fs::metadata(path).map_or(true, |m| m.len() > MAX_BYTES) {
        return Vec::new();
    }
    std::fs::read_to_string(path)
        .map(|text| {
            text.lines()
                .filter_map(|l| serde_json::from_str::<Interval>(l).ok())
                .filter(|i| i.end > i.start)
                .collect()
        })
        .unwrap_or_default()
}

/// Seconds with at least one agent working between the start of `first` and the end of `last`
/// (local days), optionally for one agent. Overlapping intervals count once.
pub fn seconds_in(
    intervals: &[Interval],
    agent: Option<&str>,
    first: Date,
    last: Date,
    tz: &TimeZone,
) -> u64 {
    let start_of = |d: Date| {
        d.to_zoned_datetime(tz.clone())
            .map(|z| z.timestamp().as_second())
            .ok()
    };
    let (Some(from), Some(to)) = (start_of(first), last.tomorrow().ok().and_then(start_of)) else {
        return 0;
    };
    let mut spans: Vec<(i64, i64)> = intervals
        .iter()
        .filter(|i| agent.is_none_or(|a| i.agent == a))
        .map(|i| (i.start.max(from), i.end.min(to)))
        .filter(|(s, e)| e > s)
        .collect();
    spans.sort_unstable();
    let (mut total, mut reach) = (0i64, i64::MIN);
    for (s, e) in spans {
        let s = s.max(reach);
        if e > s {
            total += e - s;
        }
        reach = reach.max(e);
    }
    u64::try_from(total).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlaps_count_once_and_ranges_clip() {
        let tz = TimeZone::UTC;
        let day = jiff::civil::date(2026, 11, 20);
        let base = day
            .to_zoned_datetime(tz.clone())
            .unwrap()
            .timestamp()
            .as_second();
        let iv = |agent: &str, s: i64, e: i64| Interval {
            agent: agent.into(),
            start: base + s,
            end: base + e,
        };
        let list = [
            iv("a", 100, 200),
            iv("b", 150, 300),       // overlaps: 100..300 = 200 s
            iv("a", -50, 10),        // starts the day before: 10 s inside
            iv("a", 86_390, 86_500), // ends the next day: 10 s inside
        ];
        assert_eq!(seconds_in(&list, None, day, day, &tz), 220);
        assert_eq!(seconds_in(&list, Some("a"), day, day, &tz), 120);
    }

    #[test]
    fn append_and_load_round_trip() {
        let path = std::env::temp_dir().join(format!("hv-observed-{}.jsonl", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let one = Interval {
            agent: "claude-code".into(),
            start: 10,
            end: 20,
        };
        append(&path, &one).unwrap();
        std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(b"garbage\n")
            .unwrap();
        append(&path, &one).unwrap();
        assert_eq!(load(&path), vec![one.clone(), one]);
        std::fs::remove_file(&path).unwrap();
    }
}
