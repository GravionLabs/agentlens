//! Static inventory of the skills installed on this machine, with their
//! estimated token cost.
//!
//! Two costs matter: the *baseline* (name and description are part of every
//! request of a client that loads the skill) and the *body* (added to the
//! context when the skill is loaded).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::env::Env;
use crate::model::{Source, Warning};
use crate::tokens::{TokenEstimator, Tokens};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LocationKind {
    UserClaude,
    UserAgents,
    UserCopilot,
    UserOpenCode,
    ProjectClaude,
    ProjectAgents,
    ProjectGithub,
    ProjectOpenCode,
    Plugin,
}

impl LocationKind {
    /// Clients that load skills from this kind of location (client documentation, October 2026).
    pub fn clients(self) -> &'static [Source] {
        use LocationKind::*;
        const ALL: &[Source] = &[
            Source::ClaudeCode,
            Source::OpenCode,
            Source::CopilotCli,
            Source::VsCodeCopilot,
        ];
        const NOT_CLAUDE: &[Source] =
            &[Source::OpenCode, Source::CopilotCli, Source::VsCodeCopilot];
        const COPILOT: &[Source] = &[Source::CopilotCli, Source::VsCodeCopilot];
        match self {
            UserClaude | ProjectClaude => ALL,
            UserAgents | ProjectAgents => NOT_CLAUDE,
            UserCopilot | ProjectGithub => COPILOT,
            UserOpenCode | ProjectOpenCode => &[Source::OpenCode],
            Plugin => &[Source::ClaudeCode],
        }
    }
}

/// A directory that holds skills (`<dir>/<skill>/SKILL.md`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Location {
    pub kind: LocationKind,
    pub dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillEntry {
    pub name: String,
    pub description: Option<String>,
    /// The `SKILL.md` file (symlinks resolved).
    pub file: PathBuf,
    /// Every location that leads to this file.
    pub locations: Vec<Location>,
    /// Clients that load the skill from at least one of its locations.
    pub clients: Vec<Source>,
    /// Name and description, sent with every request of those clients.
    pub baseline_tokens: Tokens,
    /// The text below the frontmatter, added to the context on load.
    pub body_tokens: Tokens,
    pub body_bytes: u64,
}

/// Different files that declare the same skill name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Duplicate {
    pub name: String,
    pub files: Vec<PathBuf>,
    /// All copies have the same content.
    pub identical: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Inventory {
    pub skills: Vec<SkillEntry>,
    pub duplicates: Vec<Duplicate>,
    pub warnings: Vec<Warning>,
}

impl Inventory {
    pub fn find(&self, name: &str) -> Vec<&SkillEntry> {
        self.skills.iter().filter(|s| s.name == name).collect()
    }
}

impl Location {
    /// The project a project-level location belongs to (`<project>/.claude/skills` -> `<project>`).
    pub fn project_root(&self) -> Option<&Path> {
        use LocationKind::*;
        match self.kind {
            ProjectClaude | ProjectAgents | ProjectGithub | ProjectOpenCode => {
                self.dir.parent()?.parent()
            }
            _ => None,
        }
    }
}

/// Skill directories of the user and of the given projects.
pub fn locations(env: &Env, projects: &[PathBuf]) -> Vec<Location> {
    let home = env.home();
    let mut out = vec![
        Location {
            kind: LocationKind::UserClaude,
            dir: claude_dir(env).join("skills"),
        },
        Location {
            kind: LocationKind::UserAgents,
            dir: home.join(".agents/skills"),
        },
        Location {
            kind: LocationKind::UserCopilot,
            dir: copilot_dir(env).join("skills"),
        },
        Location {
            kind: LocationKind::UserOpenCode,
            dir: config_home(env).join("opencode/skills"),
        },
    ];
    for project in projects {
        for (kind, rel) in [
            (LocationKind::ProjectClaude, ".claude/skills"),
            (LocationKind::ProjectAgents, ".agents/skills"),
            (LocationKind::ProjectGithub, ".github/skills"),
            (LocationKind::ProjectOpenCode, ".opencode/skills"),
        ] {
            out.push(Location {
                kind,
                dir: project.join(rel),
            });
        }
    }
    out.extend(plugin_locations(env));
    out
}

fn claude_dir(env: &Env) -> PathBuf {
    env.var("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| env.home().join(".claude"))
}

fn copilot_dir(env: &Env) -> PathBuf {
    env.var("COPILOT_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| env.home().join(".copilot"))
}

fn config_home(env: &Env) -> PathBuf {
    env.var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| env.home().join(".config"))
}

/// Skills of installed Claude Code plugins (`plugins/installed_plugins.json`).
fn plugin_locations(env: &Env) -> Vec<Location> {
    let file = claude_dir(env).join("plugins/installed_plugins.json");
    let Ok(text) = fs::read_to_string(file) else {
        return Vec::new();
    };
    let Ok(json) = serde_json::from_str::<Value>(&text) else {
        return Vec::new();
    };
    let paths: BTreeSet<PathBuf> = json
        .get("plugins")
        .and_then(Value::as_object)
        .into_iter()
        .flat_map(|m| m.values())
        .filter_map(Value::as_array)
        .flatten()
        .filter_map(|i| i.get("installPath").and_then(Value::as_str))
        .map(|p| PathBuf::from(p).join("skills"))
        .collect();
    paths
        .into_iter()
        .map(|dir| Location {
            kind: LocationKind::Plugin,
            dir,
        })
        .collect()
}

/// Scans the locations and estimates the token cost of every skill found.
pub fn scan(locations: &[Location], estimator: &dyn TokenEstimator) -> Inventory {
    let mut inv = Inventory::default();
    // canonical SKILL.md -> (parsed skill, locations)
    let mut found: BTreeMap<PathBuf, (Parsed, Vec<Location>)> = BTreeMap::new();
    let mut hashes: BTreeMap<PathBuf, u64> = BTreeMap::new();
    for loc in locations {
        let Ok(rd) = fs::read_dir(&loc.dir) else {
            continue;
        };
        let mut dirs: Vec<PathBuf> = rd.filter_map(Result::ok).map(|e| e.path()).collect();
        dirs.sort();
        for dir in dirs {
            let file = dir.join("SKILL.md");
            let Ok(canonical) = fs::canonicalize(&file) else {
                continue;
            };
            if let Some((_, locs)) = found.get_mut(&canonical) {
                if !locs.contains(loc) {
                    locs.push(loc.clone());
                }
                continue;
            }
            match fs::read_to_string(&canonical) {
                Ok(text) => {
                    let fallback = dir
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    let parsed = parse(&text, &fallback, &mut inv.warnings, &file);
                    hashes.insert(canonical.clone(), parsed.hash);
                    found.insert(canonical, (parsed, vec![loc.clone()]));
                }
                Err(e) => inv
                    .warnings
                    .push(Warning::new(format!("cannot read: {e}")).at(file.display().to_string())),
            }
        }
    }
    for (file, (p, mut locs)) in found {
        locs.sort_by(|a, b| a.dir.cmp(&b.dir));
        let clients: BTreeSet<Source> = locs
            .iter()
            .flat_map(|l| l.kind.clients().iter().copied())
            .collect();
        let listing = match &p.description {
            Some(d) => format!("- {}: {}", p.name, d),
            None => format!("- {}", p.name),
        };
        inv.skills.push(SkillEntry {
            name: p.name,
            description: p.description,
            file,
            locations: locs,
            clients: clients.into_iter().collect(),
            baseline_tokens: estimator.count(&listing),
            body_tokens: estimator.count(&p.body),
            body_bytes: p.body.len() as u64,
        });
    }
    inv.skills
        .sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.file.cmp(&b.file)));
    inv.duplicates = duplicates(&inv.skills, &hashes);
    inv
}

struct Parsed {
    name: String,
    description: Option<String>,
    body: String,
    hash: u64,
}

fn hash_of(content: &[u8]) -> u64 {
    let mut h = DefaultHasher::new();
    content.hash(&mut h);
    h.finish()
}

fn duplicates(skills: &[SkillEntry], hashes: &BTreeMap<PathBuf, u64>) -> Vec<Duplicate> {
    let mut by_name: BTreeMap<&str, Vec<&SkillEntry>> = BTreeMap::new();
    for s in skills {
        by_name.entry(&s.name).or_default().push(s);
    }
    by_name
        .into_iter()
        .filter(|(_, v)| v.len() > 1)
        .map(|(name, v)| {
            let hs: BTreeSet<_> = v.iter().filter_map(|s| hashes.get(&s.file)).collect();
            Duplicate {
                name: name.to_owned(),
                files: v.iter().map(|s| s.file.clone()).collect(),
                identical: hs.len() <= 1,
            }
        })
        .collect()
}

/// Splits `---\nfrontmatter\n---\nbody`. Falls back to line matching when the
/// frontmatter is not valid YAML (an unquoted colon in a description is common).
fn parse(text: &str, fallback_name: &str, warnings: &mut Vec<Warning>, file: &Path) -> Parsed {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let hash = hash_of(text.as_bytes());
    let (front, body) = split_frontmatter(text);
    let (mut name, mut description) = (None, None);
    if let Some(front) = front {
        match serde_yaml_ng::from_str::<serde_yaml_ng::Value>(front) {
            Ok(v) => {
                name = v.get("name").and_then(|n| n.as_str()).map(str::to_owned);
                description = v
                    .get("description")
                    .and_then(|d| d.as_str())
                    .map(|d| d.trim().to_owned());
            }
            Err(_) => {
                warnings.push(
                    Warning::new("frontmatter is not valid YAML, read line by line")
                        .at(file.display().to_string()),
                );
                name = front
                    .lines()
                    .find_map(|l| l.strip_prefix("name:"))
                    .map(|v| v.trim().trim_matches('"').to_owned());
                description = front
                    .lines()
                    .find_map(|l| l.strip_prefix("description:"))
                    .map(|v| v.trim().to_owned());
            }
        }
    } else {
        warnings.push(Warning::new("no frontmatter").at(file.display().to_string()));
    }
    let name = name
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| fallback_name.to_owned());
    Parsed {
        name,
        description: description.filter(|d| !d.is_empty()),
        body: body.to_owned(),
        hash,
    }
}

fn split_frontmatter(text: &str) -> (Option<&str>, &str) {
    let Some(rest) = text
        .strip_prefix("---\n")
        .or_else(|| text.strip_prefix("---\r\n"))
    else {
        return (None, text);
    };
    let mut offset = 0;
    for line in rest.split_inclusive('\n') {
        if line.trim_end() == "---" {
            return (Some(&rest[..offset]), &rest[offset + line.len()..]);
        }
        offset += line.len();
    }
    (None, text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontmatter_is_split_from_body() {
        let (f, b) = split_frontmatter("---\nname: a\n---\n# Body\n");
        assert_eq!(f, Some("name: a\n"));
        assert_eq!(b, "# Body\n");
        assert_eq!(split_frontmatter("no front matter").0, None);
        assert_eq!(split_frontmatter("---\nnever closed\n").0, None);
    }

    #[test]
    fn folded_descriptions_are_read_as_one_text() {
        let mut w = Vec::new();
        let p = parse(
            "---\nname: x\ndescription: >\n  one\n  two\n---\nbody",
            "dir",
            &mut w,
            Path::new("f"),
        );
        assert_eq!(
            (p.name.as_str(), p.description.as_deref()),
            ("x", Some("one two"))
        );
        assert!(w.is_empty());
    }

    #[test]
    fn invalid_yaml_falls_back_to_line_matching_with_a_warning() {
        let mut w = Vec::new();
        let p = parse(
            "---\nname: x\ndescription: use: when needed\n---\nbody",
            "dir",
            &mut w,
            Path::new("f"),
        );
        assert_eq!(p.description.as_deref(), Some("use: when needed"));
        assert_eq!(w.len(), 1);
    }

    #[test]
    fn missing_name_falls_back_to_directory_name() {
        let mut w = Vec::new();
        let p = parse(
            "---\ndescription: d\n---\nb",
            "dirname",
            &mut w,
            Path::new("f"),
        );
        assert_eq!(p.name, "dirname");
    }
}
