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
        Some(cli::Subcommand::OfflineScenario {input,output}) => {
            let result=crate::offline_scenario::run(input,output).map_err(CliError::Input)?;
            println!("{}",serde_json::to_string_pretty(&result).map_err(|e|CliError::Input(e.to_string()))?);Ok(())
        }
        Some(cli::Subcommand::UpgradeCheck {snapshot,wasm,genesis}) => {
            let result=crate::upgrade_check::run(snapshot,wasm,*genesis).map_err(CliError::Input)?;
            println!("{}",serde_json::to_string_pretty(&result).map_err(|e|CliError::Input(e.to_string()))?);Ok(())
        }
        Some(cli::Subcommand::FreshCheck { blocks }) => {
            let result=crate::fresh_check::run(*blocks).map_err(CliError::Input)?;
            println!("{}",serde_json::to_string_pretty(&result).map_err(|e|CliError::Input(e.to_string()))?);
            Ok(())
        }
        Some(cli::Subcommand::FreshClaim { genesis, validator }) => {
            println!("{}",crate::fresh_check::synthetic_claim(*genesis,validator.clone()).map_err(CliError::Input)?);
            Ok(())
        }
        Some(cli::Subcommand::FreshSpec(cmd)) => {
            let inputs = if cmd.synthetic {
                era_runtime::fresh_genesis_inputs::Inputs::synthetic()
            } else {
                let bytes = std::fs::read(cmd.inputs.as_ref().expect("clap requires public input path")).map_err(CliError::from)?;
                serde_json::from_slice(&bytes).map_err(|e|CliError::Input(e.to_string()))?
            };
            if cmd.allocations {
                let rows=inputs.allocations().map_err(CliError::Input)?;
                println!("{}",serde_json::to_string_pretty(&rows).map_err(|e|CliError::Input(e.to_string()))?);
            } else {
                let spec=crate::chain_spec::fresh_spec(&inputs).map_err(CliError::Input)?;
                println!("{}",sc_service::ChainSpec::as_json(&spec,true).map_err(CliError::Input)?);
            }
            Ok(())
        }

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

        Some(cli::Subcommand::CheckBlock(cmd)) => {
            let runner=app.create_runner(cmd)?;
            runner.async_run(|config| {
                let p=service::new_partial(&config).map_err(|e| *e)?;
                service::verify_maintenance_genesis(&config,&p.client).map_err(|e| *e)?;
                let queue=service::maintenance_import_queue(&config,p.client.clone(),p.select_chain,p.transaction_pool,&p.task_manager).map_err(|e| *e)?;
                Ok((cmd.run(p.client,queue),p.task_manager))
            })
        }
        Some(cli::Subcommand::ImportBlocks(cmd)) => {
            let runner=app.create_runner(cmd)?;
            runner.async_run(|config| {
                let p=service::new_partial(&config).map_err(|e| *e)?;
                service::verify_maintenance_genesis(&config,&p.client).map_err(|e| *e)?;
                let queue=service::maintenance_import_queue(&config,p.client.clone(),p.select_chain,p.transaction_pool,&p.task_manager).map_err(|e| *e)?;
                Ok((cmd.run(p.client,queue),p.task_manager))
            })
        }
        Some(cli::Subcommand::ExportBlocks(cmd)) => {
            let runner=app.create_runner(cmd)?;
            runner.async_run(|config| {
                let p=service::new_partial(&config).map_err(|e| *e)?;
                service::verify_maintenance_genesis(&config,&p.client).map_err(|e| *e)?;
                Ok((cmd.run(p.client,config.database),p.task_manager))
            })
        }
        Some(cli::Subcommand::ExportState(cmd)) => {
            let runner=app.create_runner(cmd)?;
            runner.async_run(|config| {
                let p=service::new_partial(&config).map_err(|e| *e)?;
                service::verify_maintenance_genesis(&config,&p.client).map_err(|e| *e)?;
                Ok((cmd.run(p.client,config.chain_spec),p.task_manager))
            })
        }
        Some(cli::Subcommand::Revert(cmd)) => {
            let runner=app.create_runner(cmd)?;
            runner.async_run(|config| {
                let p=service::new_partial(&config).map_err(|e| *e)?;
                service::verify_maintenance_genesis(&config,&p.client).map_err(|e| *e)?;
                let aux=Box::new(|client: std::sync::Arc<service::FullClient>,backend,blocks| {
                    sc_consensus_babe::revert(client.clone(),backend,blocks)?;
                    sc_consensus_grandpa::revert(client,blocks)?;
                    Ok(())
                });
                Ok((cmd.run(p.client,p.backend,Some(aux)),p.task_manager))
            })
        }

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
