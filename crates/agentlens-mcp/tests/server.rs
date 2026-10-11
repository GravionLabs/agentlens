//! Talks to the real binary over stdio, the way an MCP client does: a home with fixture sessions,
//! JSON-RPC messages as lines on stdin, the answers as lines on stdout.

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{Value, json};
use tempfile::TempDir;

fn fixtures() -> &'static Path {
    Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../agentlens-core/tests/fixtures"
    ))
}

/// A home with a Claude Code session, a Copilot CLI session and one installed skill.
fn home() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    let projects = dir.path().join(".claude/projects/-work-demo");
    fs::create_dir_all(&projects).unwrap();
    fs::copy(
        fixtures().join("claude_code/context.jsonl"),
        projects.join("context.jsonl"),
    )
    .unwrap();
    let copilot = dir.path().join(".copilot/session-state/cp-1");
    fs::create_dir_all(&copilot).unwrap();
    fs::copy(
        fixtures().join("copilot_cli/with_store.jsonl"),
        copilot.join("events.jsonl"),
    )
    .unwrap();
    let skill = dir.path().join(".claude/skills/demo");
    fs::create_dir_all(&skill).unwrap();
    fs::write(
        skill.join("SKILL.md"),
        "---\nname: demo\ndescription: Does demo things\n---\nbody",
    )
    .unwrap();
    dir
}

/// The running server and the lines it writes.
struct Client {
    child: Child,
    stdin: ChildStdin,
    lines: mpsc::Receiver<String>,
    next_id: u64,
}

impl Client {
    fn start(home: &Path) -> Client {
        let mut child = Command::new(env!("CARGO_BIN_EXE_agentlens-mcp"))
            .env("HOME", home)
            .env("USERPROFILE", home)
            .env_remove("XDG_DATA_HOME")
            .env_remove("XDG_CONFIG_HOME")
            .env_remove("XDG_CACHE_HOME")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let stdout: ChildStdout = child.stdout.take().unwrap();
        let (tx, lines) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        let mut client = Client {
            child,
            stdin,
            lines,
            next_id: 1,
        };
        let init = client.request(
            "initialize",
            json!({
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "test", "version": "0"}
            }),
        );
        assert!(init.get("result").is_some(), "initialize: {init}");
        client.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
        client
    }

    fn send(&mut self, message: &Value) {
        writeln!(self.stdin, "{message}").unwrap();
        self.stdin.flush().unwrap();
    }

    /// Sends a request and returns the response with the same id.
    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        self.send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        loop {
            let line = self
                .lines
                .recv_timeout(Duration::from_secs(30))
                .expect("the server did not answer");
            let value: Value = serde_json::from_str(&line).expect("a JSON-RPC line");
            if value.get("id") == Some(&json!(id)) {
                return value;
            }
        }
    }

    fn call(&mut self, tool: &str, arguments: Value) -> Value {
        let response = self.request("tools/call", json!({"name": tool, "arguments": arguments}));
        response["result"].clone()
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

const TOOLS: [&str; 11] = [
    "sources",
    "skills",
    "advice",
    "context",
    "unused",
    "flow",
    "cost",
    "efficiency",
    "compare",
    "sessions",
    "timeline",
];

#[test]
fn lists_one_read_only_tool_per_analysis() {
    let home = home();
    let mut client = Client::start(home.path());
    let list = client.request("tools/list", json!({}));
    let tools = list["result"]["tools"].as_array().expect("tools");
    let mut names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    names.sort_unstable();
    let mut expected = TOOLS.to_vec();
    expected.sort_unstable();
    assert_eq!(names, expected);
    for tool in tools {
        assert_eq!(
            tool["annotations"]["readOnlyHint"], true,
            "{} must be read-only",
            tool["name"]
        );
        assert_eq!(
            tool["annotations"]["openWorldHint"], false,
            "{}",
            tool["name"]
        );
        assert!(!tool["description"].as_str().unwrap().is_empty());
    }
}

#[test]
fn a_tool_returns_what_the_application_shows() {
    let home = home();
    let mut client = Client::start(home.path());
    let result = client.call("skills", json!({"sources": ["claude-code"]}));
    assert_eq!(result["isError"], false, "{result}");
    let structured = &result["structuredContent"];
    assert!(
        structured["data"]["report"]["skills"].is_array(),
        "{structured}"
    );
    assert!(structured["warnings"].is_u64());
    assert!(structured["errors"].is_array());
    // The same JSON as the text content, for clients that only read text.
    let text: Value = serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(&text, structured);

    let sources = client.call("sources", json!({}));
    let list = sources["structuredContent"]["data"].as_array().unwrap();
    assert!(
        list.iter()
            .any(|s| s["source"] == "claude-code" && s["found"] == true)
    );
}

#[test]
fn a_bad_filter_is_an_error_the_agent_can_read() {
    let home = home();
    let mut client = Client::start(home.path());
    let result = client.call("context", json!({"since": "soon"}));
    assert_eq!(result["isError"], true, "{result}");
    assert!(
        result["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("cannot parse"),
        "{result}"
    );
    let result = client.call("cost", json!({"by": "weather"}));
    assert_eq!(result["isError"], true);
}

#[test]
fn every_tool_answers_on_fixture_sessions() {
    let home = home();
    let mut client = Client::start(home.path());
    for tool in TOOLS.iter().filter(|t| **t != "timeline") {
        let result = client.call(tool, json!({"sources": ["claude-code"]}));
        assert_eq!(result["isError"], false, "{tool}: {result}");
        let data = &result["structuredContent"]["data"];
        assert!(data.is_object() || data.is_array(), "{tool}: {data}");
    }
    // A timeline of a session listed by the sessions tool.
    let sessions = client.call("sessions", json!({"sources": ["claude-code"]}));
    let first = &sessions["structuredContent"]["data"][0];
    let timeline = client.call(
        "timeline",
        json!({"sources": ["claude-code"], "source": first["source"], "id": first["id"]}),
    );
    assert_eq!(timeline["isError"], false, "{timeline}");
}

#[test]
fn the_binary_answers_version_and_help() {
    for flag in ["--version", "--help"] {
        let out = Command::new(env!("CARGO_BIN_EXE_agentlens-mcp"))
            .arg(flag)
            .output()
            .unwrap();
        assert!(out.status.success());
        assert!(String::from_utf8_lossy(&out.stdout).contains("agentlens-mcp"));
    }
}
