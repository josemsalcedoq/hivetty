//! Estimated cost (F5-13, U-60) from a price table the user keeps in `pricing.toml`:
//!
//! ```toml
//! # USD per million tokens. A key matches every model whose id starts with it; the longest
//! # matching key wins.
//! [models."claude-opus"]
//! input = 15.0
//! output = 75.0
//! cache_read = 1.5
//! cache_write = 18.75
//! ```
//!
//! hivetty ships no prices: they change, and a wrong built-in number would be worse than none.

use serde::Deserialize;

use crate::UsageRecord;

/// Prices of one model family, in USD per million tokens.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Price {
    pub input: f64,
    pub output: f64,
    #[serde(default)]
    pub cache_read: Option<f64>,
    #[serde(default)]
    pub cache_write: Option<f64>,
}

/// The whole table.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pricing {
    #[serde(default)]
    pub models: std::collections::BTreeMap<String, Price>,
}

impl Pricing {
    /// Parses `pricing.toml`.
    pub fn parse(text: &str) -> Result<Self, String> {
        toml::from_str(text).map_err(|e| e.to_string())
    }

    /// Whether any price is set.
    pub fn is_empty(&self) -> bool {
        self.models.is_empty()
    }

    /// The price for a model id: the longest key it starts with.
    pub fn price(&self, model: &str) -> Option<&Price> {
        self.models
            .iter()
            .filter(|(key, _)| model.starts_with(key.as_str()))
            .max_by_key(|(key, _)| key.len())
            .map(|(_, p)| p)
    }

    /// Cost of one turn in USD, if its model has a price. Cache tokens without their own price
    /// are charged as input.
    pub fn cost(&self, r: &UsageRecord) -> Option<f64> {
        let p = self.price(&r.model)?;
        let per = |tokens: u64, usd: f64| tokens as f64 / 1e6 * usd;
        Some(
            per(r.input_tokens, p.input)
                + per(r.output_tokens, p.output)
                + per(r.cache_read_tokens, p.cache_read.unwrap_or(p.input))
                + per(r.cache_write_tokens, p.cache_write.unwrap_or(p.input)),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TABLE: &str = r#"
[models."claude"]
input = 3.0
output = 15.0

[models."claude-opus"]
input = 15.0
output = 75.0
cache_read = 1.5
"#;

    fn rec(model: &str) -> UsageRecord {
        UsageRecord {
            agent: "claude-code",
            provider: "anthropic",
            model: model.into(),
            timestamp: String::new(),
            session_id: None,
            project: None,
            input_tokens: 1_000_000,
            output_tokens: 100_000,
            cache_read_tokens: 2_000_000,
            cache_write_tokens: 0,
        }
    }

    #[test]
    fn longest_prefix_wins_and_cache_defaults_to_input() {
        let pricing = Pricing::parse(TABLE).unwrap();
        // opus: 1M × 15 + 0.1M × 75 + 2M × 1.5 = 15 + 7.5 + 3
        assert!((pricing.cost(&rec("claude-opus-5-5")).unwrap() - 25.5).abs() < 1e-9);
        // other claude: 1M × 3 + 0.1M × 15 + 2M × 3 (cache read at the input price)
        assert!((pricing.cost(&rec("claude-haiku-5-5")).unwrap() - 10.5).abs() < 1e-9);
        assert_eq!(pricing.cost(&rec("gpt-6")), None);
        assert!(Pricing::parse("[models.x]\ninput = \"free\"").is_err());
    }
}
