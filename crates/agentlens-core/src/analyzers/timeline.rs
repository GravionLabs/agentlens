//! One session over time: the context size of every main-agent request and
//! markers for what happened in between.

use std::path::PathBuf;

use jiff::Timestamp;
use serde::{Deserialize, Serialize};

use crate::model::{EventKind, SessionEvents, SkillTrigger, Source, ToolStatus};
use crate::prices::PriceTable;

/// A timeline never holds more points than this; longer sessions are thinned.
pub const MAX_POINTS: usize = 400;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionSummary {
    pub source: Source,
    pub id: String,
    pub project: Option<PathBuf>,
    pub model: Option<String>,
    pub started_at: Option<Timestamp>,
    pub ended_at: Option<Timestamp>,
    /// Main-agent requests.
    pub requests: u64,
    /// Largest context size of a main-agent request.
    pub peak_tokens: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Point {
    /// Position among the main-agent requests, starting at 0.
    pub request: u64,
    pub at: Option<Timestamp>,
    /// Input plus cache read plus cache write; measured.
    pub context_tokens: u64,
    pub output_tokens: u64,
    /// Cost of the request if the client reported it or the price table knows the model.
    pub cost_usd: Option<f64>,
    /// `true` if the cost was computed from the price table.
    pub cost_estimated: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum MarkerKind {
    Skill { name: String, trigger: SkillTrigger },
    Compaction,
    ToolError { tool: String },
    HookFailed { hook: String },
    PermissionPrompt { wait_ms: Option<u64> },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Marker {
    /// Number of main-agent requests before the marker: it sits between
    /// request `request - 1` and request `request`.
    pub request: u64,
    pub at: Option<Timestamp>,
    pub kind: MarkerKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Timeline {
    pub session: SessionSummary,
    pub points: Vec<Point>,
    pub markers: Vec<Marker>,
    /// Number of requests before thinning, if points were dropped.
    pub thinned_from: Option<u64>,
}

pub fn summary(se: &SessionEvents) -> SessionSummary {
    let (mut requests, mut peak) = (0, 0);
    for e in &se.events {
        if let EventKind::Usage(u) = &e.kind
            && !u.subagent
        {
            requests += 1;
            peak = peak.max(u.total_input());
        }
    }
    let times = se.events.iter().filter_map(|e| e.at);
    SessionSummary {
        source: se.session.source,
        id: se.session.id.clone(),
        project: se.session.project_dir.clone(),
        model: se.session.model.clone(),
        started_at: se.session.started_at.or_else(|| times.clone().min()),
        ended_at: se.session.ended_at.or_else(|| times.max()),
        requests,
        peak_tokens: peak,
    }
}

/// All sessions, most recent first; sessions without requests are left out.
pub fn list(sessions: &[SessionEvents]) -> Vec<SessionSummary> {
    let mut out: Vec<SessionSummary> = sessions
        .iter()
        .map(summary)
        .filter(|s| s.requests > 0)
        .collect();
    out.sort_by(|a, b| b.ended_at.cmp(&a.ended_at).then_with(|| a.id.cmp(&b.id)));
    out
}

pub fn timeline(se: &SessionEvents, prices: &PriceTable) -> Timeline {
    let mut points = Vec::new();
    let mut markers = Vec::new();
    for e in &se.events {
        let request = points.len() as u64;
        match &e.kind {
            EventKind::Usage(u) if !u.subagent => {
                let mut usage = u.clone();
                if usage.model.is_none() {
                    usage.model = se.session.model.clone();
                }
                let cost = prices.cost(&usage);
                points.push(Point {
                    request,
                    at: e.at,
                    context_tokens: u.total_input(),
                    output_tokens: u.output,
                    cost_usd: cost.map(|c| c.usd),
                    cost_estimated: cost.is_some_and(|c| !c.measured),
                });
            }
            EventKind::SkillInvoked(s) => markers.push(Marker {
                request,
                at: e.at,
                kind: MarkerKind::Skill {
                    name: s.name.clone(),
                    trigger: s.trigger,
                },
            }),
            EventKind::Compaction => markers.push(Marker {
                request,
                at: e.at,
                kind: MarkerKind::Compaction,
            }),
            EventKind::ToolCall(c) if c.status == ToolStatus::Error => markers.push(Marker {
                request,
                at: e.at,
                kind: MarkerKind::ToolError {
                    tool: c.name.clone(),
                },
            }),
            EventKind::Hook(h) if h.success == Some(false) => markers.push(Marker {
                request,
                at: e.at,
                kind: MarkerKind::HookFailed {
                    hook: h.kind.clone(),
                },
            }),
            EventKind::PermissionPrompt(p) => markers.push(Marker {
                request,
                at: e.at,
                kind: MarkerKind::PermissionPrompt { wait_ms: p.wait_ms },
            }),
            _ => {}
        }
    }

    let total = points.len() as u64;
    let thinned_from = (points.len() > MAX_POINTS).then(|| {
        points = thin(&points, MAX_POINTS);
        total
    });
    Timeline {
        session: summary(se),
        points,
        markers,
        thinned_from,
    }
}

/// Keeps `max` points: the first, the last and the largest context size of
/// every bucket in between, so peaks survive.
fn thin(points: &[Point], max: usize) -> Vec<Point> {
    let buckets = max - 2;
    let inner = &points[1..points.len() - 1];
    let size = inner.len().div_ceil(buckets);
    let mut out = vec![points[0].clone()];
    for chunk in inner.chunks(size) {
        if let Some(p) = chunk.iter().max_by_key(|p| p.context_tokens) {
            out.push(p.clone());
        }
    }
    out.push(points[points.len() - 1].clone());
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Hook, PermissionPrompt, Session, SkillInvoked, ToolCall, Usage};

    fn at(s: &str) -> Option<Timestamp> {
        Some(s.parse().unwrap())
    }

    fn request(total: u64, subagent: bool) -> EventKind {
        EventKind::Usage(Usage {
            model: Some("claude-sonnet-5-5".into()),
            input: total,
            output: 10,
            subagent,
            ..Usage::default()
        })
    }

    fn session(id: &str, events: Vec<(Option<Timestamp>, EventKind)>) -> SessionEvents {
        let mut se = SessionEvents::new(Session::new(id, Source::ClaudeCode));
        for (t, k) in events {
            se.push(t, k);
        }
        se
    }

    #[test]
    fn points_and_markers_follow_the_main_requests() {
        let se = session(
            "a",
            vec![
                (at("2026-10-01T10:00:00Z"), EventKind::UserTurn),
                (at("2026-10-01T10:00:01Z"), request(1000, false)),
                (at("2026-10-01T10:00:02Z"), request(5000, true)),
                (
                    at("2026-10-01T10:00:03Z"),
                    EventKind::SkillInvoked(SkillInvoked::new("demo", SkillTrigger::Model)),
                ),
                (
                    at("2026-10-01T10:00:04Z"),
                    EventKind::ToolCall(ToolCall {
                        name: "Bash".into(),
                        input_summary: None,
                        status: ToolStatus::Error,
                        duration_ms: None,
                    }),
                ),
                (at("2026-10-01T10:00:05Z"), request(2000, false)),
                (at("2026-10-01T10:00:06Z"), EventKind::Compaction),
                (
                    at("2026-10-01T10:00:07Z"),
                    EventKind::Hook(Hook {
                        kind: "post".into(),
                        success: Some(false),
                        duration_ms: None,
                    }),
                ),
                (
                    at("2026-10-01T10:00:08Z"),
                    EventKind::PermissionPrompt(PermissionPrompt { wait_ms: Some(900) }),
                ),
                (at("2026-10-01T10:00:09Z"), request(300, false)),
            ],
        );
        let t = timeline(&se, &PriceTable::bundled());
        assert_eq!(
            t.points
                .iter()
                .map(|p| (p.request, p.context_tokens))
                .collect::<Vec<_>>(),
            [(0, 1000), (1, 2000), (2, 300)]
        );
        let at_request: Vec<u64> = t.markers.iter().map(|m| m.request).collect();
        assert_eq!(at_request, [1, 1, 2, 2, 2]);
        assert!(matches!(&t.markers[0].kind, MarkerKind::Skill { name, .. } if name == "demo"));
        assert!(matches!(&t.markers[1].kind, MarkerKind::ToolError { tool } if tool == "Bash"));
        assert!(matches!(t.markers[2].kind, MarkerKind::Compaction));
        assert!(matches!(&t.markers[3].kind, MarkerKind::HookFailed { hook } if hook == "post"));
        assert!(matches!(
            t.markers[4].kind,
            MarkerKind::PermissionPrompt { wait_ms: Some(900) }
        ));
        assert_eq!(t.thinned_from, None);
        // Sonnet 5.5: $2 per million input tokens, $10 output.
        let c = t.points[0].cost_usd.unwrap();
        assert!((c - (1000.0 * 2.0 + 10.0 * 10.0) / 1e6).abs() < 1e-12);
        assert!(t.points[0].cost_estimated);
    }

    #[test]
    fn reported_cost_is_not_marked_estimated_and_unknown_models_have_none() {
        let reported = EventKind::Usage(Usage {
            cost_usd: Some(0.5),
            ..Usage::default()
        });
        let unknown = EventKind::Usage(Usage {
            model: Some("mystery".into()),
            input: 5,
            ..Usage::default()
        });
        let t = timeline(
            &session("a", vec![(None, reported), (None, unknown)]),
            &PriceTable::bundled(),
        );
        assert_eq!(
            (t.points[0].cost_usd, t.points[0].cost_estimated),
            (Some(0.5), false)
        );
        assert_eq!(t.points[1].cost_usd, None);
    }

    #[test]
    fn long_sessions_are_thinned_but_keep_first_last_and_the_peak() {
        let n = MAX_POINTS * 5 + 17;
        let mut events = Vec::new();
        for i in 0..n {
            let size = if i == 777 { 9_999_999 } else { 1000 + i as u64 };
            events.push((None, request(size, false)));
        }
        let t = timeline(&session("big", events), &PriceTable::bundled());
        assert_eq!(t.thinned_from, Some(n as u64));
        assert!(t.points.len() <= MAX_POINTS, "{}", t.points.len());
        assert_eq!(t.points.first().unwrap().request, 0);
        assert_eq!(t.points.last().unwrap().request, n as u64 - 1);
        assert!(t.points.iter().any(|p| p.context_tokens == 9_999_999));
        assert!(t.points.windows(2).all(|w| w[0].request < w[1].request));
        assert_eq!(
            t.session.requests, n as u64,
            "the summary counts every request"
        );
    }

    #[test]
    fn the_list_is_most_recent_first_and_skips_sessions_without_requests() {
        let old = session(
            "old",
            vec![(at("2026-09-01T10:00:00Z"), request(10, false))],
        );
        let new = session(
            "new",
            vec![(at("2026-10-01T10:00:00Z"), request(20, false))],
        );
        let idle = session(
            "idle",
            vec![(at("2026-10-05T10:00:00Z"), EventKind::UserTurn)],
        );
        let l = list(&[old, new, idle]);
        assert_eq!(
            l.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
            ["new", "old"]
        );
        assert_eq!(l[0].peak_tokens, 20);
    }
}
