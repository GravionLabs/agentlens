//! How sessions unfold: context growth and compactions, tool errors, hooks.
//!
//! Everything here is computed from numbers the clients report, so the values
//! are measured. Context size is the total input of a main-agent request
//! (uncached input plus cache reads and writes).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use std::collections::BTreeMap;

use crate::model::{Event, EventKind, SessionEvents, Source, ToolStatus};

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

/// A retry loop in one session's events: the indices of its first and last call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LoopSpan {
    pub first: usize,
    pub last: usize,
    pub tool: String,
    pub input: Option<String>,
    pub calls: u64,
    pub errors: u64,
}

/// The retry loops of one session: [`LOOP_MIN_CALLS`] or more adjacent calls (other events in between do
/// not break the run) of the same tool with the same input summary, the first two of which failed.
pub(crate) fn retry_loops(events: &[Event]) -> Vec<LoopSpan> {
    // The current run of identical adjacent calls and the number of its leading failures.
    let mut run: Option<(LoopSpan, u64)> = None;
    let mut loops = Vec::new();
    let flush = |run: &mut Option<(LoopSpan, u64)>, loops: &mut Vec<LoopSpan>| {
        if let Some((span, leading)) = run.take()
            && span.calls >= LOOP_MIN_CALLS
            && leading >= 2
            && span.input.is_some()
        {
            loops.push(span);
        }
    };
    for (i, e) in events.iter().enumerate() {
        let EventKind::ToolCall(c) = &e.kind else {
            continue;
        };
        let failed = c.status == ToolStatus::Error;
        match &mut run {
            Some((span, leading)) if span.tool == c.name && span.input == c.input_summary => {
                if failed && *leading == span.calls {
                    *leading += 1;
                }
                span.calls += 1;
                span.errors += u64::from(failed);
                span.last = i;
            }
            _ => {
                flush(&mut run, &mut loops);
                run = Some((
                    LoopSpan {
                        first: i,
                        last: i,
                        tool: c.name.clone(),
                        input: c.input_summary.clone(),
                        calls: 1,
                        errors: u64::from(failed),
                    },
                    u64::from(failed),
                ));
            }
        }
    }
    flush(&mut run, &mut loops);
    loops
}

pub fn errors(sessions: &[SessionEvents]) -> ErrorReport {
    let mut tools: BTreeMap<(Source, String), ToolErrors> = BTreeMap::new();
    let mut loops = Vec::new();
    for se in sessions {
        let source = se.session.source;
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
        }
        for span in retry_loops(&se.events) {
            if let Some(t) = tools.get_mut(&(source, span.tool.clone())) {
                t.loops += 1;
            }
            loops.push(RetryLoop {
                source,
                session: se.session.id.clone(),
                tool: span.tool,
                input: span.input,
                calls: span.calls,
                errors: span.errors,
            });
        }
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

/// Runs of one hook type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HookStats {
    pub source: Source,
    pub hook: String,
    pub runs: u64,
    pub failures: u64,
    /// Runs that never reported an outcome.
    pub unknown: u64,
    /// Summed duration of the runs that reported one.
    pub total_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionStats {
    pub source: Source,
    pub prompts: u64,
    /// Prompts with a recorded answer.
    pub answered: u64,
    /// Summed time until the answers.
    pub total_wait_ms: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HooksReport {
    /// Sorted by total runtime, largest first.
    pub hooks: Vec<HookStats>,
    pub permissions: Vec<PermissionStats>,
    /// Sessions per source whose client records neither hooks nor permission
    /// prompts at all.
    pub unavailable: BTreeMap<Source, u32>,
}

pub fn hooks(sessions: &[SessionEvents]) -> HooksReport {
    let mut hooks: BTreeMap<(Source, String), HookStats> = BTreeMap::new();
    let mut perms: BTreeMap<Source, PermissionStats> = BTreeMap::new();
    let mut session_count: BTreeMap<Source, u32> = BTreeMap::new();
    for se in sessions {
        let source = se.session.source;
        *session_count.entry(source).or_default() += 1;
        for e in &se.events {
            match &e.kind {
                EventKind::Hook(h) => {
                    let st = hooks
                        .entry((source, h.kind.clone()))
                        .or_insert_with(|| HookStats {
                            source,
                            hook: h.kind.clone(),
                            runs: 0,
                            failures: 0,
                            unknown: 0,
                            total_ms: 0,
                        });
                    st.runs += 1;
                    match h.success {
                        Some(false) => st.failures += 1,
                        None => st.unknown += 1,
                        Some(true) => {}
                    }
                    st.total_ms += h.duration_ms.unwrap_or(0);
                }
                EventKind::PermissionPrompt(p) => {
                    let st = perms.entry(source).or_insert(PermissionStats {
                        source,
                        prompts: 0,
                        answered: 0,
                        total_wait_ms: 0,
                    });
                    st.prompts += 1;
                    if let Some(w) = p.wait_ms {
                        st.answered += 1;
                        st.total_wait_ms += w;
                    }
                }
                _ => {}
            }
        }
    }
    let unavailable = session_count
        .into_iter()
        .filter(|(s, _)| !perms.contains_key(s) && !hooks.keys().any(|(hs, _)| hs == s))
        .collect();
    let mut hooks: Vec<HookStats> = hooks.into_values().collect();
    hooks.sort_by(|a, b| {
        b.total_ms
            .cmp(&a.total_ms)
            .then_with(|| a.hook.cmp(&b.hook))
    });
    HooksReport {
        hooks,
        permissions: perms.into_values().collect(),
        unavailable,
    }
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

    #[test]
    fn hooks_and_permissions_are_summed_per_source() {
        use crate::model::{Hook, PermissionPrompt};
        let hook = |kind: &str, success: Option<bool>, ms: Option<u64>| {
            EventKind::Hook(Hook {
                kind: kind.into(),
                success,
                duration_ms: ms,
            })
        };
        let mut a = SessionEvents::new(Session::new("a", Source::CopilotCli));
        for k in [
            hook("post", Some(true), Some(100)),
            hook("post", Some(false), Some(300)),
            hook("post", None, None),
            hook("pre", Some(true), Some(1000)),
            EventKind::PermissionPrompt(PermissionPrompt {
                wait_ms: Some(1500),
            }),
            EventKind::PermissionPrompt(PermissionPrompt { wait_ms: None }),
        ] {
            a.push(None, k);
        }
        let claude = session("c", None, vec![EventKind::UserTurn]);
        let r = hooks(&[a, claude.clone(), claude]);
        assert_eq!(r.hooks[0].hook, "pre", "sorted by runtime");
        let post = &r.hooks[1];
        assert_eq!(
            (post.runs, post.failures, post.unknown, post.total_ms),
            (3, 1, 1, 400)
        );
        let p = &r.permissions[0];
        assert_eq!((p.prompts, p.answered, p.total_wait_ms), (2, 1, 1500));
        assert_eq!(r.unavailable.get(&Source::ClaudeCode), Some(&2));
        assert!(!r.unavailable.contains_key(&Source::CopilotCli));
    }

    /// Reads a fixture of the adapter tests through its adapter, laid out below a temporary home.
    fn read_fixture(
        adapter: &dyn crate::adapter::Adapter,
        fixture: &str,
        rel: &str,
    ) -> SessionEvents {
        let home = tempfile::tempdir().unwrap();
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(fixture);
        let dest = home.path().join(rel);
        std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
        std::fs::copy(&src, &dest).unwrap();
        let env = crate::env::Env::for_home(home.path());
        adapter
            .sessions(&env, &crate::adapter::SessionFilter::default())
            .map(|r| r.unwrap())
            .next()
            .unwrap()
    }

    #[test]
    fn the_claude_code_fixture_holds_one_loop_of_three_builds() {
        let se = read_fixture(
            &crate::adapters::claude_code::ClaudeCodeAdapter,
            "claude_code/retry_loop.jsonl",
            ".claude/projects/-work-demo/retry_loop.jsonl",
        );
        let spans = retry_loops(&se.events);
        assert_eq!(spans.len(), 1);
        let l = &spans[0];
        assert_eq!(
            (l.tool.as_str(), l.input.as_deref(), l.calls, l.errors),
            ("Bash", Some("cargo"), 3, 2)
        );
        assert!(l.first < l.last);
        assert!(matches!(se.events[l.first].kind, EventKind::ToolCall(_)));
        assert!(matches!(se.events[l.last].kind, EventKind::ToolCall(_)));
    }

    #[test]
    fn the_copilot_fixture_holds_one_loop_with_a_failing_hook_inside() {
        let se = read_fixture(
            &crate::adapters::copilot_cli::CopilotCliAdapter,
            "copilot_cli/retry_loop.jsonl",
            ".copilot/session-state/cp-loop/events.jsonl",
        );
        let spans = retry_loops(&se.events);
        assert_eq!(spans.len(), 1);
        let l = &spans[0];
        assert_eq!(
            (l.tool.as_str(), l.input.as_deref(), l.calls, l.errors),
            ("bash", Some("npm"), 3, 3)
        );
        let failed_hooks = se.events[l.first..=l.last]
            .iter()
            .filter(|e| {
                matches!(&e.kind, EventKind::Hook(h) if h.kind == "preToolUse" && h.success == Some(false))
            })
            .count();
        assert_eq!(failed_hooks, 1);
    }

    #[test]
    fn spans_are_event_indices_and_other_events_do_not_break_a_run() {
        let call = |status| {
            EventKind::ToolCall(crate::model::ToolCall {
                name: "Bash".into(),
                input_summary: Some("make".into()),
                status,
                duration_ms: None,
            })
        };
        let se = session(
            "s",
            None,
            vec![
                EventKind::UserTurn,
                call(ToolStatus::Error),
                request(10, false),
                call(ToolStatus::Error),
                request(10, false),
                call(ToolStatus::Ok),
                call(ToolStatus::Ok),
            ],
        );
        let spans = retry_loops(&se.events);
        assert_eq!(spans.len(), 1);
        assert_eq!((spans[0].first, spans[0].last), (1, 6));
        assert_eq!((spans[0].calls, spans[0].errors), (4, 2));
    }
}
