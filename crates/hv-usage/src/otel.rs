//! What agents export over OpenTelemetry (F5-15, `docs/agent-integration.md` 3.2): the numbers
//! their logs do not have. Claude Code sends lines of code added and removed, commits, pull
//! requests and its active time. hivetty receives them on a local endpoint (only for the
//! sessions it starts, when `[usage] otlp` is on) and keeps only these numbers with their
//! time, never the attributes that identify the account. Format checked against Claude Code
//! 2.1.330 (`tests/fixtures/otel/claude-code/2.1.330`).

use std::io::Write;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// Largest file read.
const MAX_BYTES: u64 = 16 << 20;

/// What a point counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Metric {
    LinesAdded,
    LinesRemoved,
    Commits,
    PullRequests,
}

/// One received number.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub metric: Metric,
    pub value: f64,
    /// Unix seconds (end of the reported interval).
    pub time: i64,
}

/// An OTLP number, which JSON may carry as a string (64-bit integers) or a number.
fn number(v: &serde_json::Value) -> Option<f64> {
    match v {
        serde_json::Value::Number(n) => n.as_f64(),
        serde_json::Value::String(s) => s.parse().ok(),
        _ => None,
    }
    .filter(|n: &f64| n.is_finite() && *n >= 0.0)
}

fn attribute<'a>(point: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    point
        .get("attributes")?
        .as_array()?
        .iter()
        .find(|a| a.get("key").and_then(|k| k.as_str()) == Some(key))?
        .pointer("/value/stringValue")?
        .as_str()
}

/// The points of one OTLP/HTTP JSON metrics request. Only delta sums are read (hivetty asks
/// for delta temporality); anything else is ignored.
pub fn parse_metrics(body: &str) -> Vec<Point> {
    let Ok(json) = serde_json::from_str::<serde_json::Value>(body) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let metrics = json
        .get("resourceMetrics")
        .and_then(|r| r.as_array())
        .into_iter()
        .flatten()
        .filter_map(|r| r.get("scopeMetrics")?.as_array())
        .flatten()
        .filter_map(|s| s.get("metrics")?.as_array())
        .flatten();
    for metric in metrics {
        let Some(sum) = metric.get("sum") else {
            continue;
        };
        // 1 = AGGREGATION_TEMPORALITY_DELTA
        if sum.get("aggregationTemporality").and_then(|t| t.as_i64()) != Some(1) {
            continue;
        }
        let name = metric
            .get("name")
            .and_then(|n| n.as_str())
            .unwrap_or_default();
        for point in sum
            .get("dataPoints")
            .and_then(|d| d.as_array())
            .into_iter()
            .flatten()
        {
            let kind = match (name, attribute(point, "type")) {
                ("claude_code.lines_of_code.count", Some("added")) => Metric::LinesAdded,
                ("claude_code.lines_of_code.count", Some("removed")) => Metric::LinesRemoved,
                ("claude_code.commit.count", _) => Metric::Commits,
                ("claude_code.pull_request.count", _) => Metric::PullRequests,
                _ => continue,
            };
            let Some(value) = point
                .get("asInt")
                .or_else(|| point.get("asDouble"))
                .and_then(number)
            else {
                continue;
            };
            let time = point
                .get("timeUnixNano")
                .and_then(number)
                .map_or(0, |ns| (ns / 1e9) as i64);
            if value > 0.0 {
                out.push(Point {
                    metric: kind,
                    value,
                    time,
                });
            }
        }
    }
    out
}

/// Appends points to the file (one JSON object per line). Blocking.
pub fn append(path: &Path, points: &[Point]) -> std::io::Result<()> {
    if points.is_empty() {
        return Ok(());
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut text = String::new();
    for p in points {
        text.push_str(&serde_json::to_string(p).map_err(std::io::Error::other)?);
        text.push('\n');
    }
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?
        .write_all(text.as_bytes())
}

/// Reads the points; bad lines and an oversized file are skipped.
pub fn load(path: &Path) -> Vec<Point> {
    if std::fs::metadata(path).map_or(true, |m| m.len() > MAX_BYTES) {
        return Vec::new();
    }
    std::fs::read_to_string(path)
        .map(|t| {
            t.lines()
                .filter_map(|l| serde_json::from_str(l).ok())
                .collect()
        })
        .unwrap_or_default()
}

/// Totals between two Unix times (inclusive start, exclusive end).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CodeStats {
    pub lines_added: u64,
    pub lines_removed: u64,
    pub commits: u64,
    pub pull_requests: u64,
}

/// Adds up the points in `[from, to)`.
pub fn totals(points: &[Point], from: i64, to: i64) -> CodeStats {
    let mut t = CodeStats::default();
    for p in points.iter().filter(|p| p.time >= from && p.time < to) {
        let n = p.value.round() as u64;
        match p.metric {
            Metric::LinesAdded => t.lines_added += n,
            Metric::LinesRemoved => t.lines_removed += n,
            Metric::Commits => t.commits += n,
            Metric::PullRequests => t.pull_requests += n,
        }
    }
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../tests/fixtures/otel/claude-code/2.1.330/metrics.jsonl");

    #[test]
    fn a_real_export_gives_lines_of_code() {
        let points: Vec<Point> = FIXTURE.lines().flat_map(parse_metrics).collect();
        // The session wrote one file: lines added, none removed (zero values are dropped).
        assert_eq!(points.len(), 1, "{points:?}");
        assert_eq!(points[0].metric, Metric::LinesAdded);
        assert_eq!(points[0].value, 4.0);
        assert!(points[0].time > 1_700_000_000);
        let t = totals(&points, 0, i64::MAX);
        assert_eq!((t.lines_added, t.lines_removed), (4, 0));
    }

    #[test]
    fn commits_cumulative_sums_and_garbage() {
        let body = |temporality: i64| {
            serde_json::json!({"resourceMetrics": [{"scopeMetrics": [{"metrics": [{
                "name": "claude_code.commit.count",
                "sum": {"aggregationTemporality": temporality, "dataPoints": [
                    {"asInt": "2", "timeUnixNano": "1795395175441000000"}
                ]}
            }]}]}]})
            .to_string()
        };
        assert_eq!(parse_metrics(&body(1))[0].metric, Metric::Commits);
        assert!(
            parse_metrics(&body(2)).is_empty(),
            "cumulative sums are not added"
        );
        assert!(parse_metrics("not json").is_empty());
    }

    #[test]
    fn points_round_trip_through_the_file() {
        let path = std::env::temp_dir().join(format!("hv-otel-{}.jsonl", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let p = Point {
            metric: Metric::PullRequests,
            value: 1.0,
            time: 10,
        };
        append(&path, std::slice::from_ref(&p)).unwrap();
        assert_eq!(load(&path), vec![p]);
        std::fs::remove_file(&path).unwrap();
    }
}
