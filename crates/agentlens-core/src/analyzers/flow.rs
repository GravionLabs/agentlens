//! How sessions unfold: context growth and compactions, tool errors, hooks.
//!
//! Everything here is computed from numbers the clients report, so the values
//! are measured. Context size is the total input of a main-agent request
//! (uncached input plus cache reads and writes).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use std::collections::BTreeMap;

use crate::model::{EventKind, SessionEvents, Source, ToolStatus};

/// Context windows by model-name prefix (vendor documentation, October 2026).
/// Models that are not listed are never flagged.
const WINDOWS: &[(&str, u64)] = &[("claude-", 200_000)];

/// A session is near the limit from this share of the window on.
const NEAR_LIMIT_PERCENT: u64 = 90;

/// The window of a model if it is known and consistent with what was seen: a
/// request larger than the documented window means the session ran with a
/// larger variant, so the limit is unknown.
pub fn window_for(model: Option<&str>, peak: u64) -> Option<u64> {
    let model = model?;
    WINDOWS
        .iter()
        .find(|(prefix, _)| model.starts_with(prefix))
        .map(|(_, w)| *w)
        .filter(|w| peak <= *w)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionGrowth {
    pub source: Source,
    pub session: String,
    pub project: Option<PathBuf>,
    pub model: Option<String>,
    /// Main-agent requests.
    pub requests: u64,
    pub first_tokens: u64,
    pub last_tokens: u64,
    pub peak_tokens: u64,
    /// Context size of the request before each compaction.
    pub compactions: Vec<u64>,
    /// The limit the session is compared with, if one is known.
    pub window: Option<u64>,
    /// Where `window` comes from.
    pub window_basis: Option<WindowBasis>,
    pub near_limit: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WindowBasis {
    /// The documented context window of the model family.
    Documented,
    /// The largest context size at which a session of the same model was
    /// compacted: a proxy where the documented window does not fit (larger
    /// variants of a model).
    ObservedCompaction,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrowthReport {
    /// Sorted by peak context size, largest first.
    pub sessions: Vec<SessionGrowth>,
}

pub fn growth(sessions: &[SessionEvents]) -> GrowthReport {
    let mut out: Vec<SessionGrowth> = Vec::new();
    for se in sessions {
        let mut sizes = Vec::new();
        let mut compactions = Vec::new();
        let mut model = se.session.model.clone();
        for e in &se.events {
            match &e.kind {
                EventKind::Usage(u) if !u.subagent => {
                    sizes.push(u.total_input());
                    if model.is_none() {
                        model = u.model.clone();
                    }
                }
                EventKind::Compaction => {
                    if let Some(last) = sizes.last() {
                        compactions.push(*last);
                    }
                }
                _ => {}
            }
        }
        let (Some(first), Some(last)) = (sizes.first(), sizes.last()) else {
            continue;
        };
        let peak = sizes.iter().copied().max().unwrap_or(0);
        let window = window_for(model.as_deref(), peak);
        out.push(SessionGrowth {
            window_basis: window.map(|_| WindowBasis::Documented),
            source: se.session.source,
            session: se.session.id.clone(),
            project: se.session.project_dir.clone(),
            model,
            requests: sizes.len() as u64,
            first_tokens: *first,
            last_tokens: *last,
            peak_tokens: peak,
            compactions,
            window,
            near_limit: window.is_some_and(|w| peak * 100 >= w * NEAR_LIMIT_PERCENT),
        });
    }
    // Without a fitting documented window, compare with the largest size at
    // which this model was compacted.
    let mut observed: BTreeMap<String, u64> = Default::default();
    for g in &out {
        if let (Some(m), Some(c)) = (&g.model, g.compactions.iter().max()) {
            let e = observed.entry(m.clone()).or_default();
            *e = (*e).max(*c);
        }
    }
    for g in out.iter_mut().filter(|g| g.window.is_none()) {
        if let Some(limit) = g.model.as_ref().and_then(|m| observed.get(m)).copied() {
            g.window = Some(limit);
            g.window_basis = Some(WindowBasis::ObservedCompaction);
            g.near_limit = g.peak_tokens * 100 >= limit * NEAR_LIMIT_PERCENT;
        }
    }
    out.sort_by(|a, b| {
        b.peak_tokens
            .cmp(&a.peak_tokens)
            .then_with(|| a.session.cmp(&b.session))
    });
    GrowthReport { sessions: out }
}

/// Calls of one tool and how many failed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolErrors {
    pub source: Source,
    pub tool: String,
    pub calls: u64,
    pub errors: u64,
    /// Calls whose outcome the client did not record; never counted as errors.
    pub unknown: u64,
    /// Retry loops of this tool.
    pub loops: u64,
}

/// Three or more adjacent calls of the same tool with the same input summary,
/// the first two of which failed. Calls without an input summary cannot be
/// compared and never form a loop.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetryLoop {
    pub source: Source,
    pub session: String,
    pub tool: String,
    pub input: Option<String>,
    pub calls: u64,
    pub errors: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorReport {
    /// Sorted by errors, largest first.
    pub tools: Vec<ToolErrors>,
    /// Sorted by calls, largest first.
    pub loops: Vec<RetryLoop>,
}

/// Calls in a row needed to call it a loop.
pub const LOOP_MIN_CALLS: u64 = 3;

pub fn errors(sessions: &[SessionEvents]) -> ErrorReport {
    let mut tools: BTreeMap<(Source, String), ToolErrors> = BTreeMap::new();
    let mut loops = Vec::new();
    for se in sessions {
        let source = se.session.source;
        // The current run of identical adjacent calls: (tool, input, calls, errors, leading failures).
        let mut run: Option<(String, Option<String>, u64, u64, u64)> = None;
        let flush = |run: &mut Option<(String, Option<String>, u64, u64, u64)>,
                     tools: &mut BTreeMap<(Source, String), ToolErrors>,
                     loops: &mut Vec<RetryLoop>| {
            if let Some((tool, input, calls, errs, leading)) = run.take()
                && calls >= LOOP_MIN_CALLS
                && leading >= 2
                && input.is_some()
            {
                if let Some(t) = tools.get_mut(&(source, tool.clone())) {
                    t.loops += 1;
                }
                loops.push(RetryLoop {
                    source,
                    session: se.session.id.clone(),
                    tool,
                    input,
                    calls,
                    errors: errs,
                });
            }
        };
        for e in &se.events {
            let EventKind::ToolCall(c) = &e.kind else {
                continue;
            };
            let t = tools
                .entry((source, c.name.clone()))
                .or_insert_with(|| ToolErrors {
                    source,
                    tool: c.name.clone(),
                    calls: 0,
                    errors: 0,
                    unknown: 0,
                    loops: 0,
                });
            t.calls += 1;
            match c.status {
                ToolStatus::Error => t.errors += 1,
                ToolStatus::Unknown => t.unknown += 1,
                ToolStatus::Ok => {}
            }
            let failed = c.status == ToolStatus::Error;
            match &mut run {
                Some((tool, input, calls, errs, leading))
                    if *tool == c.name && *input == c.input_summary =>
                {
                    if failed && *leading == *calls {
                        *leading += 1;
                    }
                    *calls += 1;
                    *errs += u64::from(failed);
                }
                _ => {
                    flush(&mut run, &mut tools, &mut loops);
                    run = Some((
                        c.name.clone(),
                        c.input_summary.clone(),
                        1,
                        u64::from(failed),
                        u64::from(failed),
                    ));
                }
            }
        }
        flush(&mut run, &mut tools, &mut loops);
    }
    let mut tools: Vec<ToolErrors> = tools.into_values().collect();
    tools.sort_by(|a, b| {
        b.errors
            .cmp(&a.errors)
            .then_with(|| b.calls.cmp(&a.calls))
            .then_with(|| a.tool.cmp(&b.tool))
    });
    loops.sort_by(|a, b| {
        b.calls
            .cmp(&a.calls)
            .then_with(|| a.session.cmp(&b.session))
    });
    ErrorReport { tools, loops }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Session, Usage};

    fn request(total: u64, subagent: bool) -> EventKind {
        EventKind::Usage(Usage {
            input: total / 2,
            cache_read: total - total / 2 - 1,
            cache_write: 1,
            subagent,
            measured: true,
            ..Usage::default()
        })
    }

    fn session(id: &str, model: Option<&str>, kinds: Vec<EventKind>) -> SessionEvents {
        let mut s = Session::new(id, Source::ClaudeCode);
        s.model = model.map(Into::into);
        let mut se = SessionEvents::new(s);
        for k in kinds {
            se.push(None, k);
        }
        se
    }

    #[test]
    fn growth_tracks_first_last_peak_and_the_size_before_compactions() {
        let se = session(
            "a",
            Some("claude-test"),
            vec![
                request(1000, false),
                request(5000, true), // subagent requests are not the main context
                request(4000, false),
                request(9000, false),
                EventKind::Compaction,
                request(2000, false),
            ],
        );
        let g = &growth(&[se]).sessions[0];
        assert_eq!(g.requests, 4);
        assert_eq!(
            (g.first_tokens, g.last_tokens, g.peak_tokens),
            (1000, 2000, 9000)
        );
        assert_eq!(g.compactions, [9000]);
        assert!(!g.near_limit);
    }

    #[test]
    fn near_limit_needs_a_known_window_and_ninety_percent() {
        let near = session("near", Some("claude-test"), vec![request(180_000, false)]);
        let below = session("below", Some("claude-test"), vec![request(179_999, false)]);
        let unknown = session(
            "unknown",
            Some("other-model"),
            vec![request(900_000, false)],
        );
        let larger = session("larger", Some("claude-test"), vec![request(500_000, false)]);
        let r = growth(&[near, below, unknown, larger]);
        let flag = |id: &str| {
            r.sessions
                .iter()
                .find(|s| s.session == id)
                .unwrap()
                .near_limit
        };
        assert!(flag("near"));
        assert!(!flag("below"));
        assert!(
            !flag("unknown"),
            "no documented window and no compaction seen"
        );
        assert!(
            !flag("larger"),
            "a request above the window means a larger variant"
        );
        assert_eq!(r.sessions[0].session, "unknown", "sorted by peak");
    }

    #[test]
    fn without_a_fitting_window_the_largest_compaction_of_the_model_is_the_limit() {
        let compacted = session(
            "compacted",
            Some("big-model"),
            vec![
                request(900_000, false),
                EventKind::Compaction,
                request(100, false),
            ],
        );
        let near = session("near", Some("big-model"), vec![request(820_000, false)]);
        let far = session("far", Some("big-model"), vec![request(500_000, false)]);
        let other = session("other", Some("other-model"), vec![request(990_000, false)]);
        let r = growth(&[compacted, near, far, other]);
        let get = |id: &str| r.sessions.iter().find(|s| s.session == id).unwrap();
        assert_eq!(get("near").window, Some(900_000));
        assert_eq!(
            get("near").window_basis,
            Some(WindowBasis::ObservedCompaction)
        );
        assert!(get("near").near_limit);
        assert!(!get("far").near_limit);
        assert!(!get("other").near_limit && get("other").window.is_none());
    }

    #[test]
    fn model_falls_back_to_the_first_usage_and_empty_sessions_are_skipped() {
        let usage = Usage {
            model: Some("claude-x".into()),
            input: 10,
            ..Usage::default()
        };
        let with = session("with", None, vec![EventKind::Usage(usage)]);
        let empty = session("empty", None, vec![EventKind::UserTurn]);
        let r = growth(&[with, empty]);
        assert_eq!(r.sessions.len(), 1);
        assert_eq!(r.sessions[0].model.as_deref(), Some("claude-x"));
    }

    fn call(name: &str, input: Option<&str>, status: ToolStatus) -> EventKind {
        EventKind::ToolCall(crate::model::ToolCall {
            name: name.into(),
            input_summary: input.map(Into::into),
            status,
            duration_ms: None,
        })
    }

    #[test]
    fn error_rate_counts_unknown_separately() {
        use ToolStatus::*;
        let se = session(
            "a",
            None,
            vec![
                call("Read", Some("x"), Ok),
                call("Read", Some("y"), Error),
                call("Read", Some("z"), Unknown),
                call("Bash", Some("ls"), Ok),
            ],
        );
        let r = errors(&[se]);
        let read = r.tools.iter().find(|t| t.tool == "Read").unwrap();
        assert_eq!((read.calls, read.errors, read.unknown), (3, 1, 1));
        assert_eq!(r.tools[0].tool, "Read", "sorted by errors");
        assert!(r.loops.is_empty());
    }

    #[test]
    fn a_loop_needs_three_adjacent_identical_calls_and_two_leading_failures() {
        use ToolStatus::*;
        let looped = session(
            "loop",
            None,
            vec![
                call("Bash", Some("make"), Error),
                call("Bash", Some("make"), Error),
                call("Bash", Some("make"), Ok),
                call("Read", Some("a"), Ok),
            ],
        );
        let two = session(
            "two",
            None,
            vec![
                call("Bash", Some("make"), Error),
                call("Bash", Some("make"), Ok),
            ],
        );
        let succeeds = session(
            "fine",
            None,
            vec![
                call("Read", Some("a"), Ok),
                call("Read", Some("a"), Ok),
                call("Read", Some("a"), Ok),
            ],
        );
        let interrupted = session(
            "interrupted",
            None,
            vec![
                call("Bash", Some("make"), Error),
                call("Read", Some("a"), Ok),
                call("Bash", Some("make"), Error),
                call("Bash", Some("make"), Error),
            ],
        );
        let one_failure = session(
            "one",
            None,
            vec![
                call("Bash", Some("make"), Error),
                call("Bash", Some("make"), Ok),
                call("Bash", Some("make"), Ok),
            ],
        );
        let no_input = session(
            "noinput",
            None,
            vec![
                call("click", None, Error),
                call("click", None, Error),
                call("click", None, Error),
            ],
        );
        let r = errors(&[looped, two, succeeds, interrupted, one_failure, no_input]);
        assert_eq!(r.loops.len(), 1);
        let l = &r.loops[0];
        assert_eq!(
            (l.session.as_str(), l.tool.as_str(), l.calls, l.errors),
            ("loop", "Bash", 3, 2)
        );
        assert_eq!(l.input.as_deref(), Some("make"));
        let bash = r.tools.iter().find(|t| t.tool == "Bash").unwrap();
        assert_eq!(bash.loops, 1);
    }
}
