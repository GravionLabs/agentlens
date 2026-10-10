//! Inventory of instruction files (`CLAUDE.md`, `AGENTS.md`, Copilot
//! instructions, rules) that clients add to the context of every request.
//!
//! Only sizes are read (file metadata), never the content.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::env::Env;
use crate::inventory::{claude_dir, config_home, copilot_dir};
use crate::model::{Source, Warning};
use crate::tokens::{TokenEstimator, Tokens};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InstructionKind {
    /// `CLAUDE.md`, `CLAUDE.local.md`, `.claude/CLAUDE.md`
    ClaudeMd,
    /// `AGENTS.md`
    AgentsMd,
    /// `.github/copilot-instructions.md` and `.github/instructions/*.md`
    CopilotInstructions,
    /// `.claude/rules/*.md`
    Rules,
}

impl InstructionKind {
    /// Clients that load this kind of file (client documentation, October 2026).
    pub fn clients(self) -> &'static [Source] {
        match self {
            InstructionKind::ClaudeMd | InstructionKind::Rules => &[Source::ClaudeCode],
            InstructionKind::AgentsMd => {
                &[Source::OpenCode, Source::CopilotCli, Source::VsCodeCopilot]
            }
            InstructionKind::CopilotInstructions => &[Source::CopilotCli, Source::VsCodeCopilot],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstructionFile {
    pub path: PathBuf,
    pub kind: InstructionKind,
    /// The project whose sessions load the file; `None` for user-level files.
    pub project: Option<PathBuf>,
    pub clients: Vec<Source>,
    pub bytes: u64,
    /// Estimated size in tokens.
    pub tokens: Tokens,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstructionInventory {
    pub files: Vec<InstructionFile>,
    pub warnings: Vec<Warning>,
}

struct Scan<'a> {
    estimator: &'a dyn TokenEstimator,
    out: InstructionInventory,
}

impl Scan<'_> {
    fn file(&mut self, path: &Path, kind: InstructionKind, project: Option<&Path>) {
        let meta = match fs::metadata(path) {
            Ok(m) if m.is_file() => m,
            Ok(_) => return,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
            Err(e) => {
                self.out.warnings.push(
                    Warning::new(format!("cannot read instruction file: {e}"))
                        .at(path.display().to_string()),
                );
                return;
            }
        };
        self.out.files.push(InstructionFile {
            path: path.to_path_buf(),
            kind,
            project: project.map(Path::to_path_buf),
            clients: kind.clients().to_vec(),
            bytes: meta.len(),
            tokens: self.estimator.count_bytes(meta.len()),
        });
    }

    /// `*.md` files below `dir`, at most `depth` levels deep.
    fn markdown_in(
        &mut self,
        dir: &Path,
        suffix: &str,
        depth: u32,
        kind: InstructionKind,
        project: Option<&Path>,
    ) {
        let Ok(rd) = fs::read_dir(dir) else { return };
        let mut paths: Vec<PathBuf> = rd.filter_map(Result::ok).map(|e| e.path()).collect();
        paths.sort();
        for p in paths {
            if p.is_dir() {
                if depth > 0 {
                    self.markdown_in(&p, suffix, depth - 1, kind, project);
                }
            } else if p
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with(suffix))
            {
                self.file(&p, kind, project);
            }
        }
    }
}

/// The directory itself and its ancestors up to the git root (or, without a
/// repository, up to but excluding the home directory).
fn ancestors_to_root<'a>(project: &'a Path, home: &Path) -> Vec<&'a Path> {
    let mut dirs = Vec::new();
    for dir in project.ancestors() {
        if dir == home || dir.parent().is_none() {
            break;
        }
        dirs.push(dir);
        if dir.join(".git").exists() {
            break;
        }
    }
    dirs
}

/// Instruction files of the user and of the given projects.
pub fn scan(
    env: &Env,
    projects: &[PathBuf],
    estimator: &dyn TokenEstimator,
) -> InstructionInventory {
    use InstructionKind::*;
    let mut s = Scan {
        estimator,
        out: InstructionInventory::default(),
    };
    let claude = claude_dir(env);
    s.file(&claude.join("CLAUDE.md"), ClaudeMd, None);
    s.markdown_in(&claude.join("rules"), ".md", 3, Rules, None);
    s.file(&config_home(env).join("opencode/AGENTS.md"), AgentsMd, None);
    s.file(
        &copilot_dir(env).join("copilot-instructions.md"),
        CopilotInstructions,
        None,
    );

    for project in projects {
        let p = Some(project.as_path());
        for dir in ancestors_to_root(project, env.home()) {
            s.file(&dir.join("CLAUDE.md"), ClaudeMd, p);
            s.file(&dir.join("CLAUDE.local.md"), ClaudeMd, p);
            s.file(&dir.join(".claude/CLAUDE.md"), ClaudeMd, p);
            s.file(&dir.join("AGENTS.md"), AgentsMd, p);
        }
        s.markdown_in(&project.join(".claude/rules"), ".md", 3, Rules, p);
        s.file(
            &project.join(".github/copilot-instructions.md"),
            CopilotInstructions,
            p,
        );
        s.markdown_in(
            &project.join(".github/instructions"),
            ".instructions.md",
            2,
            CopilotInstructions,
            p,
        );
    }
    s.out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokens::CharsPerToken;

    fn write(root: &Path, rel: &str, len: usize) {
        let p = root.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, "x".repeat(len)).unwrap();
    }

    fn rels(inv: &InstructionInventory, root: &Path) -> Vec<String> {
        inv.files
            .iter()
            .map(|f| {
                f.path
                    .strip_prefix(root)
                    .unwrap()
                    .display()
                    .to_string()
                    .replace('\\', "/")
            })
            .collect()
    }

    #[test]
    fn finds_user_and_project_files_with_sizes_and_clients() {
        let home = tempfile::tempdir().unwrap();
        let h = home.path();
        write(h, ".claude/CLAUDE.md", 40);
        write(h, ".claude/rules/style.md", 80);
        write(h, "repo/.git/HEAD", 1);
        write(h, "repo/AGENTS.md", 400);
        write(h, "repo/CLAUDE.md", 200);
        write(h, "repo/pkg/CLAUDE.md", 100);
        write(h, "repo/pkg/.github/copilot-instructions.md", 60);
        write(h, "repo/pkg/.github/instructions/a.instructions.md", 20);
        write(h, "repo/pkg/.github/instructions/ignored.md", 20);
        let project = h.join("repo/pkg");
        let inv = scan(
            &Env::for_home(h),
            std::slice::from_ref(&project),
            &CharsPerToken,
        );
        assert!(inv.warnings.is_empty());
        assert_eq!(
            rels(&inv, h),
            [
                ".claude/CLAUDE.md",
                ".claude/rules/style.md",
                "repo/pkg/CLAUDE.md",
                "repo/CLAUDE.md",
                "repo/AGENTS.md",
                "repo/pkg/.github/copilot-instructions.md",
                "repo/pkg/.github/instructions/a.instructions.md",
            ]
        );
        let agents = inv
            .files
            .iter()
            .find(|f| f.kind == InstructionKind::AgentsMd)
            .unwrap();
        assert_eq!(
            (agents.bytes, agents.tokens.count, agents.tokens.measured),
            (400, 100, false)
        );
        assert!(!agents.clients.contains(&Source::ClaudeCode));
        assert_eq!(agents.project.as_deref(), Some(project.as_path()));
        let user = &inv.files[0];
        assert_eq!(user.project, None);
        assert_eq!(user.clients, [Source::ClaudeCode]);
    }

    #[test]
    fn the_walk_stops_at_the_git_root_and_before_home() {
        let home = tempfile::tempdir().unwrap();
        let h = home.path();
        write(h, "AGENTS.md", 10); // in home: not a project file
        write(h, "outer/AGENTS.md", 10); // above the git root
        write(h, "outer/repo/.git/HEAD", 1);
        write(h, "outer/repo/sub/AGENTS.md", 10);
        let project = h.join("outer/repo/sub");
        let inv = scan(&Env::for_home(h), &[project], &CharsPerToken);
        assert_eq!(rels(&inv, h), ["outer/repo/sub/AGENTS.md"]);
    }

    #[test]
    fn missing_files_are_silent_and_no_content_is_kept() {
        let home = tempfile::tempdir().unwrap();
        let inv = scan(
            &Env::for_home(home.path()),
            &[home.path().join("nope")],
            &CharsPerToken,
        );
        assert!(inv.files.is_empty() && inv.warnings.is_empty());
        let json = serde_json::to_string(&inv).unwrap();
        assert!(!json.contains("content"));
    }
}
