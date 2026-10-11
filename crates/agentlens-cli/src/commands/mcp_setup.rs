use std::io::{BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use agentlens_core::env::Env;
use agentlens_core::mcp_setup::{self, Change, Client, Target};
use anyhow::{Result, bail};

use crate::args::McpClient;

const BINARY: &str = if cfg!(windows) {
    "agentlens-mcp.exe"
} else {
    "agentlens-mcp"
};
const PLACEHOLDER: &str = "/path/to/agentlens-mcp";

pub struct Params<'a> {
    pub clients: &'a [McpClient],
    pub write: bool,
    pub remove: bool,
    pub binary: Option<&'a Path>,
    pub yes: bool,
}

fn client(c: McpClient) -> Client {
    match c {
        McpClient::ClaudeCode => Client::ClaudeCode,
        McpClient::Vscode => Client::VsCode,
        McpClient::CopilotCli => Client::CopilotCli,
        McpClient::Opencode => Client::OpenCode,
    }
}

/// `--binary`, else `agentlens-mcp` next to this program, else the first one on `PATH`.
fn find_binary(given: Option<&Path>) -> Result<Option<PathBuf>> {
    if let Some(path) = given {
        if !path.is_file() {
            bail!("{} is not a file", path.display());
        }
        return Ok(Some(std::path::absolute(path)?));
    }
    if let Some(next_to) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(BINARY)))
        .filter(|p| p.is_file())
    {
        return Ok(Some(next_to));
    }
    Ok(std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|dir| dir.join(BINARY))
            .find(|p| p.is_file())
    }))
}

fn confirm(question: &str) -> Result<bool> {
    if !std::io::stdin().is_terminal() {
        bail!("`{question}` needs a confirmation: run this in a terminal, or pass --yes");
    }
    print!("{question} [y/N] ");
    std::io::stdout().flush()?;
    let mut answer = String::new();
    std::io::stdin().lock().read_line(&mut answer)?;
    Ok(matches!(answer.trim().to_lowercase().as_str(), "y" | "yes"))
}

/// Shows, for each client, what to add.
fn show(targets: &[Target], binary: &Path, found: bool) {
    for t in targets {
        println!(
            "{} ({})",
            t.client.name(),
            if t.detected {
                "found on this machine"
            } else {
                "not found"
            }
        );
        match &t.file {
            Some(file) => println!("Add to {}:\n", file.display()),
            None => println!("Run:\n"),
        }
        for line in mcp_setup::snippet(t.client, binary).lines() {
            println!("  {line}");
        }
        println!();
    }
    if !found {
        println!(
            "agentlens-mcp was not found next to agentlens or on PATH; replace {PLACEHOLDER} with its full path, or pass --binary."
        );
    }
    println!(
        "Nothing was changed. `agentlens mcp-setup --write` adds the server for you (a backup is made first)."
    );
}

/// Runs `claude mcp add` or `claude mcp remove`.
fn run_claude(binary: &Path, remove: bool) -> Result<String> {
    let command = mcp_setup::claude_command(binary, remove);
    let output = Command::new(&command[0]).args(&command[1..]).output();
    match output {
        Ok(o) if o.status.success() => Ok(format!(
            "{}: {} (`{}`)",
            Client::ClaudeCode.name(),
            if remove { "removed" } else { "registered" },
            command.join(" ")
        )),
        Ok(o) => bail!(
            "{}: `{}` failed: {}",
            Client::ClaudeCode.name(),
            command.join(" "),
            String::from_utf8_lossy(&o.stderr).trim()
        ),
        Err(e) => bail!(
            "{}: cannot run `claude` ({e}); run it yourself: {}",
            Client::ClaudeCode.name(),
            command.join(" ")
        ),
    }
}

fn apply(t: &Target, binary: Option<&Path>, remove: bool) -> Result<String> {
    if t.client == Client::ClaudeCode {
        return run_claude(binary.unwrap_or_else(|| Path::new(PLACEHOLDER)), remove);
    }
    let done = if remove {
        mcp_setup::unregister(t)
    } else {
        mcp_setup::register(t, binary.expect("checked"))
    }
    .map_err(|e| anyhow::anyhow!("{}: {e}", t.client.name()))?;
    let what = match done.change {
        Change::Added => "added",
        Change::Updated => "updated",
        Change::Unchanged => "already registered, nothing changed",
        Change::Removed => "removed",
        Change::NotPresent => "was not registered, nothing changed",
    };
    let mut line = format!("{}: {what} in {}", t.client.name(), done.file.display());
    if let Some(backup) = done.backup {
        line.push_str(&format!(" (backup: {})", backup.display()));
    }
    Ok(line)
}

pub fn run(env: &Env, p: Params) -> Result<()> {
    let all = mcp_setup::targets(env);
    let chosen: Vec<Target> = if p.clients.is_empty() {
        all.into_iter()
            .filter(|t| !(p.write || p.remove) || t.detected)
            .collect()
    } else {
        let wanted: Vec<Client> = p.clients.iter().copied().map(client).collect();
        all.into_iter()
            .filter(|t| wanted.contains(&t.client))
            .collect()
    };
    let binary = find_binary(p.binary)?;

    if !(p.write || p.remove) {
        show(
            &chosen,
            binary.as_deref().unwrap_or(Path::new(PLACEHOLDER)),
            binary.is_some(),
        );
        return Ok(());
    }
    if chosen.is_empty() {
        println!("No supported client was found on this machine; choose one with --client.");
        return Ok(());
    }
    if p.write && binary.is_none() {
        bail!("agentlens-mcp was not found next to agentlens or on PATH; pass --binary FILE");
    }

    let verb = if p.remove { "Remove" } else { "Register" };
    let mut failed = 0;
    for t in &chosen {
        let place = t
            .file
            .as_ref()
            .map(|f| f.display().to_string())
            .unwrap_or_else(|| "with `claude mcp`".into());
        let question = format!("{verb} agentlens-mcp for {} ({place})?", t.client.name());
        if !p.yes && !confirm(&question)? {
            println!("{}: skipped", t.client.name());
            continue;
        }
        match apply(t, binary.as_deref(), p.remove) {
            Ok(line) => println!("{line}"),
            Err(e) => {
                eprintln!("warning: {e}");
                failed += 1;
            }
        }
    }
    if failed > 0 {
        bail!("{failed} client(s) could not be changed");
    }
    Ok(())
}
