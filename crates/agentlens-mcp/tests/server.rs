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

const TOOLS: [&str; 12] = [
    "sources",
    "skills",
    "advice",
    "context",
    "unused",
    "flow",
    "cost",
    "efficiency",
    "compare",
    "trend",
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
    assert!(
        structured["schema_version"]
            .as_u64()
            .is_some_and(|v| v >= 2),
        "the result says which version of the layout it has: {structured}"
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
fn the_efficiency_tool_carries_the_roles_table() {
    let home = home();
    let mut client = Client::start(home.path());
    let result = client.call("efficiency", json!({"sources": ["claude-code"]}));
    assert_eq!(result["isError"], false, "{result}");
    let roles = result["structuredContent"]["data"]["roles"]
        .as_array()
        .expect("roles in the efficiency result");
    assert!(
        roles
            .iter()
            .any(|r| r["role"] == "main" && r["kind"] == "main"),
        "{roles:?}"
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
fn until_closes_the_period() {
    // The Claude Code session of the home ran on 2026-10-02 at 09:00.
    let home = home();
    let mut client = Client::start(home.path());
    let sessions = |client: &mut Client, args: Value| {
        let r = client.call("sessions", args);
        assert_eq!(r["isError"], false, "{r}");
        r["structuredContent"]["data"].as_array().unwrap().len()
    };
    let base = |extra: Value| {
        let mut v = json!({"sources": ["claude-code"]});
        v.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        v
    };
    assert_eq!(sessions(&mut client, base(json!({}))), 1);
    assert_eq!(
        sessions(&mut client, base(json!({"until": "2026-10-03"}))),
        1
    );
    assert_eq!(
        sessions(&mut client, base(json!({"until": "2026-10-02"}))),
        0
    );
    assert_eq!(
        sessions(
            &mut client,
            base(json!({"since": "2026-10-01", "until": "2026-10-03"}))
        ),
        1
    );
    // The cost tool takes it too: the session inside the period is counted, the one outside is not.
    let mut cost = |extra: Value| {
        let r = client.call("cost", base(extra));
        assert_eq!(r["isError"], false, "{r}");
        r["structuredContent"]["data"]["report"].clone()
    };
    let all = cost(json!({}));
    assert!(!all["rows"].as_array().unwrap().is_empty(), "{all}");
    assert_eq!(cost(json!({"until": "2026-10-03"})), all);
    let none = cost(json!({"until": "2026-10-02"}));
    assert!(none["rows"].as_array().unwrap().is_empty(), "{none}");
    assert_ne!(none["total"], all["total"]);
}

#[test]
fn a_relative_until_counts_back_from_now() {
    let home = home();
    let mut client = Client::start(home.path());
    let mut count = |args: Value| {
        let r = client.call("sessions", args);
        assert_eq!(r["isError"], false, "{r}");
        r["structuredContent"]["data"].as_array().unwrap().len()
    };
    // The session is from October 2026: an hour ago is after it, 3000 days ago long before.
    assert_eq!(
        count(json!({"sources": ["claude-code"], "since": "3000d", "until": "1h"})),
        1
    );
    assert_eq!(
        count(json!({"sources": ["claude-code"], "until": "3000d"})),
        0
    );
}

#[test]
fn until_not_after_since_is_an_error_the_agent_can_read() {
    let home = home();
    let mut client = Client::start(home.path());
    let result = client.call(
        "cost",
        json!({"since": "2026-10-02", "until": "2026-10-01"}),
    );
    assert_eq!(result["isError"], true, "{result}");
    assert!(
        result["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("until"),
        "{result}"
    );
    let result = client.call("context", json!({"until": "soon"}));
    assert_eq!(result["isError"], true, "{result}");
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

#[test]
fn the_binary_carries_the_marker_of_the_command_line_tools() {
    // The desktop application recognizes an old copy in ~/.local/bin by it (without running it).
    let bin = Path::new(env!("CARGO_BIN_EXE_agentlens-mcp"));
    assert!(agentlens_core::cli_tools::has_marker(bin));
}

#[test]
fn announces_tools_prompts_and_resources() {
    let home = home();
    let mut client = Client::start(home.path());
    // The initialize result was checked by `start`; ask again through a fresh server to read it.
    let init = client.request(
        "initialize",
        json!({
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": {"name": "test", "version": "0"}
        }),
    );
    let caps = &init["result"]["capabilities"];
    assert!(caps["tools"].is_object(), "{init}");
    assert!(caps["prompts"].is_object(), "{init}");
    assert!(caps["resources"].is_object(), "{init}");
    assert!(
        init["result"]["instructions"]
            .as_str()
            .unwrap()
            .contains("agentlens://glossary")
    );
}

#[test]
fn the_optimize_prompt_tells_an_agent_how_to_analyze_a_skill_setup() {
    let home = home();
    let mut client = Client::start(home.path());
    let list = client.request("prompts/list", json!({}));
    let prompts = list["result"]["prompts"].as_array().expect("prompts");
    let prompt = prompts
        .iter()
        .find(|p| p["name"] == "optimize-skills")
        .expect("optimize-skills");
    assert!(!prompt["description"].as_str().unwrap().is_empty());
    assert!(
        prompt["arguments"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["name"] == "since")
    );

    let got = client.request(
        "prompts/get",
        json!({"name": "optimize-skills", "arguments": {"since": "4w"}}),
    );
    let text = got["result"]["messages"][0]["content"]["text"]
        .as_str()
        .unwrap();
    assert_eq!(got["result"]["messages"][0]["role"], "user");
    assert!(text.contains("last 4w"), "{text}");
    for needle in [
        "`advice`",
        "`trend`",
        "agentlens://glossary",
        "file_uses",
        "Do not change anything",
        "ranked by what each could have saved",
        "An `inferred` saving rests on an assumption",
    ] {
        assert!(text.contains(needle), "{needle} missing in {text}");
    }
    // Without an argument it looks at the last 30 days.
    let default = client.request("prompts/get", json!({"name": "optimize-skills"}));
    assert!(
        default["result"]["messages"][0]["content"]["text"]
            .as_str()
            .unwrap()
            .contains("last 30d")
    );
    let missing = client.request("prompts/get", json!({"name": "nothing"}));
    assert!(missing.get("error").is_some(), "{missing}");
}

#[test]
fn the_glossary_is_a_resource() {
    let home = home();
    let mut client = Client::start(home.path());
    let list = client.request("resources/list", json!({}));
    let resources = list["result"]["resources"].as_array().expect("resources");
    assert_eq!(resources.len(), 1);
    assert_eq!(resources[0]["uri"], "agentlens://glossary");
    assert_eq!(resources[0]["mimeType"], "text/markdown");

    let read = client.request("resources/read", json!({"uri": "agentlens://glossary"}));
    let text = read["result"]["contents"][0]["text"]
        .as_str()
        .expect("text");
    // The terms of the analyses an agent meets in the results.
    for term in [
        "### Carry",
        "### Baseline",
        "### File use",
        "### Measured and estimated",
    ] {
        assert!(text.contains(term), "{term} missing");
    }
    assert_eq!(read["result"]["contents"][0]["uri"], "agentlens://glossary");

    let unknown = client.request("resources/read", json!({"uri": "agentlens://nothing"}));
    assert!(unknown.get("error").is_some(), "{unknown}");
}

#[test]
fn the_trend_tool_compares_two_windows_and_reads_a_bad_against() {
    // The Claude Code session of the home ran on 2026-10-02 at 09:00.
    let home = home();
    let mut client = Client::start(home.path());
    let args = |against: &str| json!({"sources": ["claude-code"], "since": "2026-10-02", "until": "2026-10-03", "against": against});
    let ok = client.call("trend", args("2026-09-01..2026-09-02"));
    assert_eq!(ok["isError"], false, "{ok}");
    let data = &ok["structuredContent"]["data"];
    assert_eq!(data["windows"]["current"]["sessions"], 1, "{ok}");
    assert_eq!(data["windows"]["against"]["sessions"], 0);
    assert!(data["volume"]["requests"]["change"].is_null());
    assert!(data["volume"]["requests"]["after"].as_f64().unwrap() > 0.0);
    // The default is the window right before; no prompt or file content is in the result.
    let default = client.call(
        "trend",
        json!({"sources": ["claude-code"], "since": "2026-10-02", "until": "2026-10-03"}),
    );
    assert_eq!(default["isError"], false, "{default}");
    assert_eq!(
        default["structuredContent"]["data"]["windows"]["against"]["since"],
        "2026-10-01T00:00:00Z"
    );
    let text = default["structuredContent"].to_string();
    let fixture = fs::read_to_string(fixtures().join("claude_code/context.jsonl")).unwrap();
    for line in fixture.lines() {
        if let Some(prompt) = serde_json::from_str::<Value>(line)
            .ok()
            .and_then(|v| v["message"]["content"].as_str().map(str::to_owned))
            .filter(|p| p.len() > 8)
        {
            assert!(!text.contains(&prompt), "{prompt}");
        }
    }
    for bad in [
        "2026-09-01",
        "x..y",
        "2026-10-01..2026-10-05",
        "2026-09-02..2026-09-01",
    ] {
        let r = client.call("trend", args(bad));
        assert_eq!(r["isError"], true, "{bad}: {r}");
        assert!(
            r["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("against")
                || r["content"][0]["text"]
                    .as_str()
                    .unwrap()
                    .contains("overlaps"),
            "{bad}: {r}"
        );
    }
}
