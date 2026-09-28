//! Synthetic fresh-chain tests. Offline block execution is not a four-node finality rehearsal.
use codec::Encode;
use era_runtime::fresh_genesis_inputs::Inputs;
use era_runtime::*;
use frame_support::{
    assert_ok,
    traits::{fungible::Mutate, Contains, OnRuntimeUpgrade, StorageVersion},
};
use sp_consensus_babe::{
    digests::{CompatibleDigestItem, PreDigest, SecondaryPlainPreDigest},
    runtime_decl_for_babe_api::BabeApi,
};
use sp_core::{sr25519, Pair, H256};
use sp_runtime::{
    generic::SignedPayload, traits::Header as HeaderT, BuildStorage, Digest, DigestItem,
    MultiAddress, StateVersion,
};

fn ext() -> sp_io::TestExternalities {
    Inputs::synthetic()
        .genesis()
        .unwrap()
        .build_storage()
        .unwrap()
        .into()
}
fn begin(n: u32, parent: H256) {
    // Non-aligned genesis slot detects the former absolute-slot modulo calculation.
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
    Executive::initialize_block(&header);
    assert_ok!(Executive::apply_extrinsic(UncheckedExtrinsic::new_bare(
        RuntimeCall::Timestamp(pallet_timestamp::Call::set { now: slot * 6000 })
    )));
}
fn signed(
    call: RuntimeCall,
    uri: &str,
    nonce: u32,
    wrong_genesis: Option<H256>,
) -> UncheckedExtrinsic {
    let pair = sr25519::Pair::from_string(uri, None).unwrap();
    let extra: SignedExtra = (
        frame_system::CheckNonZeroSender::new(),
        frame_system::CheckSpecVersion::new(),
        frame_system::CheckTxVersion::new(),
        frame_system::CheckGenesis::new(),
        frame_system::CheckMortality::from(sp_runtime::generic::Era::Immortal),
        frame_system::CheckNonce::from(nonce),
        frame_system::CheckWeight::new(),
        pallet_transaction_payment::ChargeTransactionPayment::from(0),
    );
    let payload = if let Some(hash) = wrong_genesis {
        SignedPayload::from_raw(
            call.clone(),
            extra.clone(),
            (
                (),
                VERSION.spec_version,
                VERSION.transaction_version,
                hash,
                hash,
                (),
                (),
                (),
            ),
        )
    } else {
        SignedPayload::new(call.clone(), extra.clone()).unwrap()
    };
    let sig = payload.using_encoded(|bytes| pair.sign(bytes));
    UncheckedExtrinsic::new_signed(
        call,
        MultiAddress::Id(pair.public().into()),
        sig.into(),
        extra,
    )
}
#[test]
fn deterministic_genesis_exact_allocations_versions_and_no_legacy_debt() {
    let inputs = Inputs::synthetic();
    let a = inputs.genesis().unwrap().build_storage().unwrap();
    let b = inputs.genesis().unwrap().build_storage().unwrap();
    assert_eq!(a.top, b.top);
    let rows = inputs.allocations().unwrap();
    let mut totals = std::collections::BTreeMap::new();
    for row in rows {
        *totals.entry(row.category).or_insert(0u128) += row.base_units.parse::<u128>().unwrap();
    }
    assert_eq!(totals["founding"], 20_000_000 * DECIMALS);
    assert_eq!(totals["ecosystem"], 20_000_000 * DECIMALS);
    assert_eq!(totals["presale"], 20_000_000 * DECIMALS);
    assert_eq!(totals["liquidity"], 10_000_000 * DECIMALS);
    assert_eq!(totals["onboarding"], 10_000_000 * DECIMALS);
    assert_eq!(totals["validator-rewards"], 20_000_000 * DECIMALS);
    ext().execute_with(|| {
        assert_eq!(Balances::total_issuance(), 100_000_000 * DECIMALS);
        assert_eq!(
            IssuanceCap::remaining_allowance(),
            Some(900_000_000 * DECIMALS)
        );
        fresh_genesis::validate_lifecycle();
        assert_eq!(StorageVersion::get::<Staking>(), StorageVersion::new(16));
        assert_eq!(
            StorageVersion::get::<ValidatorSecurity>(),
            StorageVersion::new(1)
        );
        assert_eq!(StorageVersion::get::<Nfts>(), StorageVersion::new(2));
        assert_eq!(Session::validators().len(), 4);
        assert!(SecurityBudget::active());
        assert_eq!(
            SecurityBudget::remaining_reward_budget().unwrap(),
            20_000_000 * DECIMALS
        );
        let before = sp_io::storage::root(StateVersion::V1);
        <v13_migration::V13Migration as OnRuntimeUpgrade>::on_runtime_upgrade();
        <v14_migration::V14Migration as OnRuntimeUpgrade>::on_runtime_upgrade();
        assert_eq!(sp_io::storage::root(StateVersion::V1), before);
    });
}
#[test]
fn invalid_roles_and_production_development_keys_fail_closed() {
    let mut inputs = Inputs::synthetic();
    inputs.authorities[1].babe = inputs.authorities[0].babe;
    assert!(inputs.genesis().is_err());
    let mut inputs = Inputs::synthetic();
    inputs.beneficiaries[1] = inputs.beneficiaries[0].clone();
    assert!(inputs.genesis().is_err());
    let mut inputs = Inputs::synthetic();
    inputs.development = false;
    inputs.founders = FounderCustodySigners::get();
    assert!(inputs.genesis().is_err());
}
#[test]
fn founding_vesting_locks_every_base_unit_until_launch_relative_release() {
    ext().execute_with(|| {
        for (who, amount) in Inputs::synthetic()
            .beneficiaries
            .iter()
            .zip(upgrade13_policy::FOUNDING_ALLOCATION_TARGETS)
        {
            let schedules = pallet_vesting::Vesting::<Runtime>::get(who).unwrap();
            let locked = |block| {
                schedules
                    .iter()
                    .map(|s| s.locked_at::<sp_runtime::traits::ConvertInto>(block))
                    .sum::<u128>()
            };
            assert_eq!(locked(0), amount);
            assert_eq!(locked(1), amount);
            assert!(locked(5_256_000) > 0);
            assert_eq!(locked(5_256_001), 0);
            assert_eq!(System::account(who).data.frozen, amount);
            assert!(Balances::transfer_allow_death(
                RuntimeOrigin::signed(who.clone()),
                MultiAddress::Id(Inputs::synthetic().community),
                DECIMALS
            )
            .is_err());
        }
    });
}
#[test]
fn block_one_executes_signed_transfer_charges_fees_and_rejects_wrong_genesis() {
    ext().execute_with(|| {
        begin(1, H256::repeat_byte(0x14));
        let input = Inputs::synthetic();
        let before = Balances::free_balance(&input.community);
        let issuance = Balances::total_issuance();
        let call = RuntimeCall::Balances(pallet_balances::Call::transfer_keep_alive {
            dest: MultiAddress::Id(input.community.clone()),
            value: DECIMALS / 10,
        });
        let wrong = signed(
            call.clone(),
            "//RelaunchSudo",
            0,
            Some(H256::repeat_byte(0x13)),
        );
        assert!(Executive::apply_extrinsic(wrong).is_err());
        assert_eq!(System::account_nonce(&input.sudo), 0);
        assert_ok!(Executive::apply_extrinsic(signed(
            call,
            "//RelaunchSudo",
            0,
            None
        )));
        assert_eq!(
            Balances::free_balance(&input.community),
            before + DECIMALS / 10
        );
        assert_eq!(System::account_nonce(&input.sudo), 1);
        assert_eq!(Balances::total_issuance(), issuance);
        assert_eq!(
            IssuanceCap::remaining_allowance(),
            Some(900_000_000 * DECIMALS)
        );
        assert!(pallet_security_budget::UnallocatedFeeStaking::<Runtime>::get() > 0);
        Executive::finalize_block();
        fresh_genesis::validate_lifecycle();
    });
}
#[test]
fn lifetime_allowance_is_shared_burn_independent_and_uncontrolled_mint_calls_filtered() {
    ext().execute_with(|| {
        let who = Inputs::synthetic().community;
        assert_ok!(IssuanceCap::controlled_mint_into(&who, DECIMALS));
        assert_ok!(<Balances as Mutate<AccountId>>::burn_from(
            &who,
            DECIMALS,
            frame_support::traits::tokens::Preservation::Preserve,
            frame_support::traits::tokens::Precision::Exact,
            frame_support::traits::tokens::Fortitude::Polite
        ));
        assert_eq!(Balances::total_issuance(), 100_000_000 * DECIMALS);
        assert_eq!(
            IssuanceCap::remaining_allowance(),
            Some(899_999_999 * DECIMALS)
        );
        assert!(IssuanceCap::controlled_mint_into(&who, 900_000_000 * DECIMALS).is_err());
        let force = RuntimeCall::Balances(pallet_balances::Call::force_set_balance {
            who: MultiAddress::Id(who),
            new_free: 1_000_000_001 * DECIMALS,
        });
        assert!(!IssuanceCallFilter::contains(&force));
        assert!(!IssuanceCallFilter::contains(&RuntimeCall::Sudo(
            pallet_sudo::Call::sudo {
                call: Box::new(force)
            }
        )));
    });
}
#[test]
fn real_configured_session_and_era_transitions_preserve_babe_api_and_allocate_rewards() {
    ext().execute_with(|| {
        let mut parent = H256::repeat_byte(0x14);
        let mut last_epoch = 0;
        let mut last_session = 0;
        // Production timing: 100 slots/session, six sessions/era. No direct consensus-state edits.
        for n in 1..=1302 {
            begin(n, parent);
            let current = <Runtime as BabeApi<Block>>::current_epoch();
            let next = <Runtime as BabeApi<Block>>::next_epoch();
            assert_eq!(current, Babe::current_epoch());
            assert_eq!(next, Babe::next_epoch());
            assert_eq!(
                <Runtime as BabeApi<Block>>::current_epoch_start(),
                Babe::current_epoch_start()
            );
            assert_eq!(next.epoch_index, current.epoch_index + 1);
            assert_eq!(
                current.start_slot,
                sp_consensus_babe::Slot::from(300_000_037 + current.epoch_index * 100)
            );
            assert_eq!(current.config, Babe::epoch_config().unwrap());
            let config = <Runtime as BabeApi<Block>>::configuration();
            assert_eq!(config.c, current.config.c);
            assert_eq!(config.randomness, current.randomness);
            assert_eq!(config.authorities, current.authorities);
            assert!(current.epoch_index >= last_epoch);
            last_epoch = current.epoch_index;
            assert!(Session::current_index() >= last_session);
            last_session = Session::current_index();
            parent = Executive::finalize_block().hash();
            assert!(
                !pallet_security_budget::AllocationPaused::<Runtime>::get(),
                "paused at block {n}: {:?}",
                System::events()
            );
        }
        assert!(last_session >= 12);
        assert!(pallet_staking::ActiveEra::<Runtime>::get().unwrap().index >= 2);
        assert!(pallet_security_budget::CommittedLiabilities::<Runtime>::get() > 0);
        assert_eq!(IssuanceCap::remaining_allowance().unwrap(), 900_000_000 * DECIMALS);
        assert!(SecurityBudget::principal_reserved_total() > 0);
        assert!(pallet_security_budget::EraBudgets::<Runtime>::contains_key(
            0
        ));
        let input = Inputs::synthetic();
        let validator = Session::validators()[0].clone();
        assert_ok!(SecurityBudget::claim_reward_page(
            RuntimeOrigin::signed(input.community),
            0,
            validator,
            0
        ));
        assert!(pallet_security_budget::StakingPaidTotal::<Runtime>::get() > 0);
        fresh_genesis::validate_lifecycle();
    });
}

#[test]
fn ordinary_custody_requires_three_real_synthetic_signatures_and_preserves_categories() {
    use era_v14_custody_governance::CustodyCategory;
    ext().execute_with(|| {
        begin(1, H256::repeat_byte(0x14));
        let inputs = Inputs::synthetic();
        let category = CustodyCategory::Presale;
        let source = FounderCustody::custody_account(category);
        let ecosystem = FounderCustody::custody_account(CustodyCategory::Ecosystem);
        let before = Balances::free_balance(&source);
        let other = Balances::free_balance(&ecosystem);
        let call =
            RuntimeCall::FounderCustody(era_v14_custody_governance::Call::approve_withdrawal {
                category,
                request_id: 0,
                destination: inputs.community.clone(),
                amount: DECIMALS,
            });
        assert!(FounderCustody::approve_withdrawal(
            RuntimeOrigin::signed(inputs.sudo),
            category,
            0,
            inputs.community.clone(),
            DECIMALS
        )
        .is_err());
        for (i, uri) in [
            "//RelaunchFounder1",
            "//RelaunchFounder2",
            "//RelaunchFounder3",
        ]
        .iter()
        .enumerate()
        {
            assert_ok!(Executive::apply_extrinsic(signed(
                call.clone(),
                uri,
                0,
                None
            )));
            assert_eq!(
                Balances::free_balance(&source),
                if i < 2 { before } else { before - DECIMALS }
            );
        }
        assert_eq!(Balances::free_balance(&ecosystem), other);
        assert_eq!(Balances::total_issuance(), 100_000_000 * DECIMALS);
        assert!(FounderCustody::approve_withdrawal(
            RuntimeOrigin::signed(inputs.founders[0].clone()),
            category,
            0,
            inputs.community,
            DECIMALS
        )
        .is_err());
        Executive::finalize_block();
    });
}

#[test]
fn already_public_old_network_signed_upgrade_is_rejected_without_effects() {
    use codec::Decode;
    let text = include_str!("fixtures/old-network-signed-extrinsic.hex")
        .trim()
        .trim_start_matches("0x");
    let bytes: Vec<u8> = text
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(core::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect();
    let tx = UncheckedExtrinsic::decode(&mut &bytes[..])
        .expect("public historical signed fixture decodes");
    ext().execute_with(|| {
        begin(1, H256::repeat_byte(0x14));
        let before = sp_io::storage::root(StateVersion::V1);
        assert!(Executive::apply_extrinsic(tx).is_err());
        assert_eq!(sp_io::storage::root(StateVersion::V1), before);
        Executive::finalize_block();
    });
}

#[cfg(feature = "try-runtime")]
#[test]
fn optional_upgrade_verification_respects_fresh_lifecycle() {
    ext().execute_with(|| {
        let before = sp_io::storage::root(StateVersion::V1);
        let a = <v13_migration::V13Migration as OnRuntimeUpgrade>::pre_upgrade().unwrap();
        let b = <v14_migration::V14Migration as OnRuntimeUpgrade>::pre_upgrade().unwrap();
        <v13_migration::V13Migration as OnRuntimeUpgrade>::on_runtime_upgrade();
        <v14_migration::V14Migration as OnRuntimeUpgrade>::on_runtime_upgrade();
        <v13_migration::V13Migration as OnRuntimeUpgrade>::post_upgrade(a).unwrap();
        <v14_migration::V14Migration as OnRuntimeUpgrade>::post_upgrade(b).unwrap();
        assert_eq!(sp_io::storage::root(StateVersion::V1), before);
    });
}
