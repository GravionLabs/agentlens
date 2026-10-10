//! The adapter interface: one implementation per session data source.

use std::collections::HashMap;
use std::path::PathBuf;

use jiff::Timestamp;

use crate::env::Env;
use crate::model::{Event, EventKind, Session, SessionEvents, Source};

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

    /// `true` for adapters that read an export of sessions other adapters read
    /// locally (OpenTelemetry). The registry merges such a session into its
    /// local copy instead of counting it twice, see [`merge_export`].
    fn is_export(&self) -> bool {
        false
    }

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

/// Merges an export (OpenTelemetry) of a session into its local copy.
///
/// The export's usage events carry what the client measured itself (cost, skill
/// attribution) and replace the local usage events inside the time range the
/// export covers; local usage outside that range stays, so a partial export
/// does not undercount. Everything else comes from the local copy, which also
/// supplies project, branch and model where the export has none.
pub fn merge_export(local: SessionEvents, export: SessionEvents) -> SessionEvents {
    let usage_times: Vec<Timestamp> = export
        .events
        .iter()
        .filter(|e| matches!(e.kind, EventKind::Usage(_)))
        .filter_map(|e| e.at)
        .collect();
    let (Some(&from), Some(&to)) = (usage_times.iter().min(), usage_times.iter().max()) else {
        // The export holds no timed usage: nothing to replace.
        return local;
    };
    let in_range = |e: &Event| e.at.is_some_and(|t| t >= from && t <= to);
    let mut merged = local;
    merged
        .events
        .retain(|e| !(matches!(e.kind, EventKind::Usage(_)) && in_range(e)));
    merged.events.extend(
        export
            .events
            .into_iter()
            .filter(|e| matches!(e.kind, EventKind::Usage(_)) && in_range(e)),
    );
    // Order by time; an event without a time stays behind the event before it.
    let mut last = None;
    let mut keyed: Vec<(Option<Timestamp>, Event)> = merged
        .events
        .into_iter()
        .map(|e| {
            last = e.at.or(last);
            (last, e)
        })
        .collect();
    keyed.sort_by_key(|(k, _)| *k);
    merged.events = keyed.into_iter().map(|(_, e)| e).collect();
    merged.warnings.extend(export.warnings);
    if merged.session.model.is_none() {
        merged.session.model = export.session.model;
    }
    merged.session.started_at = merged.session.started_at.or(export.session.started_at);
    merged.session.ended_at = merged.session.ended_at.max(export.session.ended_at);
    merged
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
    ) -> Box<dyn Iterator<Item = Result<SessionEvents, AdapterError>> + 'a> {
        let selected: Vec<&dyn Adapter> = self
            .adapters()
            .filter(|a| only.is_empty() || a.sources().iter().any(|s| only.contains(s)))
            .filter(|a| a.detect(env).found)
            .collect();
        let (exports, locals): (Vec<&dyn Adapter>, Vec<&dyn Adapter>) =
            selected.into_iter().partition(|a| a.is_export());
        let matches = move |r: &Result<SessionEvents, AdapterError>| {
            r.as_ref()
                .map(|s| filter.matches(&s.session))
                .unwrap_or(true)
        };
        let local = locals
            .into_iter()
            .flat_map(move |a| a.sessions(env, filter));
        if exports.is_empty() {
            return Box::new(local.filter(matches));
        }

        // Exports are read first: a session that is also local is merged, one
        // that is not stays as it is.
        let mut exported: HashMap<(Source, String), SessionEvents> = HashMap::new();
        let mut errors = Vec::new();
        for r in exports.into_iter().flat_map(|a| a.sessions(env, filter)) {
            match r {
                Ok(s) => {
                    exported.insert((s.session.source, s.session.id.clone()), s);
                }
                Err(e) => errors.push(Err(e)),
            }
        }
        let mut local = local;
        let mut leftovers: Option<Vec<SessionEvents>> = None;
        let merged = std::iter::from_fn(move || match local.next() {
            Some(Ok(s)) => {
                let key = (s.session.source, s.session.id.clone());
                Some(Ok(match exported.remove(&key) {
                    Some(export) => merge_export(s, export),
                    None => s,
                }))
            }
            Some(err) => Some(err),
            // Local sessions are done: emit the exports without a local copy.
            None => leftovers
                .get_or_insert_with(|| {
                    let mut v: Vec<SessionEvents> = exported.drain().map(|(_, s)| s).collect();
                    v.sort_by(|a, b| b.session.id.cmp(&a.session.id));
                    v
                })
                .pop()
                .map(Ok),
        });
        Box::new(errors.into_iter().chain(merged).filter(matches))
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

    fn usage_at(at: &str, input: u64, skill: Option<&str>) -> (Option<Timestamp>, EventKind) {
        (
            Some(at.parse().unwrap()),
            EventKind::Usage(crate::model::Usage {
                input,
                skill: skill.map(Into::into),
                ..crate::model::Usage::default()
            }),
        )
    }

    fn session_with(events: Vec<(Option<Timestamp>, EventKind)>) -> SessionEvents {
        let mut se = SessionEvents::new(Session::new("s", Source::ClaudeCode));
        for (at, k) in events {
            se.push(at, k);
        }
        se
    }

    fn inputs(se: &SessionEvents) -> Vec<(u64, Option<String>)> {
        se.events
            .iter()
            .filter_map(|e| match &e.kind {
                EventKind::Usage(u) => Some((u.input, u.skill.clone())),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn export_usage_replaces_local_usage_inside_its_range_only() {
        let local = session_with(vec![
            usage_at("2026-10-01T10:00:00Z", 1, None),
            usage_at("2026-10-01T10:01:00Z", 2, None),
            (
                Some("2026-10-01T10:01:30Z".parse().unwrap()),
                EventKind::Compaction,
            ),
            usage_at("2026-10-01T10:02:00Z", 3, None),
            usage_at("2026-10-01T10:05:00Z", 4, None),
        ]);
        let export = session_with(vec![
            usage_at("2026-10-01T10:01:00Z", 20, Some("demo")),
            usage_at("2026-10-01T10:02:00Z", 30, Some("demo")),
        ]);
        let merged = merge_export(local, export);
        assert_eq!(
            inputs(&merged),
            [
                (1, None),
                (20, Some("demo".into())),
                (30, Some("demo".into())),
                (4, None)
            ]
        );
        // Non-usage events come from the local copy and stay in time order.
        let kinds: Vec<_> = merged
            .events
            .iter()
            .map(|e| matches!(e.kind, EventKind::Compaction))
            .collect();
        assert_eq!(kinds, [false, false, true, false, false]);
    }

    #[test]
    fn an_export_without_usage_leaves_the_local_copy_alone_and_fills_metadata() {
        let mut local = session_with(vec![usage_at("2026-10-01T10:00:00Z", 1, None)]);
        local.session.project_dir = Some("/work/demo".into());
        let mut export = session_with(vec![(
            Some("2026-10-01T10:00:00Z".parse().unwrap()),
            EventKind::UserTurn,
        )]);
        export.session.model = Some("m".into());
        let merged = merge_export(local.clone(), export);
        assert_eq!(merged.events, local.events);
        let mut export = session_with(vec![usage_at("2026-10-01T10:00:00Z", 9, None)]);
        export.session.model = Some("m".into());
        let merged = merge_export(local, export);
        assert_eq!(merged.session.model.as_deref(), Some("m"));
        assert_eq!(
            merged.session.project_dir.as_deref(),
            Some(std::path::Path::new("/work/demo"))
        );
    }

    #[test]
    fn events_without_a_time_keep_their_place() {
        let local = session_with(vec![
            usage_at("2026-10-01T10:00:00Z", 1, None),
            (None, EventKind::UserTurn),
            usage_at("2026-10-01T10:05:00Z", 2, None),
        ]);
        let export = session_with(vec![usage_at("2026-10-01T10:03:00Z", 7, None)]);
        let merged = merge_export(local, export);
        let order: Vec<_> = merged
            .events
            .iter()
            .map(|e| match &e.kind {
                EventKind::Usage(u) => u.input as i64,
                _ => -1,
            })
            .collect();
        assert_eq!(order, [1, -1, 7, 2]);
    }
}
