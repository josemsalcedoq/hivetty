//! Agent usage: parsers for each agent's local logs and aggregations for the dashboard.
//!
//! Tokens come from the agents' own logs (docs/adr/0005-usage-data-sources.md); agent time
//! comes from hivetty's observed state intervals (phase F4).

pub mod claude;
pub mod codex;
pub mod dashboard;
pub mod gemini;
pub mod observed;
pub mod pricing;
pub mod store;

use std::collections::BTreeMap;

use jiff::Timestamp;
use jiff::civil::Date;
use jiff::tz::TimeZone;

/// What earlier lines of one log said (its session, folder and current model), kept between
/// incremental reads of that log.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FileContext {
    pub session: Option<String>,
    pub cwd: Option<String>,
    pub model: Option<String>,
}

/// One assistant turn with its token usage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageRecord {
    /// Agent id (`claude-code`, `codex`, …).
    pub agent: &'static str,
    /// Model provider (`anthropic`, `openai`, …).
    pub provider: &'static str,
    /// Model id as reported by the agent.
    pub model: String,
    /// RFC 3339 timestamp (UTC).
    pub timestamp: String,
    /// Agent session id.
    pub session_id: Option<String>,
    /// Working directory of the session.
    pub project: Option<String>,
    /// Uncached input tokens.
    pub input_tokens: u64,
    /// Output tokens.
    pub output_tokens: u64,
    /// Input tokens read from the prompt cache.
    pub cache_read_tokens: u64,
    /// Input tokens written to the prompt cache.
    pub cache_write_tokens: u64,
}

impl UsageRecord {
    /// All input tokens: uncached + cache read + cache write (the dashboard's "INPUT TOKENS").
    pub fn total_input(&self) -> u64 {
        self.input_tokens + self.cache_read_tokens + self.cache_write_tokens
    }

    /// Local calendar day of the record, if its timestamp parses.
    pub fn local_day(&self) -> Option<Date> {
        let ts: Timestamp = self.timestamp.parse().ok()?;
        Some(ts.to_zoned_datetime(TimeZone::system()).date())
    }
}

/// One task: a prompt the user sent to an agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    /// Agent id.
    pub agent: &'static str,
    /// RFC 3339 timestamp (UTC).
    pub timestamp: String,
    /// Agent session id.
    pub session_id: Option<String>,
}

/// Token totals.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Totals {
    /// Uncached input.
    pub input: u64,
    /// Output.
    pub output: u64,
    /// Cache reads.
    pub cache_read: u64,
    /// Cache writes.
    pub cache_write: u64,
    /// Number of assistant turns.
    pub turns: u64,
}

impl Totals {
    fn add(&mut self, r: &UsageRecord) {
        self.input += r.input_tokens;
        self.output += r.output_tokens;
        self.cache_read += r.cache_read_tokens;
        self.cache_write += r.cache_write_tokens;
        self.turns += 1;
    }

    /// All input tokens.
    pub fn total_input(&self) -> u64 {
        self.input + self.cache_read + self.cache_write
    }

    /// Share of input served from the cache, 0.0–1.0 (the "98% from cache" figure).
    pub fn cache_share(&self) -> f64 {
        let total = self.total_input();
        if total == 0 {
            0.0
        } else {
            self.cache_read as f64 / total as f64
        }
    }
}

/// Totals per local day.
pub fn daily(records: &[UsageRecord]) -> BTreeMap<Date, Totals> {
    let mut out: BTreeMap<Date, Totals> = BTreeMap::new();
    for r in records {
        if let Some(day) = r.local_day() {
            out.entry(day).or_default().add(r);
        }
    }
    out
}

/// Totals per model.
pub fn by_model(records: &[UsageRecord]) -> BTreeMap<String, Totals> {
    let mut out: BTreeMap<String, Totals> = BTreeMap::new();
    for r in records {
        out.entry(r.model.clone()).or_default().add(r);
    }
    out
}

/// Formats a count the dashboard way: `11.6B`, `61.3M`, `22.9K`, `963`.
pub fn compact(n: u64) -> String {
    let n = n as f64;
    let (value, suffix) = if n >= 1e9 {
        (n / 1e9, "B")
    } else if n >= 1e6 {
        (n / 1e6, "M")
    } else if n >= 1e3 {
        (n / 1e3, "K")
    } else {
        return format!("{n}");
    };
    if value >= 100.0 {
        format!("{value:.0}{suffix}")
    } else {
        format!("{value:.1}{suffix}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_numbers_match_the_dashboard() {
        assert_eq!(compact(11_600_000_000), "11.6B");
        assert_eq!(compact(61_300_000), "61.3M");
        assert_eq!(compact(22_900), "22.9K");
        assert_eq!(compact(963_000), "963K");
        assert_eq!(compact(963), "963");
    }

    #[test]
    fn totals_and_cache_share() {
        let records = claude::parse_reader(
            include_str!("../tests/fixtures/claude-basic.jsonl").as_bytes(),
            &mut std::collections::HashSet::new(),
        );
        let by_model = by_model(&records);
        let opus = by_model["claude-opus-5-5"];
        assert_eq!(opus.total_input(), 3410);
        assert!((opus.cache_share() - 3000.0 / 3410.0).abs() < 1e-9);
        let day: u64 = daily(&records).values().map(|t| t.output).sum();
        assert_eq!(day, 250);
    }
}
