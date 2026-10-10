mod report;
mod scan;
mod skills;

use agentlens_core::adapters::default_registry;
use agentlens_core::env::Env;
use anyhow::{Context, Result};

use crate::args::{Cli, Command};

pub fn run(cli: Cli) -> Result<()> {
    let env =
        Env::from_system().context("cannot determine the home directory (HOME or USERPROFILE)")?;
    let registry = default_registry();
    let filter = cli.filter()?;
    match cli.command {
        Command::Scan => scan::run(&registry, &env, &filter, &cli.source, cli.format),
        Command::Skills => skills::run(&registry, &env, &filter, &cli.source, cli.format),
        Command::Report { html } => report::run(&registry, &env, &filter, &cli.source, &html),
    }
}
