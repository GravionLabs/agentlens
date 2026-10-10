//! Helpers for adapter tests: build a throw-away home directory from fixtures.
#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};

use agentlens_core::env::Env;
use tempfile::TempDir;

/// A temporary home directory that is deleted on drop.
pub struct TempHome {
    dir: TempDir,
}

impl TempHome {
    pub fn new() -> Self {
        TempHome {
            dir: tempfile::tempdir().expect("create temp dir"),
        }
    }

    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    /// Writes `contents` to `rel` (relative to home), creating parent directories.
    pub fn write(&self, rel: &str, contents: impl AsRef<[u8]>) -> PathBuf {
        let path = self.dir.path().join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, contents).unwrap();
        path
    }

    /// Copies a file from `tests/fixtures/<fixture>` to `rel` in the home directory.
    pub fn copy_fixture(&self, fixture: &str, rel: &str) -> PathBuf {
        let src = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(fixture);
        self.write(
            rel,
            fs::read(&src).unwrap_or_else(|e| panic!("read fixture {}: {e}", src.display())),
        )
    }

    pub fn env(&self) -> Env {
        Env::for_home(self.dir.path())
    }
}

impl Default for TempHome {
    fn default() -> Self {
        Self::new()
    }
}
