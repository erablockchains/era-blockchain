//! Benchmarks for V14 capped issuance, fee routing, and transfer-funded rewards.

use super::*;
use alloc::{collections::btree_map::BTreeMap, vec::Vec};
use frame_benchmarking::v2::*;
use frame_support::traits::{
    fungible::{Inspect, Mutate},
    Get, Hooks, StorageVersion,
};
use frame_system::pallet_prelude::BlockNumberFor;
use frame_system::RawOrigin;
use pallet_staking::{EraRewardPoints, ValidatorPrefs};
use sp_runtime::Perbill;
use sp_staking::{ExposurePage, IndividualExposure, PagedExposureMetadata};

fn minimum<T: Config>() -> u128 {
    <T as pallet_staking::Config>::Currency::minimum_balance()
}

fn fund<T: Config>(account: &T::AccountId, amount: u128) {
    <T as pallet_staking::Config>::Currency::mint_into(account, amount)
        .expect("benchmark account funding");
}

fn maximum_routing_amount<T: Config>() -> u128 {
    let minimum = minimum::<T>();
    // The configured 10M annual issuance ceiling times 100 is the 1B ETKN lifetime cap.
    // Leave room for the three benchmark fixture accounts so synthetic total issuance stays
    // within that same cap. The amount changes no routing branch or storage cardinality.
    T::GrossAnnualIssuanceCeiling::get()
        .saturating_mul(100)
        .saturating_sub(minimum.saturating_mul(3))
}

fn prepare_routing_accounts<T: Config>(collection_amount: u128) {
    let minimum = minimum::<T>();
    fund::<T>(&Pallet::<T>::staking_pot_account(), minimum);
    fund::<T>(&Pallet::<T>::treasury_account(), minimum);
    fund::<T>(
        &Pallet::<T>::fee_collection_account(),
        minimum.saturating_add(collection_amount),
    );
}

#[benchmarks]
mod benchmarks {
    use super::*;

    #[benchmark]
    fn activate() {
        let era = 10;
        StorageVersion::new(1).put::<Pallet<T>>();
        Active::<T>::kill();
        LegacyRewardLiability::<T>::put(T::ExistingEarnedRewardLiability::get());
        pallet_reward_reserve::LegacyClaimOnly::<T>::kill();
        pallet_reward_reserve::PendingRewardFee::<T>::kill();
        pallet_reward_reserve::PendingTreasuryFee::<T>::kill();
        pallet_reward_reserve::PendingTipTotal::<T>::kill();
        pallet_staking::ValidatorCount::<T>::put(T::ActivationValidatorCount::get());
        pallet_staking::ActiveEra::<T>::put(pallet_staking::ActiveEraInfo {
            index: era,
            start: Some(1_000_000),
        });
        for i in 0..T::ActivationValidatorCount::get() {
            let validator: T::AccountId = account("validator", i, 0);
            pallet_staking::ErasStakersOverview::<T>::insert(
                era,
                &validator,
                PagedExposureMetadata {
                    total: T::MinimumValidatorBond::get(),
                    own: T::MinimumValidatorBond::get(),
                    nominator_count: 0,
                    page_count: 0,
                },
            );
            pallet_staking::ErasValidatorPrefs::<T>::insert(
                era,
                validator,
                ValidatorPrefs {
                    commission: T::MaximumValidatorCommission::get(),
                    blocked: false,
                },
            );
        }

        #[extrinsic_call]
        activate(RawOrigin::Root);

        assert!(Active::<T>::get());
        assert_eq!(FirstEligibleEra::<T>::get(), Some(era + 1));
    }

    #[benchmark]
    fn claim_reward_page(n: Linear<1, 64>) {
        let caller: T::AccountId = whitelisted_caller();
        let validator: T::AccountId = account("validator", 0, 0);
        let era = 1;
        let own = T::MinimumValidatorBond::get();
        let nominator_value = T::MinimumValidatorBond::get() / 1_000;
        let mut others: Vec<IndividualExposure<T::AccountId, u128>> = Vec::new();
        for i in 0..n {
            others.push(IndividualExposure {
                who: account("nominator", i, 0),
                value: nominator_value,
            });
        }
        let page_total = nominator_value.saturating_mul(n.into());
        let total = own.saturating_add(page_total);
        let budget = minimum::<T>().saturating_mul(100_000);
        pallet_staking::ActiveEra::<T>::put(pallet_staking::ActiveEraInfo {
            index: era + 1,
            start: Some(2_000_000),
        });
        pallet_staking::ErasStakersOverview::<T>::insert(
            era,
            &validator,
            PagedExposureMetadata {
                total,
                own,
                nominator_count: n,
                page_count: 1,
            },
        );
        pallet_staking::ErasStakersPaged::<T>::insert(
            (era, &validator, 0),
            ExposurePage { page_total, others },
        );
        pallet_staking::ErasValidatorPrefs::<T>::insert(
            era,
            &validator,
            ValidatorPrefs {
                commission: T::MaximumValidatorCommission::get(),
                blocked: false,
            },
        );
        Active::<T>::put(true);
        FirstEligibleEra::<T>::put(era);
        EraBudgets::<T>::insert(
            era,
            EraBudget {
                gross_issuance: 0,
                staking_issuance: 0,
                treasury_issuance: 0,
                fee_staking: budget,
                failed_author_tips: 0,
                retained_carry: 0,
                total: budget,
                eligible_stake: total,
                eligible_points: 100,
                duration_millis: 1,
            },
        );
        EligibleValidatorPoints::<T>::insert(era, &validator, 100);
        EraLiability::<T>::insert(era, budget);
        CommittedLiabilities::<T>::put(budget);
        StakingFundingTotal::<T>::put(budget);
        fund::<T>(
            &Pallet::<T>::staking_pot_account(),
            budget.saturating_add(minimum::<T>()),
        );

        #[extrinsic_call]
        claim_reward_page(RawOrigin::Signed(caller), era, validator.clone(), 0);

        assert!(ClaimedPages::<T>::contains_key(era, (validator, 0)));
    }

    #[benchmark]
    fn retry_normal_fee() {
        let caller: T::AccountId = whitelisted_caller();
        let fee = minimum::<T>().saturating_mul(100);
        let (staking, treasury) = Pallet::<T>::fee_split(fee);
        prepare_routing_accounts::<T>(fee);
        PendingStakingFee::<T>::put(staking);
        PendingTreasuryFee::<T>::put(treasury);

        #[extrinsic_call]
        retry_normal_fee(RawOrigin::Signed(caller));

        assert_eq!(Pallet::<T>::pending_collection_obligations(), Some(0));
    }

    #[benchmark]
    fn retry_tip() {
        let caller: T::AccountId = whitelisted_caller();
        let tip = minimum::<T>().saturating_mul(100);
        prepare_routing_accounts::<T>(tip);
        PendingMissingAuthorTips::<T>::put(tip);
        PendingTipTotal::<T>::put(tip);

        #[extrinsic_call]
        retry_tip(RawOrigin::Signed(caller), None);

        assert_eq!(PendingTipTotal::<T>::get(), 0);
    }

    #[benchmark]
    fn route_normal_fee(z: Linear<0, 100>) {
        let fee = maximum_routing_amount::<T>().saturating_mul(z.into()) / 100;
        prepare_routing_accounts::<T>(fee);
        Active::<T>::put(true);

        #[block]
        {
            Pallet::<T>::stage_normal_fee(fee).expect("benchmark fee staging");
            Pallet::<T>::settle_pending_normal_fee().expect("benchmark fee settlement");
        }

        assert_eq!(NormalFeeRoutedTotal::<T>::get(), fee);
    }

    #[benchmark]
    fn route_tip_author(z: Linear<0, 100>) {
        let tip = maximum_routing_amount::<T>().saturating_mul(z.into()) / 100;
        let author: T::AccountId = account("tip-author", 0, 0);
        prepare_routing_accounts::<T>(tip);
        Active::<T>::put(true);

        #[block]
        {
            Pallet::<T>::stage_tip(Some(author.clone()), tip).expect("benchmark tip staging");
            Pallet::<T>::settle_pending_tip(Some(author.clone()))
                .expect("benchmark receivable tip settlement");
        }

        assert_eq!(
            AuthorTipPaidTotal::<T>::get(),
            Pallet::<T>::receivable_tip_split(tip).0
        );
        assert_eq!(PendingTipTotal::<T>::get(), 0);
    }

    #[benchmark]
    fn route_tip_fallback(z: Linear<0, 100>) {
        let tip = maximum_routing_amount::<T>().saturating_mul(z.into()) / 100;
        prepare_routing_accounts::<T>(tip);
        Active::<T>::put(true);

        #[block]
        {
            Pallet::<T>::stage_tip(None, tip).expect("benchmark tip staging");
            Pallet::<T>::settle_pending_tip(None).expect("benchmark fallback tip settlement");
        }

        assert_eq!(FailedAuthorTipTotal::<T>::get(), tip);
        assert_eq!(PendingTipTotal::<T>::get(), 0);
    }

    #[benchmark]
    fn on_initialize(v: Linear<4, 16>) {
        let completed_era = 1;
        let active_era = completed_era + 1;
        let start = 1_000_000u64;
        let duration = T::SecurityMillisecondsPerYear::get();
        let mut individual = BTreeMap::new();
        for i in 0..v {
            let validator: T::AccountId = account("validator", i, 0);
            individual.insert(validator.clone(), 10);
            pallet_staking::ErasStakersOverview::<T>::insert(
                completed_era,
                &validator,
                PagedExposureMetadata {
                    total: T::MinimumValidatorBond::get(),
                    own: T::MinimumValidatorBond::get(),
                    nominator_count: 0,
                    page_count: 0,
                },
            );
            pallet_staking::ErasValidatorPrefs::<T>::insert(
                completed_era,
                validator,
                ValidatorPrefs {
                    commission: Perbill::from_percent(0),
                    blocked: false,
                },
            );
        }
        pallet_staking::ErasRewardPoints::<T>::insert(
            completed_era,
            EraRewardPoints {
                total: v.saturating_mul(10),
                individual,
            },
        );
        pallet_staking::ActiveEra::<T>::put(pallet_staking::ActiveEraInfo {
            index: active_era,
            start: Some(start.saturating_add(duration)),
        });
        Active::<T>::put(true);
        AllocationPaused::<T>::kill();
        FirstEligibleEra::<T>::put(completed_era);
        LastObservedEra::<T>::put(completed_era);
        LastObservedEraStart::<T>::put(start);
        NextExpiryEra::<T>::put(completed_era);
        T::Issuance::benchmark_set_remaining_allowance(T::GrossAnnualIssuanceCeiling::get());

        #[block]
        {
            <Pallet<T> as Hooks<BlockNumberFor<T>>>::on_initialize(
                frame_system::Pallet::<T>::block_number(),
            );
        }

        assert!(EraBudgets::<T>::contains_key(completed_era));
    }

    #[benchmark]
    fn migration_initialize() {
        frame_support::storage::unhashed::kill(&StorageVersion::storage_key::<Pallet<T>>());
        MigrationCompletedAt::<T>::kill();
        AllocationPaused::<T>::kill();
        LegacyRewardLiability::<T>::kill();

        #[block]
        {
            StorageVersion::new(1).put::<Pallet<T>>();
            Pallet::<T>::record_migration_completed(
                T::ActivationValidatorCount::get(),
                T::ExistingEarnedRewardLiability::get(),
            )
            .expect("benchmark migration initialization");
        }

        assert!(MigrationCompletedAt::<T>::get().is_some());
        assert!(AllocationPaused::<T>::get());
        assert_eq!(
            LegacyRewardLiability::<T>::get(),
            T::ExistingEarnedRewardLiability::get()
        );
    }
}
