//! Which installed agents, commands and skills name a skill in their own files.
//!
//! A text search for the exact skill name in those files, nothing else: the text is not kept, only the
//! kind and name of the file that mentions it. A mention does not prove that the skill is used.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::catalog::Catalog;
use crate::env::Env;
use crate::inventory::{Inventory, claude_dir, config_home, copilot_dir};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReferenceKind {
    Agent,
    Command,
    Skill,
}

/// An installed agent, command or skill whose file mentions a skill.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Reference {
    pub kind: ReferenceKind,
    pub name: String,
}

impl Reference {
    /// `agent dev-workflow`.
    pub fn label(&self) -> String {
        let kind = match self.kind {
            ReferenceKind::Agent => "agent",
            ReferenceKind::Command => "command",
            ReferenceKind::Skill => "skill",
        };
        format!("{kind} {}", self.name)
    }
}

/// The references per skill name; a skill nobody mentions has no entry.
pub type References = BTreeMap<String, Vec<Reference>>;

const MAX_DEPTH: usize = 3;

/// Command and prompt files below `dir`; a file in a subfolder is named `folder:file`.
fn command_files(
    dir: &Path,
    suffix: &str,
    prefix: &str,
    depth: usize,
    out: &mut Vec<(String, PathBuf)>,
) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    let mut paths: Vec<PathBuf> = rd.filter_map(Result::ok).map(|e| e.path()).collect();
    paths.sort();
    for path in paths {
        let Some(file) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if path.is_dir() {
            if depth < MAX_DEPTH {
                command_files(&path, suffix, &format!("{prefix}{file}:"), depth + 1, out);
            }
        } else if let Some(stem) = file.strip_suffix(suffix) {
            out.push((format!("{prefix}{stem}"), path));
        }
    }
}

fn is_name_char(c: char) -> bool {
    c.is_alphanumeric() || c == '-' || c == '_'
}

/// `name` occurs in `text` as a whole word: not as a part of a longer name such as `my-name`.
fn mentions(text: &str, name: &str) -> bool {
    text.match_indices(name).any(|(at, _)| {
        let before = text[..at].chars().next_back();
        let after = text[at + name.len()..].chars().next();
        !before.is_some_and(is_name_char) && !after.is_some_and(is_name_char)
    })
}

/// Searches the files of the installed agents, commands and skills for the given skill names.
pub fn scan(
    env: &Env,
    projects: &[PathBuf],
    names: &[&str],
    inventory: &Inventory,
    catalog: &Catalog,
) -> References {
    let mut out = References::new();
    if names.is_empty() {
        return out;
    }

    let mut commands: Vec<(String, PathBuf)> = Vec::new();
    let opencode = config_home(env).join("opencode");
    command_files(
        &claude_dir(env).join("commands"),
        ".md",
        "",
        0,
        &mut commands,
    );
    command_files(&opencode.join("commands"), ".md", "", 0, &mut commands);
    command_files(&opencode.join("command"), ".md", "", 0, &mut commands);
    command_files(
        &copilot_dir(env).join("prompts"),
        ".prompt.md",
        "",
        0,
        &mut commands,
    );
    for project in projects {
        command_files(
            &project.join(".claude/commands"),
            ".md",
            "",
            0,
            &mut commands,
        );
        command_files(
            &project.join(".opencode/commands"),
            ".md",
            "",
            0,
            &mut commands,
        );
        command_files(
            &project.join(".opencode/command"),
            ".md",
            "",
            0,
            &mut commands,
        );
        command_files(
            &project.join(".github/prompts"),
            ".prompt.md",
            "",
            0,
            &mut commands,
        );
    }

    let mut files: Vec<(Reference, &Path)> = Vec::new();
    for a in &catalog.agents {
        files.push((
            Reference {
                kind: ReferenceKind::Agent,
                name: a.name.clone(),
            },
            &a.file,
        ));
    }
    for (name, path) in &commands {
        files.push((
            Reference {
                kind: ReferenceKind::Command,
                name: name.clone(),
            },
            path,
        ));
    }
    for s in &inventory.skills {
        files.push((
            Reference {
                kind: ReferenceKind::Skill,
                name: s.name.clone(),
            },
            &s.file,
        ));
    }

    let mut seen: BTreeSet<(Reference, &Path)> = BTreeSet::new();
    for (reference, path) in files {
        if !seen.insert((reference.clone(), path)) {
            continue;
        }
        let Ok(text) = fs::read_to_string(path) else {
            continue;
        };
        for name in names {
            let itself = reference.kind == ReferenceKind::Skill && reference.name == *name;
            if !itself && mentions(&text, name) {
                let refs = out.entry((*name).to_owned()).or_default();
                if !refs.contains(&reference) {
                    refs.push(reference.clone());
                }
            }
        }
    }
    for refs in out.values_mut() {
        refs.sort();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog;
    use crate::inventory;
    use crate::tokens::CharsPerToken;

    fn write(root: &Path, rel: &str, text: &str) {
        let p = root.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    }

    fn run(home: &Path, names: &[&str]) -> References {
        let env = Env::for_home(home);
        let projects = [home.join("repo")];
        let inv = inventory::scan(&inventory::locations(&env, &projects), &CharsPerToken);
        let cat = catalog::scan(&env, &projects, &CharsPerToken);
        scan(&env, &projects, names, &inv, &cat)
    }

    #[test]
    fn whole_names_only() {
        assert!(mentions("run verify-project now", "verify-project"));
        assert!(mentions("`verify-project`", "verify-project"));
        assert!(mentions("verify-project", "verify-project"));
        assert!(!mentions("my-verify-project", "verify-project"));
        assert!(!mentions("verify-project2", "verify-project"));
        assert!(!mentions("verify-project_x", "verify-project"));
        assert!(mentions(
            "my-verify-project and verify-project.",
            "verify-project"
        ));
    }

    #[test]
    fn agents_commands_and_skills_that_name_a_skill_are_found() {
        let home = tempfile::tempdir().unwrap();
        let h = home.path();
        write(
            h,
            ".claude/agents/flow.md",
            "---\nname: flow\ndescription: d\n---\nUses review-fanout and verify-project.",
        );
        write(
            h,
            ".claude/agents/other.md",
            "---\nname: other\ndescription: d\n---\nUses nothing.",
        );
        write(
            h,
            ".claude/commands/ship.md",
            "Run verify-project, then release.",
        );
        write(
            h,
            "repo/.claude/commands/team/audit.md",
            "Call review-fanout.",
        );
        write(
            h,
            ".claude/skills/verify-project/SKILL.md",
            "---\nname: verify-project\ndescription: d\n---\nverify-project checks things.",
        );
        write(
            h,
            ".claude/skills/review-fanout/SKILL.md",
            "---\nname: review-fanout\ndescription: d\n---\nThen verify-project.",
        );
        write(
            h,
            ".claude/skills/lonely/SKILL.md",
            "---\nname: lonely\ndescription: d\n---\nbody",
        );

        let refs = run(h, &["verify-project", "review-fanout", "lonely"]);
        let labels = |name: &str| -> Vec<String> {
            refs.get(name)
                .map(|r| r.iter().map(Reference::label).collect())
                .unwrap_or_default()
        };
        assert_eq!(
            labels("verify-project"),
            ["agent flow", "command ship", "skill review-fanout"],
            "its own file does not count"
        );
        assert_eq!(
            labels("review-fanout"),
            ["agent flow", "command team:audit"]
        );
        assert!(!refs.contains_key("lonely"));
    }

    #[test]
    fn nothing_is_searched_without_names() {
        let home = tempfile::tempdir().unwrap();
        assert!(run(home.path(), &[]).is_empty());
    }
}
