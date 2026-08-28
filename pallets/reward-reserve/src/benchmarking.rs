//! Benchmarks for the transfer-only reward reserve pallet.

use super::*;
use alloc::{collections::btree_map::BTreeMap, vec::Vec};
use frame_benchmarking::v2::*;
use frame_support::traits::fungible::{Inspect, Mutate};
use frame_system::RawOrigin;
use pallet_staking::{EraRewardPoints, ValidatorPrefs};
use sp_runtime::{
    traits::{SaturatedConversion, Saturating, Zero},
    Perbill,
};
use sp_staking::{ExposurePage, IndividualExposure, PagedExposureMetadata};

fn balance<T: Config>(units: u32) -> BalanceOf<T> {
    T::Currency::minimum_balance().saturating_mul(units.saturated_into())
}

#[benchmarks]
mod benchmarks {
    use super::*;

    #[benchmark]
    fn claim_reward_page(n: Linear<1, 64>) {
        let caller: T::AccountId = whitelisted_caller();
        let validator: T::AccountId = account("validator", 0, 0);
        let era: sp_staking::EraIndex = 1;
        let own = balance::<T>(100);
        let nominator_value = balance::<T>(10);
        let mut others: Vec<IndividualExposure<T::AccountId, BalanceOf<T>>> = Vec::new();
        for i in 0..n {
            others.push(IndividualExposure {
                who: account("nominator", i, 0),
                value: nominator_value,
            });
        }
        let page_total = nominator_value.saturating_mul(n.saturated_into());
        let total = own.saturating_add(page_total);
        let budget = balance::<T>(100_000);
        let mut individual = BTreeMap::new();
        individual.insert(validator.clone(), 100);
        pallet_staking::ErasRewardPoints::<T>::insert(
            era,
            EraRewardPoints {
                total: 100,
                individual,
            },
        );
        pallet_staking::ErasValidatorPrefs::<T>::insert(
            era,
            &validator,
            ValidatorPrefs {
                commission: Perbill::from_percent(10),
                blocked: false,
            },
        );
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
        EraRewardBudgets::<T>::insert(era, budget);
        EraRewardLiabilities::<T>::insert(era, budget);
        CommittedLiabilities::<T>::put(budget);
        RewardSystemActive::<T>::put(true);
        FirstEligibleEra::<T>::put(era);
        let pot = Pallet::<T>::reward_pot_account();
        T::Currency::mint_into(&pot, budget.saturating_add(Pallet::<T>::reward_pot_floor()))
            .expect("benchmark pot funding");
        LastPotBalance::<T>::put(Pallet::<T>::pot_balance());

        #[extrinsic_call]
        claim_reward_page(RawOrigin::Signed(caller), era, validator.clone(), 0);

        assert!(ClaimedRewardPages::<T>::contains_key(era, (validator, 0)));
    }

    #[benchmark]
    fn set_fee_routing_configuration() {
        let config = Pallet::<T>::initial_fee_routing_configuration();
        #[extrinsic_call]
        set_fee_routing_configuration(RawOrigin::Root, config);
        assert_eq!(Pallet::<T>::fee_routing_configuration(), config);
    }

    #[benchmark]
    fn spend_ecosystem_treasury() {
        let beneficiary: T::AccountId = whitelisted_caller();
        let treasury = Pallet::<T>::ecosystem_treasury_account();
        let amount = balance::<T>(10);
        T::Currency::mint_into(
            &treasury,
            amount.saturating_add(Pallet::<T>::treasury_pot_floor()),
        )
        .expect("benchmark treasury funding");
        #[extrinsic_call]
        spend_ecosystem_treasury(RawOrigin::Root, beneficiary.clone(), amount);
        assert!(T::Currency::balance(&beneficiary) >= amount);
    }

    #[benchmark]
    fn activate_rewards_and_fee_routing() {
        let reward = Pallet::<T>::reward_pot_account();
        let treasury = Pallet::<T>::ecosystem_treasury_account();
        let collection = Pallet::<T>::fee_collection_account();
        T::Currency::mint_into(
            &reward,
            T::InitialRewardReserve::get().saturating_add(Pallet::<T>::reward_pot_floor()),
        )
        .expect("benchmark reward activation balance");
        T::Currency::mint_into(&treasury, Pallet::<T>::treasury_pot_floor())
            .expect("benchmark treasury activation balance");
        T::Currency::mint_into(&collection, Pallet::<T>::fee_collection_floor())
            .expect("benchmark collection activation balance");
        pallet_staking::ActiveEra::<T>::put(pallet_staking::ActiveEraInfo {
            index: 10,
            start: Some(1_000_000),
        });

        #[extrinsic_call]
        activate_rewards_and_fee_routing(RawOrigin::Root);

        assert!(RewardSystemActive::<T>::get());
        assert!(FeeRoutingActive::<T>::get());
    }

    #[benchmark]
    fn retry_deferred_fee_routing() {
        let caller: T::AccountId = whitelisted_caller();
        let reward = Pallet::<T>::reward_pot_account();
        let treasury = Pallet::<T>::ecosystem_treasury_account();
        let collection = Pallet::<T>::fee_collection_account();
        let fee = balance::<T>(100);
        let (reward_share, treasury_share) =
            Pallet::<T>::fee_split(fee, 70).expect("benchmark split");
        T::Currency::mint_into(&reward, Pallet::<T>::reward_pot_floor())
            .expect("benchmark reward floor");
        T::Currency::mint_into(&treasury, Pallet::<T>::treasury_pot_floor())
            .expect("benchmark treasury floor");
        T::Currency::mint_into(
            &collection,
            Pallet::<T>::fee_collection_floor().saturating_add(fee),
        )
        .expect("benchmark staged fee");
        RewardSystemActive::<T>::put(true);
        FeeRoutingActive::<T>::put(true);
        PendingRewardFee::<T>::put(reward_share);
        PendingTreasuryFee::<T>::put(treasury_share);

        #[extrinsic_call]
        retry_deferred_fee_routing(RawOrigin::Signed(caller));

        assert!(Pallet::<T>::pending_normal_fee().is_zero());
    }

    #[benchmark]
    fn retry_deferred_author_tip() {
        let caller: T::AccountId = whitelisted_caller();
        let author: T::AccountId = account("author", 0, 0);
        let collection = Pallet::<T>::fee_collection_account();
        let tip = balance::<T>(10);
        T::Currency::mint_into(&author, T::Currency::minimum_balance())
            .expect("benchmark author account");
        T::Currency::mint_into(
            &collection,
            Pallet::<T>::fee_collection_floor().saturating_add(tip),
        )
        .expect("benchmark staged tip");
        RewardSystemActive::<T>::put(true);
        FeeRoutingActive::<T>::put(true);
        PendingAuthorTips::<T>::insert(&author, tip);
        PendingTipTotal::<T>::put(tip);

        #[extrinsic_call]
        retry_deferred_author_tip(RawOrigin::Signed(caller), author.clone());

        assert!(PendingAuthorTips::<T>::get(author).is_zero());
    }

    #[benchmark]
    fn cancel_legacy_zero_point_liability() {
        let era: sp_staking::EraIndex = 1_782;
        let liability = balance::<T>(1_000);
        let period_start = 1_000_000u64;
        let pot = Pallet::<T>::reward_pot_account();
        T::Currency::mint_into(
            &pot,
            liability.saturating_add(Pallet::<T>::reward_pot_floor()),
        )
        .expect("benchmark funded liability");
        LegacyZeroPointEraRange::<T>::put((era, era));
        LegacyLiabilityBudgetPeriodStartMillis::<T>::put(period_start);
        BudgetPeriodStartMillis::<T>::put(period_start);
        EraRewardBudgets::<T>::insert(era, liability);
        EraRewardLiabilities::<T>::insert(era, liability);
        AnnualBudgetUsed::<T>::put(liability);
        CommittedLiabilities::<T>::put(liability);

        #[extrinsic_call]
        cancel_legacy_zero_point_liability(RawOrigin::Root, era);

        assert_eq!(
            CancelledZeroPointLiabilities::<T>::get(era),
            Some(liability)
        );
        assert!(EraRewardLiabilities::<T>::get(era).is_zero());
    }

    #[benchmark]
    fn sync_reward_pot() {
        let caller: T::AccountId = whitelisted_caller();
        let pot = Pallet::<T>::reward_pot_account();
        LastPotBalance::<T>::put(BalanceOf::<T>::zero());
        T::Currency::mint_into(&pot, balance::<T>(10)).expect("benchmark pot funding");

        #[extrinsic_call]
        sync_reward_pot(RawOrigin::Signed(caller));

        assert!(ReserveFundingTotal::<T>::get() > BalanceOf::<T>::zero());
    }
}
