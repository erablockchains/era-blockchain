// node/src/cli.rs

use clap::{Parser, Subcommand as ClapSubcommand};
use sc_chain_spec::{GenericChainSpec, NoExtension};
use sc_cli::{
    BuildSpecCmd, ChainInfoCmd, CheckBlockCmd, ExportBlocksCmd, ExportStateCmd, ImportBlocksCmd,
    KeySubcommand, PurgeChainCmd, RevertCmd, RunCmd, SubstrateCli,
};
use std::path::PathBuf;
use std::str::FromStr;

/// ERA node CLI.
#[derive(Debug, Parser)]
pub struct Cli {
    /// Subcommand to run (if any).
    #[command(subcommand)]
    pub subcommand: Option<Subcommand>,

    /// Common run options (shared with the default `run` command).
    #[command(flatten)]
    pub run: RunCmd,
}

#[derive(Debug, ClapSubcommand)]
pub enum Subcommand {
    /// Key management CLI utilities.
    #[command(subcommand)]
    Key(KeySubcommand),

    /// Build a chain specification.
    BuildSpec(BuildSpecCmd),

    /// Validate blocks.
    CheckBlock(CheckBlockCmd),

    /// Export blocks.
    ExportBlocks(ExportBlocksCmd),

    /// Export the state of a given block into a chain spec.
    ExportState(ExportStateCmd),

    /// Import blocks.
    ImportBlocks(ImportBlocksCmd),

    /// Remove the whole chain.
    PurgeChain(PurgeChainCmd),

    /// Revert the chain to a previous state.
    Revert(RevertCmd),

    /// Sub-commands concerned with benchmarking.
    #[cfg(feature = "runtime-benchmarks")]
    #[command(subcommand)]
    Benchmark(Box<frame_benchmarking_cli::BenchmarkCmd>),

    /// Db meta columns information.
    ChainInfo(ChainInfoCmd),
}

/// Map chain identifiers to chain specs.
/// Ensures `--dev` / `development` use our in-code GenesisBuilder JSON,
/// not any embedded/stale JSON spec.
pub fn load_spec(id: &str) -> Result<Box<dyn sc_service::ChainSpec>, String> {
    Ok(match id {
        "" | "dev" | "development" => Box::new(crate::chain_spec::development_config()),
        "era-prod" | "prod" | "live" => Box::new(crate::chain_spec::era_prod_config()),
        // Allow loading an external JSON by path if explicitly provided.
        path => Box::new(GenericChainSpec::<NoExtension>::from_json_file(
            PathBuf::from_str(path).map_err(|e| e.to_string())?,
        )?),
    })
}

impl SubstrateCli for Cli {
    fn impl_name() -> String {
        "era-node".into()
    }

    fn impl_version() -> String {
        env!("CARGO_PKG_VERSION").into()
    }

    fn description() -> String {
        "ERA Node".into()
    }

    fn author() -> String {
        "ERA Team".into()
    }

    fn support_url() -> String {
        "https://example.com/era".into()
    }

    fn copyright_start_year() -> i32 {
        2024
    }

    /// IMPORTANT: this is what the runner calls to obtain the chain spec.
    fn load_spec(&self, id: &str) -> Result<Box<dyn sc_service::ChainSpec>, String> {
        load_spec(id)
    }
}
