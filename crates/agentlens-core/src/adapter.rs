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
    /// Only sessions that were last active before this instant (exclusive), so
    /// the period is `[since, until)`.
    pub until: Option<Timestamp>,
    /// Only sessions whose project directory is this directory or inside it.
    pub project: Option<PathBuf>,
}

impl SessionFilter {
    /// `true` if the session passes the filter. Sessions without the needed
    /// metadata pass, so that missing data does not hide them.
    ///
    /// A session counts at its end (its start if it has no end): it passes when
    /// that instant is at or after `since` and before `until`.
    pub fn matches(&self, session: &Session) -> bool {
        if let Some(last) = session.ended_at.or(session.started_at) {
            if self.since.is_some_and(|since| last < since) {
                return false;
            }
            if self.until.is_some_and(|until| last >= until) {
                return false;
            }
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

/// Clients stamp a request a few milliseconds apart from the span or log record
/// of the same request (Copilot CLI: 2 to 4 ms). Local usage this close outside
/// the exported range still belongs to it.
const MERGE_TOLERANCE_MS: i64 = 250;

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
    let slack = jiff::SignedDuration::from_millis(MERGE_TOLERANCE_MS);
    let (from, to) = (
        from.checked_sub(slack).unwrap_or(from),
        to.checked_add(slack).unwrap_or(to),
    );
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

/// Sessions read for one selection, kept for a while by a caching registry.
type CacheKey = (
    Option<Timestamp>,
    Option<Timestamp>,
    Option<PathBuf>,
    Vec<Source>,
);
type Cached = (
    std::time::Instant,
    std::sync::Arc<Vec<Result<SessionEvents, String>>>,
);

/// All adapters known to the program.
#[derive(Default)]
pub struct Registry {
    adapters: Vec<Box<dyn Adapter>>,
    cache: Option<RegistryCache>,
}

struct RegistryCache {
    ttl: std::time::Duration,
    entries: std::sync::Mutex<HashMap<CacheKey, Cached>>,
}

impl Registry {
    pub fn new() -> Self {
        Registry::default()
    }

    /// Keeps the sessions of a selection for `ttl`, so that several analyses of
    /// the same selection read the client files once (the desktop application
    /// asks for one analysis per view). [`Registry::clear_cache`] forgets them.
    pub fn with_cache(mut self, ttl: std::time::Duration) -> Registry {
        self.cache = Some(RegistryCache {
            ttl,
            entries: std::sync::Mutex::new(HashMap::new()),
        });
        self
    }

    /// Forgets cached sessions, so the next analysis reads the files again.
    pub fn clear_cache(&self) {
        if let Some(c) = &self.cache {
            c.entries.lock().unwrap_or_else(|e| e.into_inner()).clear();
        }
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
        let Some(cache) = &self.cache else {
            return self.read_sessions(env, filter, only);
        };
        let mut sources = only.to_vec();
        sources.sort();
        let key: CacheKey = (filter.since, filter.until, filter.project.clone(), sources);
        let fresh = {
            let entries = cache.entries.lock().unwrap_or_else(|e| e.into_inner());
            entries
                .get(&key)
                .filter(|(at, _)| at.elapsed() < cache.ttl)
                .map(|(_, v)| std::sync::Arc::clone(v))
        };
        let all = fresh.unwrap_or_else(|| {
            let read: Vec<Result<SessionEvents, String>> = self
                .read_sessions(env, filter, only)
                .map(|r| r.map_err(|e| e.to_string()))
                .collect();
            let read = std::sync::Arc::new(read);
            cache
                .entries
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(
                    key,
                    (std::time::Instant::now(), std::sync::Arc::clone(&read)),
                );
            read
        });
        Box::new((0..all.len()).map(move |i| all[i].clone().map_err(AdapterError::Other)))
    }

    fn read_sessions<'a>(
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
            ..SessionFilter::default()
        };
        assert_eq!(ids(&registry(), &f, &[]), ["new"]);
    }

    #[test]
    fn until_filter_drops_new_sessions() {
        let f = SessionFilter {
            until: Some("2026-06-01T00:00:00Z".parse().unwrap()),
            ..SessionFilter::default()
        };
        assert_eq!(ids(&registry(), &f, &[]), ["old"]);
    }

    fn at(ts: &str) -> Option<Timestamp> {
        Some(ts.parse().unwrap())
    }

    fn timed(started: Option<Timestamp>, ended: Option<Timestamp>) -> Session {
        let mut s = Session::new("s", Source::ClaudeCode);
        s.started_at = started;
        s.ended_at = ended;
        s
    }

    fn window() -> SessionFilter {
        SessionFilter {
            since: at("2026-10-01T00:00:00Z"),
            until: at("2026-10-08T00:00:00Z"),
            ..SessionFilter::default()
        }
    }

    #[test]
    fn since_is_inclusive_and_until_is_exclusive() {
        let f = window();
        assert!(f.matches(&timed(None, at("2026-10-01T00:00:00Z"))));
        assert!(f.matches(&timed(None, at("2026-10-07T23:59:59Z"))));
        assert!(!f.matches(&timed(None, at("2026-10-08T00:00:00Z"))));
        assert!(!f.matches(&timed(None, at("2026-09-30T23:59:59Z"))));
    }

    #[test]
    fn a_session_counts_at_its_end_or_at_its_start_without_one() {
        let f = window();
        // Started before the window, ended inside it: kept.
        assert!(f.matches(&timed(
            at("2026-09-20T00:00:00Z"),
            at("2026-10-02T00:00:00Z")
        )));
        // Started inside the window, ended after it: dropped.
        assert!(!f.matches(&timed(
            at("2026-10-07T00:00:00Z"),
            at("2026-10-09T00:00:00Z")
        )));
        // No end: the start decides.
        assert!(f.matches(&timed(at("2026-10-03T00:00:00Z"), None)));
        assert!(!f.matches(&timed(at("2026-10-08T00:00:00Z"), None)));
    }

    #[test]
    fn a_session_without_times_passes_both_bounds() {
        assert!(window().matches(&timed(None, None)));
    }

    #[test]
    fn project_filter_keeps_sessions_without_project_metadata() {
        let f = SessionFilter {
            project: Some("/work/a".into()),
            ..SessionFilter::default()
        };
        assert_eq!(ids(&registry(), &f, &[]), ["old", "new"]);
        let f = SessionFilter {
            project: Some("/work/b".into()),
            ..SessionFilter::default()
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

    struct Counting(std::sync::Arc<std::sync::atomic::AtomicU32>);

    impl Adapter for Counting {
        fn id(&self) -> Source {
            Source::ClaudeCode
        }

        fn detect(&self, _env: &Env) -> Detection {
            Detection::found(vec!["/x".into()])
        }

        fn sessions<'a>(&'a self, _env: &'a Env, _filter: &'a SessionFilter) -> SessionIter<'a> {
            self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Box::new(std::iter::once(Ok(SessionEvents::new(Session::new(
                "a",
                Source::ClaudeCode,
            )))))
        }
    }

    #[test]
    fn a_caching_registry_reads_a_selection_once_until_it_is_cleared() {
        use std::sync::atomic::Ordering::SeqCst;
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let mut r = Registry::new().with_cache(std::time::Duration::from_secs(60));
        r.register(Box::new(Counting(calls.clone())));
        let env = Env::for_home("/h");
        let all = SessionFilter::default();
        let ids = |f: &SessionFilter| -> Vec<String> {
            r.sessions(&env, f, &[])
                .map(|s| s.unwrap().session.id)
                .collect()
        };
        assert_eq!(ids(&all), ["a"]);
        assert_eq!(ids(&all), ["a"]);
        assert_eq!(calls.load(SeqCst), 1);
        // Another selection is read on its own.
        let recent = SessionFilter {
            since: Some("2026-01-01T00:00:00Z".parse().unwrap()),
            ..SessionFilter::default()
        };
        ids(&recent);
        assert_eq!(calls.load(SeqCst), 2);
        r.clear_cache();
        ids(&all);
        assert_eq!(calls.load(SeqCst), 3);
        // Without a cache every call reads.
        let mut plain = Registry::new();
        plain.register(Box::new(Counting(calls.clone())));
        plain.sessions(&env, &all, &[]).count();
        plain.sessions(&env, &all, &[]).count();
        assert_eq!(calls.load(SeqCst), 5);
    }

    #[test]
    fn selections_that_differ_only_by_until_are_cached_apart() {
        use std::sync::atomic::Ordering::SeqCst;
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let mut r = Registry::new().with_cache(std::time::Duration::from_secs(60));
        r.register(Box::new(Counting(calls.clone())));
        let env = Env::for_home("/h");
        let since = Some("2026-01-01T00:00:00Z".parse().unwrap());
        let open = SessionFilter {
            since,
            ..SessionFilter::default()
        };
        let closed = SessionFilter {
            since,
            until: Some("2026-02-01T00:00:00Z".parse().unwrap()),
            ..SessionFilter::default()
        };
        r.sessions(&env, &open, &[]).count();
        r.sessions(&env, &closed, &[]).count();
        assert_eq!(calls.load(SeqCst), 2);
        r.sessions(&env, &closed, &[]).count();
        assert_eq!(calls.load(SeqCst), 2);
    }

    #[test]
    fn an_expired_entry_is_read_again() {
        use std::sync::atomic::Ordering::SeqCst;
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let mut r = Registry::new().with_cache(std::time::Duration::ZERO);
        r.register(Box::new(Counting(calls.clone())));
        let env = Env::for_home("/h");
        r.sessions(&env, &SessionFilter::default(), &[]).count();
        r.sessions(&env, &SessionFilter::default(), &[]).count();
        assert_eq!(calls.load(SeqCst), 2);
    }
}
