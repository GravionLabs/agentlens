//! Model prices and the cost of a request (ADR 0002).
//!
//! The bundled table only holds prices that were checked against the vendor's
//! price list; a user file can override or extend it. Models without a price
//! stay unpriced, nothing is guessed.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::env::Env;
use crate::inventory::config_home;
use crate::model::Usage;

const BUNDLED: &str = include_str!("../assets/prices.json");

/// Prices in USD per million tokens.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Price {
    pub input: f64,
    pub output: f64,
    /// Defaults to `input` if the entry has none.
    #[serde(default)]
    pub cache_write: Option<f64>,
    /// Defaults to `input` if the entry has none.
    #[serde(default)]
    pub cache_read: Option<f64>,
    /// Different prices for requests whose prompt is longer than a threshold.
    #[serde(default)]
    pub above: Option<Tier>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub checked: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tier {
    pub over_tokens: u64,
    pub input: f64,
    pub output: f64,
    #[serde(default)]
    pub cache_write: Option<f64>,
    #[serde(default)]
    pub cache_read: Option<f64>,
}

/// The prices one request pays per token class, in USD per million tokens (the tier already chosen).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rates {
    pub input: f64,
    pub output: f64,
    pub cache_write: f64,
    pub cache_read: f64,
}

/// Where a request's cost comes from.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RequestCost {
    pub usd: f64,
    /// `true` if the client reported the cost, `false` if it was computed.
    pub measured: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PriceTable {
    entries: BTreeMap<String, Price>,
}

/// `Claude-Sonnet-4.6` and `claude-sonnet-4-6` name the same model.
pub(crate) fn normalize(model: &str) -> String {
    model.trim().to_lowercase().replace('.', "-")
}

impl PriceTable {
    pub fn bundled() -> PriceTable {
        PriceTable::parse(BUNDLED).expect("the bundled price table is valid")
    }

    pub fn parse(text: &str) -> Result<PriceTable, String> {
        let raw: BTreeMap<String, Value> = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let mut entries = BTreeMap::new();
        for (key, value) in raw {
            if key.starts_with('_') {
                continue;
            }
            let price: Price =
                serde_json::from_value(value).map_err(|e| format!("price `{key}`: {e}"))?;
            entries.insert(normalize(&key), price);
        }
        Ok(PriceTable { entries })
    }

    /// Entries of `other` replace entries with the same key.
    pub fn with_overrides(mut self, other: PriceTable) -> PriceTable {
        self.entries.extend(other.entries);
        self
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The entry with the longest key that is a prefix of the model id.
    pub fn lookup(&self, model: &str) -> Option<&Price> {
        let model = normalize(model);
        self.entries
            .iter()
            .filter(|(prefix, _)| model.starts_with(prefix.as_str()))
            .max_by_key(|(prefix, _)| prefix.len())
            .map(|(_, p)| p)
    }

    /// The prices per token class (USD per million) that a request of `model` with `total_input`
    /// tokens of input pays; `None` if the model has no price.
    pub fn rates(&self, model: &str, total_input: u64) -> Option<Rates> {
        let p = self.lookup(model)?;
        Some(match &p.above {
            Some(t) if total_input > t.over_tokens => Rates {
                input: t.input,
                output: t.output,
                cache_write: t.cache_write.unwrap_or(t.input),
                cache_read: t.cache_read.unwrap_or(t.input),
            },
            _ => Rates {
                input: p.input,
                output: p.output,
                cache_write: p.cache_write.unwrap_or(p.input),
                cache_read: p.cache_read.unwrap_or(p.input),
            },
        })
    }

    /// Cost computed from the table; `None` if the model has no price.
    pub fn compute(&self, model: &str, usage: &Usage) -> Option<f64> {
        let r = self.rates(model, usage.total_input())?;
        let per_million = usage.input as f64 * r.input
            + usage.cache_write as f64 * r.cache_write
            + usage.cache_read as f64 * r.cache_read
            + usage.output as f64 * r.output;
        Some(per_million / 1_000_000.0)
    }

    /// The reported cost if there is one, otherwise the computed one.
    pub fn cost(&self, usage: &Usage) -> Option<RequestCost> {
        if let Some(usd) = usage.cost_usd {
            return Some(RequestCost {
                usd,
                measured: true,
            });
        }
        let usd = self.compute(usage.model.as_deref()?, usage)?;
        Some(RequestCost {
            usd,
            measured: false,
        })
    }
}

/// `<config home>/agentlens/prices.json`
pub fn user_file(env: &Env) -> PathBuf {
    config_home(env).join("agentlens/prices.json")
}

/// The bundled table plus the overrides from `file`, or from the default user
/// file if it exists and no file is given.
pub fn load(env: &Env, file: Option<&Path>) -> Result<PriceTable, String> {
    let table = PriceTable::bundled();
    let path = match file {
        Some(f) => f.to_path_buf(),
        None => {
            let default = user_file(env);
            if !default.is_file() {
                return Ok(table);
            }
            default
        }
    };
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let overrides = PriceTable::parse(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(table.with_overrides(overrides))
}

/// Like [`load`], but never fails: for an analysis that only ranks by price (advice), a broken or
/// unreadable price file falls back to the bundled table, with a warning that names the file and the error.
pub fn load_or_bundled(env: &Env, file: Option<&Path>) -> (PriceTable, Option<String>) {
    match load(env, file) {
        Ok(table) => (table, None),
        Err(e) => (
            PriceTable::bundled(),
            Some(format!(
                "cannot load prices: {e}; the bundled prices are used"
            )),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usage(model: &str, input: u64, output: u64, read: u64, write: u64) -> Usage {
        Usage {
            model: Some(model.into()),
            input,
            output,
            cache_read: read,
            cache_write: write,
            ..Usage::default()
        }
    }

    #[test]
    fn bundled_entries_all_have_a_source_and_a_check_date() {
        let t = PriceTable::bundled();
        assert!(t.len() >= 15);
        for (key, p) in &t.entries {
            let src = p.source.as_deref().unwrap_or("");
            assert!(src.starts_with("https://"), "{key} has no source");
            let date = p.checked.as_deref().unwrap_or("");
            assert!(
                date.len() == 10 && date.starts_with("20"),
                "{key} has no check date"
            );
            assert!(
                p.cache_read.is_some() && p.cache_write.is_some(),
                "{key} lacks cache prices"
            );
        }
    }

    #[test]
    fn longest_prefix_wins_and_dots_equal_dashes() {
        let t = PriceTable::bundled();
        assert_eq!(
            t.lookup("claude-fable-5-1-20261001").unwrap().cache_read,
            Some(0.25)
        );
        assert_eq!(t.lookup("claude-fable-5").unwrap().cache_read, Some(1.0));
        assert_eq!(t.lookup("Claude-Sonnet-4.6").unwrap().input, 3.0);
        assert_eq!(t.lookup("claude-opus-4-5-x").unwrap().output, 25.0);
        assert!(t.lookup("gpt-5").is_none(), "unknown models are unpriced");
    }

    #[test]
    fn cost_is_computed_per_token_class() {
        let t = PriceTable::bundled();
        // Sonnet 5.5: $2 in, $10 out, $2.50 cache write, $0.10 cache read per MTok.
        let u = usage("claude-sonnet-5-5", 1_000_000, 100_000, 2_000_000, 400_000);
        let c = t.compute("claude-sonnet-5-5", &u).unwrap();
        assert!((c - (2.0 + 1.0 + 0.2 + 1.0)).abs() < 1e-9, "{c}");
    }

    #[test]
    fn long_prompts_use_the_higher_tier() {
        let t = PriceTable::bundled();
        let short = usage("claude-haiku-5-5", 100_000, 1_000_000, 0, 0);
        let long = usage("claude-haiku-5-5", 100_001, 1_000_000, 0, 0);
        assert!((t.compute("claude-haiku-5-5", &short).unwrap() - (0.01 + 0.5)).abs() < 1e-9);
        let c = t.compute("claude-haiku-5-5", &long).unwrap();
        assert!((c - (100_001.0 * 0.5 / 1e6 + 2.5)).abs() < 1e-9, "{c}");
    }

    #[test]
    fn reported_cost_wins_and_is_measured() {
        let t = PriceTable::bundled();
        let mut u = usage("claude-sonnet-5-5", 1_000_000, 0, 0, 0);
        assert_eq!(
            t.cost(&u),
            Some(RequestCost {
                usd: 2.0,
                measured: false
            })
        );
        u.cost_usd = Some(0.5);
        assert_eq!(
            t.cost(&u),
            Some(RequestCost {
                usd: 0.5,
                measured: true
            })
        );
        let unknown = usage("mystery", 10, 10, 0, 0);
        assert_eq!(t.cost(&unknown), None);
    }

    #[test]
    fn load_or_bundled_falls_back_to_the_bundled_table_with_a_warning() {
        let dir = tempfile::tempdir().unwrap();
        let env = Env::for_home(dir.path());
        // No user file: the bundled table, no warning.
        let (t, w) = load_or_bundled(&env, None);
        assert_eq!((t.len(), w), (PriceTable::bundled().len(), None));

        let file = user_file(&env);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, "{ nope").unwrap();
        assert!(load(&env, None).is_err(), "load itself still fails");
        let (t, w) = load_or_bundled(&env, None);
        assert_eq!(t.len(), PriceTable::bundled().len());
        let w = w.expect("a warning");
        assert!(w.starts_with("cannot load prices: "), "{w}");
        assert!(
            w.contains(&file.display().to_string()),
            "names the file: {w}"
        );
        assert!(w.ends_with("; the bundled prices are used"), "{w}");

        // An unreadable file (a directory) as well.
        let (_, w) = load_or_bundled(&env, Some(dir.path()));
        assert!(w.is_some());
    }

    #[test]
    fn user_file_overrides_and_extends() {
        let user = PriceTable::parse(
            r#"{"_note":"x","claude-sonnet-5-5":{"input":1,"output":1},"my-model":{"input":3,"output":6,"cache_read":0.3}}"#,
        )
        .unwrap();
        let t = PriceTable::bundled().with_overrides(user);
        let u = usage("claude-sonnet-5-5", 1_000_000, 1_000_000, 0, 0);
        assert!((t.compute("claude-sonnet-5-5", &u).unwrap() - 2.0).abs() < 1e-9);
        let m = usage("my-model-v2", 1_000_000, 0, 1_000_000, 1_000_000);
        // cache_write falls back to the input price.
        assert!((t.compute("my-model-v2", &m).unwrap() - (3.0 + 0.3 + 3.0)).abs() < 1e-9);
        assert!(PriceTable::parse("{ nope").is_err());
        assert!(PriceTable::parse(r#"{"x":{"input":"a"}}"#).is_err());
    }
}
