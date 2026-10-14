use agentlens_core::adapter::SessionFilter;
use agentlens_core::model::Source;
use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use jiff::Timestamp;

#[derive(Parser, Debug)]
#[command(
    name = "agentlens",
    version,
    about = "Analyze the local sessions of coding agents"
)]
pub struct Cli {
    /// Only sessions active since this point: a duration (`12h`, `30d`, `4w`) or a date (`2026-10-01`)
    #[arg(long, global = true, value_name = "WHEN")]
    pub since: Option<String>,

    /// Only this source (repeatable): claude-code, opencode, copilot-cli, vscode-copilot
    #[arg(long, global = true, value_name = "ID", value_parser = parse_source)]
    pub source: Vec<Source>,

    /// Only sessions that ran in this project directory (or below it)
    #[arg(long, global = true, value_name = "DIR")]
    pub project: Option<std::path::PathBuf>,

    /// OpenTelemetry export (OTLP JSON lines of Claude Code logs) to analyze as well (repeatable)
    #[arg(long, global = true, value_name = "FILE")]
    pub otel: Vec<std::path::PathBuf>,

    /// Count the text of skills with the Anthropic count_tokens API instead of estimating (skills,
    /// context, unused, compare, report). Needs ANTHROPIC_API_KEY and sends the text of your skills
    /// to the API; without this flag nothing leaves the machine.
    #[arg(long, global = true)]
    pub exact: bool,

    /// Model whose tokenizer --exact uses
    #[arg(long, global = true, value_name = "MODEL", requires = "exact")]
    pub exact_model: Option<String>,

    /// Output format
    #[arg(long, global = true, value_enum, default_value_t = Format::Table)]
    pub format: Format,

    /// Show token counts in text tables as 1.55 M, 2.11 k or 3.20 B; JSON and CSV stay exact
    #[arg(long, global = true)]
    pub rounded: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Read an OpenTelemetry export and report what it holds
    ImportOtel {
        /// OTLP JSON-lines file written by an OpenTelemetry Collector `file` exporter
        file: std::path::PathBuf,
    },
    /// Show which sources were found and how many sessions they hold
    Scan,
    /// Show which skills are used and what they cost in tokens
    Skills,
    /// Show what to change to save tokens: unused skills, costly descriptions, large contexts
    Advise,
    /// Show what the clients put into the context besides the conversation
    Context {
        /// Which kind of static context to show
        #[arg(long, value_enum, default_value_t = ContextView::All)]
        kind: ContextView,
    },
    /// Show how sessions unfold: context growth, compactions, tool errors, hooks
    Flow {
        /// Which section to show
        #[arg(long, value_enum, default_value_t = FlowSection::All)]
        section: FlowSection,
        /// Rows per table (0 shows all); JSON and CSV always contain everything
        #[arg(long, default_value_t = 25)]
        top: usize,
    },
    /// Compare the clients side by side over the same period
    Compare,
    /// Show cache hit rate and the share of work done by subagents
    Efficiency {
        /// Group the cache table by session, project or source
        #[arg(long, value_enum, default_value_t = EfficiencyBy::Source)]
        by: EfficiencyBy,
        /// Rows of the cache table (0 shows all); JSON and CSV always contain everything
        #[arg(long, default_value_t = 25)]
        top: usize,
        /// Price file that overrides or extends the bundled prices (used for the cost share)
        #[arg(long, value_name = "FILE")]
        prices: Option<std::path::PathBuf>,
    },
    /// Show what the sessions cost
    Cost {
        /// Group the cost by model, project, git branch or issue number (parsed from the branch name)
        #[arg(long, value_enum, default_value_t = CostBy::Model)]
        by: CostBy,
        /// Price file that overrides or extends the bundled prices
        #[arg(long, value_name = "FILE")]
        prices: Option<std::path::PathBuf>,
        /// Recompute the usage with the prices of this model (id prefix) and compare
        #[arg(long, value_name = "MODEL")]
        what_if: Option<String>,
    },
    /// List installed skills, agents and MCP servers that were not used
    Unused {
        /// Length of the period to look at (ignored if --since is given)
        #[arg(long, value_name = "DAYS", default_value_t = 30)]
        days: u32,
        /// Also list items that were used
        #[arg(long)]
        all: bool,
    },
    /// Show how to register the MCP server in the coding agents, and register it
    ///
    /// Without `--write` or `--remove` nothing is changed: for each client the configuration to add is
    /// printed. `--write` adds the server to the configuration of the clients (a backup of the file is
    /// made first), `--remove` takes it out again. This is the only command that writes into files of the
    /// clients.
    McpSetup {
        /// Client to handle; repeat for several. Default: the clients found on this machine
        #[arg(long = "client", value_enum)]
        clients: Vec<McpClient>,
        /// Register the server in the configuration of the clients
        #[arg(long)]
        write: bool,
        /// Remove the registration again
        #[arg(long, conflicts_with = "write")]
        remove: bool,
        /// Path of agentlens-mcp (default: next to this program, or on PATH)
        #[arg(long, value_name = "FILE")]
        binary: Option<std::path::PathBuf>,
        /// Do not ask before changing a file
        #[arg(long)]
        yes: bool,
    },
    /// Write a self-contained report
    Report {
        /// Write an HTML file that works offline
        #[arg(long, value_name = "FILE")]
        html: std::path::PathBuf,
    },
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContextView {
    All,
    /// Tool lists and definitions and MCP server instructions
    Tools,
    Instructions,
    Agents,
    Skills,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum EfficiencyBy {
    Session,
    Project,
    Source,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum CostBy {
    Model,
    Project,
    Branch,
    Issue,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlowSection {
    All,
    Growth,
    Errors,
    Hooks,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum McpClient {
    ClaudeCode,
    Vscode,
    CopilotCli,
    Opencode,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Table,
    Json,
    Csv,
}

fn parse_source(s: &str) -> Result<Source, String> {
    Source::from_id(s).ok_or_else(|| {
        "unknown source, expected one of: claude-code, opencode, copilot-cli, vscode-copilot"
            .to_owned()
    })
}

impl Cli {
    pub fn filter(&self) -> Result<SessionFilter> {
        let since = match &self.since {
            Some(s) => Some(parse_since(s, Timestamp::now())?),
            None => None,
        };
        Ok(SessionFilter {
            since,
            until: None,
            project: self.project.clone(),
        })
    }
}

/// The instant `days` days before `now`.
pub fn days_ago(days: u32, now: Timestamp) -> Result<Timestamp> {
    parse_since(&format!("{days}d"), now)
}

/// `30d`, `12h`, `4w` (relative to `now`), a date (`2026-10-01`, midnight UTC) or an RFC 3339 instant.
pub fn parse_since(text: &str, now: Timestamp) -> Result<Timestamp> {
    agentlens_core::since::parse_since(text, now).map_err(|e| anyhow::anyhow!("--since: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> Timestamp {
        "2026-10-10T12:00:00Z".parse().unwrap()
    }

    #[test]
    fn durations_are_relative_to_now() {
        assert_eq!(
            parse_since("30d", now()).unwrap().to_string(),
            "2026-09-10T12:00:00Z"
        );
        assert_eq!(
            parse_since("12h", now()).unwrap().to_string(),
            "2026-10-10T00:00:00Z"
        );
        assert_eq!(
            parse_since("2w", now()).unwrap().to_string(),
            "2026-09-26T12:00:00Z"
        );
    }

    #[test]
    fn dates_and_instants_are_absolute() {
        assert_eq!(
            parse_since("2026-10-01", now()).unwrap().to_string(),
            "2026-10-01T00:00:00Z"
        );
        assert_eq!(
            parse_since("2026-10-01T08:30:00Z", now())
                .unwrap()
                .to_string(),
            "2026-10-01T08:30:00Z"
        );
    }

    #[test]
    fn garbage_is_rejected_with_a_hint() {
        let err = parse_since("soon", now()).unwrap_err().to_string();
        assert!(err.contains("30d"), "{err}");
        assert!(parse_since("d", now()).is_err());
        assert!(parse_since("5x", now()).is_err());
    }
}
