mod context;
mod cost;
mod flow;
mod report;
mod scan;
mod skills;
mod unused;

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
        Command::Context { kind } => {
            context::run(&registry, &env, &filter, &cli.source, kind, cli.format)
        }
        Command::Cost { by, prices } => cost::run(
            &registry,
            &env,
            &filter,
            &cli.source,
            prices.as_deref(),
            by,
            cli.format,
        ),
        Command::Flow { section, top } => flow::run(
            &registry,
            &env,
            &filter,
            &cli.source,
            section,
            top,
            cli.format,
        ),
        Command::Unused { days, all } => {
            let mut filter = filter;
            if cli.since.is_none() {
                filter.since = Some(crate::args::days_ago(days, jiff::Timestamp::now())?);
            }
            unused::run(&registry, &env, &filter, &cli.source, all, cli.format)
        }
        Command::Report { html } => report::run(&registry, &env, &filter, &cli.source, &html),
    }
}
