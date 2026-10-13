//! Installing the command line tools (`agentlens` and `agentlens-mcp`) from the desktop application,
//! like *Install 'code' command in PATH* of VS Code.
//!
//! macOS has no installer script and an AppImage installs nothing, so the application offers to put
//! the two binaries it ships (next to its own executable, `bin_dir`) into `~/.local/bin`. The shell
//! passes `bin_dir` in; everything else comes from an [`Env`], so tests work on a temporary home.
//!
//! The rules (read-only rule of `AGENTS.md`):
//! - The only things written are `~/.local/bin/agentlens` and `~/.local/bin/agentlens-mcp` (and the
//!   directory itself when it is missing), and only by [`install`] and [`remove`], which the shell
//!   calls on the user's click. [`status`] writes nothing.
//! - A file at one of those paths that is not ours is never overwritten or removed; the result names
//!   it. Ours is a symlink to the binary in `bin_dir`, an old link of ours (a dangling symlink to a
//!   binary of the same name, or a link into an `.app/Contents/MacOS` bundle), or a regular file with
//!   exactly the bytes of the binary in `bin_dir`. An old link is repaired by [`install`].
//! - No shell profile is edited: the result says whether `~/.local/bin` is on `PATH` and, if not, the
//!   line to add for zsh and bash.
//!
//! # Symlinks or copies
//!
//! On macOS the binaries live in `AgentLens.app/Contents/MacOS`, a path an update of the application
//! keeps, so the tools are **symlinks** and follow every update.
//!
//! An AppImage has no such path. `$APPIMAGE` (the AppImage file) is stable, but it starts the
//! application, not the CLI or the MCP server, so it cannot be the target of either link. The binaries
//! are only reachable inside the image's mount point (`/tmp/.mount_<random>/usr/bin`), which the
//! runtime creates anew on every start and removes when the application quits; a link there is dangling
//! as soon as the window closes. So when `APPIMAGE` is set the tools are **copies** of the binaries.
//! A copy does not follow an update of the AppImage: after an update it no longer matches the new
//! binaries and is then reported as a foreign file (there is no record to tell an old copy from the
//! user's own file, and writing one would be a third file), so the user removes it before installing
//! again.
//!
//! On Windows nothing is installed: the result says the action is unsupported.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::env::Env;

/// The binaries that are installed, by file name.
pub const TOOLS: [&str; 2] = ["agentlens", "agentlens-mcp"];

/// How the tools are put into `~/.local/bin`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Method {
    /// Symlinks to the binaries next to the application (macOS, Linux packages).
    Symlink,
    /// Copies of the binaries (AppImage, whose binaries have no stable path).
    Copy,
    /// Not supported on this platform (Windows).
    Unsupported,
}

/// What is at the path of a tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    /// Nothing.
    Missing,
    /// Our link (or copy) of the binary in `bin_dir`.
    Installed,
    /// An old link of ours that no longer points at the binary in `bin_dir`; installing repairs it.
    Stale,
    /// A file or link that is not ours; it is left alone.
    Foreign,
}

/// What [`install`] or [`remove`] did with a tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Created,
    /// An old link of ours was replaced.
    Repaired,
    /// It was installed already.
    Kept,
    Removed,
    /// Nothing to remove.
    Absent,
    /// A foreign file is in the way; it was not touched.
    SkippedForeign,
    /// The action failed; see `error`.
    Failed,
}

/// One tool: where it goes, what it points at and what is there now.
#[derive(Debug, Clone, Serialize)]
pub struct Tool {
    pub name: String,
    /// `~/.local/bin/<name>`.
    pub path: PathBuf,
    /// The binary in `bin_dir` it links to (or is a copy of).
    pub target: PathBuf,
    /// What is at `path` after the call.
    pub state: State,
    /// What the call did; `None` for [`status`].
    pub action: Option<Action>,
    pub error: Option<String>,
}

/// The line to add to a shell profile so that `~/.local/bin` is on `PATH`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProfileLine {
    pub shell: String,
    /// The profile file, written with `~` (for example `~/.zshrc`).
    pub file: String,
    pub line: String,
}

/// The state of the command line tools, the answer of [`status`], [`install`] and [`remove`].
#[derive(Debug, Clone, Serialize)]
pub struct CliTools {
    /// Whether this platform can install the tools at all (not on Windows).
    pub supported: bool,
    /// Whether the application should offer the action: on macOS and in an AppImage, not for the
    /// Linux packages (which install the tools themselves) and not on Windows.
    pub offered: bool,
    pub method: Method,
    /// `~/.local/bin`.
    pub dir: PathBuf,
    /// Whether the call created `dir`.
    pub created_dir: bool,
    /// Whether `dir` is on `PATH`.
    pub on_path: bool,
    /// The lines for zsh and bash when `dir` is not on `PATH`; empty otherwise.
    pub profile_lines: Vec<ProfileLine>,
    pub tools: Vec<Tool>,
    /// The paths that hold a file that is not ours (named so the user can look at it).
    pub foreign: Vec<PathBuf>,
    /// A sentence for the user (why it is unsupported, why copies are used).
    pub note: Option<String>,
}

impl CliTools {
    /// The errors of the tools, for the `errors` of an answer.
    pub fn errors(&self) -> Vec<String> {
        self.tools
            .iter()
            .filter_map(|t| {
                t.error
                    .as_ref()
                    .map(|e| format!("{}: {e}", t.path.display()))
            })
            .collect()
    }
}

/// `~/.local/bin`.
pub fn bin_home(env: &Env) -> PathBuf {
    env.home().join(".local/bin")
}

/// How the tools are installed in this environment.
pub fn method(env: &Env) -> Method {
    if !cfg!(unix) {
        Method::Unsupported
    } else if env.var("APPIMAGE").is_some() {
        Method::Copy
    } else {
        Method::Symlink
    }
}

/// What is installed, without writing anything.
pub fn status(env: &Env, bin_dir: &Path) -> CliTools {
    let mut out = base(env);
    out.tools = TOOLS
        .iter()
        .map(|name| {
            let (path, target) = paths(env, bin_dir, name);
            let state = classify(&path, &target, name);
            Tool {
                name: name.to_string(),
                path,
                target,
                state,
                action: None,
                error: None,
            }
        })
        .collect();
    finish(out)
}

/// Links (or copies) the tools into `~/.local/bin`; creates the directory, repairs old links of ours
/// and leaves foreign files alone.
pub fn install(env: &Env, bin_dir: &Path) -> CliTools {
    let mut out = base(env);
    if out.method == Method::Unsupported {
        return status(env, bin_dir);
    }
    if !out.dir.is_dir() {
        if let Err(e) = fs::create_dir_all(&out.dir) {
            let msg = format!("cannot create {}: {e}", out.dir.display());
            let mut st = status(env, bin_dir);
            for t in &mut st.tools {
                t.action = Some(Action::Failed);
                t.error = Some(msg.clone());
            }
            return st;
        }
        out.created_dir = true;
    }
    let method = out.method;
    out.tools = TOOLS
        .iter()
        .map(|name| install_one(env, bin_dir, name, method))
        .collect();
    finish(out)
}

/// Removes our links (and copies) from `~/.local/bin`; anything else stays.
pub fn remove(env: &Env, bin_dir: &Path) -> CliTools {
    let mut out = base(env);
    out.tools = TOOLS
        .iter()
        .map(|name| {
            let (path, target) = paths(env, bin_dir, name);
            let state = classify(&path, &target, name);
            let (state, action, error) = match state {
                State::Missing => (State::Missing, Action::Absent, None),
                State::Foreign => (State::Foreign, Action::SkippedForeign, None),
                State::Installed | State::Stale => match fs::remove_file(&path) {
                    Ok(()) => (State::Missing, Action::Removed, None),
                    Err(e) => (state, Action::Failed, Some(format!("cannot remove: {e}"))),
                },
            };
            Tool {
                name: name.to_string(),
                path,
                target,
                state,
                action: Some(action),
                error,
            }
        })
        .collect();
    finish(out)
}

fn base(env: &Env) -> CliTools {
    let method = method(env);
    let dir = bin_home(env);
    let on_path = on_path(env, &dir);
    let note = match method {
        Method::Unsupported => {
            Some("Installing the command line tools is not supported on Windows.".to_string())
        }
        Method::Copy => Some(
            "The AppImage has no stable path for its binaries, so the tools are copies: \
             install them again after an update."
                .to_string(),
        ),
        Method::Symlink => None,
    };
    CliTools {
        supported: method != Method::Unsupported,
        offered: method == Method::Copy || (method == Method::Symlink && cfg!(target_os = "macos")),
        method,
        dir,
        created_dir: false,
        on_path,
        profile_lines: if on_path { Vec::new() } else { profile_lines() },
        tools: Vec::new(),
        foreign: Vec::new(),
        note,
    }
}

fn finish(mut out: CliTools) -> CliTools {
    out.foreign = out
        .tools
        .iter()
        .filter(|t| t.state == State::Foreign)
        .map(|t| t.path.clone())
        .collect();
    out
}

fn paths(env: &Env, bin_dir: &Path, name: &str) -> (PathBuf, PathBuf) {
    (bin_home(env).join(name), bin_dir.join(name))
}

/// Whether `dir` is one of the entries of `$PATH`.
fn on_path(env: &Env, dir: &Path) -> bool {
    let Some(path) = env.var("PATH") else {
        return false;
    };
    let home = env.home();
    std::env::split_paths(path).any(|entry| {
        let entry = match entry.strip_prefix("~") {
            Ok(rest) => home.join(rest),
            Err(_) => entry,
        };
        entry == dir
    })
}

fn profile_lines() -> Vec<ProfileLine> {
    let line = r#"export PATH="$HOME/.local/bin:$PATH""#.to_string();
    // A login shell on macOS reads ~/.bash_profile, not ~/.bashrc.
    let bash = if cfg!(target_os = "macos") {
        "~/.bash_profile"
    } else {
        "~/.bashrc"
    };
    vec![
        ProfileLine {
            shell: "zsh".to_string(),
            file: "~/.zshrc".to_string(),
            line: line.clone(),
        },
        ProfileLine {
            shell: "bash".to_string(),
            file: bash.to_string(),
            line,
        },
    ]
}

/// What is at `path`, measured against the binary `target`.
fn classify(path: &Path, target: &Path, name: &str) -> State {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return State::Missing;
    };
    if meta.file_type().is_symlink() {
        let Ok(link) = fs::read_link(path) else {
            return State::Foreign;
        };
        let resolved = match path.parent() {
            Some(parent) if link.is_relative() => parent.join(&link),
            _ => link.clone(),
        };
        if resolved == target || same_file(&resolved, target) {
            return State::Installed;
        }
        let same_name = link.file_name().is_some_and(|n| n == name);
        let dangling = fs::metadata(&resolved).is_err();
        let in_bundle = link.parent().is_some_and(|p| {
            p.ends_with("Contents/MacOS") && p.to_string_lossy().contains(".app/")
        });
        if same_name && (dangling || in_bundle) {
            State::Stale
        } else {
            State::Foreign
        }
    } else if meta.is_file() && same_content(path, target) {
        State::Installed
    } else {
        State::Foreign
    }
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

fn same_content(a: &Path, b: &Path) -> bool {
    let (Ok(ma), Ok(mb)) = (fs::metadata(a), fs::metadata(b)) else {
        return false;
    };
    if ma.len() != mb.len() {
        return false;
    }
    match (fs::read(a), fs::read(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

fn install_one(env: &Env, bin_dir: &Path, name: &str, method: Method) -> Tool {
    let (path, target) = paths(env, bin_dir, name);
    let before = classify(&path, &target, name);
    let mut tool = Tool {
        name: name.to_string(),
        path,
        target,
        state: before,
        action: None,
        error: None,
    };
    // A symlink of ours where copies are wanted (or the reverse) is replaced too.
    let wrong_kind = before == State::Installed
        && fs::symlink_metadata(&tool.path).is_ok_and(|m| m.file_type().is_symlink())
            != (method == Method::Symlink);
    let action = match before {
        State::Foreign => Action::SkippedForeign,
        State::Installed if !wrong_kind => Action::Kept,
        _ if !tool.target.is_file() => {
            tool.error = Some(format!("{} not found", tool.target.display()));
            Action::Failed
        }
        State::Missing => match create(&tool.path, &tool.target, method) {
            Ok(()) => Action::Created,
            Err(e) => {
                tool.error = Some(e.to_string());
                Action::Failed
            }
        },
        State::Installed | State::Stale => {
            match fs::remove_file(&tool.path)
                .and_then(|()| create(&tool.path, &tool.target, method))
            {
                Ok(()) => Action::Repaired,
                Err(e) => {
                    tool.error = Some(e.to_string());
                    Action::Failed
                }
            }
        }
    };
    tool.action = Some(action);
    tool.state = classify(&tool.path, &tool.target, name);
    tool
}

#[cfg(unix)]
fn create(path: &Path, target: &Path, method: Method) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    match method {
        Method::Symlink => std::os::unix::fs::symlink(target, path),
        Method::Copy => {
            fs::copy(target, path)?;
            fs::set_permissions(path, fs::Permissions::from_mode(0o755))
        }
        Method::Unsupported => Err(unsupported()),
    }
}

#[cfg(not(unix))]
fn create(_path: &Path, _target: &Path, _method: Method) -> io::Result<()> {
    Err(unsupported())
}

fn unsupported() -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        "installing the command line tools is not supported on this platform",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(unix))]
    #[test]
    fn is_unsupported_without_symlinks() {
        let home = tempfile::tempdir().unwrap();
        let env = Env::for_home(home.path());
        let out = install(&env, &home.path().join("app"));
        assert!(!out.supported);
        assert!(!out.offered);
        assert_eq!(out.method, Method::Unsupported);
        assert!(out.note.is_some());
        assert!(!bin_home(&env).exists());
    }

    #[test]
    fn path_check_finds_local_bin() {
        let env = Env::for_home("/home/user").with_var("PATH", "/usr/bin:/home/user/.local/bin/");
        assert!(on_path(&env, &bin_home(&env)));
        let env = Env::for_home("/home/user").with_var("PATH", "/usr/bin:~/.local/bin");
        assert!(on_path(&env, &bin_home(&env)));
    }

    #[test]
    fn path_check_misses_local_bin() {
        let env = Env::for_home("/home/user").with_var("PATH", "/usr/bin:/home/other/.local/bin");
        assert!(!on_path(&env, &bin_home(&env)));
        assert!(!on_path(
            &Env::for_home("/home/user"),
            Path::new("/home/user/.local/bin")
        ));
    }

    #[cfg(unix)]
    mod unix {
        use super::super::*;
        use std::os::unix::fs::{PermissionsExt, symlink};
        use tempfile::TempDir;

        /// A home with an application directory holding both (fake) binaries.
        fn setup() -> (TempDir, Env, PathBuf) {
            let home = tempfile::tempdir().unwrap();
            let app = home
                .path()
                .join("Applications/AgentLens.app/Contents/MacOS");
            fs::create_dir_all(&app).unwrap();
            for name in TOOLS {
                fs::write(app.join(name), format!("binary {name}")).unwrap();
            }
            let env = Env::for_home(home.path());
            (home, env, app)
        }

        fn tool<'a>(out: &'a CliTools, name: &str) -> &'a Tool {
            out.tools.iter().find(|t| t.name == name).unwrap()
        }

        #[test]
        fn creates_the_links_and_keeps_them_the_second_time() {
            let (_home, env, app) = setup();
            fs::create_dir_all(bin_home(&env)).unwrap();
            let out = install(&env, &app);
            assert!(out.supported);
            assert_eq!(out.method, Method::Symlink);
            assert!(!out.created_dir);
            assert!(out.foreign.is_empty());
            assert!(out.errors().is_empty());
            for name in TOOLS {
                let t = tool(&out, name);
                assert_eq!(t.action, Some(Action::Created));
                assert_eq!(t.state, State::Installed);
                assert_eq!(
                    fs::read_link(bin_home(&env).join(name)).unwrap(),
                    app.join(name)
                );
            }
            let again = install(&env, &app);
            assert!(again.tools.iter().all(|t| t.action == Some(Action::Kept)));
            let st = status(&env, &app);
            assert!(
                st.tools
                    .iter()
                    .all(|t| t.state == State::Installed && t.action.is_none())
            );
        }

        #[test]
        fn creates_a_missing_local_bin() {
            let (_home, env, app) = setup();
            assert!(!bin_home(&env).exists());
            let st = status(&env, &app);
            assert!(st.tools.iter().all(|t| t.state == State::Missing));
            assert!(!bin_home(&env).exists(), "status writes nothing");
            let out = install(&env, &app);
            assert!(out.created_dir);
            assert!(bin_home(&env).is_dir());
            assert!(out.tools.iter().all(|t| t.state == State::Installed));
        }

        #[test]
        fn never_overwrites_a_foreign_file_and_names_it() {
            let (_home, env, app) = setup();
            let dir = bin_home(&env);
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join("agentlens"), "my own script").unwrap();
            let other = env.home().join("other/agentlens-mcp");
            fs::create_dir_all(other.parent().unwrap()).unwrap();
            fs::write(&other, "another build").unwrap();
            symlink(&other, dir.join("agentlens-mcp")).unwrap();

            let out = install(&env, &app);
            assert_eq!(
                out.foreign,
                vec![dir.join("agentlens"), dir.join("agentlens-mcp")]
            );
            for name in TOOLS {
                assert_eq!(tool(&out, name).action, Some(Action::SkippedForeign));
                assert_eq!(tool(&out, name).state, State::Foreign);
            }
            assert_eq!(
                fs::read_to_string(dir.join("agentlens")).unwrap(),
                "my own script"
            );
            assert_eq!(fs::read_link(dir.join("agentlens-mcp")).unwrap(), other);

            let removed = remove(&env, &app);
            assert!(
                removed
                    .tools
                    .iter()
                    .all(|t| t.action == Some(Action::SkippedForeign))
            );
            assert!(dir.join("agentlens").exists());
            assert!(fs::symlink_metadata(dir.join("agentlens-mcp")).is_ok());
        }

        #[test]
        fn remove_takes_only_our_links() {
            let (_home, env, app) = setup();
            let dir = bin_home(&env);
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join("agentlens"), "my own script").unwrap();
            let out = install(&env, &app);
            assert_eq!(tool(&out, "agentlens-mcp").action, Some(Action::Created));

            let removed = remove(&env, &app);
            assert_eq!(
                tool(&removed, "agentlens-mcp").action,
                Some(Action::Removed)
            );
            assert_eq!(tool(&removed, "agentlens-mcp").state, State::Missing);
            assert_eq!(
                tool(&removed, "agentlens").action,
                Some(Action::SkippedForeign)
            );
            assert!(fs::symlink_metadata(dir.join("agentlens-mcp")).is_err());
            assert_eq!(
                fs::read_to_string(dir.join("agentlens")).unwrap(),
                "my own script"
            );
            assert!(
                app.join("agentlens-mcp").exists(),
                "the binary itself stays"
            );

            let again = remove(&env, &app);
            assert_eq!(tool(&again, "agentlens-mcp").action, Some(Action::Absent));
        }

        #[test]
        fn repairs_a_broken_old_link() {
            let (_home, env, app) = setup();
            let dir = bin_home(&env);
            fs::create_dir_all(&dir).unwrap();
            let gone = env
                .home()
                .join("Downloads/AgentLens.app/Contents/MacOS/agentlens");
            symlink(&gone, dir.join("agentlens")).unwrap();
            assert_eq!(tool(&status(&env, &app), "agentlens").state, State::Stale);

            let out = install(&env, &app);
            let t = tool(&out, "agentlens");
            assert_eq!(t.action, Some(Action::Repaired));
            assert_eq!(t.state, State::Installed);
            assert_eq!(
                fs::read_link(dir.join("agentlens")).unwrap(),
                app.join("agentlens")
            );
            assert!(out.foreign.is_empty());
        }

        #[test]
        fn a_stale_link_is_removed_too() {
            let (_home, env, app) = setup();
            let dir = bin_home(&env);
            fs::create_dir_all(&dir).unwrap();
            symlink(
                "/tmp/.mount_gone/usr/bin/agentlens-mcp",
                dir.join("agentlens-mcp"),
            )
            .unwrap();
            let out = remove(&env, &app);
            assert_eq!(tool(&out, "agentlens-mcp").action, Some(Action::Removed));
            assert!(fs::symlink_metadata(dir.join("agentlens-mcp")).is_err());
        }

        #[test]
        fn a_missing_binary_is_an_error_not_a_link() {
            let (_home, env, app) = setup();
            fs::remove_file(app.join("agentlens-mcp")).unwrap();
            let out = install(&env, &app);
            let t = tool(&out, "agentlens-mcp");
            assert_eq!(t.action, Some(Action::Failed));
            assert_eq!(t.state, State::Missing);
            assert!(t.error.as_ref().unwrap().contains("not found"));
            assert_eq!(out.errors().len(), 1);
            assert_eq!(tool(&out, "agentlens").action, Some(Action::Created));
        }

        #[test]
        fn the_api_answers_with_the_errors_of_the_tools() {
            use crate::adapters::default_registry;
            use crate::api::{self, Context};
            let (_home, env, app) = setup();
            fs::remove_file(app.join("agentlens")).unwrap();
            let ctx = Context {
                env,
                registry: default_registry(),
            };
            let st = api::cli_tools_status(&ctx, &app).unwrap();
            assert!(st.errors.is_empty());
            let done = api::install_cli_tools(&ctx, &app).unwrap();
            assert_eq!(done.schema_version, crate::report::SCHEMA_VERSION);
            assert_eq!(done.errors.len(), 1);
            assert!(done.errors[0].contains("not found"));
            let gone = api::remove_cli_tools(&ctx, &app).unwrap();
            assert_eq!(
                tool(&gone.data, "agentlens-mcp").action,
                Some(Action::Removed)
            );
        }

        #[test]
        fn reports_path_and_profile_lines() {
            let (_home, env, app) = setup();
            let off = status(&env, &app);
            assert!(!off.on_path);
            let shells: Vec<_> = off.profile_lines.iter().map(|l| l.shell.as_str()).collect();
            assert_eq!(shells, ["zsh", "bash"]);
            assert!(
                off.profile_lines
                    .iter()
                    .all(|l| l.line == r#"export PATH="$HOME/.local/bin:$PATH""#)
            );
            assert_eq!(off.profile_lines[0].file, "~/.zshrc");

            let path = format!("/usr/bin:{}", bin_home(&env).display());
            let on = status(&env.clone().with_var("PATH", path), &app);
            assert!(on.on_path);
            assert!(on.profile_lines.is_empty());
        }

        #[test]
        fn copies_the_binaries_in_an_appimage() {
            let home = tempfile::tempdir().unwrap();
            let mount = home.path().join("tmp/.mount_AgentLx1/usr/bin");
            fs::create_dir_all(&mount).unwrap();
            for name in TOOLS {
                fs::write(mount.join(name), format!("binary {name}")).unwrap();
            }
            let env = Env::for_home(home.path()).with_var(
                "APPIMAGE",
                home.path()
                    .join("Apps/AgentLens.AppImage")
                    .display()
                    .to_string(),
            );
            let out = install(&env, &mount);
            assert_eq!(out.method, Method::Copy);
            assert!(out.offered);
            assert!(out.note.is_some());
            let dir = bin_home(&env);
            for name in TOOLS {
                let path = dir.join(name);
                let meta = fs::symlink_metadata(&path).unwrap();
                assert!(meta.file_type().is_file(), "a copy, not a link");
                assert_eq!(meta.permissions().mode() & 0o777, 0o755);
                assert_eq!(
                    fs::read(&path).unwrap(),
                    fs::read(mount.join(name)).unwrap()
                );
                assert_eq!(tool(&out, name).state, State::Installed);
            }
            assert!(
                install(&env, &mount)
                    .tools
                    .iter()
                    .all(|t| t.action == Some(Action::Kept))
            );

            // An update brings other binaries: the old copy can no longer be told from a foreign file.
            fs::write(mount.join("agentlens"), "binary agentlens, new version").unwrap();
            let updated = install(&env, &mount);
            assert_eq!(
                tool(&updated, "agentlens").action,
                Some(Action::SkippedForeign)
            );
            assert_eq!(updated.foreign, vec![dir.join("agentlens")]);

            let removed = remove(&env, &mount);
            assert_eq!(
                tool(&removed, "agentlens-mcp").action,
                Some(Action::Removed)
            );
            assert_eq!(
                tool(&removed, "agentlens").action,
                Some(Action::SkippedForeign)
            );
            assert!(!dir.join("agentlens-mcp").exists());
            assert!(dir.join("agentlens").exists());
        }

        #[test]
        fn an_appimage_replaces_an_old_dangling_link_with_a_copy() {
            let home = tempfile::tempdir().unwrap();
            let mount = home.path().join("tmp/.mount_new/usr/bin");
            fs::create_dir_all(&mount).unwrap();
            for name in TOOLS {
                fs::write(mount.join(name), format!("binary {name}")).unwrap();
            }
            let dir = home.path().join(".local/bin");
            fs::create_dir_all(&dir).unwrap();
            symlink(
                home.path().join("tmp/.mount_old/usr/bin/agentlens"),
                dir.join("agentlens"),
            )
            .unwrap();
            let env = Env::for_home(home.path()).with_var("APPIMAGE", "/apps/AgentLens.AppImage");
            let out = install(&env, &mount);
            assert_eq!(tool(&out, "agentlens").action, Some(Action::Repaired));
            assert!(
                fs::symlink_metadata(dir.join("agentlens"))
                    .unwrap()
                    .file_type()
                    .is_file()
            );
        }
    }
}
