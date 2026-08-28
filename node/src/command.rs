// node/src/command.rs

use crate::{cli, service};
use clap::Parser;
use era_runtime::opaque::Block as RuntimeBlock; // <-- concrete Block type
#[cfg(feature = "runtime-benchmarks")]
use frame_benchmarking_cli::BenchmarkCmd;
use sc_cli::{Error as CliError, SubstrateCli};
use sc_service::Configuration;
#[cfg(feature = "runtime-benchmarks")]
use sp_runtime::traits::HashingFor;

pub fn run() -> Result<(), Box<CliError>> {
    let app = cli::Cli::parse();

    let result: sc_cli::Result<()> = match &app.subcommand {
        // ---- supported subcommands ----
        Some(cli::Subcommand::Key(cmd)) => cmd.run(&app),

        Some(cli::Subcommand::BuildSpec(cmd)) => {
            let runner = app.create_runner(cmd)?;
            runner.sync_run(|config| cmd.run(config.chain_spec, config.network))
        }

        Some(cli::Subcommand::ChainInfo(cmd)) => {
            let runner = app.create_runner(cmd)?;
            // ChainInfoCmd::run<B>(&Configuration) needs a concrete Block type.
            runner.sync_run(|config: Configuration| cmd.run::<RuntimeBlock>(&config))
        }

        Some(cli::Subcommand::PurgeChain(cmd)) => {
            let runner = app.create_runner(cmd)?;
            runner.sync_run(|config| cmd.run(config.database))
        }

        // ---- not wired yet (needs import-queue/client plumbing) ----
        Some(cli::Subcommand::CheckBlock(_))
        | Some(cli::Subcommand::ExportBlocks(_))
        | Some(cli::Subcommand::ExportState(_))
        | Some(cli::Subcommand::ImportBlocks(_))
        | Some(cli::Subcommand::Revert(_)) => Err(CliError::Input(
            "Subcommand not wired in this node build (no import-queue/client). Use: build-spec / purge-chain / run / key / chain-info."
                .into(),
        )),

        #[cfg(feature = "runtime-benchmarks")]
        Some(cli::Subcommand::Benchmark(cmd)) => {
            let runner = app.create_runner(cmd.as_ref())?;
            runner.sync_run(|config| match cmd.as_ref() {
                BenchmarkCmd::Pallet(pallet_cmd) => {
                    let chain_spec = pallet_cmd.runtime.is_none().then_some(config.chain_spec);
                    pallet_cmd.run_with_spec::<HashingFor<RuntimeBlock>, ()>(chain_spec)
                },
                _ => Err(CliError::Input("Only pallet benchmarking is wired in this node build".into())),
            })
        }

        // ---- default: run node ----
        None => {
            let runner = app.create_runner(&app.run)?;
            runner.run_node_until_exit(|config: Configuration| async move {
                let (task_manager, _client) =
                    service::new_full(config).map_err(|error| *error)?;
                Ok(task_manager)
            })
        }
    };

    result.map_err(Box::new)
}
