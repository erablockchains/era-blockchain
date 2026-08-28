//! Benchmarks for the AI predictions pallet.

use super::*;
use crate::Pallet as AiPredictions;
use alloc::vec;
use frame_benchmarking::v2::*;
use frame_support::traits::Currency;
use frame_system::pallet_prelude::BlockNumberFor;
use frame_system::RawOrigin;
use sp_runtime::traits::{SaturatedConversion, Saturating};

fn balance<T: Config>(value: u32) -> BalanceOf<T> {
    T::Currency::minimum_balance().saturating_mul(value.saturated_into())
}
fn block<T: Config>(value: u32) -> BlockNumberFor<T> {
    value.saturated_into()
}
fn fund<T: Config>(who: &T::AccountId) {
    T::Currency::make_free_balance_be(who, balance::<T>(1_000_000));
}
fn onboarding<T: Config>(approval: bool) {
    ModelOnboardingConfig::<T>::put(ModelOnboardingSettings {
        public_model_registration_enabled: true,
        require_model_approval: approval,
        ..Default::default()
    });
}
fn model<T: Config>(owner: &T::AccountId, status: ModelStatus) -> ModelId {
    let id = NextModelId::<T>::get();
    Models::<T>::insert(
        id,
        AiModel {
            owner: owner.clone(),
            model_hash: vec![1u8; 32].try_into().ok().unwrap(),
            metadata_uri: b"ipfs://benchmark-model".to_vec().try_into().ok().unwrap(),
            registered_at: block::<T>(1),
            updated_at: None,
            active: true,
        },
    );
    ModelGovernanceById::<T>::insert(
        id,
        ModelGovernance {
            status,
            authorized_submitter: None,
            updated_at: block::<T>(1),
        },
    );
    ModelStatsById::<T>::insert(id, ModelStats::default());
    NextModelId::<T>::put(id + 1);
    id
}
fn prediction<T: Config>(owner: &T::AccountId) -> (PredictionId, PredictionTokenId) {
    let model_id = model::<T>(owner, ModelStatus::Approved);
    let prediction_id = NextPredictionId::<T>::get();
    Predictions::<T>::insert(
        prediction_id,
        Prediction {
            model_id,
            submitter: owner.clone(),
            domain: PredictionDomain::Finance,
            category_code: b"finance.benchmark".to_vec().try_into().ok().unwrap(),
            prediction_hash: vec![2u8; 32].try_into().ok().unwrap(),
            metadata_uri: b"ipfs://benchmark-prediction"
                .to_vec()
                .try_into()
                .ok()
                .unwrap(),
            confidence: 80,
            created_at: block::<T>(1),
            expires_at: block::<T>(100),
            status: PredictionStatus::Open,
            outcome: None,
            validator: None,
            validated_at: None,
        },
    );
    NextPredictionId::<T>::put(prediction_id + 1);
    let token_id = NextPredictionTokenId::<T>::get();
    PredictionTokens::<T>::insert(
        token_id,
        PredictionToken {
            prediction_id,
            owner: owner.clone(),
            metadata_uri: b"ipfs://benchmark-token".to_vec().try_into().ok().unwrap(),
            created_at: block::<T>(1),
            updated_at: None,
            transferable: true,
            frozen: false,
            burned: false,
        },
    );
    PredictionTokenByPrediction::<T>::insert(prediction_id, token_id);
    NextPredictionTokenId::<T>::put(token_id + 1);
    (prediction_id, token_id)
}
fn market<T: Config>() {
    StakingConfig::<T>::put(StakingSettings {
        staking_enabled: true,
        min_stake: balance::<T>(1),
    });
    PredictionMarketEconomicsConfig::<T>::put(PredictionMarketEconomicsSettings {
        market_enabled: true,
        allow_unstake_before_settlement: true,
        fees_enabled: false,
        fee_bps: 0,
        treasury_enabled: true,
    });
}

#[benchmarks]
mod benchmarks {
    use super::*;

    #[benchmark]
    fn register_model() {
        onboarding::<T>(true);
        let caller: T::AccountId = whitelisted_caller();
        #[extrinsic_call]
        register_model(
            RawOrigin::Signed(caller.clone()),
            vec![1u8; 32],
            b"ipfs://benchmark-model".to_vec(),
        );
        assert_eq!(
            ModelGovernanceById::<T>::get(0).unwrap().status,
            ModelStatus::Pending
        );
    }

    #[benchmark]
    fn approve_model() {
        let owner: T::AccountId = account("owner", 0, 0);
        let id = model::<T>(&owner, ModelStatus::Pending);
        #[extrinsic_call]
        approve_model(RawOrigin::Root, id);
        assert_eq!(
            ModelGovernanceById::<T>::get(id).unwrap().status,
            ModelStatus::Approved
        );
    }

    #[benchmark]
    fn suspend_model() {
        let owner: T::AccountId = account("owner", 0, 0);
        let id = model::<T>(&owner, ModelStatus::Approved);
        #[extrinsic_call]
        suspend_model(RawOrigin::Root, id);
        assert_eq!(
            ModelGovernanceById::<T>::get(id).unwrap().status,
            ModelStatus::Suspended
        );
    }

    #[benchmark]
    fn reject_model() {
        let owner: T::AccountId = account("owner", 0, 0);
        let id = model::<T>(&owner, ModelStatus::Pending);
        #[extrinsic_call]
        reject_model(RawOrigin::Root, id);
        assert_eq!(
            ModelGovernanceById::<T>::get(id).unwrap().status,
            ModelStatus::Rejected
        );
    }

    #[benchmark]
    fn update_model() {
        let owner: T::AccountId = whitelisted_caller();
        let id = model::<T>(&owner, ModelStatus::Approved);
        #[extrinsic_call]
        update_model(
            RawOrigin::Signed(owner),
            id,
            vec![3u8; 32],
            b"ipfs://benchmark-model-updated".to_vec(),
        );
        assert!(Models::<T>::get(id).unwrap().updated_at.is_some());
    }

    #[benchmark]
    fn set_model_submitter() {
        let owner: T::AccountId = whitelisted_caller();
        let submitter: T::AccountId = account("submitter", 0, 0);
        let id = model::<T>(&owner, ModelStatus::Approved);
        #[extrinsic_call]
        set_model_submitter(RawOrigin::Signed(owner), id, Some(submitter.clone()));
        assert_eq!(
            ModelGovernanceById::<T>::get(id)
                .unwrap()
                .authorized_submitter,
            Some(submitter)
        );
    }

    #[benchmark]
    fn submit_prediction() {
        onboarding::<T>(true);
        let owner: T::AccountId = whitelisted_caller();
        let id = model::<T>(&owner, ModelStatus::Approved);
        #[extrinsic_call]
        submit_prediction(
            RawOrigin::Signed(owner),
            id,
            PredictionDomain::Finance,
            b"finance.benchmark".to_vec(),
            vec![2u8; 32],
            b"ipfs://benchmark-prediction".to_vec(),
            80,
            block::<T>(100),
        );
        assert!(Predictions::<T>::contains_key(0));
    }

    #[benchmark]
    fn validate_prediction() {
        let validator: T::AccountId = whitelisted_caller();
        AuthorizedValidators::<T>::insert(&validator, true);
        let owner: T::AccountId = account("owner", 0, 0);
        let (prediction_id, _) = prediction::<T>(&owner);
        ActivePredictionCountByModel::<T>::insert(0, 1);
        #[extrinsic_call]
        validate_prediction(
            RawOrigin::Signed(validator),
            prediction_id,
            PredictionOutcome::Successful,
        );
        assert_eq!(
            Predictions::<T>::get(prediction_id).unwrap().status,
            PredictionStatus::Validated
        );
    }

    #[benchmark]
    fn close_prediction() {
        let owner: T::AccountId = whitelisted_caller();
        let (prediction_id, _) = prediction::<T>(&owner);
        ActivePredictionCountByModel::<T>::insert(0, 1);
        #[extrinsic_call]
        close_prediction(RawOrigin::Signed(owner), prediction_id);
        assert_eq!(
            Predictions::<T>::get(prediction_id).unwrap().status,
            PredictionStatus::Closed
        );
    }

    #[benchmark]
    fn stake_on_prediction_outcome_side() {
        let caller: T::AccountId = whitelisted_caller();
        fund::<T>(&caller);
        market::<T>();
        let (_, token_id) = prediction::<T>(&caller);
        #[extrinsic_call]
        stake_on_prediction_outcome_side(
            RawOrigin::Signed(caller.clone()),
            token_id,
            PredictionOutcomeSide::Yes,
            balance::<T>(100),
        );
        assert!(PredictionTokenSideStakes::<T>::contains_key(
            token_id, caller
        ));
    }

    #[benchmark]
    fn unstake_from_prediction_outcome_side() {
        let caller: T::AccountId = whitelisted_caller();
        market::<T>();
        let (_, token_id) = prediction::<T>(&caller);
        let amount = balance::<T>(100);
        PredictionTokenSideStakes::<T>::insert(
            token_id,
            &caller,
            PredictionSideStake {
                side: PredictionOutcomeSide::Yes,
                amount,
                staked_at: block::<T>(1),
                updated_at: None,
            },
        );
        PredictionTokenSideTotals::<T>::insert(token_id, PredictionOutcomeSide::Yes, amount);
        T::Currency::make_free_balance_be(&AiPredictions::<T>::market_account_id(), amount);
        #[extrinsic_call]
        unstake_from_prediction_outcome_side(
            RawOrigin::Signed(caller.clone()),
            token_id,
            PredictionOutcomeSide::Yes,
            amount,
        );
        assert!(!PredictionTokenSideStakes::<T>::contains_key(
            token_id, caller
        ));
    }

    #[benchmark]
    fn claim_prediction_market_payout() {
        let caller: T::AccountId = whitelisted_caller();
        market::<T>();
        let (_, token_id) = prediction::<T>(&caller);
        let amount = balance::<T>(100);
        PredictionTokenSideStakes::<T>::insert(
            token_id,
            &caller,
            PredictionSideStake {
                side: PredictionOutcomeSide::Yes,
                amount,
                staked_at: block::<T>(1),
                updated_at: None,
            },
        );
        PredictionTokenSideTotals::<T>::insert(token_id, PredictionOutcomeSide::Yes, amount);
        PredictionTokenSettlements::<T>::insert(
            token_id,
            PredictionTokenSettlement {
                token_id,
                status: SettlementStatus::Finalized,
                proposed_outcome: Some(SettlementOutcome::Correct),
                final_outcome: Some(SettlementOutcome::Correct),
                proposed_by: None,
                finalized_by: None,
                proposed_at: Some(block::<T>(1)),
                dispute_until: Some(block::<T>(2)),
                finalized_at: Some(block::<T>(3)),
                evidence_uri: None,
                total_stake_at_finalization: amount,
                dispute_count: 0,
            },
        );
        T::Currency::make_free_balance_be(&AiPredictions::<T>::market_account_id(), amount);
        #[extrinsic_call]
        claim_prediction_market_payout(RawOrigin::Signed(caller.clone()), token_id);
        assert!(PredictionTokenSidePayoutClaims::<T>::contains_key(
            token_id, caller
        ));
    }

    impl_benchmark_test_suite!(
        AiPredictions,
        crate::mock::new_test_ext(),
        crate::mock::Test
    );
}
