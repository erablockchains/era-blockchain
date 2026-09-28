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
    /// Deterministic fresh V14 raw spec; requires explicit public bindings or synthetic mode.
    FreshSpec(FreshSpecCmd),
    /// Execute synthetic genesis blocks in the built Wasm, without network or database access.
    FreshCheck { #[arg(long, default_value_t=602)] blocks: u32 },
    /// Encode a reward-claim transaction using only the hard-coded disposable community fixture.
    FreshClaim {
        #[arg(long)] genesis: sp_core::H256,
        #[arg(long)] validator: era_runtime::AccountId,
    },
    /// Run candidate first-block initialization in Wasm over a verified public snapshot.
    UpgradeCheck { #[arg(long)] snapshot: PathBuf, #[arg(long)] wasm: PathBuf, #[arg(long)] genesis: sp_core::H256 },
    /// Bounded offline signed Wasm scenario; fixed public synthetic identities only.
    OfflineScenario { #[arg(long)] input: PathBuf, #[arg(long)] output: PathBuf },
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
///
/// The built-in aliases are development-only. Production operation must use an independently
/// reviewed external JSON chain specification; Live-labelled deterministic development keys are
/// never synthesized by the node.
pub fn load_spec(id: &str) -> Result<Box<dyn sc_service::ChainSpec>, String> {
    Ok(match id {
        "" => {
            return Err(
                "no default chain specification is selected; pass --dev, --chain dev, or an independently reviewed external JSON chain-spec path"
                    .into(),
            );
        }
        "dev" | "development" => Box::new(crate::chain_spec::development_config()),
        "era-prod" | "prod" | "live" | "mainnet" => {
            return Err(
                "built-in production aliases are disabled; pass an independently reviewed external JSON chain-spec path"
                    .into(),
            );
        }
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

#[cfg(test)]
mod tests {
    use super::load_spec;

    #[test]
    fn only_explicit_development_aliases_select_the_in_code_spec() {
        for alias in ["dev", "development"] {
            let spec = load_spec(alias).expect("explicit development alias");
            assert_eq!(spec.id(), "era-dev");
            assert_eq!(spec.chain_type(), sc_service::ChainType::Development);
        }
    }

    #[test]
    fn production_aliases_fail_closed_and_require_external_json() {
        for alias in ["", "era-prod", "prod", "live", "mainnet"] {
            let error = match load_spec(alias) {
                Ok(_) => panic!("{alias} unexpectedly created an in-code chain specification"),
                Err(error) => error,
            };
            assert!(error.contains("external JSON chain-spec path"));
        }
    }
}

#[derive(Debug, clap::Args)]
pub struct FreshSpecCmd {
    #[arg(long, conflicts_with="synthetic", required_unless_present="synthetic")]
    pub inputs: Option<PathBuf>,
    #[arg(long)]
    pub synthetic: bool,
    /// Print the full allocation table instead of raw chain storage.
    #[arg(long)]
    pub allocations: bool,
}
