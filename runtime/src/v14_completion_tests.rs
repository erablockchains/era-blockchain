//! Final V14 synthetic application acceptance. No production keys, state or submissions.
use super::*;
use codec::{Decode, Encode};
use frame_support::{assert_ok, traits::StorageVersion};
use sp_runtime::{BuildStorage, MultiAddress, StateVersion};

fn account(id: u8) -> AccountId {
    AccountId::new([id; 32])
}
fn signed(id: u8) -> RuntimeOrigin {
    RuntimeOrigin::signed(account(id))
}
fn address(id: u8) -> MultiAddress<AccountId, ()> {
    MultiAddress::Id(account(id))
}
fn root() -> Vec<u8> {
    sp_io::storage::root(StateVersion::V1)
}
fn ext() -> sp_io::TestExternalities {
    let mut storage = frame_system::GenesisConfig::<Runtime>::default()
        .build_storage()
        .unwrap();
    pallet_balances::GenesisConfig::<Runtime> {
        balances: (1..=4).map(|id| (account(id), 1000 * DECIMALS)).collect(),
        dev_accounts: None,
    }
    .assimilate_storage(&mut storage)
    .unwrap();
    let mut ext = sp_io::TestExternalities::new(storage);
    ext.execute_with(|| {
        System::set_block_number(100);
        StorageVersion::new(1).put::<ValidatorSecurity>();
    });
    ext
}
fn items() {
    assert_ok!(Nfts::create(signed(1), address(1), Default::default()));
    assert_ok!(Nfts::mint(signed(1), 0, 1, address(1), None));
    assert_ok!(Nfts::mint(signed(1), 0, 2, address(2), None));
}
#[test]
fn nft_trade_ownership_price_permission_and_conservation() {
    ext().execute_with(|| {
        items();
        let issuance = Balances::total_issuance();
        let before = root();
        assert!(Nfts::set_price(signed(2), 0, 1, Some(5 * DECIMALS), None).is_err());
        assert_eq!(root(), before);
        assert_ok!(Nfts::set_price(
            signed(1),
            0,
            1,
            Some(5 * DECIMALS),
            Some(address(2))
        ));
        for (buyer, bid) in [(3, 5 * DECIMALS), (2, 4 * DECIMALS)] {
            let before = root();
            assert!(Nfts::buy_item(signed(buyer), 0, 1, bid).is_err());
            assert_eq!(root(), before);
        }
        let seller = Balances::free_balance(account(1));
        let buyer = Balances::free_balance(account(2));
        assert_ok!(Nfts::buy_item(signed(2), 0, 1, 6 * DECIMALS));
        assert_eq!(Nfts::owner(0, 1), Some(account(2)));
        assert_eq!(Balances::free_balance(account(1)), seller + 5 * DECIMALS);
        assert_eq!(Balances::free_balance(account(2)), buyer - 5 * DECIMALS);
        assert_eq!(Balances::total_issuance(), issuance);
        let before = root();
        assert!(Nfts::buy_item(signed(3), 0, 1, 6 * DECIMALS).is_err());
        assert_eq!(root(), before);
    });
}
#[test]
fn nft_swap_transfers_both_items_and_exact_price_once() {
    ext().execute_with(|| {
        items();
        let issuance = Balances::total_issuance();
        let price = pallet_nfts::PriceWithDirection {
            amount: 5 * DECIMALS,
            direction: pallet_nfts::PriceDirection::Receive,
        };
        assert_ok!(Nfts::create_swap(
            signed(1),
            0,
            1,
            0,
            Some(2),
            Some(price.clone()),
            2
        ));
        let before = root();
        assert!(Nfts::claim_swap(signed(2), 0, 2, 0, 1, None).is_err());
        assert_eq!(root(), before);
        let seller = Balances::free_balance(account(1));
        let buyer = Balances::free_balance(account(2));
        System::set_block_number(102);
        assert_ok!(Nfts::claim_swap(signed(2), 0, 2, 0, 1, Some(price.clone())));
        assert_eq!(Nfts::owner(0, 1), Some(account(2)));
        assert_eq!(Nfts::owner(0, 2), Some(account(1)));
        assert_eq!(Balances::free_balance(account(1)), seller + 5 * DECIMALS);
        assert_eq!(Balances::free_balance(account(2)), buyer - 5 * DECIMALS);
        assert_eq!(Balances::total_issuance(), issuance);
        let before = root();
        assert!(Nfts::claim_swap(signed(1), 0, 2, 0, 1, Some(price)).is_err());
        assert_eq!(root(), before);
    });
}
#[test]
fn nft_swap_second_transfer_failure_rolls_back_first_transfer_and_payment() {
    ext().execute_with(|| {
        items();
        let price = pallet_nfts::PriceWithDirection {
            amount: 5 * DECIMALS,
            direction: pallet_nfts::PriceDirection::Receive,
        };
        assert_ok!(Nfts::create_swap(
            signed(1),
            0,
            1,
            0,
            Some(2),
            Some(price.clone()),
            2
        ));
        assert_ok!(Nfts::lock_item_transfer(signed(1), 0, 1));
        let before = root();
        assert!(Nfts::claim_swap(signed(2), 0, 2, 0, 1, Some(price)).is_err());
        assert_eq!(root(), before);
        assert_eq!(Nfts::owner(0, 1), Some(account(1)));
        assert_eq!(Nfts::owner(0, 2), Some(account(2)));
    });
}
#[test]
fn nft_swap_expiration_and_permissionless_expired_cleanup() {
    ext().execute_with(|| {
        items();
        assert_ok!(Nfts::create_swap(signed(1), 0, 1, 0, Some(2), None, 2));
        let before = root();
        assert!(Nfts::cancel_swap(signed(3), 0, 1).is_err());
        assert_eq!(root(), before);
        System::set_block_number(103);
        let before = root();
        assert!(Nfts::claim_swap(signed(2), 0, 2, 0, 1, None).is_err());
        assert_eq!(root(), before);
        assert_ok!(Nfts::cancel_swap(signed(3), 0, 1));
        assert_eq!(Nfts::owner(0, 1), Some(account(1)));
        assert_eq!(Nfts::owner(0, 2), Some(account(2)));
    });
}

fn amm_asset(id: u32) {
    assert_ok!(Assets::create(signed(1), id, address(1), 1));
    for who in 1..=3 {
        assert_ok!(Assets::mint(
            signed(1),
            id,
            address(who),
            100_000 * DECIMALS
        ));
    }
}
#[test]
fn amm_registry_and_activation_are_fail_closed_without_pairs() {
    use era_v14_amm::v1::Asset::{Native, Registered};
    ext().execute_with(|| {
        let before = root();
        assert!(Amm::activate(RuntimeOrigin::root()).is_err());
        assert_eq!(root(), before);
        amm_asset(7);
        let before = root();
        assert!(Amm::approve_pair(signed(1), Native, Registered(7)).is_err());
        assert_eq!(root(), before);
        let before = root();
        assert!(Amm::create_pool(signed(1), Native, Registered(7), 110).is_err());
        assert_eq!(root(), before);
        assert_ok!(Amm::approve_pair(
            RuntimeOrigin::root(),
            Native,
            Registered(7)
        ));
        assert_ok!(Amm::activate(RuntimeOrigin::root()));
        let before = root();
        assert!(Amm::create_pool(signed(1), Native, Registered(8), 110).is_err());
        assert_eq!(root(), before);
    });
}
#[test]
fn amm_native_registered_pool_liquidity_swaps_removal_conserve_issuance() {
    use era_v14_amm::v1::{
        Api,
        Asset::{Native, Registered},
        EraV14AmmApiV1, PoolId,
    };
    ext().execute_with(|| {
        amm_asset(7);
        let native = Balances::total_issuance();
        let supply = Assets::total_supply(7);
        assert_ok!(Amm::approve_pair(
            RuntimeOrigin::root(),
            Native,
            Registered(7)
        ));
        assert_ok!(Amm::activate(RuntimeOrigin::root()));
        let reserved = Balances::reserved_balance(account(1));
        assert_ok!(Amm::create_pool(signed(1), Native, Registered(7), 110));
        assert_eq!(
            Balances::reserved_balance(account(1)),
            reserved + 10 * DECIMALS
        );
        let before = root();
        assert!(Amm::create_pool(signed(1), Native, Registered(7), 110).is_err());
        assert_eq!(root(), before);
        assert_ok!(Amm::add_liquidity(
            signed(1),
            Native,
            Registered(7),
            100 * DECIMALS,
            1000 * DECIMALS,
            100 * DECIMALS,
            1000 * DECIMALS,
            110
        ));
        assert_ok!(Amm::add_liquidity(
            signed(2),
            Native,
            Registered(7),
            10 * DECIMALS,
            100 * DECIMALS,
            10 * DECIMALS,
            100 * DECIMALS,
            110
        ));
        let pool = PoolId::new(Native, Registered(7)).unwrap();
        let p = Api::<Runtime>::pool_v1(pool).unwrap();
        assert_eq!(p.record.reserve_0, 110 * DECIMALS);
        assert_eq!(p.record.reserve_1, 1100 * DECIMALS);
        assert_eq!(p.record.locked_lp, 1000);
        let q = Api::<Runtime>::quote_exact_input_v1(Native, Registered(7), DECIMALS).unwrap();
        let before = root();
        assert!(Amm::swap_exact_input(
            signed(3),
            Native,
            Registered(7),
            DECIMALS,
            q.amount_out + 1,
            account(3),
            110
        )
        .is_err());
        assert_eq!(root(), before);
        let before_out = Assets::balance(7, account(3));
        assert_ok!(Amm::swap_exact_input(
            signed(3),
            Native,
            Registered(7),
            DECIMALS,
            q.amount_out,
            account(3),
            110
        ));
        assert_eq!(Assets::balance(7, account(3)), before_out + q.amount_out);
        let q =
            Api::<Runtime>::quote_exact_output_v1(Registered(7), Native, DECIMALS / 10).unwrap();
        assert_ok!(Amm::swap_exact_output(
            signed(3),
            Registered(7),
            Native,
            DECIMALS / 10,
            q.amount_in,
            account(3),
            110
        ));
        let lp = EraV14Amm::lp((Native, Registered(7)), &account(2)).unwrap();
        assert_ok!(Amm::remove_liquidity(
            signed(2),
            Native,
            Registered(7),
            lp,
            0,
            0,
            account(2),
            110
        ));
        assert_eq!(
            EraV14Amm::lp((Native, Registered(7)), &account(2)).unwrap(),
            0
        );
        assert_eq!(Balances::total_issuance(), native);
        assert_eq!(Assets::total_supply(7), supply);
        assert_eq!(
            Balances::reserved_balance(account(1)),
            reserved + 10 * DECIMALS
        ); // No refund/destruction route.
        assert_eq!(Api::<Runtime>::pools_v1(None, 64).unwrap().entries.len(), 1);
        assert!(Api::<Runtime>::pools_v1(None, 65).is_err());
    });
}
#[test]
fn amm_registered_pair_atomic_failure_and_deadline() {
    use era_v14_amm::v1::Asset::Registered;
    ext().execute_with(|| {
        amm_asset(7);
        amm_asset(8);
        assert_ok!(Amm::approve_pair(
            RuntimeOrigin::root(),
            Registered(7),
            Registered(8)
        ));
        assert_ok!(Amm::activate(RuntimeOrigin::root()));
        assert_ok!(Amm::create_pool(
            signed(1),
            Registered(7),
            Registered(8),
            110
        ));
        let before = root();
        assert!(Amm::add_liquidity(
            signed(1),
            Registered(7),
            Registered(8),
            100 * DECIMALS,
            200_000 * DECIMALS,
            0,
            0,
            110
        )
        .is_err());
        assert_eq!(root(), before);
        assert_ok!(Amm::add_liquidity(
            signed(1),
            Registered(7),
            Registered(8),
            100 * DECIMALS,
            100 * DECIMALS,
            0,
            0,
            110
        ));
        System::set_block_number(111);
        let before = root();
        assert!(Amm::swap_exact_input(
            signed(2),
            Registered(7),
            Registered(8),
            DECIMALS,
            0,
            account(2),
            110
        )
        .is_err());
        assert_eq!(root(), before);
    });
}

#[cfg(not(feature = "runtime-benchmarks"))]
fn penalty_fixture() -> (Vec<BabeObservationEvidence>, GrandpaObservationEvidence) {
    use frame_support::traits::{Currency, KeyOwnerProofSystem};
    use sp_core::Pair;
    let treasury = AiPenaltyDestination::get();
    drop(Balances::make_free_balance_be(&treasury, 100 * DECIMALS));
    for id in [0xa0, 0xa1, 0xa2, 0xa3, 0xd0] {
        drop(Balances::make_free_balance_be(
            &account(id),
            50_000 * DECIMALS,
        ));
        assert_ok!(Staking::bond(
            signed(id),
            if id == 0xd0 { 30_000 } else { 10_000 } * DECIMALS,
            pallet_staking::RewardDestination::Stash
        ));
    }
    pallet_staking::ActiveEra::<Runtime>::put(pallet_staking::ActiveEraInfo {
        index: 0,
        start: Some(0),
    });
    pallet_staking::CurrentEra::<Runtime>::put(0);
    pallet_staking::ErasStartSessionIndex::<Runtime>::insert(0, 0);
    pallet_staking::BondedEras::<Runtime>::put(vec![(0, 0)]);
    pallet_staking::Invulnerables::<Runtime>::kill();
    pallet_staking::SlashRewardFraction::<Runtime>::put(Perbill::zero());
    pallet_staking::ErasStakersOverview::<Runtime>::insert(
        0,
        account(0xa0),
        sp_staking::PagedExposureMetadata {
            total: 40_000 * DECIMALS,
            own: 10_000 * DECIMALS,
            nominator_count: 1,
            page_count: 1,
        },
    );
    pallet_staking::ErasStakersPaged::<Runtime>::insert(
        (0, account(0xa0), 0),
        sp_staking::ExposurePage {
            page_total: 30_000 * DECIMALS,
            others: vec![pallet_staking::IndividualExposure {
                who: account(0xd0),
                value: 30_000 * DECIMALS,
            }],
        },
    );
    // Synthetic recommendation A. Direct storage setup is not an activation extrinsic.
    v14_penalties::Policy::<Runtime>::put(v14_penalties::PolicyV1 {
        maximum_fraction_ppb: 10_000_000,
        activation_session: 0,
    });
    let babe = super::tests::r3b_valid_babe_reports(3);
    let pair = sp_consensus_grandpa::AuthorityPair::from_seed(&[0xc0; 32]);
    pallet_session::CurrentIndex::<Runtime>::put(0);
    let owner = Historical::prove((sp_consensus_grandpa::KEY_TYPE, pair.public())).unwrap();
    pallet_session::CurrentIndex::<Runtime>::put(1);
    pallet_grandpa::SetIdSession::<Runtime>::insert(0, 0);
    let vote = |marker| finality_grandpa::Prevote {
        target_hash: H256::repeat_byte(marker),
        target_number: 1,
    };
    let sign = |v: &finality_grandpa::Prevote<Hash, BlockNumber>| {
        pair.sign(&sp_consensus_grandpa::localized_payload(
            1,
            0,
            &finality_grandpa::Message::Prevote(v.clone()),
        ))
    };
    let first = vote(1);
    let second = vote(2);
    let grandpa = (
        sp_consensus_grandpa::EquivocationProof::new(
            0,
            sp_consensus_grandpa::Equivocation::Prevote(finality_grandpa::Equivocation {
                round_number: 1,
                identity: pair.public(),
                first: (first.clone(), sign(&first)),
                second: (second.clone(), sign(&second)),
            }),
        ),
        owner,
    );
    (babe, grandpa)
}
#[cfg(not(feature = "runtime-benchmarks"))]
#[test]
fn penalty_verified_protocols_share_historical_cap_and_duplicates_do_not_multiply() {
    use sp_staking::offence::OffenceReportSystem;
    ext().execute_with(|| {
        let (reports, grandpa) = penalty_fixture();
        let validators = Session::validators();
        assert_ok!(BabeObserveOnlyReportSystem::process_evidence(
            None,
            reports[0].clone()
        ));
        let queued = pallet_staking::UnappliedSlashes::<Runtime>::get(24);
        assert_eq!(queued.len(), 1);
        assert_eq!(queued[0].own, 100 * DECIMALS);
        assert_eq!(queued[0].others, vec![(account(0xd0), 300 * DECIMALS)]);
        assert_eq!(queued[0].reporters.len(), 0);
        // A distinct slot and the other protocol share the maximum in the same offence era.
        assert_ok!(BabeObserveOnlyReportSystem::process_evidence(
            None,
            reports[1].clone()
        ));
        assert_ok!(GrandpaObserveOnlyReportSystem::process_evidence(
            None,
            grandpa.clone()
        ));
        let count = pallet_staking::UnappliedSlashes::<Runtime>::get(24).len();
        assert_eq!(count, 1);
        let _ = BabeObserveOnlyReportSystem::process_evidence(None, reports[0].clone());
        let _ = GrandpaObserveOnlyReportSystem::process_evidence(None, grandpa);
        assert_eq!(
            pallet_staking::UnappliedSlashes::<Runtime>::get(24).len(),
            1
        );
        assert_eq!(Session::validators(), validators);
    });
}
#[cfg(not(feature = "runtime-benchmarks"))]
#[test]
fn penalty_invalid_proof_and_expired_queue_leave_state_unchanged() {
    use sp_staking::offence::OffenceReportSystem;
    ext().execute_with(|| {
        let (reports, _) = penalty_fixture();
        let mut invalid = reports[0].clone();
        invalid.0.second_header = invalid.0.first_header.clone();
        let before = root();
        assert!(BabeObserveOnlyReportSystem::process_evidence(None, invalid).is_err());
        assert_eq!(root(), before);
        pallet_staking::ActiveEra::<Runtime>::put(pallet_staking::ActiveEraInfo {
            index: 24,
            start: Some(0),
        });
        pallet_staking::ErasStartSessionIndex::<Runtime>::insert(24, 144);
        let before = root();
        assert!(BabeObserveOnlyReportSystem::process_evidence(None, reports[0].clone()).is_err());
        assert_eq!(root(), before);
        assert!(pallet_staking::UnappliedSlashes::<Runtime>::get(24).is_empty());
    });
}
#[cfg(not(feature = "runtime-benchmarks"))]
#[test]
fn penalty_root_cancellation_preserves_maximum_and_cannot_be_replayed() {
    use sp_staking::offence::OffenceReportSystem;
    ext().execute_with(|| {
        let (reports, grandpa) = penalty_fixture();
        assert_ok!(BabeObserveOnlyReportSystem::process_evidence(
            None,
            reports[0].clone()
        ));
        let before = root();
        assert!(Staking::cancel_deferred_slash(signed(1), 24, vec![0]).is_err());
        assert_eq!(root(), before);
        assert_ok!(Staking::cancel_deferred_slash(
            RuntimeOrigin::root(),
            24,
            vec![0]
        ));
        assert!(pallet_staking::UnappliedSlashes::<Runtime>::get(24).is_empty());
        assert_eq!(
            pallet_staking::ValidatorSlashInEra::<Runtime>::get(0, account(0xa0))
                .unwrap()
                .0,
            Perbill::from_percent(1)
        );
        assert_ok!(BabeObserveOnlyReportSystem::process_evidence(
            None,
            reports[1].clone()
        ));
        assert_ok!(GrandpaObserveOnlyReportSystem::process_evidence(
            None, grandpa
        ));
        assert!(pallet_staking::UnappliedSlashes::<Runtime>::get(24).is_empty());
    });
}
#[cfg(not(feature = "runtime-benchmarks"))]
#[test]
fn penalty_deferred_enactment_conserves_issuance_and_reduces_historical_stakes() {
    use pallet_session::SessionManager;
    use sp_staking::offence::OffenceReportSystem;
    ext().execute_with(|| {
        let (reports, _) = penalty_fixture();
        let issuance = Balances::total_issuance();
        let treasury = AiPenaltyDestination::get();
        let before = Balances::free_balance(&treasury);
        let validators = Session::validators();
        assert_ok!(BabeObserveOnlyReportSystem::process_evidence(
            None,
            reports[0].clone()
        ));
        assert_eq!(Balances::free_balance(&treasury), before);
        pallet_staking::ActiveEra::<Runtime>::put(pallet_staking::ActiveEraInfo {
            index: 23,
            start: Some(0),
        });
        pallet_staking::ErasStartSessionIndex::<Runtime>::insert(24, 144);
        <Staking as SessionManager<AccountId>>::start_session(144);
        assert!(pallet_staking::UnappliedSlashes::<Runtime>::get(24).is_empty());
        assert_eq!(Balances::free_balance(&treasury), before + 400 * DECIMALS);
        assert_eq!(Balances::total_issuance(), issuance);
        assert_eq!(
            pallet_staking::Ledger::<Runtime>::get(account(0xa0))
                .unwrap()
                .active,
            9_900 * DECIMALS
        );
        assert_eq!(
            pallet_staking::Ledger::<Runtime>::get(account(0xd0))
                .unwrap()
                .active,
            29_700 * DECIMALS
        );
        assert_eq!(Session::validators(), validators);
    });
}
#[cfg(not(feature = "runtime-benchmarks"))]
#[test]
fn penalty_absent_or_future_policy_keeps_observation_only() {
    use sp_staking::offence::OffenceReportSystem;
    for absent in [false, true] {
        ext().execute_with(|| {
            let (reports, _) = penalty_fixture();
            if absent {
                v14_penalties::Policy::<Runtime>::kill();
            } else {
                v14_penalties::Policy::<Runtime>::put(v14_penalties::PolicyV1 {
                    maximum_fraction_ppb: 10_000_000,
                    activation_session: 2,
                });
            }
            assert_ok!(BabeObserveOnlyReportSystem::process_evidence(
                None,
                reports[0].clone()
            ));
            assert!(pallet_staking::UnappliedSlashes::<Runtime>::get(24).is_empty());
            assert_eq!(
                era_validator_security::observation::ObservationCount::<Runtime>::get(),
                1
            );
        });
    }
}

#[test]
fn sdk_all_25_current_calls_decode_in_the_actual_runtime_and_keep_existing_indices() {
    let vectors: serde_json::Value = serde_json::from_str(include_str!(
        "../../clients/native-sdk/fixtures/runtime-call-vectors.json"
    ))
    .unwrap();
    let bytes = |value: &serde_json::Value| -> Vec<u8> {
        let h = value.as_str().unwrap().strip_prefix("0x").unwrap();
        (0..h.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&h[i..i + 2], 16).unwrap())
            .collect()
    };
    let calls = vectors["vectors"].as_array().unwrap();
    assert_eq!(calls.len(), 25);
    for vector in calls {
        let encoded = bytes(&vector["hex"]);
        let mut input = encoded.as_slice();
        let call = RuntimeCall::decode(&mut input)
            .expect("SDK call must decode with the real runtime codec");
        assert!(input.is_empty(), "{}", vector["method"]);
        assert_eq!(call.encode(), encoded);
    }
    let encoded = bytes(&vectors["extrinsic"]);
    let mut input = encoded.as_slice();
    let extrinsic = UncheckedExtrinsic::decode(&mut input).unwrap();
    assert!(input.is_empty());
    assert_eq!(extrinsic.encode(), encoded);
    // Codec compatibility does not make the old spec14 signature valid on spec15.
    assert_eq!(vectors["bindings"]["specVersion"].as_u64(), Some(14));
    assert_eq!(VERSION.spec_version, 15);
    use frame_support::traits::PalletInfo as _;
    assert_eq!(PalletInfo::index::<FounderCustody>(), Some(22));
    assert_eq!(PalletInfo::index::<FreshGenesis>(), Some(23));
    assert_eq!(PalletInfo::index::<Amm>(), Some(24));
    assert_eq!(PalletInfo::index::<EraV14Amm>(), Some(25));
    assert_eq!(PalletInfo::index::<EquivocationPenalties>(), Some(26));
}
#[cfg(not(feature = "runtime-benchmarks"))]
#[test]
fn penalty_policy_requires_root_future_cutoff_and_funded_destination() {
    ext().execute_with(|| {
        assert!(!v14_penalties::Policy::<Runtime>::exists());
        assert!(
            EquivocationPenalties::prepare_policy(RuntimeOrigin::root(), 10_000_000, 2).is_err()
        );
        let _ = penalty_fixture();
        v14_penalties::Policy::<Runtime>::kill();
        for (origin, cap, cutoff) in [
            (signed(1), 10_000_000, 2),
            (RuntimeOrigin::root(), 0, 2),
            (RuntimeOrigin::root(), 1_000_000_001, 2),
            (RuntimeOrigin::root(), 10_000_000, 1),
        ] {
            let before = root();
            assert!(EquivocationPenalties::prepare_policy(origin, cap, cutoff).is_err());
            assert_eq!(root(), before);
        }
        assert_ok!(EquivocationPenalties::prepare_policy(
            RuntimeOrigin::root(),
            10_000_000,
            2
        ));
        assert!(
            EquivocationPenalties::prepare_policy(RuntimeOrigin::root(), 50_000_000, 3).is_err()
        );
    });
}

#[cfg(not(feature = "runtime-benchmarks"))]
#[test]
fn penalty_numerical_alternatives_match_four_validator_worked_examples() {
    use sp_staking::offence::OffenceReportSystem;
    for (cap,own,nom) in [(10_000_000,100,300),(50_000_000,500,1500),(1_000_000_000,5625,16875)] {
        ext().execute_with(|| {
            let (reports,_)=penalty_fixture();
            v14_penalties::Policy::<Runtime>::put(v14_penalties::PolicyV1{maximum_fraction_ppb:cap,activation_session:0});
            assert_ok!(BabeObserveOnlyReportSystem::process_evidence(None,reports[0].clone()));
            let queue=pallet_staking::UnappliedSlashes::<Runtime>::get(24);
            assert_eq!(queue[0].own,own*DECIMALS);assert_eq!(queue[0].others,vec![(account(0xd0),nom*DECIMALS)]);
        });
    }
}
#[test]
fn amm_partial_pair_admission_is_atomic() {
    use era_v14_amm::v1::Asset::Registered;
    ext().execute_with(|| {
        amm_asset(7);
        let before=root();
        assert!(Amm::approve_pair(RuntimeOrigin::root(),Registered(7),Registered(999)).is_err());
        assert_eq!(root(),before);
    });
}

#[test]
fn development_market_sdk_vectors_match_native_runtime_calls() {
    use era_v14_amm::v1::Asset::{Native,Registered};
    let json:serde_json::Value=serde_json::from_str(include_str!("../../clients/native-sdk/fixtures/development-market-calls.json")).unwrap();
    let price=Some(pallet_nfts::PriceWithDirection{amount:5,direction:pallet_nfts::PriceDirection::Receive});
    let calls:Vec<(&str,RuntimeCall)>=vec![
      ("createPool",v14_amm::Call::create_pool{asset_a:Native,asset_b:Registered(7),deadline:110}.into()),
      ("addLiquidity",v14_amm::Call::add_liquidity{asset_a:Native,asset_b:Registered(7),desired_a:100,desired_b:200,min_a:1,min_b:2,deadline:110}.into()),
      ("removeLiquidity",v14_amm::Call::remove_liquidity{asset_a:Native,asset_b:Registered(7),lp:50,min_a:1,min_b:2,recipient:account(2),deadline:110}.into()),
      ("swapExactInput",v14_amm::Call::swap_exact_input{asset_in:Native,asset_out:Registered(7),amount_in:10,min_out:9,recipient:account(2),deadline:110}.into()),
      ("swapExactOutput",v14_amm::Call::swap_exact_output{asset_in:Native,asset_out:Registered(7),amount_out:9,max_in:10,recipient:account(2),deadline:110}.into()),
      ("setPrice",pallet_nfts::Call::set_price{collection:1,item:2,price:Some(5),whitelisted_buyer:Some(address(2))}.into()),
      ("buyItem",pallet_nfts::Call::buy_item{collection:1,item:2,bid_price:6}.into()),
      ("createSwap",pallet_nfts::Call::create_swap{offered_collection:1,offered_item:2,desired_collection:3,maybe_desired_item:Some(4),maybe_price:price.clone(),duration:100}.into()),
      ("cancelSwap",pallet_nfts::Call::cancel_swap{offered_collection:1,offered_item:2}.into()),
      ("claimSwap",pallet_nfts::Call::claim_swap{send_collection:3,send_item:4,receive_collection:1,receive_item:2,witness_price:price}.into()),
    ];
    for (name,call) in calls {let encoded="0x".to_owned()+&call.encode().iter().map(|b|format!("{b:02x}")).collect::<String>();assert_eq!(json[name].as_str().unwrap(),encoded,"{name}");}
}

#[test]
fn measured_amm_calls_fit_each_admission_dimension() {
    use frame_support::{dispatch::DispatchClass,traits::Get};
    let limits:frame_system::limits::BlockWeights=<Runtime as frame_system::Config>::BlockWeights::get();let normal=limits.get(DispatchClass::Normal);
    let capacity=normal.max_total.unwrap();let max_extrinsic=normal.max_extrinsic.unwrap();
    for call in [0,1,2,3,4,10,11] {
        let weight=v14_amm::measured_weight(call);
        assert!(weight.all_lte(Weight::from_parts(capacity.ref_time()/4,capacity.proof_size()/4)) && weight.all_lte(max_extrinsic),"call {call}: {weight:?}");
    }
}

#[test]
fn measured_penalty_reports_and_policy_fit_admission_with_policy_present_or_absent() {
    use frame_support::{dispatch::DispatchClass, traits::Get};
    ext().execute_with(|| {
        let limits:frame_system::limits::BlockWeights=<Runtime as frame_system::Config>::BlockWeights::get();
        let bound=limits.get(DispatchClass::Normal).max_extrinsic.unwrap();
        for n in [4,7,16,100,1000] {
            let a=<BabeObserveOnlyWeightAdapter<Runtime> as pallet_babe::WeightInfo>::report_equivocation(n,64);
            let b=<GrandpaObserveOnlyWeightAdapter<Runtime> as pallet_grandpa::WeightInfo>::report_equivocation(n,64);
            assert!(a.all_lte(bound) && b.all_lte(bound),"{n}: {a:?}, {b:?}; {bound:?}");
            v14_penalties::Policy::<Runtime>::put(v14_penalties::PolicyV1 {maximum_fraction_ppb:10_000_000,activation_session:0});
            assert_eq!(a,<BabeObserveOnlyWeightAdapter<Runtime> as pallet_babe::WeightInfo>::report_equivocation(n,64));
            assert_eq!(b,<GrandpaObserveOnlyWeightAdapter<Runtime> as pallet_grandpa::WeightInfo>::report_equivocation(n,64));
            v14_penalties::Policy::<Runtime>::kill();
        }
        assert!(v14_penalties::measured_prepare_weight().all_lte(bound));
    });
}
