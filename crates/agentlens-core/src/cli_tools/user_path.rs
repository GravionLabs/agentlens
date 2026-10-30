//! The command line tools on Windows: the install directory of the application (`bin_dir`) as an
//! entry of the user `PATH` (`HKCU\Environment\Path`), [ADR 0008](../../../../docs/adr/0008-windows-path-entry.md).
//!
//! Split in two:
//! - the string logic ([`contains`], [`add`], [`remove`]) and the flow of [`status`], [`install`] and
//!   [`remove_entry`] over a [`PathStore`], both platform independent and unit-tested everywhere;
//! - the store itself, the registry of Windows (`registry`, only compiled there): reading and writing
//!   the value with its type, the backup and the `WM_SETTINGCHANGE` broadcast.
//!
//! The rules:
//! - Only the entry equal to `bin_dir` is ours. Entries compare case-insensitively, with a trailing
//!   backslash (or slash) and surrounding quotes ignored. An entry with a `%VAR%` reference is compared
//!   as written and never expanded, so it is never ours and stays as it is.
//! - *Install* appends the entry when it is missing; *Remove* removes it. All other entries keep their
//!   order; empty segments are dropped when the value is written. Nothing to change writes nothing.
//! - The value type stays (`REG_EXPAND_SZ` or `REG_SZ`); a missing value is created as `REG_EXPAND_SZ`.
//! - Before a change the old value is saved to the backup (`HKCU\Software\GravionLabs\AgentLens\PathBackup`,
//!   value `Path`), and a failed backup writes nothing. A value that cannot be read (another type,
//!   invalid UTF-16) is left alone and the result says so.
//! - Another folder on `PATH` that holds the CLI (an older AgentLens install, a `cargo install`) is
//!   reported in `foreign` and never changed.

use std::io;
use std::path::{Path, PathBuf};

use super::{Action, CliTools, Method, State, TOOLS, Tool};

/// The user `PATH` as the registry stores it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathValue {
    pub value: String,
    /// `REG_EXPAND_SZ` (true) or `REG_SZ` (false).
    pub expand: bool,
}

/// Where the user `PATH` lives: the registry on Windows, memory in the tests.
pub trait PathStore {
    /// The value, `None` when there is none; an error when it exists but cannot be read as a string.
    fn read(&self) -> io::Result<Option<PathValue>>;
    /// Saves the old value before a change.
    fn backup(&self, old: &PathValue) -> io::Result<()>;
    fn write(&self, new: &PathValue) -> io::Result<()>;
    /// Tells running programs that the environment changed (new terminals then see the new `PATH`).
    fn broadcast(&self);
}

/// Where the value is, for the messages.
pub const PATH_KEY: &str = r"HKEY_CURRENT_USER\Environment\Path";
/// Where the old value is saved (value `Path`), the key of the installer too (#443).
pub const BACKUP_KEY: &str = r"HKEY_CURRENT_USER\Software\GravionLabs\AgentLens\PathBackup";

/// An entry as compared: without quotes, surrounding blanks and trailing separators, in lower case.
fn key(entry: &str) -> String {
    entry
        .trim()
        .trim_matches('"')
        .trim_end_matches(['\\', '/'])
        .to_lowercase()
}

/// Whether two entries name the same directory.
pub fn same_entry(a: &str, b: &str) -> bool {
    let (a, b) = (key(a), key(b));
    !a.is_empty() && a == b
}

/// The non-empty entries of a `PATH` value, in order.
pub fn entries(path: &str) -> impl Iterator<Item = &str> {
    path.split(';').filter(|e| !e.trim().is_empty())
}

/// Whether `entry` is one of the entries of `path`.
pub fn contains(path: &str, entry: &str) -> bool {
    entries(path).any(|e| same_entry(e, entry))
}

/// `path` with `entry` appended, or `None` when it is there already (nothing to write).
pub fn add(path: &str, entry: &str) -> Option<String> {
    if contains(path, entry) {
        return None;
    }
    let mut out: Vec<&str> = entries(path).collect();
    out.push(entry);
    Some(out.join(";"))
}

/// `path` without `entry` (every copy of it), or `None` when it is not there (nothing to write).
pub fn remove(path: &str, entry: &str) -> Option<String> {
    if !contains(path, entry) {
        return None;
    }
    Some(
        entries(path)
            .filter(|e| !same_entry(e, entry))
            .collect::<Vec<_>>()
            .join(";"),
    )
}

/// The entry for `bin_dir`, or why it cannot be one.
fn entry_of(bin_dir: &Path) -> Result<&str, String> {
    let entry = bin_dir
        .to_str()
        .ok_or_else(|| format!("{} is not valid Unicode", bin_dir.display()))?;
    if entry.contains([';', '%']) {
        return Err(format!(
            "the folder of AgentLens ({entry}) contains ; or %, which PATH would split or expand; \
             add it to PATH by hand"
        ));
    }
    Ok(entry)
}

/// The other folders on `path` that hold the CLI (literal entries only, `%VAR%` is not expanded).
fn other_installs(path: &str, entry: &str) -> Vec<PathBuf> {
    let cli = format!("{}{}", TOOLS[0], std::env::consts::EXE_SUFFIX);
    entries(path)
        .filter(|e| !same_entry(e, entry) && !e.contains('%'))
        .map(|e| PathBuf::from(e.trim().trim_matches('"')))
        .filter(|dir| dir.join(&cli).is_file())
        .collect()
}

fn base(bin_dir: &Path) -> CliTools {
    CliTools {
        supported: true,
        offered: true,
        method: Method::UserPath,
        dir: bin_dir.to_path_buf(),
        created_dir: false,
        on_path: false,
        profile_lines: Vec::new(),
        tools: TOOLS
            .iter()
            .map(|name| {
                let path = bin_dir.join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
                Tool {
                    name: name.to_string(),
                    target: path.clone(),
                    path,
                    state: State::Missing,
                    action: None,
                    error: None,
                }
            })
            .collect(),
        foreign: Vec::new(),
        note: None,
        error: None,
    }
}

/// Fills in what the value says: whether the entry is there, and the other installs.
fn describe(out: &mut CliTools, value: Option<&str>, entry: &str) {
    let value = value.unwrap_or("");
    out.on_path = contains(value, entry);
    let state = if out.on_path {
        State::Installed
    } else {
        State::Missing
    };
    for t in &mut out.tools {
        t.state = state;
    }
    out.foreign = other_installs(value, entry);
}

fn fail_all(mut out: CliTools, error: String) -> CliTools {
    for t in &mut out.tools {
        t.action = Some(Action::Failed);
    }
    out.error = Some(error);
    out
}

fn unreadable(e: &io::Error) -> String {
    format!("cannot read the user PATH ({PATH_KEY}): {e}; it was left alone")
}

/// Whether `bin_dir` is on the user `PATH`; writes nothing.
pub fn status(store: &dyn PathStore, bin_dir: &Path) -> CliTools {
    let mut out = base(bin_dir);
    let entry = match entry_of(bin_dir) {
        Ok(entry) => entry,
        Err(e) => {
            out.error = Some(e);
            return out;
        }
    };
    match store.read() {
        Ok(value) => describe(&mut out, value.as_ref().map(|v| v.value.as_str()), entry),
        Err(e) => out.error = Some(unreadable(&e)),
    }
    out
}

/// Saves `old`, writes `new` and tells running programs; the error is the message for the result.
fn change(store: &dyn PathStore, old: Option<&PathValue>, new: PathValue) -> Result<(), String> {
    if let Some(old) = old {
        store.backup(old).map_err(|e| {
            format!("cannot save the old PATH to {BACKUP_KEY}: {e}; the user PATH was left alone")
        })?;
    }
    store
        .write(&new)
        .map_err(|e| format!("cannot change the user PATH ({PATH_KEY}): {e}"))?;
    store.broadcast();
    Ok(())
}

/// Adds `bin_dir` to the user `PATH` unless it is there already.
pub fn install(store: &dyn PathStore, bin_dir: &Path) -> CliTools {
    let out = base(bin_dir);
    let entry = match entry_of(bin_dir) {
        Ok(entry) => entry,
        Err(e) => return fail_all(out, e),
    };
    let old = match store.read() {
        Ok(old) => old,
        Err(e) => return fail_all(status(store, bin_dir), unreadable(&e)),
    };
    let mut out = out;
    describe(&mut out, old.as_ref().map(|v| v.value.as_str()), entry);
    let present: Vec<bool> = out.tools.iter().map(|t| t.target.is_file()).collect();
    if !present.contains(&true) {
        for t in &mut out.tools {
            t.action = Some(Action::Failed);
            t.error = Some(format!("{} not found", t.target.display()));
        }
        return out;
    }
    let action = match add(old.as_ref().map_or("", |v| v.value.as_str()), entry) {
        None => Action::Kept,
        Some(value) => {
            let new = PathValue {
                value,
                expand: old.as_ref().is_none_or(|v| v.expand),
            };
            if let Err(e) = change(store, old.as_ref(), new) {
                return fail_all(out, e);
            }
            out.on_path = true;
            Action::Created
        }
    };
    for (t, present) in out.tools.iter_mut().zip(present) {
        t.state = State::Installed;
        if present {
            t.action = Some(action);
        } else {
            t.action = Some(Action::Failed);
            t.error = Some(format!("{} not found", t.target.display()));
        }
    }
    out
}

/// Removes the `bin_dir` entry from the user `PATH`; every other entry stays.
pub fn remove_entry(store: &dyn PathStore, bin_dir: &Path) -> CliTools {
    let out = base(bin_dir);
    let entry = match entry_of(bin_dir) {
        Ok(entry) => entry,
        Err(e) => return fail_all(out, e),
    };
    let old = match store.read() {
        Ok(old) => old,
        Err(e) => return fail_all(status(store, bin_dir), unreadable(&e)),
    };
    let mut out = out;
    describe(&mut out, old.as_ref().map(|v| v.value.as_str()), entry);
    let removed = old
        .as_ref()
        .and_then(|v| remove(&v.value, entry).map(|value| (v, value)));
    let action = match removed {
        None => Action::Absent,
        Some((old, value)) => {
            let new = PathValue {
                value,
                expand: old.expand,
            };
            if let Err(e) = change(store, Some(old), new) {
                return fail_all(out, e);
            }
            out.on_path = false;
            Action::Removed
        }
    };
    for t in &mut out.tools {
        t.state = State::Missing;
        t.action = Some(action);
    }
    out
}

/// The registry of the running system; on other platforms a store that cannot be read (the user
/// `PATH` method is only chosen on Windows).
pub fn system_store() -> Box<dyn PathStore> {
    #[cfg(windows)]
    {
        Box::new(registry::Registry::user())
    }
    #[cfg(not(windows))]
    {
        Box::new(NoRegistry)
    }
}

#[cfg(not(windows))]
struct NoRegistry;

#[cfg(not(windows))]
impl PathStore for NoRegistry {
    fn read(&self) -> io::Result<Option<PathValue>> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "there is no registry on this platform",
        ))
    }
    fn backup(&self, _old: &PathValue) -> io::Result<()> {
        Err(io::ErrorKind::Unsupported.into())
    }
    fn write(&self, _new: &PathValue) -> io::Result<()> {
        Err(io::ErrorKind::Unsupported.into())
    }
    fn broadcast(&self) {}
}

#[cfg(windows)]
pub mod registry {
    //! The user `PATH` in the registry of Windows.

    use std::io;

    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, REG_EXPAND_SZ, REG_SZ};
    use winreg::{RegKey, RegValue};

    use super::{PathStore, PathValue};

    /// The key of the value and of its backup, both under `HKEY_CURRENT_USER` (the tests use keys of
    /// their own).
    pub struct Registry {
        pub key: String,
        pub backup_key: String,
    }

    impl Registry {
        /// `HKCU\Environment` and `HKCU\Software\GravionLabs\AgentLens\PathBackup`.
        pub fn user() -> Registry {
            Registry {
                key: "Environment".to_string(),
                backup_key: r"Software\GravionLabs\AgentLens\PathBackup".to_string(),
            }
        }
    }

    const VALUE: &str = "Path";

    fn hkcu() -> RegKey {
        RegKey::predef(HKEY_CURRENT_USER)
    }

    /// The UTF-16 bytes of a string value, up to the first NUL.
    pub fn decode(bytes: &[u8]) -> io::Result<String> {
        let units: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .take_while(|&u| u != 0)
            .collect();
        String::from_utf16(&units).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    fn encode(value: &str) -> Vec<u8> {
        value
            .encode_utf16()
            .chain(std::iter::once(0))
            .flat_map(u16::to_le_bytes)
            .collect()
    }

    fn put(key: &str, value: &PathValue) -> io::Result<()> {
        let (key, _) = hkcu().create_subkey(key)?;
        key.set_raw_value(
            VALUE,
            &RegValue {
                bytes: encode(&value.value).into(),
                vtype: if value.expand { REG_EXPAND_SZ } else { REG_SZ },
            },
        )
    }

    impl PathStore for Registry {
        fn read(&self) -> io::Result<Option<PathValue>> {
            let key = match hkcu().open_subkey_with_flags(&self.key, KEY_READ) {
                Ok(key) => key,
                Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
                Err(e) => return Err(e),
            };
            // RegQueryValueExW: a REG_EXPAND_SZ comes back as stored, `%VAR%` not expanded.
            let raw = match key.get_raw_value(VALUE) {
                Ok(raw) => raw,
                Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
                Err(e) => return Err(e),
            };
            let expand = match raw.vtype {
                REG_EXPAND_SZ => true,
                REG_SZ => false,
                other => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("the value has type {other:?}, not a string"),
                    ));
                }
            };
            Ok(Some(PathValue {
                value: decode(&raw.bytes)?,
                expand,
            }))
        }

        fn backup(&self, old: &PathValue) -> io::Result<()> {
            put(&self.backup_key, old)
        }

        fn write(&self, new: &PathValue) -> io::Result<()> {
            put(&self.key, new)
        }

        /// Sends `WM_SETTINGCHANGE` ("Environment") to all top-level windows. The workspace forbids
        /// `unsafe`, so the message is not sent through FFI: .NET's `SetEnvironmentVariable` with the
        /// `User` target broadcasts it after every call, and deleting a value that does not exist
        /// changes nothing else. Best effort: a failure only means running programs learn of the new
        /// `PATH` later (a new terminal started from Explorer).
        fn broadcast(&self) {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            let _ = std::process::Command::new("powershell.exe")
                .args([
                    "-NoProfile",
                    "-NonInteractive",
                    "-Command",
                    "[Environment]::SetEnvironmentVariable('AgentLensSettingChange', $null, 'User')",
                ])
                .creation_flags(CREATE_NO_WINDOW)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        }
    }

    #[cfg(test)]
    mod tests {
        use super::super::{install, remove_entry, status};
        use super::*;
        use crate::cli_tools::{Action, State, TOOLS};

        /// A store over keys of its own under `HKCU\Software\GravionLabs\AgentLensTest`, removed at the end.
        struct TestKeys {
            root: String,
            registry: Registry,
        }

        impl TestKeys {
            fn new(name: &str) -> TestKeys {
                let root = format!(
                    r"Software\GravionLabs\AgentLensTest\{name}-{}",
                    std::process::id()
                );
                let _ = hkcu().delete_subkey_all(&root);
                TestKeys {
                    registry: Registry {
                        key: format!(r"{root}\Environment"),
                        backup_key: format!(r"{root}\PathBackup"),
                    },
                    root,
                }
            }

            fn raw(&self, key: &str) -> Option<RegValue<'static>> {
                hkcu().open_subkey(key).ok()?.get_raw_value(VALUE).ok()
            }
        }

        impl Drop for TestKeys {
            fn drop(&mut self) {
                let _ = hkcu().delete_subkey_all(&self.root);
            }
        }

        fn app() -> (tempfile::TempDir, std::path::PathBuf) {
            let dir = tempfile::tempdir().unwrap();
            let bin = dir.path().join("AgentLens");
            std::fs::create_dir_all(&bin).unwrap();
            for name in TOOLS {
                std::fs::write(bin.join(format!("{name}.exe")), "binary").unwrap();
            }
            (dir, bin)
        }

        #[test]
        fn installs_and_removes_keeping_the_type_and_the_other_entries() {
            let keys = TestKeys::new("roundtrip");
            let (_dir, bin) = app();
            let old = r"%USERPROFILE%\AppData\Local\Microsoft\WindowsApps;C:\Tools";
            put(
                &keys.registry.key,
                &PathValue {
                    value: old.to_string(),
                    expand: true,
                },
            )
            .unwrap();

            let out = install(&keys.registry, &bin);
            assert!(out.error.is_none(), "{:?}", out.error);
            assert!(out.tools.iter().all(|t| t.action == Some(Action::Created)));
            let raw = keys.raw(&keys.registry.key).unwrap();
            assert_eq!(raw.vtype, REG_EXPAND_SZ);
            assert_eq!(
                decode(&raw.bytes).unwrap(),
                format!("{old};{}", bin.display())
            );
            let backup = keys.raw(&keys.registry.backup_key).unwrap();
            assert_eq!(backup.vtype, REG_EXPAND_SZ);
            assert_eq!(decode(&backup.bytes).unwrap(), old);
            assert_eq!(
                status(&keys.registry, &bin).tools[0].state,
                State::Installed
            );

            let again = install(&keys.registry, &bin);
            assert!(again.tools.iter().all(|t| t.action == Some(Action::Kept)));

            let gone = remove_entry(&keys.registry, &bin);
            assert!(gone.tools.iter().all(|t| t.action == Some(Action::Removed)));
            let raw = keys.raw(&keys.registry.key).unwrap();
            assert_eq!(raw.vtype, REG_EXPAND_SZ);
            assert_eq!(decode(&raw.bytes).unwrap(), old);
            assert!(
                remove_entry(&keys.registry, &bin)
                    .tools
                    .iter()
                    .all(|t| t.action == Some(Action::Absent))
            );
        }

        #[test]
        fn keeps_a_plain_string_and_creates_a_missing_value_as_expandable() {
            let keys = TestKeys::new("types");
            let (_dir, bin) = app();
            install(&keys.registry, &bin);
            let raw = keys.raw(&keys.registry.key).unwrap();
            assert_eq!(raw.vtype, REG_EXPAND_SZ);
            assert_eq!(decode(&raw.bytes).unwrap(), bin.display().to_string());
            assert!(
                keys.raw(&keys.registry.backup_key).is_none(),
                "nothing to save"
            );

            put(
                &keys.registry.key,
                &PathValue {
                    value: r"C:\Tools".to_string(),
                    expand: false,
                },
            )
            .unwrap();
            install(&keys.registry, &bin);
            assert_eq!(keys.raw(&keys.registry.key).unwrap().vtype, REG_SZ);
        }

        #[test]
        fn leaves_a_value_of_another_type_alone() {
            let keys = TestKeys::new("dword");
            let (_dir, bin) = app();
            let (key, _) = hkcu().create_subkey(&keys.registry.key).unwrap();
            key.set_value(VALUE, &7u32).unwrap();
            let out = install(&keys.registry, &bin);
            assert!(out.error.as_ref().unwrap().contains("left alone"));
            assert!(out.tools.iter().all(|t| t.action == Some(Action::Failed)));
            let v: u32 = key.get_value(VALUE).unwrap();
            assert_eq!(v, 7);
            assert!(status(&keys.registry, &bin).error.is_some());
        }

        #[test]
        fn broadcasts_without_hanging() {
            Registry::user().broadcast();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::fs;

    #[test]
    fn compares_entries_without_case_trailing_separator_and_quotes() {
        assert!(same_entry(
            r"C:\Program Files\AgentLens",
            r"c:\program files\agentlens\"
        ));
        assert!(same_entry(r#""C:\Tools""#, r"C:\Tools"));
        assert!(same_entry(" C:\\Tools ", r"C:\Tools"));
        assert!(!same_entry(r"C:\Tools", r"C:\Tools2"));
        assert!(!same_entry("", ""));
        // A reference is not expanded: it never equals the folder it may stand for.
        assert!(!same_entry(
            r"%LOCALAPPDATA%\AgentLens",
            r"C:\Users\u\AppData\Local\AgentLens"
        ));
    }

    #[test]
    fn adds_once_at_the_end_and_keeps_the_order() {
        assert_eq!(
            add(r"C:\B;C:\A", r"C:\App").as_deref(),
            Some(r"C:\B;C:\A;C:\App")
        );
        assert_eq!(add(r"C:\B;c:\app\;C:\A", r"C:\App"), None);
        assert_eq!(add("", r"C:\App").as_deref(), Some(r"C:\App"));
    }

    #[test]
    fn drops_empty_segments_when_it_writes() {
        assert_eq!(
            add(r";C:\B;;C:\A;", r"C:\App").as_deref(),
            Some(r"C:\B;C:\A;C:\App")
        );
        assert_eq!(
            remove(r"C:\B;;C:\App;C:\A;", r"C:\App").as_deref(),
            Some(r"C:\B;C:\A")
        );
        assert_eq!(remove(";;", r"C:\App"), None);
    }

    #[test]
    fn removes_exactly_the_entry_and_every_copy_of_it() {
        assert_eq!(
            remove(r"C:\App2;C:\APP\;%APPS%\App;C:\B;C:\app", r"C:\App").as_deref(),
            Some(r"C:\App2;%APPS%\App;C:\B")
        );
        assert_eq!(remove(r"C:\B;C:\App2", r"C:\App"), None);
        assert_eq!(remove(r"C:\App", r"C:\App").as_deref(), Some(""));
    }

    #[test]
    fn keeps_references_as_written() {
        let path = r"%USERPROFILE%\AppData\Local\Microsoft\WindowsApps;%SystemRoot%";
        assert_eq!(add(path, r"C:\App").unwrap(), format!(r"{path};C:\App"));
        assert!(contains(path, "%systemroot%"));
    }

    /// A store in memory that records what was written.
    #[derive(Default)]
    struct Memory {
        value: RefCell<Option<PathValue>>,
        backup: RefCell<Option<PathValue>>,
        unreadable: bool,
        backup_fails: bool,
        write_fails: bool,
        broadcasts: RefCell<u32>,
    }

    impl Memory {
        fn with(value: &str, expand: bool) -> Memory {
            Memory {
                value: RefCell::new(Some(PathValue {
                    value: value.to_string(),
                    expand,
                })),
                ..Memory::default()
            }
        }
        fn value(&self) -> Option<PathValue> {
            self.value.borrow().clone()
        }
    }

    impl PathStore for Memory {
        fn read(&self) -> io::Result<Option<PathValue>> {
            if self.unreadable {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "not a string"));
            }
            Ok(self.value())
        }
        fn backup(&self, old: &PathValue) -> io::Result<()> {
            if self.backup_fails {
                return Err(io::ErrorKind::PermissionDenied.into());
            }
            *self.backup.borrow_mut() = Some(old.clone());
            Ok(())
        }
        fn write(&self, new: &PathValue) -> io::Result<()> {
            if self.write_fails {
                return Err(io::ErrorKind::PermissionDenied.into());
            }
            *self.value.borrow_mut() = Some(new.clone());
            Ok(())
        }
        fn broadcast(&self) {
            *self.broadcasts.borrow_mut() += 1;
        }
    }

    /// An application folder with both binaries; the entry uses the separator of this platform.
    fn app() -> (tempfile::TempDir, PathBuf, String) {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("AgentLens");
        fs::create_dir_all(&bin).unwrap();
        for name in TOOLS {
            fs::write(
                bin.join(format!("{name}{}", std::env::consts::EXE_SUFFIX)),
                "binary",
            )
            .unwrap();
        }
        let entry = bin.to_str().unwrap().to_string();
        (dir, bin, entry)
    }

    fn actions(out: &CliTools) -> Vec<Option<Action>> {
        out.tools.iter().map(|t| t.action).collect()
    }

    #[test]
    fn install_appends_the_entry_saves_the_old_value_and_broadcasts() {
        let (_dir, bin, entry) = app();
        let old = r"%USERPROFILE%\AppData\Local\Microsoft\WindowsApps;C:\Tools";
        let store = Memory::with(old, true);
        let out = install(&store, &bin);
        assert_eq!(out.method, Method::UserPath);
        assert!(out.offered && out.supported && out.on_path);
        assert_eq!(out.dir, bin);
        assert!(out.profile_lines.is_empty());
        assert!(out.errors().is_empty());
        assert_eq!(actions(&out), [Some(Action::Created); 2]);
        assert!(out.tools.iter().all(|t| t.state == State::Installed));
        assert_eq!(
            store.value(),
            Some(PathValue {
                value: format!("{old};{entry}"),
                expand: true
            })
        );
        assert_eq!(
            store.backup.borrow().as_ref().unwrap().value,
            old,
            "the old value is saved"
        );
        assert_eq!(*store.broadcasts.borrow(), 1);

        // Idempotent: the second time writes nothing.
        let again = install(&store, &bin);
        assert_eq!(actions(&again), [Some(Action::Kept); 2]);
        assert_eq!(*store.broadcasts.borrow(), 1);
        assert_eq!(store.value().unwrap().value, format!("{old};{entry}"));
    }

    #[test]
    fn install_keeps_a_plain_string_and_creates_a_missing_value_as_expandable() {
        let (_dir, bin, entry) = app();
        let store = Memory::with(r"C:\Tools", false);
        install(&store, &bin);
        assert!(!store.value().unwrap().expand);

        let empty = Memory::default();
        let out = install(&empty, &bin);
        assert_eq!(actions(&out), [Some(Action::Created); 2]);
        assert_eq!(
            empty.value(),
            Some(PathValue {
                value: entry,
                expand: true
            })
        );
        assert!(empty.backup.borrow().is_none(), "no old value to save");
    }

    #[test]
    fn an_entry_of_an_earlier_installer_counts_as_installed() {
        let (_dir, bin, entry) = app();
        let store = Memory::with(&format!(r"C:\Tools;{}\", entry.to_uppercase()), true);
        let st = status(&store, &bin);
        assert!(st.on_path);
        assert!(
            st.tools
                .iter()
                .all(|t| t.state == State::Installed && t.action.is_none())
        );
        let out = install(&store, &bin);
        assert_eq!(actions(&out), [Some(Action::Kept); 2]);
        assert!(store.backup.borrow().is_none());
    }

    #[test]
    fn remove_takes_exactly_the_entry_and_is_idempotent() {
        let (_dir, bin, entry) = app();
        let store = Memory::with(&format!(r"C:\A;{entry};%B%\bin;C:\C"), false);
        let out = remove_entry(&store, &bin);
        assert_eq!(actions(&out), [Some(Action::Removed); 2]);
        assert!(!out.on_path);
        assert!(out.tools.iter().all(|t| t.state == State::Missing));
        assert_eq!(
            store.value(),
            Some(PathValue {
                value: r"C:\A;%B%\bin;C:\C".to_string(),
                expand: false
            })
        );
        assert_eq!(
            store.backup.borrow().as_ref().unwrap().value,
            format!(r"C:\A;{entry};%B%\bin;C:\C")
        );
        assert_eq!(*store.broadcasts.borrow(), 1);

        let again = remove_entry(&store, &bin);
        assert_eq!(actions(&again), [Some(Action::Absent); 2]);
        assert_eq!(*store.broadcasts.borrow(), 1);
        let none = Memory::default();
        assert_eq!(
            actions(&remove_entry(&none, &bin)),
            [Some(Action::Absent); 2]
        );
        assert!(none.value().is_none(), "no value is created by remove");
    }

    #[test]
    fn status_writes_nothing() {
        let (_dir, bin, _entry) = app();
        let store = Memory::with(r"C:\Tools", true);
        let st = status(&store, &bin);
        assert!(!st.on_path);
        assert!(
            st.tools
                .iter()
                .all(|t| t.state == State::Missing && t.action.is_none())
        );
        assert_eq!(store.value().unwrap().value, r"C:\Tools");
        assert!(store.backup.borrow().is_none());
        assert_eq!(*store.broadcasts.borrow(), 0);
    }

    #[test]
    fn an_unreadable_value_is_left_alone_and_named() {
        let (_dir, bin, _entry) = app();
        let store = Memory {
            unreadable: true,
            ..Memory::with(r"C:\Tools", true)
        };
        for out in [install(&store, &bin), remove_entry(&store, &bin)] {
            assert!(out.error.as_ref().unwrap().contains("left alone"));
            assert_eq!(actions(&out), [Some(Action::Failed); 2]);
            assert_eq!(out.errors().len(), 1);
        }
        assert!(status(&store, &bin).error.unwrap().contains(PATH_KEY));
        assert_eq!(store.value().unwrap().value, r"C:\Tools");
        assert!(store.backup.borrow().is_none());
    }

    #[test]
    fn a_failed_backup_writes_nothing() {
        let (_dir, bin, _entry) = app();
        let store = Memory {
            backup_fails: true,
            ..Memory::with(r"C:\Tools", true)
        };
        let out = install(&store, &bin);
        assert!(out.error.as_ref().unwrap().contains(BACKUP_KEY));
        assert_eq!(actions(&out), [Some(Action::Failed); 2]);
        assert!(!out.on_path);
        assert_eq!(store.value().unwrap().value, r"C:\Tools");
        assert_eq!(*store.broadcasts.borrow(), 0);
    }

    #[test]
    fn a_failed_write_is_an_error() {
        let (_dir, bin, entry) = app();
        let store = Memory {
            write_fails: true,
            ..Memory::with(&format!(r"C:\Tools;{entry}"), true)
        };
        let out = remove_entry(&store, &bin);
        assert!(out.error.as_ref().unwrap().starts_with("cannot change"));
        assert_eq!(actions(&out), [Some(Action::Failed); 2]);
        assert!(out.on_path, "the entry is still there");
        assert_eq!(*store.broadcasts.borrow(), 0);
    }

    #[test]
    fn missing_binaries_fail_and_none_at_all_writes_nothing() {
        let (_dir, bin, _entry) = app();
        let mcp = bin.join(format!("agentlens-mcp{}", std::env::consts::EXE_SUFFIX));
        fs::remove_file(&mcp).unwrap();
        let store = Memory::with(r"C:\Tools", true);
        let out = install(&store, &bin);
        assert_eq!(out.tools[0].action, Some(Action::Created));
        assert_eq!(out.tools[1].action, Some(Action::Failed));
        assert!(out.tools[1].error.as_ref().unwrap().contains("not found"));
        assert_eq!(out.errors().len(), 1);

        let (_dir2, empty_bin, _) = app();
        for name in TOOLS {
            fs::remove_file(empty_bin.join(format!("{name}{}", std::env::consts::EXE_SUFFIX)))
                .unwrap();
        }
        let untouched = Memory::with(r"C:\Tools", true);
        let none = install(&untouched, &empty_bin);
        assert_eq!(actions(&none), [Some(Action::Failed); 2]);
        assert_eq!(untouched.value().unwrap().value, r"C:\Tools");
    }

    #[test]
    fn refuses_a_folder_that_path_would_split_or_expand() {
        let store = Memory::with(r"C:\Tools", true);
        for bad in ["/apps/a;b", "/apps/%x%"] {
            let out = install(&store, Path::new(bad));
            assert!(out.error.as_ref().unwrap().contains("; or %"));
            assert_eq!(actions(&out), [Some(Action::Failed); 2]);
            assert!(status(&store, Path::new(bad)).error.is_some());
        }
        assert_eq!(store.value().unwrap().value, r"C:\Tools");
    }

    #[test]
    fn reports_other_folders_that_hold_the_cli_and_never_removes_them() {
        let (_dir, bin, entry) = app();
        let (_old_dir, old_bin, old_entry) = app();
        let store = Memory::with(&format!(r"{old_entry};C:\Tools;%X%"), true);
        let out = install(&store, &bin);
        assert_eq!(out.foreign, vec![old_bin.clone()]);
        assert_eq!(
            store.value().unwrap().value,
            format!(r"{old_entry};C:\Tools;%X%;{entry}")
        );
        let gone = remove_entry(&store, &bin);
        assert_eq!(gone.foreign, vec![old_bin]);
        assert_eq!(
            store.value().unwrap().value,
            format!(r"{old_entry};C:\Tools;%X%")
        );
    }
}
