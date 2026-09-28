//! Actual runtime policy and append-only evaluation tests; no production identities/funds.
use super::*;
use pallet_ai_predictions as ai;
use frame_support::{assert_ok, assert_noop};
use sp_runtime::BuildStorage;
fn who(n:u8)->AccountId { AccountId::new([n;32]) }
fn signed(n:u8)->RuntimeOrigin { RuntimeOrigin::signed(who(n)) }
fn ext()->sp_io::TestExternalities {
    AiFinancialModesAllowed::set(false);
    let mut s=frame_system::GenesisConfig::<Runtime>::default().build_storage().unwrap();
    pallet_balances::GenesisConfig::<Runtime>{balances:(1..=5).map(|n|(who(n),1000*DECIMALS)).collect(),dev_accounts:None}.assimilate_storage(&mut s).unwrap();
    let mut ext=sp_io::TestExternalities::new(s);
    ext.execute_with(||{ System::set_block_number(10); pallet_timestamp::Now::<Runtime>::put(100_000); }); ext
}
fn setup() {
    assert_ok!(AiPredictions::authorize_validator(RuntimeOrigin::root(),who(1)));
    assert_ok!(AiPredictions::authorize_validator(RuntimeOrigin::root(),who(3)));
    assert_ok!(AiPredictions::register_model(signed(1),vec![7;32],b"fixture://reference-model-v1".to_vec()));
    assert_ok!(AiPredictions::approve_model(signed(3),0));
    assert_ok!(AiPredictions::set_model_submitter(signed(1),0,Some(who(2))));
    let mut config=ai::ModelOnboardingConfig::<Runtime>::get();config.market_creation_enabled=false;
    ai::ModelOnboardingConfig::<Runtime>::put(config);
}
fn submit()->sp_runtime::DispatchResult { AiPredictions::submit_evaluation(signed(2),0,[1;32],[7;32],[8;32],b"fixture://envelope".to_vec(),50,100,200,300) }
#[test]
fn evaluation_nonfinancial_lifecycle_retries_corrections_and_no_money() {ext().execute_with(||{
    setup();let before=Balances::total_issuance();let balances:Vec<_>=(1..=5).map(|n|Balances::free_balance(who(n))).collect();
    assert_ok!(submit());assert_noop!(AiPredictions::close_prediction(signed(2),0),ai::Error::<Runtime>::EvaluationEvidenceRequired);assert_ok!(submit());assert_eq!(ai::NextPredictionId::<Runtime>::get(),1);
    assert_eq!(ai::EvaluationRequests::<Runtime>::get(0,[1;32]),Some(0));
    assert_noop!(AiPredictions::submit_evaluation(signed(2),0,[1;32],[7;32],[9;32],b"fixture://envelope".to_vec(),50,100,200,300),ai::Error::<Runtime>::EvaluationRequestConflict);
    assert_noop!(AiPredictions::validate_prediction(signed(3),0,ai::PredictionOutcome::Successful),ai::Error::<Runtime>::EvaluationEvidenceRequired);
    assert_noop!(AiPredictions::record_evaluation_evidence(signed(3),0,0,[9;32],ai::PredictionOutcome::Successful),ai::Error::<Runtime>::EvaluationTooEarly);
    pallet_timestamp::Now::<Runtime>::put(300_000);
    assert_noop!(AiPredictions::record_evaluation_evidence(signed(1),0,0,[9;32],ai::PredictionOutcome::Successful),ai::Error::<Runtime>::EvaluationReviewerConflict);
    assert_ok!(AiPredictions::record_evaluation_evidence(signed(3),0,0,[9;32],ai::PredictionOutcome::Failed));
    assert_noop!(AiPredictions::record_evaluation_evidence(signed(3),0,0,[9;32],ai::PredictionOutcome::Failed),ai::Error::<Runtime>::EvaluationRevisionConflict);
    assert_ok!(AiPredictions::record_evaluation_evidence(signed(3),0,1,[10;32],ai::PredictionOutcome::Inconclusive));
    assert_eq!(ai::EvaluationEvidence::<Runtime>::get(0,0).unwrap().2,ai::PredictionOutcome::Failed);
    assert_eq!(ai::EvaluationEvidence::<Runtime>::get(0,1).unwrap().2,ai::PredictionOutcome::Inconclusive);
    assert_eq!(ai::NextPredictionTokenId::<Runtime>::get(),0);
    assert_eq!(Balances::total_issuance(),before);
    assert_eq!((1..=5).map(|n|Balances::free_balance(who(n))).collect::<Vec<_>>(),balances);
});}
#[test]
fn evaluation_requires_current_approval_and_enforces_cutoff() {ext().execute_with(||{
    setup();assert_ok!(submit());
    assert_ok!(AiPredictions::update_model(signed(1),0,vec![6;32],b"fixture://version2".to_vec()));
    assert_eq!(ai::EvaluationBindings::<Runtime>::get(0),Some([7;32]));
    assert_eq!(ai::ModelGovernanceById::<Runtime>::get(0).unwrap().status,ai::ModelStatus::Pending);
    assert_noop!(AiPredictions::submit_evaluation(signed(2),0,[2;32],[6;32],[9;32],b"fixture://v2".to_vec(),50,100,200,300),ai::Error::<Runtime>::ModelNotApproved);
    pallet_timestamp::Now::<Runtime>::put(200_000);
    assert_noop!(AiPredictions::submit_evaluation(signed(2),0,[2;32],[6;32],[9;32],b"fixture://v2".to_vec(),50,100,200,300),ai::Error::<Runtime>::EvaluationTooEarly);
    assert_ok!(submit()); // finalized business retry remains readable after cutoff/model change.
});}
#[test]
fn evaluation_financial_paths_refuse_even_with_stale_enabled_storage_and_root() {ext().execute_with(||{
    setup();
    ai::TokenizationConfig::<Runtime>::put(ai::TokenizationSettings{tokenization_enabled:true,transfers_enabled:true});
    ai::StakingConfig::<Runtime>::put(ai::StakingSettings{staking_enabled:true,min_stake:0});
    assert_noop!(AiPredictions::tokenize_prediction(signed(2),0,b"fixture://token".to_vec(),true),ai::Error::<Runtime>::FinancialModeDisabled);
    assert_noop!(AiPredictions::approve_prediction_token_transfer(signed(2),0,who(4)),ai::Error::<Runtime>::FinancialModeDisabled);
    assert_noop!(AiPredictions::transfer_prediction_token(signed(4),0,who(5)),ai::Error::<Runtime>::FinancialModeDisabled);
    assert_noop!(AiPredictions::stake_on_prediction_token(signed(2),0,1),ai::Error::<Runtime>::FinancialModeDisabled);
    assert_noop!(AiPredictions::stake_on_prediction_outcome_side(signed(2),0,ai::PredictionOutcomeSide::Yes,1),ai::Error::<Runtime>::FinancialModeDisabled);
    assert_noop!(AiPredictions::dispute_prediction_outcome(signed(2),0,ai::DisputeReason::Other,b"fixture://e".to_vec(),1),ai::Error::<Runtime>::FinancialModeDisabled);
    assert_noop!(AiPredictions::set_tokenization_config(RuntimeOrigin::root(),true,true),ai::Error::<Runtime>::FinancialModeDisabled);
    assert_noop!(AiPredictions::set_staking_config(RuntimeOrigin::root(),true,0),ai::Error::<Runtime>::FinancialModeDisabled);
    // Bypass dispatch still reaches the pallet gate; Root cannot enable it.
    use sp_runtime::traits::Dispatchable;
    use frame_support::traits::UnfilteredDispatchable;
    let c=RuntimeCall::AiPredictions(ai::Call::stake_on_prediction_token{token_id:0,amount:1});
    assert!(c.clone().dispatch_bypass_filter(signed(2)).is_err());
    assert!(c.dispatch(RuntimeOrigin::root()).is_err());
});}
#[test]
fn evaluation_preserves_legacy_passive_refund_when_modes_disabled() {ext().execute_with(||{
    setup();AiFinancialModesAllowed::set(true);
    assert_ok!(AiPredictions::set_tokenization_config(RuntimeOrigin::root(),true,true));
    assert_ok!(AiPredictions::set_staking_config(RuntimeOrigin::root(),true,1));
    let mut cfg=ai::ModelOnboardingConfig::<Runtime>::get();cfg.market_creation_enabled=true;ai::ModelOnboardingConfig::<Runtime>::put(cfg);
    assert_ok!(AiPredictions::submit_prediction(signed(2),0,ai::PredictionDomain::Other,b"old".to_vec(),vec![8;32],b"fixture://old".to_vec(),50,100));
    assert_ok!(AiPredictions::tokenize_prediction(signed(2),0,b"fixture://token".to_vec(),false));
    assert_ok!(AiPredictions::stake_on_prediction_token(signed(4),0,20*DECIMALS));
    AiFinancialModesAllowed::set(false);
    assert_ok!(AiPredictions::set_staking_config(RuntimeOrigin::root(),false,1));
    assert_ok!(AiPredictions::cancel_prediction_settlement(RuntimeOrigin::root(),0,b"fixture://recovery".to_vec()));
    assert_ok!(AiPredictions::claim_prediction_token_settlement(signed(4),0));
    assert_eq!(Balances::reserved_balance(who(4)),0);assert_eq!(Balances::free_balance(who(4)),1000*DECIMALS);
});}
#[test]
fn evaluation_export_profile_and_python_scale_vectors() {
    AiFinancialModesAllowed::set(false);
    use codec::{Encode,Decode};
    let bytes=std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"),"/../tools/ai-evaluation/fixtures/calls.json")).unwrap();
    let vectors:serde_json::Value=serde_json::from_slice(&bytes).unwrap();
    for v in vectors.as_array().unwrap() {
        let encoded=v["call"].as_str().unwrap().trim_start_matches("0x");
        let raw:Vec<u8>=(0..encoded.len()).step_by(2).map(|i|u8::from_str_radix(&encoded[i..i+2],16).unwrap()).collect();
        let mut input=raw.as_slice();let call=RuntimeCall::decode(&mut input).unwrap();assert!(input.is_empty());assert_eq!(call.encode(),raw);
        match call { RuntimeCall::AiPredictions(ai::Call::submit_evaluation{model_id,cutoff_utc,evidence_after_utc,..})=>{assert_eq!(model_id,0);assert!(cutoff_utc<evidence_after_utc);}, RuntimeCall::AiPredictions(ai::Call::record_evaluation_evidence{revision,..})=>assert_eq!(revision,0), _=>panic!("unexpected call") }
    }
    if let Ok(path)=std::env::var("ERA_V14_EVALUATION_PROFILE") {
        let hex=|bytes:&[u8]|bytes.iter().map(|b|format!("{b:02x}")).collect::<String>();
        let metadata=Runtime::metadata().encode();
        let profile=serde_json::json!({"specVersion":VERSION.spec_version,"transactionVersion":VERSION.transaction_version,"metadataSha256":hex(&sp_io::hashing::sha2_256(&metadata)),"palletIndex":12,"requestsPrefix":hex(&ai::EvaluationRequests::<Runtime>::hashed_key_for(0,[0;32])[..32]),"predictionsPrefix":hex(&ai::Predictions::<Runtime>::hashed_key_for(0)[..32]),"evidencePrefix":hex(&ai::EvaluationEvidence::<Runtime>::hashed_key_for(0,0)[..32]),"financialModesAllowed":false,"status":"development profile; not deployed or release accepted"});
        std::fs::write(path,serde_json::to_vec_pretty(&profile).unwrap()).unwrap();
    }
}
#[test]
fn evaluation_mode_preserves_historical_market_and_bond_refunds() {
    use super::v14_ai_lifecycle_tests as old;
    old::ext().execute_with(|| {
        old::prepare(&old::fixture()["cases"][0]);
        assert_ok!(AiPredictions::stake_on_prediction_outcome_side(signed(3),0,ai::PredictionOutcomeSide::Yes,40*DECIMALS));
        assert_ok!(AiPredictions::stake_on_prediction_outcome_side(signed(4),0,ai::PredictionOutcomeSide::No,60*DECIMALS));
        assert_ok!(AiPredictions::propose_prediction_outcome(signed(8),0,ai::SettlementOutcome::Correct,b"fixture://e".to_vec()));
        assert_ok!(AiPredictions::dispute_prediction_outcome(signed(6),0,ai::DisputeReason::Other,b"fixture://d".to_vec(),10*DECIMALS));
        AiFinancialModesAllowed::set(false);
        assert_ok!(AiPredictions::set_market_economics_config(RuntimeOrigin::root(),false,true));
        assert_ok!(AiPredictions::set_settlement_economics_config(RuntimeOrigin::root(),false,false,false));
        assert_ok!(AiPredictions::cancel_prediction_settlement(RuntimeOrigin::root(),0,b"fixture://recovery".to_vec()));
        assert_ok!(AiPredictions::resolve_prediction_dispute_bond(RuntimeOrigin::root(),0,false));
        assert_ok!(AiPredictions::claim_prediction_market_payout(signed(3),0));
        assert_ok!(AiPredictions::claim_prediction_market_payout(signed(4),0));
        for n in [3,4,6] {assert_eq!(Balances::free_balance(who(n)),1000*DECIMALS);assert_eq!(Balances::reserved_balance(who(n)),0);}
    });
}
