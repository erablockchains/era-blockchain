#![cfg_attr(not(feature = "std"), no_std)]
#![recursion_limit = "256"]

#[cfg(feature = "std")]
include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));

extern crate alloc;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod v14_completion_tests;
#[cfg(test)]
mod v14_ai_lifecycle_tests;
#[cfg(test)]
mod v14_ai_evaluation_tests;
#[cfg(test)]
mod v14_security_weight_tests;

/// Persistent shared ETKN issuance allowance and its only production minting interface.
pub mod issuance_cap;
pub mod fresh_genesis;
#[cfg(feature = "std")]
pub mod fresh_genesis_inputs;

/// Decision-independent Upgrade 13 policy constants and fail-closed helpers.
///
/// This module remains pure; executable allowance enforcement lives in [`issuance_cap`].
pub mod upgrade13_policy;

/// Ordered, transactional V13 retained-supply and issuance-cap migration.
pub mod v13_migration;

/// Ordered, dormant V13-to-V14 economic-state migration.
pub mod v14_migration;
mod v14_migration_lifecycle;

/// Bounded installation-time restoration of strict vesting locks.
pub mod ws3_vesting;

/// Historical pinned-SDK custody derivation used only by sealed synthetic rehearsal tests.
pub mod ws3_custody;

/// Internal, non-dispatchable R2 boundaries. All activation inputs remain fail-closed.
mod v14_integration;

#[cfg(test)]
mod v14_amm_tests;
/// Frozen V1 native asset adapter, exposed through the explicit managed allocator/API.
mod v14_assets;
mod v14_allocator;
mod v14_allocator_weights;
#[cfg(test)] mod v14_allocator_tests;
pub mod v14_amm;
pub mod v14_penalties;
mod v14_nft_weights;
mod v14_amm_weights;
mod v14_penalty_weights;

use alloc::{borrow::Cow, vec::Vec};

use crate::sp_api_hidden_includes_construct_runtime::hidden_include::genesis_builder_helper::{
    build_state, get_preset,
};

use frame_support::{
    construct_runtime, parameter_types,
    traits::{
        fungible::{Balanced, Credit},
        ConstBool, ConstU128, ConstU32, Contains, Imbalance, InstanceFilter,
        OnUnbalanced, WithdrawReasons,
    },
    weights::{constants::RocksDbWeight, ConstantMultiplier, IdentityFee, Weight},
};
use frame_system as system;

use frame_election_provider_support::{
    bounds::{ElectionBounds, ElectionBoundsBuilder},
    onchain, SequentialPhragmen,
};

use sp_api::impl_runtime_apis;
use sp_core::{OpaqueMetadata, H256};
use sp_runtime::{
    generic, impl_opaque_keys,
    traits::{AccountIdLookup, BlakeTwo256, Block as BlockT, IdentifyAccount, Verify},
    transaction_validity::{TransactionSource, TransactionValidity},
    ApplyExtrinsicResult, MultiSignature, Perbill,
};
use sp_version::RuntimeVersion;

// ===== Base types =====
pub type BlockNumber = u32;
pub type AccountPublic = <Signature as Verify>::Signer;
pub type Signature = MultiSignature;
pub type AccountId = <<Signature as Verify>::Signer as IdentifyAccount>::AccountId;
pub type Index = u32;
pub type Hash = H256;
pub type Balance = u128;
pub type Moment = u64;

// ===== Monetary units =====
pub const DECIMALS: Balance = 1_000_000_000_000_000_000; // 10^18 (nakamishi)
pub const EXISTENTIAL_DEPOSIT: Balance = DECIMALS / 10_000; // 0.0001 ERA

// ===== Time =====
pub const MILLISECS_PER_BLOCK: u64 = 6_000;
pub const SLOT_DURATION: u64 = MILLISECS_PER_BLOCK;

// ===== Extrinsic & block (runtime) =====
pub type Address = sp_runtime::MultiAddress<AccountId, ()>;

pub type SignedExtra = (
    frame_system::CheckNonZeroSender<Runtime>,
    frame_system::CheckSpecVersion<Runtime>,
    frame_system::CheckTxVersion<Runtime>,
    frame_system::CheckGenesis<Runtime>,
    frame_system::CheckMortality<Runtime>,
    frame_system::CheckNonce<Runtime>,
    frame_system::CheckWeight<Runtime>,
    pallet_transaction_payment::ChargeTransactionPayment<Runtime>,
);

pub type UncheckedExtrinsic =
    generic::UncheckedExtrinsic<Address, RuntimeCall, Signature, SignedExtra>;

pub type Header = generic::Header<BlockNumber, BlakeTwo256>;
pub type Block = generic::Block<Header, UncheckedExtrinsic>;

// ===== Opaque types (node) =====
pub mod opaque {
    use super::*;
    pub use sp_runtime::OpaqueExtrinsic as UncheckedExtrinsic;
    pub type Header = sp_runtime::generic::Header<BlockNumber, sp_runtime::traits::BlakeTwo256>;
    pub type Block = sp_runtime::generic::Block<Header, UncheckedExtrinsic>;
    pub type BlockId = sp_runtime::generic::BlockId<Block>;
}

// ===== Runtime version =====
parameter_types! {
    pub const BlockHashCount: BlockNumber = 2400;
    pub const SS58Prefix: u16 = 42;
    pub const Version: RuntimeVersion = VERSION;
}

/// Reject only calls capable of bypassing the shared ETKN issuance allowance.
///
/// Sudo dispatches nested calls without the system base filter, so every sudo call-bearing variant
/// is inspected recursively. Raw storage operations remain available for unrelated keys but cannot
/// touch native accounts, Balances state, or issuance-cap state.
pub struct IssuanceCallFilter;

impl IssuanceCallFilter {
    fn prefix_overlaps(prefix: &[u8], protected: &[u8]) -> bool {
        prefix.starts_with(protected) || protected.starts_with(prefix)
    }

    fn raw_key_touches_issuance_state(key: &[u8]) -> bool {
        let balances = sp_io::hashing::twox_128(b"Balances");
        let issuance_cap = sp_io::hashing::twox_128(b"IssuanceCap");
        let v13_input = sp_io::hashing::twox_128(issuance_cap::V13_MIGRATION_INPUT_PALLET_PREFIX);
        let security_budget = sp_io::hashing::twox_128(b"SecurityBudget");
        let reward_reserve = sp_io::hashing::twox_128(b"RewardReserve");
        let staking = sp_io::hashing::twox_128(b"Staking");
        let system_account = frame_support::storage::storage_prefix(b"System", b"Account");
        key.starts_with(&sp_io::hashing::twox_128(b"FreshGenesis"))
            || key.starts_with(&balances)
            || key.starts_with(&issuance_cap)
            || key.starts_with(&v13_input)
            || key.starts_with(&security_budget)
            || key.starts_with(&reward_reserve)
            || key.starts_with(&staking)
            || key.starts_with(&system_account)
    }

    fn raw_prefix_touches_issuance_state(prefix: &[u8]) -> bool {
        let balances = sp_io::hashing::twox_128(b"Balances");
        let issuance_cap = sp_io::hashing::twox_128(b"IssuanceCap");
        let v13_input = sp_io::hashing::twox_128(issuance_cap::V13_MIGRATION_INPUT_PALLET_PREFIX);
        let security_budget = sp_io::hashing::twox_128(b"SecurityBudget");
        let reward_reserve = sp_io::hashing::twox_128(b"RewardReserve");
        let staking = sp_io::hashing::twox_128(b"Staking");
        let system_account = frame_support::storage::storage_prefix(b"System", b"Account");
        Self::prefix_overlaps(prefix, &sp_io::hashing::twox_128(b"FreshGenesis"))
            || Self::prefix_overlaps(prefix, &balances)
            || Self::prefix_overlaps(prefix, &issuance_cap)
            || Self::prefix_overlaps(prefix, &v13_input)
            || Self::prefix_overlaps(prefix, &security_budget)
            || Self::prefix_overlaps(prefix, &reward_reserve)
            || Self::prefix_overlaps(prefix, &staking)
            || Self::prefix_overlaps(prefix, &system_account)
    }

    pub(crate) fn is_uncontrolled_issuance(call: &RuntimeCall) -> bool {
        match call {
            RuntimeCall::Balances(pallet_balances::Call::force_set_balance { .. }) => true,
            RuntimeCall::Balances(pallet_balances::Call::force_adjust_total_issuance {
                direction: pallet_balances::AdjustmentDirection::Increase,
                ..
            }) => true,
            RuntimeCall::Staking(pallet_staking::Call::validate { prefs })
                if prefs.commission > SecurityBudgetMaxValidatorCommission::get() =>
            {
                true
            }
            RuntimeCall::Staking(pallet_staking::Call::set_min_commission { new })
                if *new > SecurityBudgetMaxValidatorCommission::get() =>
            {
                true
            }
            RuntimeCall::Staking(pallet_staking::Call::set_staking_configs {
                min_validator_bond,
                min_commission,
                ..
            }) if matches!(min_validator_bond, pallet_staking::ConfigOp::Remove)
                || matches!(
                    min_validator_bond,
                    pallet_staking::ConfigOp::Set(value)
                        if *value < SecurityBudgetMinimumValidatorBond::get()
                )
                || matches!(
                    min_commission,
                    pallet_staking::ConfigOp::Set(value)
                        if *value > SecurityBudgetMaxValidatorCommission::get()
                ) =>
            {
                true
            }
            RuntimeCall::Staking(pallet_staking::Call::set_validator_count { new })
                if *new > pallet_staking::ValidatorCount::<Runtime>::get().saturating_add(1) =>
            {
                true
            }
            RuntimeCall::Staking(pallet_staking::Call::increase_validator_count { additional })
                if *additional != 1 =>
            {
                true
            }
            RuntimeCall::Staking(pallet_staking::Call::scale_validator_count { factor })
                if *factor != sp_runtime::Percent::from_percent(0) =>
            {
                true
            }
            RuntimeCall::System(frame_system::Call::set_storage { items }) => items
                .iter()
                .any(|(key, _)| Self::raw_key_touches_issuance_state(key)),
            RuntimeCall::System(frame_system::Call::kill_storage { keys }) => keys
                .iter()
                .any(|key| Self::raw_key_touches_issuance_state(key)),
            RuntimeCall::System(frame_system::Call::kill_prefix { prefix, .. }) => {
                Self::raw_prefix_touches_issuance_state(prefix)
            }
            RuntimeCall::Sudo(pallet_sudo::Call::sudo { call })
            | RuntimeCall::Sudo(pallet_sudo::Call::sudo_unchecked_weight { call, .. })
            | RuntimeCall::Sudo(pallet_sudo::Call::sudo_as { call, .. }) => {
                Self::is_uncontrolled_issuance(call)
            }
            _ => false,
        }
    }
}

impl Contains<RuntimeCall> for IssuanceCallFilter {
    fn contains(call: &RuntimeCall) -> bool {
        !Self::is_uncontrolled_issuance(call)
    }
}

impl system::Config for Runtime {
    type BaseCallFilter = IssuanceCallFilter;
    type BlockWeights = ();
    type BlockLength = ();
    type DbWeight = RocksDbWeight;
    type RuntimeOrigin = RuntimeOrigin;
    type RuntimeEvent = RuntimeEvent;
    type RuntimeCall = RuntimeCall;
    type Nonce = Index;
    type Hash = Hash;
    type Hashing = BlakeTwo256;
    type AccountId = AccountId;
    type Lookup = AccountIdLookup<AccountId, ()>;
    type Block = Block;
    type BlockHashCount = BlockHashCount;
    type Version = Version;
    type PalletInfo = PalletInfo;
    type AccountData = pallet_balances::AccountData<Balance>;
    type OnNewAccount = ();
    type OnKilledAccount = ();
    type SystemWeightInfo = ();
    type SS58Prefix = SS58Prefix;
    type MaxConsumers = ConstU32<16>;

    // present on this SDK tag
    type RuntimeTask = ();
    type ExtensionsWeightInfo = ();
    type OnSetCode = ();
    type SingleBlockMigrations = (v13_migration::V13Migration, v14_migration::V14Migration);
    type MultiBlockMigrator = ();
    type PreInherents = ();
    type PostInherents = ();
    type PostTransactions = ();
}

// ===== Timestamp =====
parameter_types! {
    pub const MinimumPeriod: Moment = (SLOT_DURATION / 2) as Moment;
}
impl pallet_timestamp::Config for Runtime {
    type Moment = Moment;
    // BABE must be notified when timestamp is set (slot progression)
    type OnTimestampSet = Babe;
    type MinimumPeriod = MinimumPeriod;
    type WeightInfo = ();
}

// ===== Balances =====
parameter_types! {
    pub const ExistentialDeposit: Balance = EXISTENTIAL_DEPOSIT;
}
impl pallet_balances::Config for Runtime {
    type Balance = Balance;
    type DustRemoval = ();
    type RuntimeEvent = RuntimeEvent;
    type ExistentialDeposit = ExistentialDeposit;
    type AccountStore = system::Pallet<Runtime>;
    type MaxLocks = ConstU32<50>;
    type MaxReserves = ConstU32<50>;
    type ReserveIdentifier = [u8; 8];
    type WeightInfo = ();
    // present on this SDK line:
    type RuntimeHoldReason = RuntimeHoldReason;
    type RuntimeFreezeReason = ();
    type FreezeIdentifier = ();
    type MaxFreezes = ConstU32<0>;
    type DoneSlashHandler = ();
}

impl fresh_genesis::Config for Runtime {}

impl issuance_cap::Config for Runtime {
    type Currency = Balances;
}

// ===== Transaction Payment =====
type FeeCredit = Credit<AccountId, Balances>;

pub struct DealWithFees;
impl DealWithFees {
    fn resolve(destination: &AccountId, credit: FeeCredit) -> Result<(), FeeCredit> {
        <Balances as Balanced<AccountId>>::resolve(destination, credit)
    }

    fn fail_closed_with_credit(
        credit: FeeCredit,
        normal_fee: Balance,
        tip: Balance,
        reason: pallet_reward_reserve::FeeRoutingFailure,
    ) -> ! {
        RewardReserve::note_fee_routing_failure(normal_fee, tip, reason);
        // The activated collection account is ED-pinned and can only receive credits whose value
        // was withdrawn elsewhere in the same issuance. Resolution is therefore infallible under
        // valid Balances invariants. If those invariants are ever violated, retain the credit and
        // trap the block so the outer runtime execution rolls back instead of burning it.
        let _preserved_credit = core::mem::ManuallyDrop::new(credit);
        panic!("activated fee collection account rejected an existing-issuance credit")
    }

    fn collect(credit: FeeCredit, normal_fee: Balance, tip: Balance) {
        let collection = RewardReserve::fee_collection_account();
        match Self::resolve(&collection, credit) {
            Ok(()) => {}
            Err(returned) => Self::fail_closed_with_credit(
                returned,
                normal_fee,
                tip,
                pallet_reward_reserve::FeeRoutingFailure::CollectionPotResolution,
            ),
        }
    }

    fn burn_legacy_spec10(fees: FeeCredit, tips: Option<FeeCredit>) {
        // Spec10 used CurrencyAdapter<Balances, ()>. Dropping both corrected credits deliberately
        // rescinded issuance. Dormant Spec11 preserves that exact economic behavior until Root
        // completes the activation gate.
        drop(fees);
        if let Some(tip_credit) = tips {
            drop(tip_credit);
        }
    }

    pub(crate) fn route_credits(
        fees: FeeCredit,
        tips: Option<FeeCredit>,
        author: Option<AccountId>,
    ) {
        if SecurityBudget::active() {
            Self::route_v14_credits(fees, tips, author);
            return;
        }
        let normal_fee = fees.peek();
        let tip = tips.as_ref().map(Imbalance::peek).unwrap_or_default();
        if !RewardReserve::fee_routing_active() {
            Self::burn_legacy_spec10(fees, tips);
            return;
        }

        if let Err(error) = RewardReserve::stage_normal_fee(normal_fee) {
            RewardReserve::note_fee_routing_failure(
                normal_fee,
                tip,
                pallet_reward_reserve::FeeRoutingFailure::PendingRoutingInconsistent,
            );
            let _preserved_credit = core::mem::ManuallyDrop::new(fees);
            panic!("unable to record collected normal-fee obligation: {error:?}")
        }
        Self::collect(fees, normal_fee, tip);
        match RewardReserve::settle_pending_fee_routing() {
            Ok(()) => {}
            Err(_deferred) => {
                // The complete normal fee remains in the Fee Collection Pot with recorded,
                // retryable Reward/Treasury obligations and a precise failure event.
            }
        }

        if let Some(tip_credit) = tips {
            if tip != 0 {
                if let Err(error) = RewardReserve::stage_author_tip(author.clone(), tip) {
                    RewardReserve::note_fee_routing_failure(
                        normal_fee,
                        tip,
                        pallet_reward_reserve::FeeRoutingFailure::PendingRoutingInconsistent,
                    );
                    let _preserved_credit = core::mem::ManuallyDrop::new(tip_credit);
                    panic!("unable to record collected author-tip obligation: {error:?}")
                }
                Self::collect(tip_credit, normal_fee, tip);
                if let Some(author_account) = author {
                    match RewardReserve::settle_author_tip(author_account) {
                        Ok(()) => {}
                        Err(_deferred) => {
                            // The tip remains held and keyed to its author for explicit retry.
                        }
                    }
                } else {
                    RewardReserve::note_fee_routing_failure(
                        normal_fee,
                        tip,
                        pallet_reward_reserve::FeeRoutingFailure::MissingBlockAuthor,
                    );
                }
            }
        }
    }

    fn route_v14_credits(fees: FeeCredit, tips: Option<FeeCredit>, author: Option<AccountId>) {
        let normal_fee = fees.peek();
        let tip = tips.as_ref().map(Imbalance::peek).unwrap_or_default();
        if let Err(error) = SecurityBudget::stage_normal_fee(normal_fee) {
            let _preserved_credit = core::mem::ManuallyDrop::new(fees);
            panic!("unable to stage V14 normal fee: {error:?}")
        }
        Self::collect(fees, normal_fee, tip);
        if SecurityBudget::settle_pending_normal_fee().is_err() {
            SecurityBudget::note_normal_fee_deferred();
        }

        if let Some(tip_credit) = tips {
            if tip == 0 {
                return;
            }
            if let Err(error) = SecurityBudget::stage_tip(author.clone(), tip) {
                let _preserved_credit = core::mem::ManuallyDrop::new(tip_credit);
                panic!("unable to stage V14 tip: {error:?}")
            }
            Self::collect(tip_credit, normal_fee, tip);
            if SecurityBudget::settle_pending_tip(author.clone()).is_err() {
                SecurityBudget::note_tip_deferred(author);
            }
        }
    }
}

impl OnUnbalanced<FeeCredit> for DealWithFees {
    fn on_unbalanceds(mut fees_then_tips: impl Iterator<Item = FeeCredit>) {
        if let Some(fees) = fees_then_tips.next() {
            let tips = fees_then_tips.next();
            Self::route_credits(fees, tips, Authorship::author());
        }
    }
}

pub struct FeeRoutingTransactionPaymentWeight;
impl FeeRoutingTransactionPaymentWeight {
    fn legacy_routing_envelope() -> Weight {
        // Retained for the pre-activation RewardReserve branch; this branch and V14 routing are
        // mutually exclusive.
        Weight::from_parts(10_000_000_000, 16_000)
            .saturating_add(RocksDbWeight::get().reads_writes(24, 20))
    }

    fn v14_routing_envelope() -> Weight {
        use pallet_security_budget::weights::WeightInfo;
        type W = pallet_security_budget::weights::SubstrateWeight<Runtime>;
        let normal_fee = W::route_normal_fee(100);
        let tip = W::route_tip_author(100).max(W::route_tip_fallback(100));
        normal_fee.saturating_add(tip)
    }
}

impl pallet_transaction_payment::WeightInfo for FeeRoutingTransactionPaymentWeight {
    fn charge_transaction_payment() -> Weight {
        let routing = Self::legacy_routing_envelope().max(Self::v14_routing_envelope());
        pallet_transaction_payment::weights::SubstrateWeight::<Runtime>::charge_transaction_payment(
        )
        .saturating_add(routing)
    }
}

impl pallet_transaction_payment::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type OnChargeTransaction = pallet_transaction_payment::FungibleAdapter<Balances, DealWithFees>;
    type OperationalFeeMultiplier = frame_support::traits::ConstU8<5>;
    type WeightToFee = IdentityFee<Balance>;
    type LengthToFee = ConstantMultiplier<Balance, ConstU128<0>>;
    type FeeMultiplierUpdate = ();
    type WeightInfo = FeeRoutingTransactionPaymentWeight;
}

// ===== Sudo =====
impl pallet_sudo::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type RuntimeCall = RuntimeCall;
    type WeightInfo = ();
}

// ===== Session keys (BABE + GRANDPA) =====
pub mod session_keys {
    use super::*;
    impl_opaque_keys! {
        pub struct SessionKeys {
            pub babe: pallet_babe::AuthorityId,
            pub grandpa: pallet_grandpa::AuthorityId,
        }
    }
}
pub use session_keys::SessionKeys;

// ===== Session =====
impl pallet_session::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type SessionManager = pallet_session::historical::NoteHistoricalRoot<Runtime, Staking>;
    type Keys = SessionKeys;
    type ShouldEndSession = pallet_babe::Pallet<Runtime>;
    type NextSessionRotation = pallet_babe::Pallet<Runtime>;
    type SessionHandler = (
        pallet_babe::Pallet<Runtime>,
        pallet_grandpa::Pallet<Runtime>,
    );
    type ValidatorId = <Self as system::Config>::AccountId;
    type ValidatorIdOf = sp_runtime::traits::ConvertInto;
    type DisablingStrategy = ();
    type WeightInfo = ();
}
impl pallet_session::historical::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type FullIdentification = pallet_staking::Exposure<AccountId, Balance>;
    type FullIdentificationOf = pallet_staking::DefaultExposureOf<Runtime>;
}

// ===== Authorship (v1.19.1 minimal) =====
impl pallet_authorship::Config for Runtime {
    // Map BABE author index -> AccountId via Session
    type FindAuthor = pallet_session::FindAccountFromAuthorIndex<Self, Babe>;
    // Credit each BABE-authored block to the active validator's Staking reward points.
    type EventHandler = Staking;
}

// ===== Verified consensus reports: observe by default, policy-parametric local Apply =====
// Historical internal type names remain for fixture continuity; these now delegate
// to v14_penalties after verified recording when a policy is configured.
pub struct ObserveOnlyOffenceSink;
pub struct BabeObserveOnlyReportSystem;
pub struct GrandpaObserveOnlyReportSystem;
pub struct BabeObserveOnlyWeightAdapter<T>(core::marker::PhantomData<T>);
pub struct GrandpaObserveOnlyWeightAdapter<T>(core::marker::PhantomData<T>);

pub type HistoricalIdentification = pallet_session::historical::IdentificationTuple<Runtime>;
pub type BabeObservationEvidence = (
    sp_consensus_babe::EquivocationProof<Header>,
    sp_session::MembershipProof,
);
pub type GrandpaObservationEvidence = (
    sp_consensus_grandpa::EquivocationProof<Hash, BlockNumber>,
    sp_session::MembershipProof,
);

pub(crate) fn canonical_babe_proof_hash(
    proof: &sp_consensus_babe::EquivocationProof<Header>,
) -> [u8; 32] {
    sp_io::hashing::blake2_256(&codec::Encode::encode(proof))
}

pub(crate) fn canonical_grandpa_proof_hash(
    proof: &sp_consensus_grandpa::EquivocationProof<Hash, BlockNumber>,
) -> [u8; 32] {
    sp_io::hashing::blake2_256(&codec::Encode::encode(proof))
}

parameter_types! {
    pub const ReportLongevity: u64 =
        BondingDuration::get() as u64 * SessionsPerEra::get() as u64 * EpochDuration::get();
}

fn component_wise_weight_max<const N: usize>(weights: [Weight; N]) -> Weight {
    weights.into_iter().fold(Weight::zero(), |maximum, weight| {
        Weight::from_parts(
            maximum.ref_time().max(weight.ref_time()),
            maximum.proof_size().max(weight.proof_size()),
        )
    })
}

impl pallet_babe::WeightInfo for BabeObserveOnlyWeightAdapter<Runtime> {
    fn plan_config_change() -> Weight {
        <() as pallet_babe::WeightInfo>::plan_config_change()
    }

    fn report_equivocation(validator_count: u32, _max_nominators_per_validator: u32) -> Weight {
        use era_validator_security::observation::WeightInfo;
        type W = era_validator_security::weights::SubstrateWeight<Runtime>;
        let validator_count = validator_count.max(100).min(MaxBabeAuthorities::get());
        component_wise_weight_max([
            W::babe_new(validator_count),
            W::babe_duplicate(validator_count),
            W::babe_invalid(validator_count),
            W::babe_capacity_full(validator_count),
        ]).saturating_add(v14_penalties::measured_apply_weight())
    }
}

impl pallet_grandpa::WeightInfo for GrandpaObserveOnlyWeightAdapter<Runtime> {
    fn report_equivocation(validator_count: u32, _max_nominators_per_validator: u32) -> Weight {
        use era_validator_security::observation::WeightInfo;
        type W = era_validator_security::weights::SubstrateWeight<Runtime>;
        let validator_count = validator_count.max(100).min(MaxGrandpaAuthorities::get());
        component_wise_weight_max([
            W::grandpa_new(validator_count),
            W::grandpa_duplicate(validator_count),
            W::grandpa_invalid(validator_count),
            W::grandpa_capacity_full(validator_count),
        ]).saturating_add(v14_penalties::measured_apply_weight())
    }

    fn note_stalled() -> Weight {
        <() as pallet_grandpa::WeightInfo>::note_stalled()
    }
}

impl
    sp_staking::offence::ReportOffence<
        AccountId,
        HistoricalIdentification,
        pallet_babe::EquivocationOffence<HistoricalIdentification>,
    > for ObserveOnlyOffenceSink
{
    fn report_offence(
        _reporters: Vec<AccountId>,
        offence: pallet_babe::EquivocationOffence<HistoricalIdentification>,
    ) -> Result<(), sp_staking::offence::OffenceError> {
        let (offender, _) = offence.offender;
        let penalty_offender=offender.clone();
        match ValidatorSecurity::record_validated_babe(
            offender,
            offence.session_index,
            *offence.slot,
        ) {
            Ok(outcome) => v14_penalties::after_verified_report(penalty_offender,offence.session_index,0,offence.validator_set_count,outcome.stored>0),
            Err(error) => {
                let code = ValidatorSecurity::adapter_error_code(error);
                era_validator_security::observation::PendingAdapterError::<Runtime>::put(code);
                Err(sp_staking::offence::OffenceError::Other(code))
            }
        }
    }

    fn is_known_offence(
        _offenders: &[HistoricalIdentification],
        _time_slot: &sp_consensus_babe::Slot,
    ) -> bool {
        false
    }
}

impl
    sp_staking::offence::ReportOffence<
        AccountId,
        HistoricalIdentification,
        pallet_grandpa::EquivocationOffence<HistoricalIdentification>,
    > for ObserveOnlyOffenceSink
{
    fn report_offence(
        _reporters: Vec<AccountId>,
        offence: pallet_grandpa::EquivocationOffence<HistoricalIdentification>,
    ) -> Result<(), sp_staking::offence::OffenceError> {
        let (offender, _) = offence.offender;
        let penalty_offender=offender.clone();
        match ValidatorSecurity::record_validated_grandpa(
            offender,
            offence.session_index,
            offence.time_slot.set_id,
            offence.time_slot.round,
        ) {
            Ok(outcome) => v14_penalties::after_verified_report(penalty_offender,offence.session_index,1,offence.validator_set_count,outcome.stored>0),
            Err(error) => {
                let code = ValidatorSecurity::adapter_error_code(error);
                era_validator_security::observation::PendingAdapterError::<Runtime>::put(code);
                Err(sp_staking::offence::OffenceError::Other(code))
            }
        }
    }

    fn is_known_offence(
        _offenders: &[HistoricalIdentification],
        _time_slot: &pallet_grandpa::TimeSlot,
    ) -> bool {
        false
    }
}

impl sp_staking::offence::OffenceReportSystem<Option<AccountId>, BabeObservationEvidence>
    for BabeObserveOnlyReportSystem
{
    type Longevity = ReportLongevity;

    fn publish_evidence(evidence: BabeObservationEvidence) -> Result<(), ()> {
        type Inner = pallet_babe::EquivocationReportSystem<
            Runtime,
            ObserveOnlyOffenceSink,
            Historical,
            ReportLongevity,
        >;
        <Inner as sp_staking::offence::OffenceReportSystem<_, _>>::publish_evidence(evidence)
    }

    fn check_evidence(
        evidence: BabeObservationEvidence,
    ) -> Result<(), sp_runtime::transaction_validity::TransactionValidityError> {
        use frame_support::traits::KeyOwnerProofSystem;
        use sp_runtime::transaction_validity::InvalidTransaction;
        use sp_session::{GetSessionNumber, GetValidatorCount};

        let (equivocation_proof, key_owner_proof) = evidence;
        if key_owner_proof.validator_count() > MaxBabeAuthorities::get() {
            return Err(InvalidTransaction::BadProof.into());
        }
        let (offender, _) = Historical::check_proof(
            (sp_consensus_babe::KEY_TYPE, equivocation_proof.offender),
            key_owner_proof.clone(),
        )
        .ok_or(InvalidTransaction::BadProof)?;
        match ValidatorSecurity::identity_known_babe(
            offender,
            key_owner_proof.session(),
            *equivocation_proof.slot,
        ) {
            Ok(false) => Ok(()),
            Ok(true) => Err(InvalidTransaction::Stale.into()),
            Err(error) => {
                Err(InvalidTransaction::Custom(ValidatorSecurity::adapter_error_code(error)).into())
            }
        }
    }

    fn process_evidence(
        reporter: Option<AccountId>,
        evidence: BabeObservationEvidence,
    ) -> Result<(), sp_runtime::DispatchError> {
        use frame_support::storage::{with_transaction, TransactionOutcome};
        use sp_staking::offence::OffenceReportSystem;

        if reporter.is_some() {
            return Err(
                era_validator_security::observation::Error::<Runtime>::SignedIntakeRejected.into(),
            );
        }
        let proof_hash = canonical_babe_proof_hash(&evidence.0);
        with_transaction(|| {
            if let Err(error) = ValidatorSecurity::begin_evidence(
                era_validator_security::observation::ObservationKind::Babe,
                proof_hash,
            ) {
                return TransactionOutcome::Rollback(Err(error.into()));
            }
            type Inner = pallet_babe::EquivocationReportSystem<
                Runtime,
                ObserveOnlyOffenceSink,
                Historical,
                ReportLongevity,
            >;
            match <Inner as OffenceReportSystem<_, _>>::process_evidence(None, evidence) {
                Ok(()) if ValidatorSecurity::transient_state_is_clear() => {
                    TransactionOutcome::Commit(Ok(()))
                }
                Ok(()) => {
                    TransactionOutcome::Rollback(Err(era_validator_security::observation::Error::<
                        Runtime,
                    >::AdapterFailure
                        .into()))
                }
                Err(error) => {
                    let mapped =
                        era_validator_security::observation::PendingAdapterError::<Runtime>::get()
                            .map(ValidatorSecurity::dispatch_error_for_code)
                            .unwrap_or(error);
                    TransactionOutcome::Rollback(Err(mapped))
                }
            }
        })
    }
}

impl sp_staking::offence::OffenceReportSystem<Option<AccountId>, GrandpaObservationEvidence>
    for GrandpaObserveOnlyReportSystem
{
    type Longevity = ReportLongevity;

    fn publish_evidence(evidence: GrandpaObservationEvidence) -> Result<(), ()> {
        type Inner = pallet_grandpa::EquivocationReportSystem<
            Runtime,
            ObserveOnlyOffenceSink,
            Historical,
            ReportLongevity,
        >;
        <Inner as sp_staking::offence::OffenceReportSystem<_, _>>::publish_evidence(evidence)
    }

    fn check_evidence(
        evidence: GrandpaObservationEvidence,
    ) -> Result<(), sp_runtime::transaction_validity::TransactionValidityError> {
        use frame_support::traits::KeyOwnerProofSystem;
        use sp_runtime::transaction_validity::InvalidTransaction;
        use sp_session::{GetSessionNumber, GetValidatorCount};

        let (equivocation_proof, key_owner_proof) = evidence;
        if key_owner_proof.validator_count() > MaxGrandpaAuthorities::get() {
            return Err(InvalidTransaction::BadProof.into());
        }
        let (offender, _) = Historical::check_proof(
            (
                sp_consensus_grandpa::KEY_TYPE,
                equivocation_proof.offender().clone(),
            ),
            key_owner_proof.clone(),
        )
        .ok_or(InvalidTransaction::BadProof)?;
        match ValidatorSecurity::identity_known_grandpa(
            offender,
            key_owner_proof.session(),
            equivocation_proof.set_id(),
            equivocation_proof.round(),
        ) {
            Ok(false) => Ok(()),
            Ok(true) => Err(InvalidTransaction::Stale.into()),
            Err(error) => {
                Err(InvalidTransaction::Custom(ValidatorSecurity::adapter_error_code(error)).into())
            }
        }
    }

    fn process_evidence(
        reporter: Option<AccountId>,
        evidence: GrandpaObservationEvidence,
    ) -> Result<(), sp_runtime::DispatchError> {
        use frame_support::storage::{with_transaction, TransactionOutcome};
        use sp_staking::offence::OffenceReportSystem;

        if reporter.is_some() {
            return Err(
                era_validator_security::observation::Error::<Runtime>::SignedIntakeRejected.into(),
            );
        }
        let proof_hash = canonical_grandpa_proof_hash(&evidence.0);
        with_transaction(|| {
            if let Err(error) = ValidatorSecurity::begin_evidence(
                era_validator_security::observation::ObservationKind::Grandpa,
                proof_hash,
            ) {
                return TransactionOutcome::Rollback(Err(error.into()));
            }
            type Inner = pallet_grandpa::EquivocationReportSystem<
                Runtime,
                ObserveOnlyOffenceSink,
                Historical,
                ReportLongevity,
            >;
            match <Inner as OffenceReportSystem<_, _>>::process_evidence(None, evidence) {
                Ok(()) if ValidatorSecurity::transient_state_is_clear() => {
                    TransactionOutcome::Commit(Ok(()))
                }
                Ok(()) => {
                    TransactionOutcome::Rollback(Err(era_validator_security::observation::Error::<
                        Runtime,
                    >::AdapterFailure
                        .into()))
                }
                Err(error) => {
                    let mapped =
                        era_validator_security::observation::PendingAdapterError::<Runtime>::get()
                            .map(ValidatorSecurity::dispatch_error_for_code)
                            .unwrap_or(error);
                    TransactionOutcome::Rollback(Err(mapped))
                }
            }
        })
    }
}

impl era_validator_security::observation::Config for Runtime {
    type ObserverWeightInfo = era_validator_security::weights::SubstrateWeight<Runtime>;
    type BabeMaxAuthorities = MaxBabeAuthorities;
    type GrandpaMaxAuthorities = MaxGrandpaAuthorities;

    #[cfg(feature = "runtime-benchmarks")]
    type BenchmarkHelper = RuntimeObserverBenchmarkHelper;
}

// ===== BABE =====
parameter_types! {
    pub const EpochDuration: u64 = 10 * 60 * 1000 / MILLISECS_PER_BLOCK; // ~10 minutes in blocks
    pub const ExpectedBlockTime: u64 = MILLISECS_PER_BLOCK;
    pub const MaxBabeAuthorities: u32 = 1_000;
}
impl pallet_babe::Config for Runtime {
    type EpochDuration = EpochDuration;
    type ExpectedBlockTime = ExpectedBlockTime;
    type EpochChangeTrigger = pallet_babe::ExternalTrigger;
    type DisabledValidators = ();
    type KeyOwnerProof = sp_session::MembershipProof;
    type EquivocationReportSystem = BabeObserveOnlyReportSystem;
    type MaxAuthorities = MaxBabeAuthorities;
    type WeightInfo = BabeObserveOnlyWeightAdapter<Runtime>;
    // present on this SDK line
    type MaxNominators = ConstU32<0>;
}

// ===== GRANDPA =====
parameter_types! {
    pub const MaxSetIdSessionEntries: u64 = 32;
    pub const MaxGrandpaAuthorities: u32 = 1_000;
}
impl pallet_grandpa::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type KeyOwnerProof = sp_session::MembershipProof;
    type EquivocationReportSystem = GrandpaObserveOnlyReportSystem;
    type MaxAuthorities = MaxGrandpaAuthorities;
    type MaxSetIdSessionEntries = MaxSetIdSessionEntries;
    type WeightInfo = GrandpaObserveOnlyWeightAdapter<Runtime>;
    // present on this SDK line
    type MaxNominators = ConstU32<0>;
}

// ===== Staking (bounded on-chain election provider) =====
parameter_types! {
    pub const SessionsPerEra: sp_staking::SessionIndex = 6;
    pub const BondingDuration: sp_staking::EraIndex = 24;
    pub const SlashDeferDuration: sp_staking::EraIndex = 23;
    pub const HistoryDepth: u32 = 84;
    pub const MaxUnlocking: u32 = 32;
    pub const MaxExposurePageSize: u32 = 4096;
    pub const MaxElectingVoters: u32 = 64;
    pub const MaxElectableTargets: u32 = 16;
    pub const MaxElectionBackersPerWinner: u32 = 64;
    pub const MaxValidatorSet: u32 = 16;
    pub const MaxControllersInDeprecationBatch: u32 = 0;
}

pub struct StakingElectionBounds;
impl frame_support::traits::Get<ElectionBounds> for StakingElectionBounds {
    fn get() -> ElectionBounds {
        ElectionBoundsBuilder::default()
            .voters_count(64.into())
            .voters_size((64 * 1024).into())
            .targets_count(16.into())
            .targets_size((4 * 1024).into())
            .build()
    }
}

pub struct OnChainSeqPhragmen;
impl onchain::Config for OnChainSeqPhragmen {
    type System = Runtime;
    type Solver = SequentialPhragmen<AccountId, Perbill>;
    type DataProvider = Staking;
    type WeightInfo = frame_election_provider_support::weights::SubstrateWeight<Runtime>;
    type MaxWinnersPerPage = MaxElectableTargets;
    type MaxBackersPerWinner = MaxElectionBackersPerWinner;
    type Sort = ConstBool<true>;
    type Bounds = StakingElectionBounds;
}

pub type StakingElectionProvider = onchain::OnChainExecution<OnChainSeqPhragmen>;

// Minimal type to satisfy `BenchmarkingConfig`
pub struct StakingBenchCfg;
impl pallet_staking::BenchmarkingConfig for StakingBenchCfg {
    type MaxValidators = MaxElectableTargets;
    type MaxNominators = ConstU32<48>;
}

impl pallet_staking::Config for Runtime {
    type Currency = pallet_balances::Pallet<Runtime>;
    type CurrencyBalance = Balance;
    type UnixTime = pallet_timestamp::Pallet<Runtime>;
    type RuntimeEvent = RuntimeEvent;

    type Slash = v14_penalties::TreasurySlash;
    type Reward = ();
    type RewardRemainder = ();
    type SessionsPerEra = SessionsPerEra;
    type BondingDuration = BondingDuration;
    type SlashDeferDuration = SlashDeferDuration;

    type SessionInterface = Self;

    // Keep payout simple: use the unit type which implements `EraPayout`
    type EraPayout = ();
    type NextNewSession = ();
    type HistoryDepth = HistoryDepth;
    type EventListeners = ();

    type VoterList = pallet_staking::UseNominatorsAndValidatorsMap<Runtime>;
    type TargetList = pallet_staking::UseValidatorsMap<Runtime>;
    type MaxUnlockingChunks = MaxUnlocking;
    type NominationsQuota = pallet_staking::FixedNominationsQuota<16>;

    type ElectionProvider = StakingElectionProvider;
    type GenesisElectionProvider = StakingElectionProvider;

    type OldCurrency = pallet_balances::Pallet<Runtime>;
    type RuntimeHoldReason = RuntimeHoldReason;
    type CurrencyToVote = sp_staking::currency_to_vote::U128CurrencyToVote;
    type AdminOrigin = frame_system::EnsureRoot<AccountId>;
    type MaxExposurePageSize = MaxExposurePageSize;
    type MaxValidatorSet = MaxValidatorSet;
    type MaxControllersInDeprecationBatch = MaxControllersInDeprecationBatch;
    type Filter = ();

    type BenchmarkingConfig = StakingBenchCfg;
    type WeightInfo = ();
}

// ===== Vesting =====
pub struct AllReasons;
impl sp_core::Get<WithdrawReasons> for AllReasons {
    fn get() -> WithdrawReasons {
        WithdrawReasons::all()
    }
}
/// No fees, tips, transfers, or reserves may consume unvested principal.
pub struct NoUnvestedWithdrawalExceptions;
impl sp_core::Get<WithdrawReasons> for NoUnvestedWithdrawalExceptions {
    fn get() -> WithdrawReasons {
        WithdrawReasons::empty()
    }
}
impl pallet_vesting::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type Currency = pallet_balances::Pallet<Runtime>;
    type BlockNumberToBalance = sp_runtime::traits::ConvertInto;
    type MinVestedTransfer = ConstU128<{ DECIMALS / 1_000 }>;
    type WeightInfo = ();
    type BlockNumberProvider = frame_system::Pallet<Runtime>;
    type UnvestedFundsAllowedWithdrawReasons = NoUnvestedWithdrawalExceptions;
    const MAX_VESTING_SCHEDULES: u32 = 128;
}

// ===== Historical standard-pallet custody rehearsal primitives =====
//
// These standard pallets and fail-closed proxy filters remain for compatibility and sealed
// rehearsal evidence. They are not controllers or destinations for the active founding custody.
parameter_types! {
    pub const MultisigDepositBase: Balance = DECIMALS;
    pub const MultisigDepositFactor: Balance = DECIMALS / 10;
    pub const MaxMultisigSignatories: u32 = 16;
    pub const ProxyDepositBase: Balance = DECIMALS;
    pub const ProxyDepositFactor: Balance = DECIMALS / 10;
    pub const MaxProxies: u32 = 8;
    pub const MaxPendingProxyAnnouncements: u32 = 8;
    pub const AnnouncementDepositBase: Balance = DECIMALS;
    pub const AnnouncementDepositFactor: Balance = DECIMALS / 10;
}

/// Historical rehearsal notice periods at six seconds per block. No active founding-custody
/// authority is granted to these proxy types.
pub const PRESALE_PROXY_DELAY: BlockNumber = 28_800; // 48 hours
pub const ECOSYSTEM_PROXY_DELAY: BlockNumber = 28_800; // 48 hours
pub const LIQUIDITY_PROXY_DELAY: BlockNumber = 100_800; // 7 days
pub const COMMUNITY_PROXY_DELAY: BlockNumber = 28_800; // 48 hours

#[derive(
    codec::Encode,
    codec::Decode,
    codec::DecodeWithMemTracking,
    codec::MaxEncodedLen,
    scale_info::TypeInfo,
    Clone,
    Copy,
    Debug,
    Default,
    Eq,
    Ord,
    PartialEq,
    PartialOrd,
)]
pub enum ProxyType {
    /// Most permissive by requirement of the pinned pallet, but limited to data-only remarks.
    #[default]
    AuditOnly,
    Presale,
    Ecosystem,
    Liquidity,
    Community,
}

impl InstanceFilter<RuntimeCall> for ProxyType {
    fn filter(&self, call: &RuntimeCall) -> bool {
        matches!(
            (self, call),
            (
                Self::AuditOnly,
                RuntimeCall::System(frame_system::Call::remark { .. })
            )
        )
    }

    fn is_superset(&self, other: &Self) -> bool {
        self == other || matches!(self, Self::AuditOnly)
    }
}

impl pallet_multisig::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type RuntimeCall = RuntimeCall;
    type Currency = Balances;
    type DepositBase = MultisigDepositBase;
    type DepositFactor = MultisigDepositFactor;
    type MaxSignatories = MaxMultisigSignatories;
    type WeightInfo = pallet_multisig::weights::SubstrateWeight<Runtime>;
    type BlockNumberProvider = System;
}

impl pallet_proxy::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type RuntimeCall = RuntimeCall;
    type Currency = Balances;
    type ProxyType = ProxyType;
    type ProxyDepositBase = ProxyDepositBase;
    type ProxyDepositFactor = ProxyDepositFactor;
    type MaxProxies = MaxProxies;
    type WeightInfo = pallet_proxy::weights::SubstrateWeight<Runtime>;
    type MaxPending = MaxPendingProxyAnnouncements;
    type CallHasher = BlakeTwo256;
    type AnnouncementDepositBase = AnnouncementDepositBase;
    type AnnouncementDepositFactor = AnnouncementDepositFactor;
    type BlockNumberProvider = System;
}

mod v14_asset_weights;

// ===== FRAME-native application foundations =====
parameter_types! {
    pub const AssetDeposit: Balance = 10 * DECIMALS;
    pub const AssetAccountDeposit: Balance = DECIMALS / 10;
    pub const AssetMetadataDepositBase: Balance = DECIMALS;
    pub const AssetMetadataDepositPerByte: Balance = DECIMALS / 1_000;
    pub const AssetApprovalDeposit: Balance = DECIMALS / 10;
    pub const AssetStringLimit: u32 = 64;
    pub const AssetRemoveItemsLimit: u32 = 1_000;
}

impl pallet_assets::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type Balance = Balance;
    type RemoveItemsLimit = AssetRemoveItemsLimit;
    type AssetId = u32;
    type AssetIdParameter = u32;
    type Currency = Balances;
    type CreateOrigin = v14_allocator::CreateOrigin<0>;
    type ForceOrigin = frame_system::EnsureNever<AccountId>;
    type AssetDeposit = AssetDeposit;
    type AssetAccountDeposit = AssetAccountDeposit;
    type MetadataDepositBase = AssetMetadataDepositBase;
    type MetadataDepositPerByte = AssetMetadataDepositPerByte;
    type ApprovalDeposit = AssetApprovalDeposit;
    type StringLimit = AssetStringLimit;
    type Freezer = ();
    type Holder = ();
    type Extra = ();
    type CallbackHandle = ();
    type WeightInfo = v14_asset_weights::AssetsWeight<Runtime>;
}

parameter_types! {
    pub const NftCollectionDeposit: Balance = 10 * DECIMALS;
    pub const NftItemDeposit: Balance = DECIMALS;
    pub const NftMetadataDepositBase: Balance = DECIMALS;
    pub const NftAttributeDepositBase: Balance = DECIMALS;
    pub const NftDepositPerByte: Balance = DECIMALS / 1_000;
    pub const NftStringLimit: u32 = 128;
    pub const NftKeyLimit: u32 = 64;
    pub const NftValueLimit: u32 = 128;
    pub const NftApprovalsLimit: u32 = 20;
    pub const NftItemAttributesApprovalsLimit: u32 = 10;
    pub const NftMaxTips: u32 = 10;
    pub const NftMaxDeadlineDuration: BlockNumber = 14_400;
    pub const NftMaxAttributesPerCall: u32 = 10;
    // Final V14 scope requires native trading and swaps. Isolated development only:
    // deploying this changed runtime still requires the tested upgrade approval.
    pub NftFeatures: pallet_nfts::PalletFeatures = pallet_nfts::PalletFeatures::all_enabled();
}

impl pallet_nfts::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type CollectionId = u32;
    type ItemId = u32;
    type ItemCreationGuard = v14_allocator::ItemGuard;
    type AllocationManagedCollection = v14_allocator::ManagedCollection;
    type Currency = Balances;
    type ForceOrigin = frame_system::EnsureNever<AccountId>;
    type CreateOrigin = v14_allocator::CreateOrigin<1>;
    type Locker = ();
    type CollectionDeposit = NftCollectionDeposit;
    type ItemDeposit = NftItemDeposit;
    type MetadataDepositBase = NftMetadataDepositBase;
    type AttributeDepositBase = NftAttributeDepositBase;
    type DepositPerByte = NftDepositPerByte;
    type StringLimit = NftStringLimit;
    type KeyLimit = NftKeyLimit;
    type ValueLimit = NftValueLimit;
    type ApprovalsLimit = NftApprovalsLimit;
    type ItemAttributesApprovalsLimit = NftItemAttributesApprovalsLimit;
    type MaxTips = NftMaxTips;
    type MaxDeadlineDuration = NftMaxDeadlineDuration;
    type MaxAttributesPerCall = NftMaxAttributesPerCall;
    type Features = NftFeatures;
    type OffchainSignature = Signature;
    type OffchainPublic = AccountPublic;
    #[cfg(feature = "runtime-benchmarks")]
    type Helper = ();
    type WeightInfo = v14_nft_weights::NftsWeight<Runtime>;
    type BlockNumberProvider = System;
}

parameter_types! {
    pub const WorldRegistrationDeposit: Balance = 10 * DECIMALS;
    pub const MaxWorldIdLength: u32 = 64;
}

impl pallet_era_worlds::Config for Runtime {
    type Currency = Balances;
    // Identities and governance are unresolved, so privileged activation remains fail-closed.
    type RegistryAdminOrigin = frame_system::EnsureNever<()>;
    type EmergencyPauseOrigin = frame_system::EnsureNever<()>;
    type RegistrationDeposit = WorldRegistrationDeposit;
    type MaxWorldIdLength = MaxWorldIdLength;
    type WeightInfo = pallet_era_worlds::weights::SubstrateWeight<Runtime>;
}

// ===== AI Predictions =====
parameter_types! {
    pub const AiPredictionsPalletId: frame_support::PalletId = frame_support::PalletId(*b"era/aipr");
    pub AiPenaltyDestination: AccountId = sp_runtime::traits::AccountIdConversion::into_account_truncating(&EcosystemTreasuryPalletId::get());
}

#[cfg(not(test))]
pub type AiFinancialModesAllowed = frame_support::traits::ConstBool<false>;
#[cfg(test)]
parameter_types! { pub static AiFinancialModesAllowed: bool = false; }

pub struct AiNowSeconds;
impl frame_support::traits::Get<u64> for AiNowSeconds { fn get() -> u64 { Timestamp::get() / 1000 } }

impl pallet_ai_predictions::Config for Runtime {
    type NowSeconds = AiNowSeconds;
    type FinancialModesAllowed = AiFinancialModesAllowed;
    type Currency = Balances;
    type PalletId = AiPredictionsPalletId;
    type PenaltyDestination = AiPenaltyDestination;
    type MaxHashLen = ConstU32<128>;
    type MaxMetadataUriLen = ConstU32<256>;
    type MaxCategoryCodeLen = ConstU32<96>;
    type MaxModelsPerOwner = ConstU32<64>;
    type MaxPredictionsPerModel = ConstU32<1024>;
    type WeightInfo = pallet_ai_predictions::weights::WeightInfo<Runtime>;
}

// ===== Transfer-only staking reward reserve =====
parameter_types! {
    pub const RewardReservePalletId: frame_support::PalletId = frame_support::PalletId(*b"era/rwds");
    pub const EcosystemTreasuryPalletId: frame_support::PalletId = frame_support::PalletId(*b"era/trea");
    pub const FeeCollectionPalletId: frame_support::PalletId = frame_support::PalletId(*b"era/fees");
    // V14 retires this allocation target. Existing earned claims remain payable claim-only.
    pub const InitialRewardReserve: Balance = 0;
    pub const RewardPotSafetyFloor: Balance = EXISTENTIAL_DEPOSIT;
    pub const TreasuryPotSafetyFloor: Balance = EXISTENTIAL_DEPOSIT;
    pub const AnnualRewardCap: Balance = 5_000_000 * DECIMALS;
    pub const RewardMillisecondsPerYear: u64 = 31_556_952_000;
    pub const RewardMaxValidatorCommission: Perbill = Perbill::from_percent(10);
    pub const RewardLowReserveThreshold: Balance = 5_000_000 * DECIMALS;
    pub const InitialRewardPotFeeShare: u8 = 70;
    pub const InitialEcosystemTreasuryFeeShare: u8 = 30;
    pub const InitialBurnFeeShare: u8 = 0;
    pub const InitialTipToAuthorShare: u8 = 100;
    pub const MaxRewardPotFeeShare: u8 = 70;
    pub const MaxEcosystemTreasuryFeeShare: u8 = 30;
    pub const MaxBurnFeeShare: u8 = 0;
}

impl pallet_reward_reserve::Config for Runtime {
    type RewardPalletId = RewardReservePalletId;
    type EcosystemTreasuryPalletId = EcosystemTreasuryPalletId;
    type FeeCollectionPalletId = FeeCollectionPalletId;
    type InitialRewardReserve = InitialRewardReserve;
    type RewardPotSafetyFloor = RewardPotSafetyFloor;
    type TreasuryPotSafetyFloor = TreasuryPotSafetyFloor;
    type AnnualRewardCap = AnnualRewardCap;
    type MillisecondsPerYear = RewardMillisecondsPerYear;
    type MaxValidatorCommission = RewardMaxValidatorCommission;
    type LowReserveThreshold = RewardLowReserveThreshold;
    type InitialRewardPotFeeShare = InitialRewardPotFeeShare;
    type InitialEcosystemTreasuryFeeShare = InitialEcosystemTreasuryFeeShare;
    type InitialBurnFeeShare = InitialBurnFeeShare;
    type InitialTipToAuthorShare = InitialTipToAuthorShare;
    type MaxRewardPotFeeShare = MaxRewardPotFeeShare;
    type MaxEcosystemTreasuryFeeShare = MaxEcosystemTreasuryFeeShare;
    type MaxBurnFeeShare = MaxBurnFeeShare;
    type TreasurySpendOrigin = frame_system::EnsureNever<AccountId>;
    type MaxRewardNominatorsPerPage = MaxElectionBackersPerWinner;
    type RewardWeightInfo = pallet_reward_reserve::weights::SubstrateWeight<Runtime>;
}

// ===== Strict three-of-three founding custody =====
parameter_types! {
    // Reuses the existing custody PalletId. Founding accounts use the version-2 domain in the
    // custody pallet and therefore cannot collide with the dormant AMM version-1 subaccounts.
    pub const FounderCustodyPalletId: frame_support::PalletId = frame_support::PalletId(*b"era/vamm");
    pub FounderCustodySigners: [AccountId; 3] = [
        sp_runtime::AccountId32::new([
            0x48, 0xf1, 0x61, 0xfd, 0x46, 0x3c, 0x1b, 0xad, 0x4f, 0xb2, 0x61, 0x3f, 0x59, 0xc0, 0x60, 0x72,
            0x7e, 0xe5, 0x4e, 0x61, 0x1f, 0xe5, 0x31, 0x11, 0xee, 0x47, 0x62, 0xae, 0x42, 0x25, 0x37, 0x33,
        ]),
        sp_runtime::AccountId32::new([
            0xf0, 0xff, 0x6a, 0xee, 0xca, 0x8a, 0x9d, 0x00, 0x28, 0xf9, 0x06, 0xd6, 0xce, 0xee, 0xd2, 0x74,
            0x22, 0x1a, 0xb7, 0x39, 0x88, 0x38, 0x45, 0xee, 0xc8, 0x40, 0x01, 0x63, 0x0c, 0xe8, 0x16, 0x6a,
        ]),
        sp_runtime::AccountId32::new([
            0xfa, 0x97, 0x66, 0xa7, 0x14, 0x36, 0x32, 0x1f, 0x94, 0xc5, 0x3d, 0x04, 0x3c, 0x29, 0x47, 0x8f,
            0x54, 0xff, 0xa3, 0x2e, 0xc6, 0xb2, 0xd4, 0xc3, 0xd7, 0xc0, 0xee, 0xa4, 0x11, 0xc9, 0x10, 0x2b,
        ]),
    ];
}

impl era_v14_custody_governance::Config for Runtime {
    type Currency = Balances;
    type CustodyPalletId = FounderCustodyPalletId;
    type Signers = fresh_genesis::CustodySigners;
    type WeightInfo = era_v14_custody_governance::weights::SubstrateWeight<Runtime>;
}

// ===== V14 capped staking security budget =====
parameter_types! {
    pub const SecurityBudgetPalletId: frame_support::PalletId = frame_support::PalletId(*b"era/v14s");
    pub const SecurityBudgetMinimumValidatorBond: Balance = 10_000 * DECIMALS;
    pub const SecurityBudgetMaxValidatorCommission: Perbill = Perbill::from_percent(20);
    pub const SecurityBudgetTargetStakingApr: Perbill = Perbill::from_percent(10);
    pub const SecurityBudgetGrossAnnualCeiling: Balance = 6_000_000 * DECIMALS;
    pub const SecurityBudgetTotalStakingRewards: Balance = 20_000_000 * DECIMALS;
    pub const SecurityBudgetExistingEarnedRewardLiability: Balance =
        47_913_372_622_298_883_618_387;
    pub const SecurityBudgetProtectedCommunityOnboarding: Balance = 10_000_000 * DECIMALS;
    pub const SecurityBudgetRetiredLegacyRewardReserveTarget: Balance = 0;
    pub const SecurityBudgetMillisecondsPerYear: u64 = 31_556_952_000;
    pub const SecurityBudgetActivationValidatorCount: u32 = 4;
    pub const SecurityBudgetMaxExposurePages: u32 = 1;
}

pub struct RuntimeControlledIssuance;
impl pallet_security_budget::ControlledIssuance<AccountId> for RuntimeControlledIssuance {
    fn remaining_allowance() -> Option<Balance> {
        IssuanceCap::remaining_allowance()
    }

    fn mint_split(
        staking: &AccountId,
        staking_amount: Balance,
        treasury: &AccountId,
        treasury_amount: Balance,
        carry_before: u8,
        carry_after: u8,
    ) -> frame_support::dispatch::DispatchResult {
        IssuanceCap::controlled_mint_split(
            staking,
            staking_amount,
            treasury,
            treasury_amount,
            carry_before,
            carry_after,
        )?;
        Ok(())
    }

    #[cfg(feature = "runtime-benchmarks")]
    fn benchmark_set_remaining_allowance(amount: Balance) {
        issuance_cap::RemainingAllowance::<Runtime>::put(amount);
    }
}

impl pallet_security_budget::Config for Runtime {
    type Issuance = RuntimeControlledIssuance;
    type StakingPotPalletId = SecurityBudgetPalletId;
    type TreasuryPalletId = EcosystemTreasuryPalletId;
    type V14FeeCollectionPalletId = FeeCollectionPalletId;
    type MinimumValidatorBond = SecurityBudgetMinimumValidatorBond;
    type MaximumValidatorCommission = SecurityBudgetMaxValidatorCommission;
    type TargetStakingApr = SecurityBudgetTargetStakingApr;
    type GrossAnnualIssuanceCeiling = SecurityBudgetGrossAnnualCeiling;
    type TotalStakingRewardBudget = SecurityBudgetTotalStakingRewards;
    type ExistingEarnedRewardLiability = fresh_genesis::LegacyLiability;
    type ProtectedCommunityOnboardingBudget = SecurityBudgetProtectedCommunityOnboarding;
    type RetiredLegacyRewardReserveTarget = SecurityBudgetRetiredLegacyRewardReserveTarget;
    type SecurityMillisecondsPerYear = SecurityBudgetMillisecondsPerYear;
    type RuntimeTechnicalMaxValidators = MaxValidatorSet;
    type ActivationValidatorCount = SecurityBudgetActivationValidatorCount;
    type V14MaxRewardNominatorsPerPage = MaxElectionBackersPerWinner;
    type MaxExposurePages = SecurityBudgetMaxExposurePages;
    type ActivationOrigin = frame_system::EnsureRoot<AccountId>;
    type SecurityWeightInfo = pallet_security_budget::weights::SubstrateWeight<Runtime>;
}

// ===== Executive =====
pub type Executive = frame_executive::Executive<
    Runtime,
    Block,
    system::ChainContext<Runtime>,
    Runtime,
    v14_migration_lifecycle::MigrationLifecycle,
>;

// This pinned Executive's try-runtime path omits `System::SingleBlockMigrations`. Supply the
// same migration through its diagnostic-only custom slot; production keeps the normal five-
// parameter Executive above and therefore executes it exactly once through the system type.
#[cfg(feature = "try-runtime")]
pub(crate) type InnerTryRuntimeExecutive = frame_executive::Executive<
    Runtime,
    Block,
    system::ChainContext<Runtime>,
    Runtime,
    v14_migration_lifecycle::MigrationLifecycle,
    (v13_migration::V13Migration, v14_migration::V14Migration),
>;

#[cfg(feature = "try-runtime")]
pub use v14_migration_lifecycle::TryRuntimeExecutive;

// ===== Pallet inclusion =====
construct_runtime! {
    pub enum Runtime {
        System: frame_system = 0,
        Timestamp: pallet_timestamp::{Pallet, Call, Storage, Inherent} = 1,
        Balances: pallet_balances = 2,
        TransactionPayment: pallet_transaction_payment = 3,
        Sudo: pallet_sudo = 4,
        // Staking must build bonds before Session asks it for genesis elections/exposures.
        // Keep every existing SCALE pallet index explicit despite the corrected genesis order.
        Staking: pallet_staking = 10,
        Session: pallet_session::{Pallet, Call, Storage, Event<T>, Config<T>} = 5,
        Historical: pallet_session::historical::{Pallet, Event<T>} = 6,
        Babe: pallet_babe::{Pallet, Call, Storage, Config<T>, ValidateUnsigned} = 7,
        Grandpa: pallet_grandpa::{Pallet, Call, Storage, Config<T>, Event, ValidateUnsigned} = 8,
        Authorship: pallet_authorship = 9,
        Vesting: pallet_vesting = 11,
        AiPredictions: pallet_ai_predictions::{Pallet, Call, Storage, Event<T>} = 12,
        RewardReserve: pallet_reward_reserve::{Pallet, Call, Storage, Event<T>} = 13,
        Multisig: pallet_multisig::{Pallet, Call, Storage, Event<T>} = 14,
        Proxy: pallet_proxy::{Pallet, Call, Storage, Event<T>} = 15,
        Assets: pallet_assets::{Pallet, Call, Storage, Event<T>} = 16,
        Nfts: pallet_nfts::{Pallet, Call, Storage, Event<T>} = 17,
        EraWorlds: pallet_era_worlds::{Pallet, Call, Storage, Event<T>} = 18,
        IssuanceCap: issuance_cap::{Pallet, Storage, Event<T>} = 19,
        SecurityBudget: pallet_security_budget::{Pallet, Call, Storage, Event<T>} = 20,
        ValidatorSecurity: era_validator_security::observation::{Pallet, Storage, Event<T>} = 21,
        FounderCustody: era_v14_custody_governance::{Pallet, Call, Storage, Event<T>} = 22,
        FreshGenesis: fresh_genesis::{Pallet, Storage, Config<T>} = 23,
        Amm: v14_amm::{Pallet, Call, Storage, Event<T>} = 24,
        EraV14Amm: era_v14_amm::{Pallet, Storage} = 25,
        EquivocationPenalties: v14_penalties::{Pallet, Call, Storage, Event<T>} = 26,
        EraV14AssetAllocator: v14_allocator::{Pallet, Call, Storage, Event<T>} = 27,
    }
}

impl<C> frame_system::offchain::CreateTransactionBase<C> for Runtime
where
    RuntimeCall: From<C>,
{
    type RuntimeCall = RuntimeCall;
    type Extrinsic = UncheckedExtrinsic;
}

impl<C> frame_system::offchain::CreateBare<C> for Runtime
where
    RuntimeCall: From<C>,
{
    fn create_bare(call: Self::RuntimeCall) -> Self::Extrinsic {
        generic::UncheckedExtrinsic::new_bare(call)
    }
}

#[cfg(feature = "runtime-benchmarks")]
pub struct ObserverBenchmarkSessionManager;

#[cfg(feature = "runtime-benchmarks")]
impl pallet_session::SessionManager<AccountId> for ObserverBenchmarkSessionManager {
    fn new_session(_: sp_staking::SessionIndex) -> Option<Vec<AccountId>> {
        Some(Session::validators())
    }

    fn new_session_genesis(_: sp_staking::SessionIndex) -> Option<Vec<AccountId>> {
        Some(Session::validators())
    }

    fn start_session(_: sp_staking::SessionIndex) {}
    fn end_session(_: sp_staking::SessionIndex) {}
}

#[cfg(feature = "runtime-benchmarks")]
impl
    pallet_session::historical::SessionManager<
        AccountId,
        pallet_staking::Exposure<AccountId, Balance>,
    > for ObserverBenchmarkSessionManager
{
    fn new_session(
        _: sp_staking::SessionIndex,
    ) -> Option<Vec<(AccountId, pallet_staking::Exposure<AccountId, Balance>)>> {
        Some(
            Session::validators()
                .into_iter()
                .map(|validator| (validator, Default::default()))
                .collect(),
        )
    }

    fn new_session_genesis(
        _: sp_staking::SessionIndex,
    ) -> Option<Vec<(AccountId, pallet_staking::Exposure<AccountId, Balance>)>> {
        <Self as pallet_session::historical::SessionManager<_, _>>::new_session(0)
    }

    fn start_session(_: sp_staking::SessionIndex) {}
    fn end_session(_: sp_staking::SessionIndex) {}
}

#[cfg(feature = "runtime-benchmarks")]
pub struct RuntimeObserverBenchmarkHelper;

#[cfg(feature = "runtime-benchmarks")]
impl RuntimeObserverBenchmarkHelper {
    fn seed(label: &[u8], index: u32) -> [u8; 32] {
        let mut input = label.to_vec();
        input.extend_from_slice(&index.to_le_bytes());
        sp_io::hashing::blake2_256(&input)
    }

    fn prepare_validators(
        validator_count: u32,
        configured_max: u32,
    ) -> Result<(sp_core::sr25519::Public, sp_core::ed25519::Public), sp_runtime::DispatchError>
    {
        if validator_count == 0 || validator_count > configured_max {
            return Err(sp_runtime::DispatchError::Other(
                "validator count outside configured consensus maximum",
            ));
        }
        let mut validators = Vec::with_capacity(validator_count as usize);
        let mut first_babe = None;
        let mut first_grandpa = None;
        for index in 0..validator_count {
            let account = AccountId::new(Self::seed(b"r3b-account", index));
            #[cfg(test)]
            let babe = {
                use sp_core::Pair;
                sp_core::sr25519::Pair::from_string(&alloc::format!("//r3b-babe//{index}"), None)
                    .expect("static benchmark BABE test derivation")
                    .public()
            };
            #[cfg(not(test))]
            let babe = sp_io::crypto::sr25519_generate(
                sp_consensus_babe::KEY_TYPE,
                Some(alloc::format!("//r3b-babe//{index}").into_bytes()),
            );
            #[cfg(test)]
            let grandpa = {
                use sp_core::Pair;
                sp_core::ed25519::Pair::from_string(&alloc::format!("//r3b-grandpa//{index}"), None)
                    .expect("static benchmark GRANDPA test derivation")
                    .public()
            };
            #[cfg(not(test))]
            let grandpa = sp_io::crypto::ed25519_generate(
                sp_consensus_grandpa::KEY_TYPE,
                Some(alloc::format!("//r3b-grandpa//{index}").into_bytes()),
            );
            pallet_session::NextKeys::<Runtime>::insert(
                &account,
                SessionKeys {
                    babe: babe.into(),
                    grandpa: grandpa.into(),
                },
            );
            validators.push(account);
            if index == 0 {
                first_babe = Some(babe);
                first_grandpa = Some(grandpa);
            }
        }
        pallet_session::Validators::<Runtime>::put(validators);
        pallet_session::CurrentIndex::<Runtime>::put(0);
        pallet_babe::GenesisSlot::<Runtime>::put(sp_consensus_babe::Slot::from(0));
        pallet_babe::SkippedEpochs::<Runtime>::kill();
        pallet_grandpa::SetIdSession::<Runtime>::insert(0, 0);
        Ok((
            first_babe.expect("validated nonzero authority count"),
            first_grandpa.expect("validated nonzero authority count"),
        ))
    }

    fn retained_membership_proof<Key>(
        key: Key,
    ) -> Result<sp_session::MembershipProof, sp_runtime::DispatchError>
    where
        Key: Clone,
        Historical:
            frame_support::traits::KeyOwnerProofSystem<Key, Proof = sp_session::MembershipProof>,
    {
        use frame_support::traits::KeyOwnerProofSystem;
        use pallet_session::SessionManager;

        type HistoricalRootManager = pallet_session::historical::NoteHistoricalRoot<
            Runtime,
            ObserverBenchmarkSessionManager,
        >;
        <HistoricalRootManager as SessionManager<AccountId>>::new_session(0).ok_or(
            sp_runtime::DispatchError::Other("historical benchmark validator set"),
        )?;
        let proof = Historical::prove(key.clone()).ok_or(sp_runtime::DispatchError::Other(
            "historical benchmark membership proof",
        ))?;
        pallet_session::CurrentIndex::<Runtime>::put(1);
        if !pallet_session::historical::HistoricalSessions::<Runtime>::contains_key(0)
            || Historical::check_proof(key, proof.clone()).is_none()
        {
            return Err(sp_runtime::DispatchError::Other(
                "retained historical proof verification",
            ));
        }
        Ok(proof)
    }

    fn rehearsal(
        records: Vec<era_validator_security::observation::ObservationInput<AccountId>>,
    ) -> Result<(), sp_runtime::DispatchError> {
        let bounded = frame_support::BoundedVec::<_, ConstU32<16>>::try_from(records)
            .map_err(|_| sp_runtime::DispatchError::Other("benchmark batch bound"))?;
        ValidatorSecurity::record_rehearsal_batch(bounded)?;
        Ok(())
    }

    fn seed_expired(prune: u32) -> Result<(), sp_runtime::DispatchError> {
        use era_validator_security::observation::{ObservationInput, ObservationKind};
        use frame_support::traits::StorageVersion;

        StorageVersion::new(1).put::<ValidatorSecurity>();
        for chunk in 0..prune.div_ceil(16) {
            System::set_block_number(chunk + 1);
            let start = chunk * 16;
            let end = core::cmp::min(start + 16, prune);
            Self::rehearsal(
                (start..end)
                    .map(|index| ObservationInput {
                        kind: ObservationKind::Babe,
                        offender: AccountId::new(Self::seed(b"r3b-expired", index)),
                        session_index: 0,
                        babe_slot: Some(10_000 + index as u64),
                        grandpa_coordinate: None,
                        proof_hash: Self::seed(b"r3b-expired-proof", index),
                    })
                    .collect(),
            )?;
        }
        System::set_block_number(20_000);
        Ok(())
    }

    fn babe_header(
        public: &sp_core::sr25519::Public,
        slot: sp_consensus_babe::Slot,
        marker: u8,
    ) -> Header {
        use sp_consensus_babe::digests::{
            CompatibleDigestItem, PreDigest, SecondaryPlainPreDigest,
        };
        use sp_runtime::traits::Header as HeaderT;

        let digest = sp_runtime::Digest {
            logs: alloc::vec![sp_runtime::DigestItem::babe_pre_digest(
                PreDigest::SecondaryPlain(SecondaryPlainPreDigest {
                    authority_index: 0,
                    slot,
                }),
            )],
        };
        let mut header = <Header as HeaderT>::new(
            1,
            H256::repeat_byte(marker),
            H256::repeat_byte(marker.wrapping_add(1)),
            H256::zero(),
            digest,
        );
        let prehash = header.hash();
        #[cfg(test)]
        let signature = {
            use sp_core::Pair;
            let pair = sp_core::sr25519::Pair::from_string("//r3b-babe//0", None)
                .expect("static benchmark BABE test derivation");
            assert_eq!(&pair.public(), public);
            pair.sign(prehash.as_ref())
        };
        #[cfg(not(test))]
        let signature =
            sp_io::crypto::sr25519_sign(sp_consensus_babe::KEY_TYPE, public, prehash.as_ref())
                .expect("benchmark BABE key was inserted into the keystore");
        header
            .digest_mut()
            .push(sp_runtime::DigestItem::babe_seal(signature.into()));
        header
    }

    fn babe_evidence(
        validator_count: u32,
        prune: u32,
        valid: bool,
    ) -> Result<BabeObservationEvidence, sp_runtime::DispatchError> {
        let (babe, _) = Self::prepare_validators(validator_count, MaxBabeAuthorities::get())?;
        Self::seed_expired(prune)?;
        let slot = sp_consensus_babe::Slot::from(1);
        let first_header = Self::babe_header(&babe, slot, 1);
        let second_header = if valid {
            Self::babe_header(&babe, slot, 2)
        } else {
            first_header.clone()
        };
        let offender: sp_consensus_babe::AuthorityId = babe.into();
        let key_owner_proof =
            Self::retained_membership_proof((sp_consensus_babe::KEY_TYPE, offender.clone()))?;
        Ok((
            sp_consensus_babe::EquivocationProof {
                slot,
                offender,
                first_header,
                second_header,
            },
            key_owner_proof,
        ))
    }

    fn grandpa_evidence(
        validator_count: u32,
        prune: u32,
        valid: bool,
    ) -> Result<GrandpaObservationEvidence, sp_runtime::DispatchError> {
        let (_, grandpa) = Self::prepare_validators(validator_count, MaxGrandpaAuthorities::get())?;
        Self::seed_expired(prune)?;
        let set_id = 0;
        let round = 1;
        let first_vote = finality_grandpa::Prevote {
            target_hash: H256::repeat_byte(1),
            target_number: 1,
        };
        let second_vote = finality_grandpa::Prevote {
            target_hash: if valid {
                H256::repeat_byte(2)
            } else {
                H256::repeat_byte(1)
            },
            target_number: 1,
        };
        let sign = |vote: &finality_grandpa::Prevote<Hash, BlockNumber>| {
            let message = finality_grandpa::Message::Prevote(vote.clone());
            let payload = sp_consensus_grandpa::localized_payload(round, set_id, &message);
            #[cfg(test)]
            let signature = {
                use sp_core::Pair;
                let pair = sp_core::ed25519::Pair::from_string("//r3b-grandpa//0", None)
                    .expect("static benchmark GRANDPA test derivation");
                assert_eq!(pair.public(), grandpa);
                pair.sign(&payload)
            };
            #[cfg(not(test))]
            let signature =
                sp_io::crypto::ed25519_sign(sp_consensus_grandpa::KEY_TYPE, &grandpa, &payload)
                    .expect("benchmark GRANDPA key was inserted into the keystore");
            signature.into()
        };
        let identity: sp_consensus_grandpa::AuthorityId = grandpa.into();
        let proof = sp_consensus_grandpa::EquivocationProof::new(
            set_id,
            sp_consensus_grandpa::Equivocation::Prevote(finality_grandpa::Equivocation {
                round_number: round,
                identity: identity.clone(),
                first: (first_vote.clone(), sign(&first_vote)),
                second: (second_vote.clone(), sign(&second_vote)),
            }),
        );
        let key_owner_proof =
            Self::retained_membership_proof((sp_consensus_grandpa::KEY_TYPE, identity))?;
        Ok((proof, key_owner_proof))
    }

    fn private_batch(
        n: u32,
        unique_new: u32,
        prune: u32,
    ) -> Result<
        frame_support::BoundedVec<
            era_validator_security::observation::ObservationInput<AccountId>,
            ConstU32<16>,
        >,
        sp_runtime::DispatchError,
    > {
        use era_validator_security::observation::{ObservationInput, ObservationKind};

        if n == 0 || n > 16 || unique_new > n {
            return Err(sp_runtime::DispatchError::Other(
                "private batch parameters require 1 <= n <= 16 and unique_new <= n",
            ));
        }
        Self::seed_expired(prune)?;
        if unique_new == 0 {
            Self::rehearsal(alloc::vec![ObservationInput {
                kind: ObservationKind::Babe,
                offender: AccountId::new(Self::seed(b"r3b-existing", 0)),
                session_index: 1,
                babe_slot: Some(30_000),
                grandpa_coordinate: None,
                proof_hash: Self::seed(b"r3b-existing-proof", 0),
            }])?;
        }
        let records = (0..n)
            .map(|index| {
                let identity = if unique_new == 0 {
                    0
                } else {
                    index.min(unique_new - 1)
                };
                ObservationInput {
                    kind: ObservationKind::Babe,
                    offender: AccountId::new(Self::seed(
                        if unique_new == 0 {
                            b"r3b-existing"
                        } else {
                            b"r3b-batch"
                        },
                        identity,
                    )),
                    session_index: 1,
                    babe_slot: Some(if unique_new == 0 {
                        30_000
                    } else {
                        40_000 + identity as u64
                    }),
                    grandpa_coordinate: None,
                    proof_hash: Self::seed(b"r3b-batch-proof", index),
                }
            })
            .collect::<Vec<_>>();
        records
            .try_into()
            .map_err(|_| sp_runtime::DispatchError::Other("private batch bound"))
    }

    fn fill_capacity() -> Result<(), sp_runtime::DispatchError> {
        use era_validator_security::observation::{
            ObservationInput, ObservationKind, MAX_RETAINED_OBSERVATIONS,
        };
        use frame_support::traits::StorageVersion;

        StorageVersion::new(1).put::<ValidatorSecurity>();
        for chunk in 0..(MAX_RETAINED_OBSERVATIONS / 16) {
            System::set_block_number(chunk + 1);
            Self::rehearsal(
                (0..16)
                    .map(|offset| {
                        let index = chunk * 16 + offset;
                        ObservationInput {
                            kind: ObservationKind::Grandpa,
                            offender: AccountId::new(Self::seed(b"r3b-full", index)),
                            session_index: 1,
                            babe_slot: None,
                            grandpa_coordinate: Some((1, index as u64)),
                            proof_hash: Self::seed(b"r3b-full-proof", index),
                        }
                    })
                    .collect(),
            )?;
        }
        System::set_block_number(MAX_RETAINED_OBSERVATIONS / 16 + 1);
        Ok(())
    }

    fn state_commitment() -> [u8; 32] {
        use codec::Encode;
        use frame_support::traits::PalletInfoAccess;

        let prefix =
            sp_io::hashing::twox_128(<ValidatorSecurity as PalletInfoAccess>::name().as_bytes());
        let mut encoded = Vec::new();
        if let Some(value) = sp_io::storage::get(&prefix) {
            prefix.encode_to(&mut encoded);
            value.encode_to(&mut encoded);
        }
        let mut cursor = prefix.to_vec();
        while let Some(key) = sp_io::storage::next_key(&cursor) {
            if !key.starts_with(&prefix) {
                break;
            }
            cursor = key.clone();
            key.encode_to(&mut encoded);
            sp_io::storage::get(&key)
                .unwrap_or_default()
                .encode_to(&mut encoded);
        }
        sp_io::hashing::blake2_256(&encoded)
    }
}

#[cfg(feature = "runtime-benchmarks")]
impl era_validator_security::observation::BenchmarkHelper<Runtime>
    for RuntimeObserverBenchmarkHelper
{
    type BabeEvidence = BabeObservationEvidence;
    type GrandpaEvidence = GrandpaObservationEvidence;
    type PrivateBatch = frame_support::BoundedVec<
        era_validator_security::observation::ObservationInput<AccountId>,
        ConstU32<16>,
    >;

    fn babe_max_authorities() -> u32 {
        MaxBabeAuthorities::get()
    }

    fn grandpa_max_authorities() -> u32 {
        MaxGrandpaAuthorities::get()
    }

    fn setup_babe(
        validator_count: u32,
        max_nominators_per_validator: u32,
        prune: u32,
    ) -> Result<Self::BabeEvidence, sp_runtime::DispatchError> {
        if max_nominators_per_validator
            != <<Runtime as pallet_babe::Config>::MaxNominators as frame_support::traits::Get<
                u32,
            >>::get()
        {
            return Err(sp_runtime::DispatchError::Other(
                "BABE benchmark nominators must equal configured maximum",
            ));
        }
        Self::babe_evidence(validator_count, prune, true)
    }

    fn setup_grandpa(
        validator_count: u32,
        max_nominators_per_validator: u32,
        prune: u32,
    ) -> Result<Self::GrandpaEvidence, sp_runtime::DispatchError> {
        if max_nominators_per_validator
            != <<Runtime as pallet_grandpa::Config>::MaxNominators as frame_support::traits::Get<
                u32,
            >>::get()
        {
            return Err(sp_runtime::DispatchError::Other(
                "GRANDPA benchmark nominators must equal configured maximum",
            ));
        }
        Self::grandpa_evidence(validator_count, prune, true)
    }

    fn setup_invalid_babe(
        validator_count: u32,
    ) -> Result<Self::BabeEvidence, sp_runtime::DispatchError> {
        Self::babe_evidence(validator_count, 0, false)
    }

    fn setup_invalid_grandpa(
        validator_count: u32,
    ) -> Result<Self::GrandpaEvidence, sp_runtime::DispatchError> {
        Self::grandpa_evidence(validator_count, 0, false)
    }

    fn invalid_babe_error() -> sp_runtime::DispatchError {
        pallet_babe::Error::<Runtime>::InvalidEquivocationProof.into()
    }

    fn invalid_grandpa_error() -> sp_runtime::DispatchError {
        pallet_grandpa::Error::<Runtime>::InvalidEquivocationProof.into()
    }

    fn check_babe(
        evidence: &Self::BabeEvidence,
    ) -> Result<(), sp_runtime::transaction_validity::TransactionValidityError> {
        <BabeObserveOnlyReportSystem as
            sp_staking::offence::OffenceReportSystem<_, _>>::check_evidence(evidence.clone())
    }

    fn check_grandpa(
        evidence: &Self::GrandpaEvidence,
    ) -> Result<(), sp_runtime::transaction_validity::TransactionValidityError> {
        <GrandpaObserveOnlyReportSystem as
            sp_staking::offence::OffenceReportSystem<_, _>>::check_evidence(evidence.clone())
    }

    fn process_babe(evidence: Self::BabeEvidence) -> sp_runtime::DispatchResult {
        <BabeObserveOnlyReportSystem as
            sp_staking::offence::OffenceReportSystem<_, _>>::process_evidence(None, evidence)
    }

    fn process_grandpa(evidence: Self::GrandpaEvidence) -> sp_runtime::DispatchResult {
        <GrandpaObserveOnlyReportSystem as
            sp_staking::offence::OffenceReportSystem<_, _>>::process_evidence(None, evidence)
    }

    fn setup_private_batch(
        n: u32,
        unique_new: u32,
        prune: u32,
    ) -> Result<Self::PrivateBatch, sp_runtime::DispatchError> {
        Self::private_batch(n, unique_new, prune)
    }

    fn setup_capacity_full_babe(
        validator_count: u32,
    ) -> Result<Self::BabeEvidence, sp_runtime::DispatchError> {
        let evidence = Self::babe_evidence(validator_count, 0, true)?;
        Self::fill_capacity()?;
        Ok(evidence)
    }

    fn setup_capacity_full_grandpa(
        validator_count: u32,
    ) -> Result<Self::GrandpaEvidence, sp_runtime::DispatchError> {
        let evidence = Self::grandpa_evidence(validator_count, 0, true)?;
        Self::fill_capacity()?;
        Ok(evidence)
    }

    fn process_private_batch(
        batch: Self::PrivateBatch,
    ) -> Result<era_validator_security::observation::RecordOutcome, sp_runtime::DispatchError> {
        ValidatorSecurity::record_rehearsal_batch(batch).map_err(Into::into)
    }

    fn observer_state_commitment() -> [u8; 32] {
        Self::state_commitment()
    }

    fn observer_counters_and_transients() -> (
        u32,
        Option<(u32, u32)>,
        Option<era_validator_security::observation::EvidenceContext>,
        Option<u8>,
    ) {
        (
            era_validator_security::observation::ObservationCount::<Runtime>::get(),
            era_validator_security::observation::NewObservationsInBlock::<Runtime>::get(),
            era_validator_security::observation::PendingEvidenceContext::<Runtime>::get(),
            era_validator_security::observation::PendingAdapterError::<Runtime>::get(),
        )
    }

    fn observer_event_count() -> u32 {
        System::events()
            .iter()
            .filter(|record| matches!(&record.event, RuntimeEvent::ValidatorSecurity(_)))
            .count() as u32
    }
}

#[cfg(feature = "runtime-benchmarks")]
mod v14_evaluation_benchmarks;
#[cfg(feature = "runtime-benchmarks")]
mod v14_amm_benchmarks;
#[cfg(feature = "runtime-benchmarks")]
mod v14_penalty_benchmarks;
#[cfg(feature = "runtime-benchmarks")]
mod v14_allocator_benchmarks;
#[cfg(feature = "runtime-benchmarks")]
mod v14_asset_benchmarks;
#[cfg(feature = "runtime-benchmarks")]
mod v14_nft_benchmarks;

#[cfg(feature = "runtime-benchmarks")]
frame_benchmarking::define_benchmarks!(
    [pallet_ai_predictions, AiPredictions]
    [pallet_reward_reserve, RewardReserve]
    [pallet_security_budget, SecurityBudget]
    [era_v14_custody_governance, FounderCustody]
    [era_validator_security::observation, ValidatorSecurity]
    [pallet_nfts, Nfts]
    [v14_nft_benchmarks, v14_nft_benchmarks::Pallet<Runtime>]
    [v14_asset_benchmarks, v14_asset_benchmarks::Pallet<Runtime>]
    [v14_evaluation_benchmarks, v14_evaluation_benchmarks::Pallet<Runtime>]
    [v14_amm_benchmarks, v14_amm_benchmarks::Pallet<Runtime>]
    [v14_penalty_benchmarks, v14_penalty_benchmarks::Pallet<Runtime>]
    [v14_allocator_benchmarks, v14_allocator_benchmarks::Pallet<Runtime>]
    [pallet_era_worlds, EraWorlds]
);

// ===== Runtime APIs =====
impl_runtime_apis! {
    impl era_v14_application_primitives::assets::runtime_api::EraV14AssetsApiV1<Block> for Runtime {
        fn asset_v1(id:u32)->era_v14_application_primitives::assets::v1::ApiResult<era_v14_application_primitives::assets::v1::AssetV1> { <v14_allocator::Api as era_v14_application_primitives::assets::v1::EraV14AssetsApiV1>::asset_v1(id) }
        fn assets_v1(cursor:Option<u32>,limit:u32)->era_v14_application_primitives::assets::v1::ApiResult<era_v14_application_primitives::assets::v1::Page<era_v14_application_primitives::assets::v1::AssetV1>> { <v14_allocator::Api as era_v14_application_primitives::assets::v1::EraV14AssetsApiV1>::assets_v1(cursor,limit) }
        fn collection_v1(id:u32)->era_v14_application_primitives::assets::v1::ApiResult<era_v14_application_primitives::assets::v1::CollectionV1> { <v14_allocator::Api as era_v14_application_primitives::assets::v1::EraV14AssetsApiV1>::collection_v1(id) }
        fn collections_v1(cursor:Option<u32>,limit:u32)->era_v14_application_primitives::assets::v1::ApiResult<era_v14_application_primitives::assets::v1::Page<era_v14_application_primitives::assets::v1::CollectionV1>> { <v14_allocator::Api as era_v14_application_primitives::assets::v1::EraV14AssetsApiV1>::collections_v1(cursor,limit) }
        fn item_v1(collection:u32,id:u32)->era_v14_application_primitives::assets::v1::ApiResult<era_v14_application_primitives::assets::v1::ItemV1> { <v14_allocator::Api as era_v14_application_primitives::assets::v1::EraV14AssetsApiV1>::item_v1(collection,id) }
        fn items_v1(collection:u32,cursor:Option<u32>,limit:u32)->era_v14_application_primitives::assets::v1::ApiResult<era_v14_application_primitives::assets::v1::Page<era_v14_application_primitives::assets::v1::ItemV1>> { <v14_allocator::Api as era_v14_application_primitives::assets::v1::EraV14AssetsApiV1>::items_v1(collection,cursor,limit) }
    }
    impl era_v14_amm::EraV14AmmRuntimeApi<Block> for Runtime {
        fn pool_v1(pool:era_v14_amm::v1::PoolId)->era_v14_amm::v1::ApiResult<era_v14_amm::v1::PoolV1> { <era_v14_amm::v1::Api<Runtime> as era_v14_amm::v1::EraV14AmmApiV1>::pool_v1(pool) }
        fn pools_v1(cursor:Option<era_v14_amm::v1::PoolId>,limit:u32)->era_v14_amm::v1::ApiResult<era_v14_amm::v1::PageV1<era_v14_amm::v1::PoolV1>> { <era_v14_amm::v1::Api<Runtime> as era_v14_amm::v1::EraV14AmmApiV1>::pools_v1(cursor,limit) }
        fn lp_position_v1(pool:era_v14_amm::v1::PoolId,account:[u8;32])->era_v14_amm::v1::ApiResult<era_v14_amm::v1::LpPositionV1> { <era_v14_amm::v1::Api<Runtime> as era_v14_amm::v1::EraV14AmmApiV1>::lp_position_v1(pool,account) }
        fn positions_v1(account:[u8;32],cursor:Option<era_v14_amm::v1::PoolId>,limit:u32)->era_v14_amm::v1::ApiResult<era_v14_amm::v1::PageV1<era_v14_amm::v1::LpPositionV1>> { <era_v14_amm::v1::Api<Runtime> as era_v14_amm::v1::EraV14AmmApiV1>::positions_v1(account,cursor,limit) }
        fn quote_exact_input_v1(asset_in:era_v14_amm::v1::Asset,asset_out:era_v14_amm::v1::Asset,amount:u128)->era_v14_amm::v1::ApiResult<era_v14_amm::v1::QuoteV1> { <era_v14_amm::v1::Api<Runtime> as era_v14_amm::v1::EraV14AmmApiV1>::quote_exact_input_v1(asset_in,asset_out,amount) }
        fn quote_exact_output_v1(asset_in:era_v14_amm::v1::Asset,asset_out:era_v14_amm::v1::Asset,amount:u128)->era_v14_amm::v1::ApiResult<era_v14_amm::v1::QuoteV1> { <era_v14_amm::v1::Api<Runtime> as era_v14_amm::v1::EraV14AmmApiV1>::quote_exact_output_v1(asset_in,asset_out,amount) }
    }

    impl sp_session::SessionKeys<Block> for Runtime {
        fn generate_session_keys(seed: Option<Vec<u8>>) -> Vec<u8> {
            SessionKeys::generate(seed)
        }
        fn decode_session_keys(encoded: Vec<u8>) -> Option<Vec<(Vec<u8>, sp_core::crypto::KeyTypeId)>> {
            SessionKeys::decode_into_raw_public_keys(&encoded)
        }
    }

    // ------------ Core ------------
    impl sp_api::Core<Block> for Runtime {
        fn version() -> RuntimeVersion { VERSION }

        fn execute_block(block: Block) {
            Executive::execute_block(block);
        }

        fn initialize_block(header: &<Block as BlockT>::Header) -> sp_runtime::ExtrinsicInclusionMode {
            Executive::initialize_block(header)
        }
    }

    // ------------ Metadata ------------
    impl sp_api::Metadata<Block> for Runtime {
        fn metadata() -> OpaqueMetadata {
            OpaqueMetadata::new(Runtime::metadata().into())
        }

        fn metadata_at_version(version: u32) -> Option<OpaqueMetadata> {
            Runtime::metadata_at_version(version)
        }

        fn metadata_versions() -> alloc::vec::Vec<u32> {
            Runtime::metadata_versions()
        }
    }

    // ------------ Block Builder ------------
    impl sp_block_builder::BlockBuilder<Block> for Runtime {
        fn apply_extrinsic(extrinsic: <Block as BlockT>::Extrinsic) -> ApplyExtrinsicResult {
            Executive::apply_extrinsic(extrinsic)
        }
        fn finalize_block() -> <Block as BlockT>::Header {
            Executive::finalize_block()
        }
        fn inherent_extrinsics(data: sp_inherents::InherentData) -> Vec<<Block as BlockT>::Extrinsic> {
            data.create_extrinsics()
        }
        fn check_inherents(block: Block, data: sp_inherents::InherentData)
            -> sp_inherents::CheckInherentsResult
        {
            data.check_extrinsics(&block)
        }
    }

    // ------------ Tx Pool ------------
    impl sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block> for Runtime {
        fn validate_transaction(
            source: TransactionSource,
            tx: <Block as BlockT>::Extrinsic,
            block_hash: <Block as BlockT>::Hash,
        ) -> TransactionValidity {
            Executive::validate_transaction(source, tx, block_hash)
        }
    }

    // ------------ System RPC helpers ------------
    impl frame_system_rpc_runtime_api::AccountNonceApi<Block, AccountId, Index> for Runtime {
        fn account_nonce(account: AccountId) -> Index {
            system::Account::<Runtime>::get(account).nonce
        }
    }

    impl pallet_transaction_payment_rpc_runtime_api::TransactionPaymentApi<Block, Balance> for Runtime {
        fn query_info(uxt: <Block as BlockT>::Extrinsic, len: u32)
            -> pallet_transaction_payment_rpc_runtime_api::RuntimeDispatchInfo<Balance>
        {
            pallet_transaction_payment::Pallet::<Runtime>::query_info(uxt, len)
        }
        fn query_fee_details(uxt: <Block as BlockT>::Extrinsic, len: u32)
            -> pallet_transaction_payment_rpc_runtime_api::FeeDetails<Balance>
        {
            pallet_transaction_payment::Pallet::<Runtime>::query_fee_details(uxt, len)
        }
        fn query_weight_to_fee(weight: Weight) -> Balance {
            pallet_transaction_payment::Pallet::<Runtime>::weight_to_fee(weight)
        }
        fn query_length_to_fee(length: u32) -> Balance {
            pallet_transaction_payment::Pallet::<Runtime>::length_to_fee(length)
        }
    }

    // ------------ BABE API ------------
    impl sp_consensus_babe::BabeApi<Block> for Runtime {
        fn configuration() -> sp_consensus_babe::BabeConfiguration {
            let config = Babe::epoch_config().expect("BABE epoch configuration initialized at genesis");
            sp_consensus_babe::BabeConfiguration {
                slot_duration: Babe::slot_duration(),
                epoch_length: EpochDuration::get(),
                c: config.c,
                authorities: Babe::authorities().to_vec(),
                randomness: Babe::randomness(),
                allowed_slots: config.allowed_slots,
            }
        }
        fn current_epoch_start() -> sp_consensus_babe::Slot { Babe::current_epoch_start() }
        fn current_epoch() -> sp_consensus_babe::Epoch { Babe::current_epoch() }
        fn next_epoch() -> sp_consensus_babe::Epoch { Babe::next_epoch() }

        fn generate_key_ownership_proof(
            _slot: sp_consensus_babe::Slot,
            authority_id: sp_consensus_babe::AuthorityId,
        ) -> Option<sp_consensus_babe::OpaqueKeyOwnershipProof> {
            use codec::Encode;
            use frame_support::traits::KeyOwnerProofSystem;

            Historical::prove((sp_consensus_babe::KEY_TYPE, authority_id))
                .map(|proof| proof.encode())
                .map(sp_consensus_babe::OpaqueKeyOwnershipProof::new)
        }

        fn submit_report_equivocation_unsigned_extrinsic(
            proof: sp_consensus_babe::EquivocationProof<Header>,
            key_owner_proof: sp_consensus_babe::OpaqueKeyOwnershipProof,
        ) -> Option<()> {
            let key_owner_proof = key_owner_proof.decode()?;
            Babe::submit_unsigned_equivocation_report(proof, key_owner_proof)
        }
    }

    // ------------ GRANDPA API ------------
    impl sp_consensus_grandpa::GrandpaApi<Block> for Runtime {
        fn grandpa_authorities() -> sp_consensus_grandpa::AuthorityList {
            pallet_grandpa::Pallet::<Runtime>::grandpa_authorities()
        }

        fn current_set_id() -> sp_consensus_grandpa::SetId {
            pallet_grandpa::CurrentSetId::<Runtime>::get()
        }

        fn submit_report_equivocation_unsigned_extrinsic(
            equivocation_proof: sp_consensus_grandpa::EquivocationProof<
                <Block as sp_runtime::traits::Block>::Hash,
                sp_runtime::traits::NumberFor<Block>,
            >,
            key_owner_proof: sp_runtime::OpaqueValue,
        ) -> Option<()> {
            let key_owner_proof = key_owner_proof.decode()?;
            Grandpa::submit_unsigned_equivocation_report(
                equivocation_proof,
                key_owner_proof,
            )
        }

        fn generate_key_ownership_proof(
            _set_id: sp_consensus_grandpa::SetId,
            authority_id: sp_consensus_grandpa::AuthorityId,
        ) -> Option<sp_runtime::OpaqueValue> {
            use codec::Encode;
            use frame_support::traits::KeyOwnerProofSystem;

            Historical::prove((sp_consensus_grandpa::KEY_TYPE, authority_id))
                .map(|proof| proof.encode())
                .map(sp_runtime::OpaqueValue::new)
        }
    }

    #[cfg(feature = "runtime-benchmarks")]
    impl frame_benchmarking::Benchmark<Block> for Runtime {
        fn benchmark_metadata(extra: bool) -> (Vec<frame_benchmarking::BenchmarkList>, Vec<frame_support::traits::StorageInfo>) {
            use frame_benchmarking::BenchmarkList;
            use frame_support::traits::StorageInfoTrait;
            let mut list = Vec::<BenchmarkList>::new();
            list_benchmarks!(list, extra);
            (list, AllPalletsWithSystem::storage_info())
        }

        fn dispatch_benchmark(config: frame_benchmarking::BenchmarkConfig) -> Result<Vec<frame_benchmarking::BenchmarkBatch>, alloc::string::String> {
            use frame_benchmarking::BenchmarkBatch;
            use frame_support::traits::WhitelistedStorageKeys;
            let whitelist = AllPalletsWithSystem::whitelisted_storage_keys();
            let mut batches = Vec::<BenchmarkBatch>::new();
            let params = (&config, &whitelist);
            add_benchmarks!(params, batches);
            Ok(batches)
        }
    }

    #[cfg(feature = "try-runtime")]
    impl frame_try_runtime::TryRuntime<Block> for Runtime {
        fn on_runtime_upgrade(checks: frame_support::traits::UpgradeCheckSelect) -> (Weight, Weight) {
            let weight = TryRuntimeExecutive::try_runtime_upgrade(checks).expect("try-runtime upgrade checks failed");
            (weight, Weight::from_parts(u64::MAX, u64::MAX))
        }

        fn execute_block(block: Block, state_root_check: bool, signature_check: bool, select: frame_support::traits::TryStateSelect) -> Weight {
            Executive::try_execute_block(block, state_root_check, signature_check, select).expect("try-runtime execute-block failed")
        }
    }

    // ------------ Genesis builder passthrough (for chain spec tooling) ------------
    impl sp_genesis_builder::GenesisBuilder<Block> for Runtime {
        fn build_state(config: Vec<u8>) -> sp_genesis_builder::Result {
            build_state::<RuntimeGenesisConfig>(config)
        }
        fn get_preset(id: &Option<sp_genesis_builder::PresetId>) -> Option<Vec<u8>> {
            get_preset::<RuntimeGenesisConfig>(id, |_name| None)
        }
        fn preset_names() -> Vec<sp_genesis_builder::PresetId> {
            Vec::new()
        }
    }
}

#[sp_version::runtime_version]
pub const VERSION: RuntimeVersion = RuntimeVersion {
    spec_name: Cow::Borrowed("era"),
    impl_name: Cow::Borrowed("era"),
    authoring_version: 1,
    spec_version: 15,
    impl_version: 1,
    apis: RUNTIME_API_VERSIONS,
    transaction_version: 1,
    system_version: 1,
};
