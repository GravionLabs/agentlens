mod args;
mod commands;
mod output;

use clap::Parser;

fn main() -> anyhow::Result<()> {
    // Lets the desktop application recognize an old copy of this binary in ~/.local/bin.
    agentlens_core::cli_tools::embed_marker();
    let cli = args::Cli::parse();
    commands::run(cli)
}
