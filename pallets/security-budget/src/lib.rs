#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub use pallet::*;
#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
pub mod weights;

use frame_support::dispatch::DispatchResult;

/// The runtime-owned issuance gateway. It has no dispatchable implementation.
pub trait ControlledIssuance<AccountId> {
    fn remaining_allowance() -> Option<u128>;
    fn mint_split(
        staking: &AccountId,
        staking_amount: u128,
        treasury: &AccountId,
        treasury_amount: u128,
        carry_before: u8,
        carry_after: u8,
    ) -> DispatchResult;

    #[cfg(feature = "runtime-benchmarks")]
    fn benchmark_set_remaining_allowance(amount: u128);
}

#[frame_support::pallet]
pub mod pallet {
    use alloc::vec::Vec;
    use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
    use frame_support::{
        ensure,
        pallet_prelude::*,
        storage::{with_transaction, TransactionOutcome},
        traits::{
            fungible::{Inspect, Mutate},
            tokens::{Fortitude, Preservation},
            EnsureOrigin, Get, StorageVersion,
        },
        transactional, PalletId,
    };
    use frame_system::pallet_prelude::*;
    use scale_info::TypeInfo;
    use sp_core::U256;
    use sp_runtime::{
        traits::{AccountIdConversion, Zero},
        DispatchError, Perbill, RuntimeDebug,
    };
    use sp_staking::{EraIndex, Page};

    use crate::{weights::WeightInfo, ControlledIssuance};

    const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);
    const PERBILL_DENOMINATOR: u128 = 1_000_000_000;
    type EligibleSummary<AccountId> = (u128, u32, Vec<(AccountId, u32)>, u32);

    #[derive(
        Clone,
        Copy,
        Decode,
        DecodeWithMemTracking,
        Encode,
        Eq,
        MaxEncodedLen,
        PartialEq,
        RuntimeDebug,
        TypeInfo,
    )]
    pub enum TipFallbackReason {
        MissingAuthor,
        UnreceivableAuthor,
    }

    #[derive(
        Clone,
        Copy,
        Decode,
        DecodeWithMemTracking,
        Encode,
        Eq,
        MaxEncodedLen,
        PartialEq,
        RuntimeDebug,
        TypeInfo,
    )]
    pub enum TreasurySource {
        NewIssuance,
        NormalFee,
        ReceivableTip,
        FailedAuthorTip,
    }

    #[derive(
        Clone,
        Copy,
        Decode,
        DecodeWithMemTracking,
        Encode,
        Eq,
        MaxEncodedLen,
        PartialEq,
        RuntimeDebug,
        TypeInfo,
    )]
    pub enum EraSkipReason {
        NoRewardPoints,
        NoEligibleValidators,
        NoFundedBudget,
        LifetimeRewardBudgetExhausted,
    }

    #[derive(
        Clone,
        Copy,
        Decode,
        DecodeWithMemTracking,
        Encode,
        Eq,
        MaxEncodedLen,
        PartialEq,
        RuntimeDebug,
        TypeInfo,
    )]
    pub enum AllocationFailureReason {
        EraTransition,
        Expiry,
    }

    #[derive(
        Clone,
        Copy,
        Decode,
        DecodeWithMemTracking,
        Encode,
        Eq,
        MaxEncodedLen,
        PartialEq,
        RuntimeDebug,
        TypeInfo,
    )]
    pub enum IssuanceLimit {
        None,
        AnnualCeiling,
        LifetimeAllowance,
        AnnualCeilingAndLifetimeAllowance,
        LifetimeRewardBudget,
        Multiple,
    }

    #[derive(
        Clone,
        Decode,
        DecodeWithMemTracking,
        Encode,
        Eq,
        MaxEncodedLen,
        PartialEq,
        RuntimeDebug,
        TypeInfo,
    )]
    pub struct EraBudget {
        pub gross_issuance: u128,
        pub staking_issuance: u128,
        pub treasury_issuance: u128,
        pub fee_staking: u128,
        pub failed_author_tips: u128,
        pub retained_carry: u128,
        pub total: u128,
        pub eligible_stake: u128,
        pub eligible_points: u32,
        pub duration_millis: u64,
    }

    #[pallet::config]
    pub trait Config:
        frame_system::Config<RuntimeEvent: From<Event<Self>>>
        + pallet_staking::Config<CurrencyBalance = u128>
        + pallet_reward_reserve::Config
    {
        type Issuance: ControlledIssuance<Self::AccountId>;

        #[pallet::constant]
        type StakingPotPalletId: Get<PalletId>;
        #[pallet::constant]
        type TreasuryPalletId: Get<PalletId>;
        #[pallet::constant]
        type V14FeeCollectionPalletId: Get<PalletId>;
        #[pallet::constant]
        type MinimumValidatorBond: Get<u128>;
        #[pallet::constant]
        type MaximumValidatorCommission: Get<Perbill>;
        #[pallet::constant]
        type TargetStakingApr: Get<Perbill>;
        #[pallet::constant]
        type GrossAnnualIssuanceCeiling: Get<u128>;
        /// Lifetime validator/nominator reward budget, including the migrated legacy liability.
        #[pallet::constant]
        type TotalStakingRewardBudget: Get<u128>;
        /// Chain-initialization-dependent liability baseline (zero on fresh genesis).
        /// This is a storage-backed getter, not a metadata constant.
        type ExistingEarnedRewardLiability: Get<u128>;
        /// Separate protected allocation that this pallet must never spend.
        #[pallet::constant]
        type ProtectedCommunityOnboardingBudget: Get<u128>;
        /// The retired legacy allocator has no post-V14 reward target.
        #[pallet::constant]
        type RetiredLegacyRewardReserveTarget: Get<u128>;
        #[pallet::constant]
        type SecurityMillisecondsPerYear: Get<u64>;
        #[pallet::constant]
        type RuntimeTechnicalMaxValidators: Get<u32>;
        #[pallet::constant]
        type ActivationValidatorCount: Get<u32>;
        #[pallet::constant]
        type V14MaxRewardNominatorsPerPage: Get<u32>;
        #[pallet::constant]
        type MaxExposurePages: Get<u32>;

        type ActivationOrigin: EnsureOrigin<Self::RuntimeOrigin>;
        type SecurityWeightInfo: WeightInfo;
    }

    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    #[pallet::storage]
    #[pallet::getter(fn active)]
    pub type Active<T> = StorageValue<_, bool, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn activated_at)]
    pub type ActivatedAt<T: Config> = StorageValue<_, BlockNumberFor<T>, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn migration_completed_at)]
    pub type MigrationCompletedAt<T: Config> = StorageValue<_, BlockNumberFor<T>, OptionQuery>;

    /// Legacy earned liability captured exactly once by the coordinated V14 migration.
    #[pallet::storage]
    #[pallet::getter(fn legacy_reward_liability)]
    pub type LegacyRewardLiability<T> = StorageValue<_, u128, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn first_eligible_era)]
    pub type FirstEligibleEra<T> = StorageValue<_, EraIndex, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn legacy_cutoff_era)]
    pub type LegacyCutoffEra<T> = StorageValue<_, EraIndex, OptionQuery>;

    #[pallet::storage]
    pub type LastObservedEra<T> = StorageValue<_, EraIndex, OptionQuery>;

    #[pallet::storage]
    pub type LastObservedEraStart<T> = StorageValue<_, u64, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn emission_period_start)]
    pub type EmissionPeriodStart<T> = StorageValue<_, u64, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn gross_issued_in_period)]
    pub type GrossIssuedInPeriod<T> = StorageValue<_, u128, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn target_remainder)]
    pub type TargetRemainder<T> = StorageValue<_, u128, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn issuance_split_carry)]
    pub type IssuanceSplitCarry<T> = StorageValue<_, u8, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn unallocated_fee_staking)]
    pub type UnallocatedFeeStaking<T> = StorageValue<_, u128, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn unallocated_tip_fallback)]
    pub type UnallocatedTipFallback<T> = StorageValue<_, u128, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn unallocated_staking_carry)]
    pub type UnallocatedStakingCarry<T> = StorageValue<_, u128, ValueQuery>;

    /// Fresh-genesis principal is a funding source, not earned debt or mint allowance.
    /// Absent on historical/migration state: the old allocator behavior remains unchanged.
    #[pallet::storage]
    #[pallet::getter(fn unallocated_principal)]
    pub type UnallocatedPrincipal<T> = StorageValue<_, u128, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn principal_reserved_total)]
    pub type PrincipalReservedTotal<T> = StorageValue<_, u128, ValueQuery>;

    /// Kept separate to retain the existing EraBudget SCALE layout.
    #[pallet::storage]
    #[pallet::getter(fn era_principal)]
    pub type EraPrincipal<T> = StorageMap<_, Twox64Concat, EraIndex, u128, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn pending_staking_fee)]
    pub type PendingStakingFee<T> = StorageValue<_, u128, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn pending_treasury_fee)]
    pub type PendingTreasuryFee<T> = StorageValue<_, u128, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn pending_author_tip)]
    pub type PendingAuthorTips<T: Config> =
        StorageMap<_, Twox64Concat, T::AccountId, u128, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn pending_missing_author_tips)]
    pub type PendingMissingAuthorTips<T> = StorageValue<_, u128, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn pending_tip_total)]
    pub type PendingTipTotal<T> = StorageValue<_, u128, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn era_budget)]
    pub type EraBudgets<T> = StorageMap<_, Twox64Concat, EraIndex, EraBudget, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn eligible_validator_points)]
    pub type EligibleValidatorPoints<T: Config> =
        StorageDoubleMap<_, Twox64Concat, EraIndex, Twox64Concat, T::AccountId, u32, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn era_paid)]
    pub type EraPaid<T> = StorageMap<_, Twox64Concat, EraIndex, u128, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn validator_paid)]
    pub type ValidatorPaid<T: Config> =
        StorageDoubleMap<_, Twox64Concat, EraIndex, Twox64Concat, T::AccountId, u128, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn era_liability)]
    pub type EraLiability<T> = StorageMap<_, Twox64Concat, EraIndex, u128, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn committed_liabilities)]
    pub type CommittedLiabilities<T> = StorageValue<_, u128, ValueQuery>;

    #[pallet::storage]
    pub type ClaimedPages<T: Config> = StorageDoubleMap<
        _,
        Twox64Concat,
        EraIndex,
        Twox64Concat,
        (T::AccountId, Page),
        (),
        OptionQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn expired_era)]
    pub type ExpiredEras<T> = StorageMap<_, Twox64Concat, EraIndex, (), OptionQuery>;

    #[pallet::storage]
    pub type NextExpiryEra<T> = StorageValue<_, EraIndex, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn staking_funding_total)]
    pub type StakingFundingTotal<T> = StorageValue<_, u128, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn staking_paid_total)]
    pub type StakingPaidTotal<T> = StorageValue<_, u128, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn treasury_issuance_total)]
    pub type TreasuryIssuanceTotal<T> = StorageValue<_, u128, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn treasury_normal_fee_total)]
    pub type TreasuryNormalFeeTotal<T> = StorageValue<_, u128, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn treasury_tip_total)]
    pub type TreasuryTipTotal<T> = StorageValue<_, u128, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn normal_fee_routed_total)]
    pub type NormalFeeRoutedTotal<T> = StorageValue<_, u128, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn author_tip_paid_total)]
    pub type AuthorTipPaidTotal<T> = StorageValue<_, u128, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn failed_author_tip_total)]
    pub type FailedAuthorTipTotal<T> = StorageValue<_, u128, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn allocation_paused)]
    pub type AllocationPaused<T> = StorageValue<_, bool, ValueQuery>;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        MigrationCompleted {
            at: BlockNumberFor<T>,
            staking_pot: T::AccountId,
            minimum_validator_bond: u128,
            active_validator_count: u32,
            legacy_reward_liability: u128,
            total_reward_budget: u128,
        },
        Activated {
            at: BlockNumberFor<T>,
            legacy_cutoff_era: EraIndex,
            first_eligible_era: EraIndex,
            staking_pot: T::AccountId,
            treasury: T::AccountId,
        },
        EraSkipped {
            era: EraIndex,
            reason: EraSkipReason,
        },
        IssuanceTargetCalculated {
            era: EraIndex,
            staking_target: u128,
            gross_target: u128,
            gross_issued: u128,
            limit: IssuanceLimit,
        },
        EraBudgetReserved {
            era: EraIndex,
            gross_issuance: u128,
            staking_issuance: u128,
            treasury_issuance: u128,
            fee_staking: u128,
            failed_author_tips: u128,
            retained_carry: u128,
            total: u128,
            remaining_allowance: u128,
        },
        NormalFeeStaged {
            amount: u128,
            staking: u128,
            treasury: u128,
        },
        NormalFeeRouted {
            amount: u128,
            staking: u128,
            treasury: u128,
        },
        NormalFeeDeferred {
            staking: u128,
            treasury: u128,
        },
        TipStaged {
            amount: u128,
            author: Option<T::AccountId>,
        },
        TipRouted {
            amount: u128,
            author: T::AccountId,
            author_share: u128,
            treasury: u128,
        },
        TipFallback {
            amount: u128,
            staking: u128,
            treasury: u128,
            reason: TipFallbackReason,
        },
        TipDeferred {
            amount: u128,
            author: Option<T::AccountId>,
        },
        TreasuryInflow {
            source: TreasurySource,
            amount: u128,
            source_total: u128,
        },
        ValidatorRewardPaid {
            era: EraIndex,
            validator: T::AccountId,
            page: Page,
            amount: u128,
            commission: u128,
        },
        NominatorRewardPaid {
            era: EraIndex,
            validator: T::AccountId,
            nominator: T::AccountId,
            page: Page,
            amount: u128,
        },
        RewardPagePaid {
            era: EraIndex,
            validator: T::AccountId,
            page: Page,
            amount: u128,
        },
        LiabilityReleased {
            era: EraIndex,
            amount: u128,
            expired: bool,
        },
        AllocationPaused {
            era: EraIndex,
            reason: AllocationFailureReason,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        NotActive,
        AlreadyActive,
        WrongStorageVersion,
        MissingActiveEra,
        MissingEraStart,
        InvalidAccountDerivation,
        LegacyRoutingPending,
        InvalidActiveValidatorCount,
        ValidatorBelowMinimumBond,
        CommissionAboveMaximum,
        InvalidEra,
        EraAlreadyFinalized,
        InvalidDuration,
        InvalidRewardPoints,
        InvalidExposure,
        TooManyValidators,
        TooManyRewardRecipients,
        InvalidPage,
        AlreadyClaimed,
        ClaimExpired,
        InsufficientLiability,
        InsufficientStakingPot,
        ArithmeticOverflow,
        InvalidCarry,
        AllowanceNotInitialized,
        IssuanceFailed,
        TransferFailed,
        TreasuryTransferFailed,
        AuthorTransferFailed,
        IssuanceInvariantViolated,
        AllowanceInvariantViolated,
        PendingRoutingInconsistent,
        LegacyLiabilityMismatch,
        RewardBudgetExceeded,
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        fn integrity_test() {
            assert_eq!(T::TargetStakingApr::get(), Perbill::from_percent(10));
            assert_eq!(
                T::MaximumValidatorCommission::get(),
                Perbill::from_percent(20)
            );
            assert!(T::RuntimeTechnicalMaxValidators::get() >= T::ActivationValidatorCount::get());
            assert!(T::ActivationValidatorCount::get() == 4);
            assert!(T::SecurityMillisecondsPerYear::get() > 0);
            assert!(T::GrossAnnualIssuanceCeiling::get() > 0);
            assert!(T::TotalStakingRewardBudget::get() > 0);
            assert!(T::ExistingEarnedRewardLiability::get() <= T::TotalStakingRewardBudget::get());
            assert_eq!(
                T::ProtectedCommunityOnboardingBudget::get(),
                10_000_000_000_000_000_000_000_000
            );
            assert_eq!(T::RetiredLegacyRewardReserveTarget::get(), 0);
        }

        fn on_initialize(_n: BlockNumberFor<T>) -> Weight {
            if !Active::<T>::get() || AllocationPaused::<T>::get() {
                return T::DbWeight::get().reads(2);
            }
            if let Err(era) = Self::observe_era_transition() {
                AllocationPaused::<T>::put(true);
                Self::deposit_event(Event::AllocationPaused {
                    era,
                    reason: AllocationFailureReason::EraTransition,
                });
                return T::SecurityWeightInfo::on_initialize(
                    T::RuntimeTechnicalMaxValidators::get(),
                ).saturating_add(T::DbWeight::get().reads_writes(12, 4))
                 .saturating_add(Weight::from_parts(0, 36_864));
            }
            if let Some(active) = pallet_staking::ActiveEra::<T>::get() {
                if Self::expire_one_era(active.index).is_err() {
                    AllocationPaused::<T>::put(true);
                    Self::deposit_event(Event::AllocationPaused {
                        era: active.index,
                        reason: AllocationFailureReason::Expiry,
                    });
                }
            }
            // Conservatively include the new first-era timestamp binding in every active hook.
            T::SecurityWeightInfo::on_initialize(T::RuntimeTechnicalMaxValidators::get())
                .saturating_add(T::DbWeight::get().reads_writes(12, 4))
                .saturating_add(Weight::from_parts(0, 36_864))
        }
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::call_index(0)]
        // The chain-specific liability baseline reads the fresh-genesis marker.
        #[pallet::weight(T::SecurityWeightInfo::activate()
            .saturating_add(T::DbWeight::get().reads(1))
            .saturating_add(Weight::from_parts(0, 4096)))]
        #[transactional]
        pub fn activate(origin: OriginFor<T>) -> DispatchResult {
            T::ActivationOrigin::ensure_origin(origin)?;
            ensure!(!Active::<T>::get(), Error::<T>::AlreadyActive);
            ensure!(
                StorageVersion::get::<Self>() == STORAGE_VERSION,
                Error::<T>::WrongStorageVersion
            );
            ensure!(
                Self::accounts_are_distinct(),
                Error::<T>::InvalidAccountDerivation
            );
            ensure!(
                pallet_reward_reserve::Pallet::<T>::pending_collection_obligations().is_zero(),
                Error::<T>::LegacyRoutingPending
            );

            let active =
                pallet_staking::ActiveEra::<T>::get().ok_or(Error::<T>::MissingActiveEra)?;
            let start = active.start.ok_or(Error::<T>::MissingEraStart)?;
            ensure!(start > 0, Error::<T>::MissingEraStart);
            Self::validate_activation_set(active.index)?;
            ensure!(
                LegacyRewardLiability::<T>::get() == T::ExistingEarnedRewardLiability::get()
                    && pallet_reward_reserve::Pallet::<T>::committed_liabilities()
                        <= LegacyRewardLiability::<T>::get(),
                Error::<T>::LegacyLiabilityMismatch
            );
            Self::remaining_reward_budget()?;
            let legacy_cutoff = if pallet_reward_reserve::Pallet::<T>::legacy_claim_only() {
                pallet_reward_reserve::Pallet::<T>::v14_legacy_cutoff_era()
                    .ok_or(Error::<T>::LegacyLiabilityMismatch)?
            } else {
                pallet_reward_reserve::Pallet::<T>::enter_v14_claim_only(active.index)?;
                active.index
            };

            let first = active
                .index
                .checked_add(1)
                .ok_or(Error::<T>::ArithmeticOverflow)?;
            Active::<T>::put(true);
            ActivatedAt::<T>::put(frame_system::Pallet::<T>::block_number());
            FirstEligibleEra::<T>::put(first);
            LegacyCutoffEra::<T>::put(legacy_cutoff);
            LastObservedEra::<T>::put(active.index);
            LastObservedEraStart::<T>::put(start);
            NextExpiryEra::<T>::put(first);
            AllocationPaused::<T>::put(false);
            Self::deposit_event(Event::Activated {
                at: frame_system::Pallet::<T>::block_number(),
                legacy_cutoff_era: legacy_cutoff,
                first_eligible_era: first,
                staking_pot: Self::staking_pot_account(),
                treasury: Self::treasury_account(),
            });
            Ok(())
        }

        #[pallet::call_index(1)]
        #[pallet::weight(T::SecurityWeightInfo::claim_reward_page(
            T::V14MaxRewardNominatorsPerPage::get()
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

        #[pallet::call_index(2)]
        #[pallet::weight(T::SecurityWeightInfo::retry_normal_fee())]
        pub fn retry_normal_fee(origin: OriginFor<T>) -> DispatchResult {
            let _ = ensure_signed(origin)?;
            Self::settle_pending_normal_fee()
        }

        #[pallet::call_index(3)]
        #[pallet::weight(T::SecurityWeightInfo::retry_tip())]
        pub fn retry_tip(origin: OriginFor<T>, author: Option<T::AccountId>) -> DispatchResult {
            let _ = ensure_signed(origin)?;
            Self::settle_pending_tip(author)
        }
    }

    impl<T: Config> Pallet<T> {
        pub fn record_migration_completed(
            active_validator_count: u32,
            legacy_reward_liability: u128,
        ) -> DispatchResult {
            ensure!(
                MigrationCompletedAt::<T>::get().is_none()
                    && LegacyRewardLiability::<T>::get() == 0
                    && legacy_reward_liability == T::ExistingEarnedRewardLiability::get()
                    && legacy_reward_liability <= T::TotalStakingRewardBudget::get(),
                Error::<T>::LegacyLiabilityMismatch
            );
            let at = frame_system::Pallet::<T>::block_number();
            LegacyRewardLiability::<T>::put(legacy_reward_liability);
            MigrationCompletedAt::<T>::put(at);
            AllocationPaused::<T>::put(true);
            Self::deposit_event(Event::MigrationCompleted {
                at,
                staking_pot: Self::staking_pot_account(),
                minimum_validator_bond: T::MinimumValidatorBond::get(),
                active_validator_count,
                legacy_reward_liability,
                total_reward_budget: T::TotalStakingRewardBudget::get(),
            });
            Ok(())
        }

        pub fn staking_pot_account() -> T::AccountId {
            T::StakingPotPalletId::get().into_account_truncating()
        }

        pub fn treasury_account() -> T::AccountId {
            T::TreasuryPalletId::get().into_account_truncating()
        }

        pub fn fee_collection_account() -> T::AccountId {
            T::V14FeeCollectionPalletId::get().into_account_truncating()
        }

        pub fn staking_pot_balance() -> u128 {
            <T as pallet_staking::Config>::Currency::balance(&Self::staking_pot_account())
        }

        /// Earned legacy rewards are charged once. New paid rewards and open liabilities share
        /// the remainder; released rounding or expired liability can therefore be reused once.
        pub fn accounted_reward_total() -> Result<u128, DispatchError> {
            LegacyRewardLiability::<T>::get()
                .checked_add(StakingPaidTotal::<T>::get())
                .and_then(|used| used.checked_add(CommittedLiabilities::<T>::get()))
                .ok_or_else(|| Error::<T>::ArithmeticOverflow.into())
        }

        /// ceil(20,000,000 * 10^18 / 43,800); never accumulated across skipped eras.
        pub const PRINCIPAL_PER_ERA_CEILING: u128 = 456_621_004_566_210_045_663;

        fn available_principal() -> Result<u128, DispatchError> {
            let obligations = [CommittedLiabilities::<T>::get(),
                UnallocatedFeeStaking::<T>::get(), UnallocatedTipFallback::<T>::get(),
                UnallocatedStakingCarry::<T>::get()]
                .into_iter().try_fold(0u128, |sum, value| sum.checked_add(value))
                .ok_or(Error::<T>::ArithmeticOverflow)?;
            let unencumbered = <T as pallet_staking::Config>::Currency::reducible_balance(
                &Self::staking_pot_account(), Preservation::Expendable, Fortitude::Polite)
                .checked_sub(obligations).ok_or(Error::<T>::InsufficientStakingPot)?;
            Ok(UnallocatedPrincipal::<T>::get().min(unencumbered))
        }

        pub fn remaining_reward_budget() -> Result<u128, DispatchError> {
            let used = Self::accounted_reward_total()?;
            T::TotalStakingRewardBudget::get()
                .checked_sub(used)
                .ok_or_else(|| Error::<T>::RewardBudgetExceeded.into())
        }

        pub fn pending_collection_obligations() -> Option<u128> {
            PendingStakingFee::<T>::get()
                .checked_add(PendingTreasuryFee::<T>::get())?
                .checked_add(PendingTipTotal::<T>::get())
        }

        pub fn accounts_are_distinct() -> bool {
            let staking = Self::staking_pot_account();
            let treasury = Self::treasury_account();
            let collection = Self::fee_collection_account();
            let legacy = pallet_reward_reserve::Pallet::<T>::reward_pot_account();
            staking != treasury
                && staking != collection
                && staking != legacy
                && treasury != collection
                && treasury != legacy
                && collection != legacy
        }

        pub fn fee_split(amount: u128) -> (u128, u128) {
            let treasury = amount / 10;
            (amount - treasury, treasury)
        }

        pub fn receivable_tip_split(amount: u128) -> (u128, u128) {
            let treasury = amount / 10;
            (amount - treasury, treasury)
        }

        pub fn failed_tip_split(amount: u128) -> (u128, u128) {
            let treasury = amount / 2;
            (amount - treasury, treasury)
        }

        pub fn issuance_split(gross: u128, carry: u8) -> Result<(u128, u128, u8), DispatchError> {
            ensure!(carry < 10, Error::<T>::InvalidCarry);
            let numerator = U256::from(gross)
                .checked_mul(U256::from(9u8))
                .and_then(|n| n.checked_add(U256::from(carry)))
                .ok_or(Error::<T>::ArithmeticOverflow)?;
            let staking = Self::u256_to_u128(numerator / U256::from(10u8))?;
            let next = (numerator % U256::from(10u8)).low_u32() as u8;
            let treasury = gross
                .checked_sub(staking)
                .ok_or(Error::<T>::ArithmeticOverflow)?;
            Ok((staking, treasury, next))
        }

        pub fn gross_for_staking_target(target: u128, carry: u8) -> Result<u128, DispatchError> {
            ensure!(carry < 10, Error::<T>::InvalidCarry);
            if target == 0 {
                return Ok(0);
            }
            let required = U256::from(target)
                .checked_mul(U256::from(10u8))
                .ok_or(Error::<T>::ArithmeticOverflow)?;
            let adjusted = required.saturating_sub(U256::from(carry));
            Self::u256_to_u128(
                adjusted
                    .checked_add(U256::from(8u8))
                    .ok_or(Error::<T>::ArithmeticOverflow)?
                    / U256::from(9u8),
            )
        }

        pub fn target_staking_issuance(
            eligible_stake: u128,
            duration_millis: u64,
            remainder: u128,
        ) -> Result<(u128, u128), DispatchError> {
            let year = T::SecurityMillisecondsPerYear::get();
            ensure!(
                year > 0 && duration_millis > 0 && duration_millis <= year,
                Error::<T>::InvalidDuration
            );
            let denominator = U256::from(PERBILL_DENOMINATOR)
                .checked_mul(U256::from(year))
                .ok_or(Error::<T>::ArithmeticOverflow)?;
            ensure!(
                U256::from(remainder) < denominator,
                Error::<T>::ArithmeticOverflow
            );
            let numerator = U256::from(eligible_stake)
                .checked_mul(U256::from(T::TargetStakingApr::get().deconstruct()))
                .and_then(|n| n.checked_mul(U256::from(duration_millis)))
                .and_then(|n| n.checked_add(U256::from(remainder)))
                .ok_or(Error::<T>::ArithmeticOverflow)?;
            Ok((
                Self::u256_to_u128(numerator / denominator)?,
                Self::u256_to_u128(numerator % denominator)?,
            ))
        }

        pub fn stage_normal_fee(amount: u128) -> DispatchResult {
            ensure!(Active::<T>::get(), Error::<T>::NotActive);
            if amount == 0 {
                return Ok(());
            }
            let (staking, treasury) = Self::fee_split(amount);
            let next_staking = Self::checked_add(PendingStakingFee::<T>::get(), staking)?;
            let next_treasury = Self::checked_add(PendingTreasuryFee::<T>::get(), treasury)?;
            PendingStakingFee::<T>::put(next_staking);
            PendingTreasuryFee::<T>::put(next_treasury);
            Self::deposit_event(Event::NormalFeeStaged {
                amount,
                staking,
                treasury,
            });
            Ok(())
        }

        pub fn settle_pending_normal_fee() -> DispatchResult {
            with_transaction(|| {
                let result = Self::do_settle_pending_normal_fee();
                if result.is_ok() {
                    TransactionOutcome::Commit(result)
                } else {
                    TransactionOutcome::Rollback(result)
                }
            })
        }

        fn do_settle_pending_normal_fee() -> DispatchResult {
            let staking = PendingStakingFee::<T>::get();
            let treasury = PendingTreasuryFee::<T>::get();
            let total = staking
                .checked_add(treasury)
                .ok_or(Error::<T>::ArithmeticOverflow)?;
            if total == 0 {
                return Ok(());
            }
            Self::ensure_collection_solvency()?;
            let issuance_before = <T as pallet_staking::Config>::Currency::total_issuance();
            let allowance_before = T::Issuance::remaining_allowance();
            Self::transfer_from_collection(&Self::staking_pot_account(), staking, false)?;
            Self::transfer_from_collection(&Self::treasury_account(), treasury, true)?;
            ensure!(
                <T as pallet_staking::Config>::Currency::total_issuance() == issuance_before,
                Error::<T>::IssuanceInvariantViolated
            );
            ensure!(
                T::Issuance::remaining_allowance() == allowance_before,
                Error::<T>::AllowanceInvariantViolated
            );
            UnallocatedFeeStaking::<T>::put(Self::checked_add(
                UnallocatedFeeStaking::<T>::get(),
                staking,
            )?);
            StakingFundingTotal::<T>::put(Self::checked_add(
                StakingFundingTotal::<T>::get(),
                staking,
            )?);
            TreasuryNormalFeeTotal::<T>::put(Self::checked_add(
                TreasuryNormalFeeTotal::<T>::get(),
                treasury,
            )?);
            NormalFeeRoutedTotal::<T>::put(Self::checked_add(
                NormalFeeRoutedTotal::<T>::get(),
                total,
            )?);
            PendingStakingFee::<T>::kill();
            PendingTreasuryFee::<T>::kill();
            Self::ensure_staking_solvency()?;
            Self::deposit_event(Event::NormalFeeRouted {
                amount: total,
                staking,
                treasury,
            });
            Self::deposit_event(Event::TreasuryInflow {
                source: TreasurySource::NormalFee,
                amount: treasury,
                source_total: TreasuryNormalFeeTotal::<T>::get(),
            });
            Ok(())
        }

        pub fn note_normal_fee_deferred() {
            Self::deposit_event(Event::NormalFeeDeferred {
                staking: PendingStakingFee::<T>::get(),
                treasury: PendingTreasuryFee::<T>::get(),
            });
        }

        pub fn stage_tip(author: Option<T::AccountId>, amount: u128) -> DispatchResult {
            ensure!(Active::<T>::get(), Error::<T>::NotActive);
            if amount == 0 {
                return Ok(());
            }
            let next_total = Self::checked_add(PendingTipTotal::<T>::get(), amount)?;
            match author.clone() {
                Some(ref who) => {
                    let next = Self::checked_add(PendingAuthorTips::<T>::get(who), amount)?;
                    PendingAuthorTips::<T>::insert(who, next);
                }
                None => {
                    let next = Self::checked_add(PendingMissingAuthorTips::<T>::get(), amount)?;
                    PendingMissingAuthorTips::<T>::put(next);
                }
            }
            PendingTipTotal::<T>::put(next_total);
            Self::deposit_event(Event::TipStaged { amount, author });
            Ok(())
        }

        pub fn note_tip_deferred(author: Option<T::AccountId>) {
            let amount = match author.as_ref() {
                Some(who) => PendingAuthorTips::<T>::get(who),
                None => PendingMissingAuthorTips::<T>::get(),
            };
            Self::deposit_event(Event::TipDeferred { amount, author });
        }

        pub fn settle_pending_tip(author: Option<T::AccountId>) -> DispatchResult {
            match author {
                None => {
                    let amount = PendingMissingAuthorTips::<T>::get();
                    Self::settle_tip_fallback(None, amount, TipFallbackReason::MissingAuthor)
                }
                Some(who) => {
                    let amount = PendingAuthorTips::<T>::get(&who);
                    if amount == 0 {
                        return Ok(());
                    }
                    if Self::is_protocol_account(&who) {
                        return Self::settle_tip_fallback(
                            Some(who),
                            amount,
                            TipFallbackReason::UnreceivableAuthor,
                        );
                    }
                    match Self::settle_receivable_tip(&who, amount) {
                        Ok(()) => Ok(()),
                        Err(DispatchError::Other("author-unreceivable")) => {
                            Self::settle_tip_fallback(
                                Some(who),
                                amount,
                                TipFallbackReason::UnreceivableAuthor,
                            )
                        }
                        Err(error) => Err(error),
                    }
                }
            }
        }

        fn settle_receivable_tip(author: &T::AccountId, amount: u128) -> DispatchResult {
            with_transaction(|| {
                let result = Self::do_settle_receivable_tip(author, amount);
                if result.is_ok() {
                    TransactionOutcome::Commit(result)
                } else {
                    TransactionOutcome::Rollback(result)
                }
            })
        }

        fn do_settle_receivable_tip(author: &T::AccountId, amount: u128) -> DispatchResult {
            let (author_share, treasury) = Self::receivable_tip_split(amount);
            Self::ensure_collection_solvency()?;
            let issuance_before = <T as pallet_staking::Config>::Currency::total_issuance();
            let allowance_before = T::Issuance::remaining_allowance();
            if !author_share.is_zero() {
                <T as pallet_staking::Config>::Currency::transfer(
                    &Self::fee_collection_account(),
                    author,
                    author_share,
                    Preservation::Preserve,
                )
                .map_err(|_| DispatchError::Other("author-unreceivable"))?;
            }
            Self::transfer_from_collection(&Self::treasury_account(), treasury, true)?;
            ensure!(
                <T as pallet_staking::Config>::Currency::total_issuance() == issuance_before,
                Error::<T>::IssuanceInvariantViolated
            );
            ensure!(
                T::Issuance::remaining_allowance() == allowance_before,
                Error::<T>::AllowanceInvariantViolated
            );
            PendingAuthorTips::<T>::remove(author);
            PendingTipTotal::<T>::put(
                PendingTipTotal::<T>::get()
                    .checked_sub(amount)
                    .ok_or(Error::<T>::PendingRoutingInconsistent)?,
            );
            AuthorTipPaidTotal::<T>::put(Self::checked_add(
                AuthorTipPaidTotal::<T>::get(),
                author_share,
            )?);
            TreasuryTipTotal::<T>::put(Self::checked_add(TreasuryTipTotal::<T>::get(), treasury)?);
            Self::deposit_event(Event::TipRouted {
                amount,
                author: author.clone(),
                author_share,
                treasury,
            });
            Self::deposit_event(Event::TreasuryInflow {
                source: TreasurySource::ReceivableTip,
                amount: treasury,
                source_total: TreasuryTipTotal::<T>::get(),
            });
            Ok(())
        }

        fn settle_tip_fallback(
            author: Option<T::AccountId>,
            amount: u128,
            reason: TipFallbackReason,
        ) -> DispatchResult {
            if amount == 0 {
                return Ok(());
            }
            with_transaction(|| {
                let result = Self::do_settle_tip_fallback(author, amount, reason);
                if result.is_ok() {
                    TransactionOutcome::Commit(result)
                } else {
                    TransactionOutcome::Rollback(result)
                }
            })
        }

        fn do_settle_tip_fallback(
            author: Option<T::AccountId>,
            amount: u128,
            reason: TipFallbackReason,
        ) -> DispatchResult {
            let (staking, treasury) = Self::failed_tip_split(amount);
            Self::ensure_collection_solvency()?;
            let issuance_before = <T as pallet_staking::Config>::Currency::total_issuance();
            let allowance_before = T::Issuance::remaining_allowance();
            Self::transfer_from_collection(&Self::staking_pot_account(), staking, false)?;
            Self::transfer_from_collection(&Self::treasury_account(), treasury, true)?;
            ensure!(
                <T as pallet_staking::Config>::Currency::total_issuance() == issuance_before,
                Error::<T>::IssuanceInvariantViolated
            );
            ensure!(
                T::Issuance::remaining_allowance() == allowance_before,
                Error::<T>::AllowanceInvariantViolated
            );
            if let Some(who) = author {
                PendingAuthorTips::<T>::remove(who);
            } else {
                PendingMissingAuthorTips::<T>::kill();
            }
            PendingTipTotal::<T>::put(
                PendingTipTotal::<T>::get()
                    .checked_sub(amount)
                    .ok_or(Error::<T>::PendingRoutingInconsistent)?,
            );
            UnallocatedTipFallback::<T>::put(Self::checked_add(
                UnallocatedTipFallback::<T>::get(),
                staking,
            )?);
            StakingFundingTotal::<T>::put(Self::checked_add(
                StakingFundingTotal::<T>::get(),
                staking,
            )?);
            FailedAuthorTipTotal::<T>::put(Self::checked_add(
                FailedAuthorTipTotal::<T>::get(),
                amount,
            )?);
            TreasuryTipTotal::<T>::put(Self::checked_add(TreasuryTipTotal::<T>::get(), treasury)?);
            Self::ensure_staking_solvency()?;
            Self::deposit_event(Event::TipFallback {
                amount,
                staking,
                treasury,
                reason,
            });
            Self::deposit_event(Event::TreasuryInflow {
                source: TreasurySource::FailedAuthorTip,
                amount: treasury,
                source_total: TreasuryTipTotal::<T>::get(),
            });
            Ok(())
        }

        pub fn finalize_completed_era(
            era: EraIndex,
            duration: u64,
            end_millis: u64,
        ) -> DispatchResult {
            with_transaction(|| {
                let result = Self::do_finalize_completed_era(era, duration, end_millis);
                if result.is_ok() {
                    TransactionOutcome::Commit(result)
                } else {
                    TransactionOutcome::Rollback(result)
                }
            })
        }

        fn do_finalize_completed_era(
            era: EraIndex,
            duration: u64,
            end_millis: u64,
        ) -> DispatchResult {
            ensure!(
                Active::<T>::get() && !AllocationPaused::<T>::get(),
                Error::<T>::NotActive
            );
            ensure!(
                era >= FirstEligibleEra::<T>::get().ok_or(Error::<T>::InvalidEra)?,
                Error::<T>::InvalidEra
            );
            ensure!(
                !EraBudgets::<T>::contains_key(era),
                Error::<T>::EraAlreadyFinalized
            );
            ensure!(
                duration > 0 && duration <= T::SecurityMillisecondsPerYear::get(),
                Error::<T>::InvalidDuration
            );
            let completed_start = end_millis
                .checked_sub(duration)
                .ok_or(Error::<T>::InvalidDuration)?;
            let (eligible_stake, eligible_points, validators, observed_points) =
                Self::eligible_summary(era)?;
            if observed_points == 0 {
                Self::deposit_event(Event::EraSkipped {
                    era,
                    reason: EraSkipReason::NoRewardPoints,
                });
                return Ok(());
            }
            if eligible_stake == 0 || eligible_points == 0 {
                Self::deposit_event(Event::EraSkipped {
                    era,
                    reason: EraSkipReason::NoEligibleValidators,
                });
                return Ok(());
            }
            let reward_remaining = Self::remaining_reward_budget()?;
            if reward_remaining == 0 {
                Self::deposit_event(Event::EraSkipped {
                    era,
                    reason: EraSkipReason::LifetimeRewardBudgetExhausted,
                });
                return Ok(());
            }

            let mut period_start = EmissionPeriodStart::<T>::get().unwrap_or(completed_start);
            let mut period_used = GrossIssuedInPeriod::<T>::get();
            ensure!(end_millis >= period_start, Error::<T>::InvalidDuration);
            let year = T::SecurityMillisecondsPerYear::get();
            let elapsed = end_millis - period_start;
            if elapsed >= year {
                let periods = elapsed / year;
                period_start = period_start
                    .checked_add(
                        periods
                            .checked_mul(year)
                            .ok_or(Error::<T>::ArithmeticOverflow)?,
                    )
                    .ok_or(Error::<T>::ArithmeticOverflow)?;
                period_used = 0;
            }

            let target_before = TargetRemainder::<T>::get();
            let (staking_target, target_after) =
                Self::target_staking_issuance(eligible_stake, duration, target_before)?;
            let principal = if UnallocatedPrincipal::<T>::get() == 0 { 0 } else {
                Self::available_principal()?.min(Self::PRINCIPAL_PER_ERA_CEILING)
                    .min(staking_target).min(reward_remaining)
            };
            let issuance_target = staking_target.checked_sub(principal)
                .ok_or(Error::<T>::ArithmeticOverflow)?;
            let carry_before = IssuanceSplitCarry::<T>::get();
            let gross_target = Self::gross_for_staking_target(issuance_target, carry_before)?;
            let period_remaining = T::GrossAnnualIssuanceCeiling::get()
                .checked_sub(period_used)
                .ok_or(Error::<T>::ArithmeticOverflow)?;
            let allowance =
                T::Issuance::remaining_allowance().ok_or(Error::<T>::AllowanceNotInitialized)?;
            let reward_gross_limit =
                Self::gross_for_staking_target(reward_remaining - principal, carry_before)?;
            let annual_limited = gross_target > period_remaining;
            let allowance_limited = gross_target > allowance;
            let reward_limited = gross_target > reward_gross_limit;
            let limit = match (annual_limited, allowance_limited, reward_limited) {
                (false, false, false) => IssuanceLimit::None,
                (true, false, false) => IssuanceLimit::AnnualCeiling,
                (false, true, false) => IssuanceLimit::LifetimeAllowance,
                (true, true, false) => IssuanceLimit::AnnualCeilingAndLifetimeAllowance,
                (false, false, true) => IssuanceLimit::LifetimeRewardBudget,
                _ => IssuanceLimit::Multiple,
            };
            let gross = gross_target
                .min(period_remaining)
                .min(allowance)
                .min(reward_gross_limit);
            let (staking_issuance, treasury_issuance, carry_after) =
                Self::issuance_split(gross, carry_before)?;
            ensure!(
                staking_issuance <= reward_remaining - principal,
                Error::<T>::RewardBudgetExceeded
            );
            Self::deposit_event(Event::IssuanceTargetCalculated {
                era,
                staking_target,
                gross_target,
                gross_issued: gross,
                limit,
            });

            let fees_available = UnallocatedFeeStaking::<T>::get();
            let failed_tips_available = UnallocatedTipFallback::<T>::get();
            let retained_available = UnallocatedStakingCarry::<T>::get();
            let mut source_remaining = reward_remaining
                .checked_sub(principal)
                .and_then(|room| room.checked_sub(staking_issuance))
                .ok_or(Error::<T>::ArithmeticOverflow)?;
            let fees = fees_available.min(source_remaining);
            source_remaining = source_remaining
                .checked_sub(fees)
                .ok_or(Error::<T>::ArithmeticOverflow)?;
            let failed_tips = failed_tips_available.min(source_remaining);
            source_remaining = source_remaining
                .checked_sub(failed_tips)
                .ok_or(Error::<T>::ArithmeticOverflow)?;
            let retained = retained_available.min(source_remaining);
            source_remaining = source_remaining
                .checked_sub(retained)
                .ok_or(Error::<T>::ArithmeticOverflow)?;
            let total = reward_remaining
                .checked_sub(source_remaining)
                .ok_or(Error::<T>::ArithmeticOverflow)?;
            if total == 0 {
                TargetRemainder::<T>::put(target_after);
                EmissionPeriodStart::<T>::put(period_start);
                GrossIssuedInPeriod::<T>::put(period_used);
                Self::deposit_event(Event::EraSkipped {
                    era,
                    reason: EraSkipReason::NoFundedBudget,
                });
                return Ok(());
            }

            if gross > 0 {
                T::Issuance::mint_split(
                    &Self::staking_pot_account(),
                    staking_issuance,
                    &Self::treasury_account(),
                    treasury_issuance,
                    carry_before,
                    carry_after,
                )
                .map_err(|_| Error::<T>::IssuanceFailed)?;
                StakingFundingTotal::<T>::put(Self::checked_add(
                    StakingFundingTotal::<T>::get(),
                    staking_issuance,
                )?);
                TreasuryIssuanceTotal::<T>::put(Self::checked_add(
                    TreasuryIssuanceTotal::<T>::get(),
                    treasury_issuance,
                )?);
                Self::deposit_event(Event::TreasuryInflow {
                    source: TreasurySource::NewIssuance,
                    amount: treasury_issuance,
                    source_total: TreasuryIssuanceTotal::<T>::get(),
                });
            }

            // Reserving principal moves existing pot backing into the era liability exactly
            // once. It neither transfers/mints money nor replenishes the issuance allowance.
            UnallocatedPrincipal::<T>::try_mutate(|left| -> DispatchResult {
                *left = left.checked_sub(principal).ok_or(Error::<T>::ArithmeticOverflow)?;
                Ok(())
            })?;
            PrincipalReservedTotal::<T>::put(Self::checked_add(
                PrincipalReservedTotal::<T>::get(), principal)?);
            EraPrincipal::<T>::insert(era, principal);
            for (validator, points) in validators {
                EligibleValidatorPoints::<T>::insert(era, validator, points);
            }
            EraBudgets::<T>::insert(
                era,
                EraBudget {
                    gross_issuance: gross,
                    staking_issuance,
                    treasury_issuance,
                    fee_staking: fees,
                    failed_author_tips: failed_tips,
                    retained_carry: retained,
                    total,
                    eligible_stake,
                    eligible_points,
                    duration_millis: duration,
                },
            );
            EraLiability::<T>::insert(era, total);
            CommittedLiabilities::<T>::put(Self::checked_add(
                CommittedLiabilities::<T>::get(),
                total,
            )?);
            UnallocatedFeeStaking::<T>::put(fees_available - fees);
            UnallocatedTipFallback::<T>::put(failed_tips_available - failed_tips);
            UnallocatedStakingCarry::<T>::put(retained_available - retained);
            Self::remaining_reward_budget()?;
            TargetRemainder::<T>::put(target_after);
            IssuanceSplitCarry::<T>::put(carry_after);
            EmissionPeriodStart::<T>::put(period_start);
            GrossIssuedInPeriod::<T>::put(Self::checked_add(period_used, gross)?);
            Self::ensure_staking_solvency()?;
            Self::deposit_event(Event::EraBudgetReserved {
                era,
                gross_issuance: gross,
                staking_issuance,
                treasury_issuance,
                fee_staking: fees,
                failed_author_tips: failed_tips,
                retained_carry: retained,
                total,
                remaining_allowance: T::Issuance::remaining_allowance()
                    .ok_or(Error::<T>::AllowanceNotInitialized)?,
            });
            Ok(())
        }

        fn do_claim_reward_page(
            _caller: T::AccountId,
            era: EraIndex,
            validator: T::AccountId,
            page: Page,
        ) -> DispatchResult {
            ensure!(Active::<T>::get(), Error::<T>::NotActive);
            let active =
                pallet_staking::ActiveEra::<T>::get().ok_or(Error::<T>::MissingActiveEra)?;
            ensure!(era < active.index, Error::<T>::InvalidEra);
            ensure!(
                era >= active.index.saturating_sub(T::HistoryDepth::get()),
                Error::<T>::ClaimExpired
            );
            ensure!(
                !ExpiredEras::<T>::contains_key(era),
                Error::<T>::ClaimExpired
            );
            ensure!(
                !ClaimedPages::<T>::contains_key(era, (validator.clone(), page)),
                Error::<T>::AlreadyClaimed
            );
            let budget = EraBudgets::<T>::get(era).ok_or(Error::<T>::InvalidEra)?;
            let points = EligibleValidatorPoints::<T>::get(era, &validator)
                .ok_or(Error::<T>::InvalidRewardPoints)?;
            let page_count = Self::reward_page_count(era, &validator)?;
            ensure!(page < page_count, Error::<T>::InvalidPage);
            let exposure = pallet_staking::EraInfo::<T>::get_paged_exposure(era, &validator, page)
                .ok_or(Error::<T>::InvalidExposure)?;
            ensure!(
                exposure.others().len() as u32 <= T::V14MaxRewardNominatorsPerPage::get(),
                Error::<T>::TooManyRewardRecipients
            );
            let exposure_total = exposure.total();
            ensure!(
                exposure_total > 0 && exposure.own() <= exposure_total,
                Error::<T>::InvalidExposure
            );
            let (_, validator_own) = Self::exposure_summary(era, &validator)?;
            ensure!(
                validator_own >= T::MinimumValidatorBond::get(),
                Error::<T>::InvalidExposure
            );
            let page_nominator_total = exposure
                .others()
                .iter()
                .try_fold(0u128, |sum, nominator| sum.checked_add(nominator.value))
                .ok_or(Error::<T>::ArithmeticOverflow)?;
            let expected_page_total = exposure
                .own()
                .checked_add(page_nominator_total)
                .ok_or(Error::<T>::ArithmeticOverflow)?;
            ensure!(
                expected_page_total == exposure.page_total()
                    && expected_page_total <= exposure_total,
                Error::<T>::InvalidExposure
            );
            let validator_group =
                Self::mul_div(budget.total, points as u128, budget.eligible_points as u128)?;
            let commission_rate =
                pallet_staking::ErasValidatorPrefs::<T>::get(era, &validator).commission;
            ensure!(
                commission_rate <= T::MaximumValidatorCommission::get(),
                Error::<T>::CommissionAboveMaximum
            );
            let commission = Self::mul_div(
                validator_group,
                commission_rate.deconstruct() as u128,
                PERBILL_DENOMINATOR,
            )?;
            let shared = validator_group
                .checked_sub(commission)
                .ok_or(Error::<T>::ArithmeticOverflow)?;
            let mut payouts: Vec<(T::AccountId, u128, bool)> = Vec::new();
            let mut page_total = 0u128;
            let validator_amount = if page == 0 {
                let self_share = Self::mul_div(shared, exposure.own(), exposure_total)?;
                commission
                    .checked_add(self_share)
                    .ok_or(Error::<T>::ArithmeticOverflow)?
            } else {
                0
            };
            if validator_amount > 0 {
                payouts.push((validator.clone(), validator_amount, true));
                page_total = validator_amount;
            }
            for (index, nominator) in exposure.others().iter().enumerate() {
                ensure!(
                    !Self::is_protocol_account(&nominator.who) && nominator.who != validator,
                    Error::<T>::InvalidExposure
                );
                ensure!(
                    nominator.value <= exposure_total,
                    Error::<T>::InvalidExposure
                );
                for prior in exposure.others().iter().take(index) {
                    ensure!(prior.who != nominator.who, Error::<T>::InvalidExposure);
                }
                let amount = Self::mul_div(shared, nominator.value, exposure_total)?;
                page_total = page_total
                    .checked_add(amount)
                    .ok_or(Error::<T>::ArithmeticOverflow)?;
                if amount > 0 {
                    payouts.push((nominator.who.clone(), amount, false));
                }
            }
            let liability = EraLiability::<T>::get(era);
            let validator_paid = ValidatorPaid::<T>::get(era, &validator);
            let next_validator_paid = validator_paid
                .checked_add(page_total)
                .ok_or(Error::<T>::ArithmeticOverflow)?;
            ensure!(
                next_validator_paid <= validator_group
                    && liability >= page_total
                    && CommittedLiabilities::<T>::get() >= page_total,
                Error::<T>::InsufficientLiability
            );
            Self::ensure_staking_solvency()?;
            let issuance_before = <T as pallet_staking::Config>::Currency::total_issuance();
            let allowance_before = T::Issuance::remaining_allowance();
            for (recipient, amount, _) in payouts.iter() {
                <T as pallet_staking::Config>::Currency::transfer(
                    &Self::staking_pot_account(),
                    recipient,
                    *amount,
                    Preservation::Expendable,
                )
                .map_err(|_| Error::<T>::TransferFailed)?;
            }
            ensure!(
                <T as pallet_staking::Config>::Currency::total_issuance() == issuance_before,
                Error::<T>::IssuanceInvariantViolated
            );
            ensure!(
                T::Issuance::remaining_allowance() == allowance_before,
                Error::<T>::AllowanceInvariantViolated
            );
            ClaimedPages::<T>::insert(era, (validator.clone(), page), ());
            ValidatorPaid::<T>::insert(era, &validator, next_validator_paid);
            EraPaid::<T>::insert(era, Self::checked_add(EraPaid::<T>::get(era), page_total)?);
            EraLiability::<T>::insert(era, liability - page_total);
            CommittedLiabilities::<T>::put(CommittedLiabilities::<T>::get() - page_total);
            StakingPaidTotal::<T>::put(Self::checked_add(
                StakingPaidTotal::<T>::get(),
                page_total,
            )?);
            Self::deposit_event(Event::ValidatorRewardPaid {
                era,
                validator: validator.clone(),
                page,
                amount: validator_amount,
                commission: if page == 0 { commission } else { 0 },
            });
            for (recipient, amount, is_validator) in payouts {
                if !is_validator {
                    Self::deposit_event(Event::NominatorRewardPaid {
                        era,
                        validator: validator.clone(),
                        nominator: recipient,
                        page,
                        amount,
                    });
                }
            }
            Self::deposit_event(Event::RewardPagePaid {
                era,
                validator,
                page,
                amount: page_total,
            });
            if Self::all_pages_claimed(era)? {
                Self::release_remaining_liability(era, false)?;
            }
            Self::remaining_reward_budget()?;
            Self::ensure_staking_solvency()?;
            Ok(())
        }

        fn eligible_summary(era: EraIndex) -> Result<EligibleSummary<T::AccountId>, DispatchError> {
            let reward_points = pallet_staking::ErasRewardPoints::<T>::get(era);
            ensure!(
                reward_points.individual.len() as u32 <= T::RuntimeTechnicalMaxValidators::get(),
                Error::<T>::TooManyValidators
            );
            let observed_total = reward_points
                .individual
                .values()
                .try_fold(0u32, |sum, value| sum.checked_add(*value))
                .ok_or(Error::<T>::ArithmeticOverflow)?;
            ensure!(
                observed_total == reward_points.total,
                Error::<T>::InvalidRewardPoints
            );
            let mut stake = 0u128;
            let mut points = 0u32;
            let mut validators = Vec::new();
            for (validator, validator_points) in reward_points.individual.iter() {
                if *validator_points == 0 {
                    continue;
                }
                let (total, own) = Self::exposure_summary(era, validator)?;
                if own < T::MinimumValidatorBond::get() {
                    continue;
                }
                ensure!(
                    pallet_staking::ErasValidatorPrefs::<T>::get(era, validator).commission
                        <= T::MaximumValidatorCommission::get(),
                    Error::<T>::CommissionAboveMaximum
                );
                stake = stake
                    .checked_add(total)
                    .ok_or(Error::<T>::ArithmeticOverflow)?;
                points = points
                    .checked_add(*validator_points)
                    .ok_or(Error::<T>::ArithmeticOverflow)?;
                validators.push((validator.clone(), *validator_points));
            }
            Ok((stake, points, validators, observed_total))
        }

        fn exposure_summary(
            era: EraIndex,
            validator: &T::AccountId,
        ) -> Result<(u128, u128), DispatchError> {
            if let Some(overview) = pallet_staking::ErasStakersOverview::<T>::get(era, validator) {
                return Ok((overview.total, overview.own));
            }
            ensure!(
                pallet_staking::ErasStakersClipped::<T>::contains_key(era, validator),
                Error::<T>::InvalidExposure
            );
            let exposure = pallet_staking::ErasStakersClipped::<T>::get(era, validator);
            Ok((exposure.total, exposure.own))
        }

        fn validate_activation_set(era: EraIndex) -> DispatchResult {
            ensure!(
                pallet_staking::ValidatorCount::<T>::get() == T::ActivationValidatorCount::get(),
                Error::<T>::InvalidActiveValidatorCount
            );
            let mut count = 0u32;
            for (validator, overview) in pallet_staking::ErasStakersOverview::<T>::iter_prefix(era)
            {
                count = count.checked_add(1).ok_or(Error::<T>::ArithmeticOverflow)?;
                ensure!(
                    count <= T::RuntimeTechnicalMaxValidators::get(),
                    Error::<T>::TooManyValidators
                );
                ensure!(
                    overview.own >= T::MinimumValidatorBond::get(),
                    Error::<T>::ValidatorBelowMinimumBond
                );
                ensure!(
                    pallet_staking::ErasValidatorPrefs::<T>::get(era, validator).commission
                        <= T::MaximumValidatorCommission::get(),
                    Error::<T>::CommissionAboveMaximum
                );
            }
            ensure!(
                count == T::ActivationValidatorCount::get(),
                Error::<T>::InvalidActiveValidatorCount
            );
            Ok(())
        }

        fn observe_era_transition() -> Result<(), EraIndex> {
            let active = pallet_staking::ActiveEra::<T>::get().ok_or(0u32)?;
            let last = LastObservedEra::<T>::get().ok_or(active.index)?;
            if active.index == last {
                // At fresh genesis the first timestamp is not known. Bind the real era start
                // once Staking has finalized its first block; never invent a wall-clock origin.
                if LastObservedEraStart::<T>::get().is_none() {
                    if let Some(start) = active.start {
                        LastObservedEraStart::<T>::put(start);
                    }
                }
                return Ok(());
            }
            // Staking sets a new era's start at on_finalize, after this hook. Defer its
            // accounting until the next block instead of permanently pausing allocation.
            if active.start.is_none() {
                return Ok(());
            }
            let expected = last.checked_add(1).ok_or(last)?;
            if active.index != expected {
                return Err(last);
            }
            let old_start = LastObservedEraStart::<T>::get().ok_or(last)?;
            let new_start = active.start.ok_or(last)?;
            let duration = new_start.checked_sub(old_start).ok_or(last)?;
            let first_eligible = FirstEligibleEra::<T>::get().ok_or(last)?;
            if last >= first_eligible {
                Self::finalize_completed_era(last, duration, new_start).map_err(|_| last)?;
            }
            LastObservedEra::<T>::put(active.index);
            LastObservedEraStart::<T>::put(new_start);
            Ok(())
        }

        fn expire_one_era(active: EraIndex) -> DispatchResult {
            let Some(era) = NextExpiryEra::<T>::get() else {
                return Ok(());
            };
            let history_plus_one = T::HistoryDepth::get()
                .checked_add(1)
                .ok_or(Error::<T>::ArithmeticOverflow)?;
            // Before a full history window exists there is intentionally no expirable era.
            let Some(limit) = active.checked_sub(history_plus_one) else {
                return Ok(());
            };
            if era > limit {
                return Ok(());
            }
            if EraBudgets::<T>::contains_key(era) && !ExpiredEras::<T>::contains_key(era) {
                with_transaction(|| {
                    let result = Self::release_remaining_liability(era, true);
                    if result.is_ok() {
                        ExpiredEras::<T>::insert(era, ());
                        TransactionOutcome::Commit(result)
                    } else {
                        TransactionOutcome::Rollback(result)
                    }
                })?;
            }
            NextExpiryEra::<T>::put(era.checked_add(1).ok_or(Error::<T>::ArithmeticOverflow)?);
            Ok(())
        }

        fn reward_page_count(
            era: EraIndex,
            validator: &T::AccountId,
        ) -> Result<Page, DispatchError> {
            let count = if let Some(overview) =
                pallet_staking::ErasStakersOverview::<T>::get(era, validator)
            {
                if overview.page_count == 0 && overview.own > 0 {
                    1
                } else {
                    overview.page_count
                }
            } else if pallet_staking::ErasStakersClipped::<T>::contains_key(era, validator) {
                1
            } else {
                return Err(Error::<T>::InvalidExposure.into());
            };
            ensure!(
                count > 0 && count <= T::MaxExposurePages::get(),
                Error::<T>::InvalidExposure
            );
            Ok(count)
        }

        fn all_pages_claimed(era: EraIndex) -> Result<bool, DispatchError> {
            let mut validators = 0u32;
            for (validator, _) in EligibleValidatorPoints::<T>::iter_prefix(era) {
                validators = validators
                    .checked_add(1)
                    .ok_or(Error::<T>::ArithmeticOverflow)?;
                ensure!(
                    validators <= T::RuntimeTechnicalMaxValidators::get(),
                    Error::<T>::TooManyValidators
                );
                for page in 0..Self::reward_page_count(era, &validator)? {
                    if !ClaimedPages::<T>::contains_key(era, (validator.clone(), page)) {
                        return Ok(false);
                    }
                }
            }
            Ok(validators > 0)
        }

        fn release_remaining_liability(era: EraIndex, expired: bool) -> DispatchResult {
            let amount = EraLiability::<T>::take(era);
            if amount > 0 {
                CommittedLiabilities::<T>::put(
                    CommittedLiabilities::<T>::get()
                        .checked_sub(amount)
                        .ok_or(Error::<T>::InsufficientLiability)?,
                );
                UnallocatedStakingCarry::<T>::put(Self::checked_add(
                    UnallocatedStakingCarry::<T>::get(),
                    amount,
                )?);
                Self::deposit_event(Event::LiabilityReleased {
                    era,
                    amount,
                    expired,
                });
            }
            Ok(())
        }

        fn transfer_from_collection(
            destination: &T::AccountId,
            amount: u128,
            treasury: bool,
        ) -> DispatchResult {
            if amount == 0 {
                return Ok(());
            }
            <T as pallet_staking::Config>::Currency::transfer(
                &Self::fee_collection_account(),
                destination,
                amount,
                Preservation::Preserve,
            )
            .map_err(|_| {
                if treasury {
                    Error::<T>::TreasuryTransferFailed
                } else {
                    Error::<T>::TransferFailed
                }
            })?;
            Ok(())
        }

        fn ensure_staking_solvency() -> DispatchResult {
            let obligations = [
                CommittedLiabilities::<T>::get(),
                UnallocatedFeeStaking::<T>::get(),
                UnallocatedTipFallback::<T>::get(),
                UnallocatedStakingCarry::<T>::get(),
            ]
            .into_iter()
            .try_fold(0u128, |sum, value| sum.checked_add(value))
            .ok_or(Error::<T>::ArithmeticOverflow)?;
            ensure!(
                <T as pallet_staking::Config>::Currency::reducible_balance(
                    &Self::staking_pot_account(),
                    Preservation::Expendable,
                    Fortitude::Polite,
                ) >= obligations,
                Error::<T>::InsufficientStakingPot
            );
            Ok(())
        }

        fn ensure_collection_solvency() -> DispatchResult {
            let obligations =
                Self::pending_collection_obligations().ok_or(Error::<T>::ArithmeticOverflow)?;
            ensure!(
                <T as pallet_staking::Config>::Currency::reducible_balance(
                    &Self::fee_collection_account(),
                    Preservation::Preserve,
                    Fortitude::Polite,
                ) >= obligations,
                Error::<T>::PendingRoutingInconsistent
            );
            Ok(())
        }

        fn is_protocol_account(account: &T::AccountId) -> bool {
            account == &Self::staking_pot_account()
                || account == &Self::treasury_account()
                || account == &Self::fee_collection_account()
                || account == &pallet_reward_reserve::Pallet::<T>::reward_pot_account()
        }

        fn checked_add(a: u128, b: u128) -> Result<u128, DispatchError> {
            a.checked_add(b)
                .ok_or_else(|| Error::<T>::ArithmeticOverflow.into())
        }

        fn mul_div(a: u128, b: u128, denominator: u128) -> Result<u128, DispatchError> {
            ensure!(denominator > 0, Error::<T>::ArithmeticOverflow);
            Self::u256_to_u128(
                U256::from(a)
                    .checked_mul(U256::from(b))
                    .ok_or(Error::<T>::ArithmeticOverflow)?
                    / U256::from(denominator),
            )
        }

        fn u256_to_u128(value: U256) -> Result<u128, DispatchError> {
            ensure!(
                value <= U256::from(u128::MAX),
                Error::<T>::ArithmeticOverflow
            );
            Ok(value.low_u128())
        }
    }
}

#[cfg(test)]
mod math_tests {
    use sp_core::U256;

    #[test]
    fn approved_remainders_conserve_every_base_unit() {
        for amount in 0u128..=101 {
            let fee_treasury = amount / 10;
            assert_eq!(amount - fee_treasury + fee_treasury, amount);
            let fallback_treasury = amount / 2;
            assert_eq!(amount - fallback_treasury + fallback_treasury, amount);
        }
    }

    #[test]
    fn ten_one_unit_gross_splits_reconcile_nine_to_one() {
        let mut carry = 0u128;
        let mut staking = 0u128;
        let mut treasury = 0u128;
        for _ in 0..10 {
            let n = U256::from(9u8) + U256::from(carry);
            let s = (n / U256::from(10u8)).low_u128();
            carry = (n % U256::from(10u8)).low_u128();
            staking += s;
            treasury += 1 - s;
        }
        assert_eq!((staking, treasury, carry), (9, 1, 0));
    }
}
