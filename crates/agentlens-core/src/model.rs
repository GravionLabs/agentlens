//! Normalized session and event types. Every adapter emits these; analyzers
//! only ever see these.
//!
//! Timestamps are [`jiff::Timestamp`] (UTC instants).

use std::path::PathBuf;

use jiff::Timestamp;
use serde::{Deserialize, Serialize};

/// The kind of data source a session was read from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Source {
    #[serde(rename = "claude-code")]
    ClaudeCode,
    #[serde(rename = "opencode")]
    OpenCode,
    #[serde(rename = "copilot-cli")]
    CopilotCli,
    #[serde(rename = "vscode-copilot")]
    VsCodeCopilot,
    #[serde(rename = "otel")]
    Otel,
}

impl Source {
    /// Stable identifier used on the command line and in reports.
    pub fn id(self) -> &'static str {
        match self {
            Source::ClaudeCode => "claude-code",
            Source::OpenCode => "opencode",
            Source::CopilotCli => "copilot-cli",
            Source::VsCodeCopilot => "vscode-copilot",
            Source::Otel => "otel",
        }
    }

    /// Parses the identifier returned by [`Source::id`].
    pub fn from_id(id: &str) -> Option<Source> {
        [
            Source::ClaudeCode,
            Source::OpenCode,
            Source::CopilotCli,
            Source::VsCodeCopilot,
            Source::Otel,
        ]
        .into_iter()
        .find(|s| s.id() == id)
    }
}

/// One coding-agent session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub source: Source,
    pub project_dir: Option<PathBuf>,
    pub branch: Option<String>,
    /// The main model, if the client records one.
    pub model: Option<String>,
    pub started_at: Option<Timestamp>,
    pub ended_at: Option<Timestamp>,
}

impl Session {
    pub fn new(id: impl Into<String>, source: Source) -> Self {
        Session {
            id: id.into(),
            source,
            project_dir: None,
            branch: None,
            model: None,
            started_at: None,
            ended_at: None,
        }
    }
}

/// A non-fatal problem found while reading a session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Warning {
    pub message: String,
    /// Where it happened, e.g. `file.jsonl:42`.
    pub location: Option<String>,
}

impl Warning {
    pub fn new(message: impl Into<String>) -> Self {
        Warning {
            message: message.into(),
            location: None,
        }
    }

    pub fn at(mut self, location: impl Into<String>) -> Self {
        self.location = Some(location.into());
        self
    }
}

/// A session together with its events (in the order they happened) and the
/// warnings collected while reading it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionEvents {
    pub session: Session,
    pub events: Vec<Event>,
    pub warnings: Vec<Warning>,
}

impl SessionEvents {
    pub fn new(session: Session) -> Self {
        SessionEvents {
            session,
            events: Vec::new(),
            warnings: Vec::new(),
        }
    }

    pub fn push(&mut self, at: Option<Timestamp>, kind: EventKind) {
        self.events.push(Event { at, kind });
    }

    pub fn warn(&mut self, warning: Warning) {
        self.warnings.push(warning);
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub at: Option<Timestamp>,
    pub kind: EventKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum EventKind {
    /// The user sent a message; starts a new turn.
    UserTurn,
    ToolCall(ToolCall),
    SkillInvoked(SkillInvoked),
    Usage(Usage),
    /// The context was compacted; earlier content no longer counts as carried.
    Compaction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ToolStatus {
    Ok,
    Error,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCall {
    pub name: String,
    /// Short, privacy-conscious summary of the input (e.g. a file path), never the full input.
    pub input_summary: Option<String>,
    pub status: ToolStatus,
    pub duration_ms: Option<u64>,
}

/// How a skill was started.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SkillTrigger {
    /// The model called the skill tool by itself.
    Model,
    /// The user typed the skill as a command.
    User,
    /// Another skill started it.
    Nested,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillInvoked {
    pub name: String,
    pub trigger: SkillTrigger,
    pub path: Option<PathBuf>,
    /// Size of the content the client put into the context, if known.
    pub content_bytes: Option<u64>,
    /// Estimated tokens of that content (never measured), if known.
    pub content_tokens: Option<u64>,
}

impl SkillInvoked {
    pub fn new(name: impl Into<String>, trigger: SkillTrigger) -> Self {
        SkillInvoked {
            name: name.into(),
            trigger,
            path: None,
            content_bytes: None,
            content_tokens: None,
        }
    }
}

/// Token usage of one model request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Usage {
    pub model: Option<String>,
    /// Input tokens that were not served from the cache.
    pub input: u64,
    /// Output tokens. For Claude Code this already includes the reasoning tokens.
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    /// Informational; see the adapter for whether it is a part of `output` or separate.
    pub reasoning: u64,
    pub cost_usd: Option<f64>,
    /// `true` if the client reported these numbers, `false` if they are estimated.
    pub measured: bool,
    /// The request was made by a subagent.
    pub subagent: bool,
}

impl Usage {
    /// Everything the request sent to the model, cached or not.
    pub fn total_input(&self) -> u64 {
        self.input + self.cache_read + self.cache_write
    }
}

impl Default for Usage {
    fn default() -> Self {
        Usage {
            model: None,
            input: 0,
            output: 0,
            cache_read: 0,
            cache_write: 0,
            reasoning: 0,
            cost_usd: None,
            measured: true,
            subagent: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_events_round_trip_through_json() {
        let mut s = Session::new("s1", Source::ClaudeCode);
        s.project_dir = Some("/work/demo".into());
        s.started_at = Some("2026-10-01T10:00:00Z".parse().unwrap());
        let mut se = SessionEvents::new(s);
        se.push(None, EventKind::UserTurn);
        se.push(
            Some("2026-10-01T10:00:05Z".parse().unwrap()),
            EventKind::SkillInvoked(SkillInvoked::new("demo", SkillTrigger::Model)),
        );
        se.push(
            None,
            EventKind::ToolCall(ToolCall {
                name: "Read".into(),
                input_summary: Some("src/main.rs".into()),
                status: ToolStatus::Ok,
                duration_ms: Some(12),
            }),
        );
        se.push(
            None,
            EventKind::Usage(Usage {
                input: 10,
                output: 5,
                ..Usage::default()
            }),
        );
        se.push(None, EventKind::Compaction);
        se.warn(Warning::new("bad line").at("a.jsonl:3"));

        let json = serde_json::to_string(&se).unwrap();
        let back: SessionEvents = serde_json::from_str(&json).unwrap();
        assert_eq!(back, se);
    }

    #[test]
    fn serialized_source_matches_its_id() {
        for s in [
            Source::ClaudeCode,
            Source::OpenCode,
            Source::CopilotCli,
            Source::VsCodeCopilot,
            Source::Otel,
        ] {
            assert_eq!(
                serde_json::to_string(&s).unwrap(),
                format!("\"{}\"", s.id())
            );
        }
    }

    #[test]
    fn source_ids_round_trip() {
        for s in [
            Source::ClaudeCode,
            Source::OpenCode,
            Source::CopilotCli,
            Source::VsCodeCopilot,
            Source::Otel,
        ] {
            assert_eq!(Source::from_id(s.id()), Some(s));
        }
        assert_eq!(Source::from_id("nope"), None);
    }

    #[test]
    fn total_input_includes_cache() {
        let u = Usage {
            input: 1,
            cache_read: 10,
            cache_write: 100,
            ..Usage::default()
        };
        assert_eq!(u.total_input(), 111);
    }
}
