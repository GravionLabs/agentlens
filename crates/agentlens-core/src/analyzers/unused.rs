//! Installed skills, agents and MCP servers, how often they were used in the
//! period and what they cost while idle.
//!
//! The baseline cost of an unused item is an estimate: skills and agents are
//! listed in every request of the clients that load them; for MCP servers the
//! cost comes from the context items the client reported (Claude Code only), so
//! other clients show no cost rather than zero.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::analyzers::context::{ContextReport, mcp_server};
use crate::analyzers::skills::SkillReport;
use crate::catalog::Catalog;
use crate::model::{ContextKind, EventKind, SessionEvents, Source};
use crate::references::{Reference, References};
use crate::tokens::Tokens;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UnusedKind {
    Skill,
    Agent,
    McpServer,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnusedItem {
    pub kind: UnusedKind,
    pub name: String,
    pub project: Option<PathBuf>,
    pub clients: Vec<Source>,
    /// Uses in the analyzed period. For a skill: the times it was loaded plus the tool calls that read
    /// or ran files in its folder.
    pub uses: u32,
    /// Requests that carried the item.
    pub requests: u64,
    /// Baseline tokens over the period (estimated unless the client reported the sizes); `None` if unknown.
    pub overhead_tokens: Option<Tokens>,
    /// For an unused skill: the installed agents, commands and skills whose files name it. A mention
    /// does not prove that it is used.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub referenced_by: Vec<Reference>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnusedReport {
    pub sessions: u32,
    /// Sorted by overhead (largest first); unknown costs last. Unused skills that something refers to
    /// come after everything else.
    pub items: Vec<UnusedItem>,
}

/// `claude.ai Claude Docs` and `claude_ai_Claude_Docs` name the same server.
pub(crate) fn norm(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect()
}

pub(crate) fn calls_server(tool: &str, server: &str) -> bool {
    mcp_server(tool).is_some_and(|s| norm(s) == server)
        || norm(tool).starts_with(&format!("{server}_"))
}

fn is_agent_tool(tool: &str) -> bool {
    matches!(tool, "Agent" | "Task" | "task")
}

fn applies(session: &SessionEvents, clients: &[Source], project: &Option<PathBuf>) -> bool {
    clients.contains(&session.session.source)
        && (project.is_none() || *project == session.session.project_dir)
}

fn main_requests(session: &SessionEvents) -> u64 {
    session
        .events
        .iter()
        .filter(|e| matches!(&e.kind, EventKind::Usage(u) if !u.subagent))
        .count() as u64
}

pub fn analyze(
    skills: &SkillReport,
    catalog: &Catalog,
    context: &ContextReport,
    sessions: &[SessionEvents],
    references: &References,
) -> UnusedReport {
    let mut items = Vec::new();

    for s in skills.skills.iter().filter(|s| s.in_inventory) {
        items.push(UnusedItem {
            kind: UnusedKind::Skill,
            name: s.name.clone(),
            project: None,
            clients: s.clients.clone(),
            uses: s.invocations + s.file_uses,
            requests: s.baseline.requests,
            overhead_tokens: Some(s.baseline.tokens),
            referenced_by: if s.invocations + s.file_uses == 0 {
                references.get(&s.name).cloned().unwrap_or_default()
            } else {
                Vec::new()
            },
        });
    }

    for a in &catalog.agents {
        let (mut uses, mut requests) = (0u32, 0u64);
        for se in sessions
            .iter()
            .filter(|se| applies(se, &a.clients, &a.project))
        {
            requests += main_requests(se);
            uses += se
                .events
                .iter()
                .filter(|e| matches!(&e.kind, EventKind::ToolCall(c)
                    if is_agent_tool(&c.name) && c.input_summary.as_deref() == Some(a.name.as_str())))
                .count() as u32;
        }
        items.push(UnusedItem {
            kind: UnusedKind::Agent,
            name: a.name.clone(),
            project: a.project.clone(),
            clients: a.clients.clone(),
            uses,
            requests,
            overhead_tokens: Some(Tokens {
                count: a.listing_tokens.count * requests,
                measured: a.listing_tokens.measured,
            }),
            referenced_by: Vec::new(),
        });
    }

    for m in &catalog.mcp_servers {
        let server = norm(&m.name);
        let mut uses = 0u32;
        for se in sessions
            .iter()
            .filter(|se| applies(se, &m.clients, &m.project))
        {
            uses += se
                .events
                .iter()
                .filter(
                    |e| matches!(&e.kind, EventKind::ToolCall(c) if calls_server(&c.name, &server)),
                )
                .count() as u32;
        }
        let rows: Vec<_> = context
            .rows
            .iter()
            .filter(|r| {
                matches!(
                    r.kind,
                    ContextKind::ToolListing
                        | ContextKind::ToolDefinition
                        | ContextKind::McpInstructions
                ) && norm(&r.owner) == server
                    && m.clients.contains(&r.source)
            })
            .collect();
        items.push(UnusedItem {
            kind: UnusedKind::McpServer,
            name: m.name.clone(),
            project: m.project.clone(),
            clients: m.clients.clone(),
            uses,
            requests: rows.iter().map(|r| r.requests).sum(),
            overhead_tokens: (!rows.is_empty()).then(|| Tokens {
                count: rows.iter().map(|r| r.overhead_tokens.count).sum(),
                measured: rows.iter().all(|r| r.overhead_tokens.measured),
            }),
            referenced_by: Vec::new(),
        });
    }

    items.sort_by(|a, b| {
        let count = |i: &UnusedItem| i.overhead_tokens.map(|t| t.count);
        // Skills that something names come last.
        (!a.referenced_by.is_empty())
            .cmp(&!b.referenced_by.is_empty())
            .then_with(|| count(b).cmp(&count(a)))
            .then_with(|| a.kind.cmp(&b.kind))
            .then_with(|| a.name.cmp(&b.name))
    });
    UnusedReport {
        sessions: sessions.len() as u32,
        items,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzers::context::ContextRow;
    use crate::analyzers::skills::{BaselineCost, SkillStats, TriggerCounts};
    use crate::catalog::{AgentEntry, McpServer};
    use crate::model::{Session, ToolCall, ToolStatus, Usage};
    use crate::tokens::Tokens;

    fn call(name: &str, summary: Option<&str>) -> EventKind {
        EventKind::ToolCall(ToolCall {
            name: name.into(),
            input_summary: summary.map(Into::into),
            status: ToolStatus::Ok,
            duration_ms: None,
        })
    }

    fn request() -> EventKind {
        EventKind::Usage(Usage::default())
    }

    fn session(source: Source, project: &str, kinds: Vec<EventKind>) -> SessionEvents {
        let mut s = Session::new("s", source);
        s.project_dir = Some(project.into());
        let mut se = SessionEvents::new(s);
        for k in kinds {
            se.push(None, k);
        }
        se
    }

    fn skill(name: &str, uses: u32, baseline: u64) -> SkillStats {
        SkillStats {
            name: name.into(),
            in_inventory: true,
            clients: vec![Source::ClaudeCode],
            invocations: uses,
            file_uses: 0,
            sessions: 0,
            projects: vec![],
            first_used: None,
            last_used: None,
            triggers: TriggerCounts::default(),
            by_source: Default::default(),
            uses_by_day: Default::default(),
            load: Tokens::measured(0),
            unknown_loads: 0,
            carry: Default::default(),
            follow_on: Default::default(),
            attributed: Default::default(),
            baseline: BaselineCost {
                requests: 4,
                tokens: Tokens::estimated(baseline),
            },
        }
    }

    fn row(owner: &str, kind: ContextKind, requests: u64, overhead: u64) -> ContextRow {
        ContextRow {
            source: Source::ClaudeCode,
            project: None,
            kind,
            owner: owner.into(),
            items: 1,
            bytes: 0,
            tokens: Tokens::estimated(0),
            requests,
            overhead_tokens: Tokens::estimated(overhead),
        }
    }

    #[test]
    fn used_and_unused_items_with_their_idle_cost() {
        let skills = SkillReport {
            skills: vec![skill("used", 2, 100), skill("idle", 0, 300)],
            ..SkillReport::default()
        };
        let catalog = Catalog {
            agents: vec![
                AgentEntry {
                    name: "reviewer".into(),
                    file: "a.md".into(),
                    project: None,
                    clients: vec![Source::ClaudeCode],
                    listing_tokens: Tokens::estimated(10),
                },
                AgentEntry {
                    name: "other-project".into(),
                    file: "b.md".into(),
                    project: Some("/elsewhere".into()),
                    clients: vec![Source::ClaudeCode],
                    listing_tokens: Tokens::estimated(10),
                },
            ],
            mcp_servers: vec![
                McpServer {
                    name: "claude.ai Claude Docs".into(),
                    file: "c.json".into(),
                    project: None,
                    clients: vec![Source::ClaudeCode],
                },
                McpServer {
                    name: "idle-srv".into(),
                    file: "c.json".into(),
                    project: None,
                    clients: vec![Source::ClaudeCode],
                },
                McpServer {
                    name: "oc".into(),
                    file: "o.json".into(),
                    project: None,
                    clients: vec![Source::OpenCode],
                },
            ],
            warnings: vec![],
        };
        let context = ContextReport {
            rows: vec![
                row("claude_ai_Claude_Docs", ContextKind::ToolDefinition, 5, 500),
                row(
                    "claude.ai Claude Docs",
                    ContextKind::McpInstructions,
                    5,
                    700,
                ),
                row("idle-srv", ContextKind::McpInstructions, 5, 50),
                row("idle-srv", ContextKind::SkillListing, 5, 9999), // not a server cost
            ],
            ..ContextReport::default()
        };
        let sessions = [session(
            Source::ClaudeCode,
            "/work/demo",
            vec![
                request(),
                call("Agent", Some("reviewer")),
                request(),
                call("mcp__claude_ai_Claude_Docs__batch", None),
                call("mcp__claude_ai_Claude_Docs__read", None),
                request(),
            ],
        )];
        let r = analyze(&skills, &catalog, &context, &sessions, &References::new());
        let find = |kind, name: &str| {
            r.items
                .iter()
                .find(|i| i.kind == kind && i.name == name)
                .unwrap()
        };

        assert_eq!(find(UnusedKind::Skill, "idle").uses, 0);
        assert_eq!(
            find(UnusedKind::Skill, "idle")
                .overhead_tokens
                .map(|t| t.count),
            Some(300)
        );
        let reviewer = find(UnusedKind::Agent, "reviewer");
        assert_eq!(
            (
                reviewer.uses,
                reviewer.requests,
                reviewer.overhead_tokens.map(|t| t.count)
            ),
            (1, 3, Some(30))
        );
        let scoped = find(UnusedKind::Agent, "other-project");
        assert_eq!(
            (scoped.uses, scoped.requests),
            (0, 0),
            "other project's sessions do not count"
        );
        let docs = find(UnusedKind::McpServer, "claude.ai Claude Docs");
        assert_eq!(
            (docs.uses, docs.overhead_tokens.map(|t| t.count)),
            (2, Some(1200))
        );
        assert!(
            !docs.overhead_tokens.unwrap().measured,
            "sizes taken from bytes are estimates"
        );
        let idle = find(UnusedKind::McpServer, "idle-srv");
        assert_eq!(
            (idle.uses, idle.overhead_tokens.map(|t| t.count)),
            (0, Some(50))
        );
        assert_eq!(
            find(UnusedKind::McpServer, "oc").overhead_tokens,
            None,
            "unknown, not zero"
        );
        assert_eq!(
            r.items.last().unwrap().overhead_tokens,
            None,
            "unknown costs sort last"
        );
    }

    #[test]
    fn opencode_style_tool_names_match_servers() {
        assert!(calls_server("docs_search", "docs"));
        assert!(!calls_server("docsearch", "docs"));
        assert!(calls_server("mcp__Docs__x", "docs"));
    }

    #[test]
    fn a_referenced_unused_skill_is_marked_and_ranked_after_the_others() {
        use crate::references::ReferenceKind;
        let mut refs = References::new();
        let reference = Reference {
            kind: ReferenceKind::Agent,
            name: "flow".into(),
        };
        refs.insert("idle".into(), vec![reference.clone()]);
        // A used skill is never marked, even if something names it.
        refs.insert("used".into(), vec![reference.clone()]);
        let skills = SkillReport {
            skills: vec![
                skill("used", 2, 100),
                skill("idle", 0, 900),
                skill("other", 0, 50),
            ],
            ..SkillReport::default()
        };
        let r = analyze(
            &skills,
            &Catalog::default(),
            &ContextReport::default(),
            &[],
            &refs,
        );
        let names: Vec<&str> = r.items.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["used", "other", "idle"]);

        // Several referenced skills keep the order by cost and name among themselves.
        refs.insert("other".into(), vec![reference.clone()]);
        let r = analyze(
            &skills,
            &Catalog::default(),
            &ContextReport::default(),
            &[],
            &refs,
        );
        let names: Vec<&str> = r.items.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["used", "idle", "other"]);
        assert_eq!(r.items[2].referenced_by, [reference]);
        assert!(r.items[0].referenced_by.is_empty());
    }
}
