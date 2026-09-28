//! Actual-runtime nonfinancial evaluation measurements. No financial mode override.
use crate::{AccountId, Runtime, RuntimeOrigin, System, AiPredictions};
use alloc::{vec, vec::Vec};
use core::marker::PhantomData;
use frame_benchmarking::v1::{account, benchmarks};
use frame_support::{assert_ok};
use pallet_ai_predictions as ai;
pub use frame_system::Config;
pub struct Pallet<T:Config>(PhantomData<T>);
fn owner()->AccountId {account("evaluation-owner",0,0)}
fn reviewer()->AccountId {account("evaluation-reviewer",0,0)}
fn setup() {
    System::set_block_number(10);pallet_timestamp::Now::<Runtime>::put(100_000);
    assert_ok!(AiPredictions::authorize_validator(RuntimeOrigin::root(),owner()));
    assert_ok!(AiPredictions::authorize_validator(RuntimeOrigin::root(),reviewer()));
    assert_ok!(AiPredictions::register_model(RuntimeOrigin::signed(owner()),vec![7;32],vec![255;256]));
    assert_ok!(AiPredictions::approve_model(RuntimeOrigin::signed(reviewer()),0));
    ai::PredictionIdsByModel::<Runtime>::insert(0,frame_support::BoundedVec::try_from((1..1024).collect::<Vec<u64>>()).unwrap());
}
fn submit() {assert_ok!(AiPredictions::submit_evaluation(RuntimeOrigin::signed(owner()),0,[1;32],[7;32],[8;32],vec![255;256],100,100,200,300));}
benchmarks! {
    submit_evaluation {setup();}
    : { AiPredictions::submit_evaluation(RuntimeOrigin::signed(owner()),0,[1;32],[7;32],[8;32],vec![255;256],100,100,200,300)?; }
    verify {assert_eq!(ai::NextPredictionId::<Runtime>::get(),1);}
    submit_evaluation_retry {setup();submit();}
    : { AiPredictions::submit_evaluation(RuntimeOrigin::signed(owner()),0,[1;32],[7;32],[8;32],vec![255;256],100,100,200,300)?; }
    verify {assert_eq!(ai::NextPredictionId::<Runtime>::get(),1);}
    evidence_first {setup();submit();pallet_timestamp::Now::<Runtime>::put(300_000);}
    : { AiPredictions::record_evaluation_evidence(RuntimeOrigin::signed(reviewer()),0,0,[9;32],ai::PredictionOutcome::Failed)?; }
    evidence_correction {setup();submit();pallet_timestamp::Now::<Runtime>::put(300_000);assert_ok!(AiPredictions::record_evaluation_evidence(RuntimeOrigin::signed(reviewer()),0,0,[9;32],ai::PredictionOutcome::Failed));ai::EvaluationRevisionCount::<Runtime>::insert(0,31);}
    : { AiPredictions::record_evaluation_evidence(RuntimeOrigin::signed(reviewer()),0,31,[10;32],ai::PredictionOutcome::Inconclusive)?; }
    update_model {setup();}
    : { AiPredictions::update_model(RuntimeOrigin::signed(owner()),0,vec![6;128],vec![255;256])?; }
    authorize_validator {let who=reviewer();}
    : { AiPredictions::authorize_validator(RuntimeOrigin::root(),who)?; }
    financial_refusal {let who=owner();}
    : { assert!(AiPredictions::stake_on_prediction_token(RuntimeOrigin::signed(who),0,1).is_err()); }
    update_model_by_reviewer {setup();}
    : { AiPredictions::update_model(RuntimeOrigin::signed(reviewer()),0,vec![6;128],vec![255;256])?; }
    register_model_restricted {
        System::set_block_number(10);assert_ok!(AiPredictions::authorize_validator(RuntimeOrigin::root(),owner()));
        ai::ModelIdsByOwner::<Runtime>::insert(owner(),frame_support::BoundedVec::try_from((1..64).collect::<Vec<u64>>()).unwrap());
    }: { AiPredictions::register_model(RuntimeOrigin::signed(owner()),vec![7;128],vec![255;256])?; }
    approve_model {setup();AiPredictions::update_model(RuntimeOrigin::signed(owner()),0,vec![7;32],vec![255;256])?;}
    : { AiPredictions::approve_model(RuntimeOrigin::signed(reviewer()),0)?; }
    set_model_submitter {setup();}
    : { AiPredictions::set_model_submitter(RuntimeOrigin::signed(reviewer()),0,Some(owner()))?; }
}
