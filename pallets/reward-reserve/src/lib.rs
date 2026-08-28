//! ERA transfer-only staking reward reserve.
//!
//! This pallet deliberately does not use classic staking's mint-backed payout path. It reads
//! finalized staking reward points, validator preferences, and paged exposures, then transfers
//! existing funds from a deterministic keyless pallet account. Standard `Staking::EraPaid`
//! remains zero and explorers must use this pallet's storage and events.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub use pallet::*;
pub mod weights;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::pallet_prelude::*;
use scale_info::TypeInfo;
use sp_runtime::RuntimeDebug;

/// Public accounting source for funds observed entering the reward pot.
#[derive(
    Encode,
    Decode,
    DecodeWithMemTracking,
    Clone,
    Copy,
    PartialEq,
    Eq,
    RuntimeDebug,
    TypeInfo,
    MaxEncodedLen,
)]
pub enum FundingSource {
    /// A separately authorized transfer of existing reserve funds.
    ReserveTransfer,
    /// A corrected normal transaction-fee contribution under the active 70/30 router.
    TransactionFee,
}

/// Why RewardReserve permanently skipped an era without creating a liability.
#[derive(
    Encode,
    Decode,
    DecodeWithMemTracking,
    Clone,
    Copy,
    PartialEq,
    Eq,
    RuntimeDebug,
    TypeInfo,
    MaxEncodedLen,
)]
pub enum RewardEraSkipReason {
    PartialPointSourceEra,
    NoRewardPoints,
    AllocationPaused,
}

/// Public reason for releasing a proven, pre-Spec12 zero-point liability.
#[derive(
    Encode,
    Decode,
    DecodeWithMemTracking,
    Clone,
    Copy,
    PartialEq,
    Eq,
    RuntimeDebug,
    TypeInfo,
    MaxEncodedLen,
)]
pub enum LiabilityCancellationReason {
    NoRewardPoints,
}

/// Complete post-correction fee-routing configuration.
#[derive(
    Encode,
    Decode,
    DecodeWithMemTracking,
    Clone,
    Copy,
    PartialEq,
    Eq,
    RuntimeDebug,
    TypeInfo,
    MaxEncodedLen,
)]
pub struct FeeRoutingConfiguration {
    pub reward_pot_percent: u8,
    pub ecosystem_treasury_percent: u8,
    pub burn_percent: u8,
    pub tip_to_author_percent: u8,
}

/// Public classification for a routing failure that used a safe fallback destination.
#[derive(
    Encode,
    Decode,
    DecodeWithMemTracking,
    Clone,
    Copy,
    PartialEq,
    Eq,
    RuntimeDebug,
    TypeInfo,
    MaxEncodedLen,
)]
pub enum FeeRoutingFailure {
    InvalidConfiguration,
    DestinationsNotReady,
    CollectionPotResolution,
    RewardPotResolution,
    EcosystemTreasuryResolution,
    MissingBlockAuthor,
    AuthorTipResolution,
    PendingRoutingInconsistent,
}

/// Public reason recorded when a Root activation request fails a prerequisite.
#[derive(
    Encode,
    Decode,
    DecodeWithMemTracking,
    Clone,
    Copy,
    PartialEq,
    Eq,
    RuntimeDebug,
    TypeInfo,
    MaxEncodedLen,
)]
pub enum ActivationRejection {
    AlreadyActivated,
    RewardPotMissing,
    RewardPotBelowExistentialDeposit,
    RewardPotBelowActivationReserve,
    TreasuryPotMissing,
    TreasuryPotBelowExistentialDeposit,
    FeeCollectionPotMissing,
    FeeCollectionPotBelowExistentialDeposit,
    InvalidFeeConfiguration,
    PendingRoutingInconsistent,
    ActivationStateInvalid,
}

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use crate::weights::WeightInfo as _;
    use alloc::vec::Vec;
    use frame_support::{
        storage::with_storage_layer,
        traits::{
            fungible::{Inspect, Mutate},
            Get, StorageVersion,
        },
        transactional, PalletId,
    };
    use frame_system::pallet_prelude::*;
    use sp_core::U256;
    use sp_runtime::{
        traits::{
            AccountIdConversion, CheckedAdd, CheckedSub, SaturatedConversion, Saturating, Zero,
        },
        Perbill,
    };
    use sp_staking::{EraIndex, Page};

    pub type BalanceOf<T> = <<T as pallet_staking::Config>::Currency as Inspect<
        <T as frame_system::Config>::AccountId,
    >>::Balance;

    #[cfg(feature = "try-runtime")]
    type PostUpgradeState<T> = (
        StorageVersion,
        StorageVersion,
        BalanceOf<T>,
        BalanceOf<T>,
        BalanceOf<T>,
        BalanceOf<T>,
        bool,
        bool,
        bool,
        (
            bool,
            bool,
            Option<BlockNumberFor<T>>,
            Option<EraIndex>,
            Option<EraIndex>,
            BalanceOf<T>,
            BalanceOf<T>,
            BalanceOf<T>,
            BalanceOf<T>,
            BalanceOf<T>,
            BalanceOf<T>,
            bool,
            Option<u64>,
            Option<FeeRoutingConfiguration>,
            Option<pallet_staking::ActiveEraInfo>,
        ),
    );

    const STORAGE_VERSION: StorageVersion = StorageVersion::new(2);

    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    #[pallet::config]
    pub trait Config:
        frame_system::Config<RuntimeEvent: From<Event<Self>>> + pallet_staking::Config
    {
        /// Deterministic keyless reward-pot identifier.
        #[pallet::constant]
        type RewardPalletId: Get<PalletId>;

        /// Maximum reward liability allocated during one budget year.
        #[pallet::constant]
        type AnnualRewardCap: Get<BalanceOf<Self>>;

        /// Milliseconds in the deterministic budget year.
        #[pallet::constant]
        type MillisecondsPerYear: Get<u64>;

        /// Validator commission ceiling applied by this reward system.
        #[pallet::constant]
        type MaxValidatorCommission: Get<Perbill>;

        /// Low-reserve warning threshold measured as uncommitted spendable balance.
        #[pallet::constant]
        type LowReserveThreshold: Get<BalanceOf<Self>>;

        /// Deterministic keyless protocol treasury identifier.
        #[pallet::constant]
        type EcosystemTreasuryPalletId: Get<PalletId>;

        /// Deterministic keyless staging account for corrected fees and deferred tips.
        #[pallet::constant]
        type FeeCollectionPalletId: Get<PalletId>;

        /// Available Reward Pot balance required before explicit activation.
        #[pallet::constant]
        type InitialRewardReserve: Get<BalanceOf<Self>>;

        /// Reward Pot floor, inclusive of the existential-deposit requirement.
        #[pallet::constant]
        type RewardPotSafetyFloor: Get<BalanceOf<Self>>;

        /// Treasury Pot floor, inclusive of the existential-deposit requirement.
        #[pallet::constant]
        type TreasuryPotSafetyFloor: Get<BalanceOf<Self>>;

        #[pallet::constant]
        type InitialRewardPotFeeShare: Get<u8>;
        #[pallet::constant]
        type InitialEcosystemTreasuryFeeShare: Get<u8>;
        #[pallet::constant]
        type InitialBurnFeeShare: Get<u8>;
        #[pallet::constant]
        type InitialTipToAuthorShare: Get<u8>;
        #[pallet::constant]
        type MaxRewardPotFeeShare: Get<u8>;
        #[pallet::constant]
        type MaxEcosystemTreasuryFeeShare: Get<u8>;
        #[pallet::constant]
        type MaxBurnFeeShare: Get<u8>;

        /// Root today; replaceable with an approved governance or multisig origin later.
        type TreasurySpendOrigin: EnsureOrigin<Self::RuntimeOrigin>;

        /// Maximum nominators this reward pallet will transfer to in one page.
        #[pallet::constant]
        type MaxRewardNominatorsPerPage: Get<u32>;

        /// Dispatchable weights.
        type RewardWeightInfo: weights::WeightInfo;
    }

    /// Explicit activation flag. Migration always leaves this false.
    #[pallet::storage]
    #[pallet::getter(fn reward_system_active)]
    pub type RewardSystemActive<T> = StorageValue<_, bool, ValueQuery>;

    /// Corrected 70/30 routing is operational only after the same atomic activation.
    #[pallet::storage]
    #[pallet::getter(fn fee_routing_active)]
    pub type FeeRoutingActive<T> = StorageValue<_, bool, ValueQuery>;

    /// Block at which Root completed the activation gate.
    #[pallet::storage]
    #[pallet::getter(fn activation_block)]
    pub type ActivationBlock<T: Config> = StorageValue<_, BlockNumberFor<T>, OptionQuery>;

    /// Era active when Root explicitly activates the reward system.
    #[pallet::storage]
    #[pallet::getter(fn activation_era)]
    pub type ActivationEra<T> = StorageValue<_, EraIndex, OptionQuery>;

    /// First complete era after the point source became operational.
    #[pallet::storage]
    #[pallet::getter(fn first_eligible_era)]
    pub type FirstEligibleEra<T> = StorageValue<_, EraIndex, OptionQuery>;

    /// Era during which the production reward-point source became operational.
    #[pallet::storage]
    #[pallet::getter(fn point_source_operational_era)]
    pub type PointSourceOperationalEra<T> = StorageValue<_, EraIndex, OptionQuery>;

    /// Inclusive range of pre-Spec12 eras eligible for guarded zero-point cancellation.
    #[pallet::storage]
    #[pallet::getter(fn legacy_zero_point_era_range)]
    pub type LegacyZeroPointEraRange<T> = StorageValue<_, (EraIndex, EraIndex), OptionQuery>;

    /// Budget period captured with the legacy zero-point range.
    #[pallet::storage]
    pub type LegacyLiabilityBudgetPeriodStartMillis<T> = StorageValue<_, u64, OptionQuery>;

    /// Last active era observed by the pallet.
    #[pallet::storage]
    pub type LastObservedEra<T> = StorageValue<_, EraIndex, OptionQuery>;

    /// Start timestamp recorded by staking for the last observed era.
    #[pallet::storage]
    pub type LastObservedEraStartMillis<T> = StorageValue<_, u64, OptionQuery>;

    /// Completed era waiting for the next era's staking start timestamp.
    #[pallet::storage]
    pub type PendingCompletedEra<T> = StorageValue<_, (EraIndex, u64), OptionQuery>;

    /// Finalized maximum liability for each eligible era.
    #[pallet::storage]
    #[pallet::getter(fn era_reward_budget)]
    pub type EraRewardBudgets<T: Config> =
        StorageMap<_, Blake2_128Concat, EraIndex, BalanceOf<T>, OptionQuery>;

    /// Amount actually transferred for each era.
    #[pallet::storage]
    #[pallet::getter(fn era_reward_paid)]
    pub type EraRewardPaid<T: Config> =
        StorageMap<_, Blake2_128Concat, EraIndex, BalanceOf<T>, ValueQuery>;

    /// Funded but unpaid liability remaining for each era.
    #[pallet::storage]
    #[pallet::getter(fn era_reward_liability)]
    pub type EraRewardLiabilities<T: Config> =
        StorageMap<_, Blake2_128Concat, EraIndex, BalanceOf<T>, ValueQuery>;

    /// Exact custom reward pages already claimed.
    #[pallet::storage]
    pub type ClaimedRewardPages<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        EraIndex,
        Blake2_128Concat,
        (T::AccountId, Page),
        (),
        OptionQuery,
    >;

    /// Eras whose claim window has closed and whose unclaimed liability was released.
    #[pallet::storage]
    pub type ExpiredEras<T> = StorageMap<_, Blake2_128Concat, EraIndex, (), OptionQuery>;

    /// Eras deliberately skipped before any liability or annual-budget consumption.
    #[pallet::storage]
    #[pallet::getter(fn skipped_reward_era)]
    pub type SkippedRewardEras<T> =
        StorageMap<_, Blake2_128Concat, EraIndex, RewardEraSkipReason, OptionQuery>;

    /// Proven pre-Spec12 zero-point liabilities released by a separately authorized Root call.
    #[pallet::storage]
    #[pallet::getter(fn cancelled_zero_point_liability)]
    pub type CancelledZeroPointLiabilities<T: Config> =
        StorageMap<_, Blake2_128Concat, EraIndex, BalanceOf<T>, OptionQuery>;

    /// Aggregate funded liabilities across all claimable eras.
    #[pallet::storage]
    #[pallet::getter(fn committed_liabilities)]
    pub type CommittedLiabilities<T: Config> = StorageValue<_, BalanceOf<T>, ValueQuery>;

    /// Start of the current annual budget window.
    #[pallet::storage]
    pub type BudgetPeriodStartMillis<T> = StorageValue<_, u64, OptionQuery>;

    /// Budget allocated in the current annual window.
    #[pallet::storage]
    #[pallet::getter(fn annual_budget_used)]
    pub type AnnualBudgetUsed<T: Config> = StorageValue<_, BalanceOf<T>, ValueQuery>;

    /// Cursor used to release at most one expired era per block.
    #[pallet::storage]
    pub type NextExpiryEra<T> = StorageValue<_, EraIndex, OptionQuery>;

    /// Last observed total pot balance, used only for public funding-source accounting.
    #[pallet::storage]
    pub type LastPotBalance<T: Config> = StorageValue<_, BalanceOf<T>, OptionQuery>;

    /// Last observed treasury balance, preventing duplicate fee accounting.
    #[pallet::storage]
    pub type LastTreasuryBalance<T: Config> = StorageValue<_, BalanceOf<T>, OptionQuery>;

    /// Intended normal-fee and tip routing configuration; dormant until activation.
    #[pallet::storage]
    #[pallet::getter(fn stored_fee_routing_configuration)]
    pub type FeeRoutingConfig<T> = StorageValue<_, FeeRoutingConfiguration, OptionQuery>;

    /// Reward share collected but not yet transferred from the Fee Collection Pot.
    #[pallet::storage]
    #[pallet::getter(fn pending_reward_fee)]
    pub type PendingRewardFee<T: Config> = StorageValue<_, BalanceOf<T>, ValueQuery>;

    /// Treasury share collected but not yet transferred from the Fee Collection Pot.
    #[pallet::storage]
    #[pallet::getter(fn pending_treasury_fee)]
    pub type PendingTreasuryFee<T: Config> = StorageValue<_, BalanceOf<T>, ValueQuery>;

    /// Aggregate author tips retained in the Fee Collection Pot pending delivery.
    #[pallet::storage]
    #[pallet::getter(fn pending_tip_total)]
    pub type PendingTipTotal<T: Config> = StorageValue<_, BalanceOf<T>, ValueQuery>;

    /// Deferred tips keyed by the block author to whom they remain owed.
    #[pallet::storage]
    #[pallet::getter(fn pending_author_tip)]
    pub type PendingAuthorTips<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, BalanceOf<T>, ValueQuery>;

    /// Tips collected from a block with no determinable author; held, never burned.
    #[pallet::storage]
    #[pallet::getter(fn pending_unassigned_tip)]
    pub type PendingUnassignedTip<T: Config> = StorageValue<_, BalanceOf<T>, ValueQuery>;

    /// Existing-balance contributions observed from reserve transfers.
    #[pallet::storage]
    #[pallet::getter(fn reserve_funding_total)]
    pub type ReserveFundingTotal<T: Config> = StorageValue<_, BalanceOf<T>, ValueQuery>;

    /// Corrected normal-fee contributions resolved into the reward pot.
    #[pallet::storage]
    #[pallet::getter(fn fee_contribution_total)]
    pub type FeeContributionTotal<T: Config> = StorageValue<_, BalanceOf<T>, ValueQuery>;

    /// Normal fees routed to the deterministic Ecosystem Treasury pot.
    #[pallet::storage]
    #[pallet::getter(fn ecosystem_treasury_fee_total)]
    pub type EcosystemTreasuryFeeTotal<T: Config> = StorageValue<_, BalanceOf<T>, ValueQuery>;

    /// Total corrected normal fees routed without burning.
    #[pallet::storage]
    #[pallet::getter(fn normal_fee_routed_total)]
    pub type NormalFeeRoutedTotal<T: Config> = StorageValue<_, BalanceOf<T>, ValueQuery>;

    /// Tips successfully resolved to block authors.
    #[pallet::storage]
    #[pallet::getter(fn author_tip_total)]
    pub type AuthorTipTotal<T: Config> = StorageValue<_, BalanceOf<T>, ValueQuery>;

    /// Whether new era allocations are currently paused for reserve insufficiency.
    #[pallet::storage]
    #[pallet::getter(fn allocation_paused)]
    pub type AllocationPaused<T> = StorageValue<_, bool, ValueQuery>;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        RewardSystemActivated {
            block: BlockNumberFor<T>,
            activation_era: EraIndex,
            first_eligible_era: EraIndex,
            reward_pot_balance: BalanceOf<T>,
        },
        RewardActivationRejected {
            reason: ActivationRejection,
        },
        FeeRoutingActivated {
            block: BlockNumberFor<T>,
            configuration: FeeRoutingConfiguration,
        },
        RewardPotFundingRecorded {
            source: FundingSource,
            amount: BalanceOf<T>,
            total: BalanceOf<T>,
        },
        EraRewardBudgetFinalized {
            era: EraIndex,
            duration_millis: u64,
            target: BalanceOf<T>,
            budget: BalanceOf<T>,
            annual_budget_used: BalanceOf<T>,
        },
        EraRewardLiabilityReserved {
            era: EraIndex,
            amount: BalanceOf<T>,
            committed_liabilities: BalanceOf<T>,
        },
        EraRewardSkippedNoPoints {
            era: EraIndex,
            calculated_budget: BalanceOf<T>,
        },
        EraRewardSkippedPartialPointSource {
            era: EraIndex,
        },
        EraRewardSkippedAllocationPaused {
            era: EraIndex,
            calculated_budget: BalanceOf<T>,
        },
        EraRewardLiabilityCancelled {
            era: EraIndex,
            amount: BalanceOf<T>,
            reason: LiabilityCancellationReason,
        },
        ValidatorRewardClaimed {
            era: EraIndex,
            page: Page,
            validator: T::AccountId,
            amount: BalanceOf<T>,
            commission: BalanceOf<T>,
        },
        NominatorRewardClaimed {
            era: EraIndex,
            page: Page,
            validator: T::AccountId,
            nominator: T::AccountId,
            amount: BalanceOf<T>,
        },
        EraRewardPagePaid {
            era: EraIndex,
            page: Page,
            validator: T::AccountId,
            caller: T::AccountId,
            amount: BalanceOf<T>,
            remaining_liability: BalanceOf<T>,
        },
        FeeContributionReceived {
            amount: BalanceOf<T>,
            total: BalanceOf<T>,
        },
        TransactionFeeRouted {
            normal_fee: BalanceOf<T>,
            reward_pot_contribution: BalanceOf<T>,
            ecosystem_treasury_contribution: BalanceOf<T>,
            burned: BalanceOf<T>,
            tip: BalanceOf<T>,
            author: Option<T::AccountId>,
        },
        RewardPotFeeContribution {
            amount: BalanceOf<T>,
            total: BalanceOf<T>,
        },
        EcosystemTreasuryFeeContribution {
            amount: BalanceOf<T>,
            total: BalanceOf<T>,
        },
        FeeRoutingConfigurationChanged {
            old: FeeRoutingConfiguration,
            new: FeeRoutingConfiguration,
        },
        FeeRoutingDeferred {
            normal_fee: BalanceOf<T>,
            pending_reward: BalanceOf<T>,
            pending_treasury: BalanceOf<T>,
        },
        FeeRoutingCompleted {
            normal_fee: BalanceOf<T>,
            reward_pot_contribution: BalanceOf<T>,
            ecosystem_treasury_contribution: BalanceOf<T>,
        },
        FeeRoutingFailed {
            normal_fee: BalanceOf<T>,
            tip: BalanceOf<T>,
            reason: FeeRoutingFailure,
        },
        AuthorTipDeferred {
            author: Option<T::AccountId>,
            amount: BalanceOf<T>,
        },
        AuthorTipCompleted {
            author: T::AccountId,
            amount: BalanceOf<T>,
        },
        EcosystemTreasurySpent {
            beneficiary: T::AccountId,
            amount: BalanceOf<T>,
        },
        RewardReserveLow {
            available: BalanceOf<T>,
            committed_liabilities: BalanceOf<T>,
        },
        RewardPotBelowMinimum {
            balance: BalanceOf<T>,
            required: BalanceOf<T>,
        },
        TreasuryPotBelowMinimum {
            balance: BalanceOf<T>,
            required: BalanceOf<T>,
        },
        RewardAllocationPaused {
            era: EraIndex,
            available: BalanceOf<T>,
        },
        RewardAllocationResumed {
            era: EraIndex,
            available: BalanceOf<T>,
        },
        ClaimExpired {
            era: EraIndex,
            released_liability: BalanceOf<T>,
        },
        EraRoundingRemainderReleased {
            era: EraIndex,
            amount: BalanceOf<T>,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        NotActivated,
        AlreadyActivated,
        RewardPotMissing,
        RewardPotBelowExistentialDeposit,
        RewardPotBelowActivationReserve,
        TreasuryPotMissing,
        TreasuryPotBelowExistentialDeposit,
        FeeCollectionPotMissing,
        FeeCollectionPotBelowExistentialDeposit,
        PendingRoutingInconsistent,
        ActivationStateInvalid,
        FeeRoutingUnavailable,
        EraNotEligible,
        EraNotFinalized,
        EraAlreadyFinalized,
        ClaimExpired,
        AlreadyClaimed,
        ValidatorNotRewarded,
        InvalidRewardPoints,
        RewardEraSkipped,
        RewardEraCancelled,
        InvalidLegacyZeroPointEra,
        LegacyLiabilityPeriodChanged,
        LiabilityAlreadyCancelled,
        LiabilityAmountMismatch,
        LiabilityAlreadyPaid,
        LiabilityHasClaims,
        InvalidExposure,
        InvalidPage,
        TooManyRewardRecipients,
        ArithmeticOverflow,
        InsufficientRewardPot,
        InsufficientEraLiability,
        RewardTransferFailed,
        IssuanceInvariantViolated,
        NoFundingIncrease,
        InvalidFeeRoutingConfiguration,
        TreasurySpendFailed,
        InvalidTreasuryBeneficiary,
        ZeroAmount,
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        fn integrity_test() {
            assert!(
                T::MillisecondsPerYear::get() > 0,
                "reward budget year must be nonzero"
            );
            assert!(
                !T::InitialRewardReserve::get().is_zero(),
                "initial reward reserve must be nonzero"
            );
            let minimum = <T as pallet_staking::Config>::Currency::minimum_balance();
            assert!(
                T::RewardPotSafetyFloor::get() >= minimum,
                "reward safety floor must cover ED"
            );
            assert!(
                T::TreasuryPotSafetyFloor::get() >= minimum,
                "treasury safety floor must cover ED"
            );
            let initial = Self::initial_fee_routing_configuration();
            assert!(
                Self::valid_fee_routing_configuration(&initial),
                "invalid initial fee routing"
            );
            assert!(
                T::InitialTipToAuthorShare::get() == 100,
                "all tips must route to author"
            );
            assert!(
                T::RewardPalletId::get() != T::EcosystemTreasuryPalletId::get(),
                "reward and treasury destinations must differ"
            );
            assert!(
                T::RewardPalletId::get() != T::FeeCollectionPalletId::get(),
                "reward and fee collection accounts must differ"
            );
            assert!(
                T::EcosystemTreasuryPalletId::get() != T::FeeCollectionPalletId::get(),
                "treasury and fee collection accounts must differ"
            );
            assert!(
                T::MaxRewardNominatorsPerPage::get()
                    <= <T as pallet_staking::Config>::MaxExposurePageSize::get(),
                "reward transfer bound must fit staking exposure pages"
            );
        }

        fn on_initialize(_block: BlockNumberFor<T>) -> Weight {
            Self::observe_pot_funding();
            if RewardSystemActive::<T>::get() {
                if Self::pot_balance() < Self::reward_pot_floor() {
                    AllocationPaused::<T>::put(true);
                    Self::deposit_event(Event::RewardPotBelowMinimum {
                        balance: Self::pot_balance(),
                        required: Self::reward_pot_floor(),
                    });
                }
                if Self::ecosystem_treasury_balance() < Self::treasury_pot_floor() {
                    Self::deposit_event(Event::TreasuryPotBelowMinimum {
                        balance: Self::ecosystem_treasury_balance(),
                        required: Self::treasury_pot_floor(),
                    });
                }
                Self::observe_era_transition();
                if let Some(active) = pallet_staking::ActiveEra::<T>::get() {
                    Self::expire_one_era(active.index);
                }
            }
            T::DbWeight::get().reads_writes(22, 14)
        }

        fn on_runtime_upgrade() -> Weight {
            let on_chain = StorageVersion::get::<Pallet<T>>();
            if on_chain >= STORAGE_VERSION {
                return T::DbWeight::get().reads(1);
            }

            if on_chain == StorageVersion::new(0) {
                RewardSystemActive::<T>::kill();
                FeeRoutingActive::<T>::kill();
                ActivationBlock::<T>::kill();
                ActivationEra::<T>::kill();
                FirstEligibleEra::<T>::kill();
                PointSourceOperationalEra::<T>::kill();
                LegacyZeroPointEraRange::<T>::kill();
                LegacyLiabilityBudgetPeriodStartMillis::<T>::kill();
                LastObservedEra::<T>::kill();
                LastObservedEraStartMillis::<T>::kill();
                PendingCompletedEra::<T>::kill();
                BudgetPeriodStartMillis::<T>::kill();
                NextExpiryEra::<T>::kill();
                AnnualBudgetUsed::<T>::kill();
                CommittedLiabilities::<T>::kill();
                PendingRewardFee::<T>::kill();
                PendingTreasuryFee::<T>::kill();
                PendingTipTotal::<T>::kill();
                PendingUnassignedTip::<T>::kill();
                AllocationPaused::<T>::put(true);
                FeeRoutingConfig::<T>::put(Self::initial_fee_routing_configuration());
                LastPotBalance::<T>::put(Self::pot_balance());
                LastTreasuryBalance::<T>::put(Self::ecosystem_treasury_balance());
                STORAGE_VERSION.put::<Pallet<T>>();
                return T::DbWeight::get().reads_writes(4, 24);
            }

            if on_chain == StorageVersion::new(1) {
                if RewardSystemActive::<T>::get() {
                    if let Some(active) = pallet_staking::ActiveEra::<T>::get() {
                        let upper = active.index.saturating_sub(1);
                        if let Some(first) =
                            FirstEligibleEra::<T>::get().filter(|first| *first <= upper)
                        {
                            LegacyZeroPointEraRange::<T>::put((first, upper));
                            if let Some(period_start) = BudgetPeriodStartMillis::<T>::get() {
                                LegacyLiabilityBudgetPeriodStartMillis::<T>::put(period_start);
                            }
                        }
                        PointSourceOperationalEra::<T>::put(active.index);
                        FirstEligibleEra::<T>::put(active.index.saturating_add(1));
                        SkippedRewardEras::<T>::insert(
                            active.index,
                            RewardEraSkipReason::PartialPointSourceEra,
                        );
                        LastObservedEra::<T>::put(active.index);
                        match active.start {
                            Some(start) => LastObservedEraStartMillis::<T>::put(start),
                            None => LastObservedEraStartMillis::<T>::kill(),
                        }
                        PendingCompletedEra::<T>::kill();
                    } else {
                        AllocationPaused::<T>::put(true);
                    }
                }
                STORAGE_VERSION.put::<Pallet<T>>();
                return T::DbWeight::get().reads_writes(8, 10);
            }

            AllocationPaused::<T>::put(true);
            T::DbWeight::get().reads_writes(1, 1)
        }

        #[cfg(feature = "try-runtime")]
        fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
            let version = StorageVersion::get::<Pallet<T>>();
            if version != StorageVersion::new(0)
                && version != StorageVersion::new(1)
                && version != STORAGE_VERSION
            {
                return Err("reward reserve expects storage version 0, 1, or 2".into());
            }
            let reward = Self::reward_pot_account();
            let treasury = Self::ecosystem_treasury_account();
            let collection = Self::fee_collection_account();
            let operational = (
                RewardSystemActive::<T>::get(),
                FeeRoutingActive::<T>::get(),
                ActivationBlock::<T>::get(),
                ActivationEra::<T>::get(),
                FirstEligibleEra::<T>::get(),
                AnnualBudgetUsed::<T>::get(),
                CommittedLiabilities::<T>::get(),
                PendingRewardFee::<T>::get(),
                PendingTreasuryFee::<T>::get(),
                PendingTipTotal::<T>::get(),
                PendingUnassignedTip::<T>::get(),
                AllocationPaused::<T>::get(),
                BudgetPeriodStartMillis::<T>::get(),
                FeeRoutingConfig::<T>::get(),
                pallet_staking::ActiveEra::<T>::get(),
            );
            Ok((
                version,
                <pallet_staking::Pallet<T> as frame_support::traits::GetStorageVersion>::on_chain_storage_version(),
                <T as pallet_staking::Config>::Currency::total_issuance(),
                Self::pot_balance(),
                Self::ecosystem_treasury_balance(),
                Self::fee_collection_balance(),
                frame_system::Pallet::<T>::account_exists(&reward),
                frame_system::Pallet::<T>::account_exists(&treasury),
                frame_system::Pallet::<T>::account_exists(&collection),
                operational,
            )
                .encode())
        }

        #[cfg(feature = "try-runtime")]
        fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
            let (
                old,
                staking_version,
                issuance,
                reward_balance,
                treasury_balance,
                collection_balance,
                reward_exists,
                treasury_exists,
                collection_exists,
                operational,
            ): PostUpgradeState<T> =
                Decode::decode(&mut &state[..]).map_err(|_| "invalid reward reserve pre-state")?;
            if StorageVersion::get::<Pallet<T>>() != STORAGE_VERSION {
                return Err("reward reserve storage version is not 2".into());
            }
            if <pallet_staking::Pallet<T> as frame_support::traits::GetStorageVersion>::on_chain_storage_version()
                != staking_version
            {
                return Err("staking storage version changed during reward reserve migration".into());
            }
            if <T as pallet_staking::Config>::Currency::total_issuance() != issuance {
                return Err("reward reserve migration changed total issuance".into());
            }
            if Self::pot_balance() != reward_balance
                || Self::ecosystem_treasury_balance() != treasury_balance
                || Self::fee_collection_balance() != collection_balance
            {
                return Err("reward reserve migration moved a keyless account balance".into());
            }
            if frame_system::Pallet::<T>::account_exists(&Self::reward_pot_account())
                != reward_exists
                || frame_system::Pallet::<T>::account_exists(&Self::ecosystem_treasury_account())
                    != treasury_exists
                || frame_system::Pallet::<T>::account_exists(&Self::fee_collection_account())
                    != collection_exists
            {
                return Err("reward reserve migration created or removed a keyless account".into());
            }

            let (
                reward_active,
                fee_active,
                activation_block,
                activation_era,
                old_first,
                annual_used,
                committed,
                pending_reward,
                pending_treasury,
                pending_tip,
                pending_unassigned,
                old_paused,
                budget_period,
                fee_config,
                active_era,
            ) = operational;
            if old == StorageVersion::new(0) {
                if RewardSystemActive::<T>::get()
                    || FeeRoutingActive::<T>::get()
                    || ActivationBlock::<T>::get().is_some()
                    || ActivationEra::<T>::get().is_some()
                    || FirstEligibleEra::<T>::get().is_some()
                    || !AnnualBudgetUsed::<T>::get().is_zero()
                    || !CommittedLiabilities::<T>::get().is_zero()
                {
                    return Err("version-0 migration did not remain dormant".into());
                }
            } else {
                if RewardSystemActive::<T>::get() != reward_active
                    || FeeRoutingActive::<T>::get() != fee_active
                    || ActivationBlock::<T>::get() != activation_block
                    || ActivationEra::<T>::get() != activation_era
                    || AnnualBudgetUsed::<T>::get() != annual_used
                    || CommittedLiabilities::<T>::get() != committed
                    || PendingRewardFee::<T>::get() != pending_reward
                    || PendingTreasuryFee::<T>::get() != pending_treasury
                    || PendingTipTotal::<T>::get() != pending_tip
                    || PendingUnassignedTip::<T>::get() != pending_unassigned
                    || BudgetPeriodStartMillis::<T>::get() != budget_period
                    || FeeRoutingConfig::<T>::get() != fee_config
                {
                    return Err("live reward or fee accounting changed during migration".into());
                }
                if old == StorageVersion::new(1) && reward_active {
                    let active = active_era.ok_or("active reward system has no active era")?;
                    if PointSourceOperationalEra::<T>::get() != Some(active.index)
                        || FirstEligibleEra::<T>::get() != Some(active.index.saturating_add(1))
                        || SkippedRewardEras::<T>::get(active.index)
                            != Some(RewardEraSkipReason::PartialPointSourceEra)
                    {
                        return Err("partial point-source era was not safely skipped".into());
                    }
                    let upper = active.index.saturating_sub(1);
                    let expected_range = old_first
                        .filter(|first| *first <= upper)
                        .map(|first| (first, upper));
                    if LegacyZeroPointEraRange::<T>::get() != expected_range {
                        return Err("legacy zero-point era range mismatch".into());
                    }
                    if expected_range.is_some()
                        && LegacyLiabilityBudgetPeriodStartMillis::<T>::get() != budget_period
                    {
                        return Err("legacy liability budget period mismatch".into());
                    }
                    if AllocationPaused::<T>::get() != old_paused {
                        return Err("allocation pause state changed unexpectedly".into());
                    }
                } else if FirstEligibleEra::<T>::get() != old_first
                    || AllocationPaused::<T>::get() != old_paused
                {
                    return Err("no-op migration changed eligibility or pause state".into());
                }
            }
            if !Self::valid_fee_routing_configuration(&Self::fee_routing_configuration()) {
                return Err("invalid fee routing configuration after migration".into());
            }
            Ok(())
        }
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// Claim one finalized reward page. Any signed account may trigger the public payout.
        #[pallet::call_index(0)]
        #[pallet::weight(T::RewardWeightInfo::claim_reward_page(
            T::MaxRewardNominatorsPerPage::get()
        ))]
        #[transactional]
        pub fn claim_reward_page(
            origin: OriginFor<T>,
            era: EraIndex,
            validator: T::AccountId,
            page: Page,
        ) -> DispatchResult {
            let caller = ensure_signed(origin)?;
            Self::do_claim_reward_page(caller, era, validator, page)
        }

        /// Reconcile a same-block, separately authorized transfer into the reward pot.
        /// This call records public accounting only and never moves funds.
        #[pallet::call_index(1)]
        #[pallet::weight(T::RewardWeightInfo::sync_reward_pot())]
        pub fn sync_reward_pot(origin: OriginFor<T>) -> DispatchResult {
            let _caller = ensure_signed(origin)?;
            ensure!(Self::observe_pot_funding(), Error::<T>::NoFundingIncrease);
            Ok(())
        }

        /// Update fee routing within the approved compile-time bounds.
        #[pallet::call_index(2)]
        #[pallet::weight(T::RewardWeightInfo::set_fee_routing_configuration())]
        pub fn set_fee_routing_configuration(
            origin: OriginFor<T>,
            new: FeeRoutingConfiguration,
        ) -> DispatchResult {
            ensure_root(origin)?;
            ensure!(
                Self::valid_fee_routing_configuration(&new),
                Error::<T>::InvalidFeeRoutingConfiguration
            );
            let old = Self::fee_routing_configuration();
            FeeRoutingConfig::<T>::put(new);
            Self::deposit_event(Event::FeeRoutingConfigurationChanged { old, new });
            Ok(())
        }

        /// Spend only from the isolated Ecosystem Treasury pot under the configured authority.
        #[pallet::call_index(3)]
        #[pallet::weight(T::RewardWeightInfo::spend_ecosystem_treasury())]
        #[transactional]
        pub fn spend_ecosystem_treasury(
            origin: OriginFor<T>,
            beneficiary: T::AccountId,
            amount: BalanceOf<T>,
        ) -> DispatchResult {
            T::TreasurySpendOrigin::ensure_origin(origin)?;
            ensure!(!amount.is_zero(), Error::<T>::ZeroAmount);
            ensure!(
                beneficiary != Self::reward_pot_account(),
                Error::<T>::InvalidTreasuryBeneficiary
            );
            ensure!(
                beneficiary != Self::ecosystem_treasury_account(),
                Error::<T>::InvalidTreasuryBeneficiary
            );
            ensure!(
                beneficiary != Self::fee_collection_account(),
                Error::<T>::InvalidTreasuryBeneficiary
            );
            ensure!(
                Self::spendable_treasury_balance() >= amount,
                Error::<T>::TreasuryPotBelowExistentialDeposit
            );
            let issuance = <T as pallet_staking::Config>::Currency::total_issuance();
            <T as pallet_staking::Config>::Currency::transfer(
                &Self::ecosystem_treasury_account(),
                &beneficiary,
                amount,
                frame_support::traits::tokens::Preservation::Preserve,
            )
            .map_err(|_| Error::<T>::TreasurySpendFailed)?;
            ensure!(
                <T as pallet_staking::Config>::Currency::total_issuance() == issuance,
                Error::<T>::IssuanceInvariantViolated
            );
            LastTreasuryBalance::<T>::put(Self::ecosystem_treasury_balance());
            Self::deposit_event(Event::EcosystemTreasurySpent {
                beneficiary,
                amount,
            });
            Ok(())
        }

        /// Atomically activate rewards and corrected-fee routing after every floor and reserve gate.
        #[pallet::call_index(4)]
        #[pallet::weight(T::RewardWeightInfo::activate_rewards_and_fee_routing())]
        pub fn activate_rewards_and_fee_routing(origin: OriginFor<T>) -> DispatchResult {
            ensure_root(origin)?;
            let (active_era, active_start) = match Self::validate_activation() {
                Ok(value) => value,
                Err((reason, error)) => {
                    Self::deposit_event(Event::RewardActivationRejected { reason });
                    return Err(error);
                }
            };
            let block = frame_system::Pallet::<T>::block_number();
            let first_eligible_era = active_era.saturating_add(1);
            Self::initialize_activation(active_era, active_start);
            ActivationBlock::<T>::put(block);
            RewardSystemActive::<T>::put(true);
            FeeRoutingActive::<T>::put(true);
            AllocationPaused::<T>::put(false);
            LastPotBalance::<T>::put(Self::pot_balance());
            LastTreasuryBalance::<T>::put(Self::ecosystem_treasury_balance());
            let configuration = Self::fee_routing_configuration();
            Self::deposit_event(Event::RewardSystemActivated {
                block,
                activation_era: active_era,
                first_eligible_era,
                reward_pot_balance: Self::pot_balance(),
            });
            Self::deposit_event(Event::FeeRoutingActivated {
                block,
                configuration,
            });
            Ok(())
        }

        /// Retry all staged normal-fee obligations. Any signed account may trigger safe settlement.
        #[pallet::call_index(5)]
        #[pallet::weight(T::RewardWeightInfo::retry_deferred_fee_routing())]
        pub fn retry_deferred_fee_routing(origin: OriginFor<T>) -> DispatchResult {
            let _caller = ensure_signed(origin)?;
            ensure!(
                !Self::pending_normal_fee().is_zero(),
                Error::<T>::FeeRoutingUnavailable
            );
            Self::settle_pending_fee_routing()
        }

        /// Retry a staged tip owed to a specific block author.
        #[pallet::call_index(6)]
        #[pallet::weight(T::RewardWeightInfo::retry_deferred_author_tip())]
        pub fn retry_deferred_author_tip(
            origin: OriginFor<T>,
            author: T::AccountId,
        ) -> DispatchResult {
            let _caller = ensure_signed(origin)?;
            ensure!(
                !PendingAuthorTips::<T>::get(&author).is_zero(),
                Error::<T>::FeeRoutingUnavailable
            );
            Self::settle_author_tip(author)
        }

        /// Release one proven pre-Spec12 zero-point liability without moving funds.
        #[pallet::call_index(7)]
        #[pallet::weight(T::RewardWeightInfo::cancel_legacy_zero_point_liability())]
        #[transactional]
        pub fn cancel_legacy_zero_point_liability(
            origin: OriginFor<T>,
            era: EraIndex,
        ) -> DispatchResult {
            ensure_root(origin)?;
            let (first, last) =
                LegacyZeroPointEraRange::<T>::get().ok_or(Error::<T>::InvalidLegacyZeroPointEra)?;
            ensure!(
                era >= first && era <= last,
                Error::<T>::InvalidLegacyZeroPointEra
            );
            ensure!(
                !CancelledZeroPointLiabilities::<T>::contains_key(era),
                Error::<T>::LiabilityAlreadyCancelled
            );
            ensure!(
                !ExpiredEras::<T>::contains_key(era),
                Error::<T>::ClaimExpired
            );
            let legacy_period = LegacyLiabilityBudgetPeriodStartMillis::<T>::get()
                .ok_or(Error::<T>::LegacyLiabilityPeriodChanged)?;
            ensure!(
                BudgetPeriodStartMillis::<T>::get() == Some(legacy_period),
                Error::<T>::LegacyLiabilityPeriodChanged
            );

            let stored_liability =
                EraRewardLiabilities::<T>::try_get(era).map_err(|_| Error::<T>::EraNotFinalized)?;
            ensure!(
                !stored_liability.is_zero(),
                Error::<T>::LiabilityAmountMismatch
            );
            let budget = EraRewardBudgets::<T>::get(era).ok_or(Error::<T>::EraNotFinalized)?;
            ensure!(
                budget == stored_liability,
                Error::<T>::LiabilityAmountMismatch
            );
            ensure!(
                EraRewardPaid::<T>::get(era).is_zero(),
                Error::<T>::LiabilityAlreadyPaid
            );
            ensure!(
                ClaimedRewardPages::<T>::iter_prefix(era).next().is_none(),
                Error::<T>::LiabilityHasClaims
            );
            let points = pallet_staking::ErasRewardPoints::<T>::get(era);
            ensure!(
                points.total.is_zero() && points.individual.values().all(|value| value.is_zero()),
                Error::<T>::InvalidRewardPoints
            );

            let committed = CommittedLiabilities::<T>::get();
            let annual_used = AnnualBudgetUsed::<T>::get();
            ensure!(
                Self::spendable_pot_balance() >= committed,
                Error::<T>::InsufficientRewardPot
            );
            let new_committed = committed
                .checked_sub(&stored_liability)
                .ok_or(Error::<T>::LiabilityAmountMismatch)?;
            let new_annual_used = annual_used
                .checked_sub(&stored_liability)
                .ok_or(Error::<T>::LiabilityAmountMismatch)?;
            let pot_before = Self::pot_balance();
            let issuance_before = <T as pallet_staking::Config>::Currency::total_issuance();

            EraRewardLiabilities::<T>::remove(era);
            CommittedLiabilities::<T>::put(new_committed);
            AnnualBudgetUsed::<T>::put(new_annual_used);
            CancelledZeroPointLiabilities::<T>::insert(era, stored_liability);

            ensure!(
                Self::pot_balance() == pot_before,
                Error::<T>::IssuanceInvariantViolated
            );
            ensure!(
                <T as pallet_staking::Config>::Currency::total_issuance() == issuance_before,
                Error::<T>::IssuanceInvariantViolated
            );
            Self::deposit_event(Event::EraRewardLiabilityCancelled {
                era,
                amount: stored_liability,
                reason: LiabilityCancellationReason::NoRewardPoints,
            });
            Ok(())
        }
    }

    impl<T: Config> Pallet<T> {
        pub fn reward_pot_account() -> T::AccountId {
            T::RewardPalletId::get().into_account_truncating()
        }

        pub fn ecosystem_treasury_account() -> T::AccountId {
            T::EcosystemTreasuryPalletId::get().into_account_truncating()
        }

        pub fn fee_collection_account() -> T::AccountId {
            T::FeeCollectionPalletId::get().into_account_truncating()
        }

        pub fn pot_balance() -> BalanceOf<T> {
            <T as pallet_staking::Config>::Currency::balance(&Self::reward_pot_account())
        }

        pub fn ecosystem_treasury_balance() -> BalanceOf<T> {
            <T as pallet_staking::Config>::Currency::balance(&Self::ecosystem_treasury_account())
        }

        pub fn fee_collection_balance() -> BalanceOf<T> {
            <T as pallet_staking::Config>::Currency::balance(&Self::fee_collection_account())
        }

        pub fn reward_pot_floor() -> BalanceOf<T> {
            T::RewardPotSafetyFloor::get()
                .max(<T as pallet_staking::Config>::Currency::minimum_balance())
        }

        pub fn treasury_pot_floor() -> BalanceOf<T> {
            T::TreasuryPotSafetyFloor::get()
                .max(<T as pallet_staking::Config>::Currency::minimum_balance())
        }

        pub fn fee_collection_floor() -> BalanceOf<T> {
            <T as pallet_staking::Config>::Currency::minimum_balance()
        }

        pub fn spendable_pot_balance() -> BalanceOf<T> {
            Self::pot_balance().saturating_sub(Self::reward_pot_floor())
        }

        pub fn spendable_treasury_balance() -> BalanceOf<T> {
            Self::ecosystem_treasury_balance().saturating_sub(Self::treasury_pot_floor())
        }

        pub fn spendable_fee_collection_balance() -> BalanceOf<T> {
            Self::fee_collection_balance().saturating_sub(Self::fee_collection_floor())
        }

        pub fn available_reward_balance() -> BalanceOf<T> {
            Self::spendable_pot_balance().saturating_sub(CommittedLiabilities::<T>::get())
        }

        pub fn pending_normal_fee() -> BalanceOf<T> {
            PendingRewardFee::<T>::get().saturating_add(PendingTreasuryFee::<T>::get())
        }

        pub fn pending_collection_obligations() -> BalanceOf<T> {
            Self::pending_normal_fee().saturating_add(PendingTipTotal::<T>::get())
        }

        pub fn pending_routing_consistent() -> bool {
            let Some(normal) =
                PendingRewardFee::<T>::get().checked_add(&PendingTreasuryFee::<T>::get())
            else {
                return false;
            };
            let Some(total) = normal.checked_add(&PendingTipTotal::<T>::get()) else {
                return false;
            };
            PendingTipTotal::<T>::get() >= PendingUnassignedTip::<T>::get()
                && Self::spendable_fee_collection_balance() >= total
        }

        pub fn fee_destinations_ready() -> bool {
            RewardSystemActive::<T>::get()
                && FeeRoutingActive::<T>::get()
                && Self::pot_balance() >= Self::reward_pot_floor()
                && Self::ecosystem_treasury_balance() >= Self::treasury_pot_floor()
                && Self::fee_collection_balance() >= Self::fee_collection_floor()
                && Self::pending_routing_consistent()
        }

        pub fn initial_fee_routing_configuration() -> FeeRoutingConfiguration {
            FeeRoutingConfiguration {
                reward_pot_percent: T::InitialRewardPotFeeShare::get(),
                ecosystem_treasury_percent: T::InitialEcosystemTreasuryFeeShare::get(),
                burn_percent: T::InitialBurnFeeShare::get(),
                tip_to_author_percent: T::InitialTipToAuthorShare::get(),
            }
        }

        pub fn fee_routing_configuration() -> FeeRoutingConfiguration {
            FeeRoutingConfig::<T>::get().unwrap_or_else(Self::initial_fee_routing_configuration)
        }

        pub fn valid_fee_routing_configuration(config: &FeeRoutingConfiguration) -> bool {
            config.reward_pot_percent <= T::MaxRewardPotFeeShare::get()
                && config.ecosystem_treasury_percent <= T::MaxEcosystemTreasuryFeeShare::get()
                && config.burn_percent <= T::MaxBurnFeeShare::get()
                && config.tip_to_author_percent == 100
                && config
                    .reward_pot_percent
                    .checked_add(config.ecosystem_treasury_percent)
                    .and_then(|v| v.checked_add(config.burn_percent))
                    == Some(100)
        }

        fn validate_activation(
        ) -> Result<(EraIndex, Option<u64>), (ActivationRejection, sp_runtime::DispatchError)>
        {
            if RewardSystemActive::<T>::get() || FeeRoutingActive::<T>::get() {
                return Err((
                    ActivationRejection::AlreadyActivated,
                    Error::<T>::AlreadyActivated.into(),
                ));
            }
            if ActivationBlock::<T>::get().is_some()
                || ActivationEra::<T>::get().is_some()
                || FirstEligibleEra::<T>::get().is_some()
                || LastObservedEra::<T>::get().is_some()
                || LastObservedEraStartMillis::<T>::get().is_some()
                || PendingCompletedEra::<T>::get().is_some()
                || EraRewardBudgets::<T>::iter().next().is_some()
                || EraRewardLiabilities::<T>::iter().next().is_some()
                || ClaimedRewardPages::<T>::iter().next().is_some()
                || ExpiredEras::<T>::iter().next().is_some()
            {
                return Err((
                    ActivationRejection::ActivationStateInvalid,
                    Error::<T>::ActivationStateInvalid.into(),
                ));
            }
            if !AnnualBudgetUsed::<T>::get().is_zero()
                || !CommittedLiabilities::<T>::get().is_zero()
                || !Self::pending_normal_fee().is_zero()
                || !PendingTipTotal::<T>::get().is_zero()
                || !PendingUnassignedTip::<T>::get().is_zero()
                || PendingAuthorTips::<T>::iter().next().is_some()
            {
                return Err((
                    ActivationRejection::PendingRoutingInconsistent,
                    Error::<T>::PendingRoutingInconsistent.into(),
                ));
            }
            let configuration = Self::fee_routing_configuration();
            if !Self::valid_fee_routing_configuration(&configuration)
                || configuration != Self::initial_fee_routing_configuration()
            {
                return Err((
                    ActivationRejection::InvalidFeeConfiguration,
                    Error::<T>::InvalidFeeRoutingConfiguration.into(),
                ));
            }

            let reward = Self::reward_pot_account();
            if !frame_system::Pallet::<T>::account_exists(&reward) {
                return Err((
                    ActivationRejection::RewardPotMissing,
                    Error::<T>::RewardPotMissing.into(),
                ));
            }
            if Self::pot_balance() < Self::reward_pot_floor() {
                return Err((
                    ActivationRejection::RewardPotBelowExistentialDeposit,
                    Error::<T>::RewardPotBelowExistentialDeposit.into(),
                ));
            }
            if Self::spendable_pot_balance() < T::InitialRewardReserve::get() {
                return Err((
                    ActivationRejection::RewardPotBelowActivationReserve,
                    Error::<T>::RewardPotBelowActivationReserve.into(),
                ));
            }

            let treasury = Self::ecosystem_treasury_account();
            if !frame_system::Pallet::<T>::account_exists(&treasury) {
                return Err((
                    ActivationRejection::TreasuryPotMissing,
                    Error::<T>::TreasuryPotMissing.into(),
                ));
            }
            if Self::ecosystem_treasury_balance() < Self::treasury_pot_floor() {
                return Err((
                    ActivationRejection::TreasuryPotBelowExistentialDeposit,
                    Error::<T>::TreasuryPotBelowExistentialDeposit.into(),
                ));
            }

            let collection = Self::fee_collection_account();
            if !frame_system::Pallet::<T>::account_exists(&collection) {
                return Err((
                    ActivationRejection::FeeCollectionPotMissing,
                    Error::<T>::FeeCollectionPotMissing.into(),
                ));
            }
            if Self::fee_collection_balance() < Self::fee_collection_floor() {
                return Err((
                    ActivationRejection::FeeCollectionPotBelowExistentialDeposit,
                    Error::<T>::FeeCollectionPotBelowExistentialDeposit.into(),
                ));
            }
            let active = pallet_staking::ActiveEra::<T>::get().ok_or_else(|| {
                (
                    ActivationRejection::ActivationStateInvalid,
                    Error::<T>::ActivationStateInvalid.into(),
                )
            })?;
            Ok((active.index, active.start))
        }

        pub fn target_era_reward(duration_millis: u64) -> Option<BalanceOf<T>> {
            let year = T::MillisecondsPerYear::get();
            if year == 0 {
                return None;
            }
            let cap: u128 = T::AnnualRewardCap::get().saturated_into();
            let target = U256::from(cap)
                .checked_mul(U256::from(duration_millis))?
                .checked_div(U256::from(year))?;
            if target > U256::from(u128::MAX) {
                return None;
            }
            Some(target.as_u128().saturated_into())
        }

        pub fn fee_split(
            amount: BalanceOf<T>,
            percent: u8,
        ) -> Option<(BalanceOf<T>, BalanceOf<T>)> {
            if percent > 100 {
                return None;
            }
            let raw: u128 = amount.saturated_into();
            let contribution = U256::from(raw)
                .checked_mul(U256::from(percent))?
                .checked_div(U256::from(100u8))?;
            if contribution > U256::from(u128::MAX) {
                return None;
            }
            let contribution: BalanceOf<T> = contribution.as_u128().saturated_into();
            let remainder = amount.checked_sub(&contribution)?;
            Some((contribution, remainder))
        }

        /// Record a corrected normal fee already held by the keyless Fee Collection Pot.
        pub fn stage_normal_fee(normal_fee: BalanceOf<T>) -> DispatchResult {
            ensure!(
                FeeRoutingActive::<T>::get(),
                Error::<T>::FeeRoutingUnavailable
            );
            let configuration = Self::fee_routing_configuration();
            ensure!(
                Self::valid_fee_routing_configuration(&configuration),
                Error::<T>::InvalidFeeRoutingConfiguration
            );
            let (reward, treasury) = Self::fee_split(normal_fee, configuration.reward_pot_percent)
                .ok_or(Error::<T>::ArithmeticOverflow)?;
            let pending_reward = PendingRewardFee::<T>::get()
                .checked_add(&reward)
                .ok_or(Error::<T>::ArithmeticOverflow)?;
            let pending_treasury = PendingTreasuryFee::<T>::get()
                .checked_add(&treasury)
                .ok_or(Error::<T>::ArithmeticOverflow)?;
            let pending_normal = pending_reward
                .checked_add(&pending_treasury)
                .ok_or(Error::<T>::ArithmeticOverflow)?;
            let all_obligations = pending_normal
                .checked_add(&PendingTipTotal::<T>::get())
                .ok_or(Error::<T>::ArithmeticOverflow)?;
            let collection_after = Self::spendable_fee_collection_balance()
                .checked_add(&normal_fee)
                .ok_or(Error::<T>::ArithmeticOverflow)?;
            ensure!(
                collection_after >= all_obligations,
                Error::<T>::PendingRoutingInconsistent
            );
            PendingRewardFee::<T>::put(pending_reward);
            PendingTreasuryFee::<T>::put(pending_treasury);
            Self::deposit_event(Event::FeeRoutingDeferred {
                normal_fee,
                pending_reward,
                pending_treasury,
            });
            Ok(())
        }

        /// Settle all staged normal fees atomically or retain every obligation for retry.
        pub fn settle_pending_fee_routing() -> DispatchResult {
            ensure!(
                FeeRoutingActive::<T>::get(),
                Error::<T>::FeeRoutingUnavailable
            );
            let reward = PendingRewardFee::<T>::get();
            let treasury = PendingTreasuryFee::<T>::get();
            let normal_fee = reward
                .checked_add(&treasury)
                .ok_or(Error::<T>::ArithmeticOverflow)?;
            ensure!(!normal_fee.is_zero(), Error::<T>::FeeRoutingUnavailable);
            let all_obligations = normal_fee
                .checked_add(&PendingTipTotal::<T>::get())
                .ok_or(Error::<T>::ArithmeticOverflow)?;
            ensure!(
                Self::spendable_fee_collection_balance() >= all_obligations,
                Error::<T>::PendingRoutingInconsistent
            );

            let collection = Self::fee_collection_account();
            let reward_account = Self::reward_pot_account();
            let treasury_account = Self::ecosystem_treasury_account();
            let issuance = <T as pallet_staking::Config>::Currency::total_issuance();
            let mut failure = FeeRoutingFailure::PendingRoutingInconsistent;
            let result = with_storage_layer(|| -> DispatchResult {
                if !reward.is_zero() {
                    failure = FeeRoutingFailure::RewardPotResolution;
                    <T as pallet_staking::Config>::Currency::transfer(
                        &collection,
                        &reward_account,
                        reward,
                        frame_support::traits::tokens::Preservation::Preserve,
                    )
                    .map_err(|_| Error::<T>::FeeRoutingUnavailable)?;
                }
                if !treasury.is_zero() {
                    failure = FeeRoutingFailure::EcosystemTreasuryResolution;
                    <T as pallet_staking::Config>::Currency::transfer(
                        &collection,
                        &treasury_account,
                        treasury,
                        frame_support::traits::tokens::Preservation::Preserve,
                    )
                    .map_err(|_| Error::<T>::FeeRoutingUnavailable)?;
                }
                ensure!(
                    <T as pallet_staking::Config>::Currency::total_issuance() == issuance,
                    Error::<T>::IssuanceInvariantViolated
                );
                PendingRewardFee::<T>::kill();
                PendingTreasuryFee::<T>::kill();
                Self::record_transaction_fee_route(
                    normal_fee,
                    reward,
                    treasury,
                    Zero::zero(),
                    None,
                )?;
                Self::deposit_event(Event::FeeRoutingCompleted {
                    normal_fee,
                    reward_pot_contribution: reward,
                    ecosystem_treasury_contribution: treasury,
                });
                Ok(())
            });
            if result.is_err() {
                Self::note_fee_routing_failure(normal_fee, Zero::zero(), failure);
            }
            result
        }

        /// Record a tip already held by the Fee Collection Pot.
        pub fn stage_author_tip(author: Option<T::AccountId>, tip: BalanceOf<T>) -> DispatchResult {
            if tip.is_zero() {
                return Ok(());
            }
            ensure!(
                FeeRoutingActive::<T>::get(),
                Error::<T>::FeeRoutingUnavailable
            );
            let new_total = PendingTipTotal::<T>::get()
                .checked_add(&tip)
                .ok_or(Error::<T>::ArithmeticOverflow)?;
            let all_obligations = Self::pending_normal_fee()
                .checked_add(&new_total)
                .ok_or(Error::<T>::ArithmeticOverflow)?;
            let collection_after = Self::spendable_fee_collection_balance()
                .checked_add(&tip)
                .ok_or(Error::<T>::ArithmeticOverflow)?;
            ensure!(
                collection_after >= all_obligations,
                Error::<T>::PendingRoutingInconsistent
            );
            if let Some(author_account) = author.clone() {
                PendingAuthorTips::<T>::try_mutate(author_account, |pending| -> DispatchResult {
                    *pending = pending
                        .checked_add(&tip)
                        .ok_or(Error::<T>::ArithmeticOverflow)?;
                    Ok(())
                })?;
            } else {
                PendingUnassignedTip::<T>::try_mutate(|pending| -> DispatchResult {
                    *pending = pending
                        .checked_add(&tip)
                        .ok_or(Error::<T>::ArithmeticOverflow)?;
                    Ok(())
                })?;
            }
            PendingTipTotal::<T>::put(new_total);
            Self::deposit_event(Event::AuthorTipDeferred {
                author,
                amount: tip,
            });
            Ok(())
        }

        /// Settle one block author's staged tips or retain the complete amount for retry.
        pub fn settle_author_tip(author: T::AccountId) -> DispatchResult {
            ensure!(
                FeeRoutingActive::<T>::get(),
                Error::<T>::FeeRoutingUnavailable
            );
            let tip = PendingAuthorTips::<T>::get(&author);
            ensure!(!tip.is_zero(), Error::<T>::FeeRoutingUnavailable);
            ensure!(
                PendingTipTotal::<T>::get() >= tip,
                Error::<T>::PendingRoutingInconsistent
            );
            ensure!(
                Self::spendable_fee_collection_balance() >= Self::pending_collection_obligations(),
                Error::<T>::PendingRoutingInconsistent
            );
            let collection = Self::fee_collection_account();
            let issuance = <T as pallet_staking::Config>::Currency::total_issuance();
            let result = with_storage_layer(|| -> DispatchResult {
                <T as pallet_staking::Config>::Currency::transfer(
                    &collection,
                    &author,
                    tip,
                    frame_support::traits::tokens::Preservation::Preserve,
                )
                .map_err(|_| Error::<T>::FeeRoutingUnavailable)?;
                ensure!(
                    <T as pallet_staking::Config>::Currency::total_issuance() == issuance,
                    Error::<T>::IssuanceInvariantViolated
                );
                PendingAuthorTips::<T>::remove(&author);
                PendingTipTotal::<T>::try_mutate(|pending| -> DispatchResult {
                    *pending = pending
                        .checked_sub(&tip)
                        .ok_or(Error::<T>::PendingRoutingInconsistent)?;
                    Ok(())
                })?;
                AuthorTipTotal::<T>::try_mutate(|total| -> DispatchResult {
                    *total = total
                        .checked_add(&tip)
                        .ok_or(Error::<T>::ArithmeticOverflow)?;
                    Ok(())
                })?;
                Self::deposit_event(Event::AuthorTipCompleted {
                    author: author.clone(),
                    amount: tip,
                });
                Ok(())
            });
            if result.is_err() {
                Self::note_fee_routing_failure(
                    Zero::zero(),
                    tip,
                    FeeRoutingFailure::AuthorTipResolution,
                );
            }
            result
        }

        pub fn finalize_completed_era(
            era: EraIndex,
            duration_millis: u64,
            era_end_millis: u64,
        ) -> DispatchResult {
            ensure!(
                !EraRewardBudgets::<T>::contains_key(era),
                Error::<T>::EraAlreadyFinalized
            );
            if SkippedRewardEras::<T>::get(era) == Some(RewardEraSkipReason::PartialPointSourceEra)
            {
                EraRewardBudgets::<T>::insert(era, BalanceOf::<T>::zero());
                Self::deposit_event(Event::EraRewardSkippedPartialPointSource { era });
                return Ok(());
            }
            ensure!(
                !SkippedRewardEras::<T>::contains_key(era),
                Error::<T>::EraAlreadyFinalized
            );
            let first = FirstEligibleEra::<T>::get().ok_or(Error::<T>::NotActivated)?;
            if era < first {
                SkippedRewardEras::<T>::insert(era, RewardEraSkipReason::PartialPointSourceEra);
                EraRewardBudgets::<T>::insert(era, BalanceOf::<T>::zero());
                Self::deposit_event(Event::EraRewardSkippedPartialPointSource { era });
                return Ok(());
            }

            let target =
                Self::target_era_reward(duration_millis).ok_or(Error::<T>::ArithmeticOverflow)?;
            if AllocationPaused::<T>::get() {
                SkippedRewardEras::<T>::insert(era, RewardEraSkipReason::AllocationPaused);
                EraRewardBudgets::<T>::insert(era, BalanceOf::<T>::zero());
                Self::deposit_event(Event::EraRewardSkippedAllocationPaused {
                    era,
                    calculated_budget: target,
                });
                return Ok(());
            }
            if !Self::has_valid_reward_points(era) {
                AllocationPaused::<T>::put(true);
                SkippedRewardEras::<T>::insert(era, RewardEraSkipReason::NoRewardPoints);
                EraRewardBudgets::<T>::insert(era, BalanceOf::<T>::zero());
                Self::deposit_event(Event::EraRewardSkippedNoPoints {
                    era,
                    calculated_budget: target,
                });
                return Ok(());
            }

            let year = T::MillisecondsPerYear::get();
            let mut period_start = BudgetPeriodStartMillis::<T>::get()
                .unwrap_or_else(|| era_end_millis.saturating_sub(duration_millis));
            if era_end_millis.saturating_sub(period_start) >= year {
                period_start = era_end_millis.saturating_sub(duration_millis);
                BudgetPeriodStartMillis::<T>::put(period_start);
                AnnualBudgetUsed::<T>::put(BalanceOf::<T>::zero());
            } else if BudgetPeriodStartMillis::<T>::get().is_none() {
                BudgetPeriodStartMillis::<T>::put(period_start);
            }

            let used = AnnualBudgetUsed::<T>::get();
            let annual_remaining = T::AnnualRewardCap::get().saturating_sub(used);
            let available = Self::available_reward_balance();
            let budget = target.min(annual_remaining).min(available);
            let new_used = used
                .checked_add(&budget)
                .ok_or(Error::<T>::ArithmeticOverflow)?;
            let new_committed = CommittedLiabilities::<T>::get()
                .checked_add(&budget)
                .ok_or(Error::<T>::ArithmeticOverflow)?;

            EraRewardBudgets::<T>::insert(era, budget);
            EraRewardPaid::<T>::insert(era, BalanceOf::<T>::zero());
            EraRewardLiabilities::<T>::insert(era, budget);
            AnnualBudgetUsed::<T>::put(new_used);
            CommittedLiabilities::<T>::put(new_committed);
            if NextExpiryEra::<T>::get().is_none() {
                NextExpiryEra::<T>::put(first);
            }

            Self::deposit_event(Event::EraRewardBudgetFinalized {
                era,
                duration_millis,
                target,
                budget,
                annual_budget_used: new_used,
            });
            if !budget.is_zero() {
                Self::deposit_event(Event::EraRewardLiabilityReserved {
                    era,
                    amount: budget,
                    committed_liabilities: new_committed,
                });
            }

            if budget < target {
                AllocationPaused::<T>::put(true);
                Self::deposit_event(Event::RewardAllocationPaused { era, available });
            }
            if Self::available_reward_balance() < T::LowReserveThreshold::get() {
                Self::deposit_event(Event::RewardReserveLow {
                    available: Self::available_reward_balance(),
                    committed_liabilities: new_committed,
                });
            }
            Ok(())
        }

        fn do_claim_reward_page(
            caller: T::AccountId,
            era: EraIndex,
            validator: T::AccountId,
            page: Page,
        ) -> DispatchResult {
            ensure!(RewardSystemActive::<T>::get(), Error::<T>::NotActivated);
            ensure!(
                !SkippedRewardEras::<T>::contains_key(era),
                Error::<T>::RewardEraSkipped
            );
            ensure!(
                !CancelledZeroPointLiabilities::<T>::contains_key(era),
                Error::<T>::RewardEraCancelled
            );
            let budget = EraRewardBudgets::<T>::get(era).ok_or(Error::<T>::EraNotFinalized)?;
            ensure!(
                !ExpiredEras::<T>::contains_key(era),
                Error::<T>::ClaimExpired
            );
            if let Some(active) = pallet_staking::ActiveEra::<T>::get() {
                ensure!(
                    era >= active.index.saturating_sub(T::HistoryDepth::get()),
                    Error::<T>::ClaimExpired
                );
            }
            ensure!(
                !ClaimedRewardPages::<T>::contains_key(era, (validator.clone(), page)),
                Error::<T>::AlreadyClaimed
            );

            let reward_points = pallet_staking::ErasRewardPoints::<T>::get(era);
            ensure!(
                !reward_points.total.is_zero(),
                Error::<T>::InvalidRewardPoints
            );
            let validator_points = reward_points
                .individual
                .get(&validator)
                .copied()
                .ok_or(Error::<T>::ValidatorNotRewarded)?;
            let page_count =
                Self::reward_page_count(era, &validator).ok_or(Error::<T>::InvalidExposure)?;
            ensure!(page < page_count, Error::<T>::InvalidPage);
            let exposure = pallet_staking::EraInfo::<T>::get_paged_exposure(era, &validator, page)
                .ok_or(Error::<T>::InvalidExposure)?;
            let exposure_total = exposure.total();
            ensure!(
                exposure.others().len() as u32 <= T::MaxRewardNominatorsPerPage::get(),
                Error::<T>::TooManyRewardRecipients
            );
            ensure!(!exposure_total.is_zero(), Error::<T>::InvalidExposure);
            ensure!(
                exposure.own() <= exposure_total,
                Error::<T>::InvalidExposure
            );
            ensure!(
                exposure.page_total() <= exposure_total,
                Error::<T>::InvalidExposure
            );

            let validator_reward_part =
                Perbill::from_rational(validator_points, reward_points.total);
            let validator_total_payout = validator_reward_part * budget;
            let configured_commission =
                pallet_staking::ErasValidatorPrefs::<T>::get(era, &validator).commission;
            let commission_rate = configured_commission.min(T::MaxValidatorCommission::get());
            let total_commission = commission_rate * validator_total_payout;
            let leftover = validator_total_payout.saturating_sub(total_commission);
            let validator_stake = Perbill::from_rational(exposure.own(), exposure_total) * leftover;
            let page_commission =
                Perbill::from_rational(exposure.page_total(), exposure_total) * total_commission;
            let validator_amount = validator_stake
                .checked_add(&page_commission)
                .ok_or(Error::<T>::ArithmeticOverflow)?;

            let pot = Self::reward_pot_account();
            let treasury = Self::ecosystem_treasury_account();
            let collection = Self::fee_collection_account();
            ensure!(
                validator != pot && validator != treasury && validator != collection,
                Error::<T>::InvalidExposure
            );
            let mut payouts: Vec<(T::AccountId, BalanceOf<T>, bool)> = Vec::new();
            if !validator_amount.is_zero() {
                payouts.push((validator.clone(), validator_amount, true));
            }
            let mut page_total = validator_amount;
            for nominator in exposure.others().iter() {
                ensure!(
                    nominator.who != pot
                        && nominator.who != treasury
                        && nominator.who != collection,
                    Error::<T>::InvalidExposure
                );
                ensure!(
                    nominator.value <= exposure_total,
                    Error::<T>::InvalidExposure
                );
                let amount = Perbill::from_rational(nominator.value, exposure_total) * leftover;
                page_total = page_total
                    .checked_add(&amount)
                    .ok_or(Error::<T>::ArithmeticOverflow)?;
                if !amount.is_zero() {
                    payouts.push((nominator.who.clone(), amount, false));
                }
            }

            let liability = EraRewardLiabilities::<T>::get(era);
            ensure!(
                liability >= page_total,
                Error::<T>::InsufficientEraLiability
            );
            ensure!(
                Self::pot_balance() >= Self::reward_pot_floor(),
                Error::<T>::RewardPotBelowExistentialDeposit
            );
            ensure!(
                Self::spendable_pot_balance() >= CommittedLiabilities::<T>::get(),
                Error::<T>::InsufficientRewardPot
            );
            let issuance_before = <T as pallet_staking::Config>::Currency::total_issuance();
            for (recipient, amount, _) in payouts.iter() {
                <T as pallet_staking::Config>::Currency::transfer(
                    &pot,
                    recipient,
                    *amount,
                    frame_support::traits::tokens::Preservation::Preserve,
                )
                .map_err(|_| Error::<T>::RewardTransferFailed)?;
            }
            ensure!(
                <T as pallet_staking::Config>::Currency::total_issuance() == issuance_before,
                Error::<T>::IssuanceInvariantViolated
            );

            ClaimedRewardPages::<T>::insert(era, (validator.clone(), page), ());
            let paid = EraRewardPaid::<T>::get(era)
                .checked_add(&page_total)
                .ok_or(Error::<T>::ArithmeticOverflow)?;
            let remaining = liability
                .checked_sub(&page_total)
                .ok_or(Error::<T>::ArithmeticOverflow)?;
            let committed = CommittedLiabilities::<T>::get()
                .checked_sub(&page_total)
                .ok_or(Error::<T>::ArithmeticOverflow)?;
            EraRewardPaid::<T>::insert(era, paid);
            EraRewardLiabilities::<T>::insert(era, remaining);
            CommittedLiabilities::<T>::put(committed);

            Self::deposit_event(Event::ValidatorRewardClaimed {
                era,
                page,
                validator: validator.clone(),
                amount: validator_amount,
                commission: page_commission,
            });
            for (recipient, amount, is_validator) in payouts.into_iter() {
                if !is_validator {
                    Self::deposit_event(Event::NominatorRewardClaimed {
                        era,
                        page,
                        validator: validator.clone(),
                        nominator: recipient,
                        amount,
                    });
                }
            }

            let mut final_remaining = remaining;
            if Self::all_reward_pages_claimed(era) && !remaining.is_zero() {
                final_remaining = Zero::zero();
                EraRewardLiabilities::<T>::insert(era, BalanceOf::<T>::zero());
                CommittedLiabilities::<T>::put(
                    CommittedLiabilities::<T>::get()
                        .checked_sub(&remaining)
                        .ok_or(Error::<T>::ArithmeticOverflow)?,
                );
                Self::deposit_event(Event::EraRoundingRemainderReleased {
                    era,
                    amount: remaining,
                });
            }
            LastPotBalance::<T>::put(Self::pot_balance());
            Self::deposit_event(Event::EraRewardPagePaid {
                era,
                page,
                validator,
                caller,
                amount: page_total,
                remaining_liability: final_remaining,
            });
            Ok(())
        }

        fn has_valid_reward_points(era: EraIndex) -> bool {
            let points = pallet_staking::ErasRewardPoints::<T>::get(era);
            if points.total.is_zero() || points.individual.is_empty() {
                return false;
            }
            let mut eligible_total = 0u32;
            for (validator, validator_points) in points.individual.iter() {
                if validator_points.is_zero() {
                    continue;
                }
                if pallet_staking::ErasStakersOverview::<T>::get(era, validator).is_none()
                    && !pallet_staking::ErasStakersClipped::<T>::contains_key(era, validator)
                {
                    return false;
                }
                let Some(total) = eligible_total.checked_add(*validator_points) else {
                    return false;
                };
                eligible_total = total;
            }
            eligible_total > 0 && eligible_total == points.total
        }

        fn reward_page_count(era: EraIndex, validator: &T::AccountId) -> Option<Page> {
            if let Some(overview) = pallet_staking::ErasStakersOverview::<T>::get(era, validator) {
                return Some(if overview.page_count == 0 && !overview.own.is_zero() {
                    1
                } else {
                    overview.page_count
                });
            }
            if pallet_staking::ErasStakersClipped::<T>::contains_key(era, validator) {
                Some(1)
            } else {
                None
            }
        }

        fn all_reward_pages_claimed(era: EraIndex) -> bool {
            let points = pallet_staking::ErasRewardPoints::<T>::get(era);
            if points.total.is_zero() {
                return false;
            }
            for (validator, validator_points) in points.individual.iter() {
                if validator_points.is_zero() {
                    continue;
                }
                let Some(page_count) = Self::reward_page_count(era, validator) else {
                    return false;
                };
                for page in 0..page_count {
                    if !ClaimedRewardPages::<T>::contains_key(era, (validator.clone(), page)) {
                        return false;
                    }
                }
            }
            true
        }

        fn initialize_activation(active_era: EraIndex, active_start: Option<u64>) {
            ActivationEra::<T>::put(active_era);
            PointSourceOperationalEra::<T>::put(active_era);
            SkippedRewardEras::<T>::insert(active_era, RewardEraSkipReason::PartialPointSourceEra);
            let first = active_era.saturating_add(1);
            FirstEligibleEra::<T>::put(first);
            LastObservedEra::<T>::put(active_era);
            if let Some(start) = active_start {
                LastObservedEraStartMillis::<T>::put(start);
            }
            NextExpiryEra::<T>::put(first);
        }

        fn observe_era_transition() {
            let Some(active) = pallet_staking::ActiveEra::<T>::get() else {
                return;
            };
            if ActivationEra::<T>::get().is_none() {
                AllocationPaused::<T>::put(true);
                return;
            }
            let Some(last) = LastObservedEra::<T>::get() else {
                LastObservedEra::<T>::put(active.index);
                if let Some(start) = active.start {
                    LastObservedEraStartMillis::<T>::put(start);
                }
                return;
            };

            if active.index == last {
                if let Some(start) = active.start {
                    if LastObservedEraStartMillis::<T>::get().is_none() {
                        LastObservedEraStartMillis::<T>::put(start);
                    }
                    if let Some((completed, completed_start)) = PendingCompletedEra::<T>::take() {
                        let duration = start.saturating_sub(completed_start);
                        if Self::finalize_completed_era(completed, duration, start).is_err() {
                            AllocationPaused::<T>::put(true);
                            Self::deposit_event(Event::RewardAllocationPaused {
                                era: completed,
                                available: Self::available_reward_balance(),
                            });
                        }
                    }
                }
                return;
            }

            if active.index != last.saturating_add(1) {
                AllocationPaused::<T>::put(true);
                Self::deposit_event(Event::RewardAllocationPaused {
                    era: last,
                    available: Self::available_reward_balance(),
                });
                PendingCompletedEra::<T>::kill();
            } else if let Some(completed_start) = LastObservedEraStartMillis::<T>::get() {
                if let Some(new_start) = active.start {
                    let duration = new_start.saturating_sub(completed_start);
                    if Self::finalize_completed_era(last, duration, new_start).is_err() {
                        AllocationPaused::<T>::put(true);
                    }
                } else {
                    PendingCompletedEra::<T>::put((last, completed_start));
                }
            }

            LastObservedEra::<T>::put(active.index);
            match active.start {
                Some(start) => LastObservedEraStartMillis::<T>::put(start),
                None => LastObservedEraStartMillis::<T>::kill(),
            }
        }

        fn expire_one_era(active_era: EraIndex) {
            let Some(cursor) = NextExpiryEra::<T>::get() else {
                return;
            };
            let expiry_limit = active_era.saturating_sub(T::HistoryDepth::get().saturating_add(1));
            if cursor > expiry_limit {
                return;
            }
            if EraRewardBudgets::<T>::contains_key(cursor)
                && !ExpiredEras::<T>::contains_key(cursor)
            {
                let released = EraRewardLiabilities::<T>::take(cursor);
                if !released.is_zero() {
                    CommittedLiabilities::<T>::mutate(|value| {
                        *value = value.saturating_sub(released)
                    });
                }
                ExpiredEras::<T>::insert(cursor, ());
                Self::deposit_event(Event::ClaimExpired {
                    era: cursor,
                    released_liability: released,
                });
            }
            NextExpiryEra::<T>::put(cursor.saturating_add(1));
        }

        fn observe_pot_funding() -> bool {
            let current = Self::pot_balance();
            let Some(previous) = LastPotBalance::<T>::get() else {
                LastPotBalance::<T>::put(current);
                return false;
            };
            LastPotBalance::<T>::put(current);
            if current <= previous {
                return false;
            }
            let amount = current.saturating_sub(previous);
            let total = ReserveFundingTotal::<T>::get().saturating_add(amount);
            ReserveFundingTotal::<T>::put(total);
            Self::deposit_event(Event::RewardPotFundingRecorded {
                source: FundingSource::ReserveTransfer,
                amount,
                total,
            });
            true
        }

        pub fn record_transaction_fee_route(
            normal_fee: BalanceOf<T>,
            reward_pot_contribution: BalanceOf<T>,
            ecosystem_treasury_contribution: BalanceOf<T>,
            tip: BalanceOf<T>,
            author: Option<T::AccountId>,
        ) -> DispatchResult {
            let routed = reward_pot_contribution
                .checked_add(&ecosystem_treasury_contribution)
                .ok_or(Error::<T>::ArithmeticOverflow)?;
            ensure!(
                routed == normal_fee,
                Error::<T>::InvalidFeeRoutingConfiguration
            );
            let reward_total = FeeContributionTotal::<T>::get()
                .checked_add(&reward_pot_contribution)
                .ok_or(Error::<T>::ArithmeticOverflow)?;
            let treasury_total = EcosystemTreasuryFeeTotal::<T>::get()
                .checked_add(&ecosystem_treasury_contribution)
                .ok_or(Error::<T>::ArithmeticOverflow)?;
            let normal_total = NormalFeeRoutedTotal::<T>::get()
                .checked_add(&normal_fee)
                .ok_or(Error::<T>::ArithmeticOverflow)?;
            let credited_tip = if author.is_some() { tip } else { Zero::zero() };
            let tip_total = AuthorTipTotal::<T>::get()
                .checked_add(&credited_tip)
                .ok_or(Error::<T>::ArithmeticOverflow)?;
            FeeContributionTotal::<T>::put(reward_total);
            EcosystemTreasuryFeeTotal::<T>::put(treasury_total);
            NormalFeeRoutedTotal::<T>::put(normal_total);
            AuthorTipTotal::<T>::put(tip_total);
            LastPotBalance::<T>::put(Self::pot_balance());
            LastTreasuryBalance::<T>::put(Self::ecosystem_treasury_balance());
            Self::deposit_event(Event::FeeContributionReceived {
                amount: reward_pot_contribution,
                total: reward_total,
            });
            Self::deposit_event(Event::RewardPotFundingRecorded {
                source: FundingSource::TransactionFee,
                amount: reward_pot_contribution,
                total: reward_total,
            });
            Self::deposit_event(Event::RewardPotFeeContribution {
                amount: reward_pot_contribution,
                total: reward_total,
            });
            Self::deposit_event(Event::EcosystemTreasuryFeeContribution {
                amount: ecosystem_treasury_contribution,
                total: treasury_total,
            });
            Self::deposit_event(Event::TransactionFeeRouted {
                normal_fee,
                reward_pot_contribution,
                ecosystem_treasury_contribution,
                burned: Zero::zero(),
                tip,
                author,
            });
            Ok(())
        }

        pub fn note_fee_routing_failure(
            normal_fee: BalanceOf<T>,
            tip: BalanceOf<T>,
            reason: FeeRoutingFailure,
        ) {
            Self::deposit_event(Event::FeeRoutingFailed {
                normal_fee,
                tip,
                reason,
            });
        }
    }
}

#[cfg(test)]
mod math_tests {
    use sp_core::U256;

    #[test]
    fn u256_duration_formula_handles_candidate_cap_without_overflow() {
        let cap = U256::from(5_000_000u128 * 1_000_000_000_000_000_000u128);
        let year = U256::from(31_556_952_000u64);
        let six_hours = U256::from(21_600_000u64);
        let reward = cap * six_hours / year;
        assert!(reward > U256::zero());
        assert!(reward < cap);
    }

    #[test]
    fn fee_split_percentages_round_down_and_conserve() {
        let fee = 101u128;
        for percent in [0u8, 30, 50, 70, 100] {
            let contribution = fee * percent as u128 / 100;
            let remainder = fee - contribution;
            assert_eq!(contribution + remainder, fee);
        }
        assert_eq!(fee * 30 / 100, 30);
    }
}
