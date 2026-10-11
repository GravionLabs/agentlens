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

use crate::instructions::InstructionFile;
use crate::model::{ContextKind, EventKind, SessionEvents, Source};
use crate::tokens::{TokenEstimator, Tokens};

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
    /// Tokens of the items, counted once per session. Measured only if the client reported the size of
    /// every item; otherwise estimated from the bytes.
    pub tokens: Tokens,
    /// Sum over the items of the main-agent requests that followed them.
    pub requests: u64,
    /// Tokens x requests; measured exactly when `tokens` is.
    pub overhead_tokens: Tokens,
}

impl ContextRow {
    /// Adds an item of `tokens` that `requests` main-agent requests carried. The row stays measured
    /// only while every item is.
    fn add(&mut self, tokens: Tokens, requests: u64) {
        self.tokens.count += tokens.count;
        self.tokens.measured &= tokens.measured;
        self.requests += requests;
        self.overhead_tokens.count += tokens.count * requests;
        self.overhead_tokens.measured = self.tokens.measured;
    }
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
        ContextKind::ToolListing | ContextKind::ToolDefinition => match name {
            // Clients that report only a total for all tool definitions.
            None => "all tools".to_owned(),
            Some(n) => mcp_server(n).unwrap_or("built-in").to_owned(),
        },
        ContextKind::SystemPrompt => "system prompt".to_owned(),
        ContextKind::McpInstructions | ContextKind::InstructionFile => {
            name.unwrap_or("unknown").to_owned()
        }
        ContextKind::AgentListing => "agents".to_owned(),
        ContextKind::SkillListing => "skills".to_owned(),
    }
}

/// First index, largest size and reported tokens per distinct item of a session.
type Seen<'a> = BTreeMap<(ContextKind, Option<&'a str>), (usize, u64, Option<u64>)>;

/// `instructions` is the inventory of instruction files; it fills in for
/// sessions whose client does not report the files it loaded.
pub fn analyze(
    sessions: &[SessionEvents],
    instructions: &[InstructionFile],
    estimator: &dyn TokenEstimator,
) -> ContextReport {
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
        let mut seen: Seen = BTreeMap::new();
        for (i, e) in se.events.iter().enumerate() {
            if let EventKind::ContextItem(item) = &e.kind {
                let entry = seen
                    .entry((item.kind, item.name.as_deref()))
                    .or_insert((i, 0, None));
                entry.1 = entry.1.max(item.bytes);
                entry.2 = entry.2.max(item.tokens);
            }
        }
        let mut any = !seen.is_empty();
        let seen_instruction_files = seen.keys().any(|(k, _)| *k == ContextKind::InstructionFile);
        for ((kind, name), (i, bytes, reported)) in seen {
            let owner = owner(kind, name);
            let tokens = match reported {
                Some(count) => Tokens::measured(count),
                None => estimator.count_bytes(bytes),
            };
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
                    tokens: Tokens::measured(0),
                    requests: 0,
                    overhead_tokens: Tokens::measured(0),
                });
            row.items += 1;
            row.bytes += bytes;
            row.add(tokens, requests);
        }
        let reported = seen_instruction_files;
        if !reported {
            let requests = later[0];
            for f in instructions {
                let applies = f.clients.contains(&source)
                    && (f.project.is_none() || f.project == se.session.project_dir);
                if !applies {
                    continue;
                }
                any = true;
                let row = rows
                    .entry((
                        source,
                        se.session.project_dir.clone(),
                        ContextKind::InstructionFile,
                        f.path.display().to_string(),
                    ))
                    .or_insert_with(|| ContextRow {
                        source,
                        project: se.session.project_dir.clone(),
                        kind: ContextKind::InstructionFile,
                        owner: f.path.display().to_string(),
                        items: 0,
                        bytes: 0,
                        tokens: Tokens::measured(0),
                        requests: 0,
                        overhead_tokens: Tokens::measured(0),
                    });
                row.items += 1;
                row.bytes += f.bytes;
                row.add(f.tokens, requests);
            }
        }
        if !any {
            *report.unavailable.entry(source).or_default() += 1;
        }
    }

    report.rows = rows.into_values().collect();
    report.rows.sort_by(|a, b| {
        b.overhead_tokens
            .count
            .cmp(&a.overhead_tokens.count)
            .then_with(|| a.owner.cmp(&b.owner))
    });
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ContextItem, Session, Usage};
    use crate::tokens::{CharsPerToken, Tokens};

    fn item(kind: ContextKind, name: Option<&str>, bytes: u64) -> EventKind {
        EventKind::ContextItem(ContextItem {
            kind,
            name: name.map(str::to_owned),
            bytes,
            tokens: None,
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
        let r = analyze(&[se], &[], &CharsPerToken);
        let row = |kind, owner: &str| {
            r.rows
                .iter()
                .find(|x| x.kind == kind && x.owner == owner)
                .unwrap()
                .clone()
        };
        let claude = row(InstructionFile, "/work/demo/CLAUDE.md");
        assert_eq!(
            (
                claude.tokens.count,
                claude.requests,
                claude.overhead_tokens.count
            ),
            (100, 3, 300)
        );
        let srv = row(ToolListing, "srv");
        assert_eq!((srv.items, srv.tokens.count), (2, 10 + 5));
        assert_eq!(srv.overhead_tokens.count, 15 * 3);
        assert_eq!(row(ToolListing, "built-in").tokens.count, 2);
        // Sizes taken from the bytes are estimates, and so is what is multiplied from them.
        assert!(!claude.tokens.measured && !claude.overhead_tokens.measured);
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
        let r = analyze(&[se], &[], &CharsPerToken);
        let row = &r.rows[0];
        assert_eq!((row.items, row.bytes, row.tokens.count), (1, 80, 20));
        assert_eq!(row.requests, 2);
        assert_eq!(row.overhead_tokens.count, 40);
    }

    #[test]
    fn reported_tokens_replace_the_size_estimate() {
        let se = session(
            Source::VsCodeCopilot,
            vec![
                EventKind::ContextItem(ContextItem {
                    kind: ContextKind::ToolDefinition,
                    name: None,
                    bytes: 0,
                    tokens: Some(500),
                }),
                request(10, false),
                request(10, false),
            ],
        );
        let r = analyze(&[se], &[], &CharsPerToken);
        assert_eq!(r.rows[0].owner, "all tools");
        assert_eq!(
            (r.rows[0].tokens, r.rows[0].overhead_tokens),
            (Tokens::measured(500), Tokens::measured(1000)),
            "a size the client reported is measured"
        );
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
        let r = analyze(&[se], &[], &CharsPerToken);
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
        let r = analyze(&[a, b], &[], &CharsPerToken);
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
        let r = analyze(&[with, without.clone(), without], &[], &CharsPerToken);
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

    fn file(path: &str, project: Option<&str>, bytes: u64, clients: &[Source]) -> InstructionFile {
        InstructionFile {
            path: path.into(),
            kind: crate::instructions::InstructionKind::AgentsMd,
            project: project.map(Into::into),
            clients: clients.to_vec(),
            bytes,
            tokens: CharsPerToken.count_bytes(bytes),
        }
    }

    #[test]
    fn inventory_fills_in_where_the_client_reports_no_files() {
        let se = session(
            Source::OpenCode,
            vec![request(1, false), request(1, false), request(1, false)],
        );
        let files = [
            file(
                "/work/demo/AGENTS.md",
                Some("/work/demo"),
                400,
                &[Source::OpenCode],
            ),
            file("/home/u/AGENTS.md", None, 40, &[Source::OpenCode]),
            file("/other/AGENTS.md", Some("/other"), 800, &[Source::OpenCode]),
            file(
                "/work/demo/CLAUDE.md",
                Some("/work/demo"),
                800,
                &[Source::ClaudeCode],
            ),
        ];
        let r = analyze(&[se], &files, &CharsPerToken);
        let owners: Vec<_> = r.rows.iter().map(|x| x.owner.as_str()).collect();
        assert_eq!(owners, ["/work/demo/AGENTS.md", "/home/u/AGENTS.md"]);
        assert_eq!(r.rows[0].overhead_tokens.count, 100 * 3);
        assert_eq!(r.rows[1].overhead_tokens.count, 10 * 3);
        assert!(
            r.unavailable.is_empty(),
            "inventory data counts as available"
        );
    }

    #[test]
    fn reported_instruction_files_win_over_the_inventory() {
        let se = session(
            Source::ClaudeCode,
            vec![
                item(
                    ContextKind::InstructionFile,
                    Some("/work/demo/CLAUDE.md"),
                    40,
                ),
                request(1, false),
            ],
        );
        let files = [file(
            "/work/demo/CLAUDE.md",
            Some("/work/demo"),
            4000,
            &[Source::ClaudeCode],
        )];
        let r = analyze(&[se], &files, &CharsPerToken);
        assert_eq!(r.rows.len(), 1);
        assert_eq!(r.rows[0].tokens.count, 10);
    }

    #[test]
    fn a_row_is_measured_only_if_every_item_in_it_is() {
        let reported = |tokens| {
            EventKind::ContextItem(ContextItem {
                kind: ContextKind::ToolDefinition,
                name: None,
                bytes: 0,
                tokens,
            })
        };
        let measured = session(
            Source::VsCodeCopilot,
            vec![reported(Some(100)), request(1, false)],
        );
        let estimated = session(
            Source::VsCodeCopilot,
            vec![reported(None), request(1, false)],
        );
        let r = analyze(&[measured, estimated], &[], &CharsPerToken);
        assert_eq!(r.rows.len(), 1);
        assert_eq!(r.rows[0].items, 2);
        assert!(
            !r.rows[0].tokens.measured,
            "one estimated item makes the row an estimate"
        );
        assert!(!r.rows[0].overhead_tokens.measured);
    }
}
