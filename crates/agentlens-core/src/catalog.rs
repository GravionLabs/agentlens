//! Agents and MCP servers that are installed or configured on this machine.
//!
//! Only names, descriptions and file locations are read. Configuration values
//! of MCP servers (commands, environment, headers, tokens) are never read into
//! the model.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::env::Env;
use crate::inventory::{claude_dir, config_home, copilot_dir, parse_markdown};
use crate::model::{Source, Warning};
use crate::tokens::{TokenEstimator, Tokens};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentEntry {
    pub name: String,
    pub file: PathBuf,
    /// The project the agent is limited to; `None` for user-level agents.
    pub project: Option<PathBuf>,
    pub clients: Vec<Source>,
    /// Name and description, listed in every request of those clients.
    pub listing_tokens: Tokens,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpServer {
    pub name: String,
    /// The configuration file that declares it.
    pub file: PathBuf,
    pub project: Option<PathBuf>,
    pub clients: Vec<Source>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Catalog {
    pub agents: Vec<AgentEntry>,
    pub mcp_servers: Vec<McpServer>,
    pub warnings: Vec<Warning>,
}

struct Scan<'a> {
    estimator: &'a dyn TokenEstimator,
    out: Catalog,
}

impl Scan<'_> {
    fn agents(&mut self, dir: &Path, suffix: &str, clients: &[Source], project: Option<&Path>) {
        let Ok(rd) = fs::read_dir(dir) else { return };
        let mut files: Vec<PathBuf> = rd
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| {
                p.is_file()
                    && p.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.ends_with(suffix))
            })
            .collect();
        files.sort();
        for file in files {
            let Ok(text) = fs::read_to_string(&file) else {
                self.out
                    .warnings
                    .push(Warning::new("cannot read agent file").at(file.display().to_string()));
                continue;
            };
            let stem = file
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.trim_end_matches(suffix).to_owned())
                .unwrap_or_default();
            let mut warnings = Vec::new();
            let (name, description) = parse_markdown(&text, &stem, &mut warnings, &file);
            self.out.warnings.extend(warnings);
            let listing = format!("{name}{}", description.as_deref().unwrap_or(""));
            self.out.agents.push(AgentEntry {
                name,
                file,
                project: project.map(Path::to_path_buf),
                clients: clients.to_vec(),
                listing_tokens: self.estimator.count(&listing),
            });
        }
    }

    /// Server names from `object[key]` of a JSON or JSONC file.
    fn mcp(
        &mut self,
        file: &Path,
        pointer: &str,
        clients: &[Source],
        project: Option<&Path>,
        only_project: Option<&Path>,
    ) {
        let Ok(text) = fs::read_to_string(file) else {
            return;
        };
        let value: Value = match serde_json::from_str(&strip_jsonc(&text)) {
            Ok(v) => v,
            Err(e) => {
                self.out.warnings.push(
                    Warning::new(format!("cannot parse MCP configuration: {e}"))
                        .at(file.display().to_string()),
                );
                return;
            }
        };
        let mut node = Some(&value);
        if let Some(p) = only_project {
            node = value
                .get("projects")
                .and_then(|m| m.get(p.to_string_lossy().as_ref()));
        }
        let Some(servers) = node
            .and_then(|n| n.pointer(pointer))
            .and_then(Value::as_object)
        else {
            return;
        };
        for (name, cfg) in servers {
            if cfg.get("enabled").and_then(Value::as_bool) == Some(false) {
                continue;
            }
            self.out.mcp_servers.push(McpServer {
                name: name.clone(),
                file: file.to_path_buf(),
                project: project.map(Path::to_path_buf),
                clients: clients.to_vec(),
            });
        }
    }
}

/// Removes `//` and `/* */` comments and trailing commas (OpenCode accepts JSONC).
fn strip_jsonc(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let (mut i, mut in_str) = (0, false);
    while i < chars.len() {
        let c = chars[i];
        if in_str {
            out.push(c);
            if c == '\\' && i + 1 < chars.len() {
                out.push(chars[i + 1]);
                i += 1;
            } else if c == '"' {
                in_str = false;
            }
        } else if c == '"' {
            in_str = true;
            out.push(c);
        } else if c == '/' && chars.get(i + 1) == Some(&'/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        } else if c == '/' && chars.get(i + 1) == Some(&'*') {
            i += 2;
            while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            i += 2;
            continue;
        } else if c == ',' {
            let next = chars[i + 1..].iter().find(|c| !c.is_whitespace());
            if !matches!(next, Some('}') | Some(']')) {
                out.push(c);
            }
        } else {
            out.push(c);
        }
        i += 1;
    }
    out
}

pub fn scan(env: &Env, projects: &[PathBuf], estimator: &dyn TokenEstimator) -> Catalog {
    const CLAUDE: &[Source] = &[Source::ClaudeCode];
    const OPENCODE: &[Source] = &[Source::OpenCode];
    const COPILOT: &[Source] = &[Source::CopilotCli, Source::VsCodeCopilot];
    let mut s = Scan {
        estimator,
        out: Catalog::default(),
    };
    let opencode_cfg = config_home(env).join("opencode");

    s.agents(&claude_dir(env).join("agents"), ".md", CLAUDE, None);
    s.agents(&opencode_cfg.join("agents"), ".md", OPENCODE, None);
    s.agents(&opencode_cfg.join("agent"), ".md", OPENCODE, None);
    s.agents(&copilot_dir(env).join("agents"), ".agent.md", COPILOT, None);

    let claude_json = match env.var("CLAUDE_CONFIG_DIR") {
        Some(dir) => PathBuf::from(dir).join(".claude.json"),
        None => env.home().join(".claude.json"),
    };
    s.mcp(&claude_json, "/mcpServers", CLAUDE, None, None);
    for ext in ["json", "jsonc"] {
        s.mcp(
            &opencode_cfg.join(format!("opencode.{ext}")),
            "/mcp",
            OPENCODE,
            None,
            None,
        );
    }
    s.mcp(
        &copilot_dir(env).join("mcp-config.json"),
        "/mcpServers",
        &[Source::CopilotCli],
        None,
        None,
    );

    for project in projects {
        let p = Some(project.as_path());
        s.agents(&project.join(".claude/agents"), ".md", CLAUDE, p);
        s.agents(&project.join(".github/agents"), ".agent.md", COPILOT, p);
        s.agents(&project.join(".opencode/agents"), ".md", OPENCODE, p);
        s.agents(&project.join(".opencode/agent"), ".md", OPENCODE, p);
        s.mcp(&claude_json, "/mcpServers", CLAUDE, p, Some(project));
        s.mcp(&project.join(".mcp.json"), "/mcpServers", CLAUDE, p, None);
        s.mcp(
            &project.join(".vscode/mcp.json"),
            "/servers",
            &[Source::VsCodeCopilot],
            p,
            None,
        );
        for ext in ["json", "jsonc"] {
            s.mcp(
                &project.join(format!("opencode.{ext}")),
                "/mcp",
                OPENCODE,
                p,
                None,
            );
            s.mcp(
                &project.join(format!(".opencode/opencode.{ext}")),
                "/mcp",
                OPENCODE,
                p,
                None,
            );
        }
    }
    s.out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokens::CharsPerToken;

    fn write(root: &Path, rel: &str, text: &str) {
        let p = root.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    }

    #[test]
    fn jsonc_comments_and_trailing_commas_are_removed() {
        let v: Value = serde_json::from_str(&strip_jsonc(
            "{\n // c\n \"a\": \"x // not a comment\", /* b */ \"b\": [1,2,],\n}",
        ))
        .unwrap();
        assert_eq!(v["a"], "x // not a comment");
        assert_eq!(v["b"][1], 2);
    }

    #[test]
    fn agents_and_server_names_are_found_without_reading_config_values() {
        let home = tempfile::tempdir().unwrap();
        let h = home.path();
        write(
            h,
            ".claude/agents/reviewer.md",
            "---\nname: reviewer\ndescription: Reviews code\n---\nbody",
        );
        write(
            h,
            "repo/.github/agents/triage.agent.md",
            "---\ndescription: Triage issues\n---\nbody",
        );
        write(
            h,
            ".claude.json",
            r#"{"mcpServers":{"docs":{"command":"secret-cmd","env":{"TOKEN":"abc"}}},
            "projects":{"REPO":{"mcpServers":{"local":{"command":"x"}}}}}"#
                .replace(
                    "REPO",
                    &h.join("repo").to_string_lossy().replace('\\', "\\\\"),
                )
                .as_str(),
        );
        write(
            h,
            "repo/.mcp.json",
            r#"{"mcpServers":{"proj":{"url":"https://example.test"}}}"#,
        );
        write(
            h,
            ".config/opencode/opencode.jsonc",
            "{ // servers\n \"mcp\": {\"oc\": {\"type\":\"local\"}, \"off\": {\"enabled\": false},},\n}",
        );
        let project = h.join("repo");
        let c = scan(
            &Env::for_home(h),
            std::slice::from_ref(&project),
            &CharsPerToken,
        );
        let agents: Vec<_> = c
            .agents
            .iter()
            .map(|a| (a.name.as_str(), a.project.is_some()))
            .collect();
        assert_eq!(agents, [("reviewer", false), ("triage", true)]);
        assert_eq!(
            c.agents[0].listing_tokens.count,
            "reviewerReviews code".len().div_ceil(4) as u64
        );
        let servers: Vec<_> = c
            .mcp_servers
            .iter()
            .map(|m| (m.name.as_str(), m.project.is_some(), m.clients[0]))
            .collect();
        assert_eq!(
            servers,
            [
                ("docs", false, Source::ClaudeCode),
                ("oc", false, Source::OpenCode),
                ("local", true, Source::ClaudeCode),
                ("proj", true, Source::ClaudeCode),
            ]
        );
        let json = serde_json::to_string(&c).unwrap();
        assert!(
            !json.contains("secret-cmd") && !json.contains("abc") && !json.contains("example.test")
        );
    }

    #[test]
    fn unparsable_config_is_a_warning() {
        let home = tempfile::tempdir().unwrap();
        write(home.path(), ".claude.json", "{ not json");
        let c = scan(&Env::for_home(home.path()), &[], &CharsPerToken);
        assert_eq!(c.warnings.len(), 1);
    }
}
