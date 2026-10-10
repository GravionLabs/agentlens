use agentlens_core::adapter::SessionFilter;
use agentlens_core::model::Source;
use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use jiff::{Span, Timestamp, tz::TimeZone};

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

    /// Only this source (repeatable): claude-code, opencode, copilot-cli
    #[arg(long, global = true, value_name = "ID", value_parser = parse_source)]
    pub source: Vec<Source>,

    /// Only sessions that ran in this project directory (or below it)
    #[arg(long, global = true, value_name = "DIR")]
    pub project: Option<std::path::PathBuf>,

    /// Output format
    #[arg(long, global = true, value_enum, default_value_t = Format::Table)]
    pub format: Format,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Show which sources were found and how many sessions they hold
    Scan,
    /// Show which skills are used and what they cost in tokens
    Skills,
    /// Show what the clients put into the context besides the conversation
    Context {
        /// Which kind of static context to show
        #[arg(long, value_enum, default_value_t = ContextView::All)]
        kind: ContextView,
    },
    /// Show how sessions unfold: context growth, compactions
    Flow {
        /// Which section to show
        #[arg(long, value_enum, default_value_t = FlowSection::All)]
        section: FlowSection,
        /// Rows per table (0 shows all); JSON and CSV always contain everything
        #[arg(long, default_value_t = 25)]
        top: usize,
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
pub enum FlowSection {
    All,
    Growth,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Table,
    Json,
    Csv,
}

fn parse_source(s: &str) -> Result<Source, String> {
    Source::from_id(s).ok_or_else(|| {
        "unknown source, expected one of: claude-code, opencode, copilot-cli".to_owned()
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
    let text = text.trim();
    let split = text
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(text.len());
    let (digits, unit) = text.split_at(split);
    if !digits.is_empty() && matches!(unit, "h" | "d" | "w") {
        let n: i64 = digits.parse().context("invalid number in --since")?;
        let span = match unit {
            "h" => Span::new().hours(n),
            "d" => Span::new().days(n),
            _ => Span::new().weeks(n),
        };
        let zoned = now
            .to_zoned(TimeZone::UTC)
            .checked_sub(span)
            .context("--since is out of range")?;
        return Ok(zoned.timestamp());
    }
    if let Ok(date) = text.parse::<jiff::civil::Date>() {
        return Ok(date.at(0, 0, 0, 0).to_zoned(TimeZone::UTC)?.timestamp());
    }
    if let Ok(ts) = text.parse::<Timestamp>() {
        return Ok(ts);
    }
    bail!("cannot parse --since `{text}`: use a duration like 30d or a date like 2026-10-01")
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
