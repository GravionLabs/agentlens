//! Installing the command line tools (`agentlens` and `agentlens-mcp`) from the desktop application,
//! like *Install 'code' command in PATH* of VS Code.
//!
//! macOS has no installer script and an AppImage installs nothing, so the application offers to put
//! the two binaries it ships (next to its own executable, `bin_dir`) into `~/.local/bin`. The shell
//! passes `bin_dir` in; everything else comes from an [`Env`], so tests work on a temporary home.
//! The decision is [ADR 0006](../../../docs/adr/0006-cli-tools-links.md).
//!
//! The rules (an exception to the read-only rule of `AGENTS.md`, see the ADR):
//! - The only things written are `~/.local/bin/agentlens` and `~/.local/bin/agentlens-mcp` (and the
//!   directory itself when it is missing), and only by [`install`] and [`remove`], which the shell
//!   calls on the user's click (and `api` only where the action is [offered]). [`status`] writes
//!   nothing.
//! - A file at one of those paths that is not ours is never overwritten or removed; the result names
//!   it. Ours is:
//!   - a symlink to the binary in `bin_dir` ([`State::Installed`]);
//!   - a regular file with exactly the bytes of the binary in `bin_dir` (an installed copy);
//!   - an old link of ours ([`State::Stale`]): a symlink to a binary of the same name inside an
//!     AgentLens application bundle (`AgentLens.app/Contents/MacOS`), inside an AppImage mount point
//!     (`.mount_<random>/usr/bin`) or in `bin_dir` itself, whether that target still exists or not;
//!   - an old copy of ours ([`State::Stale`]): a regular file that carries [`MARKER`], the byte
//!     string every `agentlens` and `agentlens-mcp` binary contains (see [`embed_marker`]). The file is
//!     only read, never run.
//!
//!   A link with the same name to anywhere else, dangling or not, is the user's and stays.
//! - Writing is atomic: the new link or copy is made under a temporary name in `~/.local/bin` and
//!   renamed over the path, so a failed repair leaves the old (working) file in place.
//! - No shell profile is edited: the result says whether `~/.local/bin` is on the `PATH` of the
//!   application and, if not, the line to add for zsh and bash.
//!
//! # Symlinks or copies
//!
//! On macOS the binaries live in `AgentLens.app/Contents/MacOS`, a path an update of the application
//! keeps, so the tools are **symlinks** and follow every update. A bundle that runs from a disk image
//! (`/Volumes/...`) or from a path of macOS App Translocation has no such path: [`install`] refuses
//! and says to move the application to `/Applications` first.
//!
//! An AppImage has no stable path either. `$APPIMAGE` (the AppImage file) is stable, but it starts the
//! application, not the CLI or the MCP server, so it cannot be the target of either link. The binaries
//! are only reachable inside the image's mount point (`/tmp/.mount_<random>/usr/bin`), which the
//! runtime creates anew on every start and removes when the application quits; a link there is dangling
//! as soon as the window closes. So when `APPIMAGE` is set the tools are **copies** of the binaries.
//! A copy does not follow an update of the AppImage. After an update its bytes differ from the new
//! binaries, but it still carries [`MARKER`], so it is reported as stale: installing again replaces
//! it and removing deletes it.
//!
//! On Windows nothing is installed: the result says the action is unsupported.
//!
//! [offered]: CliTools::offered

use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

use serde::Serialize;

use crate::env::Env;

/// The binaries that are installed, by file name.
pub const TOOLS: [&str; 2] = ["agentlens", "agentlens-mcp"];

/// A byte string that every `agentlens` and `agentlens-mcp` binary carries, so that an old copy of
/// ours in `~/.local/bin` can be told from a file of the user without running it and without a record
/// file. Never change it: copies installed by older versions are recognized by it.
pub const MARKER: &[u8] = b"agentlens-cli-tool-marker:7d1f0c52-93e8-4b6a-a0f4-2c86e5b91d3e";

/// Keeps [`MARKER`] in the binary that calls it; the `main` of `agentlens` and `agentlens-mcp` does.
#[inline(never)]
pub fn embed_marker() {
    std::hint::black_box(MARKER);
}

/// Whether the regular file at `path` carries [`MARKER`], read in blocks (the file is not run).
pub fn has_marker(path: &Path) -> bool {
    let Ok(mut file) = File::open(path) else {
        return false;
    };
    let keep = MARKER.len() - 1;
    let mut buf = vec![0u8; 64 * 1024 + keep];
    let mut filled = 0;
    loop {
        let n = match file.read(&mut buf[filled..]) {
            Ok(0) => return false,
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => return false,
        };
        filled += n;
        if buf[..filled].windows(MARKER.len()).any(|w| w == MARKER) {
            return true;
        }
        // Keep the last bytes: the marker may straddle two blocks.
        let tail = filled.saturating_sub(keep);
        buf.copy_within(tail..filled, 0);
        filled -= tail;
    }
}

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
    /// An old link or copy of ours that no longer matches the binary in `bin_dir`; installing
    /// replaces it, removing deletes it.
    Stale,
    /// A file or link that is not ours; it is left alone.
    Foreign,
}

/// What [`install`] or [`remove`] did with a tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Created,
    /// An old link or copy of ours was replaced.
    Repaired,
    /// It was installed already.
    Kept,
    Removed,
    /// Nothing to remove.
    Absent,
    /// A foreign file is in the way; it was not touched.
    SkippedForeign,
    /// The action failed; see `error` (of the tool or of the answer).
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
    /// Whether `dir` is on the `PATH` of the application process. A shell can have another `PATH`
    /// (an application started from the macOS Finder does not read the shell profile).
    pub on_path: bool,
    /// The lines for zsh and bash when `dir` is not on `PATH`; empty otherwise.
    pub profile_lines: Vec<ProfileLine>,
    pub tools: Vec<Tool>,
    /// The paths that hold a file that is not ours (named so the user can look at it).
    pub foreign: Vec<PathBuf>,
    /// A sentence for the user (why it is unsupported, why copies are used, why it cannot install).
    pub note: Option<String>,
    /// Why the whole call did nothing (for example a temporary location of the application).
    pub error: Option<String>,
}

impl CliTools {
    /// The errors of the call and of the tools, for the `errors` of an answer.
    pub fn errors(&self) -> Vec<String> {
        self.error
            .iter()
            .cloned()
            .chain(self.tools.iter().filter_map(|t| {
                t.error
                    .as_ref()
                    .map(|e| format!("{}: {e}", t.path.display()))
            }))
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

/// Whether the application offers the action here: in an AppImage and on macOS.
pub fn offered(env: &Env) -> bool {
    match method(env) {
        Method::Copy => true,
        Method::Symlink => cfg!(target_os = "macos"),
        Method::Unsupported => false,
    }
}

/// Why the action is not offered here, for an error of a command that should not have been called.
pub fn not_offered_reason(env: &Env) -> &'static str {
    match method(env) {
        Method::Unsupported => "installing the command line tools is not supported on Windows",
        _ => {
            "installing the command line tools is only offered on macOS and in an AppImage; \
             the .deb and .rpm packages put them in /usr/bin"
        }
    }
}

/// Whether `bin_dir` is a temporary location a link must not point into: a mounted disk image or a
/// path of macOS App Translocation (a quarantined application started where it was downloaded).
pub fn unstable_bin_dir(bin_dir: &Path) -> bool {
    bin_dir.starts_with("/Volumes")
        || bin_dir
            .components()
            .any(|c| c.as_os_str() == "AppTranslocation")
}

const UNSTABLE: &str = "AgentLens runs from a temporary location (a disk image or a path of macOS App \
     Translocation), so links to its programs would break: move AgentLens.app to /Applications, \
     start it from there and install the command line tools again.";

/// What is installed, without writing anything.
pub fn status(env: &Env, bin_dir: &Path) -> CliTools {
    let mut out = base(env, bin_dir);
    out.tools = TOOLS
        .iter()
        .map(|name| {
            let (path, target) = paths(env, bin_dir, name);
            let state = classify(&path, &target, bin_dir, name);
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

/// Links (or copies) the tools into `~/.local/bin`; creates the directory, replaces old links and
/// copies of ours and leaves foreign files alone.
pub fn install(env: &Env, bin_dir: &Path) -> CliTools {
    let mut out = base(env, bin_dir);
    let method = out.method;
    if method == Method::Unsupported {
        return status(env, bin_dir);
    }
    if method == Method::Symlink && unstable_bin_dir(bin_dir) {
        let mut st = status(env, bin_dir);
        st.error = Some(UNSTABLE.to_string());
        for t in &mut st.tools {
            t.action = Some(Action::Failed);
        }
        return st;
    }
    // What to do with each tool; `action: None` means it has to be written.
    let mut tools: Vec<Tool> = TOOLS
        .iter()
        .map(|name| plan(env, bin_dir, name, method))
        .collect();
    let writes = tools.iter().any(|t| t.action.is_none());
    if writes && !out.dir.is_dir() {
        match fs::create_dir_all(&out.dir) {
            Ok(()) => out.created_dir = out.dir.is_dir(),
            Err(e) => {
                let msg = format!("cannot create {}: {e}", out.dir.display());
                for t in tools.iter_mut().filter(|t| t.action.is_none()) {
                    t.action = Some(Action::Failed);
                    t.error = Some(msg.clone());
                }
                out.tools = tools;
                return finish(out);
            }
        }
    }
    for t in tools.iter_mut().filter(|t| t.action.is_none()) {
        match create(&t.path, &t.target, method) {
            Ok(()) => {
                t.action = Some(if t.state == State::Missing {
                    Action::Created
                } else {
                    Action::Repaired
                });
                t.state = State::Installed;
            }
            Err(e) => {
                t.action = Some(Action::Failed);
                t.error = Some(e.to_string());
                t.state = classify(&t.path, &t.target, bin_dir, &t.name);
            }
        }
    }
    out.tools = tools;
    finish(out)
}

/// Removes our links (and copies, old ones too) from `~/.local/bin`; anything else stays.
pub fn remove(env: &Env, bin_dir: &Path) -> CliTools {
    let mut out = base(env, bin_dir);
    out.tools = TOOLS
        .iter()
        .map(|name| {
            let (path, target) = paths(env, bin_dir, name);
            let state = classify(&path, &target, bin_dir, name);
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

fn base(env: &Env, bin_dir: &Path) -> CliTools {
    let method = method(env);
    let dir = bin_home(env);
    let on_path = on_path(env, &dir);
    let note = match method {
        Method::Unsupported => {
            Some("Installing the command line tools is not supported on Windows.".to_string())
        }
        Method::Copy => Some(
            "The AppImage has no stable path for its programs, so the tools are copies: \
             click Install again after an update of the AppImage to replace them."
                .to_string(),
        ),
        Method::Symlink if unstable_bin_dir(bin_dir) => Some(UNSTABLE.to_string()),
        Method::Symlink => None,
    };
    CliTools {
        supported: method != Method::Unsupported,
        offered: offered(env),
        method,
        dir,
        created_dir: false,
        on_path,
        profile_lines: if on_path { Vec::new() } else { profile_lines() },
        tools: Vec::new(),
        foreign: Vec::new(),
        note,
        error: None,
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

/// What is at `path`, measured against the binary `target` in `bin_dir`.
fn classify(path: &Path, target: &Path, bin_dir: &Path, name: &str) -> State {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return State::Missing;
    };
    if meta.file_type().is_symlink() {
        let Ok(link) = fs::read_link(path) else {
            return State::Foreign;
        };
        let resolved = match path.parent() {
            Some(parent) if link.is_relative() => normalize(&parent.join(&link)),
            _ => normalize(&link),
        };
        if resolved == normalize(target) || same_file(&resolved, target) {
            State::Installed
        } else if resolved.file_name().is_some_and(|n| n == name)
            && resolved
                .parent()
                .is_some_and(|dir| is_our_dir(dir, bin_dir))
        {
            State::Stale
        } else {
            State::Foreign
        }
    } else if !meta.is_file() {
        State::Foreign
    } else if same_content(path, target) {
        State::Installed
    } else if has_marker(path) {
        State::Stale
    } else {
        State::Foreign
    }
}

/// Whether a link into `dir` is one the application made: the binaries folder of an AgentLens bundle,
/// of an AppImage mount point, or `bin_dir` itself.
fn is_our_dir(dir: &Path, bin_dir: &Path) -> bool {
    if dir == normalize(bin_dir) {
        return true;
    }
    let parts: Vec<String> = dir
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    match parts.as_slice() {
        [.., app, contents, macos] if contents == "Contents" && macos == "MacOS" => {
            let app = app.to_lowercase();
            app.ends_with(".app") && app.contains("agentlens")
        }
        [.., mount, usr, bin] if usr == "usr" && bin == "bin" => mount.starts_with(".mount_"),
        _ => false,
    }
}

/// `path` with `.` and `..` resolved by name (a dangling link cannot be canonicalized).
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// Whether the two files have the same bytes, compared block by block.
fn same_content(a: &Path, b: &Path) -> bool {
    let (Ok(ma), Ok(mb)) = (fs::metadata(a), fs::metadata(b)) else {
        return false;
    };
    if ma.len() != mb.len() || !mb.is_file() {
        return false;
    }
    let (Ok(mut fa), Ok(mut fb)) = (File::open(a), File::open(b)) else {
        return false;
    };
    let mut ba = vec![0u8; 64 * 1024];
    let mut bb = vec![0u8; 64 * 1024];
    loop {
        let n = match read_full(&mut fa, &mut ba) {
            Ok(n) => n,
            Err(_) => return false,
        };
        match read_full(&mut fb, &mut bb[..n]) {
            Ok(m) if m == n => {}
            _ => return false,
        }
        if n == 0 {
            return true;
        }
        if ba[..n] != bb[..n] {
            return false;
        }
    }
}

/// Reads until `buf` is full or the file ends.
fn read_full(file: &mut File, buf: &mut [u8]) -> io::Result<usize> {
    let mut filled = 0;
    while filled < buf.len() {
        match file.read(&mut buf[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(filled)
}

/// The tool as found, with the action decided where nothing has to be written (`None`: write it).
fn plan(env: &Env, bin_dir: &Path, name: &str, method: Method) -> Tool {
    let (path, target) = paths(env, bin_dir, name);
    let state = classify(&path, &target, bin_dir, name);
    // A symlink of ours where copies are wanted (or the reverse) is replaced too.
    let wrong_kind = state == State::Installed
        && fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink())
            != (method == Method::Symlink);
    let mut tool = Tool {
        name: name.to_string(),
        path,
        target,
        state,
        action: None,
        error: None,
    };
    match state {
        State::Foreign => tool.action = Some(Action::SkippedForeign),
        State::Installed if !wrong_kind => tool.action = Some(Action::Kept),
        _ if !tool.target.is_file() => {
            tool.action = Some(Action::Failed);
            tool.error = Some(format!("{} not found", tool.target.display()));
        }
        _ => {}
    }
    tool
}

/// Makes the link or copy under a temporary name next to `path` and renames it over `path`, so `path`
/// is never missing or half written; the temporary file is removed when anything fails.
fn create(path: &Path, target: &Path, method: Method) -> io::Result<()> {
    let (Some(dir), Some(name)) = (path.parent(), path.file_name()) else {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "no file name"));
    };
    let tmp = dir.join(format!(
        ".{}.agentlens-{}.tmp",
        name.to_string_lossy(),
        std::process::id()
    ));
    let result = write_temp(&tmp, target, method).and_then(|()| fs::rename(&tmp, path));
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

#[cfg(unix)]
fn write_temp(tmp: &Path, target: &Path, method: Method) -> io::Result<()> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    match method {
        Method::Symlink => std::os::unix::fs::symlink(target, tmp),
        Method::Copy => {
            let mut src = File::open(target)?;
            let mut dst = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o755)
                .open(tmp)?;
            io::copy(&mut src, &mut dst)?;
            dst.sync_all()?;
            // The umask may have taken bits off the mode of `open`.
            fs::set_permissions(tmp, fs::Permissions::from_mode(0o755))
        }
        Method::Unsupported => Err(unsupported()),
    }
}

#[cfg(not(unix))]
fn write_temp(_tmp: &Path, _target: &Path, _method: Method) -> io::Result<()> {
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
        assert!(!offered(&env.clone().with_var("APPIMAGE", "x")));
        assert_eq!(out.method, Method::Unsupported);
        assert!(out.note.is_some());
        assert!(!bin_home(&env).exists());
    }

    /// An environment whose `PATH` uses the separator of the platform.
    fn with_path(entries: &[&str]) -> Env {
        let path = std::env::join_paths(entries).unwrap();
        Env::for_home("/home/user").with_var("PATH", path.to_str().unwrap())
    }

    #[test]
    fn path_check_finds_local_bin() {
        let env = with_path(&["/usr/bin", "/home/user/.local/bin/"]);
        assert!(on_path(&env, &bin_home(&env)));
        let env = with_path(&["/usr/bin", "~/.local/bin"]);
        assert!(on_path(&env, &bin_home(&env)));
    }

    #[test]
    fn path_check_misses_local_bin() {
        let env = with_path(&["/usr/bin", "/home/other/.local/bin"]);
        assert!(!on_path(&env, &bin_home(&env)));
        assert!(!on_path(
            &Env::for_home("/home/user"),
            Path::new("/home/user/.local/bin")
        ));
    }

    #[test]
    fn knows_temporary_locations_of_the_application() {
        assert!(unstable_bin_dir(Path::new(
            "/Volumes/AgentLens/AgentLens.app/Contents/MacOS"
        )));
        assert!(unstable_bin_dir(Path::new(
            "/private/var/folders/x/T/AppTranslocation/1A2B/d/AgentLens.app/Contents/MacOS"
        )));
        assert!(!unstable_bin_dir(Path::new(
            "/Applications/AgentLens.app/Contents/MacOS"
        )));
        assert!(!unstable_bin_dir(Path::new("/VolumesX/AgentLens.app")));
    }

    #[test]
    fn knows_the_folders_of_our_old_links() {
        let bin = Path::new("/Applications/AgentLens.app/Contents/MacOS");
        assert!(is_our_dir(bin, bin));
        assert!(is_our_dir(
            Path::new("/Users/u/Downloads/AgentLens 2.app/Contents/MacOS"),
            bin
        ));
        assert!(is_our_dir(Path::new("/tmp/.mount_AgentLx1/usr/bin"), bin));
        assert!(!is_our_dir(
            Path::new("/Applications/Other.app/Contents/MacOS"),
            bin
        ));
        assert!(!is_our_dir(Path::new("/usr/bin"), bin));
        assert!(!is_our_dir(Path::new("/home/u/src/agentlens/target"), bin));
    }

    #[test]
    fn offered_only_on_macos_and_in_an_appimage() {
        let env = Env::for_home("/home/user");
        let appimage = env.clone().with_var("APPIMAGE", "/apps/AgentLens.AppImage");
        if cfg!(unix) {
            assert!(offered(&appimage));
            assert_eq!(method(&appimage), Method::Copy);
            assert_eq!(method(&env), Method::Symlink);
        }
        // The .deb and .rpm packages install the tools themselves; macOS has no installer for them.
        assert_eq!(offered(&env), cfg!(target_os = "macos"));
        assert!(!not_offered_reason(&env).is_empty());
    }

    #[cfg(unix)]
    mod unix {
        use super::super::*;
        use std::os::unix::fs::{PermissionsExt, symlink};
        use tempfile::TempDir;

        /// The bytes of a fake binary: a version and the marker every real one carries.
        fn binary(name: &str, version: &str) -> Vec<u8> {
            let mut b = format!("binary {name} {version} ").into_bytes();
            b.extend_from_slice(MARKER);
            b.extend_from_slice(b" end");
            b
        }

        /// A home with an application directory holding both (fake) binaries.
        fn setup() -> (TempDir, Env, PathBuf) {
            let home = tempfile::tempdir().unwrap();
            let app = home
                .path()
                .join("Applications/AgentLens.app/Contents/MacOS");
            fs::create_dir_all(&app).unwrap();
            for name in TOOLS {
                fs::write(app.join(name), binary(name, "1")).unwrap();
            }
            let env = Env::for_home(home.path());
            (home, env, app)
        }

        /// A home with an AppImage mount point holding both binaries, and `APPIMAGE` set.
        fn setup_appimage() -> (TempDir, Env, PathBuf) {
            let home = tempfile::tempdir().unwrap();
            let mount = home.path().join("tmp/.mount_AgentLx1/usr/bin");
            fs::create_dir_all(&mount).unwrap();
            for name in TOOLS {
                fs::write(mount.join(name), binary(name, "1")).unwrap();
            }
            let image = home.path().join("Apps/AgentLens.AppImage");
            let env = Env::for_home(home.path()).with_var("APPIMAGE", image.display().to_string());
            (home, env, mount)
        }

        fn tool<'a>(out: &'a CliTools, name: &str) -> &'a Tool {
            out.tools.iter().find(|t| t.name == name).unwrap()
        }

        fn is_link(path: &Path) -> bool {
            fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink())
        }

        /// Whether permissions hold here (they do not for root).
        fn permissions_hold(dir: &Path) -> bool {
            let probe = dir.join(".probe");
            let denied = fs::write(&probe, "x").is_err();
            let _ = fs::remove_file(probe);
            denied
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
            let leftovers: Vec<_> = fs::read_dir(bin_home(&env))
                .unwrap()
                .map(|e| e.unwrap().file_name())
                .filter(|n| n.to_string_lossy().ends_with(".tmp"))
                .collect();
            assert!(leftovers.is_empty(), "no temporary file is left");
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
            assert!(is_link(&dir.join("agentlens-mcp")));
        }

        #[test]
        fn a_directory_in_the_way_is_foreign() {
            let (_home, env, app) = setup();
            fs::create_dir_all(bin_home(&env).join("agentlens")).unwrap();
            let out = install(&env, &app);
            assert_eq!(tool(&out, "agentlens").state, State::Foreign);
            assert_eq!(tool(&out, "agentlens-mcp").action, Some(Action::Created));
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
        fn repairs_a_broken_old_link_into_a_bundle() {
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
        fn a_live_link_into_another_agentlens_bundle_is_stale() {
            let (_home, env, app) = setup();
            let old = env
                .home()
                .join("Downloads/AgentLens.app/Contents/MacOS/agentlens-mcp");
            fs::create_dir_all(old.parent().unwrap()).unwrap();
            fs::write(&old, binary("agentlens-mcp", "0")).unwrap();
            let dir = bin_home(&env);
            fs::create_dir_all(&dir).unwrap();
            symlink(&old, dir.join("agentlens-mcp")).unwrap();
            assert_eq!(
                tool(&status(&env, &app), "agentlens-mcp").state,
                State::Stale
            );
            let out = install(&env, &app);
            assert_eq!(tool(&out, "agentlens-mcp").action, Some(Action::Repaired));
            assert!(old.exists(), "the old bundle itself is not touched");
        }

        #[test]
        fn a_dangling_link_of_the_user_is_foreign() {
            let (_home, env, app) = setup();
            let dir = bin_home(&env);
            fs::create_dir_all(&dir).unwrap();
            // The same name, but somewhere AgentLens never links to.
            symlink(
                env.home().join("src/agentlens/target/release/agentlens"),
                dir.join("agentlens"),
            )
            .unwrap();
            // Another name in an AgentLens bundle.
            symlink(
                env.home()
                    .join("Downloads/AgentLens.app/Contents/MacOS/agentlens-app"),
                dir.join("agentlens-mcp"),
            )
            .unwrap();
            let st = status(&env, &app);
            assert!(st.tools.iter().all(|t| t.state == State::Foreign));
            let out = install(&env, &app);
            assert!(
                out.tools
                    .iter()
                    .all(|t| t.action == Some(Action::SkippedForeign))
            );
            let removed = remove(&env, &app);
            assert!(
                removed
                    .tools
                    .iter()
                    .all(|t| t.action == Some(Action::SkippedForeign))
            );
            assert!(is_link(&dir.join("agentlens")));
            assert!(is_link(&dir.join("agentlens-mcp")));
        }

        #[test]
        fn a_relative_link_is_resolved_against_its_folder() {
            let (_home, env, app) = setup();
            let dir = bin_home(&env);
            fs::create_dir_all(&dir).unwrap();
            symlink(
                "../../Applications/AgentLens.app/Contents/MacOS/agentlens",
                dir.join("agentlens"),
            )
            .unwrap();
            symlink(
                "../../Downloads/AgentLens.app/Contents/MacOS/agentlens-mcp",
                dir.join("agentlens-mcp"),
            )
            .unwrap();
            let st = status(&env, &app);
            assert_eq!(tool(&st, "agentlens").state, State::Installed);
            assert_eq!(tool(&st, "agentlens-mcp").state, State::Stale);
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
        fn a_file_where_the_folder_should_be_fails_the_tools_to_write() {
            let (_home, env, app) = setup();
            fs::create_dir_all(env.home().join(".local")).unwrap();
            fs::write(bin_home(&env), "not a folder").unwrap();
            fs::remove_file(app.join("agentlens-mcp")).unwrap();
            let out = install(&env, &app);
            assert!(!out.created_dir);
            let t = tool(&out, "agentlens");
            assert_eq!(t.action, Some(Action::Failed));
            assert!(t.error.as_ref().unwrap().starts_with("cannot create"));
            // The missing binary keeps its own error.
            let mcp = tool(&out, "agentlens-mcp");
            assert!(mcp.error.as_ref().unwrap().contains("not found"));
            assert_eq!(fs::read_to_string(bin_home(&env)).unwrap(), "not a folder");
        }

        #[test]
        fn nothing_to_write_does_not_create_the_folder() {
            let (_home, env, app) = setup();
            for name in TOOLS {
                fs::remove_file(app.join(name)).unwrap();
            }
            let out = install(&env, &app);
            assert!(!out.created_dir);
            assert!(!bin_home(&env).exists());
            assert!(out.tools.iter().all(|t| t.action == Some(Action::Failed)));
        }

        #[test]
        fn a_read_only_folder_fails_and_keeps_the_old_link() {
            let (_home, env, app) = setup();
            let dir = bin_home(&env);
            fs::create_dir_all(&dir).unwrap();
            let old = "/tmp/.mount_old/usr/bin/agentlens";
            symlink(old, dir.join("agentlens")).unwrap();
            fs::set_permissions(&dir, fs::Permissions::from_mode(0o555)).unwrap();
            if !permissions_hold(&dir) {
                fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).unwrap();
                return; // root: nothing is read-only
            }
            let out = install(&env, &app);
            let removed = remove(&env, &app);
            fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).unwrap();

            let t = tool(&out, "agentlens");
            assert_eq!(t.action, Some(Action::Failed));
            assert_eq!(t.state, State::Stale);
            assert!(t.error.is_some());
            assert_eq!(tool(&out, "agentlens-mcp").action, Some(Action::Failed));
            assert_eq!(tool(&out, "agentlens-mcp").state, State::Missing);
            assert_eq!(
                fs::read_link(dir.join("agentlens")).unwrap(),
                Path::new(old)
            );
            assert_eq!(out.errors().len(), 2);
            let r = tool(&removed, "agentlens");
            assert_eq!(r.action, Some(Action::Failed));
            assert!(r.error.as_ref().unwrap().starts_with("cannot remove"));
            assert!(is_link(&dir.join("agentlens")));
        }

        #[test]
        fn a_failed_rename_leaves_no_temporary_file() {
            let (_home, env, app) = setup();
            let dir = bin_home(&env);
            // A non-empty directory cannot be replaced by a file.
            fs::create_dir_all(dir.join("agentlens/inside")).unwrap();
            let err = create(&dir.join("agentlens"), &app.join("agentlens"), Method::Copy);
            assert!(err.is_err());
            let names: Vec<_> = fs::read_dir(&dir)
                .unwrap()
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect();
            assert_eq!(names, ["agentlens"]);
            assert!(dir.join("agentlens/inside").is_dir());
        }

        #[test]
        fn refuses_to_link_into_a_temporary_location() {
            let home = tempfile::tempdir().unwrap();
            let app = home
                .path()
                .join("T/AppTranslocation/1A2B/d/AgentLens.app/Contents/MacOS");
            fs::create_dir_all(&app).unwrap();
            for name in TOOLS {
                fs::write(app.join(name), binary(name, "1")).unwrap();
            }
            let env = Env::for_home(home.path());
            let st = status(&env, &app);
            assert!(st.note.as_ref().unwrap().contains("/Applications"));
            assert!(st.error.is_none());
            let out = install(&env, &app);
            assert!(out.error.as_ref().unwrap().contains("/Applications"));
            assert_eq!(out.errors()[0], *out.error.as_ref().unwrap());
            assert!(out.tools.iter().all(|t| t.action == Some(Action::Failed)));
            assert!(!bin_home(&env).exists(), "nothing is written");

            let dmg = install(
                &env,
                Path::new("/Volumes/AgentLens/AgentLens.app/Contents/MacOS"),
            );
            assert!(dmg.error.is_some());
            assert!(!bin_home(&env).exists());
        }

        #[test]
        fn the_api_answers_with_the_errors_of_the_tools() {
            use crate::adapters::default_registry;
            use crate::api::{self, Context};
            let (_home, env, mount) = setup_appimage();
            fs::remove_file(mount.join("agentlens")).unwrap();
            let ctx = Context {
                env,
                registry: default_registry(),
            };
            let st = api::cli_tools_status(&ctx, &mount).unwrap();
            assert!(st.errors.is_empty());
            let done = api::install_cli_tools(&ctx, &mount).unwrap();
            assert_eq!(done.schema_version, crate::report::SCHEMA_VERSION);
            assert_eq!(done.errors.len(), 1);
            assert!(done.errors[0].contains("not found"));
            let gone = api::remove_cli_tools(&ctx, &mount).unwrap();
            assert_eq!(
                tool(&gone.data, "agentlens-mcp").action,
                Some(Action::Removed)
            );
        }

        #[cfg(not(target_os = "macos"))]
        #[test]
        fn the_api_refuses_where_the_action_is_not_offered() {
            use crate::adapters::default_registry;
            use crate::api::{self, Context};
            let (_home, env, app) = setup();
            let ctx = Context {
                env: env.clone(),
                registry: default_registry(),
            };
            assert!(!api::cli_tools_status(&ctx, &app).unwrap().data.offered);
            let err = api::install_cli_tools(&ctx, &app).unwrap_err();
            assert!(err.contains("only offered"), "{err}");
            assert!(api::remove_cli_tools(&ctx, &app).is_err());
            assert!(!bin_home(&env).exists());
        }

        #[cfg(target_os = "macos")]
        #[test]
        fn the_api_installs_links_on_macos() {
            use crate::adapters::default_registry;
            use crate::api::{self, Context};
            let (_home, env, app) = setup();
            let ctx = Context {
                env,
                registry: default_registry(),
            };
            let done = api::install_cli_tools(&ctx, &app).unwrap();
            assert!(done.data.offered);
            assert_eq!(done.data.method, Method::Symlink);
            assert!(done.errors.is_empty());
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
            let (_home, env, mount) = setup_appimage();
            let out = install(&env, &mount);
            assert_eq!(out.method, Method::Copy);
            assert!(out.offered);
            assert!(out.note.as_ref().unwrap().contains("Install again"));
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
        }

        #[test]
        fn an_update_of_the_appimage_makes_the_copies_stale_and_install_replaces_them() {
            let (_home, env, mount) = setup_appimage();
            install(&env, &mount);
            let dir = bin_home(&env);
            // The update brings other binaries (of another size) and a new mount point.
            let new_mount = env.home().join("tmp/.mount_AgentLy2/usr/bin");
            fs::create_dir_all(&new_mount).unwrap();
            for name in TOOLS {
                fs::write(new_mount.join(name), binary(name, "2, a newer one")).unwrap();
            }
            let st = status(&env, &new_mount);
            assert!(st.tools.iter().all(|t| t.state == State::Stale));
            assert!(st.foreign.is_empty());

            let out = install(&env, &new_mount);
            for name in TOOLS {
                assert_eq!(tool(&out, name).action, Some(Action::Repaired));
                assert_eq!(tool(&out, name).state, State::Installed);
                assert_eq!(
                    fs::read(dir.join(name)).unwrap(),
                    binary(name, "2, a newer one")
                );
            }
        }

        #[test]
        fn remove_deletes_an_old_copy_but_not_a_foreign_file() {
            let (_home, env, mount) = setup_appimage();
            let dir = bin_home(&env);
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join("agentlens"), binary("agentlens", "0, old")).unwrap();
            fs::write(dir.join("agentlens-mcp"), "#!/bin/sh\necho mine\n").unwrap();
            let out = install(&env, &mount);
            assert_eq!(tool(&out, "agentlens").action, Some(Action::Repaired));
            assert_eq!(
                tool(&out, "agentlens-mcp").action,
                Some(Action::SkippedForeign)
            );
            fs::write(dir.join("agentlens"), binary("agentlens", "0, old")).unwrap();
            let removed = remove(&env, &mount);
            assert_eq!(tool(&removed, "agentlens").action, Some(Action::Removed));
            assert_eq!(
                tool(&removed, "agentlens-mcp").action,
                Some(Action::SkippedForeign)
            );
            assert!(!dir.join("agentlens").exists());
            assert_eq!(
                fs::read_to_string(dir.join("agentlens-mcp")).unwrap(),
                "#!/bin/sh\necho mine\n"
            );
        }

        #[test]
        fn a_link_where_copies_are_wanted_and_the_reverse_are_replaced() {
            let (_home, env, mount) = setup_appimage();
            let dir = bin_home(&env);
            let plain = Env::for_home(env.home());
            // A link from a Linux package install, then an AppImage: replaced with a copy.
            let linked = install(&plain, &mount);
            assert!(
                linked
                    .tools
                    .iter()
                    .all(|t| t.action == Some(Action::Created))
            );
            assert!(is_link(&dir.join("agentlens")));
            let copied = install(&env, &mount);
            assert!(
                copied
                    .tools
                    .iter()
                    .all(|t| t.action == Some(Action::Repaired))
            );
            assert!(!is_link(&dir.join("agentlens")));
            // And back to links.
            let relinked = install(&plain, &mount);
            assert!(
                relinked
                    .tools
                    .iter()
                    .all(|t| t.action == Some(Action::Repaired) && t.state == State::Installed)
            );
            assert!(is_link(&dir.join("agentlens")));
        }

        #[test]
        fn an_appimage_replaces_an_old_dangling_link_with_a_copy() {
            let (_home, env, mount) = setup_appimage();
            let dir = bin_home(&env);
            fs::create_dir_all(&dir).unwrap();
            symlink(
                env.home().join("tmp/.mount_old/usr/bin/agentlens"),
                dir.join("agentlens"),
            )
            .unwrap();
            let out = install(&env, &mount);
            assert_eq!(tool(&out, "agentlens").action, Some(Action::Repaired));
            assert!(
                fs::symlink_metadata(dir.join("agentlens"))
                    .unwrap()
                    .file_type()
                    .is_file()
            );
        }

        #[test]
        fn finds_the_marker_across_blocks_and_not_in_other_files() {
            let dir = tempfile::tempdir().unwrap();
            // The marker straddles the first 64 KiB block.
            let mut big = vec![b'x'; 64 * 1024 - 10];
            big.extend_from_slice(MARKER);
            big.extend(vec![b'y'; 100_000]);
            let path = dir.path().join("big");
            fs::write(&path, &big).unwrap();
            assert!(has_marker(&path));
            let plain = dir.path().join("plain");
            fs::write(&plain, vec![b'z'; 200_000]).unwrap();
            assert!(!has_marker(&plain));
            assert!(!has_marker(&dir.path().join("missing")));
            let empty = dir.path().join("empty");
            fs::write(&empty, "").unwrap();
            assert!(!has_marker(&empty));
            embed_marker();
        }

        #[test]
        fn compares_content_block_by_block() {
            let dir = tempfile::tempdir().unwrap();
            let a = dir.path().join("a");
            let b = dir.path().join("b");
            let mut bytes = vec![7u8; 200_000];
            fs::write(&a, &bytes).unwrap();
            fs::write(&b, &bytes).unwrap();
            assert!(same_content(&a, &b));
            bytes[150_000] = 8;
            fs::write(&b, &bytes).unwrap();
            assert!(!same_content(&a, &b));
            fs::write(&b, "short").unwrap();
            assert!(!same_content(&a, &b));
            let e1 = dir.path().join("e1");
            let e2 = dir.path().join("e2");
            fs::write(&e1, "").unwrap();
            fs::write(&e2, "").unwrap();
            assert!(same_content(&e1, &e2));
        }
    }
}
