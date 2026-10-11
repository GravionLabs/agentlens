//! Registering the MCP server (`agentlens-mcp`) in the clients' configuration.
//!
//! This is the only place where AgentLens writes into files of the clients, so it is careful: nothing
//! is written unless the caller asks for it, a file that exists is backed up before it changes, the
//! entry is merged into the file instead of replacing it, a file that cannot be read as JSON is left
//! untouched, and everything can be undone with [`unregister`]. Paths come from an [`Env`], so tests
//! work on a temporary home.
//!
//! Claude Code keeps its settings in a state file it rewrites constantly; it is registered with its own
//! command (`claude mcp add`) instead, which the caller runs.

use std::fmt;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

use crate::adapters::claude_code::claude_home;
use crate::adapters::copilot_cli::copilot_home;
use crate::adapters::vscode_copilot::user_dirs;
use crate::env::Env;
use crate::inventory::config_home;

/// The name the server is registered under.
pub const SERVER_NAME: &str = "agentlens";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Client {
    ClaudeCode,
    VsCode,
    CopilotCli,
    OpenCode,
}

impl Client {
    pub const ALL: [Client; 4] = [
        Client::ClaudeCode,
        Client::VsCode,
        Client::CopilotCli,
        Client::OpenCode,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Client::ClaudeCode => "claude-code",
            Client::VsCode => "vscode",
            Client::CopilotCli => "copilot-cli",
            Client::OpenCode => "opencode",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Client::ClaudeCode => "Claude Code",
            Client::VsCode => "VS Code",
            Client::CopilotCli => "Copilot CLI",
            Client::OpenCode => "OpenCode",
        }
    }

    pub fn from_id(id: &str) -> Option<Client> {
        Client::ALL.into_iter().find(|c| c.id() == id)
    }

    /// The top-level key of the configuration file that holds the servers.
    fn section(self) -> Option<&'static str> {
        match self {
            Client::ClaudeCode => None,
            Client::VsCode => Some("servers"),
            Client::CopilotCli => Some("mcpServers"),
            Client::OpenCode => Some("mcp"),
        }
    }
}

/// A client on this machine, and the file its server entry goes into.
#[derive(Debug, Clone)]
pub struct Target {
    pub client: Client,
    /// Whether the client's directory exists (the client is probably installed).
    pub detected: bool,
    /// The configuration file; none for Claude Code, which is registered with a command.
    pub file: Option<PathBuf>,
}

/// One target per client, in the order of [`Client::ALL`]. A file that does not exist yet is created
/// when the entry is registered.
pub fn targets(env: &Env) -> Vec<Target> {
    Client::ALL
        .into_iter()
        .map(|client| match client {
            Client::ClaudeCode => Target {
                client,
                detected: claude_home(env).is_dir(),
                file: None,
            },
            Client::VsCode => {
                let dirs = user_dirs(env);
                let file = dirs
                    .first()
                    .cloned()
                    .unwrap_or_else(|| config_home(env).join("Code/User"))
                    .join("mcp.json");
                Target {
                    client,
                    detected: !dirs.is_empty(),
                    file: Some(file),
                }
            }
            Client::CopilotCli => Target {
                client,
                detected: copilot_home(env).is_dir(),
                file: Some(copilot_home(env).join("mcp-config.json")),
            },
            Client::OpenCode => {
                let dir = config_home(env).join("opencode");
                let file =
                    if dir.join("opencode.json").exists() || !dir.join("opencode.jsonc").exists() {
                        dir.join("opencode.json")
                    } else {
                        dir.join("opencode.jsonc")
                    };
                Target {
                    client,
                    detected: dir.is_dir() || env.data_home().join("opencode").is_dir(),
                    file: Some(file),
                }
            }
        })
        .collect()
}

/// The entry of the server in the client's format.
pub fn entry(client: Client, binary: &Path) -> Option<Value> {
    let command = binary.display().to_string();
    match client {
        Client::ClaudeCode => None,
        Client::VsCode => Some(json!({ "type": "stdio", "command": command })),
        Client::CopilotCli => Some(json!({
            "type": "local", "command": command, "args": [], "tools": ["*"]
        })),
        Client::OpenCode => Some(json!({
            "type": "local", "command": [command], "enabled": true
        })),
    }
}

/// What to show for the client: the JSON to merge into its file, or the command to run.
pub fn snippet(client: Client, binary: &Path) -> String {
    match (client.section(), entry(client, binary)) {
        (Some(section), Some(entry)) => {
            let doc = json!({ section: { SERVER_NAME: entry } });
            serde_json::to_string_pretty(&doc).unwrap_or_default()
        }
        _ => claude_command(binary, false).join(" "),
    }
}

/// The command that registers (or, with `remove`, unregisters) the server in Claude Code.
pub fn claude_command(binary: &Path, remove: bool) -> Vec<String> {
    let mut command = vec!["claude".to_owned(), "mcp".to_owned()];
    if remove {
        command.extend(["remove", "--scope", "user", SERVER_NAME].map(str::to_owned));
    } else {
        command.extend(["add", "--scope", "user", SERVER_NAME, "--"].map(str::to_owned));
        command.push(binary.display().to_string());
    }
    command
}

/// What a registration or removal did to the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    Added,
    /// The entry existed with other values and was replaced.
    Updated,
    /// The file already holds exactly this entry; nothing was written.
    Unchanged,
    Removed,
    /// There was no entry to remove; nothing was written.
    NotPresent,
}

#[derive(Debug)]
pub struct Applied {
    pub change: Change,
    pub file: PathBuf,
    /// The copy of the file made before it changed, if it existed.
    pub backup: Option<PathBuf>,
}

#[derive(Debug)]
pub enum SetupError {
    /// The file is not JSON, or not the shape the client documents. It is left untouched.
    Malformed {
        file: PathBuf,
        why: String,
    },
    Io {
        file: PathBuf,
        why: String,
    },
    /// Claude Code has no file; use [`claude_command`].
    NoFile(Client),
}

impl fmt::Display for SetupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SetupError::Malformed { file, why } => write!(
                f,
                "{} is not a configuration this tool can edit ({why}); it was left untouched. Add the entry by hand.",
                file.display()
            ),
            SetupError::Io { file, why } => write!(f, "cannot write {}: {why}", file.display()),
            SetupError::NoFile(c) => write!(f, "{} is registered with its own command", c.name()),
        }
    }
}

impl std::error::Error for SetupError {}

fn read(file: &Path) -> Result<Option<Map<String, Value>>, SetupError> {
    let text = match std::fs::read_to_string(file) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(SetupError::Io {
                file: file.into(),
                why: e.to_string(),
            });
        }
    };
    if text.trim().is_empty() {
        return Ok(Some(Map::new()));
    }
    match serde_json::from_str::<Value>(&text) {
        Ok(Value::Object(map)) => Ok(Some(map)),
        Ok(_) => Err(SetupError::Malformed {
            file: file.into(),
            why: "the top level is not an object".into(),
        }),
        Err(e) => Err(SetupError::Malformed {
            file: file.into(),
            why: e.to_string(),
        }),
    }
}

fn backup(file: &Path) -> Result<PathBuf, SetupError> {
    let stamp = jiff::Timestamp::now()
        .strftime("%Y%m%dT%H%M%SZ")
        .to_string();
    let mut name = file.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".agentlens-backup-{stamp}"));
    let copy = file.with_file_name(name);
    std::fs::copy(file, &copy).map_err(|e| SetupError::Io {
        file: copy.clone(),
        why: e.to_string(),
    })?;
    Ok(copy)
}

fn write(file: &Path, root: Map<String, Value>) -> Result<(), SetupError> {
    let io = |e: std::io::Error| SetupError::Io {
        file: file.into(),
        why: e.to_string(),
    };
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir).map_err(io)?;
    }
    let mut text =
        serde_json::to_string_pretty(&Value::Object(root)).map_err(|e| SetupError::Io {
            file: file.into(),
            why: e.to_string(),
        })?;
    text.push('\n');
    // Written next to the file and moved over it, so a reader never sees half a file.
    let mut tmp_name = file.file_name().unwrap_or_default().to_os_string();
    tmp_name.push(".agentlens-tmp");
    let tmp = file.with_file_name(tmp_name);
    std::fs::write(&tmp, text).map_err(io)?;
    std::fs::rename(&tmp, file).map_err(io)
}

/// Adds the entry of the server to the client's file, keeping everything else in it.
pub fn register(target: &Target, binary: &Path) -> Result<Applied, SetupError> {
    let (Some(file), Some(section), Some(entry)) = (
        target.file.as_ref(),
        target.client.section(),
        entry(target.client, binary),
    ) else {
        return Err(SetupError::NoFile(target.client));
    };
    let existed = read(file)?;
    let mut root = existed.clone().unwrap_or_default();
    let servers = root
        .entry(section)
        .or_insert_with(|| Value::Object(Map::new()));
    let Some(servers) = servers.as_object_mut() else {
        return Err(SetupError::Malformed {
            file: file.clone(),
            why: format!("`{section}` is not an object"),
        });
    };
    let change = match servers.get(SERVER_NAME) {
        Some(current) if *current == entry => {
            return Ok(Applied {
                change: Change::Unchanged,
                file: file.clone(),
                backup: None,
            });
        }
        Some(_) => Change::Updated,
        None => Change::Added,
    };
    servers.insert(SERVER_NAME.into(), entry);
    let backup = if existed.is_some() {
        Some(backup(file)?)
    } else {
        None
    };
    write(file, root)?;
    Ok(Applied {
        change,
        file: file.clone(),
        backup,
    })
}

/// Removes the entry of the server from the client's file, keeping everything else in it.
pub fn unregister(target: &Target) -> Result<Applied, SetupError> {
    let (Some(file), Some(section)) = (target.file.as_ref(), target.client.section()) else {
        return Err(SetupError::NoFile(target.client));
    };
    let absent = |file: &PathBuf| Applied {
        change: Change::NotPresent,
        file: file.clone(),
        backup: None,
    };
    let Some(mut root) = read(file)? else {
        return Ok(absent(file));
    };
    let removed = root
        .get_mut(section)
        .and_then(Value::as_object_mut)
        .and_then(|servers| servers.remove(SERVER_NAME));
    if removed.is_none() {
        return Ok(absent(file));
    }
    let backup = backup(file)?;
    write(file, root)?;
    Ok(Applied {
        change: Change::Removed,
        file: file.clone(),
        backup: Some(backup),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn bin() -> PathBuf {
        PathBuf::from("/opt/agentlens/agentlens-mcp")
    }

    fn target(env: &Env, client: Client) -> Target {
        targets(env)
            .into_iter()
            .find(|t| t.client == client)
            .unwrap()
    }

    fn read_json(file: &Path) -> Value {
        serde_json::from_str(&fs::read_to_string(file).unwrap()).unwrap()
    }

    #[test]
    fn finds_the_files_of_the_clients_and_whether_they_are_installed() {
        let dir = tempfile::tempdir().unwrap();
        let env = Env::for_home(dir.path());
        assert!(targets(&env).iter().all(|t| !t.detected));
        fs::create_dir_all(dir.path().join(".copilot")).unwrap();
        fs::create_dir_all(dir.path().join(".config/Code/User")).unwrap();
        fs::create_dir_all(dir.path().join(".claude")).unwrap();
        let copilot = target(&env, Client::CopilotCli);
        assert!(copilot.detected);
        assert_eq!(
            copilot.file.unwrap(),
            dir.path().join(".copilot/mcp-config.json")
        );
        let vscode = target(&env, Client::VsCode);
        assert!(vscode.detected);
        assert_eq!(
            vscode.file.unwrap(),
            dir.path().join(".config/Code/User/mcp.json")
        );
        assert!(target(&env, Client::ClaudeCode).detected);
        assert!(target(&env, Client::ClaudeCode).file.is_none());
        assert!(!target(&env, Client::OpenCode).detected);
    }

    #[test]
    fn registers_in_a_new_file_without_a_backup() {
        let dir = tempfile::tempdir().unwrap();
        let env = Env::for_home(dir.path());
        let t = target(&env, Client::CopilotCli);
        let done = register(&t, &bin()).unwrap();
        assert_eq!(done.change, Change::Added);
        assert!(done.backup.is_none());
        let doc = read_json(&done.file);
        assert_eq!(
            doc["mcpServers"]["agentlens"]["command"],
            "/opt/agentlens/agentlens-mcp"
        );
        assert_eq!(doc["mcpServers"]["agentlens"]["type"], "local");
    }

    #[test]
    fn merges_into_an_existing_file_and_backs_it_up() {
        let dir = tempfile::tempdir().unwrap();
        let env = Env::for_home(dir.path());
        let t = target(&env, Client::VsCode);
        let file = t.file.clone().unwrap();
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(
            &file,
            r#"{"servers":{"other":{"type":"http","url":"https://example.com"}},"inputs":[1]}"#,
        )
        .unwrap();
        let done = register(&t, &bin()).unwrap();
        assert_eq!(done.change, Change::Added);
        let doc = read_json(&file);
        assert_eq!(
            doc["servers"]["other"]["url"], "https://example.com",
            "others stay"
        );
        assert_eq!(doc["inputs"], json!([1]), "other keys stay");
        assert_eq!(doc["servers"]["agentlens"]["type"], "stdio");
        let backup = done.backup.unwrap();
        assert!(fs::read_to_string(backup).unwrap().contains("\"other\""));
        assert!(!file.with_file_name("mcp.json.agentlens-tmp").exists());
    }

    #[test]
    fn keeps_the_order_of_the_keys_of_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let env = Env::for_home(dir.path());
        let t = target(&env, Client::CopilotCli);
        let file = t.file.clone().unwrap();
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(
            &file,
            r#"{"zebra":1,"mcpServers":{"zed":{},"alpha":{}},"apple":2}"#,
        )
        .unwrap();
        register(&t, &bin()).unwrap();
        let doc = read_json(&file);
        let top: Vec<_> = doc.as_object().unwrap().keys().collect();
        assert_eq!(top, ["zebra", "mcpServers", "apple"]);
        let servers: Vec<_> = doc["mcpServers"].as_object().unwrap().keys().collect();
        assert_eq!(servers, ["zed", "alpha", "agentlens"]);
    }

    #[test]
    fn registering_twice_changes_nothing_the_second_time() {
        let dir = tempfile::tempdir().unwrap();
        let env = Env::for_home(dir.path());
        let t = target(&env, Client::OpenCode);
        register(&t, &bin()).unwrap();
        let before = fs::read_to_string(t.file.as_ref().unwrap()).unwrap();
        let again = register(&t, &bin()).unwrap();
        assert_eq!(again.change, Change::Unchanged);
        assert!(again.backup.is_none());
        assert_eq!(
            fs::read_to_string(t.file.as_ref().unwrap()).unwrap(),
            before
        );
        let doc = read_json(t.file.as_ref().unwrap());
        assert_eq!(
            doc["mcp"]["agentlens"]["command"],
            json!(["/opt/agentlens/agentlens-mcp"])
        );
        // Another binary path replaces the entry.
        let moved = register(&t, Path::new("/usr/local/bin/agentlens-mcp")).unwrap();
        assert_eq!(moved.change, Change::Updated);
        assert!(moved.backup.is_some());
    }

    #[test]
    fn a_file_it_cannot_read_is_left_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let env = Env::for_home(dir.path());
        let t = target(&env, Client::VsCode);
        let file = t.file.clone().unwrap();
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        // VS Code allows comments in mcp.json; they are not JSON.
        let text = "{\n  // my servers\n  \"servers\": {}\n}\n";
        fs::write(&file, text).unwrap();
        let err = register(&t, &bin()).unwrap_err();
        assert!(matches!(err, SetupError::Malformed { .. }), "{err}");
        assert!(err.to_string().contains("left untouched"));
        assert_eq!(fs::read_to_string(&file).unwrap(), text);
        assert!(matches!(unregister(&t), Err(SetupError::Malformed { .. })));

        fs::write(&file, "[1,2]").unwrap();
        assert!(matches!(
            register(&t, &bin()),
            Err(SetupError::Malformed { .. })
        ));
        fs::write(&file, r#"{"servers": 3}"#).unwrap();
        assert!(matches!(
            register(&t, &bin()),
            Err(SetupError::Malformed { .. })
        ));
        assert_eq!(fs::read_to_string(&file).unwrap(), r#"{"servers": 3}"#);
    }

    #[test]
    fn removing_takes_only_its_own_entry() {
        let dir = tempfile::tempdir().unwrap();
        let env = Env::for_home(dir.path());
        let t = target(&env, Client::CopilotCli);
        let file = t.file.clone().unwrap();
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(
            &file,
            r#"{"mcpServers":{"other":{"type":"http","url":"u","tools":["*"]}}}"#,
        )
        .unwrap();
        assert_eq!(unregister(&t).unwrap().change, Change::NotPresent);
        register(&t, &bin()).unwrap();
        let done = unregister(&t).unwrap();
        assert_eq!(done.change, Change::Removed);
        assert!(done.backup.is_some());
        let doc = read_json(&file);
        assert!(doc["mcpServers"].get("agentlens").is_none());
        assert_eq!(doc["mcpServers"]["other"]["url"], "u");
        // No file at all: nothing to remove.
        let none = target(
            &Env::for_home(tempfile::tempdir().unwrap().path()),
            Client::OpenCode,
        );
        assert_eq!(unregister(&none).unwrap().change, Change::NotPresent);
    }

    #[test]
    fn claude_code_is_registered_with_its_own_command() {
        let dir = tempfile::tempdir().unwrap();
        let env = Env::for_home(dir.path());
        let t = target(&env, Client::ClaudeCode);
        assert!(matches!(register(&t, &bin()), Err(SetupError::NoFile(_))));
        assert_eq!(
            claude_command(&bin(), false),
            [
                "claude",
                "mcp",
                "add",
                "--scope",
                "user",
                "agentlens",
                "--",
                "/opt/agentlens/agentlens-mcp"
            ]
        );
        assert_eq!(
            claude_command(&bin(), true),
            ["claude", "mcp", "remove", "--scope", "user", "agentlens"]
        );
        assert!(snippet(Client::ClaudeCode, &bin()).starts_with("claude mcp add"));
        assert!(snippet(Client::VsCode, &bin()).contains("\"servers\""));
    }
}
