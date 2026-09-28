//! Explicit new-chain initialization. Historical migrations retain their original preconditions.
use crate::*;
use alloc::vec::Vec;
use frame_support::traits::{Get, StorageVersion, VestingSchedule};
pub use pallet::*;

pub struct CustodySigners;
impl Get<[AccountId; 3]> for CustodySigners {
    fn get() -> [AccountId; 3] {
        Signers::<Runtime>::get().unwrap_or_else(FounderCustodySigners::get)
    }
}
pub struct LegacyLiability;
impl Get<Balance> for LegacyLiability {
    fn get() -> Balance {
        if Enabled::<Runtime>::get() {
            0
        } else {
            SecurityBudgetExistingEarnedRewardLiability::get()
        }
    }
}

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_support::pallet_prelude::*;
    #[pallet::config]
    pub trait Config: frame_system::Config<AccountId = crate::AccountId> {}
    const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);
    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);
    #[pallet::storage]
    pub type Enabled<T> = StorageValue<_, bool, ValueQuery>;
    #[pallet::storage]
    pub type Signers<T: Config> = StorageValue<_, [T::AccountId; 3], OptionQuery>;
    #[pallet::genesis_config]
    #[derive(frame_support::DefaultNoBound)]
    pub struct GenesisConfig<T: Config> {
        pub enabled: bool,
        pub signers: Vec<T::AccountId>,
        pub vesting_beneficiaries: Vec<T::AccountId>,
    }
    #[pallet::genesis_build]
    impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
        fn build(&self) {
            if !self.enabled {
                assert!(self.signers.is_empty() && self.vesting_beneficiaries.is_empty());
                return;
            }
            assert_eq!(
                Balances::total_issuance(),
                upgrade13_policy::TARGET_RETAINED_ISSUANCE
            );
            let signers: [AccountId; 3] = self
                .signers
                .clone()
                .try_into()
                .expect("three founder signers");
            assert!(signers[0] < signers[1] && signers[1] < signers[2]);
            assert_eq!(self.vesting_beneficiaries.len(), 5);
            for (i, who) in self.vesting_beneficiaries.iter().enumerate() {
                assert!(!self.vesting_beneficiaries[..i].contains(who));
                let amount = upgrade13_policy::FOUNDING_ALLOCATION_TARGETS[i];
                assert_eq!(Balances::free_balance(who), amount);
                assert!(!pallet_vesting::Vesting::<Runtime>::contains_key(who));
                let terms = era_v14_custody_governance::exact_vesting_schedules(amount, 1)
                    .expect("launch-relative vesting arithmetic");
                for schedule in [terms.schedule_a, terms.schedule_b] {
                    if schedule.locked > 0 {
                        Vesting::add_vesting_schedule(
                            who,
                            schedule.locked,
                            schedule.per_block,
                            schedule.starting_block,
                        )
                        .expect("valid exact founding schedule");
                    }
                }
            }
            let validators = Session::validators();
            assert_eq!(validators.len(), 4);
            assert_eq!(pallet_staking::ValidatorCount::<Runtime>::get(), 4);
            assert_eq!(pallet_staking::MinimumValidatorCount::<Runtime>::get(), 4);
            assert_eq!(
                pallet_staking::MinValidatorBond::<Runtime>::get(),
                SecurityBudgetMinimumValidatorBond::get()
            );
            for (i, who) in validators.iter().enumerate() {
                assert!(!validators[..i].contains(who));
                assert!(
                    pallet_staking::Ledger::<Runtime>::get(who)
                        .expect("genesis staking ledger")
                        .active
                        >= SecurityBudgetMinimumValidatorBond::get()
                );
                assert!(
                    pallet_staking::ErasStakersOverview::<Runtime>::get(0, who)
                        .expect("genesis elected exposure")
                        .own
                        >= SecurityBudgetMinimumValidatorBond::get()
                );
            }
            assert_eq!(Babe::authorities().len(), 4);
            assert_eq!(Grandpa::grandpa_authorities().len(), 4);
            assert!(Babe::epoch_config().is_some());
            assert!(SecurityBudget::accounts_are_distinct());
            assert_eq!(
                Balances::free_balance(SecurityBudget::staking_pot_account()),
                20_000_000 * DECIMALS
            );
            pallet_security_budget::UnallocatedPrincipal::<Runtime>::put(20_000_000 * DECIMALS);
            assert_eq!(SecurityBudget::principal_reserved_total(), 0);
            Enabled::<Runtime>::put(true);
            Signers::<Runtime>::put(signers);
            issuance_cap::RemainingAllowance::<Runtime>::put(
                upgrade13_policy::MAXIMUM_POST_CORRECTION_NEW_ISSUANCE,
            );
            // No fabricated V13/V14 migration-completion markers or legacy reward obligations.
            pallet_reward_reserve::LegacyClaimOnly::<Runtime>::put(true);
            pallet_reward_reserve::V14LegacyCutoffEra::<Runtime>::put(0);
            pallet_security_budget::Active::<Runtime>::put(true);
            pallet_security_budget::ActivatedAt::<Runtime>::put(0);
            pallet_security_budget::FirstEligibleEra::<Runtime>::put(0);
            pallet_security_budget::LastObservedEra::<Runtime>::put(0);
            pallet_security_budget::NextExpiryEra::<Runtime>::put(0);
            pallet_security_budget::AllocationPaused::<Runtime>::put(false);
            crate::ws3_vesting::restore_locks().expect("strict genesis vesting locks");
        }
    }
}

/// Guard old migration hooks on a chain explicitly initialized at genesis. Checks stay valid
/// after ordinary transfers, burns, rewards and vesting; they do not demand a historical snapshot.
pub fn validate_lifecycle() {
    assert_eq!(SecurityBudget::unallocated_principal().checked_add(SecurityBudget::principal_reserved_total()), Some(20_000_000 * DECIMALS));

    assert!(Enabled::<Runtime>::get());
    assert_eq!(
        StorageVersion::get::<FreshGenesis>(),
        StorageVersion::new(1)
    );
    assert_eq!(StorageVersion::get::<IssuanceCap>(), StorageVersion::new(1));
    assert_eq!(
        StorageVersion::get::<RewardReserve>(),
        StorageVersion::new(3)
    );
    assert_eq!(
        StorageVersion::get::<SecurityBudget>(),
        StorageVersion::new(1)
    );
    assert!(issuance_cap::V13MigrationCompleted::<Runtime>::get().is_none());
    assert!(pallet_security_budget::MigrationCompletedAt::<Runtime>::get().is_none());
    assert_eq!(SecurityBudget::legacy_reward_liability(), 0);
    assert_eq!(RewardReserve::committed_liabilities(), 0);
    let remaining = IssuanceCap::remaining_allowance().expect("fresh genesis allowance");
    assert!(remaining <= upgrade13_policy::MAXIMUM_POST_CORRECTION_NEW_ISSUANCE);
    assert!(
        Balances::total_issuance()
            .checked_add(remaining)
            .expect("cap arithmetic")
            <= upgrade13_policy::ABSOLUTE_LIFETIME_CAP
    );
}
