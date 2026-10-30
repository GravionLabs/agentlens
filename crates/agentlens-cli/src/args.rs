use agentlens_core::adapter::SessionFilter;
use agentlens_core::api::{DEFAULT_UNUSED_DAYS, Filters};
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

    /// Only sessions that ended before this point (same forms as --since); with --since a closed period
    #[arg(long, global = true, value_name = "WHEN")]
    pub until: Option<String>,

    /// Only this source (repeatable): claude-code, opencode, copilot-cli, vscode-copilot
    #[arg(long, global = true, value_name = "ID", value_parser = parse_source)]
    pub source: Vec<Source>,

    /// Only sessions that ran in this project directory (or below it)
    #[arg(long, global = true, value_name = "DIR")]
    pub project: Option<std::path::PathBuf>,

    /// Only the session with this id (repeatable); the id of Claude Code's `session_id` or OpenCode's session
    #[arg(long, global = true, value_name = "ID")]
    pub session: Vec<String>,

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
        /// Which table `--format csv` writes: the cache table or the roles table
        #[arg(long, value_enum, default_value_t = EfficiencyCsv::Cache)]
        csv_table: EfficiencyCsv,
    },
    /// Show what the sessions cost
    Cost {
        /// Group the cost by model, project, git branch, issue number (from the branch name, else from the session's commit messages) or session
        #[arg(long, value_enum, default_value_t = CostBy::Model)]
        by: CostBy,
        /// Price file that overrides or extends the bundled prices
        #[arg(long, value_name = "FILE")]
        prices: Option<std::path::PathBuf>,
        /// Recompute the usage with the prices of this model (id prefix) and compare
        #[arg(long, value_name = "MODEL")]
        what_if: Option<String>,
        /// Rows of the session table with --by session (0 shows all); JSON and CSV always contain everything
        #[arg(long, default_value_t = 25)]
        top: usize,
    },
    /// List installed skills, agents and MCP servers that were not used
    Unused {
        /// Length of the period to look at (ignored if --since is given; counted back from --until if set)
        #[arg(long, value_name = "DAYS", default_value_t = DEFAULT_UNUSED_DAYS)]
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
pub enum EfficiencyCsv {
    Cache,
    Roles,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum CostBy {
    Model,
    Project,
    Branch,
    Issue,
    Session,
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
    /// The filter flags as the filter of the application and the MCP server.
    pub fn filters(&self) -> Filters {
        Filters {
            since: self.since.clone(),
            until: self.until.clone(),
            sources: self.source.iter().map(|s| s.id().to_owned()).collect(),
            project: self.project.clone(),
            session: self.session.clone(),
        }
    }

    /// The session filter the core resolves from the flags; errors name `--since` and `--until`.
    pub fn filter(&self) -> Result<SessionFilter> {
        let (filter, _) = self
            .filters()
            .resolve_named(Timestamp::now(), ("--since", "--until"))
            .map_err(anyhow::Error::msg)?;
        Ok(filter)
    }
}
