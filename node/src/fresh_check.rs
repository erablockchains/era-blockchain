//! Offline execution of the built Wasm from synthetic genesis. No node, socket or DB starts.
use era_runtime::{
    self as rt, fresh_genesis_inputs::Inputs, Header, RuntimeCall, UncheckedExtrinsic,
};
use parity_scale_codec::{Compact, Decode, Encode};
use sc_executor::WasmExecutor;
use sp_consensus_babe::digests::{CompatibleDigestItem, PreDigest, SecondaryPlainPreDigest};
use sp_core::Pair;
use sp_core::{
    traits::{CallContext, CodeExecutor, ReadRuntimeVersionExt, RuntimeCode, WrappedRuntimeCode},
    H256,
};
use sp_runtime::{
    traits::{BlakeTwo256, Hash as HashT, Header as HeaderT},
    BuildStorage, Digest, DigestItem, StateVersion,
};
use sp_state_machine::TestExternalities;

type Ext = TestExternalities<BlakeTwo256>;
pub(crate) fn call(
    executor: &WasmExecutor<sp_io::SubstrateHostFunctions>,
    ext: &mut Ext,
    code: &[u8],
    method: &str,
    input: &[u8],
) -> Result<Vec<u8>, String> {
    let wrapped = WrappedRuntimeCode(code.into());
    let runtime = RuntimeCode {
        code_fetcher: &wrapped,
        hash: sp_core::blake2_256(code).to_vec(),
        heap_pages: None,
    };
    executor
        .call(
            &mut ext.ext(),
            &runtime,
            method,
            input,
            CallContext::Onchain,
        )
        .0
        .map_err(|e| format!("{method}: {e:?}"))
}
pub(crate) fn decode<T: Decode>(bytes: Vec<u8>) -> Result<T, String> {
    let mut raw = &bytes[..];
    let result = T::decode(&mut raw).map_err(|e| e.to_string())?;
    if !raw.is_empty() {
        return Err("trailing runtime API output".into());
    }
    Ok(result)
}
fn signed(call: RuntimeCall, uri: &str, genesis: H256) -> Result<UncheckedExtrinsic, String> {
    // Decode the exact immortal / nonce-zero / zero-tip extensions of this runtime.
    let extra = rt::SignedExtra::decode(&mut &[0u8, 0, 0][..]).map_err(|e| e.to_string())?;
    let pair = sp_core::sr25519::Pair::from_string(uri, None).map_err(|e| format!("{e:?}"))?;
    let payload = sp_runtime::generic::SignedPayload::from_raw(
        call.clone(),
        extra.clone(),
        (
            (),
            rt::VERSION.spec_version,
            rt::VERSION.transaction_version,
            genesis,
            genesis,
            (),
            (),
            (),
        ),
    );
    let signature = payload.using_encoded(|bytes| pair.sign(bytes));
    Ok(UncheckedExtrinsic::new_signed(
        call,
        sp_runtime::MultiAddress::Id(pair.public().into()),
        signature.into(),
        extra,
    ))
}
pub fn run(blocks: u32) -> Result<serde_json::Value, String> {
    if !(1..=1302).contains(&blocks) {
        return Err("offline block bound is 1..1302".into());
    }
    let code = rt::WASM_BINARY.ok_or("new runtime Wasm was not built")?;
    let inputs = Inputs::synthetic();
    let mut storage = inputs.genesis()?.build_storage()?;
    storage.top.insert(b":code".to_vec(), code.to_vec());
    let mut wasm = Ext::new_with_code(code, storage.clone());
    let mut native = Ext::new_with_code(code, storage);
    wasm.register_extension(ReadRuntimeVersionExt::new(
        WasmExecutor::<sp_io::SubstrateHostFunctions>::builder().build(),
    ));
    let executor = WasmExecutor::<sp_io::SubstrateHostFunctions>::builder().build();
    let root = wasm.execute_with(|| H256::from_slice(&sp_io::storage::root(StateVersion::V1)));
    let genesis = Header::new(
        0,
        BlakeTwo256::ordered_trie_root(vec![], StateVersion::V0),
        root,
        H256::zero(),
        Digest::default(),
    );
    let genesis_hash = genesis.hash();
    let old = H256::from_slice(
        &hex::decode("ba96ed0fe6c37790ee7da7ed9e83e9630b29fc8ba66e6753dc2e5c5704aa94e7").unwrap(),
    );
    if genesis_hash == old {
        return Err("genesis collides with old network".into());
    }
    let version = call(&executor, &mut wasm, code, "Core_version", &[])?;
    let metadata = call(&executor, &mut wasm, code, "Metadata_metadata", &[])?;
    let mut parent = genesis_hash;
    let mut observations = Vec::new();
    for n in 1..=blocks {
        let slot = 300_000_037 + u64::from(n) - 1;
        let header = Header::new(
            n,
            H256::zero(),
            H256::zero(),
            parent,
            Digest {
                logs: vec![DigestItem::babe_pre_digest(PreDigest::SecondaryPlain(
                    SecondaryPlainPreDigest {
                        authority_index: (n - 1) % 4,
                        slot: slot.into(),
                    },
                ))],
            },
        );
        let timestamp =
            UncheckedExtrinsic::new_bare(RuntimeCall::Timestamp(pallet_timestamp::Call::set {
                now: slot * 6000,
            }));
        call(
            &executor,
            &mut wasm,
            code,
            "Core_initialize_block",
            &header.encode(),
        )?;
        let applied: sp_runtime::ApplyExtrinsicResult = decode(call(
            &executor,
            &mut wasm,
            code,
            "BlockBuilder_apply_extrinsic",
            &timestamp.encode(),
        )?)?;
        applied
            .map_err(|e| format!("Wasm validity: {e:?}"))?
            .map_err(|e| format!("Wasm dispatch: {e:?}"))?;
        native.execute_with(|| {
            rt::Executive::initialize_block(&header);
            rt::Executive::apply_extrinsic(timestamp)
                .expect("native validity")
                .expect("native timestamp");
        });
        let transaction = if n == 1 {
            let encoded = (
                [2u8, 3],
                sp_runtime::MultiAddress::<rt::AccountId, ()>::Id(inputs.community.clone()),
                Compact(rt::DECIMALS / 10),
            )
                .encode();
            let transfer = RuntimeCall::decode(&mut &encoded[..]).map_err(|e| e.to_string())?;
            let wrong = signed(transfer.clone(), "//RelaunchSudo", old)?;
            let before = wasm.execute_with(|| sp_io::storage::root(StateVersion::V1));
            let invalid: sp_runtime::ApplyExtrinsicResult = decode(call(
                &executor,
                &mut wasm,
                code,
                "BlockBuilder_apply_extrinsic",
                &wrong.encode(),
            )?)?;
            if invalid.is_ok()
                || wasm.execute_with(|| sp_io::storage::root(StateVersion::V1)) != before
            {
                return Err("wrong-genesis Wasm transaction accepted or changed state".into());
            }
            if native
                .execute_with(|| rt::Executive::apply_extrinsic(wrong))
                .is_ok()
            {
                return Err("wrong-genesis native transaction accepted".into());
            }
            Some(signed(transfer, "//RelaunchSudo", genesis_hash)?)
        } else if n == 702 {
            let validator = native.execute_with(|| rt::Session::validators()[0].clone());
            Some(signed(
                RuntimeCall::SecurityBudget(pallet_security_budget::Call::claim_reward_page {
                    era: 0,
                    validator,
                    page: 0,
                }),
                "//RelaunchCommunity",
                genesis_hash,
            )?)
        } else {
            None
        };
        if let Some(tx) = transaction {
            let applied: sp_runtime::ApplyExtrinsicResult = decode(call(
                &executor,
                &mut wasm,
                code,
                "BlockBuilder_apply_extrinsic",
                &tx.encode(),
            )?)?;
            applied
                .map_err(|e| format!("signed Wasm validity at {n}: {e:?}"))?
                .map_err(|e| format!("signed Wasm dispatch at {n}: {e:?}"))?;
            native
                .execute_with(|| rt::Executive::apply_extrinsic(tx))
                .map_err(|e| format!("signed native validity: {e:?}"))?
                .map_err(|e| format!("signed native dispatch: {e:?}"))?;
        }
        let sealed: Header = decode(call(
            &executor,
            &mut wasm,
            code,
            "BlockBuilder_finalize_block",
            &[],
        )?)?;
        let expected = native.execute_with(rt::Executive::finalize_block);
        if sealed != expected {
            return Err(format!(
                "Wasm/native block mismatch at {n}: {sealed:?} / {expected:?}"
            ));
        }
        parent = sealed.hash();
        if n == 1 || n % 100 == 2 || n == blocks {
            let before = wasm.execute_with(|| sp_io::storage::root(StateVersion::V1));
            let current: sp_consensus_babe::Epoch = decode(call(
                &executor,
                &mut wasm,
                code,
                "BabeApi_current_epoch",
                &[],
            )?)?;
            let next: sp_consensus_babe::Epoch =
                decode(call(&executor, &mut wasm, code, "BabeApi_next_epoch", &[])?)?;
            let config: sp_consensus_babe::BabeConfiguration = decode(call(
                &executor,
                &mut wasm,
                code,
                "BabeApi_configuration",
                &[],
            )?)?;
            let start: sp_consensus_babe::Slot = decode(call(
                &executor,
                &mut wasm,
                code,
                "BabeApi_current_epoch_start",
                &[],
            )?)?;
            let (canonical_current, canonical_next, session, era, paused) =
                wasm.execute_with(|| {
                    (
                        rt::Babe::current_epoch(),
                        rt::Babe::next_epoch(),
                        rt::Session::current_index(),
                        pallet_staking::ActiveEra::<rt::Runtime>::get()
                            .unwrap()
                            .index,
                        pallet_security_budget::AllocationPaused::<rt::Runtime>::get(),
                    )
                });
            if current != canonical_current
                || next != canonical_next
                || start != current.start_slot
                || config.authorities != current.authorities
                || config.randomness != current.randomness
                || config.c != current.config.c
                || config.allowed_slots != current.config.allowed_slots
                || paused
            {
                return Err(format!("consensus/accounting mismatch at {n}"));
            }
            if wasm.execute_with(|| sp_io::storage::root(StateVersion::V1)) != before {
                return Err("read-only BABE API mutated storage".into());
            }
            observations.push(serde_json::json!({"block":n,"session":session,"era":era,"epoch":current.epoch_index,"next_epoch":next.epoch_index,"start_slot":u64::from(start),"randomness":hex::encode(current.randomness),"next_randomness":hex::encode(next.randomness),"authorities":current.authorities.len()}));
        }
    }
    let monetary=wasm.execute_with(||{
        rt::fresh_genesis::validate_lifecycle();
        if blocks >= 702 {
            assert!(rt::SecurityBudget::principal_reserved_total() > 0);
            assert!(rt::SecurityBudget::era_principal(0) > 0);
            assert_eq!(rt::SecurityBudget::era_budget(0).unwrap().gross_issuance, 0);
            assert!(rt::SecurityBudget::staking_paid_total() > 0);
        }
        serde_json::json!({"issuance":rt::Balances::total_issuance().to_string(),"remaining_allowance":rt::IssuanceCap::remaining_allowance().unwrap().to_string(),"legacy_liability":rt::SecurityBudget::legacy_reward_liability().to_string(),"new_reward_liability":rt::SecurityBudget::committed_liabilities().to_string(),"unallocated_principal":rt::SecurityBudget::unallocated_principal().to_string(),"principal_reserved_total":rt::SecurityBudget::principal_reserved_total().to_string(),"era_zero_principal":rt::SecurityBudget::era_principal(0).to_string(),"era_zero_gross_issuance":rt::SecurityBudget::era_budget(0).map(|b|b.gross_issuance.to_string())})
    });
    Ok(
        serde_json::json!({"status":"PASS","kind":"offline synthetic Wasm/native execution; no networking or GRANDPA voter","blocks":blocks,"genesis_hash":format!("{genesis_hash:#x}"),"old_genesis_hash":format!("{old:#x}"),"wasm_sha256":hex::encode(sp_core::hashing::sha2_256(code)),"core_version_scale":hex::encode(version),"metadata_api_scale_sha256":hex::encode(sp_core::hashing::sha2_256(&metadata)),"signed_transfer_and_fees":true,"wrong_genesis_rejected_without_effects":true,"signed_reward_claim":blocks>=702,"final_block_hash":format!("{parent:#x}"),"observations":observations,"monetary":monetary}),
    )
}

/// No operator URI or key is accepted. The supplied hash also supports wrong-genesis fixtures.
pub fn synthetic_claim(genesis: H256, validator: rt::AccountId) -> Result<String, String> {
    if !Inputs::synthetic()
        .authorities
        .iter()
        .any(|v| v.account == validator)
    {
        return Err("reward claim accepts only disposable synthetic validator accounts".into());
    }
    let tx = signed(
        RuntimeCall::SecurityBudget(pallet_security_budget::Call::claim_reward_page {
            era: 0,
            validator,
            page: 0,
        }),
        "//RelaunchCommunity",
        genesis,
    )?;
    Ok(format!("0x{}", hex::encode(tx.encode())))
}
