//! The clients side by side over the same sessions.
//!
//! Joins what the other analyzers measure per source. A value a client does not
//! record is `None`, never 0.

use std::collections::BTreeMap;

use jiff::Timestamp;
use serde::{Deserialize, Serialize};

use crate::model::{EventKind, SessionEvents, Source, ToolStatus};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceStats {
    pub source: Source,
    pub sessions: u64,
    /// Main-agent and subagent requests; `None` if no usage was recorded.
    pub requests: Option<u64>,
    /// All input tokens: uncached, cache read and cache write.
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    /// `None` if the source reports no cache tokens.
    pub cache_hit_rate: Option<f64>,
    pub tool_calls: u64,
    pub tool_errors: u64,
    /// `None` if no tool call has a recorded outcome.
    pub tool_error_rate: Option<f64>,
    pub skill_invocations: u64,
    /// Estimated baseline + load + carry of the skills (from the skill analysis).
    pub skill_overhead_tokens: Option<u64>,
    /// Sum of the time between the first and last event of each session.
    pub total_duration_ms: Option<u64>,
    /// Mean over the sessions that have timestamps.
    pub mean_session_ms: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CompareReport {
    /// Sources with at least one session, in source order.
    pub sources: Vec<SourceStats>,
}

/// `skill_overhead` holds the skill overhead per source if it was computed.
pub fn analyze(
    sessions: &[SessionEvents],
    skill_overhead: &BTreeMap<Source, u64>,
) -> CompareReport {
    #[derive(Default)]
    struct Acc {
        sessions: u64,
        requests: u64,
        input: u64,
        output: u64,
        cache_read: u64,
        cache_write: u64,
        calls: u64,
        errors: u64,
        known: u64,
        skills: u64,
        duration: u64,
        timed: u64,
    }
    let mut accs: BTreeMap<Source, Acc> = BTreeMap::new();
    for se in sessions {
        let a = accs.entry(se.session.source).or_default();
        a.sessions += 1;
        let (mut first, mut last): (Option<Timestamp>, Option<Timestamp>) = (None, None);
        for e in &se.events {
            if let Some(at) = e.at {
                first = Some(first.map_or(at, |f| f.min(at)));
                last = Some(last.map_or(at, |l| l.max(at)));
            }
            match &e.kind {
                EventKind::Usage(u) => {
                    a.requests += 1;
                    a.input += u.total_input();
                    a.output += u.output;
                    a.cache_read += u.cache_read;
                    a.cache_write += u.cache_write;
                }
                EventKind::ToolCall(c) => {
                    a.calls += 1;
                    match c.status {
                        ToolStatus::Error => {
                            a.errors += 1;
                            a.known += 1;
                        }
                        ToolStatus::Ok => a.known += 1,
                        ToolStatus::Unknown => {}
                    }
                }
                EventKind::SkillInvoked(_) => a.skills += 1,
                _ => {}
            }
        }
        if let (Some(f), Some(l)) = (first, last) {
            a.duration += u64::try_from(l.as_millisecond() - f.as_millisecond()).unwrap_or(0);
            a.timed += 1;
        }
    }
    let sources = accs
        .into_iter()
        .map(|(source, a)| {
            let cache = a.cache_read + a.cache_write > 0;
            SourceStats {
                source,
                sessions: a.sessions,
                requests: (a.requests > 0).then_some(a.requests),
                input_tokens: (a.requests > 0).then_some(a.input),
                output_tokens: (a.requests > 0).then_some(a.output),
                cache_hit_rate: (cache && a.input > 0)
                    .then(|| a.cache_read as f64 / a.input as f64),
                tool_calls: a.calls,
                tool_errors: a.errors,
                tool_error_rate: (a.known > 0).then(|| a.errors as f64 / a.known as f64),
                skill_invocations: a.skills,
                skill_overhead_tokens: skill_overhead.get(&source).copied(),
                total_duration_ms: (a.timed > 0).then_some(a.duration),
                mean_session_ms: (a.timed > 0).then(|| a.duration / a.timed),
            }
        })
        .collect();
    CompareReport { sources }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Session, SkillInvoked, SkillTrigger, ToolCall, Usage};

    fn at(s: &str) -> Option<Timestamp> {
        Some(s.parse().unwrap())
    }

    fn call(status: ToolStatus) -> EventKind {
        EventKind::ToolCall(ToolCall {
            name: "t".into(),
            input_summary: None,
            status,
            duration_ms: None,
        })
    }

    fn usage(input: u64, read: u64, write: u64, output: u64) -> EventKind {
        EventKind::Usage(Usage {
            input,
            cache_read: read,
            cache_write: write,
            output,
            ..Usage::default()
        })
    }

    #[test]
    fn two_sources_are_compared_with_hand_computed_values() {
        let mut a = SessionEvents::new(Session::new("a", Source::ClaudeCode));
        a.push(at("2026-10-01T10:00:00Z"), EventKind::UserTurn);
        a.push(None, usage(100, 600, 300, 50));
        a.push(None, call(ToolStatus::Ok));
        a.push(None, call(ToolStatus::Error));
        a.push(None, call(ToolStatus::Unknown));
        a.push(
            at("2026-10-01T10:10:00Z"),
            EventKind::SkillInvoked(SkillInvoked::new("s", SkillTrigger::Model)),
        );
        let mut b = SessionEvents::new(Session::new("b", Source::ClaudeCode));
        b.push(at("2026-10-02T09:00:00Z"), EventKind::UserTurn);
        b.push(at("2026-10-02T09:02:00Z"), usage(1000, 0, 0, 10));
        let mut c = SessionEvents::new(Session::new("c", Source::OpenCode));
        c.push(None, usage(500, 0, 0, 5));
        c.push(None, call(ToolStatus::Unknown));

        let overhead = BTreeMap::from([(Source::ClaudeCode, 777)]);
        let r = analyze(&[a, b, c], &overhead);
        assert_eq!(r.sources.len(), 2);
        let cc = &r.sources[0];
        assert_eq!(cc.source, Source::ClaudeCode);
        assert_eq!((cc.sessions, cc.requests), (2, Some(2)));
        assert_eq!((cc.input_tokens, cc.output_tokens), (Some(2000), Some(60)));
        assert!((cc.cache_hit_rate.unwrap() - 600.0 / 2000.0).abs() < 1e-9);
        assert_eq!((cc.tool_calls, cc.tool_errors), (3, 1));
        assert!(
            (cc.tool_error_rate.unwrap() - 0.5).abs() < 1e-9,
            "unknown outcomes do not count"
        );
        assert_eq!(cc.skill_invocations, 1);
        assert_eq!(cc.skill_overhead_tokens, Some(777));
        // 10 minutes and 2 minutes.
        assert_eq!(cc.total_duration_ms, Some(720_000));
        assert_eq!(cc.mean_session_ms, Some(360_000));
    }

    #[test]
    fn missing_data_is_none_not_zero() {
        let mut c = SessionEvents::new(Session::new("c", Source::OpenCode));
        c.push(None, usage(500, 0, 0, 5));
        c.push(None, call(ToolStatus::Unknown));
        let r = analyze(&[c], &BTreeMap::new());
        let s = &r.sources[0];
        assert_eq!(s.cache_hit_rate, None);
        assert_eq!(s.tool_error_rate, None);
        assert_eq!(s.skill_overhead_tokens, None);
        assert_eq!((s.total_duration_ms, s.mean_session_ms), (None, None));
        assert_eq!(s.tool_calls, 1);
    }

    #[test]
    fn sources_without_usage_show_no_token_counts() {
        let mut c = SessionEvents::new(Session::new("c", Source::CopilotCli));
        c.push(None, call(ToolStatus::Ok));
        let s = &analyze(&[c], &BTreeMap::new()).sources[0];
        assert_eq!(
            (s.requests, s.input_tokens, s.output_tokens),
            (None, None, None)
        );
    }
}
