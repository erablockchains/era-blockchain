use super::*;
use crate::mock::{
    new_test_ext, AiPredictions, Balances, RuntimeOrigin, System, ALICE, BOB, VALIDATOR,
};

use frame_support::{assert_noop, assert_ok};

fn model_hash() -> Vec<u8> {
    b"era-ai-model-v1-hash".to_vec()
}

fn updated_model_hash() -> Vec<u8> {
    b"era-ai-model-v2-hash".to_vec()
}

fn metadata_uri() -> Vec<u8> {
    b"ipfs://era-ai-model-v1".to_vec()
}

fn prediction_hash() -> Vec<u8> {
    b"prediction-hash-001".to_vec()
}

fn category_code() -> Vec<u8> {
    b"finance.debt_default_risk".to_vec()
}

fn register_default_model() {
    assert_ok!(AiPredictions::register_model(
        RuntimeOrigin::signed(ALICE),
        model_hash(),
        metadata_uri(),
    ));
}

fn submit_default_prediction() {
    assert_ok!(AiPredictions::submit_prediction(
        RuntimeOrigin::signed(ALICE),
        0,
        PredictionDomain::Finance,
        category_code(),
        prediction_hash(),
        b"ipfs://prediction-001".to_vec(),
        85,
        100,
    ));
}

#[test]
fn register_model_success() {
    new_test_ext().execute_with(|| {
        assert_ok!(AiPredictions::register_model(
            RuntimeOrigin::signed(ALICE),
            model_hash(),
            metadata_uri(),
        ));

        assert_eq!(AiPredictions::next_model_id(), 1);

        let model = AiPredictions::models(0).expect("model should exist");
        assert_eq!(model.owner, ALICE);
        assert!(model.active);

        let stats = AiPredictions::model_stats(0).expect("stats should exist");
        assert_eq!(stats.total_predictions, 0);

        System::assert_last_event(
            Event::ModelRegistered {
                model_id: 0,
                owner: ALICE,
            }
            .into(),
        );
    });
}

#[test]
fn update_model_success_by_owner() {
    new_test_ext().execute_with(|| {
        register_default_model();

        assert_ok!(AiPredictions::update_model(
            RuntimeOrigin::signed(ALICE),
            0,
            updated_model_hash(),
            b"ipfs://era-ai-model-v2".to_vec(),
        ));

        let model = AiPredictions::models(0).expect("model should exist");
        assert_eq!(model.owner, ALICE);
        assert!(model.updated_at.is_some());
    });
}

#[test]
fn reject_model_update_by_non_owner() {
    new_test_ext().execute_with(|| {
        register_default_model();

        assert_noop!(
            AiPredictions::update_model(
                RuntimeOrigin::signed(BOB),
                0,
                updated_model_hash(),
                b"ipfs://era-ai-model-v2".to_vec(),
            ),
            Error::<crate::mock::Test>::NotModelOwner
        );
    });
}

#[test]
fn submit_prediction_success() {
    new_test_ext().execute_with(|| {
        register_default_model();

        assert_ok!(AiPredictions::submit_prediction(
            RuntimeOrigin::signed(ALICE),
            0,
            PredictionDomain::Finance,
            category_code(),
            prediction_hash(),
            b"ipfs://prediction-001".to_vec(),
            85,
            100,
        ));

        assert_eq!(AiPredictions::next_prediction_id(), 1);

        let prediction = AiPredictions::predictions(0).expect("prediction should exist");
        assert_eq!(prediction.model_id, 0);
        assert_eq!(prediction.submitter, ALICE);
        assert_eq!(prediction.domain, PredictionDomain::Finance);
        assert_eq!(prediction.confidence, 85);
        assert_eq!(prediction.status, PredictionStatus::Open);
        assert_eq!(prediction.outcome, None);

        let stats = AiPredictions::model_stats(0).expect("stats should exist");
        assert_eq!(stats.total_predictions, 1);
        assert_eq!(stats.validated_predictions, 0);
    });
}

#[test]
fn reject_confidence_greater_than_100() {
    new_test_ext().execute_with(|| {
        register_default_model();

        assert_noop!(
            AiPredictions::submit_prediction(
                RuntimeOrigin::signed(ALICE),
                0,
                PredictionDomain::Finance,
                category_code(),
                prediction_hash(),
                b"ipfs://prediction-001".to_vec(),
                101,
                100,
            ),
            Error::<crate::mock::Test>::InvalidConfidence
        );
    });
}

#[test]
fn reject_prediction_for_missing_model() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            AiPredictions::submit_prediction(
                RuntimeOrigin::signed(ALICE),
                999,
                PredictionDomain::Finance,
                category_code(),
                prediction_hash(),
                b"ipfs://prediction-001".to_vec(),
                85,
                100,
            ),
            Error::<crate::mock::Test>::ModelNotFound
        );
    });
}

#[test]
fn reject_validation_by_unauthorized_validator() {
    new_test_ext().execute_with(|| {
        register_default_model();
        submit_default_prediction();

        assert_noop!(
            AiPredictions::validate_prediction(
                RuntimeOrigin::signed(BOB),
                0,
                PredictionOutcome::Successful,
            ),
            Error::<crate::mock::Test>::NotAuthorizedValidator
        );
    });
}

#[test]
fn authorize_validator_by_root() {
    new_test_ext().execute_with(|| {
        assert_ok!(AiPredictions::authorize_validator(
            RuntimeOrigin::root(),
            VALIDATOR,
        ));

        assert!(AiPredictions::authorized_validators(VALIDATOR));
    });
}

#[test]
fn validate_prediction_success_and_update_stats() {
    new_test_ext().execute_with(|| {
        register_default_model();
        submit_default_prediction();

        assert_ok!(AiPredictions::authorize_validator(
            RuntimeOrigin::root(),
            VALIDATOR,
        ));

        assert_ok!(AiPredictions::validate_prediction(
            RuntimeOrigin::signed(VALIDATOR),
            0,
            PredictionOutcome::Successful,
        ));

        let prediction = AiPredictions::predictions(0).expect("prediction should exist");
        assert_eq!(prediction.status, PredictionStatus::Validated);
        assert_eq!(prediction.outcome, Some(PredictionOutcome::Successful));
        assert_eq!(prediction.validator, Some(VALIDATOR));
        assert!(prediction.validated_at.is_some());

        let stats = AiPredictions::model_stats(0).expect("stats should exist");
        assert_eq!(stats.total_predictions, 1);
        assert_eq!(stats.validated_predictions, 1);
        assert_eq!(stats.successful_predictions, 1);
        assert_eq!(stats.failed_predictions, 0);
        assert_eq!(stats.inconclusive_predictions, 0);
    });
}

#[test]
fn reject_double_validation() {
    new_test_ext().execute_with(|| {
        register_default_model();
        submit_default_prediction();

        assert_ok!(AiPredictions::authorize_validator(
            RuntimeOrigin::root(),
            VALIDATOR,
        ));

        assert_ok!(AiPredictions::validate_prediction(
            RuntimeOrigin::signed(VALIDATOR),
            0,
            PredictionOutcome::Successful,
        ));

        assert_noop!(
            AiPredictions::validate_prediction(
                RuntimeOrigin::signed(VALIDATOR),
                0,
                PredictionOutcome::Failed,
            ),
            Error::<crate::mock::Test>::PredictionAlreadyValidated
        );
    });
}

#[test]
fn close_prediction_success() {
    new_test_ext().execute_with(|| {
        register_default_model();
        submit_default_prediction();

        assert_ok!(AiPredictions::close_prediction(
            RuntimeOrigin::signed(ALICE),
            0,
        ));

        let prediction = AiPredictions::predictions(0).expect("prediction should exist");
        assert_eq!(prediction.status, PredictionStatus::Closed);
    });
}

#[test]
fn tokenization_disabled_by_default() {
    new_test_ext().execute_with(|| {
        register_default_model();
        submit_default_prediction();

        assert_noop!(
            AiPredictions::tokenize_prediction(
                RuntimeOrigin::signed(ALICE),
                0,
                b"ipfs://prediction-token-001".to_vec(),
                true,
            ),
            Error::<crate::mock::Test>::TokenizationDisabled
        );
    });
}

#[test]
fn set_tokenization_config_by_root() {
    new_test_ext().execute_with(|| {
        assert_ok!(AiPredictions::set_tokenization_config(
            RuntimeOrigin::root(),
            true,
            false,
        ));

        let config = AiPredictions::tokenization_config();
        assert!(config.tokenization_enabled);
        assert!(!config.transfers_enabled);
    });
}

#[test]
fn tokenize_prediction_success_when_enabled() {
    new_test_ext().execute_with(|| {
        register_default_model();
        submit_default_prediction();

        assert_ok!(AiPredictions::set_tokenization_config(
            RuntimeOrigin::root(),
            true,
            false,
        ));

        assert_ok!(AiPredictions::tokenize_prediction(
            RuntimeOrigin::signed(ALICE),
            0,
            b"ipfs://prediction-token-001".to_vec(),
            true,
        ));

        assert_eq!(AiPredictions::next_prediction_token_id(), 1);
        assert_eq!(AiPredictions::prediction_token_by_prediction(0), Some(0));

        let token = AiPredictions::prediction_tokens(0).expect("token should exist");
        assert_eq!(token.prediction_id, 0);
        assert_eq!(token.owner, ALICE);
        assert!(token.transferable);
        assert!(!token.frozen);
        assert!(!token.burned);
    });
}

#[test]
fn reject_double_tokenization_of_same_prediction() {
    new_test_ext().execute_with(|| {
        register_default_model();
        submit_default_prediction();

        assert_ok!(AiPredictions::set_tokenization_config(
            RuntimeOrigin::root(),
            true,
            false,
        ));

        assert_ok!(AiPredictions::tokenize_prediction(
            RuntimeOrigin::signed(ALICE),
            0,
            b"ipfs://prediction-token-001".to_vec(),
            true,
        ));

        assert_noop!(
            AiPredictions::tokenize_prediction(
                RuntimeOrigin::signed(ALICE),
                0,
                b"ipfs://prediction-token-duplicate".to_vec(),
                true,
            ),
            Error::<crate::mock::Test>::PredictionAlreadyTokenized
        );
    });
}

#[test]
fn reject_tokenization_by_non_prediction_owner() {
    new_test_ext().execute_with(|| {
        register_default_model();
        submit_default_prediction();

        assert_ok!(AiPredictions::set_tokenization_config(
            RuntimeOrigin::root(),
            true,
            false,
        ));

        assert_noop!(
            AiPredictions::tokenize_prediction(
                RuntimeOrigin::signed(BOB),
                0,
                b"ipfs://prediction-token-001".to_vec(),
                true,
            ),
            Error::<crate::mock::Test>::NotPredictionOwner
        );
    });
}

#[test]
fn transfer_disabled_by_default_even_after_tokenization() {
    new_test_ext().execute_with(|| {
        register_default_model();
        submit_default_prediction();

        assert_ok!(AiPredictions::set_tokenization_config(
            RuntimeOrigin::root(),
            true,
            false,
        ));

        assert_ok!(AiPredictions::tokenize_prediction(
            RuntimeOrigin::signed(ALICE),
            0,
            b"ipfs://prediction-token-001".to_vec(),
            true,
        ));

        assert_noop!(
            AiPredictions::transfer_prediction_token(RuntimeOrigin::signed(ALICE), 0, BOB,),
            Error::<crate::mock::Test>::TransfersDisabled
        );
    });
}

#[test]
fn transfer_prediction_token_success_when_enabled() {
    new_test_ext().execute_with(|| {
        register_default_model();
        submit_default_prediction();

        assert_ok!(AiPredictions::set_tokenization_config(
            RuntimeOrigin::root(),
            true,
            true,
        ));

        assert_ok!(AiPredictions::tokenize_prediction(
            RuntimeOrigin::signed(ALICE),
            0,
            b"ipfs://prediction-token-001".to_vec(),
            true,
        ));

        assert_ok!(AiPredictions::transfer_prediction_token(
            RuntimeOrigin::signed(ALICE),
            0,
            BOB,
        ));

        let token = AiPredictions::prediction_tokens(0).expect("token should exist");
        assert_eq!(token.owner, BOB);
        assert!(token.updated_at.is_some());
    });
}

#[test]
fn approved_account_can_transfer_prediction_token() {
    new_test_ext().execute_with(|| {
        register_default_model();
        submit_default_prediction();

        assert_ok!(AiPredictions::set_tokenization_config(
            RuntimeOrigin::root(),
            true,
            true,
        ));

        assert_ok!(AiPredictions::tokenize_prediction(
            RuntimeOrigin::signed(ALICE),
            0,
            b"ipfs://prediction-token-001".to_vec(),
            true,
        ));

        assert_ok!(AiPredictions::approve_prediction_token_transfer(
            RuntimeOrigin::signed(ALICE),
            0,
            BOB,
        ));

        assert_eq!(AiPredictions::prediction_token_approvals(0), Some(BOB));

        assert_ok!(AiPredictions::transfer_prediction_token(
            RuntimeOrigin::signed(BOB),
            0,
            VALIDATOR,
        ));

        let token = AiPredictions::prediction_tokens(0).expect("token should exist");
        assert_eq!(token.owner, VALIDATOR);
        assert_eq!(AiPredictions::prediction_token_approvals(0), None);
    });
}

#[test]
fn revoke_prediction_token_approval_success() {
    new_test_ext().execute_with(|| {
        register_default_model();
        submit_default_prediction();

        assert_ok!(AiPredictions::set_tokenization_config(
            RuntimeOrigin::root(),
            true,
            true,
        ));

        assert_ok!(AiPredictions::tokenize_prediction(
            RuntimeOrigin::signed(ALICE),
            0,
            b"ipfs://prediction-token-001".to_vec(),
            true,
        ));

        assert_ok!(AiPredictions::approve_prediction_token_transfer(
            RuntimeOrigin::signed(ALICE),
            0,
            BOB,
        ));

        assert_ok!(AiPredictions::revoke_prediction_token_approval(
            RuntimeOrigin::signed(ALICE),
            0,
        ));

        assert_eq!(AiPredictions::prediction_token_approvals(0), None);
    });
}

#[test]
fn freeze_blocks_prediction_token_transfer() {
    new_test_ext().execute_with(|| {
        register_default_model();
        submit_default_prediction();

        assert_ok!(AiPredictions::set_tokenization_config(
            RuntimeOrigin::root(),
            true,
            true,
        ));

        assert_ok!(AiPredictions::tokenize_prediction(
            RuntimeOrigin::signed(ALICE),
            0,
            b"ipfs://prediction-token-001".to_vec(),
            true,
        ));

        assert_ok!(AiPredictions::freeze_prediction_token(
            RuntimeOrigin::root(),
            0,
        ));

        assert_noop!(
            AiPredictions::transfer_prediction_token(RuntimeOrigin::signed(ALICE), 0, BOB,),
            Error::<crate::mock::Test>::TokenFrozen
        );

        assert_ok!(AiPredictions::unfreeze_prediction_token(
            RuntimeOrigin::root(),
            0,
        ));

        assert_ok!(AiPredictions::transfer_prediction_token(
            RuntimeOrigin::signed(ALICE),
            0,
            BOB,
        ));
    });
}

#[test]
fn burn_prediction_token_success() {
    new_test_ext().execute_with(|| {
        register_default_model();
        submit_default_prediction();

        assert_ok!(AiPredictions::set_tokenization_config(
            RuntimeOrigin::root(),
            true,
            true,
        ));

        assert_ok!(AiPredictions::tokenize_prediction(
            RuntimeOrigin::signed(ALICE),
            0,
            b"ipfs://prediction-token-001".to_vec(),
            true,
        ));

        assert_ok!(AiPredictions::approve_prediction_token_transfer(
            RuntimeOrigin::signed(ALICE),
            0,
            BOB,
        ));

        assert_ok!(AiPredictions::burn_prediction_token(
            RuntimeOrigin::signed(ALICE),
            0,
        ));

        let token = AiPredictions::prediction_tokens(0).expect("token should exist");
        assert!(token.burned);
        assert!(token.frozen);
        assert!(token.updated_at.is_some());
        assert_eq!(AiPredictions::prediction_token_approvals(0), None);
    });
}

#[test]
fn burned_prediction_token_cannot_update_metadata() {
    new_test_ext().execute_with(|| {
        register_default_model();
        submit_default_prediction();

        assert_ok!(AiPredictions::set_tokenization_config(
            RuntimeOrigin::root(),
            true,
            true,
        ));

        assert_ok!(AiPredictions::tokenize_prediction(
            RuntimeOrigin::signed(ALICE),
            0,
            b"ipfs://prediction-token-001".to_vec(),
            true,
        ));

        assert_ok!(AiPredictions::burn_prediction_token(
            RuntimeOrigin::signed(ALICE),
            0,
        ));

        assert_noop!(
            AiPredictions::update_prediction_token_metadata(
                RuntimeOrigin::signed(ALICE),
                0,
                b"ipfs://prediction-token-updated".to_vec(),
            ),
            Error::<crate::mock::Test>::TokenBurned
        );
    });
}

#[test]
fn update_prediction_token_metadata_success() {
    new_test_ext().execute_with(|| {
        register_default_model();
        submit_default_prediction();

        assert_ok!(AiPredictions::set_tokenization_config(
            RuntimeOrigin::root(),
            true,
            true,
        ));

        assert_ok!(AiPredictions::tokenize_prediction(
            RuntimeOrigin::signed(ALICE),
            0,
            b"ipfs://prediction-token-001".to_vec(),
            true,
        ));

        assert_ok!(AiPredictions::update_prediction_token_metadata(
            RuntimeOrigin::signed(ALICE),
            0,
            b"ipfs://prediction-token-updated".to_vec(),
        ));

        let token = AiPredictions::prediction_tokens(0).expect("token should exist");
        assert_eq!(
            token.metadata_uri.to_vec(),
            b"ipfs://prediction-token-updated".to_vec()
        );
        assert!(token.updated_at.is_some());
    });
}

#[test]
fn reject_transfer_of_non_transferable_prediction_token() {
    new_test_ext().execute_with(|| {
        register_default_model();
        submit_default_prediction();

        assert_ok!(AiPredictions::set_tokenization_config(
            RuntimeOrigin::root(),
            true,
            true,
        ));

        assert_ok!(AiPredictions::tokenize_prediction(
            RuntimeOrigin::signed(ALICE),
            0,
            b"ipfs://prediction-token-001".to_vec(),
            false,
        ));

        assert_noop!(
            AiPredictions::transfer_prediction_token(RuntimeOrigin::signed(ALICE), 0, BOB,),
            Error::<crate::mock::Test>::TokenNotTransferable
        );
    });
}

fn create_phase3_token() {
    assert_ok!(AiPredictions::set_tokenization_config(
        RuntimeOrigin::root(),
        true,
        true,
    ));

    register_default_model();
    submit_default_prediction();

    assert_ok!(AiPredictions::tokenize_prediction(
        RuntimeOrigin::signed(ALICE),
        0,
        b"ipfs://prediction-token-phase3".to_vec(),
        true,
    ));
}

#[test]
fn staking_disabled_by_default() {
    new_test_ext().execute_with(|| {
        create_phase3_token();

        let config = AiPredictions::staking_config();
        assert!(!config.staking_enabled);
        assert_eq!(config.min_stake, 0);

        assert_noop!(
            AiPredictions::stake_on_prediction_token(RuntimeOrigin::signed(ALICE), 0, 100),
            Error::<crate::mock::Test>::StakingDisabled
        );
    });
}

#[test]
fn set_staking_config_by_root() {
    new_test_ext().execute_with(|| {
        assert_ok!(AiPredictions::set_staking_config(
            RuntimeOrigin::root(),
            true,
            50,
        ));

        let config = AiPredictions::staking_config();
        assert!(config.staking_enabled);
        assert_eq!(config.min_stake, 50);

        System::assert_last_event(
            Event::StakingConfigUpdated {
                staking_enabled: true,
                min_stake: 50,
            }
            .into(),
        );
    });
}

#[test]
fn reject_stake_below_minimum() {
    new_test_ext().execute_with(|| {
        create_phase3_token();

        assert_ok!(AiPredictions::set_staking_config(
            RuntimeOrigin::root(),
            true,
            100,
        ));

        assert_noop!(
            AiPredictions::stake_on_prediction_token(RuntimeOrigin::signed(ALICE), 0, 99),
            Error::<crate::mock::Test>::StakeBelowMinimum
        );

        assert_eq!(Balances::reserved_balance(ALICE), 0);
        assert_eq!(AiPredictions::prediction_token_total_stake(0), 0);
    });
}

#[test]
fn reject_zero_stake() {
    new_test_ext().execute_with(|| {
        create_phase3_token();

        assert_ok!(AiPredictions::set_staking_config(
            RuntimeOrigin::root(),
            true,
            0,
        ));

        assert_noop!(
            AiPredictions::stake_on_prediction_token(RuntimeOrigin::signed(ALICE), 0, 0),
            Error::<crate::mock::Test>::StakeAmountZero
        );

        assert_eq!(Balances::reserved_balance(ALICE), 0);
    });
}

#[test]
fn stake_on_prediction_token_reserves_balance() {
    new_test_ext().execute_with(|| {
        create_phase3_token();

        assert_ok!(AiPredictions::set_staking_config(
            RuntimeOrigin::root(),
            true,
            100,
        ));

        assert_eq!(Balances::free_balance(ALICE), 1_000_000);
        assert_eq!(Balances::reserved_balance(ALICE), 0);

        assert_ok!(AiPredictions::stake_on_prediction_token(
            RuntimeOrigin::signed(ALICE),
            0,
            250,
        ));

        assert_eq!(Balances::reserved_balance(ALICE), 250);
        assert_eq!(AiPredictions::prediction_token_total_stake(0), 250);

        let stake =
            AiPredictions::prediction_token_stakes(0, ALICE).expect("stake record should exist");
        assert_eq!(stake.amount, 250);
        assert_eq!(stake.created_at, 1);
        assert!(stake.updated_at.is_none());

        System::assert_last_event(
            Event::PredictionTokenStakeAdded {
                token_id: 0,
                staker: ALICE,
                amount: 250,
                total_stake: 250,
            }
            .into(),
        );
    });
}

#[test]
fn additional_stake_accumulates_existing_position() {
    new_test_ext().execute_with(|| {
        create_phase3_token();

        assert_ok!(AiPredictions::set_staking_config(
            RuntimeOrigin::root(),
            true,
            100,
        ));

        assert_ok!(AiPredictions::stake_on_prediction_token(
            RuntimeOrigin::signed(ALICE),
            0,
            250,
        ));

        System::set_block_number(2);

        assert_ok!(AiPredictions::stake_on_prediction_token(
            RuntimeOrigin::signed(ALICE),
            0,
            150,
        ));

        assert_eq!(Balances::reserved_balance(ALICE), 400);
        assert_eq!(AiPredictions::prediction_token_total_stake(0), 400);

        let stake =
            AiPredictions::prediction_token_stakes(0, ALICE).expect("stake record should exist");
        assert_eq!(stake.amount, 400);
        assert_eq!(stake.created_at, 1);
        assert_eq!(stake.updated_at, Some(2));
    });
}

#[test]
fn partial_unstake_releases_reserved_balance() {
    new_test_ext().execute_with(|| {
        create_phase3_token();

        assert_ok!(AiPredictions::set_staking_config(
            RuntimeOrigin::root(),
            true,
            100,
        ));

        assert_ok!(AiPredictions::stake_on_prediction_token(
            RuntimeOrigin::signed(ALICE),
            0,
            400,
        ));

        System::set_block_number(3);

        assert_ok!(AiPredictions::unstake_prediction_token(
            RuntimeOrigin::signed(ALICE),
            0,
            150,
        ));

        assert_eq!(Balances::reserved_balance(ALICE), 250);
        assert_eq!(AiPredictions::prediction_token_total_stake(0), 250);

        let stake =
            AiPredictions::prediction_token_stakes(0, ALICE).expect("stake record should exist");
        assert_eq!(stake.amount, 250);
        assert_eq!(stake.updated_at, Some(3));

        System::assert_last_event(
            Event::PredictionTokenStakeRemoved {
                token_id: 0,
                staker: ALICE,
                amount: 150,
                remaining_stake: 250,
            }
            .into(),
        );
    });
}

#[test]
fn full_unstake_removes_position() {
    new_test_ext().execute_with(|| {
        create_phase3_token();

        assert_ok!(AiPredictions::set_staking_config(
            RuntimeOrigin::root(),
            true,
            100,
        ));

        assert_ok!(AiPredictions::stake_on_prediction_token(
            RuntimeOrigin::signed(ALICE),
            0,
            400,
        ));

        assert_ok!(AiPredictions::unstake_prediction_token(
            RuntimeOrigin::signed(ALICE),
            0,
            400,
        ));

        assert_eq!(Balances::reserved_balance(ALICE), 0);
        assert_eq!(AiPredictions::prediction_token_total_stake(0), 0);
        assert!(AiPredictions::prediction_token_stakes(0, ALICE).is_none());
    });
}

#[test]
fn reject_unstake_more_than_position() {
    new_test_ext().execute_with(|| {
        create_phase3_token();

        assert_ok!(AiPredictions::set_staking_config(
            RuntimeOrigin::root(),
            true,
            100,
        ));

        assert_ok!(AiPredictions::stake_on_prediction_token(
            RuntimeOrigin::signed(ALICE),
            0,
            300,
        ));

        assert_noop!(
            AiPredictions::unstake_prediction_token(RuntimeOrigin::signed(ALICE), 0, 301),
            Error::<crate::mock::Test>::InsufficientStake
        );

        assert_eq!(Balances::reserved_balance(ALICE), 300);
        assert_eq!(AiPredictions::prediction_token_total_stake(0), 300);
    });
}

#[test]
fn lock_blocks_stake_and_unstake_until_released() {
    new_test_ext().execute_with(|| {
        create_phase3_token();

        assert_ok!(AiPredictions::set_staking_config(
            RuntimeOrigin::root(),
            true,
            100,
        ));

        assert_ok!(AiPredictions::stake_on_prediction_token(
            RuntimeOrigin::signed(ALICE),
            0,
            300,
        ));

        assert_ok!(AiPredictions::lock_prediction_token_stake(
            RuntimeOrigin::root(),
            0,
        ));

        assert!(AiPredictions::prediction_token_stake_locked(0));

        assert_noop!(
            AiPredictions::stake_on_prediction_token(RuntimeOrigin::signed(BOB), 0, 100),
            Error::<crate::mock::Test>::StakeLocked
        );

        assert_noop!(
            AiPredictions::unstake_prediction_token(RuntimeOrigin::signed(ALICE), 0, 100),
            Error::<crate::mock::Test>::StakeLocked
        );

        assert_ok!(AiPredictions::release_prediction_token_stake(
            RuntimeOrigin::root(),
            0,
        ));

        assert!(!AiPredictions::prediction_token_stake_locked(0));

        assert_ok!(AiPredictions::unstake_prediction_token(
            RuntimeOrigin::signed(ALICE),
            0,
            100,
        ));

        assert_eq!(Balances::reserved_balance(ALICE), 200);
        assert_eq!(AiPredictions::prediction_token_total_stake(0), 200);
    });
}

#[test]
fn reject_stake_on_burned_token() {
    new_test_ext().execute_with(|| {
        create_phase3_token();

        assert_ok!(AiPredictions::set_staking_config(
            RuntimeOrigin::root(),
            true,
            100,
        ));

        assert_ok!(AiPredictions::burn_prediction_token(
            RuntimeOrigin::signed(ALICE),
            0,
        ));

        assert_noop!(
            AiPredictions::stake_on_prediction_token(RuntimeOrigin::signed(ALICE), 0, 100),
            Error::<crate::mock::Test>::TokenBurned
        );
    });
}

#[test]
fn reject_stake_on_frozen_token() {
    new_test_ext().execute_with(|| {
        create_phase3_token();

        assert_ok!(AiPredictions::set_staking_config(
            RuntimeOrigin::root(),
            true,
            100,
        ));

        assert_ok!(AiPredictions::freeze_prediction_token(
            RuntimeOrigin::root(),
            0,
        ));

        assert_noop!(
            AiPredictions::stake_on_prediction_token(RuntimeOrigin::signed(ALICE), 0, 100),
            Error::<crate::mock::Test>::TokenFrozen
        );
    });
}

#[test]
fn phase4a_settlement_disabled_by_default() {
    new_test_ext().execute_with(|| {
        create_phase3_token();

        let config = AiPredictions::settlement_config();
        assert!(!config.settlement_enabled);
        assert_eq!(config.dispute_window_blocks, 0);
        assert_eq!(config.min_dispute_bond, 0);

        assert_noop!(
            AiPredictions::propose_prediction_outcome(
                RuntimeOrigin::root(),
                0,
                crate::SettlementOutcome::Correct,
                b"ipfs://settlement-evidence".to_vec(),
            ),
            Error::<crate::mock::Test>::SettlementDisabled
        );
    });
}

#[test]
fn phase4a_set_settlement_config_by_root() {
    new_test_ext().execute_with(|| {
        assert_ok!(AiPredictions::set_settlement_config(
            RuntimeOrigin::root(),
            true,
            10,
            25,
        ));

        let config = AiPredictions::settlement_config();
        assert!(config.settlement_enabled);
        assert_eq!(config.dispute_window_blocks, 10);
        assert_eq!(config.min_dispute_bond, 25);

        System::assert_last_event(
            Event::SettlementConfigUpdated {
                settlement_enabled: true,
                dispute_window_blocks: 10,
                min_dispute_bond: 25,
            }
            .into(),
        );
    });
}

#[test]
fn phase4a_unauthorized_account_cannot_propose_outcome() {
    new_test_ext().execute_with(|| {
        create_phase3_token();

        assert_ok!(AiPredictions::set_settlement_config(
            RuntimeOrigin::root(),
            true,
            10,
            25,
        ));

        assert_noop!(
            AiPredictions::propose_prediction_outcome(
                RuntimeOrigin::signed(BOB),
                0,
                crate::SettlementOutcome::Correct,
                b"ipfs://settlement-evidence".to_vec(),
            ),
            Error::<crate::mock::Test>::NotAuthorizedSettlementProposer
        );
    });
}

#[test]
fn phase4a_authorized_validator_can_propose_outcome() {
    new_test_ext().execute_with(|| {
        create_phase3_token();

        assert_ok!(AiPredictions::set_settlement_config(
            RuntimeOrigin::root(),
            true,
            5,
            25,
        ));

        assert_ok!(AiPredictions::authorize_validator(
            RuntimeOrigin::root(),
            VALIDATOR,
        ));

        System::set_block_number(10);

        assert_ok!(AiPredictions::propose_prediction_outcome(
            RuntimeOrigin::signed(VALIDATOR),
            0,
            crate::SettlementOutcome::Correct,
            b"ipfs://settlement-evidence".to_vec(),
        ));

        let settlement =
            AiPredictions::prediction_token_settlements(0).expect("settlement should exist");

        assert_eq!(settlement.token_id, 0);
        assert_eq!(
            settlement.status,
            crate::SettlementStatus::DisputeWindowOpen
        );
        assert_eq!(
            settlement.proposed_outcome,
            Some(crate::SettlementOutcome::Correct)
        );
        assert_eq!(settlement.final_outcome, None);
        assert_eq!(settlement.proposed_by, Some(VALIDATOR));
        assert_eq!(settlement.proposed_at, Some(10));
        assert_eq!(settlement.dispute_until, Some(15));
        assert_eq!(settlement.finalized_at, None);
        assert_eq!(settlement.dispute_count, 0);
        assert_eq!(
            settlement
                .evidence_uri
                .expect("evidence should exist")
                .to_vec(),
            b"ipfs://settlement-evidence".to_vec()
        );

        System::assert_last_event(
            Event::PredictionOutcomeProposed {
                token_id: 0,
                proposed_outcome: crate::SettlementOutcome::Correct,
                proposed_by: Some(VALIDATOR),
                dispute_until: 15,
            }
            .into(),
        );
    });
}

#[test]
fn phase4a_user_can_dispute_with_bond_reserved() {
    new_test_ext().execute_with(|| {
        create_phase3_token();

        assert_ok!(AiPredictions::set_settlement_config(
            RuntimeOrigin::root(),
            true,
            10,
            25,
        ));

        assert_ok!(AiPredictions::authorize_validator(
            RuntimeOrigin::root(),
            VALIDATOR,
        ));

        assert_ok!(AiPredictions::propose_prediction_outcome(
            RuntimeOrigin::signed(VALIDATOR),
            0,
            crate::SettlementOutcome::Correct,
            b"ipfs://settlement-evidence".to_vec(),
        ));

        System::set_block_number(3);

        assert_eq!(Balances::reserved_balance(BOB), 0);

        assert_ok!(AiPredictions::dispute_prediction_outcome(
            RuntimeOrigin::signed(BOB),
            0,
            crate::DisputeReason::WrongOutcome,
            b"ipfs://dispute-evidence".to_vec(),
            25,
        ));

        assert_eq!(Balances::reserved_balance(BOB), 25);
        assert_eq!(AiPredictions::next_dispute_id(), 1);

        let settlement =
            AiPredictions::prediction_token_settlements(0).expect("settlement should exist");

        assert_eq!(settlement.status, crate::SettlementStatus::Disputed);
        assert_eq!(settlement.dispute_count, 1);

        let dispute = AiPredictions::prediction_token_disputes(0).expect("dispute should exist");

        assert_eq!(dispute.dispute_id, 0);
        assert_eq!(dispute.token_id, 0);
        assert_eq!(dispute.disputed_by, BOB);
        assert_eq!(dispute.reason, crate::DisputeReason::WrongOutcome);
        assert_eq!(dispute.bond, 25);
        assert_eq!(dispute.submitted_at, 3);
        assert!(!dispute.resolved);
        assert_eq!(dispute.accepted, None);
        assert_eq!(
            dispute.evidence_uri.to_vec(),
            b"ipfs://dispute-evidence".to_vec()
        );

        System::assert_last_event(
            Event::PredictionOutcomeDisputed {
                token_id: 0,
                dispute_id: 0,
                disputed_by: BOB,
                reason: crate::DisputeReason::WrongOutcome,
                bond: 25,
            }
            .into(),
        );
    });
}

#[test]
fn phase4a_dispute_fails_after_dispute_window_closes() {
    new_test_ext().execute_with(|| {
        create_phase3_token();

        assert_ok!(AiPredictions::set_settlement_config(
            RuntimeOrigin::root(),
            true,
            2,
            25,
        ));

        assert_ok!(AiPredictions::propose_prediction_outcome(
            RuntimeOrigin::root(),
            0,
            crate::SettlementOutcome::Correct,
            b"ipfs://settlement-evidence".to_vec(),
        ));

        System::set_block_number(4);

        assert_noop!(
            AiPredictions::dispute_prediction_outcome(
                RuntimeOrigin::signed(BOB),
                0,
                crate::DisputeReason::WrongOutcome,
                b"ipfs://late-dispute-evidence".to_vec(),
                25,
            ),
            Error::<crate::mock::Test>::DisputeWindowClosed
        );

        assert_eq!(Balances::reserved_balance(BOB), 0);
        assert_eq!(AiPredictions::next_dispute_id(), 0);
    });
}

#[test]
fn phase4a_admin_can_finalize_disputed_settlement() {
    new_test_ext().execute_with(|| {
        create_phase3_token();

        assert_ok!(AiPredictions::set_staking_config(
            RuntimeOrigin::root(),
            true,
            100,
        ));

        assert_ok!(AiPredictions::stake_on_prediction_token(
            RuntimeOrigin::signed(ALICE),
            0,
            250,
        ));

        assert_ok!(AiPredictions::set_settlement_config(
            RuntimeOrigin::root(),
            true,
            10,
            25,
        ));

        assert_ok!(AiPredictions::propose_prediction_outcome(
            RuntimeOrigin::root(),
            0,
            crate::SettlementOutcome::Correct,
            b"ipfs://settlement-evidence".to_vec(),
        ));

        assert_ok!(AiPredictions::dispute_prediction_outcome(
            RuntimeOrigin::signed(BOB),
            0,
            crate::DisputeReason::WrongOutcome,
            b"ipfs://dispute-evidence".to_vec(),
            25,
        ));

        System::set_block_number(8);

        assert_ok!(AiPredictions::admin_finalize_prediction_outcome(
            RuntimeOrigin::root(),
            0,
            crate::SettlementOutcome::Incorrect,
            b"ipfs://admin-final-evidence".to_vec(),
        ));

        let settlement =
            AiPredictions::prediction_token_settlements(0).expect("settlement should exist");

        assert_eq!(settlement.status, crate::SettlementStatus::Finalized);
        assert_eq!(
            settlement.final_outcome,
            Some(crate::SettlementOutcome::Incorrect)
        );
        assert_eq!(settlement.finalized_by, None);
        assert_eq!(settlement.finalized_at, Some(8));
        assert_eq!(settlement.total_stake_at_finalization, 250);
        assert_eq!(
            settlement
                .evidence_uri
                .expect("evidence should exist")
                .to_vec(),
            b"ipfs://admin-final-evidence".to_vec()
        );

        System::assert_last_event(
            Event::PredictionOutcomeFinalized {
                token_id: 0,
                final_outcome: crate::SettlementOutcome::Incorrect,
                finalized_by: None,
            }
            .into(),
        );
    });
}

#[test]
fn phase4a_anyone_can_finalize_undisputed_after_dispute_window() {
    new_test_ext().execute_with(|| {
        create_phase3_token();

        assert_ok!(AiPredictions::set_settlement_config(
            RuntimeOrigin::root(),
            true,
            3,
            25,
        ));

        assert_ok!(AiPredictions::propose_prediction_outcome(
            RuntimeOrigin::root(),
            0,
            crate::SettlementOutcome::Correct,
            b"ipfs://settlement-evidence".to_vec(),
        ));

        System::set_block_number(5);

        assert_ok!(
            AiPredictions::finalize_prediction_outcome_after_dispute_window(
                RuntimeOrigin::signed(ALICE),
                0,
            )
        );

        let settlement =
            AiPredictions::prediction_token_settlements(0).expect("settlement should exist");

        assert_eq!(settlement.status, crate::SettlementStatus::Finalized);
        assert_eq!(
            settlement.final_outcome,
            Some(crate::SettlementOutcome::Correct)
        );
        assert_eq!(settlement.finalized_by, Some(ALICE));
        assert_eq!(settlement.finalized_at, Some(5));

        System::assert_last_event(
            Event::PredictionOutcomeFinalized {
                token_id: 0,
                final_outcome: crate::SettlementOutcome::Correct,
                finalized_by: Some(ALICE),
            }
            .into(),
        );
    });
}

#[test]
fn phase4a_cannot_finalize_before_dispute_window_closes() {
    new_test_ext().execute_with(|| {
        create_phase3_token();

        assert_ok!(AiPredictions::set_settlement_config(
            RuntimeOrigin::root(),
            true,
            10,
            25,
        ));

        assert_ok!(AiPredictions::propose_prediction_outcome(
            RuntimeOrigin::root(),
            0,
            crate::SettlementOutcome::Correct,
            b"ipfs://settlement-evidence".to_vec(),
        ));

        System::set_block_number(5);

        assert_noop!(
            AiPredictions::finalize_prediction_outcome_after_dispute_window(
                RuntimeOrigin::signed(ALICE),
                0,
            ),
            Error::<crate::mock::Test>::DisputeWindowStillOpen
        );
    });
}

#[test]
fn phase4a_root_can_cancel_settlement_before_finalization() {
    new_test_ext().execute_with(|| {
        create_phase3_token();

        assert_ok!(AiPredictions::cancel_prediction_settlement(
            RuntimeOrigin::root(),
            0,
            b"ipfs://cancel-evidence".to_vec(),
        ));

        let settlement =
            AiPredictions::prediction_token_settlements(0).expect("settlement should exist");

        assert_eq!(settlement.status, crate::SettlementStatus::Cancelled);
        assert_eq!(
            settlement.final_outcome,
            Some(crate::SettlementOutcome::Cancelled)
        );
        assert_eq!(settlement.finalized_at, Some(1));
        assert_eq!(
            settlement
                .evidence_uri
                .expect("evidence should exist")
                .to_vec(),
            b"ipfs://cancel-evidence".to_vec()
        );

        System::assert_last_event(Event::PredictionSettlementCancelled { token_id: 0 }.into());
    });
}

#[test]
fn phase4a_finalized_settlement_cannot_be_disputed() {
    new_test_ext().execute_with(|| {
        create_phase3_token();

        assert_ok!(AiPredictions::set_settlement_config(
            RuntimeOrigin::root(),
            true,
            2,
            25,
        ));

        assert_ok!(AiPredictions::propose_prediction_outcome(
            RuntimeOrigin::root(),
            0,
            crate::SettlementOutcome::Correct,
            b"ipfs://settlement-evidence".to_vec(),
        ));

        System::set_block_number(4);

        assert_ok!(
            AiPredictions::finalize_prediction_outcome_after_dispute_window(
                RuntimeOrigin::signed(ALICE),
                0,
            )
        );

        assert_noop!(
            AiPredictions::dispute_prediction_outcome(
                RuntimeOrigin::signed(BOB),
                0,
                crate::DisputeReason::WrongOutcome,
                b"ipfs://dispute-evidence".to_vec(),
                25,
            ),
            Error::<crate::mock::Test>::CannotDisputeFinalizedSettlement
        );
    });
}

#[test]
fn phase4a_finalized_settlement_cannot_be_cancelled() {
    new_test_ext().execute_with(|| {
        create_phase3_token();

        assert_ok!(AiPredictions::set_settlement_config(
            RuntimeOrigin::root(),
            true,
            2,
            25,
        ));

        assert_ok!(AiPredictions::propose_prediction_outcome(
            RuntimeOrigin::root(),
            0,
            crate::SettlementOutcome::Correct,
            b"ipfs://settlement-evidence".to_vec(),
        ));

        System::set_block_number(4);

        assert_ok!(
            AiPredictions::finalize_prediction_outcome_after_dispute_window(
                RuntimeOrigin::signed(ALICE),
                0,
            )
        );

        assert_noop!(
            AiPredictions::cancel_prediction_settlement(
                RuntimeOrigin::root(),
                0,
                b"ipfs://cancel-evidence".to_vec(),
            ),
            Error::<crate::mock::Test>::SettlementAlreadyFinalized
        );
    });
}

#[test]
fn phase4b_settlement_economics_disabled_by_default() {
    new_test_ext().execute_with(|| {
        assert!(!SettlementEconomicsConfig::<crate::mock::Test>::get().economics_enabled);
        assert!(!SettlementEconomicsConfig::<crate::mock::Test>::get().slash_incorrect);
        assert!(!SettlementEconomicsConfig::<crate::mock::Test>::get().slash_fraudulent);
    });
}

#[test]
fn phase4b_set_settlement_economics_config_by_root() {
    new_test_ext().execute_with(|| {
        assert_ok!(AiPredictions::set_settlement_economics_config(
            RuntimeOrigin::root(),
            true,
            true,
            true,
        ));

        let config = SettlementEconomicsConfig::<crate::mock::Test>::get();
        assert!(config.economics_enabled);
        assert!(config.slash_incorrect);
        assert!(config.slash_fraudulent);
    });
}

#[test]
fn phase4b_correct_outcome_refunds_reserved_stake() {
    new_test_ext().execute_with(|| {
        create_phase3_token();
        let token_id = 0u64;

        assert_ok!(AiPredictions::set_staking_config(
            RuntimeOrigin::root(),
            true,
            1,
        ));

        assert_ok!(AiPredictions::stake_on_prediction_token(
            RuntimeOrigin::signed(BOB),
            token_id,
            100,
        ));

        assert_eq!(Balances::reserved_balance(BOB), 100);
        assert_eq!(AiPredictions::prediction_token_total_stake(token_id), 100);

        assert_ok!(AiPredictions::set_settlement_config(
            RuntimeOrigin::root(),
            true,
            1,
            25,
        ));

        assert_ok!(AiPredictions::authorize_validator(
            RuntimeOrigin::root(),
            ALICE
        ));

        assert_ok!(AiPredictions::propose_prediction_outcome(
            RuntimeOrigin::signed(ALICE),
            token_id,
            SettlementOutcome::Correct,
            b"ipfs://phase4b-correct".to_vec(),
        ));

        frame_system::Pallet::<crate::mock::Test>::set_block_number(10);

        assert_ok!(
            AiPredictions::finalize_prediction_outcome_after_dispute_window(
                RuntimeOrigin::signed(ALICE),
                token_id,
            )
        );

        assert_ok!(AiPredictions::set_settlement_economics_config(
            RuntimeOrigin::root(),
            true,
            true,
            true,
        ));

        assert_ok!(AiPredictions::claim_prediction_token_settlement(
            RuntimeOrigin::signed(BOB),
            token_id,
        ));

        assert_eq!(Balances::reserved_balance(BOB), 0);
        assert_eq!(AiPredictions::prediction_token_total_stake(token_id), 0);
        assert!(AiPredictions::prediction_token_stakes(token_id, BOB).is_none());

        let claim = PredictionTokenSettlementClaims::<crate::mock::Test>::get(token_id, BOB)
            .expect("claim must exist");
        assert_eq!(claim.refunded, 100);
        assert_eq!(claim.slashed, 0);
    });
}

#[test]
fn phase4b_incorrect_outcome_slashes_reserved_stake_when_enabled() {
    new_test_ext().execute_with(|| {
        create_phase3_token();
        let token_id = 0u64;

        assert_ok!(AiPredictions::set_staking_config(
            RuntimeOrigin::root(),
            true,
            1,
        ));

        let bob_free_before = Balances::free_balance(BOB);

        assert_ok!(AiPredictions::stake_on_prediction_token(
            RuntimeOrigin::signed(BOB),
            token_id,
            100,
        ));

        assert_eq!(Balances::reserved_balance(BOB), 100);

        assert_ok!(AiPredictions::set_settlement_config(
            RuntimeOrigin::root(),
            true,
            1,
            25,
        ));

        assert_ok!(AiPredictions::authorize_validator(
            RuntimeOrigin::root(),
            ALICE
        ));

        assert_ok!(AiPredictions::propose_prediction_outcome(
            RuntimeOrigin::signed(ALICE),
            token_id,
            SettlementOutcome::Correct,
            b"ipfs://phase4b-proposed".to_vec(),
        ));

        assert_ok!(AiPredictions::admin_finalize_prediction_outcome(
            RuntimeOrigin::root(),
            token_id,
            SettlementOutcome::Incorrect,
            b"ipfs://phase4b-final-incorrect".to_vec(),
        ));

        assert_ok!(AiPredictions::set_settlement_economics_config(
            RuntimeOrigin::root(),
            true,
            true,
            false,
        ));

        assert_ok!(AiPredictions::claim_prediction_token_settlement(
            RuntimeOrigin::signed(BOB),
            token_id,
        ));

        assert_eq!(Balances::reserved_balance(BOB), 0);
        assert_eq!(Balances::free_balance(BOB), bob_free_before - 100);
        assert_eq!(AiPredictions::prediction_token_total_stake(token_id), 0);

        let claim = PredictionTokenSettlementClaims::<crate::mock::Test>::get(token_id, BOB)
            .expect("claim must exist");
        assert_eq!(claim.refunded, 0);
        assert_eq!(claim.slashed, 100);
    });
}

#[test]
fn phase4b_duplicate_settlement_claim_rejected() {
    new_test_ext().execute_with(|| {
        create_phase3_token();
        let token_id = 0u64;

        assert_ok!(AiPredictions::set_staking_config(
            RuntimeOrigin::root(),
            true,
            1,
        ));

        assert_ok!(AiPredictions::stake_on_prediction_token(
            RuntimeOrigin::signed(BOB),
            token_id,
            100,
        ));

        assert_ok!(AiPredictions::set_settlement_config(
            RuntimeOrigin::root(),
            true,
            1,
            25,
        ));

        assert_ok!(AiPredictions::authorize_validator(
            RuntimeOrigin::root(),
            ALICE
        ));

        assert_ok!(AiPredictions::propose_prediction_outcome(
            RuntimeOrigin::signed(ALICE),
            token_id,
            SettlementOutcome::Correct,
            b"ipfs://phase4b-correct".to_vec(),
        ));

        frame_system::Pallet::<crate::mock::Test>::set_block_number(10);

        assert_ok!(
            AiPredictions::finalize_prediction_outcome_after_dispute_window(
                RuntimeOrigin::signed(ALICE),
                token_id,
            )
        );

        assert_ok!(AiPredictions::set_settlement_economics_config(
            RuntimeOrigin::root(),
            true,
            true,
            true,
        ));

        assert_ok!(AiPredictions::claim_prediction_token_settlement(
            RuntimeOrigin::signed(BOB),
            token_id,
        ));

        assert_noop!(
            AiPredictions::claim_prediction_token_settlement(RuntimeOrigin::signed(BOB), token_id),
            Error::<crate::mock::Test>::SettlementAlreadyClaimed
        );
    });
}

#[test]
fn phase4b_dispute_bond_accepted_returns_reserved_bond() {
    new_test_ext().execute_with(|| {
        create_phase3_token();
        let token_id = 0u64;

        assert_ok!(AiPredictions::set_settlement_config(
            RuntimeOrigin::root(),
            true,
            100,
            25,
        ));

        assert_ok!(AiPredictions::authorize_validator(
            RuntimeOrigin::root(),
            ALICE
        ));

        assert_ok!(AiPredictions::propose_prediction_outcome(
            RuntimeOrigin::signed(ALICE),
            token_id,
            SettlementOutcome::Correct,
            b"ipfs://phase4b-proposed".to_vec(),
        ));

        assert_ok!(AiPredictions::dispute_prediction_outcome(
            RuntimeOrigin::signed(BOB),
            token_id,
            DisputeReason::WrongOutcome,
            b"ipfs://phase4b-dispute".to_vec(),
            25,
        ));

        assert_eq!(Balances::reserved_balance(BOB), 25);

        assert_ok!(AiPredictions::admin_finalize_prediction_outcome(
            RuntimeOrigin::root(),
            token_id,
            SettlementOutcome::Incorrect,
            b"ipfs://phase4b-final".to_vec(),
        ));

        assert_ok!(AiPredictions::set_settlement_economics_config(
            RuntimeOrigin::root(),
            true,
            true,
            true,
        ));

        assert_ok!(AiPredictions::resolve_prediction_dispute_bond(
            RuntimeOrigin::root(),
            0,
            true,
        ));

        assert_eq!(Balances::reserved_balance(BOB), 0);

        let dispute = AiPredictions::prediction_token_disputes(0).expect("dispute must exist");
        assert!(dispute.resolved);
        assert_eq!(dispute.accepted, Some(true));
    });
}

#[test]
fn phase4b_dispute_bond_rejected_slashes_reserved_bond() {
    new_test_ext().execute_with(|| {
        create_phase3_token();
        let token_id = 0u64;

        assert_ok!(AiPredictions::set_settlement_config(
            RuntimeOrigin::root(),
            true,
            100,
            25,
        ));

        assert_ok!(AiPredictions::authorize_validator(
            RuntimeOrigin::root(),
            ALICE
        ));

        let bob_free_before = Balances::free_balance(BOB);

        assert_ok!(AiPredictions::propose_prediction_outcome(
            RuntimeOrigin::signed(ALICE),
            token_id,
            SettlementOutcome::Correct,
            b"ipfs://phase4b-proposed".to_vec(),
        ));

        assert_ok!(AiPredictions::dispute_prediction_outcome(
            RuntimeOrigin::signed(BOB),
            token_id,
            DisputeReason::WrongOutcome,
            b"ipfs://phase4b-dispute".to_vec(),
            25,
        ));

        assert_eq!(Balances::reserved_balance(BOB), 25);

        assert_ok!(AiPredictions::admin_finalize_prediction_outcome(
            RuntimeOrigin::root(),
            token_id,
            SettlementOutcome::Correct,
            b"ipfs://phase4b-final".to_vec(),
        ));

        assert_ok!(AiPredictions::set_settlement_economics_config(
            RuntimeOrigin::root(),
            true,
            true,
            true,
        ));

        assert_ok!(AiPredictions::resolve_prediction_dispute_bond(
            RuntimeOrigin::root(),
            0,
            false,
        ));

        assert_eq!(Balances::reserved_balance(BOB), 0);
        assert_eq!(Balances::free_balance(BOB), bob_free_before - 25);

        let dispute = AiPredictions::prediction_token_disputes(0).expect("dispute must exist");
        assert!(dispute.resolved);
        assert_eq!(dispute.accepted, Some(false));
    });
}

#[test]
fn phase5_market_economics_disabled_by_default() {
    new_test_ext().execute_with(|| {
        assert!(!PredictionMarketEconomicsConfig::<crate::mock::Test>::get().market_enabled);
        assert!(
            PredictionMarketEconomicsConfig::<crate::mock::Test>::get()
                .allow_unstake_before_settlement
        );
    });
}

#[test]
fn phase5_root_can_enable_market_economics() {
    new_test_ext().execute_with(|| {
        assert_ok!(AiPredictions::set_market_economics_config(
            RuntimeOrigin::root(),
            true,
            true,
        ));

        let config = PredictionMarketEconomicsConfig::<crate::mock::Test>::get();
        assert!(config.market_enabled);
        assert!(config.allow_unstake_before_settlement);
    });
}

#[test]
fn phase5_user_can_stake_on_yes_side() {
    new_test_ext().execute_with(|| {
        create_phase3_token();
        let token_id = 0u64;
        let bob_free_before = Balances::free_balance(BOB);
        let market_account = AiPredictions::market_account_id();

        assert_ok!(AiPredictions::set_staking_config(
            RuntimeOrigin::root(),
            true,
            1,
        ));

        assert_ok!(AiPredictions::set_market_economics_config(
            RuntimeOrigin::root(),
            true,
            true,
        ));

        assert_ok!(AiPredictions::stake_on_prediction_outcome_side(
            RuntimeOrigin::signed(BOB),
            token_id,
            PredictionOutcomeSide::Yes,
            100,
        ));

        assert_eq!(Balances::free_balance(BOB), bob_free_before - 100);
        assert_eq!(Balances::free_balance(market_account), 100);
        assert_eq!(
            AiPredictions::prediction_token_side_totals(token_id, PredictionOutcomeSide::Yes),
            100
        );

        let stake = AiPredictions::prediction_token_side_stakes(token_id, BOB)
            .expect("side stake must exist");
        assert_eq!(stake.side, PredictionOutcomeSide::Yes);
        assert_eq!(stake.amount, 100);
    });
}

#[test]
fn phase5_user_can_unstake_before_settlement_starts() {
    new_test_ext().execute_with(|| {
        create_phase3_token();
        let token_id = 0u64;
        let bob_free_before = Balances::free_balance(BOB);
        let market_account = AiPredictions::market_account_id();

        assert_ok!(AiPredictions::set_staking_config(
            RuntimeOrigin::root(),
            true,
            1,
        ));

        assert_ok!(AiPredictions::set_market_economics_config(
            RuntimeOrigin::root(),
            true,
            true,
        ));

        assert_ok!(AiPredictions::stake_on_prediction_outcome_side(
            RuntimeOrigin::signed(BOB),
            token_id,
            PredictionOutcomeSide::No,
            100,
        ));

        assert_ok!(AiPredictions::unstake_from_prediction_outcome_side(
            RuntimeOrigin::signed(BOB),
            token_id,
            PredictionOutcomeSide::No,
            40,
        ));

        assert_eq!(Balances::free_balance(BOB), bob_free_before - 60);
        assert_eq!(Balances::free_balance(market_account), 60);
        assert_eq!(
            AiPredictions::prediction_token_side_totals(token_id, PredictionOutcomeSide::No),
            60
        );

        let stake = AiPredictions::prediction_token_side_stakes(token_id, BOB)
            .expect("side stake must exist");
        assert_eq!(stake.side, PredictionOutcomeSide::No);
        assert_eq!(stake.amount, 60);
    });
}

#[test]
fn phase5_unstake_blocked_after_settlement_starts() {
    new_test_ext().execute_with(|| {
        create_phase3_token();
        let token_id = 0u64;

        assert_ok!(AiPredictions::set_staking_config(
            RuntimeOrigin::root(),
            true,
            1,
        ));

        assert_ok!(AiPredictions::set_market_economics_config(
            RuntimeOrigin::root(),
            true,
            true,
        ));

        assert_ok!(AiPredictions::stake_on_prediction_outcome_side(
            RuntimeOrigin::signed(BOB),
            token_id,
            PredictionOutcomeSide::Yes,
            100,
        ));

        assert_ok!(AiPredictions::set_settlement_config(
            RuntimeOrigin::root(),
            true,
            10,
            25,
        ));

        assert_ok!(AiPredictions::authorize_validator(
            RuntimeOrigin::root(),
            ALICE,
        ));

        assert_ok!(AiPredictions::propose_prediction_outcome(
            RuntimeOrigin::signed(ALICE),
            token_id,
            SettlementOutcome::Correct,
            b"ipfs://phase5-market-lock".to_vec(),
        ));

        assert!(AiPredictions::prediction_token_market_locked(token_id));

        assert_noop!(
            AiPredictions::unstake_from_prediction_outcome_side(
                RuntimeOrigin::signed(BOB),
                token_id,
                PredictionOutcomeSide::Yes,
                10,
            ),
            Error::<crate::mock::Test>::MarketAlreadyLocked
        );
    });
}

#[test]
fn phase5_winning_yes_claim_receives_stake_plus_losing_pool() {
    new_test_ext().execute_with(|| {
        create_phase3_token();
        let token_id = 0u64;
        let market_account = AiPredictions::market_account_id();

        assert_ok!(AiPredictions::set_staking_config(
            RuntimeOrigin::root(),
            true,
            1,
        ));

        assert_ok!(AiPredictions::set_market_economics_config(
            RuntimeOrigin::root(),
            true,
            true,
        ));

        let alice_free_before = Balances::free_balance(ALICE);
        let bob_free_before = Balances::free_balance(BOB);

        assert_ok!(AiPredictions::stake_on_prediction_outcome_side(
            RuntimeOrigin::signed(ALICE),
            token_id,
            PredictionOutcomeSide::Yes,
            100,
        ));

        assert_ok!(AiPredictions::stake_on_prediction_outcome_side(
            RuntimeOrigin::signed(BOB),
            token_id,
            PredictionOutcomeSide::No,
            50,
        ));

        assert_eq!(Balances::free_balance(ALICE), alice_free_before - 100);
        assert_eq!(Balances::free_balance(BOB), bob_free_before - 50);
        assert_eq!(Balances::free_balance(market_account), 150);

        assert_ok!(AiPredictions::set_settlement_config(
            RuntimeOrigin::root(),
            true,
            10,
            25,
        ));

        assert_ok!(AiPredictions::authorize_validator(
            RuntimeOrigin::root(),
            ALICE,
        ));

        assert_ok!(AiPredictions::propose_prediction_outcome(
            RuntimeOrigin::signed(ALICE),
            token_id,
            SettlementOutcome::Correct,
            b"ipfs://phase5b-yes-win".to_vec(),
        ));

        assert_ok!(AiPredictions::admin_finalize_prediction_outcome(
            RuntimeOrigin::root(),
            token_id,
            SettlementOutcome::Correct,
            b"ipfs://phase5b-final-correct".to_vec(),
        ));

        assert_ok!(AiPredictions::claim_prediction_market_payout(
            RuntimeOrigin::signed(ALICE),
            token_id,
        ));

        assert_eq!(Balances::free_balance(ALICE), alice_free_before + 50);
        assert_eq!(Balances::free_balance(market_account), 0);

        let claim = AiPredictions::prediction_token_side_payout_claims(token_id, ALICE)
            .expect("winner payout claim must exist");
        assert_eq!(claim.stake_returned, 100);
        assert_eq!(claim.reward_paid, 50);
        assert_eq!(claim.slashed, 0);
        assert!(AiPredictions::prediction_token_side_stakes(token_id, ALICE).is_none());
    });
}

#[test]
fn phase5_losing_no_claim_records_slash_without_transfer() {
    new_test_ext().execute_with(|| {
        create_phase3_token();
        let token_id = 0u64;
        let market_account = AiPredictions::market_account_id();

        assert_ok!(AiPredictions::set_staking_config(
            RuntimeOrigin::root(),
            true,
            1,
        ));

        assert_ok!(AiPredictions::set_market_economics_config(
            RuntimeOrigin::root(),
            true,
            true,
        ));

        let alice_free_before = Balances::free_balance(ALICE);
        let bob_free_before = Balances::free_balance(BOB);

        assert_ok!(AiPredictions::stake_on_prediction_outcome_side(
            RuntimeOrigin::signed(ALICE),
            token_id,
            PredictionOutcomeSide::Yes,
            100,
        ));

        assert_ok!(AiPredictions::stake_on_prediction_outcome_side(
            RuntimeOrigin::signed(BOB),
            token_id,
            PredictionOutcomeSide::No,
            50,
        ));

        assert_eq!(Balances::free_balance(market_account), 150);

        assert_ok!(AiPredictions::set_settlement_config(
            RuntimeOrigin::root(),
            true,
            10,
            25,
        ));

        assert_ok!(AiPredictions::authorize_validator(
            RuntimeOrigin::root(),
            ALICE,
        ));

        assert_ok!(AiPredictions::propose_prediction_outcome(
            RuntimeOrigin::signed(ALICE),
            token_id,
            SettlementOutcome::Correct,
            b"ipfs://phase5b-yes-win".to_vec(),
        ));

        assert_ok!(AiPredictions::admin_finalize_prediction_outcome(
            RuntimeOrigin::root(),
            token_id,
            SettlementOutcome::Correct,
            b"ipfs://phase5b-final-correct".to_vec(),
        ));

        assert_ok!(AiPredictions::claim_prediction_market_payout(
            RuntimeOrigin::signed(BOB),
            token_id,
        ));

        assert_eq!(Balances::free_balance(ALICE), alice_free_before - 100);
        assert_eq!(Balances::free_balance(BOB), bob_free_before - 50);
        assert_eq!(Balances::free_balance(market_account), 150);

        let claim = AiPredictions::prediction_token_side_payout_claims(token_id, BOB)
            .expect("loser payout claim must exist");
        assert_eq!(claim.stake_returned, 0);
        assert_eq!(claim.reward_paid, 0);
        assert_eq!(claim.slashed, 50);
        assert!(AiPredictions::prediction_token_side_stakes(token_id, BOB).is_none());
    });
}

#[test]
fn phase5_duplicate_market_payout_claim_rejected() {
    new_test_ext().execute_with(|| {
        create_phase3_token();
        let token_id = 0u64;

        assert_ok!(AiPredictions::set_staking_config(
            RuntimeOrigin::root(),
            true,
            1,
        ));

        assert_ok!(AiPredictions::set_market_economics_config(
            RuntimeOrigin::root(),
            true,
            true,
        ));

        assert_ok!(AiPredictions::stake_on_prediction_outcome_side(
            RuntimeOrigin::signed(ALICE),
            token_id,
            PredictionOutcomeSide::Yes,
            100,
        ));

        assert_ok!(AiPredictions::set_settlement_config(
            RuntimeOrigin::root(),
            true,
            10,
            25,
        ));

        assert_ok!(AiPredictions::authorize_validator(
            RuntimeOrigin::root(),
            ALICE,
        ));

        assert_ok!(AiPredictions::propose_prediction_outcome(
            RuntimeOrigin::signed(ALICE),
            token_id,
            SettlementOutcome::Correct,
            b"ipfs://phase5b-duplicate".to_vec(),
        ));

        assert_ok!(AiPredictions::admin_finalize_prediction_outcome(
            RuntimeOrigin::root(),
            token_id,
            SettlementOutcome::Correct,
            b"ipfs://phase5b-final-correct".to_vec(),
        ));

        assert_ok!(AiPredictions::claim_prediction_market_payout(
            RuntimeOrigin::signed(ALICE),
            token_id,
        ));

        assert_noop!(
            AiPredictions::claim_prediction_market_payout(RuntimeOrigin::signed(ALICE), token_id),
            Error::<crate::mock::Test>::MarketPayoutAlreadyClaimed
        );
    });
}

#[test]
fn phase5_cancelled_market_refunds_side_stake() {
    new_test_ext().execute_with(|| {
        create_phase3_token();
        let token_id = 0u64;
        let market_account = AiPredictions::market_account_id();

        assert_ok!(AiPredictions::set_staking_config(
            RuntimeOrigin::root(),
            true,
            1,
        ));

        assert_ok!(AiPredictions::set_market_economics_config(
            RuntimeOrigin::root(),
            true,
            true,
        ));

        let bob_free_before = Balances::free_balance(BOB);

        assert_ok!(AiPredictions::stake_on_prediction_outcome_side(
            RuntimeOrigin::signed(BOB),
            token_id,
            PredictionOutcomeSide::No,
            75,
        ));

        assert_eq!(Balances::free_balance(BOB), bob_free_before - 75);
        assert_eq!(Balances::free_balance(market_account), 75);

        assert_ok!(AiPredictions::set_settlement_config(
            RuntimeOrigin::root(),
            true,
            10,
            25,
        ));

        assert_ok!(AiPredictions::authorize_validator(
            RuntimeOrigin::root(),
            ALICE,
        ));

        assert_ok!(AiPredictions::propose_prediction_outcome(
            RuntimeOrigin::signed(ALICE),
            token_id,
            SettlementOutcome::Correct,
            b"ipfs://phase5b-cancel-start".to_vec(),
        ));

        assert_ok!(AiPredictions::cancel_prediction_settlement(
            RuntimeOrigin::root(),
            token_id,
            b"ipfs://phase5b-cancel".to_vec(),
        ));

        assert_ok!(AiPredictions::claim_prediction_market_payout(
            RuntimeOrigin::signed(BOB),
            token_id,
        ));

        assert_eq!(Balances::free_balance(BOB), bob_free_before);
        assert_eq!(Balances::free_balance(market_account), 0);

        let claim = AiPredictions::prediction_token_side_payout_claims(token_id, BOB)
            .expect("cancel refund claim must exist");
        assert_eq!(claim.stake_returned, 75);
        assert_eq!(claim.reward_paid, 0);
        assert_eq!(claim.slashed, 0);
    });
}

#[test]
fn phase5_incorrect_outcome_pays_no_side_winner() {
    new_test_ext().execute_with(|| {
        create_phase3_token();
        let token_id = 0u64;
        let market_account = AiPredictions::market_account_id();

        assert_ok!(AiPredictions::set_staking_config(
            RuntimeOrigin::root(),
            true,
            1,
        ));

        assert_ok!(AiPredictions::set_market_economics_config(
            RuntimeOrigin::root(),
            true,
            true,
        ));

        let alice_free_before = Balances::free_balance(ALICE);
        let bob_free_before = Balances::free_balance(BOB);

        assert_ok!(AiPredictions::stake_on_prediction_outcome_side(
            RuntimeOrigin::signed(ALICE),
            token_id,
            PredictionOutcomeSide::Yes,
            40,
        ));

        assert_ok!(AiPredictions::stake_on_prediction_outcome_side(
            RuntimeOrigin::signed(BOB),
            token_id,
            PredictionOutcomeSide::No,
            80,
        ));

        assert_eq!(Balances::free_balance(market_account), 120);

        assert_ok!(AiPredictions::set_settlement_config(
            RuntimeOrigin::root(),
            true,
            10,
            25,
        ));

        assert_ok!(AiPredictions::authorize_validator(
            RuntimeOrigin::root(),
            ALICE,
        ));

        assert_ok!(AiPredictions::propose_prediction_outcome(
            RuntimeOrigin::signed(ALICE),
            token_id,
            SettlementOutcome::Correct,
            b"ipfs://phase5b-incorrect-start".to_vec(),
        ));

        assert_ok!(AiPredictions::admin_finalize_prediction_outcome(
            RuntimeOrigin::root(),
            token_id,
            SettlementOutcome::Incorrect,
            b"ipfs://phase5b-final-incorrect".to_vec(),
        ));

        assert_ok!(AiPredictions::claim_prediction_market_payout(
            RuntimeOrigin::signed(BOB),
            token_id,
        ));

        assert_eq!(Balances::free_balance(ALICE), alice_free_before - 40);
        assert_eq!(Balances::free_balance(BOB), bob_free_before + 40);
        assert_eq!(Balances::free_balance(market_account), 0);

        let claim = AiPredictions::prediction_token_side_payout_claims(token_id, BOB)
            .expect("no-side winner claim must exist");
        assert_eq!(claim.stake_returned, 80);
        assert_eq!(claim.reward_paid, 40);
        assert_eq!(claim.slashed, 0);
    });
}

#[test]
fn phase5_claim_rejected_when_no_winning_side_stake_exists() {
    new_test_ext().execute_with(|| {
        create_phase3_token();
        let token_id = 0u64;

        assert_ok!(AiPredictions::set_staking_config(
            RuntimeOrigin::root(),
            true,
            1,
        ));

        assert_ok!(AiPredictions::set_market_economics_config(
            RuntimeOrigin::root(),
            true,
            true,
        ));

        assert_ok!(AiPredictions::stake_on_prediction_outcome_side(
            RuntimeOrigin::signed(BOB),
            token_id,
            PredictionOutcomeSide::No,
            50,
        ));

        assert_ok!(AiPredictions::set_settlement_config(
            RuntimeOrigin::root(),
            true,
            10,
            25,
        ));

        assert_ok!(AiPredictions::authorize_validator(
            RuntimeOrigin::root(),
            ALICE,
        ));

        assert_ok!(AiPredictions::propose_prediction_outcome(
            RuntimeOrigin::signed(ALICE),
            token_id,
            SettlementOutcome::Correct,
            b"ipfs://phase5b-no-winning-side".to_vec(),
        ));

        assert_ok!(AiPredictions::admin_finalize_prediction_outcome(
            RuntimeOrigin::root(),
            token_id,
            SettlementOutcome::Correct,
            b"ipfs://phase5b-final-correct".to_vec(),
        ));

        assert_noop!(
            AiPredictions::claim_prediction_market_payout(RuntimeOrigin::signed(BOB), token_id),
            Error::<crate::mock::Test>::MarketNoWinningStake
        );
    });
}

#[test]
fn phase5_large_18_decimal_payout_formula_does_not_overflow() {
    let winning_total: u128 = 100_000_000_000_000_000_000;
    let losing_total: u128 = 50_000_000_000_000_000_000;
    let user_winning_stake: u128 = 100_000_000_000_000_000_000;

    let winner_share = sp_runtime::Perquintill::from_rational(user_winning_stake, winning_total);
    let reward_paid = winner_share * losing_total;

    assert_eq!(reward_paid, 50_000_000_000_000_000_000);
}

#[test]
fn phase6_market_fee_config_defaults_are_safe() {
    new_test_ext().execute_with(|| {
        let config = PredictionMarketEconomicsConfig::<crate::mock::Test>::get();

        assert!(!config.market_enabled);
        assert!(config.allow_unstake_before_settlement);
        assert!(!config.fees_enabled);
        assert_eq!(config.fee_bps, 0);
        assert!(config.treasury_enabled);

        let accounting = PredictionTokenMarketAccounting::<crate::mock::Test>::get(0);
        assert_eq!(accounting.fees_collected, 0);
        assert_eq!(accounting.remainders_collected, 0);
        assert_eq!(accounting.total_paid, 0);
        assert_eq!(accounting.total_refunded, 0);
        assert_eq!(accounting.total_slashed, 0);
    });
}

#[test]
fn phase6_root_can_set_market_fee_config() {
    new_test_ext().execute_with(|| {
        assert_ok!(AiPredictions::set_market_fee_config(
            RuntimeOrigin::root(),
            true,
            250,
            true
        ));

        let config = PredictionMarketEconomicsConfig::<crate::mock::Test>::get();
        assert!(config.fees_enabled);
        assert_eq!(config.fee_bps, 250);
        assert!(config.treasury_enabled);
    });
}

#[test]
fn phase6_market_fee_bps_cannot_exceed_100_percent() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            AiPredictions::set_market_fee_config(RuntimeOrigin::root(), true, 10_001, true),
            Error::<crate::mock::Test>::MarketFeeBpsTooHigh
        );
    });
}

#[test]
fn phase6_winning_claim_deducts_fee_and_sends_to_treasury() {
    new_test_ext().execute_with(|| {
        create_phase3_token();
        let token_id = 0u64;

        assert_ok!(AiPredictions::set_staking_config(
            RuntimeOrigin::root(),
            true,
            1,
        ));

        assert_ok!(AiPredictions::set_market_economics_config(
            RuntimeOrigin::root(),
            true,
            true,
        ));

        assert_ok!(AiPredictions::set_market_fee_config(
            RuntimeOrigin::root(),
            true,
            1_000,
            true,
        ));

        assert_ok!(AiPredictions::stake_on_prediction_outcome_side(
            RuntimeOrigin::signed(ALICE),
            token_id,
            PredictionOutcomeSide::Yes,
            100,
        ));

        assert_ok!(AiPredictions::stake_on_prediction_outcome_side(
            RuntimeOrigin::signed(BOB),
            token_id,
            PredictionOutcomeSide::No,
            50,
        ));

        assert_ok!(AiPredictions::set_settlement_config(
            RuntimeOrigin::root(),
            true,
            10,
            25,
        ));

        assert_ok!(AiPredictions::authorize_validator(
            RuntimeOrigin::root(),
            ALICE,
        ));

        assert_ok!(AiPredictions::propose_prediction_outcome(
            RuntimeOrigin::signed(ALICE),
            token_id,
            SettlementOutcome::Correct,
            b"ipfs://phase6-fee-correct".to_vec(),
        ));

        assert_ok!(AiPredictions::admin_finalize_prediction_outcome(
            RuntimeOrigin::root(),
            token_id,
            SettlementOutcome::Correct,
            b"ipfs://phase6-fee-final".to_vec(),
        ));

        let alice_free_before_claim = Balances::free_balance(ALICE);

        assert_ok!(AiPredictions::claim_prediction_market_payout(
            RuntimeOrigin::signed(ALICE),
            token_id,
        ));

        let claim = AiPredictions::prediction_token_side_payout_claims(token_id, ALICE)
            .expect("winner claim must exist");

        assert_eq!(claim.stake_returned, 100);
        assert_eq!(claim.reward_paid, 45);
        assert_eq!(claim.slashed, 0);

        assert_eq!(Balances::free_balance(ALICE), alice_free_before_claim + 145);

        let accounting = PredictionTokenMarketAccounting::<crate::mock::Test>::get(token_id);
        assert_eq!(accounting.fees_collected, 5);
        assert_eq!(accounting.remainders_collected, 0);
        assert_eq!(accounting.total_paid, 145);
        assert_eq!(accounting.total_refunded, 0);
        assert_eq!(accounting.total_slashed, 0);

        assert_ok!(AiPredictions::claim_prediction_market_payout(
            RuntimeOrigin::signed(BOB),
            token_id,
        ));

        let accounting_after_loser =
            PredictionTokenMarketAccounting::<crate::mock::Test>::get(token_id);
        assert_eq!(accounting_after_loser.fees_collected, 5);
        assert_eq!(accounting_after_loser.remainders_collected, 0);
        assert_eq!(accounting_after_loser.total_paid, 145);
        assert_eq!(accounting_after_loser.total_refunded, 0);
        assert_eq!(accounting_after_loser.total_slashed, 50);
    });
}

#[test]
fn phase6_multiple_winners_share_fee_adjusted_losing_pool() {
    new_test_ext().execute_with(|| {
        create_phase3_token();
        let token_id = 0u64;

        assert_ok!(AiPredictions::set_staking_config(
            RuntimeOrigin::root(),
            true,
            1,
        ));

        assert_ok!(AiPredictions::set_market_economics_config(
            RuntimeOrigin::root(),
            true,
            true,
        ));

        assert_ok!(AiPredictions::set_market_fee_config(
            RuntimeOrigin::root(),
            true,
            1_000,
            true,
        ));

        assert_ok!(AiPredictions::stake_on_prediction_outcome_side(
            RuntimeOrigin::signed(ALICE),
            token_id,
            PredictionOutcomeSide::Yes,
            60,
        ));

        assert_ok!(AiPredictions::stake_on_prediction_outcome_side(
            RuntimeOrigin::signed(VALIDATOR),
            token_id,
            PredictionOutcomeSide::Yes,
            40,
        ));

        assert_ok!(AiPredictions::stake_on_prediction_outcome_side(
            RuntimeOrigin::signed(BOB),
            token_id,
            PredictionOutcomeSide::No,
            50,
        ));

        assert_ok!(AiPredictions::set_settlement_config(
            RuntimeOrigin::root(),
            true,
            10,
            25,
        ));

        assert_ok!(AiPredictions::authorize_validator(
            RuntimeOrigin::root(),
            ALICE,
        ));

        assert_ok!(AiPredictions::propose_prediction_outcome(
            RuntimeOrigin::signed(ALICE),
            token_id,
            SettlementOutcome::Correct,
            b"ipfs://phase6-multi-winner-correct".to_vec(),
        ));

        assert_ok!(AiPredictions::admin_finalize_prediction_outcome(
            RuntimeOrigin::root(),
            token_id,
            SettlementOutcome::Correct,
            b"ipfs://phase6-multi-winner-final".to_vec(),
        ));

        let alice_before = Balances::free_balance(ALICE);
        let validator_before = Balances::free_balance(VALIDATOR);

        assert_ok!(AiPredictions::claim_prediction_market_payout(
            RuntimeOrigin::signed(ALICE),
            token_id,
        ));

        let alice_claim = AiPredictions::prediction_token_side_payout_claims(token_id, ALICE)
            .expect("alice claim must exist");
        assert_eq!(alice_claim.stake_returned, 60);
        assert_eq!(alice_claim.reward_paid, 27);
        assert_eq!(alice_claim.slashed, 0);
        assert_eq!(Balances::free_balance(ALICE), alice_before + 87);

        assert_ok!(AiPredictions::claim_prediction_market_payout(
            RuntimeOrigin::signed(VALIDATOR),
            token_id,
        ));

        let validator_claim =
            AiPredictions::prediction_token_side_payout_claims(token_id, VALIDATOR)
                .expect("validator winner claim must exist");
        assert_eq!(validator_claim.stake_returned, 40);
        assert_eq!(validator_claim.reward_paid, 18);
        assert_eq!(validator_claim.slashed, 0);
        assert_eq!(Balances::free_balance(VALIDATOR), validator_before + 58);

        assert_ok!(AiPredictions::claim_prediction_market_payout(
            RuntimeOrigin::signed(BOB),
            token_id,
        ));

        let bob_claim = AiPredictions::prediction_token_side_payout_claims(token_id, BOB)
            .expect("bob loser claim must exist");
        assert_eq!(bob_claim.stake_returned, 0);
        assert_eq!(bob_claim.reward_paid, 0);
        assert_eq!(bob_claim.slashed, 50);

        let accounting = PredictionTokenMarketAccounting::<crate::mock::Test>::get(token_id);
        assert_eq!(accounting.fees_collected, 5);
        assert_eq!(accounting.remainders_collected, 0);
        assert_eq!(accounting.total_paid, 145);
        assert_eq!(accounting.total_refunded, 0);
        assert_eq!(accounting.total_slashed, 50);

        let conservation_total = accounting
            .total_paid
            .saturating_add(accounting.fees_collected)
            .saturating_add(accounting.remainders_collected);

        assert_eq!(conservation_total, 150);
    });
}

#[test]
fn phase6_cancelled_market_records_refund_accounting() {
    new_test_ext().execute_with(|| {
        create_phase3_token();
        let token_id = 0u64;

        assert_ok!(AiPredictions::set_staking_config(
            RuntimeOrigin::root(),
            true,
            1,
        ));

        assert_ok!(AiPredictions::set_market_economics_config(
            RuntimeOrigin::root(),
            true,
            true,
        ));

        assert_ok!(AiPredictions::stake_on_prediction_outcome_side(
            RuntimeOrigin::signed(ALICE),
            token_id,
            PredictionOutcomeSide::Yes,
            70,
        ));

        assert_ok!(AiPredictions::stake_on_prediction_outcome_side(
            RuntimeOrigin::signed(BOB),
            token_id,
            PredictionOutcomeSide::No,
            30,
        ));

        assert_ok!(AiPredictions::set_settlement_config(
            RuntimeOrigin::root(),
            true,
            10,
            25,
        ));

        assert_ok!(AiPredictions::authorize_validator(
            RuntimeOrigin::root(),
            ALICE,
        ));

        assert_ok!(AiPredictions::propose_prediction_outcome(
            RuntimeOrigin::signed(ALICE),
            token_id,
            SettlementOutcome::Correct,
            b"ipfs://phase6-cancel-proposed".to_vec(),
        ));

        assert_ok!(AiPredictions::cancel_prediction_settlement(
            RuntimeOrigin::root(),
            token_id,
            b"ipfs://phase6-cancelled".to_vec(),
        ));

        let alice_before = Balances::free_balance(ALICE);
        let bob_before = Balances::free_balance(BOB);

        assert_ok!(AiPredictions::claim_prediction_market_payout(
            RuntimeOrigin::signed(ALICE),
            token_id,
        ));

        assert_ok!(AiPredictions::claim_prediction_market_payout(
            RuntimeOrigin::signed(BOB),
            token_id,
        ));

        assert_eq!(Balances::free_balance(ALICE), alice_before + 70);
        assert_eq!(Balances::free_balance(BOB), bob_before + 30);

        let alice_claim = AiPredictions::prediction_token_side_payout_claims(token_id, ALICE)
            .expect("alice refund claim must exist");
        assert_eq!(alice_claim.stake_returned, 70);
        assert_eq!(alice_claim.reward_paid, 0);
        assert_eq!(alice_claim.slashed, 0);

        let bob_claim = AiPredictions::prediction_token_side_payout_claims(token_id, BOB)
            .expect("bob refund claim must exist");
        assert_eq!(bob_claim.stake_returned, 30);
        assert_eq!(bob_claim.reward_paid, 0);
        assert_eq!(bob_claim.slashed, 0);

        let accounting = PredictionTokenMarketAccounting::<crate::mock::Test>::get(token_id);
        assert_eq!(accounting.fees_collected, 0);
        assert_eq!(accounting.remainders_collected, 0);
        assert_eq!(accounting.total_paid, 0);
        assert_eq!(accounting.total_refunded, 100);
        assert_eq!(accounting.total_slashed, 0);
    });
}

#[test]
fn phase6_rounding_remainder_is_not_lost_in_market_account() {
    new_test_ext().execute_with(|| {
        create_phase3_token();
        let token_id = 0u64;

        assert_ok!(AiPredictions::set_staking_config(
            RuntimeOrigin::root(),
            true,
            1,
        ));

        assert_ok!(AiPredictions::set_market_economics_config(
            RuntimeOrigin::root(),
            true,
            true,
        ));

        assert_ok!(AiPredictions::set_market_fee_config(
            RuntimeOrigin::root(),
            true,
            100,
            true,
        ));

        assert_ok!(AiPredictions::stake_on_prediction_outcome_side(
            RuntimeOrigin::signed(ALICE),
            token_id,
            PredictionOutcomeSide::Yes,
            1,
        ));

        assert_ok!(AiPredictions::stake_on_prediction_outcome_side(
            RuntimeOrigin::signed(VALIDATOR),
            token_id,
            PredictionOutcomeSide::Yes,
            2,
        ));

        assert_ok!(AiPredictions::stake_on_prediction_outcome_side(
            RuntimeOrigin::signed(BOB),
            token_id,
            PredictionOutcomeSide::No,
            100,
        ));

        assert_ok!(AiPredictions::set_settlement_config(
            RuntimeOrigin::root(),
            true,
            10,
            25,
        ));

        assert_ok!(AiPredictions::authorize_validator(
            RuntimeOrigin::root(),
            ALICE,
        ));

        assert_ok!(AiPredictions::propose_prediction_outcome(
            RuntimeOrigin::signed(ALICE),
            token_id,
            SettlementOutcome::Correct,
            b"ipfs://phase6-rounding-correct".to_vec(),
        ));

        assert_ok!(AiPredictions::admin_finalize_prediction_outcome(
            RuntimeOrigin::root(),
            token_id,
            SettlementOutcome::Correct,
            b"ipfs://phase6-rounding-final".to_vec(),
        ));

        assert_ok!(AiPredictions::claim_prediction_market_payout(
            RuntimeOrigin::signed(ALICE),
            token_id,
        ));

        assert_ok!(AiPredictions::claim_prediction_market_payout(
            RuntimeOrigin::signed(VALIDATOR),
            token_id,
        ));

        assert_ok!(AiPredictions::claim_prediction_market_payout(
            RuntimeOrigin::signed(BOB),
            token_id,
        ));

        let accounting = PredictionTokenMarketAccounting::<crate::mock::Test>::get(token_id);

        let accounted_total = accounting
            .total_paid
            .saturating_add(accounting.fees_collected)
            .saturating_add(accounting.remainders_collected);

        assert_eq!(accounting.fees_collected, 1);
        assert_eq!(accounting.remainders_collected, 0);
        assert_eq!(accounting.total_paid, 102);
        assert_eq!(accounting.total_refunded, 0);
        assert_eq!(accounting.total_slashed, 100);
        assert_eq!(accounted_total, 103);
    });
}

#[test]
fn phase6_market_config_migration_preserves_phase5_flags() {
    new_test_ext().execute_with(|| {
        let old_config = crate::PredictionMarketEconomicsSettingsV1 {
            market_enabled: true,
            allow_unstake_before_settlement: false,
        };

        let key = PredictionMarketEconomicsConfig::<crate::mock::Test>::hashed_key();
        frame_support::storage::unhashed::put(&key, &old_config);

        frame_support::traits::StorageVersion::new(0).put::<AiPredictions>();

        AiPredictions::migrate_market_economics_config_v1_to_v2();

        let migrated = PredictionMarketEconomicsConfig::<crate::mock::Test>::get();

        assert!(migrated.market_enabled);
        assert!(!migrated.allow_unstake_before_settlement);
        assert!(!migrated.fees_enabled);
        assert_eq!(migrated.fee_bps, 0);
        assert!(migrated.treasury_enabled);

        assert_eq!(
            frame_support::traits::StorageVersion::get::<AiPredictions>(),
            frame_support::traits::StorageVersion::new(3)
        );
    });
}

#[test]
fn phase6b_registered_model_starts_pending_and_requires_approval() {
    new_test_ext().execute_with(|| {
        assert_ok!(AiPredictions::set_model_onboarding_config(
            RuntimeOrigin::root(),
            ModelOnboardingSettings::default()
        ));
        assert_ok!(AiPredictions::authorize_validator(
            RuntimeOrigin::root(),
            ALICE
        ));
        register_default_model();
        assert_eq!(
            AiPredictions::model_governance(0).unwrap().status,
            ModelStatus::Pending
        );
        assert_noop!(
            AiPredictions::submit_prediction(
                RuntimeOrigin::signed(ALICE),
                0,
                PredictionDomain::Finance,
                category_code(),
                prediction_hash(),
                b"ipfs://prediction-001".to_vec(),
                85,
                100
            ),
            Error::<crate::mock::Test>::ModelNotApproved
        );
        assert_ok!(AiPredictions::approve_model(RuntimeOrigin::root(), 0));
        assert_ok!(AiPredictions::submit_prediction(
            RuntimeOrigin::signed(ALICE),
            0,
            PredictionDomain::Finance,
            category_code(),
            prediction_hash(),
            b"ipfs://prediction-001".to_vec(),
            85,
            100
        ));
    });
}

#[test]
fn phase6b_normal_user_cannot_approve_model() {
    new_test_ext().execute_with(|| {
        register_default_model();
        assert_noop!(
            AiPredictions::approve_model(RuntimeOrigin::signed(BOB), 0),
            Error::<crate::mock::Test>::NotAuthorizedModelAdmin
        );
    });
}

#[test]
fn phase6b_suspended_and_rejected_models_cannot_submit() {
    new_test_ext().execute_with(|| {
        register_default_model();
        assert_ok!(AiPredictions::suspend_model(RuntimeOrigin::root(), 0));
        assert_noop!(
            AiPredictions::submit_prediction(
                RuntimeOrigin::signed(ALICE),
                0,
                PredictionDomain::Finance,
                category_code(),
                prediction_hash(),
                b"ipfs://p".to_vec(),
                80,
                100
            ),
            Error::<crate::mock::Test>::ModelSuspended
        );
        assert_ok!(AiPredictions::reject_model(RuntimeOrigin::root(), 0));
        assert_noop!(
            AiPredictions::submit_prediction(
                RuntimeOrigin::signed(ALICE),
                0,
                PredictionDomain::Finance,
                category_code(),
                prediction_hash(),
                b"ipfs://p".to_vec(),
                80,
                100
            ),
            Error::<crate::mock::Test>::ModelRejected
        );
    });
}

#[test]
fn phase6b_authorized_submitter_and_indexes_work() {
    new_test_ext().execute_with(|| {
        register_default_model();
        assert_ok!(AiPredictions::set_model_submitter(
            RuntimeOrigin::signed(ALICE),
            0,
            Some(BOB)
        ));
        assert_ok!(AiPredictions::submit_prediction(
            RuntimeOrigin::signed(BOB),
            0,
            PredictionDomain::Finance,
            category_code(),
            prediction_hash(),
            b"ipfs://p".to_vec(),
            80,
            100
        ));
        assert_eq!(AiPredictions::model_ids_by_owner(ALICE).as_slice(), &[0]);
        assert_eq!(AiPredictions::prediction_ids_by_model(0).as_slice(), &[0]);
        assert_eq!(AiPredictions::active_prediction_count_by_model(0), 1);
    });
}

#[test]
fn phase6b_public_registration_is_controlled() {
    new_test_ext().execute_with(|| {
        assert_ok!(AiPredictions::set_model_onboarding_config(
            RuntimeOrigin::root(),
            ModelOnboardingSettings::default()
        ));
        assert_noop!(
            AiPredictions::register_model(
                RuntimeOrigin::signed(ALICE),
                model_hash(),
                metadata_uri()
            ),
            Error::<crate::mock::Test>::PublicModelRegistrationDisabled
        );
        assert_ok!(AiPredictions::authorize_validator(
            RuntimeOrigin::root(),
            ALICE
        ));
        assert_ok!(AiPredictions::register_model(
            RuntimeOrigin::signed(ALICE),
            model_hash(),
            metadata_uri()
        ));
    });
}

fn seed_phase6b_v2_model(model_id: u64, owner: u64, active: bool) {
    Models::<crate::mock::Test>::insert(
        model_id,
        AiModel {
            owner,
            model_hash: model_hash().try_into().unwrap(),
            metadata_uri: metadata_uri().try_into().unwrap(),
            registered_at: 1,
            updated_at: None,
            active,
        },
    );
    ModelStatsById::<crate::mock::Test>::insert(model_id, ModelStats::default());
    NextModelId::<crate::mock::Test>::put(model_id + 1);
}

fn seed_phase6b_v2_prediction(prediction_id: u64, model_id: u64, status: PredictionStatus) {
    Predictions::<crate::mock::Test>::insert(
        prediction_id,
        Prediction {
            model_id,
            submitter: ALICE,
            domain: PredictionDomain::Finance,
            category_code: category_code().try_into().unwrap(),
            prediction_hash: prediction_hash().try_into().unwrap(),
            metadata_uri: b"ipfs://legacy-prediction".to_vec().try_into().unwrap(),
            confidence: 80,
            created_at: 1,
            expires_at: 100,
            status,
            outcome: None,
            validator: None,
            validated_at: None,
        },
    );
    NextPredictionId::<crate::mock::Test>::put(prediction_id + 1);
}

#[test]
fn migration_v2_to_v3_preserves_phase6a_config_and_does_not_decode_v2_as_v1() {
    new_test_ext().execute_with(|| {
        let config = PredictionMarketEconomicsSettings {
            market_enabled: true,
            allow_unstake_before_settlement: false,
            fees_enabled: true,
            fee_bps: 125,
            treasury_enabled: false,
        };
        PredictionMarketEconomicsConfig::<crate::mock::Test>::put(config.clone());
        frame_support::traits::StorageVersion::new(2).put::<AiPredictions>();
        AiPredictions::migrate_market_economics_config_v1_to_v2();
        assert_eq!(
            PredictionMarketEconomicsConfig::<crate::mock::Test>::get(),
            config
        );
        assert_eq!(
            frame_support::traits::StorageVersion::get::<AiPredictions>(),
            frame_support::traits::StorageVersion::new(3)
        );
    });
}

#[test]
fn migration_v2_to_v3_adds_governance_and_discovery_indexes() {
    new_test_ext().execute_with(|| {
        seed_phase6b_v2_model(0, ALICE, true);
        seed_phase6b_v2_model(1, BOB, false);
        seed_phase6b_v2_prediction(0, 0, PredictionStatus::Open);
        seed_phase6b_v2_prediction(1, 0, PredictionStatus::Closed);
        frame_support::traits::StorageVersion::new(2).put::<AiPredictions>();
        AiPredictions::migrate_market_economics_config_v1_to_v2();
        assert_eq!(
            AiPredictions::model_governance(0).unwrap().status,
            ModelStatus::Approved
        );
        assert_eq!(
            AiPredictions::model_governance(1).unwrap().status,
            ModelStatus::Suspended
        );
        assert_eq!(AiPredictions::model_ids_by_owner(ALICE).as_slice(), &[0]);
        assert_eq!(AiPredictions::model_ids_by_owner(BOB).as_slice(), &[1]);
        assert_eq!(
            AiPredictions::prediction_ids_by_model(0).as_slice(),
            &[0, 1]
        );
        assert_eq!(AiPredictions::active_prediction_count_by_model(0), 1);
    });
}

#[test]
fn migration_v2_to_v3_preserves_accounting_positions_and_claims() {
    new_test_ext().execute_with(|| {
        let accounting = PredictionMarketAccounting {
            fees_collected: 3,
            remainders_collected: 2,
            total_paid: 90,
            total_refunded: 4,
            total_slashed: 10,
        };
        PredictionTokenMarketAccounting::<crate::mock::Test>::insert(7, accounting.clone());
        let position = PredictionSideStake {
            side: PredictionOutcomeSide::Yes,
            amount: 50,
            staked_at: 1,
            updated_at: None,
        };
        PredictionTokenSideStakes::<crate::mock::Test>::insert(7, ALICE, position.clone());
        let claim = PredictionSidePayoutClaim {
            claimed_at: 2,
            stake_returned: 50,
            reward_paid: 10,
            slashed: 0,
        };
        PredictionTokenSidePayoutClaims::<crate::mock::Test>::insert(7, ALICE, claim.clone());
        frame_support::traits::StorageVersion::new(2).put::<AiPredictions>();
        AiPredictions::migrate_market_economics_config_v1_to_v2();
        assert_eq!(
            PredictionTokenMarketAccounting::<crate::mock::Test>::get(7),
            accounting
        );
        assert_eq!(
            PredictionTokenSideStakes::<crate::mock::Test>::get(7, ALICE),
            Some(position)
        );
        assert_eq!(
            PredictionTokenSidePayoutClaims::<crate::mock::Test>::get(7, ALICE),
            Some(claim)
        );
    });
}

#[test]
fn migration_handles_bounded_model_index_overflow_safely() {
    new_test_ext().execute_with(|| {
        for model_id in 0..65 {
            seed_phase6b_v2_model(model_id, ALICE, true);
        }
        frame_support::traits::StorageVersion::new(2).put::<AiPredictions>();
        AiPredictions::migrate_market_economics_config_v1_to_v2();
        assert_eq!(AiPredictions::model_ids_by_owner(ALICE).len(), 64);
        assert!(AiPredictions::models(64).is_some());
        assert_eq!(
            AiPredictions::model_governance(64).unwrap().status,
            ModelStatus::Approved
        );
    });
}

#[test]
fn phase6b_call_indices_are_stable() {
    use codec::Encode;
    assert_eq!(
        crate::Call::<crate::mock::Test>::register_model {
            model_hash: vec![1],
            metadata_uri: vec![1]
        }
        .encode()[0],
        0
    );
    assert_eq!(
        crate::Call::<crate::mock::Test>::set_market_fee_config {
            fees_enabled: false,
            fee_bps: 0,
            treasury_enabled: true
        }
        .encode()[0],
        34
    );
    assert_eq!(
        crate::Call::<crate::mock::Test>::set_model_onboarding_config {
            config: ModelOnboardingSettings::default()
        }
        .encode()[0],
        35
    );
    assert_eq!(
        crate::Call::<crate::mock::Test>::approve_model { model_id: 0 }.encode()[0],
        36
    );
    assert_eq!(
        crate::Call::<crate::mock::Test>::suspend_model { model_id: 0 }.encode()[0],
        37
    );
    assert_eq!(
        crate::Call::<crate::mock::Test>::reject_model { model_id: 0 }.encode()[0],
        38
    );
    assert_eq!(
        crate::Call::<crate::mock::Test>::set_model_submitter {
            model_id: 0,
            submitter: Some(ALICE)
        }
        .encode()[0],
        39
    );
}

#[test]
fn phase6b_events_and_errors_are_scale_exposed() {
    use codec::Encode;
    let events = [
        Event::<crate::mock::Test>::ModelApproved { model_id: 0 }.encode(),
        Event::<crate::mock::Test>::ModelSuspended { model_id: 0 }.encode(),
        Event::<crate::mock::Test>::ModelRejected { model_id: 0 }.encode(),
        Event::<crate::mock::Test>::ModelSubmitterUpdated {
            model_id: 0,
            submitter: Some(ALICE),
        }
        .encode(),
        Event::<crate::mock::Test>::PredictionOpened {
            prediction_id: 0,
            model_id: 0,
        }
        .encode(),
    ];
    assert!(events.iter().all(|encoded| !encoded.is_empty()));
    let errors = [
        Error::<crate::mock::Test>::ModelNotApproved.encode(),
        Error::<crate::mock::Test>::ModelSuspended.encode(),
        Error::<crate::mock::Test>::ModelRejected.encode(),
        Error::<crate::mock::Test>::NotAuthorizedModelAdmin.encode(),
        Error::<crate::mock::Test>::NotAuthorizedModelSubmitter.encode(),
        Error::<crate::mock::Test>::PredictionNotOpen.encode(),
    ];
    assert!(errors.iter().all(|encoded| !encoded.is_empty()));
}

#[cfg(feature = "try-runtime")]
#[test]
fn phase6b_try_runtime_pre_and_post_checks_pass_for_v2_state() {
    use frame_support::traits::Hooks;
    new_test_ext().execute_with(|| {
        seed_phase6b_v2_model(0, ALICE, true);
        seed_phase6b_v2_prediction(0, 0, PredictionStatus::Open);
        PredictionMarketEconomicsConfig::<crate::mock::Test>::put(
            PredictionMarketEconomicsSettings {
                market_enabled: true,
                allow_unstake_before_settlement: false,
                fees_enabled: true,
                fee_bps: 100,
                treasury_enabled: true,
            },
        );
        frame_support::traits::StorageVersion::new(2).put::<AiPredictions>();
        let state = <AiPredictions as Hooks<u64>>::pre_upgrade().expect("pre-upgrade checks");
        AiPredictions::migrate_market_economics_config_v1_to_v2();
        <AiPredictions as Hooks<u64>>::post_upgrade(state).expect("post-upgrade checks");
    });
}
