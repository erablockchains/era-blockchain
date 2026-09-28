//! Executive hook delegation and bounded migration decoding. No storage or dispatch API.
use crate::*;
use codec::{Decode, DecodeAll};
use frame_support::{
    ensure,
    traits::{
        BeforeAllRuntimeMigrations, OffchainWorker, OnFinalize, OnIdle, OnInitialize, OnPoll,
        OnRuntimeUpgrade, PalletInfoAccess, StorageVersion,
    },
    weights::{Weight, WeightMeter},
};
use sp_runtime::{DispatchError, DispatchResult};

/// Read at most N bytes, rejecting missing length, truncation, trailing data and oversized
/// encodings before typed storage can substitute a default. None means genuine key absence.
pub(crate) fn raw_value<T: Decode, const N: usize>(key: &[u8]) -> Result<Option<T>, DispatchError> {
    let mut bytes = [0u8; N];
    match sp_io::storage::read(key, &mut bytes, 0) {
        None => Ok(None),
        Some(len) if len as usize <= N => T::decode_all(&mut &bytes[..len as usize])
            .map(Some)
            .map_err(|_| DispatchError::Other("malformed migration storage")),
        Some(_) => Err(DispatchError::Other("oversized migration storage")),
    }
}

pub(crate) fn version<P: PalletInfoAccess>() -> Result<Option<StorageVersion>, DispatchError> {
    raw_value::<StorageVersion, 2>(&StorageVersion::storage_key::<P>())
}

pub(crate) fn required_version<P: PalletInfoAccess>() -> Result<StorageVersion, DispatchError> {
    version::<P>()?.ok_or(DispatchError::Other("migration storage version absent"))
}

/// At most two successor probes, one exact-prefix existence read and one two-byte version read.
/// Even explicit zero-valued accounting is existing state, not an initialization request.
pub(crate) fn pristine_security_budget() -> DispatchResult {
    let version = version::<SecurityBudget>()?;
    ensure!(
        version.is_none() || version == Some(StorageVersion::new(0)),
        DispatchError::Other("unsupported pending SecurityBudget version")
    );
    pristine_namespace::<SecurityBudget>(version.is_some())
}

/// Shared bounded namespace check for supported absent/predecessor initialization only.
/// The caller must first strictly decode and approve the version (or genuine absence).
pub(crate) fn pristine_namespace<P: PalletInfoAccess>(has_version: bool) -> DispatchResult {
    let prefix = P::name_hash();
    let version_key = StorageVersion::storage_key::<P>();
    ensure!(
        !sp_io::storage::exists(&prefix),
        DispatchError::Other("dirty migration namespace prefix")
    );
    if let Some(key) = sp_io::storage::next_key(&prefix) {
        if key.starts_with(&prefix) {
            ensure!(
                key == version_key && has_version,
                DispatchError::Other("partial migration namespace state")
            );
            ensure!(
                !matches!(sp_io::storage::next_key(&key), Some(next) if next.starts_with(&prefix)),
                DispatchError::Other("partial migration namespace state")
            );
        }
    }
    Ok(())
}

/// Reuse the measured observer raw-prefix/strict-version envelopes: one prefix+successor
/// rejection and one version+successor rejection cover our two bounded successor probes.
/// This adds no fitted constants and includes both their database and proof-size allowances.
pub(crate) fn pristine_weight() -> Weight {
    use era_validator_security::observation::WeightInfo;
    type W = era_validator_security::weights::SubstrateWeight<Runtime>;
    W::migration_reject_dirty(1).saturating_add(W::migration_reject_dirty(1))
}

/// Only generated *version initialization* is omitted for the coordinated economic pallets
/// and observer. Their normal hooks still run, in the unchanged AllPalletsWithSystem order.
/// Genesis remains construct_runtime's original OnGenesis implementation.
pub struct MigrationLifecycle;

type AutomaticallyInitialized = (
    System,
    Timestamp,
    Balances,
    TransactionPayment,
    Sudo,
    Session,
    Historical,
    Babe,
    Grandpa,
    Authorship,
    Vesting,
    AiPredictions,
    Multisig,
    Proxy,
    Assets,
    EraWorlds,
);

impl BeforeAllRuntimeMigrations for MigrationLifecycle {
    fn before_all_runtime_migrations() -> Weight {
        // Validate NFT evidence before ANY generated version initializer can write.
        let nft_version = pallet_nfts::migration::v2::validate::<Runtime, ()>()
            .expect("NFT pre-initialization schema validation rejected");
        let weight = AutomaticallyInitialized::before_all_runtime_migrations();
        if nft_version.is_none() {
            // A proven pristine namespace needs the predecessor expected by pending V13.
            // The owning NFT hook installs v2 after the custom migration tuple.
            StorageVersion::new(1).put::<Nfts>();
        }
        weight.saturating_add(pallet_nfts::weights::measured_schema(
            <Runtime as frame_system::Config>::DbWeight::get(),
        ))
    }
}
impl OnRuntimeUpgrade for MigrationLifecycle {
    fn on_runtime_upgrade() -> Weight {
        AllPalletsWithSystem::on_runtime_upgrade()
    }
    #[cfg(feature = "try-runtime")]
    fn try_on_runtime_upgrade(checks: bool) -> Result<Weight, sp_runtime::TryRuntimeError> {
        pallet_nfts::migration::v2::validate::<Runtime, ()>()?;
        AllPalletsWithSystem::try_on_runtime_upgrade(checks)
    }
}
impl OnInitialize<BlockNumber> for MigrationLifecycle {
    fn on_initialize(n: BlockNumber) -> Weight {
        AllPalletsWithSystem::on_initialize(n)
    }
}
impl OnFinalize<BlockNumber> for MigrationLifecycle {
    fn on_finalize(n: BlockNumber) {
        AllPalletsWithSystem::on_finalize(n)
    }
}
impl OnIdle<BlockNumber> for MigrationLifecycle {
    fn on_idle(n: BlockNumber, remaining: Weight) -> Weight {
        AllPalletsWithSystem::on_idle(n, remaining)
    }
}
impl OnPoll<BlockNumber> for MigrationLifecycle {
    fn on_poll(n: BlockNumber, weight: &mut WeightMeter) {
        AllPalletsWithSystem::on_poll(n, weight)
    }
}
impl OffchainWorker<BlockNumber> for MigrationLifecycle {
    fn offchain_worker(n: BlockNumber) {
        AllPalletsWithSystem::offchain_worker(n)
    }
}
#[cfg(feature = "try-runtime")]
impl frame_support::traits::TryState<BlockNumber> for MigrationLifecycle {
    fn try_state(
        n: BlockNumber,
        select: frame_support::traits::TryStateSelect,
    ) -> Result<(), sp_runtime::TryRuntimeError> {
        <AllPalletsWithSystem as frame_support::traits::TryState<BlockNumber>>::try_state(n, select)
    }
}
#[cfg(feature = "try-runtime")]
impl frame_support::traits::TryDecodeEntireStorage for MigrationLifecycle {
    fn try_decode_entire_state(
    ) -> Result<usize, Vec<frame_support::traits::TryDecodeEntireStorageError>> {
        <AllPalletsWithSystem as frame_support::traits::TryDecodeEntireStorage>::try_decode_entire_state()
    }
}

/// FRAME's try-runtime tuple collects errors and continues later hooks. Enclose the entire
/// diagnostic lifecycle (including before-all and LastRuntimeUpgrade) so an Err never commits
/// later initialization. Production traps still rely on the enclosing block overlay as before.
#[cfg(feature = "try-runtime")]
pub struct TryRuntimeExecutive;
#[cfg(feature = "try-runtime")]
impl TryRuntimeExecutive {
    pub fn try_runtime_upgrade(
        checks: frame_try_runtime::UpgradeCheckSelect,
    ) -> Result<Weight, sp_runtime::TryRuntimeError> {
        frame_support::storage::with_transaction(|| {
            let result = pallet_nfts::migration::v2::validate::<Runtime, ()>()
                .and_then(|_| crate::InnerTryRuntimeExecutive::try_runtime_upgrade(checks));
            if result.is_ok() {
                frame_support::storage::TransactionOutcome::Commit(result)
            } else {
                frame_support::storage::TransactionOutcome::Rollback(result)
            }
        })
    }
}
