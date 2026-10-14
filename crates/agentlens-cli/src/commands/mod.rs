mod advise;
mod compare;
mod context;
mod cost;
mod counter;
mod efficiency;
mod flow;
mod import_otel;
mod mcp_setup;
mod report;
mod scan;
mod skills;
mod unused;

use agentlens_core::adapters::registry_with_otel;
use agentlens_core::env::Env;
use anyhow::{Context, Result};

use crate::args::{Cli, Command};

pub fn run(cli: Cli) -> Result<()> {
    let env =
        Env::from_system().context("cannot determine the home directory (HOME or USERPROFILE)")?;
    let registry = registry_with_otel(cli.otel.clone());
    let filter = cli.filter()?;
    let display = crate::output::Display {
        format: cli.format,
        rounded: cli.rounded,
    };
    let counter = counter::Counter::new(&env, cli.exact, cli.exact_model.as_deref())?;
    let est = counter.estimator();
    let result = match cli.command {
        Command::ImportOtel { file } => import_otel::run(&env, &file, display),
        Command::McpSetup {
            clients,
            write,
            remove,
            binary,
            yes,
        } => mcp_setup::run(
            &env,
            mcp_setup::Params {
                clients: &clients,
                write,
                remove,
                binary: binary.as_deref(),
                yes,
            },
        ),
        Command::Scan => scan::run(&registry, &env, &filter, &cli.source, display),
        Command::Skills => skills::run(&registry, &env, &filter, &cli.source, est, display),
        Command::Advise => advise::run(&registry, &env, &filter, &cli.source, est, display),
        Command::Compare => compare::run(&registry, &env, &filter, &cli.source, est, display),
        Command::Context { kind } => {
            context::run(&registry, &env, &filter, &cli.source, est, kind, display)
        }
        Command::Cost {
            by,
            prices,
            what_if,
        } => cost::run(
            &registry,
            &env,
            &filter,
            &cli.source,
            cost::Params {
                price_file: prices.as_deref(),
                by,
                what_if: what_if.as_deref(),
            },
            display,
        ),
        Command::Efficiency { by, top, prices } => efficiency::run(
            &registry,
            &env,
            &filter,
            &cli.source,
            efficiency::Params {
                price_file: prices.as_deref(),
                by,
                top,
            },
            display,
        ),
        Command::Flow { section, top } => {
            flow::run(&registry, &env, &filter, &cli.source, section, top, display)
        }
        Command::Unused { days, all } => {
            let mut filter = filter;
            if cli.since.is_none() {
                // The period ends at --until, or now.
                let end = filter.until.unwrap_or_else(jiff::Timestamp::now);
                filter.since = Some(crate::args::days_ago(days, end)?);
            }
            unused::run(&registry, &env, &filter, &cli.source, est, all, display)
        }
        Command::Report { html } => report::run(&registry, &env, &filter, &cli.source, est, &html),
    };
    counter.finish();
    result
}
