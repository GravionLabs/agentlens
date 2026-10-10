//! The adapter interface: one implementation per session data source.

use std::path::PathBuf;

use jiff::Timestamp;

use crate::env::Env;
use crate::model::{Session, SessionEvents, Source};

/// A problem that prevents reading a whole session (as opposed to a
/// [`crate::model::Warning`], which only skips a record).
#[derive(Debug, thiserror::Error)]
pub enum AdapterError {
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{0}")]
    Other(String),
}

/// What an adapter found on this machine.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Detection {
    pub found: bool,
    pub roots: Vec<PathBuf>,
    pub note: Option<String>,
}

impl Detection {
    pub fn not_found(note: impl Into<String>) -> Self {
        Detection {
            found: false,
            roots: Vec::new(),
            note: Some(note.into()),
        }
    }

    pub fn found(roots: Vec<PathBuf>) -> Self {
        Detection {
            found: true,
            roots,
            note: None,
        }
    }
}

/// Restricts which sessions are read.
#[derive(Debug, Clone, Default)]
pub struct SessionFilter {
    /// Only sessions that were active at or after this instant.
    pub since: Option<Timestamp>,
    /// Only sessions whose project directory is this directory or inside it.
    pub project: Option<PathBuf>,
}

impl SessionFilter {
    /// `true` if the session passes the filter. Sessions without the needed
    /// metadata pass, so that missing data does not hide them.
    pub fn matches(&self, session: &Session) -> bool {
        if let (Some(since), Some(last)) = (self.since, session.ended_at.or(session.started_at))
            && last < since
        {
            return false;
        }
        if let (Some(project), Some(dir)) = (&self.project, &session.project_dir)
            && !dir.starts_with(project)
        {
            return false;
        }
        true
    }
}

pub type SessionIter<'a> = Box<dyn Iterator<Item = Result<SessionEvents, AdapterError>> + 'a>;

/// Reads the sessions of one source and translates them into normalized events.
pub trait Adapter: Send + Sync {
    fn id(&self) -> Source;

    /// The sources of the sessions this adapter yields (`--source` selects by
    /// these). Differs from `id` for imports whose sessions belong to a client.
    fn sources(&self) -> Vec<Source> {
        vec![self.id()]
    }

    /// Cheap check whether data for this source exists; must not read sessions.
    fn detect(&self, env: &Env) -> Detection;

    /// Reads sessions lazily. Adapters should skip data the filter obviously
    /// excludes (e.g. by file modification time); the registry additionally
    /// applies [`SessionFilter::matches`].
    fn sessions<'a>(&'a self, env: &'a Env, filter: &'a SessionFilter) -> SessionIter<'a>;
}

/// All adapters known to the program.
#[derive(Default)]
pub struct Registry {
    adapters: Vec<Box<dyn Adapter>>,
}

impl Registry {
    pub fn new() -> Self {
        Registry::default()
    }

    pub fn register(&mut self, adapter: Box<dyn Adapter>) {
        self.adapters.push(adapter);
    }

    pub fn adapters(&self) -> impl Iterator<Item = &dyn Adapter> {
        self.adapters.iter().map(|a| a.as_ref())
    }

    pub fn get(&self, source: Source) -> Option<&dyn Adapter> {
        self.adapters().find(|a| a.id() == source)
    }

    pub fn detect_all(&self, env: &Env) -> Vec<(Source, Detection)> {
        self.adapters().map(|a| (a.id(), a.detect(env))).collect()
    }

    /// Sessions of all adapters whose source is in `only` (all if empty) that
    /// pass `filter`. Adapters that were not detected are skipped.
    pub fn sessions<'a>(
        &'a self,
        env: &'a Env,
        filter: &'a SessionFilter,
        only: &'a [Source],
    ) -> impl Iterator<Item = Result<SessionEvents, AdapterError>> + 'a {
        self.adapters()
            .filter(move |a| only.is_empty() || a.sources().iter().any(|s| only.contains(s)))
            .filter(move |a| a.detect(env).found)
            .flat_map(move |a| a.sessions(env, filter))
            .filter(move |r| {
                r.as_ref()
                    .map(|s| filter.matches(&s.session))
                    .unwrap_or(true)
            })
            // A session that several adapters report (local data and an
            // OpenTelemetry export) counts once: the adapter registered first wins.
            .filter({
                let mut seen = std::collections::HashSet::new();
                move |r| match r {
                    Ok(s) => seen.insert((s.session.source, s.session.id.clone())),
                    Err(_) => true,
                }
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Dummy {
        source: Source,
        found: bool,
        sessions: Vec<Session>,
    }

    impl Adapter for Dummy {
        fn id(&self) -> Source {
            self.source
        }

        fn detect(&self, _env: &Env) -> Detection {
            if self.found {
                Detection::found(vec!["/dummy".into()])
            } else {
                Detection::not_found("absent")
            }
        }

        fn sessions<'a>(&'a self, _env: &'a Env, _filter: &'a SessionFilter) -> SessionIter<'a> {
            Box::new(
                self.sessions
                    .iter()
                    .cloned()
                    .map(|s| Ok(SessionEvents::new(s))),
            )
        }
    }

    fn registry() -> Registry {
        let mut old = Session::new("old", Source::ClaudeCode);
        old.ended_at = Some("2026-01-01T00:00:00Z".parse().unwrap());
        let mut new = Session::new("new", Source::ClaudeCode);
        new.ended_at = Some("2026-10-01T00:00:00Z".parse().unwrap());
        new.project_dir = Some("/work/a/sub".into());
        let mut r = Registry::new();
        r.register(Box::new(Dummy {
            source: Source::ClaudeCode,
            found: true,
            sessions: vec![old, new],
        }));
        r.register(Box::new(Dummy {
            source: Source::OpenCode,
            found: false,
            sessions: vec![Session::new("x", Source::OpenCode)],
        }));
        r
    }

    fn ids(r: &Registry, filter: &SessionFilter, only: &[Source]) -> Vec<String> {
        let env = Env::for_home("/h");
        r.sessions(&env, filter, only)
            .map(|s| s.unwrap().session.id)
            .collect()
    }

    #[test]
    fn adapter_is_discovered_through_the_registry() {
        let r = registry();
        let env = Env::for_home("/h");
        let det = r.detect_all(&env);
        assert_eq!(det.len(), 2);
        assert!(det[0].1.found && !det[1].1.found);
        assert!(r.get(Source::ClaudeCode).is_some());
        assert!(r.get(Source::CopilotCli).is_none());
    }

    #[test]
    fn undetected_adapters_are_skipped() {
        assert_eq!(
            ids(&registry(), &SessionFilter::default(), &[]),
            ["old", "new"]
        );
    }

    #[test]
    fn since_filter_drops_old_sessions() {
        let f = SessionFilter {
            since: Some("2026-06-01T00:00:00Z".parse().unwrap()),
            project: None,
        };
        assert_eq!(ids(&registry(), &f, &[]), ["new"]);
    }

    #[test]
    fn project_filter_keeps_sessions_without_project_metadata() {
        let f = SessionFilter {
            since: None,
            project: Some("/work/a".into()),
        };
        assert_eq!(ids(&registry(), &f, &[]), ["old", "new"]);
        let f = SessionFilter {
            since: None,
            project: Some("/work/b".into()),
        };
        assert_eq!(ids(&registry(), &f, &[]), ["old"]);
    }

    #[test]
    fn source_selection_restricts_adapters() {
        assert!(ids(&registry(), &SessionFilter::default(), &[Source::OpenCode]).is_empty());
    }
}
