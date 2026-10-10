//! What the sessions cost.
//!
//! Cost reported by the client is measured; cost computed from the price table
//! is estimated; requests of models without a price are counted as unpriced and
//! never contribute zero dollars silently.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::model::{EventKind, SessionEvents};
use crate::prices::PriceTable;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CostRow {
    pub key: String,
    pub requests: u64,
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    /// Cost the client reported.
    pub measured_usd: f64,
    /// Cost computed from the price table.
    pub estimated_usd: f64,
    pub unpriced_requests: u64,
    /// Premium requests (GitHub Copilot); a plan allowance, not money.
    pub premium_requests: f64,
}

impl CostRow {
    fn new(key: &str) -> CostRow {
        CostRow {
            key: key.to_owned(),
            ..CostRow::default()
        }
    }

    pub fn total_usd(&self) -> f64 {
        self.measured_usd + self.estimated_usd
    }

    fn add(&mut self, other: &CostRow) {
        self.requests += other.requests;
        self.input += other.input;
        self.output += other.output;
        self.cache_read += other.cache_read;
        self.cache_write += other.cache_write;
        self.measured_usd += other.measured_usd;
        self.estimated_usd += other.estimated_usd;
        self.unpriced_requests += other.unpriced_requests;
        self.premium_requests += other.premium_requests;
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CostReport {
    /// Sorted by total cost, largest first; unpriced-only rows last.
    pub rows: Vec<CostRow>,
    pub total: CostRow,
    /// Models with unpriced requests and how many.
    pub unpriced_models: BTreeMap<String, u64>,
}

/// Cost per model.
pub fn by_model(sessions: &[SessionEvents], prices: &PriceTable) -> CostReport {
    let mut rows: BTreeMap<String, CostRow> = BTreeMap::new();
    let mut unpriced: BTreeMap<String, u64> = BTreeMap::new();
    for se in sessions {
        for e in &se.events {
            let EventKind::Usage(u) = &e.kind else {
                continue;
            };
            let model = u
                .model
                .clone()
                .or_else(|| se.session.model.clone())
                .unwrap_or_else(|| "unknown".into());
            let row = rows
                .entry(model.clone())
                .or_insert_with(|| CostRow::new(&model));
            row.requests += 1;
            row.input += u.input;
            row.output += u.output;
            row.cache_read += u.cache_read;
            row.cache_write += u.cache_write;
            row.premium_requests += u.premium_requests.unwrap_or(0.0);
            let mut usage = u.clone();
            usage.model = Some(model.clone());
            match prices.cost(&usage) {
                Some(c) if c.measured => row.measured_usd += c.usd,
                Some(c) => row.estimated_usd += c.usd,
                None => {
                    row.unpriced_requests += 1;
                    *unpriced.entry(model).or_default() += 1;
                }
            }
        }
    }
    let mut total = CostRow::new("total");
    for r in rows.values() {
        total.add(r);
    }
    let mut rows: Vec<CostRow> = rows.into_values().collect();
    rows.sort_by(|a, b| {
        b.total_usd()
            .partial_cmp(&a.total_usd())
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.key.cmp(&b.key))
    });
    CostReport {
        rows,
        total,
        unpriced_models: unpriced,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Session, Source, Usage};

    fn usage(model: Option<&str>, input: u64, output: u64, cost: Option<f64>) -> EventKind {
        EventKind::Usage(Usage {
            model: model.map(Into::into),
            input,
            output,
            cost_usd: cost,
            premium_requests: Some(1.0),
            ..Usage::default()
        })
    }

    fn session(kinds: Vec<EventKind>) -> SessionEvents {
        let mut se = SessionEvents::new(Session::new("s", Source::ClaudeCode));
        for k in kinds {
            se.push(None, k);
        }
        se
    }

    #[test]
    fn cost_is_split_into_measured_estimated_and_unpriced() {
        let se = session(vec![
            usage(Some("claude-sonnet-5-5"), 1_000_000, 100_000, None), // 2.0 + 1.0 estimated
            usage(Some("claude-sonnet-5-5"), 1_000_000, 0, Some(0.25)), // measured
            usage(Some("mystery"), 10, 10, None),
            usage(Some("free-model"), 10, 10, Some(0.0)), // reported zero is a price
        ]);
        let r = by_model(&[se], &PriceTable::bundled());
        let sonnet = r
            .rows
            .iter()
            .find(|x| x.key == "claude-sonnet-5-5")
            .unwrap();
        assert_eq!(sonnet.requests, 2);
        assert!((sonnet.estimated_usd - 3.0).abs() < 1e-9);
        assert!((sonnet.measured_usd - 0.25).abs() < 1e-9);
        assert_eq!(r.unpriced_models.get("mystery"), Some(&1));
        assert!(!r.unpriced_models.contains_key("free-model"));
        assert!((r.total.total_usd() - 3.25).abs() < 1e-9);
        assert_eq!(r.total.unpriced_requests, 1);
        assert_eq!(r.total.requests, 4);
        assert_eq!(r.total.premium_requests, 4.0);
        assert_eq!(r.rows[0].key, "claude-sonnet-5-5", "sorted by cost");
    }

    #[test]
    fn requests_without_a_model_use_the_session_model_or_are_unknown() {
        let mut se = session(vec![usage(None, 1_000_000, 0, None)]);
        se.session.model = Some("claude-sonnet-5-5".into());
        let unknown = session(vec![usage(None, 5, 5, None)]);
        let r = by_model(&[se, unknown], &PriceTable::bundled());
        assert!((r.rows[0].estimated_usd - 2.0).abs() < 1e-9);
        assert_eq!(r.unpriced_models.get("unknown"), Some(&1));
    }
}
