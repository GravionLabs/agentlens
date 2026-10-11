//! What the tools return is counts, names and paths, never what a person or an agent wrote: no prompt,
//! no response, no tool argument or result, no file content. A session full of marker texts goes through
//! every tool and none of the markers may come out.

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

use serde_json::{Value, json};

const MARKERS: [&str; 7] = [
    "CANARY-PROMPT-61c4",
    "CANARY-ANSWER-61c4",
    "CANARY-TOOL-ARG-61c4",
    "CANARY-TOOL-RESULT-61c4",
    "CANARY-INSTRUCTIONS-61c4",
    "CANARY-SKILL-BODY-61c4",
    "CANARY-SCRIPT-61c4",
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
    // A script run from the folder of an installed skill: its name may show, the rest of the call not.
    let mut a3 = base("2026-10-01T10:00:03.500Z", "assistant");
    a3["requestId"] = json!("r1");
    a3["message"] = json!({"id":"m1","role":"assistant","model":"claude-test-1",
        "content":[{"type":"tool_use","id":"t2","name":"Bash",
            "input":{"command":format!("bash ~/.claude/skills/canary-skill/scripts/{}.sh", MARKERS[6])}}],
        "usage":{"input_tokens":10,"output_tokens":20,"cache_read_input_tokens":100,"cache_creation_input_tokens":200}});
    out += &line(a3);
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
fn every_token_count_that_can_be_an_estimate_carries_the_flag() {
    // The reports mark an estimate with a `measured` flag next to the count, so an agent does not
    // have to know by convention which numbers are guesses.
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
    let tools = ["skills", "context", "unused", "compare"];
    let mut messages = vec![
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
            "protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"0"}}}),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
    ];
    for (i, tool) in tools.iter().enumerate() {
        messages.push(json!({"jsonrpc":"2.0","id":10 + i,"method":"tools/call",
            "params":{"name":tool,"arguments":{}}}));
    }
    for m in &messages {
        writeln!(stdin, "{m}").unwrap();
    }
    stdin.flush().unwrap();
    let mut reader = BufReader::new(child.stdout.take().unwrap());
    let mut results = std::collections::BTreeMap::new();
    let mut line = String::new();
    while results.len() < tools.len() {
        line.clear();
        assert!(reader.read_line(&mut line).unwrap() > 0);
        let v: Value = serde_json::from_str(&line).unwrap();
        if let Some(id) = v["id"].as_u64().filter(|id| *id >= 10) {
            results.insert(
                tools[(id - 10) as usize],
                v["result"]["structuredContent"]["data"].clone(),
            );
        }
    }
    let _ = child.kill();
    let _ = child.wait();

    let flag = |t: &Value| t["measured"].is_boolean() && t["count"].is_u64();
    // Skills: the load, the baseline and the carry.
    let skills = results["skills"]["report"]["skills"].as_array().unwrap();
    assert!(!skills.is_empty());
    for s in skills {
        assert!(
            flag(&s["load"]) && flag(&s["baseline"]["tokens"]) && flag(&s["carry"]["tokens"]),
            "{s}"
        );
        assert_eq!(s["carry"]["tokens"]["measured"], false, "carry is derived");
    }
    // Context: every row's size and overhead.
    let rows = results["context"]["rows"].as_array().unwrap();
    assert!(!rows.is_empty());
    for r in rows {
        assert!(flag(&r["tokens"]) && flag(&r["overhead_tokens"]), "{r}");
    }
    // Unused: the idle cost, where it is known.
    let items = results["unused"]["items"].as_array().unwrap();
    assert!(
        items.iter().any(|i| flag(&i["overhead_tokens"])),
        "{items:?}"
    );
    // Compare: the skill overhead, where there is one.
    let sources = results["compare"]["sources"].as_array().unwrap();
    for s in sources
        .iter()
        .filter(|s| !s["skill_overhead_tokens"].is_null())
    {
        assert!(flag(&s["skill_overhead_tokens"]), "{s}");
    }
}
