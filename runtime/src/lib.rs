#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "std")]
include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));

extern crate alloc;

#[cfg(test)]
mod tests;

/// Persistent shared ETKN issuance allowance and its only production minting interface.
pub mod issuance_cap;

/// Decision-independent Upgrade 13 policy constants and fail-closed helpers.
///
/// This module remains pure; executable allowance enforcement lives in [`issuance_cap`].
pub mod upgrade13_policy;

/// Ordered, transactional V13 retained-supply and issuance-cap migration.
pub mod v13_migration;

use alloc::{borrow::Cow, vec::Vec};

use crate::sp_api_hidden_includes_construct_runtime::hidden_include::genesis_builder_helper::{
    build_state, get_preset,
};

use frame_support::{
    construct_runtime, parameter_types,
    traits::{
        fungible::{Balanced, Credit},
        AsEnsureOriginWithArg, ConstBool, ConstU128, ConstU32, Contains, Imbalance, InstanceFilter,
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
        let system_account = frame_support::storage::storage_prefix(b"System", b"Account");
        key.starts_with(&balances)
            || key.starts_with(&issuance_cap)
            || key.starts_with(&v13_input)
            || key.starts_with(&system_account)
    }

    fn raw_prefix_touches_issuance_state(prefix: &[u8]) -> bool {
        let balances = sp_io::hashing::twox_128(b"Balances");
        let issuance_cap = sp_io::hashing::twox_128(b"IssuanceCap");
        let v13_input = sp_io::hashing::twox_128(issuance_cap::V13_MIGRATION_INPUT_PALLET_PREFIX);
        let system_account = frame_support::storage::storage_prefix(b"System", b"Account");
        Self::prefix_overlaps(prefix, &balances)
            || Self::prefix_overlaps(prefix, &issuance_cap)
            || Self::prefix_overlaps(prefix, &v13_input)
            || Self::prefix_overlaps(prefix, &system_account)
    }

    pub(crate) fn is_uncontrolled_issuance(call: &RuntimeCall) -> bool {
        match call {
            RuntimeCall::Balances(pallet_balances::Call::force_set_balance { .. }) => true,
            RuntimeCall::Balances(pallet_balances::Call::force_adjust_total_issuance {
                direction: pallet_balances::AdjustmentDirection::Increase,
                ..
            }) => true,
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
    type SingleBlockMigrations = v13_migration::V13Migration;
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
impl pallet_transaction_payment::WeightInfo for FeeRoutingTransactionPaymentWeight {
    fn charge_transaction_payment() -> Weight {
        pallet_transaction_payment::weights::SubstrateWeight::<Runtime>::charge_transaction_payment(
        )
        // Conservative envelope for staging plus both measured settlement paths.
        .saturating_add(Weight::from_parts(10_000_000_000, 16_000))
        .saturating_add(RocksDbWeight::get().reads_writes(24, 20))
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
    type EquivocationReportSystem = ();
    type MaxAuthorities = MaxBabeAuthorities;
    type WeightInfo = ();
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
    type EquivocationReportSystem = ();
    type MaxAuthorities = MaxGrandpaAuthorities;
    type MaxSetIdSessionEntries = MaxSetIdSessionEntries;
    type WeightInfo = ();
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

    type Slash = ();
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
impl pallet_vesting::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type Currency = pallet_balances::Pallet<Runtime>;
    type BlockNumberToBalance = sp_runtime::traits::ConvertInto;
    type MinVestedTransfer = ConstU128<{ DECIMALS / 1_000 }>;
    type WeightInfo = ();
    type BlockNumberProvider = frame_system::Pallet<Runtime>;
    type UnvestedFundsAllowedWithdrawReasons = AllReasons;
    const MAX_VESTING_SCHEDULES: u32 = 128;
}

// ===== Candidate threshold custody primitives =====
//
// These pallets are deliberately appended at new indices and contain no production identities or
// monetary migration. The category filters remain fail-closed until category-specific calls exist
// and receive separate authorization.
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

/// Candidate notice periods at six seconds per block. Standard `pallet_proxy::create_pure` does
/// not enforce category-specific values, so a later provisioning procedure must verify them.
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
    type CreateOrigin = AsEnsureOriginWithArg<frame_system::EnsureSigned<AccountId>>;
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
    type WeightInfo = pallet_assets::weights::SubstrateWeight<Runtime>;
    #[cfg(feature = "runtime-benchmarks")]
    type BenchmarkHelper = ();
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
    pub NftFeatures: pallet_nfts::PalletFeatures = pallet_nfts::PalletFeatures::from_disabled(
        pallet_nfts::PalletFeature::Trading | pallet_nfts::PalletFeature::Swaps,
    );
}

impl pallet_nfts::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type CollectionId = u32;
    type ItemId = u32;
    type Currency = Balances;
    type ForceOrigin = frame_system::EnsureNever<AccountId>;
    type CreateOrigin = AsEnsureOriginWithArg<frame_system::EnsureSigned<AccountId>>;
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
    type WeightInfo = pallet_nfts::weights::SubstrateWeight<Runtime>;
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
    // RELEASE_BLOCKER: replace with generated benchmark weights before release.
    type WeightInfo = ();
}

// ===== AI Predictions =====
parameter_types! {
    pub const AiPredictionsPalletId: frame_support::PalletId = frame_support::PalletId(*b"era/aipr");
}

impl pallet_ai_predictions::Config for Runtime {
    type Currency = Balances;
    type PalletId = AiPredictionsPalletId;
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
    // The approved 20M is the gross pallet-account balance. The safety floor is inside it.
    pub const InitialRewardReserve: Balance = 20_000_000 * DECIMALS - EXISTENTIAL_DEPOSIT;
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
    type TreasurySpendOrigin = frame_system::EnsureRoot<AccountId>;
    type MaxRewardNominatorsPerPage = MaxElectionBackersPerWinner;
    type RewardWeightInfo = pallet_reward_reserve::weights::SubstrateWeight<Runtime>;
}

// ===== Executive =====
pub type Executive = frame_executive::Executive<
    Runtime,
    Block,
    system::ChainContext<Runtime>,
    Runtime,
    AllPalletsWithSystem,
>;

// This pinned Executive's try-runtime path omits `System::SingleBlockMigrations`. Supply the
// same migration through its diagnostic-only custom slot; production keeps the normal five-
// parameter Executive above and therefore executes it exactly once through the system type.
#[cfg(feature = "try-runtime")]
pub type TryRuntimeExecutive = frame_executive::Executive<
    Runtime,
    Block,
    system::ChainContext<Runtime>,
    Runtime,
    AllPalletsWithSystem,
    v13_migration::V13Migration,
>;

// ===== Pallet inclusion =====
construct_runtime! {
    pub enum Runtime {
        System: frame_system,
        // Expose the inherent so timestamp extrinsic exists
        Timestamp: pallet_timestamp::{Pallet, Call, Storage, Inherent},
        Balances: pallet_balances,
        TransactionPayment: pallet_transaction_payment,
        Sudo: pallet_sudo,

        Session: pallet_session::{Pallet, Call, Storage, Event<T>, Config<T>},
        Historical: pallet_session::historical::{Pallet, Event<T>},
          // ✅ BABE: DO NOT expose `Inherent` on this SDK line; keep Config<T> and ValidateUnsigned.
        Babe: pallet_babe::{Pallet, Call, Storage, Config<T>, ValidateUnsigned},

        // ✅ GRANDPA: include Config<T> *and* Event so RuntimeEvent: From<pallet_grandpa::Event> holds.
        Grandpa: pallet_grandpa::{Pallet, Call, Storage, Config<T>, Event},
        Authorship: pallet_authorship,

        Staking: pallet_staking,
        Vesting: pallet_vesting,
        AiPredictions: pallet_ai_predictions::{Pallet, Call, Storage, Event<T>},
        RewardReserve: pallet_reward_reserve::{Pallet, Call, Storage, Event<T>} = 13,
        Multisig: pallet_multisig::{Pallet, Call, Storage, Event<T>} = 14,
        Proxy: pallet_proxy::{Pallet, Call, Storage, Event<T>} = 15,
        Assets: pallet_assets::{Pallet, Call, Storage, Event<T>} = 16,
        Nfts: pallet_nfts::{Pallet, Call, Storage, Event<T>} = 17,
        EraWorlds: pallet_era_worlds::{Pallet, Call, Storage, Event<T>} = 18,
        IssuanceCap: issuance_cap::{Pallet, Storage, Event<T>} = 19,
    }
}

#[cfg(feature = "runtime-benchmarks")]
frame_benchmarking::define_benchmarks!(
    [pallet_ai_predictions, AiPredictions]
    [pallet_reward_reserve, RewardReserve]
    [pallet_assets, Assets]
    [pallet_nfts, Nfts]
);

// ===== Runtime APIs =====
impl_runtime_apis! {
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
            use sp_consensus_babe::{AllowedSlots, BabeConfiguration};

            let epoch_len: u64 = EpochDuration::get();

            BabeConfiguration {
                slot_duration: ExpectedBlockTime::get(),
                epoch_length: epoch_len,
                // c = primary slot probability = 1/4
                c: (1, 4),
                authorities: pallet_babe::Authorities::<Runtime>::get().to_vec(),
                randomness: pallet_babe::Randomness::<Runtime>::get(),
                allowed_slots: AllowedSlots::PrimaryAndSecondaryPlainSlots,
            }
        }

        fn current_epoch_start() -> sp_consensus_babe::Slot {
            use sp_runtime::traits::SaturatedConversion;

            let curr = pallet_babe::CurrentSlot::<Runtime>::get();
            let len: u64 = EpochDuration::get();

            let curr_u64: u64 = curr.saturated_into();
            let start_u64 = curr_u64 - (curr_u64 % len);
            sp_consensus_babe::Slot::from(start_u64)
        }

        fn current_epoch() -> sp_consensus_babe::Epoch {
            use sp_consensus_babe::{AllowedSlots, BabeEpochConfiguration, Epoch, Slot};
            use sp_runtime::traits::SaturatedConversion;

            let curr: Slot = pallet_babe::CurrentSlot::<Runtime>::get();
            let len: u64 = EpochDuration::get();

            let curr_u64: u64 = curr.saturated_into();
            let start_u64 = curr_u64 - (curr_u64 % len);
            let start_slot = Slot::from(start_u64);

            Epoch {
                start_slot,
                duration: len,
                authorities: pallet_babe::Authorities::<Runtime>::get().to_vec(),
                randomness: pallet_babe::Randomness::<Runtime>::get(),
                // v1.19.1 expects a concrete config (not Option)
                config: BabeEpochConfiguration {
                    c: (1, 4),
                    allowed_slots: AllowedSlots::PrimaryAndSecondaryPlainSlots,
                },
                epoch_index: 0,
            }
        }

        fn next_epoch() -> sp_consensus_babe::Epoch {
            use sp_consensus_babe::{AllowedSlots, BabeEpochConfiguration, Epoch, Slot};
            use sp_runtime::traits::SaturatedConversion;

            let curr: Slot = pallet_babe::CurrentSlot::<Runtime>::get();
            let len: u64 = EpochDuration::get();

            let curr_u64: u64 = curr.saturated_into();
            let start_u64 = curr_u64 - (curr_u64 % len);
            let start_next = Slot::from(start_u64 + len);

            Epoch {
                start_slot: start_next,
                duration: len,
                authorities: pallet_babe::Authorities::<Runtime>::get().to_vec(),
                randomness: pallet_babe::Randomness::<Runtime>::get(),
                config: BabeEpochConfiguration {
                    c: (1, 4),
                    allowed_slots: AllowedSlots::PrimaryAndSecondaryPlainSlots,
                },
                epoch_index: 1,
            }
        }

        fn generate_key_ownership_proof(
            _slot: sp_consensus_babe::Slot,
            _authority_id: sp_consensus_babe::AuthorityId,
        ) -> Option<sp_consensus_babe::OpaqueKeyOwnershipProof> {
            None
        }

        fn submit_report_equivocation_unsigned_extrinsic(
            _proof: sp_consensus_babe::EquivocationProof<Header>,
            _key_owner_proof: sp_consensus_babe::OpaqueKeyOwnershipProof,
        ) -> Option<()> {
            None
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
            _equivocation_proof: sp_consensus_grandpa::EquivocationProof<
                <Block as sp_runtime::traits::Block>::Hash,
                sp_runtime::traits::NumberFor<Block>,
            >,
            _key_owner_proof: sp_runtime::OpaqueValue,
        ) -> Option<()> {
            None
        }

        fn generate_key_ownership_proof(
            _set_id: sp_consensus_grandpa::SetId,
            _authority_id: sp_consensus_grandpa::AuthorityId,
        ) -> Option<sp_runtime::OpaqueValue> {
            None
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
    spec_version: 13,
    impl_version: 1,
    apis: RUNTIME_API_VERSIONS,
    transaction_version: 1,
    system_version: 1,
};
