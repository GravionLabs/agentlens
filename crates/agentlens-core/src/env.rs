//! The environment adapters read from. Tests build one over a temp directory.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Environment variables the adapters (and `cli_tools`) care about.
const KNOWN_VARS: &[&str] = &[
    "CLAUDE_CONFIG_DIR",
    "COPILOT_HOME",
    "XDG_CACHE_HOME",
    "XDG_CONFIG_HOME",
    "XDG_DATA_HOME",
    "APPDATA",
    "LOCALAPPDATA",
    // The AppImage file while the application runs from one (`cli_tools`).
    "APPIMAGE",
    // Whether `~/.local/bin` is on the search path (`cli_tools`).
    "PATH",
];

/// Home directory plus the relevant environment variables.
#[derive(Debug, Clone)]
pub struct Env {
    home: PathBuf,
    vars: BTreeMap<String, String>,
}

impl Env {
    /// Reads home directory and known variables from the running process.
    pub fn from_system() -> Option<Env> {
        let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
        Some(Env::from_lookup(home, |k| std::env::var(k).ok()))
    }

    /// The home directory and the known variables that `lookup` returns (the process environment
    /// for [`Env::from_system`]).
    fn from_lookup(home: impl Into<PathBuf>, lookup: impl Fn(&str) -> Option<String>) -> Env {
        let vars = KNOWN_VARS
            .iter()
            .filter_map(|k| lookup(k).map(|v| (k.to_string(), v)))
            .collect();
        Env {
            home: home.into(),
            vars,
        }
    }

    /// An environment with the given home directory and no variables set.
    pub fn for_home(home: impl Into<PathBuf>) -> Env {
        Env {
            home: home.into(),
            vars: BTreeMap::new(),
        }
    }

    pub fn with_var(mut self, key: &str, value: impl Into<String>) -> Env {
        self.vars.insert(key.to_string(), value.into());
        self
    }

    pub fn home(&self) -> &Path {
        &self.home
    }

    pub fn var(&self, key: &str) -> Option<&str> {
        self.vars
            .get(key)
            .map(String::as_str)
            .filter(|v| !v.is_empty())
    }

    /// `$XDG_CACHE_HOME` or `~/.cache`.
    pub fn cache_home(&self) -> PathBuf {
        self.var("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| self.home.join(".cache"))
    }

    /// `$XDG_DATA_HOME` or `~/.local/share`.
    pub fn data_home(&self) -> PathBuf {
        self.var("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| self.home.join(".local/share"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_home_falls_back_to_local_share() {
        let env = Env::for_home("/h");
        assert_eq!(env.data_home(), PathBuf::from("/h/.local/share"));
        let env = env.with_var("XDG_DATA_HOME", "/x");
        assert_eq!(env.data_home(), PathBuf::from("/x"));
    }

    #[test]
    fn the_system_environment_keeps_appimage_and_path() {
        let env = Env::from_lookup("/h", |k| match k {
            "APPIMAGE" => Some("/apps/AgentLens.AppImage".to_string()),
            "PATH" => Some("/usr/bin:/h/.local/bin".to_string()),
            "SECRET_TOKEN" => Some("never kept".to_string()),
            _ => None,
        });
        assert_eq!(env.var("APPIMAGE"), Some("/apps/AgentLens.AppImage"));
        assert_eq!(env.var("PATH"), Some("/usr/bin:/h/.local/bin"));
        assert_eq!(env.var("SECRET_TOKEN"), None);
        assert_eq!(env.home(), Path::new("/h"));
        // The real constructor reads the same list.
        if let Some(sys) = Env::from_system() {
            assert_eq!(
                sys.var("PATH"),
                std::env::var("PATH")
                    .ok()
                    .as_deref()
                    .filter(|p| !p.is_empty())
            );
        }
    }

    #[test]
    fn empty_variable_counts_as_unset() {
        let env = Env::for_home("/h").with_var("COPILOT_HOME", "");
        assert_eq!(env.var("COPILOT_HOME"), None);
    }
}
