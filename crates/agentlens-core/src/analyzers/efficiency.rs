//! How well the context is reused and how much work subagents do.
//!
//! Cache hit rate = cache read tokens / all input tokens of a request
//! (uncached input + cache read + cache write), measured from client usage.
//! A source that reports no cache tokens at all has no hit rate (not 0%).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::model::{EventKind, SessionEvents, Source};
use crate::prices::PriceTable;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CacheBy {
    Session,
    Project,
    Source,
}

pub const NO_PROJECT: &str = "(no project)";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CacheRow {
    pub key: String,
    pub source: Source,
    pub requests: u64,
    /// Uncached input tokens.
    pub input: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    /// `None` if the source reports no cache tokens at all.
    pub hit_rate: Option<f64>,
}

/// Share of the work done by subagents, per source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubagentShare {
    pub source: Source,
    pub requests: u64,
    pub subagent_requests: u64,
    /// Input and output tokens.
    pub tokens: u64,
    pub subagent_tokens: u64,
    /// Cost of the priced requests.
    pub usd: f64,
    pub subagent_usd: f64,
    /// Requests without a price; they are left out of the cost shares.
    pub unpriced_requests: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EfficiencyReport {
    /// Sorted by requests, largest first.
    pub cache: Vec<CacheRow>,
    pub subagents: Vec<SubagentShare>,
}

pub fn analyze(sessions: &[SessionEvents], prices: &PriceTable, by: CacheBy) -> EfficiencyReport {
    let mut rows: BTreeMap<(Source, String), CacheRow> = BTreeMap::new();
    let mut shares: BTreeMap<Source, SubagentShare> = BTreeMap::new();
    for se in sessions {
        let source = se.session.source;
        let key = match by {
            CacheBy::Session => se.session.id.clone(),
            CacheBy::Project => se
                .session
                .project_dir
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| NO_PROJECT.into()),
            CacheBy::Source => source.id().to_owned(),
        };
        for e in &se.events {
            let EventKind::Usage(u) = &e.kind else {
                continue;
            };
            let row = rows
                .entry((source, key.clone()))
                .or_insert_with(|| CacheRow {
                    key: key.clone(),
                    source,
                    requests: 0,
                    input: 0,
                    cache_read: 0,
                    cache_write: 0,
                    hit_rate: None,
                });
            row.requests += 1;
            row.input += u.input;
            row.cache_read += u.cache_read;
            row.cache_write += u.cache_write;

            let share = shares.entry(source).or_insert_with(|| SubagentShare {
                source,
                requests: 0,
                subagent_requests: 0,
                tokens: 0,
                subagent_tokens: 0,
                usd: 0.0,
                subagent_usd: 0.0,
                unpriced_requests: 0,
            });
            let tokens = u.total_input() + u.output;
            share.requests += 1;
            share.tokens += tokens;
            if u.subagent {
                share.subagent_requests += 1;
                share.subagent_tokens += tokens;
            }
            let mut usage = u.clone();
            if usage.model.is_none() {
                usage.model = se.session.model.clone();
            }
            match prices.cost(&usage) {
                Some(c) => {
                    share.usd += c.usd;
                    if u.subagent {
                        share.subagent_usd += c.usd;
                    }
                }
                None => share.unpriced_requests += 1,
            }
        }
    }

    // A source without any cache token has no meaningful hit rate.
    let mut has_cache: BTreeMap<Source, bool> = BTreeMap::new();
    for r in rows.values() {
        *has_cache.entry(r.source).or_default() |= r.cache_read + r.cache_write > 0;
    }
    let mut cache: Vec<CacheRow> = rows.into_values().collect();
    for r in &mut cache {
        let total = r.input + r.cache_read + r.cache_write;
        r.hit_rate =
            (has_cache[&r.source] && total > 0).then(|| r.cache_read as f64 / total as f64);
    }
    cache.sort_by(|a, b| b.requests.cmp(&a.requests).then_with(|| a.key.cmp(&b.key)));
    EfficiencyReport {
        cache,
        subagents: shares.into_values().collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Session, Usage};

    fn request(input: u64, read: u64, write: u64, subagent: bool) -> EventKind {
        EventKind::Usage(Usage {
            model: Some("claude-sonnet-5-5".into()),
            input,
            output: 0,
            cache_read: read,
            cache_write: write,
            subagent,
            ..Usage::default()
        })
    }

    fn session(id: &str, source: Source, project: &str, kinds: Vec<EventKind>) -> SessionEvents {
        let mut s = Session::new(id, source);
        s.project_dir = Some(project.into());
        let mut se = SessionEvents::new(s);
        for k in kinds {
            se.push(None, k);
        }
        se
    }

    #[test]
    fn hit_rate_is_cache_read_over_all_input() {
        let a = session(
            "a",
            Source::ClaudeCode,
            "/p",
            vec![request(100, 600, 300, false), request(0, 1000, 0, false)],
        );
        let b = session(
            "b",
            Source::ClaudeCode,
            "/q",
            vec![request(500, 0, 500, false)],
        );
        let r = analyze(
            &[a.clone(), b.clone()],
            &PriceTable::bundled(),
            CacheBy::Session,
        );
        let get = |k: &str| r.cache.iter().find(|x| x.key == k).unwrap();
        assert!((get("a").hit_rate.unwrap() - 1600.0 / 2000.0).abs() < 1e-9);
        assert_eq!(
            get("b").hit_rate,
            Some(0.0),
            "a source with caching and no hits has 0%"
        );
        let by_project = analyze(
            &[a.clone(), b.clone()],
            &PriceTable::bundled(),
            CacheBy::Project,
        );
        assert_eq!(by_project.cache.len(), 2);
        let by_source = analyze(&[a, b], &PriceTable::bundled(), CacheBy::Source);
        assert_eq!(by_source.cache.len(), 1);
        assert_eq!(by_source.cache[0].requests, 3);
        assert!((by_source.cache[0].hit_rate.unwrap() - 1600.0 / 3000.0).abs() < 1e-9);
    }

    #[test]
    fn sources_without_cache_tokens_have_no_hit_rate() {
        let a = session("a", Source::OpenCode, "/p", vec![request(100, 0, 0, false)]);
        let r = analyze(&[a], &PriceTable::bundled(), CacheBy::Source);
        assert_eq!(r.cache[0].hit_rate, None);
    }

    #[test]
    fn subagent_share_covers_requests_tokens_and_cost() {
        let s = session(
            "a",
            Source::ClaudeCode,
            "/p",
            vec![
                request(1_000_000, 0, 0, false), // $2.00
                request(1_000_000, 0, 0, true),  // $2.00
                request(1_000_000, 0, 0, true),  // $2.00
                EventKind::Usage(Usage {
                    model: Some("mystery".into()),
                    input: 5,
                    subagent: true,
                    ..Usage::default()
                }),
            ],
        );
        let r = analyze(&[s], &PriceTable::bundled(), CacheBy::Source);
        let sh = &r.subagents[0];
        assert_eq!((sh.requests, sh.subagent_requests), (4, 3));
        assert_eq!((sh.tokens, sh.subagent_tokens), (3_000_005, 2_000_005));
        assert!((sh.usd - 6.0).abs() < 1e-9 && (sh.subagent_usd - 4.0).abs() < 1e-9);
        assert_eq!(sh.unpriced_requests, 1);
    }
}
