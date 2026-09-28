//! Synthetic off-chain artifact -> SCALE RuntimeCall -> real runtime dispatch.
//! These are origin fixtures, not signed blocks/RPC/wallet or inference-service certification.
use super::*;
use codec::{Decode, Encode};
use frame_support::assert_ok;
use pallet_ai_predictions as ai;
use serde_json::{json, Value};
use sp_runtime::{traits::Dispatchable, BuildStorage};

fn account(n: u8) -> AccountId {
    AccountId::new([n; 32])
}
fn signed(n: u8) -> RuntimeOrigin {
    RuntimeOrigin::signed(account(n))
}
fn decode_hex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}
pub(super) fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../tools/v14-ai-lifecycle-fixture/fixtures/lifecycle.json"
    ))
    .unwrap()
}
pub(super) fn ext() -> sp_io::TestExternalities {
    // Retained funded-market tests exercise historical code, never the production policy.
    AiFinancialModesAllowed::set(true);
    let mut storage = frame_system::GenesisConfig::<Runtime>::default()
        .build_storage()
        .unwrap();
    let mut balances: Vec<_> = (1..=9).map(|n| (account(n), 1000 * DECIMALS)).collect();
    balances.extend([
        (AiPredictions::market_account_id(), EXISTENTIAL_DEPOSIT),
        (
            AiPredictions::market_treasury_account_id(),
            EXISTENTIAL_DEPOSIT,
        ),
        (AiPenaltyDestination::get(), EXISTENTIAL_DEPOSIT),
    ]);
    pallet_balances::GenesisConfig::<Runtime> {
        balances,
        dev_accounts: None,
    }
    .assimilate_storage(&mut storage)
    .unwrap();
    let mut ext = sp_io::TestExternalities::new(storage);
    ext.execute_with(|| System::set_block_number(100));
    ext
}
fn send(origin: RuntimeOrigin, call: ai::Call<Runtime>) -> sp_runtime::DispatchResult {
    let bytes = RuntimeCall::AiPredictions(call).encode();
    assert_eq!(bytes[0], 12);
    let mut input = bytes.as_slice();
    let call = RuntimeCall::decode(&mut input).unwrap();
    assert!(input.is_empty());
    assert_eq!(call.encode(), bytes);
    call.dispatch(origin)
        .map(|_| ())
        .map_err(|error| error.error)
}
pub(super) fn prepare(case: &Value) {
    let f = fixture();
    let model = decode_hex(f["model_sha256"].as_str().unwrap());
    assert_eq!(
        sp_io::hashing::sha2_256(f["model_json"].as_str().unwrap().as_bytes()).as_slice(),
        model.as_slice()
    );
    let prediction = decode_hex(case["prediction_sha256"].as_str().unwrap());
    assert_eq!(
        sp_io::hashing::sha2_256(case["prediction_json"].as_str().unwrap().as_bytes()).as_slice(),
        prediction.as_slice()
    );
    // AI administrator8 is NOT a member of Session::validators(). Owner1 and submitter2 differ.
    assert!(!Session::validators().contains(&account(8)));
    assert_ok!(send(
        RuntimeOrigin::root(),
        ai::Call::authorize_validator {
            account: account(8)
        }
    ));
    assert!(send(
        signed(1),
        ai::Call::register_model {
            model_hash: model.clone(),
            metadata_uri: b"fixture://model-v1".to_vec()
        }
    )
    .is_err());
    // Restricted historical registration grants this account the existing broad AI-admin role.
    assert_ok!(send(
        RuntimeOrigin::root(),
        ai::Call::authorize_validator {
            account: account(1)
        }
    ));
    assert_ok!(send(
        signed(1),
        ai::Call::register_model {
            model_hash: model,
            metadata_uri: b"fixture://model-v1".to_vec()
        }
    ));
    assert_eq!(
        ai::ModelGovernanceById::<Runtime>::get(0).unwrap().status,
        ai::ModelStatus::Pending
    );
    let call = || ai::Call::submit_prediction {
        model_id: 0,
        domain: ai::PredictionDomain::AiBenchmark,
        category_code: b"fixture.synthetic.binary".to_vec(),
        prediction_hash: prediction.clone(),
        metadata_uri: format!("fixture://{}", case["name"].as_str().unwrap()).into_bytes(),
        confidence: 75,
        expires_at: 120,
    };
    assert!(send(signed(1), call()).is_err());
    assert_ok!(send(signed(8), ai::Call::approve_model { model_id: 0 }));
    assert_ok!(send(
        signed(1),
        ai::Call::set_model_submitter {
            model_id: 0,
            submitter: Some(account(2))
        }
    ));
    assert!(send(signed(9), call()).is_err());
    assert_ok!(send(signed(2), call()));
    assert_eq!(
        ai::Predictions::<Runtime>::get(0).unwrap().submitter,
        account(2)
    );
    assert_ok!(send(
        RuntimeOrigin::root(),
        ai::Call::set_tokenization_config {
            tokenization_enabled: true,
            transfers_enabled: true
        }
    ));
    assert!(send(
        signed(1),
        ai::Call::tokenize_prediction {
            prediction_id: 0,
            metadata_uri: b"fixture://token".to_vec(),
            transferable: true
        }
    )
    .is_err());
    assert_ok!(send(
        signed(2),
        ai::Call::tokenize_prediction {
            prediction_id: 0,
            metadata_uri: b"fixture://token".to_vec(),
            transferable: true
        }
    ));
    assert_ok!(send(
        signed(2),
        ai::Call::approve_prediction_token_transfer {
            token_id: 0,
            approved: account(9)
        }
    ));
    assert_ok!(send(
        signed(9),
        ai::Call::transfer_prediction_token {
            token_id: 0,
            to: account(7)
        }
    ));
    assert_eq!(
        ai::PredictionTokens::<Runtime>::get(0).unwrap().owner,
        account(7)
    );
    assert!(!ai::PredictionTokenApprovals::<Runtime>::contains_key(0));
    assert!(send(
        signed(2),
        ai::Call::tokenize_prediction {
            prediction_id: 0,
            metadata_uri: b"fixture://again".to_vec(),
            transferable: true
        }
    )
    .is_err());
    assert_ok!(send(
        RuntimeOrigin::root(),
        ai::Call::set_staking_config {
            staking_enabled: true,
            min_stake: DECIMALS
        }
    ));
    assert_ok!(send(
        RuntimeOrigin::root(),
        ai::Call::set_market_economics_config {
            market_enabled: true,
            allow_unstake_before_settlement: true
        }
    ));
    assert_ok!(send(
        RuntimeOrigin::root(),
        ai::Call::set_market_fee_config {
            fees_enabled: false,
            fee_bps: 0,
            treasury_enabled: true
        }
    ));
    assert_ok!(send(
        RuntimeOrigin::root(),
        ai::Call::set_settlement_config {
            settlement_enabled: true,
            dispute_window_blocks: 10,
            min_dispute_bond: 10 * DECIMALS
        }
    ));
    assert_ok!(send(
        RuntimeOrigin::root(),
        ai::Call::set_settlement_economics_config {
            economics_enabled: true,
            slash_incorrect: false,
            slash_fraudulent: false
        }
    ));
}
fn snapshot(stage: &str) -> Value {
    let accounts:Vec<_>=[("yes",3), ("no",4),("passive",5),("disputant",6),("token_holder",7)].iter().map(|(name,n)|json!({"role":name,"free_base_units":Balances::free_balance(account(*n)).to_string(),"reserved_base_units":Balances::reserved_balance(account(*n)).to_string()})).collect();
    let passive: Balance = ai::PredictionTokenStakes::<Runtime>::iter()
        .map(|(_, _, stake)| stake.amount)
        .sum();
    let bonds: Balance = ai::PredictionTokenDisputes::<Runtime>::iter()
        .filter(|(_, d)| !d.resolved)
        .map(|(_, d)| d.bond)
        .sum();
    let escrow = Balances::free_balance(AiPredictions::market_account_id()) - EXISTENTIAL_DEPOSIT;
    json!({"stage":stage,"accounts":accounts,"passive_liability_base_units":passive.to_string(),"unresolved_bond_base_units":bonds.to_string(),"market_cash_above_floor_base_units":escrow.to_string(),"funded_unpaid_total_base_units":(passive+bonds+escrow).to_string(),"unclaimed_position_records":ai::PredictionTokenSideStakes::<Runtime>::iter().count(),"market_fee_treasury_above_floor":(Balances::free_balance(AiPredictions::market_treasury_account_id())-EXISTENTIAL_DEPOSIT).to_string(),"penalty_treasury_above_floor":(Balances::free_balance(AiPenaltyDestination::get())-EXISTENTIAL_DEPOSIT).to_string(),"issuance":Balances::total_issuance().to_string()})
}
#[test]
fn ai_complete_offchain_fixture_lifecycles_and_liabilities() {
    let mut results = Vec::new();
    for case in fixture()["cases"].as_array().unwrap() {
        ext().execute_with(|| {
            let name=case["name"].as_str().unwrap();prepare(case);let issuance=Balances::total_issuance();
            let no_winner=name=="no_winner";let fraud=name=="prediction_fraud";let bad_dispute=name=="fraudulent_dispute";let disputed=fraud||bad_dispute||name=="good_faith_rejected";
            let mut snapshots=vec![snapshot("before_funding")];
            if !no_winner {assert_ok!(send(signed(3),ai::Call::stake_on_prediction_outcome_side{token_id:0,side:ai::PredictionOutcomeSide::Yes,amount:40*DECIMALS}));}
            assert_ok!(send(signed(4),ai::Call::stake_on_prediction_outcome_side{token_id:0,side:ai::PredictionOutcomeSide::No,amount:60*DECIMALS}));
            assert_ok!(send(signed(5),ai::Call::stake_on_prediction_token{token_id:0,amount:20*DECIMALS}));snapshots.push(snapshot("funded"));
            // At expiry the fixture supplies independently generated synthetic observations.
            System::set_block_number(120);
            let validation=if fraud {ai::PredictionOutcome::Inconclusive} else if name=="incorrect" {ai::PredictionOutcome::Failed} else {ai::PredictionOutcome::Successful};
            assert_ok!(send(signed(8),ai::Call::validate_prediction{prediction_id:0,outcome:validation}));
            assert!(send(signed(8),ai::Call::validate_prediction{prediction_id:0,outcome:validation}).is_err());
            let outcome=if fraud {ai::SettlementOutcome::Fraudulent} else if name=="incorrect" {ai::SettlementOutcome::Incorrect} else {ai::SettlementOutcome::Correct};
            assert_ok!(send(signed(8),ai::Call::propose_prediction_outcome{token_id:0,proposed_outcome:if fraud {ai::SettlementOutcome::Correct} else {outcome},evidence_uri:b"fixture://proposed-evidence".to_vec()}));
            assert!(send(signed(9),ai::Call::finalize_prediction_outcome_after_dispute_window{token_id:0}).is_err());
            if disputed {
                System::set_block_number(121);
                assert_ok!(send(signed(6),ai::Call::dispute_prediction_outcome{token_id:0,reason:if fraud {ai::DisputeReason::DuplicateOrFraudulentPrediction} else {ai::DisputeReason::WrongOutcome},evidence_uri:b"fixture://challenge".to_vec(),bond:10*DECIMALS}));
                assert!(send(signed(6),ai::Call::dispute_prediction_outcome{token_id:0,reason:ai::DisputeReason::Other,evidence_uri:b"fixture://retry".to_vec(),bond:10*DECIMALS}).is_err());snapshots.push(snapshot("disputed"));
                assert!(send(signed(8),ai::Call::admin_finalize_prediction_outcome{token_id:0,final_outcome:outcome,evidence_uri:b"fixture://final".to_vec()}).is_err());
                System::set_block_number(122);
                assert_ok!(send(RuntimeOrigin::root(),ai::Call::admin_finalize_prediction_outcome{token_id:0,final_outcome:outcome,evidence_uri:b"fixture://final".to_vec()}));
                if fraud {
                    assert_eq!(case["model_execution_claim_consistent"],false);
                    assert_ok!(send(signed(8),ai::Call::suspend_model{model_id:0}));
                }
                if bad_dispute {
                    assert_eq!(case["challenge_matches_reference"], false);
                    assert_ne!(case["challenge_json"], case["observation_json"]);
                    let witness:Value=serde_json::from_str(case["witness_json"].as_str().unwrap()).unwrap();
                    assert_ne!(witness["challenge_document_sha256"], witness["reference_observation_sha256"]);
                    let evidence: [u8;32]=decode_hex(case["evidence_sha256"].as_str().unwrap()).try_into().unwrap();
                    assert_ok!(send(RuntimeOrigin::root(),ai::Call::adjudicate_prediction_dispute_bond{dispute_id:0,accepted:false,fraudulent:true,evidence_hash:evidence}));
                } else {assert_ok!(send(RuntimeOrigin::root(),ai::Call::resolve_prediction_dispute_bond{dispute_id:0,accepted:fraud}));}
                assert!(send(RuntimeOrigin::root(),ai::Call::resolve_prediction_dispute_bond{dispute_id:0,accepted:true}).is_err());
            } else {
                System::set_block_number(131);assert_ok!(send(signed(9),ai::Call::finalize_prediction_outcome_after_dispute_window{token_id:0}));
            }
            snapshots.push(snapshot("adjudicated"));
            // Token buyer has no right to other accounts' financial claims.
            assert!(send(signed(7),ai::Call::claim_prediction_token_settlement{token_id:0}).is_err());
            assert_ok!(send(signed(5),ai::Call::claim_prediction_token_settlement{token_id:0}));
            assert!(send(signed(5),ai::Call::claim_prediction_token_settlement{token_id:0}).is_err());snapshots.push(snapshot("passive_claimed"));
            if !no_winner {assert_ok!(send(signed(3),ai::Call::claim_prediction_market_payout{token_id:0}));snapshots.push(snapshot("yes_claimed"));assert!(send(signed(3),ai::Call::claim_prediction_market_payout{token_id:0}).is_err());}
            assert_ok!(send(signed(4),ai::Call::claim_prediction_market_payout{token_id:0}));
            assert!(send(signed(4),ai::Call::claim_prediction_market_payout{token_id:0}).is_err());
            let (yes,no)=if fraud||no_winner {(1000,1000)} else if name=="incorrect" {(960,1040)} else {(1060,940)};
            assert_eq!(Balances::free_balance(account(3)),yes*DECIMALS);assert_eq!(Balances::free_balance(account(4)),no*DECIMALS);
            assert_eq!(Balances::free_balance(account(5)),1000*DECIMALS);assert_eq!(Balances::free_balance(account(6)),if bad_dispute {990} else {1000}*DECIMALS);
            assert_eq!(Balances::free_balance(AiPenaltyDestination::get()),EXISTENTIAL_DEPOSIT+if bad_dispute {10*DECIMALS} else {0});
            assert_eq!(Balances::total_issuance(),issuance);
            snapshots.push(snapshot("all_claims_complete"));
            assert_eq!(snapshots.last().unwrap()["funded_unpaid_total_base_units"],"0");
            assert_ok!(send(signed(7),ai::Call::burn_prediction_token{token_id:0}));
            assert!(ai::PredictionTokens::<Runtime>::get(0).unwrap().burned);assert_eq!(ai::PredictionTokenByPrediction::<Runtime>::get(0),Some(0));
            results.push(json!({"case":name,"fixture_only":true,"reported_prediction_sha256":case["prediction_sha256"],"snapshots":snapshots,"settlement":format!("{outcome:?}"),"transaction_fees_in_fixture":0,"actual_signed_extrinsics":false,"claims_complete":true}));
        });
    }
    let result = json!({"schema":1,"runtime_spec":VERSION.spec_version,"mode":"SCALE RuntimeCall decode and actual runtime dispatch with synthetic origins; no RPC/signature/block admission","fixture_only":true,"production_runner":false,"cases":results});
    if let Ok(path) = std::env::var("ERA_V14_AI_LIFECYCLE_OUTPUT") {
        std::fs::write(path, serde_json::to_vec_pretty(&result).unwrap()).unwrap();
    }
    println!("AI lifecycle: {} synthetic cases passed; per-stage liabilities and balance conservation verified",results.len());
}

#[test]
fn ai_funding_after_terminal_claim_must_reject_without_new_locked_funds() {
    ext().execute_with(|| {
        prepare(&fixture()["cases"][0]);
        assert_ok!(send(
            signed(5),
            ai::Call::stake_on_prediction_token {
                token_id: 0,
                amount: 20 * DECIMALS
            }
        ));
        assert_ok!(send(
            signed(8),
            ai::Call::propose_prediction_outcome {
                token_id: 0,
                proposed_outcome: ai::SettlementOutcome::Correct,
                evidence_uri: b"fixture://early".to_vec()
            }
        ));
        assert_ok!(send(
            RuntimeOrigin::root(),
            ai::Call::admin_finalize_prediction_outcome {
                token_id: 0,
                final_outcome: ai::SettlementOutcome::Correct,
                evidence_uri: b"fixture://early-final".to_vec()
            }
        ));
        assert_ok!(send(
            signed(5),
            ai::Call::claim_prediction_token_settlement { token_id: 0 }
        ));
        let before = sp_io::storage::root(sp_runtime::StateVersion::V1);
        assert!(
            send(
                signed(5),
                ai::Call::stake_on_prediction_token {
                    token_id: 0,
                    amount: 20 * DECIMALS
                }
            )
            .is_err(),
            "new funding after a permanent claim marker can become unclaimable"
        );
        assert_eq!(sp_io::storage::root(sp_runtime::StateVersion::V1), before);
    });
}
#[test]
fn ai_cancelled_settlement_must_close_funding() {
    ext().execute_with(|| {
        prepare(&fixture()["cases"][0]);
        assert_ok!(send(
            RuntimeOrigin::root(),
            ai::Call::cancel_prediction_settlement {
                token_id: 0,
                evidence_uri: b"fixture://cancel".to_vec()
            }
        ));
        for call in [
            ai::Call::stake_on_prediction_token {
                token_id: 0,
                amount: DECIMALS,
            },
            ai::Call::stake_on_prediction_outcome_side {
                token_id: 0,
                side: ai::PredictionOutcomeSide::Yes,
                amount: DECIMALS,
            },
        ] {
            let before = sp_io::storage::root(sp_runtime::StateVersion::V1);
            assert!(
                send(signed(3), call).is_err(),
                "cancellation must close admission"
            );
            assert_eq!(sp_io::storage::root(sp_runtime::StateVersion::V1), before);
        }
    });
}
#[test]
fn ai_cancelled_refund_must_not_be_reopened_as_a_winner_payout() {
    ext().execute_with(|| {
        prepare(&fixture()["cases"][0]);
        assert_ok!(send(signed(3),ai::Call::stake_on_prediction_outcome_side{token_id:0,side:ai::PredictionOutcomeSide::Yes,amount:40*DECIMALS}));
        assert_ok!(send(signed(4),ai::Call::stake_on_prediction_outcome_side{token_id:0,side:ai::PredictionOutcomeSide::No,amount:60*DECIMALS}));
        assert_ok!(send(RuntimeOrigin::root(),ai::Call::cancel_prediction_settlement{token_id:0,evidence_uri:b"fixture://cancel".to_vec()}));
        assert_ok!(send(signed(4),ai::Call::claim_prediction_market_payout{token_id:0}));
        let before=sp_io::storage::root(sp_runtime::StateVersion::V1);
        assert!(send(RuntimeOrigin::root(),ai::Call::admin_finalize_prediction_outcome{token_id:0,final_outcome:ai::SettlementOutcome::Correct,evidence_uri:b"fixture://reopen".to_vec()}).is_err(),"reopening cancellation can spend already refunded losing principal from a shared escrow");
        assert_eq!(sp_io::storage::root(sp_runtime::StateVersion::V1),before);
        assert_ok!(send(signed(3),ai::Call::claim_prediction_market_payout{token_id:0}));
        assert_eq!(Balances::free_balance(account(3)),1000*DECIMALS);assert_eq!(Balances::free_balance(account(4)),1000*DECIMALS);
    });
}
#[test]
fn ai_frozen_and_burned_tokens_must_reject_new_side_positions() {
    for burned in [false, true] {
        ext().execute_with(|| {
            prepare(&fixture()["cases"][0]);
            if burned {
                assert_ok!(send(
                    signed(7),
                    ai::Call::burn_prediction_token { token_id: 0 }
                ));
            } else {
                assert_ok!(send(
                    RuntimeOrigin::root(),
                    ai::Call::freeze_prediction_token { token_id: 0 }
                ));
            }
            let before = sp_io::storage::root(sp_runtime::StateVersion::V1);
            assert!(send(
                signed(3),
                ai::Call::stake_on_prediction_outcome_side {
                    token_id: 0,
                    side: ai::PredictionOutcomeSide::Yes,
                    amount: DECIMALS
                }
            )
            .is_err());
            assert_eq!(sp_io::storage::root(sp_runtime::StateVersion::V1), before);
        });
    }
}
#[test]
fn ai_expired_prediction_must_reject_new_passive_positions() {
    ext().execute_with(|| {
        prepare(&fixture()["cases"][0]);
        System::set_block_number(120);
        assert!(send(
            signed(5),
            ai::Call::stake_on_prediction_token {
                token_id: 0,
                amount: DECIMALS
            }
        )
        .is_err());
    });
}

// Characterization tests expose limitations; a pass does not certify the behavior as safe.
#[test]
fn ai_characterization_model_update_and_business_retry_are_not_immutable_or_idempotent() {
    ext().execute_with(|| {
        let case = &fixture()["cases"][0];
        prepare(case);
        let original = ai::Predictions::<Runtime>::get(0).unwrap().encode();
        assert_ok!(send(
            signed(1),
            ai::Call::update_model {
                model_id: 0,
                model_hash: b"a-different-unversioned-artifact".to_vec(),
                metadata_uri: b"fixture://model-v2".to_vec()
            }
        ));
        assert_eq!(
            ai::ModelGovernanceById::<Runtime>::get(0).unwrap().status,
            ai::ModelStatus::Approved
        );
        assert_eq!(
            ai::Predictions::<Runtime>::get(0).unwrap().encode(),
            original
        );
        assert_ok!(send(
            signed(2),
            ai::Call::submit_prediction {
                model_id: 0,
                domain: ai::PredictionDomain::AiBenchmark,
                category_code: b"fixture.synthetic.binary".to_vec(),
                prediction_hash: decode_hex(case["prediction_sha256"].as_str().unwrap()),
                metadata_uri: b"fixture://retry".to_vec(),
                confidence: 75,
                expires_at: 120
            }
        ));
        assert_eq!(ai::NextPredictionId::<Runtime>::get(), 2);
    });
}
#[test]
fn ai_characterization_validation_and_settlement_can_precede_expiry_and_disagree() {
    ext().execute_with(|| {
        prepare(&fixture()["cases"][0]);
        assert_eq!(System::block_number(), 100);
        assert_ok!(send(
            signed(8),
            ai::Call::validate_prediction {
                prediction_id: 0,
                outcome: ai::PredictionOutcome::Successful
            }
        ));
        assert_ok!(send(
            signed(8),
            ai::Call::propose_prediction_outcome {
                token_id: 0,
                proposed_outcome: ai::SettlementOutcome::Incorrect,
                evidence_uri: b"fixture://contradictory".to_vec()
            }
        ));
        assert_ok!(send(
            RuntimeOrigin::root(),
            ai::Call::admin_finalize_prediction_outcome {
                token_id: 0,
                final_outcome: ai::SettlementOutcome::Incorrect,
                evidence_uri: b"fixture://before-expiry".to_vec()
            }
        ));
        assert_eq!(
            ai::Predictions::<Runtime>::get(0).unwrap().outcome,
            Some(ai::PredictionOutcome::Successful)
        );
        assert_eq!(
            ai::PredictionTokenSettlements::<Runtime>::get(0)
                .unwrap()
                .final_outcome,
            Some(ai::SettlementOutcome::Incorrect)
        );
    });
}
#[test]
fn ai_characterization_expired_passive_position_needs_adjudication_or_root_cancellation() {
    ext().execute_with(|| {
        prepare(&fixture()["cases"][0]);
        assert_ok!(send(
            signed(5),
            ai::Call::stake_on_prediction_token {
                token_id: 0,
                amount: 20 * DECIMALS
            }
        ));
        System::set_block_number(120);
        assert!(send(
            signed(5),
            ai::Call::unstake_prediction_token {
                token_id: 0,
                amount: 20 * DECIMALS
            }
        )
        .is_err());
        assert!(send(
            signed(5),
            ai::Call::claim_prediction_token_settlement { token_id: 0 }
        )
        .is_err());
        assert_ok!(send(
            RuntimeOrigin::root(),
            ai::Call::cancel_prediction_settlement {
                token_id: 0,
                evidence_uri: b"fixture://missing-outcome".to_vec()
            }
        ));
        assert_ok!(send(
            signed(5),
            ai::Call::claim_prediction_token_settlement { token_id: 0 }
        ));
        assert_eq!(Balances::free_balance(account(5)), 1000 * DECIMALS);
    });
}

#[test]
fn ai_existing_records_and_claim_rights_survive_current_v3_hook() {
    use frame_support::traits::{Hooks, StorageVersion};
    ext().execute_with(|| {
        prepare(&fixture()["cases"][0]);
        assert_ok!(send(
            signed(5),
            ai::Call::stake_on_prediction_token {
                token_id: 0,
                amount: 20 * DECIMALS
            }
        ));
        assert_ok!(send(
            signed(3),
            ai::Call::stake_on_prediction_outcome_side {
                token_id: 0,
                side: ai::PredictionOutcomeSide::Yes,
                amount: 40 * DECIMALS
            }
        ));
        StorageVersion::new(3).put::<AiPredictions>();
        let before = sp_io::storage::root(sp_runtime::StateVersion::V1);
        let _ = <AiPredictions as Hooks<BlockNumber>>::on_runtime_upgrade();
        assert_eq!(sp_io::storage::root(sp_runtime::StateVersion::V1), before);
        assert_ok!(send(
            RuntimeOrigin::root(),
            ai::Call::cancel_prediction_settlement {
                token_id: 0,
                evidence_uri: b"fixture://preserved-refunds".to_vec()
            }
        ));
        assert_ok!(send(
            signed(5),
            ai::Call::claim_prediction_token_settlement { token_id: 0 }
        ));
        assert_ok!(send(
            signed(3),
            ai::Call::claim_prediction_market_payout { token_id: 0 }
        ));
        assert_eq!(Balances::free_balance(account(3)), 1000 * DECIMALS);
        assert_eq!(Balances::free_balance(account(5)), 1000 * DECIMALS);
        assert_eq!(
            ai::PredictionTokens::<Runtime>::get(0).unwrap().owner,
            account(7)
        );
    });
}
