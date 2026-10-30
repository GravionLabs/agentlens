//! What removing a skill, an agent or an MCP server would have saved (`cost --without`).
//!
//! The tokens are the listing of the item in the main-agent requests of the period (for a skill also its
//! load and the carry of what it loaded), as `unused` and `skills` count them. Each token is priced at
//! the effective input price per token of the request it sits in, so the cache mix of that request is
//! assumed to stay. Everything here is an estimate.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::analyzers::context::owner;
use crate::analyzers::skills::{self, RequestShare};
use crate::analyzers::unused::{UnusedKind, norm};
use crate::catalog::Catalog;
use crate::inventory::Inventory;
use crate::model::{ContextKind, EventKind, SessionEvents};
use crate::prices::PriceTable;
use crate::tokens::{TokenEstimator, Tokens};

/// The tokens and cost that one named item adds to the requests of the period.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WithoutRow {
    /// The name as it was given.
    pub item: String,
    pub kind: UnusedKind,
    /// Places the item is installed in (files for a skill, agents and servers), summed.
    pub installations: u32,
    /// Distinct requests that carried tokens of the item.
    pub requests: u64,
    /// Always an estimate.
    pub tokens: Tokens,
    /// Priced requests only.
    pub cost_usd: f64,
    /// Requests that carried tokens of the item but have no price: counted apart, not guessed.
    pub unpriced_requests: u64,
    /// `installations` and `requests` in words.
    pub detail: String,
}

/// The sum of the rows.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct WithoutTotal {
    pub requests: u64,
    pub tokens: Tokens,
    pub cost_usd: f64,
    pub unpriced_requests: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct WithoutReport {
    /// Sorted by cost, largest first.
    pub rows: Vec<WithoutRow>,
    pub total: WithoutTotal,
    /// The assumptions behind the numbers.
    pub notes: Vec<String>,
    /// Items that matched nothing installed.
    pub warnings: Vec<String>,
}

/// One `KIND:NAME` of the command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemSpec {
    pub kind: UnusedKind,
    pub name: String,
}

/// Reads `skill:NAME`, `agent:NAME` or `mcp-server:NAME` (`mcp:NAME` too).
pub fn parse_items(specs: &[String]) -> Result<Vec<ItemSpec>, String> {
    let mut items: Vec<ItemSpec> = Vec::new();
    for spec in specs {
        let bad = || format!("`{spec}` is not KIND:NAME with KIND one of skill, agent, mcp-server");
        let (kind, name) = spec.split_once(':').ok_or_else(bad)?;
        let kind = match kind.trim().to_ascii_lowercase().as_str() {
            "skill" => UnusedKind::Skill,
            "agent" => UnusedKind::Agent,
            "mcp-server" | "mcp" => UnusedKind::McpServer,
            _ => return Err(bad()),
        };
        let name = name.trim();
        if name.is_empty() {
            return Err(bad());
        }
        let item = ItemSpec {
            kind,
            name: name.to_owned(),
        };
        if !items.contains(&item) {
            items.push(item);
        }
    }
    Ok(items)
}

/// The USD per token that a request pays for its input side (fresh input plus cache), `None` if the
/// model has no price or the request has no input.
fn input_price(prices: &PriceTable, se: &SessionEvents, u: &crate::model::Usage) -> Option<f64> {
    let total = u.total_input();
    if total == 0 {
        return None;
    }
    let model = u.model.as_deref().or(se.session.model.as_deref())?;
    let r = prices.rates(model, total)?;
    let per_million = u.input as f64 * r.input
        + u.cache_write as f64 * r.cache_write
        + u.cache_read as f64 * r.cache_read;
    Some(per_million / 1_000_000.0 / total as f64)
}

fn main_request_shares(
    sessions: &[SessionEvents],
    applies: impl Fn(&SessionEvents) -> bool,
    tokens: u64,
    shares: &mut Vec<RequestShare>,
) {
    for (si, se) in sessions.iter().enumerate().filter(|(_, se)| applies(se)) {
        for (k, e) in se.events.iter().enumerate() {
            if matches!(&e.kind, EventKind::Usage(u) if !u.subagent) {
                shares.push(RequestShare {
                    session: si,
                    event: Some(k),
                    tokens,
                });
            }
        }
    }
}

/// The tokens an MCP server adds to the main-agent requests after the first appearance of each of its
/// context items, like the context analysis counts them.
fn mcp_shares(
    sessions: &[SessionEvents],
    server: &str,
    clients: &[crate::model::Source],
    estimator: &dyn TokenEstimator,
    shares: &mut Vec<RequestShare>,
) {
    for (si, se) in sessions.iter().enumerate() {
        if !clients.contains(&se.session.source) {
            continue;
        }
        type Seen<'a> = BTreeMap<(ContextKind, Option<&'a str>), (usize, u64, Option<u64>)>;
        let mut seen: Seen = BTreeMap::new();
        for (i, e) in se.events.iter().enumerate() {
            if let EventKind::ContextItem(item) = &e.kind
                && matches!(
                    item.kind,
                    ContextKind::ToolListing
                        | ContextKind::ToolDefinition
                        | ContextKind::McpInstructions
                )
                && norm(&owner(item.kind, item.name.as_deref())) == server
            {
                let entry = seen
                    .entry((item.kind, item.name.as_deref()))
                    .or_insert((i, 0, None));
                entry.1 = entry.1.max(item.bytes);
                entry.2 = entry.2.max(item.tokens);
            }
        }
        for (first, bytes, reported) in seen.into_values() {
            let tokens = reported.unwrap_or_else(|| estimator.count_bytes(bytes).count);
            for (k, e) in se.events.iter().enumerate().skip(first + 1) {
                if matches!(&e.kind, EventKind::Usage(u) if !u.subagent) {
                    shares.push(RequestShare {
                        session: si,
                        event: Some(k),
                        tokens,
                    });
                }
            }
        }
    }
}

/// Prices the tokens of the named items. `sessions` are those of the period.
pub fn analyze(
    sessions: &[SessionEvents],
    prices: &PriceTable,
    items: &[ItemSpec],
    inventory: &Inventory,
    catalog: &Catalog,
    estimator: &dyn TokenEstimator,
) -> WithoutReport {
    let (skill_report, skill_shares) = skills::analyze_traced(inventory, sessions, estimator);
    let mut rows = Vec::new();
    let mut warnings = Vec::new();
    for spec in items {
        let mut shares: Vec<RequestShare> = Vec::new();
        let installations: u32 = match spec.kind {
            UnusedKind::Skill => {
                let installed = skill_report
                    .skills
                    .iter()
                    .any(|s| s.in_inventory && s.name == spec.name);
                if installed {
                    shares = skill_shares.get(&spec.name).cloned().unwrap_or_default();
                }
                if installed {
                    inventory.find(&spec.name).len() as u32
                } else {
                    0
                }
            }
            UnusedKind::Agent => {
                let found: Vec<_> = catalog
                    .agents
                    .iter()
                    .filter(|a| a.name == spec.name)
                    .collect();
                for a in &found {
                    main_request_shares(
                        sessions,
                        |se| {
                            a.clients.contains(&se.session.source)
                                && (a.project.is_none() || a.project == se.session.project_dir)
                        },
                        a.listing_tokens.count,
                        &mut shares,
                    );
                }
                found.len() as u32
            }
            UnusedKind::McpServer => {
                let server = norm(&spec.name);
                let found: Vec<_> = catalog
                    .mcp_servers
                    .iter()
                    .filter(|m| norm(&m.name) == server)
                    .collect();
                for m in &found {
                    mcp_shares(sessions, &server, &m.clients, estimator, &mut shares);
                }
                found.len() as u32
            }
        };
        if installations == 0 {
            warnings.push(format!(
                "nothing installed matches `{}:{}`",
                kind_name(spec.kind),
                spec.name
            ));
            continue;
        }
        rows.push(price_row(sessions, prices, spec, installations, &shares));
    }
    rows.sort_by(|a, b| {
        b.cost_usd
            .partial_cmp(&a.cost_usd)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.tokens.count.cmp(&a.tokens.count))
            .then_with(|| a.item.cmp(&b.item))
    });
    let mut total = WithoutTotal {
        tokens: Tokens::estimated(0),
        ..WithoutTotal::default()
    };
    for r in &rows {
        total.requests += r.requests;
        total.tokens.count += r.tokens.count;
        total.cost_usd += r.cost_usd;
        total.unpriced_requests += r.unpriced_requests;
    }
    let mut notes = vec![
        "Estimated: the tokens come from the sizes of the listings, not from the client.".to_owned(),
        "Assumes the same requests and the same cache mix: each token is priced at the effective input price per token of the request it sits in.".to_owned(),
        "Counts the listing only (for a skill also its load and what it carried); the results of MCP tool calls and the work of subagent runs are not removed.".to_owned(),
    ];
    if total.unpriced_requests > 0 {
        notes.push(format!(
            "Requests without a price ({}) are counted apart, not priced.",
            total.unpriced_requests
        ));
    }
    WithoutReport {
        rows,
        total,
        notes,
        warnings,
    }
}

fn kind_name(kind: UnusedKind) -> &'static str {
    match kind {
        UnusedKind::Skill => "skill",
        UnusedKind::Agent => "agent",
        UnusedKind::McpServer => "mcp-server",
    }
}

fn price_row(
    sessions: &[SessionEvents],
    prices: &PriceTable,
    spec: &ItemSpec,
    installations: u32,
    shares: &[RequestShare],
) -> WithoutRow {
    let (mut tokens, mut cost) = (0u64, 0.0f64);
    let mut requests: BTreeSet<(usize, Option<usize>)> = BTreeSet::new();
    let mut unpriced: BTreeSet<(usize, Option<usize>)> = BTreeSet::new();
    for s in shares.iter().filter(|s| s.tokens > 0) {
        tokens += s.tokens;
        let key = (s.session, s.event);
        requests.insert(key);
        let se = &sessions[s.session];
        let price = s.event.and_then(|k| match &se.events[k].kind {
            EventKind::Usage(u) => input_price(prices, se, u),
            _ => None,
        });
        match price {
            Some(p) => cost += p * s.tokens as f64,
            None => {
                unpriced.insert(key);
            }
        }
    }
    let requests = requests.len() as u64;
    WithoutRow {
        item: spec.name.clone(),
        kind: spec.kind,
        installations,
        requests,
        tokens: Tokens::estimated(tokens),
        cost_usd: cost,
        unpriced_requests: unpriced.len() as u64,
        detail: format!(
            "{installations} installation{}, {requests} request{}",
            if installations == 1 { "" } else { "s" },
            if requests == 1 { "" } else { "s" }
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{AgentEntry, McpServer};
    use crate::inventory::{Location, LocationKind, SkillEntry};
    use crate::model::{ContextItem, Session, SkillInvoked, SkillTrigger, Source, Usage};
    use crate::tokens::CharsPerToken;

    const MODEL: &str = "claude-sonnet-5-5"; // $2 in, $0.10 cache read per million

    fn request(model: &str, input: u64, cache_read: u64, subagent: bool) -> EventKind {
        EventKind::Usage(Usage {
            model: Some(model.into()),
            input,
            cache_read,
            subagent,
            ..Usage::default()
        })
    }

    fn fresh() -> EventKind {
        request(MODEL, 1000, 0, false)
    }

    fn session(project: &str, kinds: Vec<EventKind>) -> SessionEvents {
        let mut s = Session::new("s", Source::ClaudeCode);
        s.project_dir = Some(project.into());
        let mut se = SessionEvents::new(s);
        for k in kinds {
            se.push(None, k);
        }
        se
    }

    fn skill_entry(name: &str, baseline: u64, project: &str) -> SkillEntry {
        let dir = format!("{project}/.claude/skills");
        SkillEntry {
            name: name.into(),
            description: None,
            file: format!("{dir}/{name}/SKILL.md").into(),
            clients: vec![Source::ClaudeCode],
            locations: vec![Location {
                kind: LocationKind::ProjectClaude,
                dir: dir.into(),
                project: Some(project.into()),
            }],
            baseline_tokens: Tokens::estimated(baseline),
            body_tokens: Tokens::estimated(0),
            body_bytes: 0,
        }
    }

    fn agent(name: &str, listing: u64) -> AgentEntry {
        AgentEntry {
            name: name.into(),
            file: "a.md".into(),
            project: None,
            clients: vec![Source::ClaudeCode],
            listing_tokens: Tokens::estimated(listing),
        }
    }

    fn run(
        sessions: &[SessionEvents],
        specs: &[&str],
        skills: Vec<SkillEntry>,
        catalog: Catalog,
    ) -> WithoutReport {
        let specs: Vec<String> = specs.iter().map(|s| (*s).to_owned()).collect();
        analyze(
            sessions,
            &PriceTable::bundled(),
            &parse_items(&specs).unwrap(),
            &Inventory {
                skills,
                ..Inventory::default()
            },
            &catalog,
            &CharsPerToken,
        )
    }

    fn catalog(agents: Vec<AgentEntry>, mcp: Vec<&str>) -> Catalog {
        Catalog {
            agents,
            mcp_servers: mcp
                .into_iter()
                .map(|name| McpServer {
                    name: name.into(),
                    file: "c.json".into(),
                    project: None,
                    clients: vec![Source::ClaudeCode],
                })
                .collect(),
            warnings: Vec::new(),
        }
    }

    fn close(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-12, "{a} != {b}");
    }

    #[test]
    fn an_agent_costs_its_listing_in_every_main_request() {
        let se = session(
            "/p",
            vec![fresh(), fresh(), request(MODEL, 1000, 0, true), fresh()],
        );
        let r = run(
            &[se],
            &["agent:reviewer"],
            vec![],
            catalog(vec![agent("reviewer", 10)], vec![]),
        );
        let row = &r.rows[0];
        assert_eq!(
            (row.kind, row.installations, row.requests),
            (UnusedKind::Agent, 1, 3)
        );
        assert_eq!(row.tokens, Tokens::estimated(30));
        // 30 tokens at $2 per million
        close(row.cost_usd, 0.00006);
        assert_eq!(row.detail, "1 installation, 3 requests");
        assert_eq!(r.total.tokens, Tokens::estimated(30));
        close(r.total.cost_usd, 0.00006);
        assert!(r.warnings.is_empty());
    }

    #[test]
    fn a_request_is_priced_at_its_effective_input_price_with_the_cache_mix() {
        // 500k fresh at $2 and 500k cache read at $0.10 per million: $1.05 for 1M, 1.05e-6 per token.
        let se = session("/p", vec![request(MODEL, 500_000, 500_000, false)]);
        let r = run(
            &[se],
            &["agent:reviewer"],
            vec![],
            catalog(vec![agent("reviewer", 1000)], vec![]),
        );
        close(r.rows[0].cost_usd, 0.00105);
    }

    #[test]
    fn an_mcp_server_counts_from_its_first_context_item_and_matches_through_norm() {
        let item = |name: &str, bytes: u64| {
            EventKind::ContextItem(ContextItem {
                kind: ContextKind::ToolDefinition,
                name: Some(name.into()),
                bytes,
                tokens: None,
            })
        };
        let se = session(
            "/p",
            vec![
                fresh(), // before the item: not carried
                item("mcp__claude_ai_Docs__read", 400),
                item("mcp__claude_ai_Docs__read", 400), // announced again: the same item
                item("mcp__other__read", 4000),
                fresh(),
                fresh(),
            ],
        );
        let r = run(
            &[se],
            &["mcp-server:claude.ai Docs"],
            vec![],
            catalog(vec![], vec!["claude.ai Docs"]),
        );
        let row = &r.rows[0];
        assert_eq!(row.kind, UnusedKind::McpServer);
        // 400 bytes = 100 tokens, two later main requests
        assert_eq!((row.requests, row.tokens), (2, Tokens::estimated(200)));
        close(row.cost_usd, 0.0004);
    }

    #[test]
    fn a_skill_costs_its_listing_in_the_requests_of_its_clients() {
        let se = session("/p", vec![fresh(), fresh()]);
        let r = run(
            &[se],
            &["skill:demo"],
            vec![skill_entry("demo", 20, "/p")],
            Catalog::default(),
        );
        assert_eq!(r.rows[0].tokens, Tokens::estimated(40));
        close(r.rows[0].cost_usd, 0.00008);
    }

    #[test]
    fn a_name_installed_in_two_projects_is_summed() {
        let a = session("/a", vec![fresh(), fresh()]);
        let b = session("/b", vec![fresh()]);
        let r = run(
            &[a, b],
            &["skill:demo"],
            vec![skill_entry("demo", 20, "/a"), skill_entry("demo", 30, "/b")],
            Catalog::default(),
        );
        let row = &r.rows[0];
        assert_eq!((row.installations, row.requests), (2, 3));
        assert_eq!(row.tokens, Tokens::estimated(2 * 20 + 30));
        assert_eq!(row.detail, "2 installations, 3 requests");
    }

    #[test]
    fn a_loaded_skill_counts_baseline_load_and_carry_once() {
        let mut load = SkillInvoked::new("demo", SkillTrigger::Model);
        load.content_bytes = Some(400); // 100 tokens, no request before: estimated
        let se = session(
            "/p",
            vec![EventKind::SkillInvoked(load), fresh(), fresh(), fresh()],
        );
        let inventory = Inventory {
            skills: vec![skill_entry("demo", 20, "/p")],
            ..Inventory::default()
        };
        let report = skills::analyze(&inventory, std::slice::from_ref(&se), &CharsPerToken);
        let overhead = report.skills[0].overhead_tokens();
        let r = run(
            &[se],
            &["skill:demo"],
            inventory.skills.clone(),
            Catalog::default(),
        );
        let row = &r.rows[0];
        // baseline 3 x 20, load 100 in the first request, carry 100 in each of the next two
        assert_eq!(overhead, 360);
        assert_eq!(row.tokens, Tokens::estimated(360));
        assert_eq!(
            row.requests, 3,
            "a request that carries several parts counts once"
        );
        close(row.cost_usd, 0.00072);
    }

    #[test]
    fn requests_without_a_price_are_counted_apart() {
        let se = session("/p", vec![fresh(), request("mystery", 1000, 0, false)]);
        let r = run(
            &[se],
            &["agent:reviewer"],
            vec![],
            catalog(vec![agent("reviewer", 10)], vec![]),
        );
        let row = &r.rows[0];
        assert_eq!((row.requests, row.unpriced_requests), (2, 1));
        assert_eq!(row.tokens, Tokens::estimated(20));
        close(row.cost_usd, 0.00002);
        assert_eq!(r.total.unpriced_requests, 1);
        assert!(
            r.notes
                .last()
                .unwrap()
                .starts_with("Requests without a price (1)")
        );
    }

    #[test]
    fn an_unknown_name_is_a_warning_not_a_row() {
        let se = session("/p", vec![fresh()]);
        let r = run(
            &[se],
            &["skill:nope", "agent:nope"],
            vec![],
            Catalog::default(),
        );
        assert!(r.rows.is_empty());
        assert_eq!(r.total.tokens, Tokens::estimated(0));
        assert_eq!(
            r.warnings,
            vec![
                "nothing installed matches `skill:nope`",
                "nothing installed matches `agent:nope`"
            ]
        );
    }

    #[test]
    fn the_notes_state_the_assumptions() {
        let r = run(&[], &["skill:nope"], vec![], Catalog::default());
        let notes = r.notes.join(" ");
        assert!(notes.contains("same requests and the same cache mix"));
        assert!(
            notes.contains(
                "results of MCP tool calls and the work of subagent runs are not removed"
            )
        );
    }

    #[test]
    fn items_are_parsed_and_deduplicated() {
        let specs = ["skill:a", "mcp:b c", "agent: d ", "skill:a"].map(String::from);
        let items = parse_items(&specs).unwrap();
        assert_eq!(items.len(), 3);
        assert_eq!(items[1].kind, UnusedKind::McpServer);
        assert_eq!(items[1].name, "b c");
        assert_eq!(items[2].name, "d");
        for bad in ["demo", "tool:x", "skill:", ":x"] {
            assert!(parse_items(&[bad.to_owned()]).is_err(), "{bad}");
        }
    }
}
