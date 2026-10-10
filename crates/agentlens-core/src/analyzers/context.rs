//! What the client puts into the context besides the conversation.
//!
//! Adapters report static context as `ContextItem` events (sizes only). The cost
//! of an item is its estimated token count times the main-agent requests that
//! followed it: static content is part of every request until the session ends
//! (a compaction does not remove the system prompt, tools or instruction files).
//! Everything here is an estimate except the size of the first request of a
//! session, which the client measured and which bounds the whole static context.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::model::{ContextKind, EventKind, SessionEvents, Source};
use crate::tokens::TokenEstimator;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextRow {
    pub source: Source,
    pub project: Option<PathBuf>,
    pub kind: ContextKind,
    /// MCP server, `built-in` for client tools, `agents`, `skills` or the file path.
    pub owner: String,
    /// Distinct items, counted once per session.
    pub items: u32,
    pub bytes: u64,
    /// Estimated tokens of the items, counted once per session.
    pub tokens: u64,
    /// Sum over the items of the main-agent requests that followed them.
    pub requests: u64,
    /// Estimated tokens x requests.
    pub overhead_tokens: u64,
}

/// Measured size of the first request of each session.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FirstRequests {
    pub sessions: u32,
    pub total_tokens: u64,
    pub max_tokens: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextReport {
    pub sessions: u32,
    /// Sorted by overhead, largest first.
    pub rows: Vec<ContextRow>,
    /// Sessions per source whose client reports no context items.
    pub unavailable: BTreeMap<Source, u32>,
    pub first_request: BTreeMap<Source, FirstRequests>,
}

/// `mcp__<server>__<tool>` belongs to `<server>`; other tools are built in.
pub fn mcp_server(tool: &str) -> Option<&str> {
    let rest = tool.strip_prefix("mcp__")?;
    rest.split("__").next().filter(|s| !s.is_empty())
}

fn owner(kind: ContextKind, name: Option<&str>) -> String {
    match kind {
        ContextKind::ToolListing | ContextKind::ToolDefinition => {
            name.and_then(mcp_server).unwrap_or("built-in").to_owned()
        }
        ContextKind::McpInstructions | ContextKind::InstructionFile => {
            name.unwrap_or("unknown").to_owned()
        }
        ContextKind::AgentListing => "agents".to_owned(),
        ContextKind::SkillListing => "skills".to_owned(),
    }
}

pub fn analyze(sessions: &[SessionEvents], estimator: &dyn TokenEstimator) -> ContextReport {
    type Key = (Source, Option<PathBuf>, ContextKind, String);
    let mut report = ContextReport::default();
    let mut rows: BTreeMap<Key, ContextRow> = BTreeMap::new();

    for se in sessions {
        report.sessions += 1;
        let source = se.session.source;
        // Main-agent requests after each event index.
        let mut later = vec![0u64; se.events.len() + 1];
        for (i, e) in se.events.iter().enumerate().rev() {
            later[i] =
                later[i + 1] + u64::from(matches!(&e.kind, EventKind::Usage(u) if !u.subagent));
        }

        if let Some(u) = se.events.iter().find_map(|e| match &e.kind {
            EventKind::Usage(u) if !u.subagent => Some(u),
            _ => None,
        }) {
            let first = report.first_request.entry(source).or_default();
            first.sessions += 1;
            first.total_tokens += u.total_input();
            first.max_tokens = first.max_tokens.max(u.total_input());
        }

        // Clients announce the same item again (resume, compaction, deltas):
        // it is one piece of context, counted from its first appearance.
        let mut seen: BTreeMap<(ContextKind, Option<&str>), (usize, u64)> = BTreeMap::new();
        for (i, e) in se.events.iter().enumerate() {
            if let EventKind::ContextItem(item) = &e.kind {
                let entry = seen
                    .entry((item.kind, item.name.as_deref()))
                    .or_insert((i, 0));
                entry.1 = entry.1.max(item.bytes);
            }
        }
        let any = !seen.is_empty();
        for ((kind, name), (i, bytes)) in seen {
            let owner = owner(kind, name);
            let tokens = estimator.count_bytes(bytes).count;
            let requests = later[i + 1];
            let row = rows
                .entry((source, se.session.project_dir.clone(), kind, owner.clone()))
                .or_insert_with(|| ContextRow {
                    source,
                    project: se.session.project_dir.clone(),
                    kind,
                    owner,
                    items: 0,
                    bytes: 0,
                    tokens: 0,
                    requests: 0,
                    overhead_tokens: 0,
                });
            row.items += 1;
            row.bytes += bytes;
            row.tokens += tokens;
            row.requests += requests;
            row.overhead_tokens += tokens * requests;
        }
        if !any {
            *report.unavailable.entry(source).or_default() += 1;
        }
    }

    report.rows = rows.into_values().collect();
    report.rows.sort_by(|a, b| {
        b.overhead_tokens
            .cmp(&a.overhead_tokens)
            .then_with(|| a.owner.cmp(&b.owner))
    });
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ContextItem, Session, Usage};
    use crate::tokens::CharsPerToken;

    fn item(kind: ContextKind, name: Option<&str>, bytes: u64) -> EventKind {
        EventKind::ContextItem(ContextItem {
            kind,
            name: name.map(str::to_owned),
            bytes,
        })
    }

    fn request(cache_write: u64, subagent: bool) -> EventKind {
        EventKind::Usage(Usage {
            cache_write,
            subagent,
            measured: true,
            ..Usage::default()
        })
    }

    fn session(source: Source, kinds: Vec<EventKind>) -> SessionEvents {
        let mut s = Session::new("s", source);
        s.project_dir = Some("/work/demo".into());
        let mut se = SessionEvents::new(s);
        for k in kinds {
            se.push(None, k);
        }
        se
    }

    #[test]
    fn overhead_is_tokens_times_later_main_requests() {
        use ContextKind::*;
        let se = session(
            Source::ClaudeCode,
            vec![
                item(InstructionFile, Some("/work/demo/CLAUDE.md"), 400),
                item(ToolListing, Some("mcp__srv__a"), 40),
                item(ToolListing, Some("mcp__srv__b"), 20),
                item(ToolListing, Some("WebFetch"), 8),
                request(1000, false),
                request(50, true), // subagent requests do not count
                request(50, false),
                EventKind::Compaction, // static content survives a compaction
                request(20, false),
            ],
        );
        let r = analyze(&[se], &CharsPerToken);
        let row = |kind, owner: &str| {
            r.rows
                .iter()
                .find(|x| x.kind == kind && x.owner == owner)
                .unwrap()
                .clone()
        };
        let claude = row(InstructionFile, "/work/demo/CLAUDE.md");
        assert_eq!(
            (claude.tokens, claude.requests, claude.overhead_tokens),
            (100, 3, 300)
        );
        let srv = row(ToolListing, "srv");
        assert_eq!((srv.items, srv.tokens), (2, 10 + 5));
        assert_eq!(srv.overhead_tokens, 15 * 3);
        assert_eq!(row(ToolListing, "built-in").tokens, 2);
        assert_eq!(
            r.rows[0].owner, "/work/demo/CLAUDE.md",
            "sorted by overhead"
        );
    }

    #[test]
    fn repeated_announcements_count_once_from_the_first() {
        let se = session(
            Source::ClaudeCode,
            vec![
                item(ContextKind::McpInstructions, Some("srv"), 40),
                request(10, false),
                item(ContextKind::McpInstructions, Some("srv"), 80),
                request(10, false),
            ],
        );
        let r = analyze(&[se], &CharsPerToken);
        let row = &r.rows[0];
        assert_eq!((row.items, row.bytes, row.tokens), (1, 80, 20));
        assert_eq!(row.requests, 2);
        assert_eq!(row.overhead_tokens, 40);
    }

    #[test]
    fn items_only_count_requests_after_them() {
        let se = session(
            Source::ClaudeCode,
            vec![
                request(10, false),
                item(ContextKind::AgentListing, None, 40),
                request(10, false),
            ],
        );
        let r = analyze(&[se], &CharsPerToken);
        assert_eq!(r.rows[0].requests, 1);
        assert_eq!(r.rows[0].owner, "agents");
    }

    #[test]
    fn first_request_is_reported_as_measured_upper_bound() {
        let a = session(
            Source::ClaudeCode,
            vec![request(1000, false), request(5, false)],
        );
        let b = session(Source::ClaudeCode, vec![request(300, false)]);
        let r = analyze(&[a, b], &CharsPerToken);
        let f = &r.first_request[&Source::ClaudeCode];
        assert_eq!((f.sessions, f.total_tokens, f.max_tokens), (2, 1300, 1000));
    }

    #[test]
    fn sources_without_context_items_are_unavailable_not_zero() {
        let with = session(
            Source::ClaudeCode,
            vec![item(ContextKind::SkillListing, None, 8), request(1, false)],
        );
        let without = session(Source::OpenCode, vec![request(1, false)]);
        let r = analyze(&[with, without.clone(), without], &CharsPerToken);
        assert_eq!(r.unavailable.get(&Source::OpenCode), Some(&2));
        assert!(!r.unavailable.contains_key(&Source::ClaudeCode));
        assert!(r.rows.iter().all(|x| x.source == Source::ClaudeCode));
    }

    #[test]
    fn mcp_server_is_taken_from_the_tool_name() {
        assert_eq!(
            mcp_server("mcp__claude_ai_Docs__batch"),
            Some("claude_ai_Docs")
        );
        assert_eq!(mcp_server("Read"), None);
        assert_eq!(mcp_server("mcp____x"), None);
    }
}
