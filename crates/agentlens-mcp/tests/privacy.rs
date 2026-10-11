//! What the tools return is counts, names and paths, never what a person or an agent wrote: no prompt,
//! no response, no tool argument or result, no file content. A session full of marker texts goes through
//! every tool and none of the markers may come out.

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

use serde_json::{Value, json};

const MARKERS: [&str; 6] = [
    "CANARY-PROMPT-61c4",
    "CANARY-ANSWER-61c4",
    "CANARY-TOOL-ARG-61c4",
    "CANARY-TOOL-RESULT-61c4",
    "CANARY-INSTRUCTIONS-61c4",
    "CANARY-SKILL-BODY-61c4",
];

fn session() -> String {
    let line = |v: Value| format!("{v}\n");
    let base = |ts: &str, kind: &str| json!({"sessionId":"canary","cwd":"/work/canary","gitBranch":"main","timestamp":ts,"type":kind});
    let mut out = String::new();
    let mut user = base("2026-10-01T10:00:00Z", "user");
    user["message"] = json!({"role":"user","content":MARKERS[0]});
    out += &line(user);
    let mut att = base("2026-10-01T10:00:01Z", "attachment");
    att["attachment"] = json!({"type":"instructions","files":[
        {"path":"/work/canary/CLAUDE.md","type":"Project","content":MARKERS[4]}]});
    out += &line(att);
    let mut a1 = base("2026-10-01T10:00:02Z", "assistant");
    a1["requestId"] = json!("r1");
    a1["message"] = json!({"id":"m1","role":"assistant","model":"claude-test-1",
        "content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":MARKERS[2]}}],
        "usage":{"input_tokens":10,"output_tokens":5,"cache_read_input_tokens":100,"cache_creation_input_tokens":200}});
    out += &line(a1);
    let mut a2 = base("2026-10-01T10:00:03Z", "assistant");
    a2["requestId"] = json!("r1");
    a2["message"] = json!({"id":"m1","role":"assistant","model":"claude-test-1",
        "content":[{"type":"text","text":MARKERS[1]}],
        "usage":{"input_tokens":10,"output_tokens":20,"cache_read_input_tokens":100,"cache_creation_input_tokens":200}});
    out += &line(a2);
    let mut res = base("2026-10-01T10:00:04Z", "user");
    res["message"] = json!({"role":"user","content":[
        {"type":"tool_result","tool_use_id":"t1","content":MARKERS[3],"is_error":true}]});
    out += &line(res);
    out
}

#[test]
fn no_tool_returns_what_was_written_in_a_session() {
    let dir = tempfile::tempdir().unwrap();
    let projects = dir.path().join(".claude/projects/-work-canary");
    fs::create_dir_all(&projects).unwrap();
    fs::write(projects.join("canary.jsonl"), session()).unwrap();
    let skill = dir.path().join(".claude/skills/canary-skill");
    fs::create_dir_all(&skill).unwrap();
    fs::write(
        skill.join("SKILL.md"),
        format!(
            "---\nname: canary-skill\ndescription: A skill\n---\n{}",
            MARKERS[5]
        ),
    )
    .unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_agentlens-mcp"))
        .env("HOME", dir.path())
        .env("USERPROFILE", dir.path())
        .env_remove("XDG_DATA_HOME")
        .env_remove("XDG_CONFIG_HOME")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut messages = vec![
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
        "protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"0"}}}),
    ];
    messages.push(json!({"jsonrpc":"2.0","method":"notifications/initialized"}));
    let tools = [
        "sources",
        "skills",
        "context",
        "unused",
        "flow",
        "cost",
        "efficiency",
        "compare",
        "sessions",
    ];
    for (i, tool) in tools.iter().enumerate() {
        messages.push(json!({"jsonrpc":"2.0","id":10 + i,"method":"tools/call",
            "params":{"name":tool,"arguments":{}}}));
    }
    messages.push(json!({"jsonrpc":"2.0","id":99,"method":"tools/call",
        "params":{"name":"timeline","arguments":{"source":"claude-code","id":"canary"}}}));
    for m in &messages {
        writeln!(stdin, "{m}").unwrap();
    }
    stdin.flush().unwrap();

    let mut reader = BufReader::new(child.stdout.take().unwrap());
    let mut everything = String::new();
    let mut answered = 0;
    let mut line = String::new();
    while answered < tools.len() + 1 {
        line.clear();
        assert!(
            reader.read_line(&mut line).unwrap() > 0,
            "the server closed early"
        );
        let v: Value = serde_json::from_str(&line).unwrap();
        if v["id"].as_u64().is_some_and(|id| id >= 10) {
            answered += 1;
            // The session must have been read, or the test would pass for the wrong reason.
            assert_eq!(v["result"]["isError"], false, "{line}");
        }
        everything += &line;
    }
    let _ = child.kill();
    let _ = child.wait();

    assert!(
        everything.contains("canary-skill"),
        "the skill name is a name, not content"
    );
    assert!(
        everything.contains("/work/canary"),
        "paths are shown, as in the CLI"
    );
    for marker in MARKERS {
        assert!(
            !everything.contains(marker),
            "{marker} must not appear in any answer"
        );
    }
}

#[test]
fn tools_say_which_numbers_are_estimates() {
    // Some reports are plain numbers (the JSON of the CLI is the same), so a tool says in its description
    // which of them are estimates, and where a count has a `measured` flag the flag is kept.
    let dir = tempfile::tempdir().unwrap();
    let projects = dir.path().join(".claude/projects/-work-canary");
    fs::create_dir_all(&projects).unwrap();
    fs::write(projects.join("canary.jsonl"), session()).unwrap();
    let skill = dir.path().join(".claude/skills/demo");
    fs::create_dir_all(&skill).unwrap();
    fs::write(
        skill.join("SKILL.md"),
        "---\nname: demo\ndescription: Does demo things\n---\nbody",
    )
    .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_agentlens-mcp"))
        .env("HOME", dir.path())
        .env("USERPROFILE", dir.path())
        .env_remove("XDG_DATA_HOME")
        .env_remove("XDG_CONFIG_HOME")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    for m in [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
            "protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"0"}}}),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}),
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"skills","arguments":{}}}),
    ] {
        writeln!(stdin, "{m}").unwrap();
    }
    stdin.flush().unwrap();
    let mut reader = BufReader::new(child.stdout.take().unwrap());
    let (mut list, mut skills) = (Value::Null, Value::Null);
    let mut line = String::new();
    while list.is_null() || skills.is_null() {
        line.clear();
        assert!(reader.read_line(&mut line).unwrap() > 0);
        let v: Value = serde_json::from_str(&line).unwrap();
        match v["id"].as_u64() {
            Some(2) => list = v,
            Some(3) => skills = v,
            _ => {}
        }
    }
    let _ = child.kill();
    let _ = child.wait();

    let description = |name: &str| -> String {
        list["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["name"] == name)
            .unwrap()["description"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    for tool in ["skills", "context", "unused", "compare"] {
        assert!(
            description(tool).contains("estimate"),
            "{tool} must say what is estimated"
        );
    }
    assert!(description("cost").contains("measured_usd"));
    let text = skills["result"]["structuredContent"].to_string();
    assert!(
        text.contains("\"measured\""),
        "the flags of the report stay: {text}"
    );
}
