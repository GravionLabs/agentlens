mod args;
mod commands;
mod output;

use clap::Parser;

fn main() -> anyhow::Result<()> {
    let cli = args::Cli::parse();
    commands::run(cli)
}
