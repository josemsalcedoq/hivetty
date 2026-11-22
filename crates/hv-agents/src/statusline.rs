//! Claude Code's statusline input (F4-32, U-65): the JSON it pipes to the `statusLine`
//! command. hivetty's own statusline (`hivetty statusline`) prints a short line and forwards
//! the JSON to the app, which keeps the context use per pane and the plan limits.
//!
//! Fields per <https://code.claude.com/docs/en/statusline>; every one is optional (the rate
//! limits only exist for subscription plans, and only after the first response).

use serde::Deserialize;

/// One plan window (5 hours or 7 days).
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct Limit {
    /// 0 to 100.
    pub used_percentage: f64,
    /// Unix seconds.
    pub resets_at: i64,
}

/// What hivetty uses from the statusline JSON.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StatusLine {
    pub session_id: Option<String>,
    /// Model display name ("Opus").
    pub model: Option<String>,
    /// Context window used, 0 to 100.
    pub context_percent: Option<f64>,
    pub five_hour: Option<Limit>,
    pub seven_day: Option<Limit>,
}

#[derive(Deserialize)]
struct Raw {
    session_id: Option<String>,
    model: Option<RawModel>,
    context_window: Option<RawContext>,
    rate_limits: Option<RawLimits>,
}

#[derive(Deserialize)]
struct RawModel {
    display_name: Option<String>,
}

#[derive(Deserialize)]
struct RawContext {
    used_percentage: Option<f64>,
}

#[derive(Deserialize)]
struct RawLimits {
    five_hour: Option<Limit>,
    seven_day: Option<Limit>,
}

/// A percentage hivetty is willing to show.
fn valid(p: f64) -> Option<f64> {
    (p.is_finite() && (0.0..=1000.0).contains(&p)).then_some(p)
}

/// Parses the statusline JSON; `None` when it is not JSON at all.
pub fn parse(payload: &str) -> Option<StatusLine> {
    let raw: Raw = serde_json::from_str(payload).ok()?;
    let limit =
        |l: Option<Limit>| l.filter(|l| valid(l.used_percentage).is_some() && l.resets_at > 0);
    let limits = raw.rate_limits;
    Some(StatusLine {
        session_id: raw.session_id,
        model: raw.model.and_then(|m| m.display_name),
        context_percent: raw
            .context_window
            .and_then(|c| c.used_percentage)
            .and_then(valid),
        five_hour: limit(limits.as_ref().and_then(|l| l.five_hour)),
        seven_day: limit(limits.as_ref().and_then(|l| l.seven_day)),
    })
}

impl StatusLine {
    /// The line printed under Claude Code's prompt: `Opus · ctx 8% · 5h 24% · 7d 41%`.
    pub fn render(&self) -> String {
        let mut parts = Vec::new();
        if let Some(model) = &self.model {
            parts.push(model.clone());
        }
        if let Some(p) = self.context_percent {
            parts.push(format!("ctx {p:.0}%"));
        }
        if let Some(l) = self.five_hour {
            parts.push(format!("5h {:.0}%", l.used_percentage));
        }
        if let Some(l) = self.seven_day {
            parts.push(format!("7d {:.0}%", l.used_percentage));
        }
        parts.join(" · ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn documented_example_parses() {
        let json = r#"{
          "cwd": "/w", "session_id": "abc123",
          "model": { "id": "claude-opus-5-5", "display_name": "Opus" },
          "context_window": { "total_input_tokens": 15500, "context_window_size": 200000,
            "used_percentage": 8, "remaining_percentage": 92 },
          "rate_limits": {
            "five_hour": { "used_percentage": 23.5, "resets_at": 1738425600 },
            "seven_day": { "used_percentage": 41.2, "resets_at": 1738857600 }
          }
        }"#;
        let s = parse(json).unwrap();
        assert_eq!(s.session_id.as_deref(), Some("abc123"));
        assert_eq!(s.context_percent, Some(8.0));
        assert_eq!(s.five_hour.unwrap().resets_at, 1_738_425_600);
        assert_eq!(s.render(), "Opus · ctx 8% · 5h 24% · 7d 41%");
    }

    #[test]
    fn missing_and_odd_fields_are_tolerated() {
        let s = parse(r#"{"context_window":{"used_percentage":null},"rate_limits":{}}"#).unwrap();
        assert_eq!(s, StatusLine::default());
        assert_eq!(s.render(), "");
        let bad = parse(r#"{"rate_limits":{"five_hour":{"used_percentage":-5,"resets_at":1}}}"#);
        assert_eq!(bad.unwrap().five_hour, None);
        assert!(parse("not json").is_none());
    }
}
