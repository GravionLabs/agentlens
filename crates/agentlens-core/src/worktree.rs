//! Resolves the directory of a git worktree to its main repository.
//!
//! Workflows that run every step in `<repo>/.worktrees/<name>` make a client
//! record that path as the session's working directory; without resolution one
//! repository shows up as many projects. Reading is read-only.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

use crate::model::Session;

/// The directory a workflow puts its worktrees in, inside the repository.
const WORKTREES_DIR: &str = ".worktrees";

/// Resolves `project_dir` of sessions and remembers the answers, so each
/// directory is looked at once per read.
#[derive(Debug, Default)]
pub struct WorktreeResolver {
    cache: HashMap<PathBuf, Option<PathBuf>>,
}

impl WorktreeResolver {
    pub fn new() -> Self {
        WorktreeResolver::default()
    }

    /// Replaces a worktree `project_dir` by its main repository and keeps the
    /// original in `worktree_dir`. Other sessions stay as they are.
    pub fn apply(&mut self, session: &mut Session) {
        let Some(dir) = session.project_dir.clone() else {
            return;
        };
        let resolved = self
            .cache
            .entry(dir.clone())
            .or_insert_with(|| main_repository(&dir))
            .clone();
        if let Some(main) = resolved {
            session.worktree_dir = Some(dir);
            session.project_dir = Some(main);
        }
    }
}

/// The main working tree of the repository `dir` is a worktree of, or `None`
/// if `dir` is not (known to be) inside a linked worktree.
///
/// An existing worktree is recognized by its `.git` file (`gitdir: …`) and the
/// `commondir` of that git directory. A worktree that was removed is cut at its
/// `.worktrees` segment. A path inside the repository itself, a submodule or no
/// repository at all stays.
pub fn main_repository(dir: &Path) -> Option<PathBuf> {
    if dir.exists() {
        for ancestor in dir.ancestors() {
            let dot_git = ancestor.join(".git");
            if dot_git.is_file() {
                let main = main_of_linked(ancestor, &dot_git)?;
                let rest = dir.strip_prefix(ancestor).ok()?;
                return Some(main.join(rest));
            }
            if dot_git.is_dir() {
                return None;
            }
        }
    }
    cut_at_worktrees(dir)
}

/// `root` has a `.git` file: the main working tree if it is a linked worktree.
fn main_of_linked(root: &Path, dot_git: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(dot_git).ok()?;
    let gitdir = text.lines().find_map(|l| l.strip_prefix("gitdir:"))?.trim();
    let gitdir = normalize(&root.join(gitdir));
    // Only a linked worktree has a `commondir`; a submodule does not.
    let common = std::fs::read_to_string(gitdir.join("commondir")).ok()?;
    let common = normalize(&gitdir.join(common.trim()));
    if common.file_name()? != ".git" {
        return None;
    }
    common.parent().map(Path::to_path_buf)
}

/// `<repo>/.worktrees/<name>/…` → `<repo>`.
fn cut_at_worktrees(dir: &Path) -> Option<PathBuf> {
    let parts: Vec<Component> = dir.components().collect();
    let at = parts
        .iter()
        .position(|c| matches!(c, Component::Normal(n) if *n == WORKTREES_DIR))?;
    // Needs a repository part before the segment and a worktree name after it.
    if at == 0
        || at + 1 >= parts.len()
        || parts[..at]
            .iter()
            .all(|c| !matches!(c, Component::Normal(_)))
    {
        return None;
    }
    Some(parts[..at].iter().collect())
}

/// Removes `.` and `..` segments without touching the file system.
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// A main repository `repo` with a linked worktree `repo/.worktrees/wt`.
    fn repo_with_worktree(root: &Path) -> (PathBuf, PathBuf) {
        let repo = root.join("repo");
        let wt = repo.join(".worktrees/wt");
        let admin = repo.join(".git/worktrees/wt");
        fs::create_dir_all(&admin).unwrap();
        fs::create_dir_all(wt.join("src")).unwrap();
        fs::write(admin.join("commondir"), "../..\n").unwrap();
        fs::write(wt.join(".git"), format!("gitdir: {}\n", admin.display())).unwrap();
        (repo, wt)
    }

    #[test]
    fn a_worktree_resolves_to_its_main_repository() {
        let tmp = tempfile::tempdir().unwrap();
        let (repo, wt) = repo_with_worktree(tmp.path());
        assert_eq!(main_repository(&wt), Some(repo.clone()));
        assert_eq!(main_repository(&wt.join("src")), Some(repo.join("src")));
    }

    #[test]
    fn a_relative_gitdir_is_followed() {
        let tmp = tempfile::tempdir().unwrap();
        let (repo, wt) = repo_with_worktree(tmp.path());
        fs::write(wt.join(".git"), "gitdir: ../../.git/worktrees/wt\n").unwrap();
        assert_eq!(main_repository(&wt), Some(repo));
    }

    #[test]
    fn a_removed_worktree_is_cut_at_the_worktrees_segment() {
        let tmp = tempfile::tempdir().unwrap();
        let (repo, wt) = repo_with_worktree(tmp.path());
        fs::remove_dir_all(&wt).unwrap();
        assert_eq!(main_repository(&wt), Some(repo.clone()));
        assert_eq!(main_repository(&wt.join("src")), Some(repo));
    }

    #[test]
    fn the_main_repository_a_submodule_and_plain_paths_stay() {
        let tmp = tempfile::tempdir().unwrap();
        let (repo, _) = repo_with_worktree(tmp.path());
        assert_eq!(main_repository(&repo), None);
        assert_eq!(main_repository(&repo.join(".worktrees")), None);

        let sub = tmp.path().join("sub");
        let admin = repo.join(".git/modules/sub");
        fs::create_dir_all(&sub).unwrap();
        fs::create_dir_all(&admin).unwrap();
        fs::write(sub.join(".git"), format!("gitdir: {}\n", admin.display())).unwrap();
        assert_eq!(main_repository(&sub), None);

        assert_eq!(main_repository(Path::new("/work/demo")), None);
        assert_eq!(main_repository(Path::new("/work/.worktrees")), None);
    }

    #[test]
    fn the_resolver_keeps_the_original_path() {
        let tmp = tempfile::tempdir().unwrap();
        let (repo, wt) = repo_with_worktree(tmp.path());
        let mut resolver = WorktreeResolver::new();
        let mut s = Session::new("s", crate::model::Source::ClaudeCode);
        s.project_dir = Some(wt.clone());
        resolver.apply(&mut s);
        assert_eq!(s.project_dir, Some(repo.clone()));
        assert_eq!(s.worktree_dir, Some(wt));

        let mut plain = Session::new("p", crate::model::Source::ClaudeCode);
        plain.project_dir = Some(repo.clone());
        resolver.apply(&mut plain);
        assert_eq!(plain.project_dir, Some(repo));
        assert_eq!(plain.worktree_dir, None);
    }
}
