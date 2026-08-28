use super::*;

use codec::{Decode, Encode};
use frame_election_provider_support::{
    DataProviderBounds, ElectionDataProvider, ElectionProvider, NposSolver, SortedListProvider,
};
use frame_support::{
    assert_noop, assert_ok,
    storage::{with_transaction, TransactionOutcome},
    traits::{
        fungible::Balanced, BeforeAllRuntimeMigrations, Contains, Currency, ExistenceRequirement,
        Get, GetStorageVersion, Hooks, InstanceFilter, IntegrityTest, NoStorageVersionSet,
        OnRuntimeUpgrade, PalletInfo as PalletInfoTrait, ReservableCurrency, StorageVersion,
        VestingSchedule,
    },
};
use pallet_session::SessionManager;
use pallet_staking::{
    EraPayout, EraRewardPoints, Forcing, IndividualExposure, StakerStatus, ValidatorPrefs,
};
use sp_consensus_babe::digests::{PreDigest, SecondaryPlainPreDigest};
use sp_core::{crypto::Ss58Codec, H256};
use sp_runtime::{
    traits::{AccountIdConversion, Convert, Dispatchable, TrailingZeroInput},
    BuildStorage, Digest, DigestItem, DispatchError, MultiAddress, Perbill, StateVersion,
};
use sp_staking::{
    currency_to_vote::{CurrencyToVote, U128CurrencyToVote},
    ExposurePage, PagedExposureMetadata,
};
use std::collections::BTreeMap;

const LIVE_ERA: sp_staking::EraIndex = 1_703;
const LIVE_ERA_START_SESSION: sp_staking::SessionIndex = 100;
const RECOVERY_SESSION: sp_staking::SessionIndex = 106;

trait SameType<T> {}
impl<T> SameType<T> for T {}

fn assert_same_type<Left, Right>()
where
    Left: SameType<Right>,
{
}

#[derive(Clone)]
struct Fixture {
    validators: Vec<AccountId>,
    nominators: Vec<AccountId>,
    session_keys: Vec<(AccountId, SessionKeys)>,
}

fn account(id: u8) -> AccountId {
    AccountId::new([id; 32])
}

fn keys(id: u8) -> SessionKeys {
    SessionKeys {
        babe: sp_core::sr25519::Public::from_raw([id; 32]).into(),
        grandpa: sp_core::ed25519::Public::from_raw([id; 32]).into(),
    }
}

fn new_test_ext(
    validator_candidates: usize,
    nominators: usize,
    desired: u32,
    minimum: u32,
) -> (sp_io::TestExternalities, Fixture) {
    let validators: Vec<_> = (1..=validator_candidates)
        .map(|n| account(n as u8))
        .collect();
    let nominators_accounts: Vec<_> = (0..nominators)
        .map(|n| account(80u8.saturating_add(n as u8)))
        .collect();

    let mut balances = vec![(account(250), 40_000_000 * DECIMALS)];
    let mut stakers = Vec::new();
    for (index, validator) in validators.iter().enumerate() {
        let balance = (20_000 + index as u128 * 1_000) * DECIMALS;
        balances.push((validator.clone(), balance));
        stakers.push((
            validator.clone(),
            validator.clone(),
            (2_000 + index as u128 * 100) * DECIMALS,
            StakerStatus::Validator,
        ));
    }
    for nominator in &nominators_accounts {
        balances.push((nominator.clone(), 10_000 * DECIMALS));
        stakers.push((
            nominator.clone(),
            nominator.clone(),
            1_000 * DECIMALS,
            StakerStatus::Nominator(validators.iter().take(16).cloned().collect()),
        ));
    }

    let mut storage = frame_system::GenesisConfig::<Runtime>::default()
        .build_storage()
        .expect("system genesis storage");
    pallet_balances::GenesisConfig::<Runtime> {
        balances,
        dev_accounts: None,
    }
    .assimilate_storage(&mut storage)
    .expect("balances genesis storage");
    pallet_staking::GenesisConfig::<Runtime> {
        validator_count: desired,
        minimum_validator_count: minimum,
        invulnerables: validators.clone(),
        force_era: Forcing::NotForcing,
        slash_reward_fraction: Perbill::zero(),
        canceled_payout: 0,
        stakers,
        min_nominator_bond: 0,
        min_validator_bond: 0,
        max_validator_count: None,
        max_nominator_count: Some(64),
    }
    .assimilate_storage(&mut storage)
    .expect("staking genesis storage");

    let session_keys: Vec<_> = validators
        .iter()
        .enumerate()
        .map(|(index, validator)| (validator.clone(), keys((index + 1) as u8)))
        .collect();
    let fixture = Fixture {
        validators,
        nominators: nominators_accounts,
        session_keys,
    };
    let mut ext = sp_io::TestExternalities::new(storage);
    ext.execute_with(|| {
        System::set_block_number(1);
        let current = fixture
            .validators
            .iter()
            .take(4)
            .cloned()
            .collect::<Vec<_>>();
        pallet_session::Validators::<Runtime>::put(current);
        pallet_session::QueuedKeys::<Runtime>::put(
            fixture
                .session_keys
                .iter()
                .take(4)
                .cloned()
                .collect::<Vec<_>>(),
        );
        for (validator, session_keys) in &fixture.session_keys {
            pallet_session::NextKeys::<Runtime>::insert(validator, session_keys);
        }
    });
    (ext, fixture)
}

fn add_upgrade13_c2_pair_atomically(
    target: &AccountId,
    pair: upgrade13_policy::ExactC2VestingScheduleTerms,
) -> Result<(), DispatchError> {
    with_transaction(|| {
        let result = <Vesting as VestingSchedule<AccountId>>::add_vesting_schedule(
            target,
            pair.schedule_a.locked,
            pair.schedule_a.per_block,
            pair.schedule_a.starting_block,
        )
        .and_then(|_| {
            <Vesting as VestingSchedule<AccountId>>::add_vesting_schedule(
                target,
                pair.schedule_b.locked,
                pair.schedule_b.per_block,
                pair.schedule_b.starting_block,
            )
        });
        if result.is_ok() {
            TransactionOutcome::Commit(result)
        } else {
            TransactionOutcome::Rollback(result)
        }
    })
}

fn set_stalled_live_shape() {
    pallet_staking::CurrentEra::<Runtime>::put(LIVE_ERA);
    pallet_staking::ActiveEra::<Runtime>::put(pallet_staking::ActiveEraInfo {
        index: LIVE_ERA,
        start: Some(1_783_026_360_001),
    });
    pallet_staking::ErasStartSessionIndex::<Runtime>::insert(LIVE_ERA, LIVE_ERA_START_SESSION);
    pallet_staking::CurrentPlannedSession::<Runtime>::put(RECOVERY_SESSION - 1);
    pallet_session::CurrentIndex::<Runtime>::put(RECOVERY_SESSION - 1);
    pallet_staking::ForceEra::<Runtime>::put(Forcing::NotForcing);
    System::reset_events();
}

fn recover_next_era() -> Vec<AccountId> {
    <Staking as SessionManager<AccountId>>::new_session(RECOVERY_SESSION)
        .expect("bounded election should plan the next era")
}

fn has_staking_event(predicate: impl Fn(&pallet_staking::Event<Runtime>) -> bool) -> bool {
    System::events().iter().any(|record| match &record.event {
        RuntimeEvent::Staking(event) => predicate(event),
        _ => false,
    })
}

fn ai_storage_digest() -> [u8; 32] {
    let prefix = sp_io::hashing::twox_128(b"AiPredictions");
    let mut encoded = Vec::new();
    let mut cursor = prefix.to_vec();
    while let Some(key) = sp_io::storage::next_key(&cursor) {
        if !key.starts_with(&prefix) {
            break;
        }
        cursor = key.clone();
        key.encode_to(&mut encoded);
        sp_io::storage::get(&key)
            .unwrap_or_default()
            .encode_to(&mut encoded);
    }
    sp_io::hashing::blake2_256(&encoded)
}

fn author_block_with_babe_digest(block: BlockNumber, authority_index: u32) {
    let pre_digest = PreDigest::SecondaryPlain(SecondaryPlainPreDigest {
        authority_index,
        slot: (block as u64).into(),
    });
    let digest = Digest {
        logs: vec![DigestItem::PreRuntime(
            sp_consensus_babe::BABE_ENGINE_ID,
            pre_digest.encode(),
        )],
    };
    System::initialize(&block, &H256::zero(), &digest);
    <Authorship as Hooks<BlockNumber>>::on_initialize(block);
    <Authorship as Hooks<BlockNumber>>::on_finalize(block);
    let _ = System::finalize();
}

fn install_valid_points_and_exposure(era: sp_staking::EraIndex, validators: &[AccountId]) {
    let mut individual = BTreeMap::new();
    for validator in validators {
        individual.insert(validator.clone(), 20);
        pallet_staking::ErasStakersOverview::<Runtime>::insert(
            era,
            validator,
            PagedExposureMetadata {
                total: 1,
                own: 1,
                nominator_count: 0,
                page_count: 0,
            },
        );
    }
    pallet_staking::ErasRewardPoints::<Runtime>::insert(
        era,
        EraRewardPoints {
            total: 20 * validators.len() as u32,
            individual,
        },
    );
}

fn assert_cancellation_metadata_surface(
    types: &scale_info::PortableRegistry,
    pallet_index: u8,
    call_type: u32,
) {
    assert_eq!(pallet_index, 13);
    let call_type = types.resolve(call_type).expect("RewardReserve call type");
    let scale_info::TypeDef::Variant(call_variants) = &call_type.type_def else {
        panic!("RewardReserve call type is not a variant")
    };
    let cancellation = call_variants
        .variants
        .iter()
        .find(|variant| variant.name == "cancel_legacy_zero_point_liability")
        .expect("cancellation call metadata");
    assert_eq!(cancellation.index, 7);
    assert_eq!(cancellation.fields.len(), 1);
    let era = &cancellation.fields[0];
    assert_eq!(era.name.as_deref(), Some("era"));
    assert!(era
        .type_name
        .as_deref()
        .is_some_and(|name| name.ends_with("EraIndex")));
    let era_type = types.resolve(era.ty.id).expect("EraIndex type");
    assert!(matches!(
        era_type.type_def,
        scale_info::TypeDef::Primitive(scale_info::TypeDefPrimitive::U32)
    ));
    assert!(cancellation
        .fields
        .iter()
        .all(|field| field.name.as_deref() != Some("expected_liability")));
}

#[test]
fn runtime_identity_and_bounds_are_exact() {
    assert_eq!(VERSION.spec_name.as_ref(), "era");
    assert_eq!(VERSION.spec_version, 13);
    assert_eq!(VERSION.transaction_version, 1);
    assert_eq!(VERSION.state_version(), sp_runtime::StateVersion::V1);
    assert_eq!(MaxElectingVoters::get(), 64);
    assert_eq!(MaxElectableTargets::get(), 16);
    assert_eq!(MaxElectionBackersPerWinner::get(), 64);
    assert_eq!(MaxValidatorSet::get(), 16);

    let bounds = StakingElectionBounds::get();
    assert_eq!(bounds.voters.count.expect("voter count").0, 64);
    assert_eq!(bounds.voters.size.expect("voter size").0, 64 * 1024);
    assert_eq!(bounds.targets.count.expect("target count").0, 16);
    assert_eq!(bounds.targets.size.expect("target size").0, 4 * 1024);
}

#[test]
fn deprecation_replacements_preserve_runtime_types_apis_and_metadata() {
    type ExpectedSignedExtra = (
        frame_system::CheckNonZeroSender<Runtime>,
        frame_system::CheckSpecVersion<Runtime>,
        frame_system::CheckTxVersion<Runtime>,
        frame_system::CheckGenesis<Runtime>,
        frame_system::CheckMortality<Runtime>,
        frame_system::CheckNonce<Runtime>,
        frame_system::CheckWeight<Runtime>,
        pallet_transaction_payment::ChargeTransactionPayment<Runtime>,
    );

    assert_same_type::<<Runtime as frame_system::Config>::Block, Block>();
    assert_same_type::<<Block as sp_runtime::traits::Block>::Header, Header>();
    assert_same_type::<<Block as sp_runtime::traits::Block>::Extrinsic, UncheckedExtrinsic>();
    assert_same_type::<SignedExtra, ExpectedSignedExtra>();

    #[cfg(not(feature = "try-runtime"))]
    let expected_api_hash = [
        0x86, 0x78, 0xec, 0x76, 0x6b, 0x36, 0x0e, 0x11, 0xd5, 0xac, 0xcd, 0x6e, 0x3b, 0x17, 0xaf,
        0x37, 0xda, 0xe4, 0xdb, 0x89, 0xa5, 0x79, 0x74, 0x25, 0xf8, 0x32, 0x47, 0x6a, 0xb4, 0x08,
        0x30, 0x7b,
    ];
    #[cfg(feature = "try-runtime")]
    let expected_api_hash = [
        0xb1, 0xd3, 0xb5, 0x5e, 0x14, 0x5b, 0x55, 0x14, 0xb0, 0x38, 0xdb, 0xff, 0x33, 0xef, 0x4b,
        0xc8, 0x16, 0x3f, 0xef, 0x28, 0x9d, 0xc9, 0xc4, 0xcd, 0x3c, 0xc1, 0xc1, 0xcc, 0xb3, 0x06,
        0xf8, 0x1b,
    ];
    assert_eq!(
        sp_io::hashing::blake2_256(&VERSION.apis.encode()),
        expected_api_hash
    );
    // IssuanceCap remains at index 19. Its additive V13 completion-marker storage intentionally
    // changes metadata without moving any pallet or changing the runtime API set.
    #[cfg(not(feature = "try-runtime"))]
    let expected_metadata_hash = [
        0x98, 0x03, 0x13, 0x9b, 0xed, 0x8f, 0x75, 0xfa, 0x9e, 0x81, 0xfd, 0xf8, 0x7f, 0x99, 0xc7,
        0xdd, 0x1f, 0x04, 0xb9, 0x33, 0xa4, 0x34, 0x91, 0xf0, 0x10, 0x4c, 0x93, 0xe2, 0x92, 0x33,
        0x5e, 0xf3,
    ];
    #[cfg(feature = "try-runtime")]
    let expected_metadata_hash = [
        0xae, 0x08, 0xeb, 0x76, 0x58, 0xc0, 0x24, 0x86, 0x04, 0x68, 0x35, 0x54, 0xa6, 0x85, 0x5a,
        0x71, 0x3b, 0xad, 0x67, 0xbc, 0xb9, 0xc1, 0x06, 0x7e, 0x22, 0xd7, 0xa9, 0xe7, 0x7b, 0xee,
        0x2c, 0x87,
    ];
    assert_eq!(
        sp_io::hashing::blake2_256(&Runtime::metadata().encode()),
        expected_metadata_hash
    );
}

#[test]
fn historical_session_identification_remains_exposure_decodable_and_membership_exact() {
    type HistoricalFullIdentification =
        <Runtime as pallet_session::historical::Config>::FullIdentification;
    type HistoricalIdentificationOf =
        <Runtime as pallet_session::historical::Config>::FullIdentificationOf;

    assert_same_type::<HistoricalFullIdentification, pallet_staking::Exposure<AccountId, Balance>>(
    );
    assert_same_type::<HistoricalIdentificationOf, pallet_staking::DefaultExposureOf<Runtime>>();

    let exposure = pallet_staking::Exposure {
        total: 13 * DECIMALS,
        own: 8 * DECIMALS,
        others: vec![IndividualExposure {
            who: account(80),
            value: 5 * DECIMALS,
        }],
    };
    let encoded = exposure.encode();
    let mut input = encoded.as_slice();
    let decoded = HistoricalFullIdentification::decode(&mut input)
        .expect("historical Exposure bytes remain decodable");
    assert!(input.is_empty());
    assert_eq!(decoded, exposure);
    assert_eq!(decoded.encode(), encoded);

    let (mut ext, fixture) = new_test_ext(4, 0, 4, 4);
    ext.execute_with(|| {
        let present: Option<HistoricalFullIdentification> =
            HistoricalIdentificationOf::convert(fixture.validators[0].clone());
        assert_eq!(present, Some(Default::default()));
        assert_eq!(HistoricalIdentificationOf::convert(account(200)), None);
    });
}

#[test]
fn currency_to_vote_boundaries_are_exact_at_all_authorized_supply_anchors() {
    const TEN_THOUSAND_ETKN: Balance = 10_000 * DECIMALS;
    let cases = [
        (
            1_005_003_999_999_999_994_375_836_000u128,
            54_481_376u128,
            183_548_961_758_968u64,
            9_999_999_999_999_956_979_968u128,
            43_020_032u128,
            54_481_375u128,
            3_486_808_000u128,
        ),
        (
            100_000_000_000_000_000_000_000_000u128,
            5_421_010u128,
            1_844_674_700_839_880u64,
            9_999_999_999_999_997_878_800u128,
            2_121_200u128,
            5_421_009u128,
            346_944_576u128,
        ),
        (
            1_000_000_000_000_000_000_000_000_000u128,
            54_210_108u128,
            184_467_442_861_394u64,
            9_999_999_999_999_997_770_552u128,
            2_229_448u128,
            54_210_107u128,
            3_469_446_848u128,
        ),
    ];

    for (
        issuance,
        factor,
        expected_vote,
        expected_restored,
        expected_loss,
        loss_bound,
        page_bound,
    ) in cases
    {
        assert_eq!(U128CurrencyToVote::will_downscale(issuance), Some(true));
        assert_eq!(U128CurrencyToVote::to_vote(factor - 1, issuance), 0);
        assert_eq!(U128CurrencyToVote::to_vote(factor, issuance), 1);
        assert_eq!(U128CurrencyToVote::to_vote(2 * factor - 1, issuance), 1);
        assert_eq!(U128CurrencyToVote::to_vote(2 * factor, issuance), 2);
        assert_eq!(U128CurrencyToVote::to_currency(1, issuance), factor);

        let vote = U128CurrencyToVote::to_vote(TEN_THOUSAND_ETKN, issuance);
        let restored = U128CurrencyToVote::to_currency(vote.into(), issuance);
        assert_eq!(vote, expected_vote);
        assert_eq!(restored, expected_restored);
        assert_eq!(TEN_THOUSAND_ETKN - restored, expected_loss);
        assert!(TEN_THOUSAND_ETKN - restored <= loss_bound);
        assert_eq!(64 * loss_bound, page_bound);

        let first_saturated = u128::from(u64::MAX) * factor;
        assert_eq!(
            U128CurrencyToVote::to_vote(first_saturated - 1, issuance),
            u64::MAX - 1
        );
        assert_eq!(
            U128CurrencyToVote::to_vote(first_saturated, issuance),
            u64::MAX
        );
    }
}

#[test]
fn runtime_metadata_preserves_existing_indices_and_appends_custody_pallets() {
    let encoded_metadata = Runtime::metadata().encode();
    assert!(!encoded_metadata.is_empty());
    assert_eq!(<PalletInfo as PalletInfoTrait>::index::<System>(), Some(0));
    assert_eq!(
        <PalletInfo as PalletInfoTrait>::index::<Timestamp>(),
        Some(1)
    );
    assert_eq!(
        <PalletInfo as PalletInfoTrait>::index::<Balances>(),
        Some(2)
    );
    assert_eq!(
        <PalletInfo as PalletInfoTrait>::index::<TransactionPayment>(),
        Some(3)
    );
    assert_eq!(<PalletInfo as PalletInfoTrait>::index::<Sudo>(), Some(4));
    assert_eq!(<PalletInfo as PalletInfoTrait>::index::<Session>(), Some(5));
    assert_eq!(
        <PalletInfo as PalletInfoTrait>::index::<Historical>(),
        Some(6)
    );
    assert_eq!(<PalletInfo as PalletInfoTrait>::index::<Babe>(), Some(7));
    assert_eq!(<PalletInfo as PalletInfoTrait>::index::<Grandpa>(), Some(8));
    assert_eq!(
        <PalletInfo as PalletInfoTrait>::index::<Authorship>(),
        Some(9)
    );
    assert_eq!(
        <PalletInfo as PalletInfoTrait>::index::<Staking>(),
        Some(10)
    );
    assert_eq!(
        <PalletInfo as PalletInfoTrait>::index::<Vesting>(),
        Some(11)
    );
    assert_eq!(
        <PalletInfo as PalletInfoTrait>::index::<AiPredictions>(),
        Some(12)
    );
    assert_eq!(
        <PalletInfo as PalletInfoTrait>::index::<RewardReserve>(),
        Some(13)
    );
    assert_eq!(
        <PalletInfo as PalletInfoTrait>::name::<RewardReserve>(),
        Some("RewardReserve")
    );
    assert_eq!(
        <PalletInfo as PalletInfoTrait>::index::<Multisig>(),
        Some(14)
    );
    assert_eq!(<PalletInfo as PalletInfoTrait>::index::<Proxy>(), Some(15));
    assert_eq!(<PalletInfo as PalletInfoTrait>::index::<Assets>(), Some(16));
    assert_eq!(<PalletInfo as PalletInfoTrait>::index::<Nfts>(), Some(17));
    assert_eq!(
        <PalletInfo as PalletInfoTrait>::index::<EraWorlds>(),
        Some(18)
    );
    assert_eq!(
        <PalletInfo as PalletInfoTrait>::index::<IssuanceCap>(),
        Some(19)
    );
}

#[test]
fn category_proxy_filters_are_fail_closed_and_future_default_deny() {
    let remark = RuntimeCall::System(frame_system::Call::remark {
        remark: b"audit".to_vec(),
    });
    assert!(ProxyType::AuditOnly.filter(&remark));
    assert!(ProxyType::AuditOnly.is_superset(&ProxyType::Presale));

    let nested_calls = vec![
        RuntimeCall::System(frame_system::Call::set_code { code: vec![] }),
        RuntimeCall::Balances(pallet_balances::Call::force_set_balance {
            who: MultiAddress::Id(account(9)),
            new_free: 0,
        }),
        RuntimeCall::Sudo(pallet_sudo::Call::sudo {
            call: Box::new(remark.clone()),
        }),
        RuntimeCall::Staking(pallet_staking::Call::chill {}),
        RuntimeCall::Session(pallet_session::Call::purge_keys {}),
        RuntimeCall::Vesting(pallet_vesting::Call::vest {}),
        RuntimeCall::AiPredictions(pallet_ai_predictions::Call::authorize_validator {
            account: account(9),
        }),
        RuntimeCall::RewardReserve(
            pallet_reward_reserve::Call::cancel_legacy_zero_point_liability { era: 1 },
        ),
        RuntimeCall::Multisig(pallet_multisig::Call::approve_as_multi {
            threshold: 3,
            other_signatories: vec![account(2), account(3)],
            maybe_timepoint: None,
            call_hash: [0; 32],
            max_weight: Weight::from_parts(u64::MAX, u64::MAX),
        }),
        RuntimeCall::Proxy(pallet_proxy::Call::add_proxy {
            delegate: MultiAddress::Id(account(9)),
            proxy_type: ProxyType::AuditOnly,
            delay: 0,
        }),
    ];

    for proxy_type in [
        ProxyType::Presale,
        ProxyType::Ecosystem,
        ProxyType::Liquidity,
        ProxyType::Community,
    ] {
        assert!(!proxy_type.filter(&remark));
        assert!(!proxy_type.is_superset(&ProxyType::AuditOnly));
        assert!(nested_calls.iter().all(|call| !proxy_type.filter(call)));
    }
}

#[test]
fn threshold_three_multisig_derivation_and_deposit_are_exact() {
    let (mut ext, _) = new_test_ext(4, 0, 4, 4);
    ext.execute_with(|| {
        let signers = vec![account(1), account(2), account(3)];
        let expected: AccountId = Decode::decode(&mut TrailingZeroInput::new(
            &(b"modlpy/utilisuba", &signers, 3u16).using_encoded(sp_io::hashing::blake2_256),
        ))
        .expect("32-byte account derivation");
        let multisig = Multisig::multi_account_id(&signers, 3);
        assert_eq!(multisig, expected);
        assert_ne!(multisig, Multisig::multi_account_id(&signers, 2));

        let call = RuntimeCall::System(frame_system::Call::remark {
            remark: b"threshold-three-test".to_vec(),
        });
        let call_hash = call.using_encoded(sp_io::hashing::blake2_256);
        let max_weight = Weight::from_parts(u64::MAX, u64::MAX);
        let reserved_before = Balances::reserved_balance(account(1));
        assert_ok!(Multisig::as_multi(
            RuntimeOrigin::signed(account(1)),
            3,
            vec![account(2), account(3)],
            None,
            Box::new(call.clone()),
            max_weight,
        ));
        let operation = pallet_multisig::Multisigs::<Runtime>::get(&multisig, call_hash)
            .expect("open multisig");
        assert_eq!(
            operation.deposit,
            MultisigDepositBase::get() + 3 * MultisigDepositFactor::get()
        );
        assert_eq!(
            Balances::reserved_balance(account(1)),
            reserved_before + operation.deposit
        );

        assert_ok!(Multisig::as_multi(
            RuntimeOrigin::signed(account(2)),
            3,
            vec![account(1), account(3)],
            Some(operation.when),
            Box::new(call.clone()),
            max_weight,
        ));
        assert_ok!(Multisig::as_multi(
            RuntimeOrigin::signed(account(3)),
            3,
            vec![account(1), account(2)],
            Some(operation.when),
            Box::new(call),
            max_weight,
        ));
        assert!(!pallet_multisig::Multisigs::<Runtime>::contains_key(
            &multisig, call_hash
        ));
        assert_eq!(Balances::reserved_balance(account(1)), reserved_before);
    });
}

#[test]
fn pure_proxy_identity_binds_spawner_type_index_and_timepoint() {
    let (mut ext, _) = new_test_ext(4, 0, 4, 4);
    ext.execute_with(|| {
        let signers = vec![account(1), account(2), account(3)];
        let spawner = Multisig::multi_account_id(&signers, 3);
        assert_ok!(Balances::transfer_allow_death(
            RuntimeOrigin::signed(account(1)),
            MultiAddress::Id(spawner.clone()),
            10 * DECIMALS,
        ));
        System::set_block_number(7);
        System::set_extrinsic_index(11);

        let explicit = Proxy::pure_account(&spawner, &ProxyType::Presale, 0, Some((7, 11)));
        let entropy = (
            b"modlpy/proxy____",
            &spawner,
            7u32,
            11u32,
            ProxyType::Presale,
            0u16,
        )
            .using_encoded(sp_io::hashing::blake2_256);
        let independent: AccountId = Decode::decode(&mut TrailingZeroInput::new(entropy.as_ref()))
            .expect("32-byte account derivation");
        assert_eq!(explicit, independent);
        assert_eq!(
            explicit,
            Proxy::pure_account(&spawner, &ProxyType::Presale, 0, None)
        );
        assert_ne!(
            explicit,
            Proxy::pure_account(&spawner, &ProxyType::Presale, 0, Some((7, 12)))
        );
        assert_ne!(
            explicit,
            Proxy::pure_account(&spawner, &ProxyType::Ecosystem, 0, Some((7, 11)))
        );

        let reserved_before = Balances::reserved_balance(&spawner);
        assert_ok!(Proxy::create_pure(
            RuntimeOrigin::signed(spawner.clone()),
            ProxyType::Presale,
            PRESALE_PROXY_DELAY,
            0,
        ));
        let (definitions, deposit) = Proxy::proxies(explicit);
        assert_eq!(definitions.len(), 1);
        assert_eq!(definitions[0].delegate, spawner);
        assert_eq!(definitions[0].proxy_type, ProxyType::Presale);
        assert_eq!(definitions[0].delay, PRESALE_PROXY_DELAY);
        assert_eq!(deposit, ProxyDepositBase::get() + ProxyDepositFactor::get());
        assert_eq!(
            Balances::reserved_balance(&definitions[0].delegate),
            reserved_before + deposit
        );
    });
}

#[test]
fn zero_point_liability_cancellation_must_load_amount_from_storage() {
    let era: sp_staking::EraIndex = 1_782;
    let pallet_call =
        pallet_reward_reserve::Call::<Runtime>::cancel_legacy_zero_point_liability { era };
    let mut expected_pallet_encoding = vec![7u8];
    expected_pallet_encoding.extend(era.encode());
    assert_eq!(pallet_call.encode(), expected_pallet_encoding);

    let runtime_call = RuntimeCall::RewardReserve(pallet_call);
    let mut expected_runtime_encoding = vec![13u8];
    expected_runtime_encoding.extend(expected_pallet_encoding);
    assert_eq!(runtime_call.encode(), expected_runtime_encoding);

    match Runtime::metadata().1 {
        frame_metadata::RuntimeMetadata::V14(metadata) => {
            let pallet = metadata
                .pallets
                .iter()
                .find(|pallet| pallet.name == "RewardReserve")
                .expect("RewardReserve metadata");
            let call_type = pallet.calls.as_ref().expect("RewardReserve calls").ty.id;
            assert_cancellation_metadata_surface(&metadata.types, pallet.index, call_type);
        }
        frame_metadata::RuntimeMetadata::V15(metadata) => {
            let pallet = metadata
                .pallets
                .iter()
                .find(|pallet| pallet.name == "RewardReserve")
                .expect("RewardReserve metadata");
            let call_type = pallet.calls.as_ref().expect("RewardReserve calls").ty.id;
            assert_cancellation_metadata_surface(&metadata.types, pallet.index, call_type);
        }
        metadata => panic!("unexpected metadata version: {metadata:?}"),
    }
}

#[test]
fn runtime_in_code_storage_versions_are_expected() {
    assert_eq!(Multisig::in_code_storage_version(), StorageVersion::new(1));
    assert_same_type::<<Proxy as GetStorageVersion>::InCodeStorageVersion, NoStorageVersionSet>();
    assert_eq!(Assets::in_code_storage_version(), StorageVersion::new(1));
    assert_eq!(Nfts::in_code_storage_version(), StorageVersion::new(1));
    assert_eq!(EraWorlds::in_code_storage_version(), StorageVersion::new(1));
    assert_eq!(Staking::in_code_storage_version(), StorageVersion::new(16));
    assert_eq!(
        AiPredictions::in_code_storage_version(),
        StorageVersion::new(3)
    );
    assert_eq!(
        RewardReserve::in_code_storage_version(),
        StorageVersion::new(2)
    );
}

#[test]
fn four_validators_are_elected_with_bounded_backers() {
    let (mut ext, fixture) = new_test_ext(4, 6, 4, 4);
    ext.execute_with(|| {
        assert_eq!(Staking::validator_count(), 4);
        assert_eq!(Staking::minimum_validator_count(), 4);
        let supports = <StakingElectionProvider as ElectionProvider>::elect(0)
            .expect("four-validator election succeeds");
        assert_eq!(supports.0.len(), 4);
        assert!(supports
            .0
            .iter()
            .all(|(_, support)| support.voters.len() <= 64));
        let mut winners = supports
            .0
            .iter()
            .map(|(winner, _)| winner.clone())
            .collect::<Vec<_>>();
        let mut expected = fixture.validators.clone();
        winners.sort();
        expected.sort();
        assert_eq!(winners, expected);
    });
}

#[test]
fn stalled_live_shape_recovers_naturally_and_preserves_four() {
    let (mut ext, fixture) = new_test_ext(4, 6, 4, 4);
    ext.execute_with(|| {
        set_stalled_live_shape();
        let before_ai = ai_storage_digest();
        let elected = recover_next_era();
        assert_eq!(elected.len(), 4);
        assert_eq!(elected, fixture.validators);
        assert_eq!(Staking::current_era(), Some(LIVE_ERA + 1));
        assert!(has_staking_event(|event| matches!(
            event,
            pallet_staking::Event::StakersElected
        )));
        assert!(!has_staking_event(|event| matches!(
            event,
            pallet_staking::Event::StakingElectionFailed
        )));

        <Staking as SessionManager<AccountId>>::start_session(RECOVERY_SESSION);
        assert_eq!(
            Staking::active_era().map(|era| era.index),
            Some(LIVE_ERA + 1)
        );
        assert_eq!(pallet_session::Validators::<Runtime>::get().len(), 4);
        assert_eq!(Staking::force_era(), Forcing::NotForcing);
        assert_eq!(ai_storage_digest(), before_ai);
    });
}

#[test]
fn staking_state_is_preserved_by_election() {
    let (mut ext, fixture) = new_test_ext(4, 6, 4, 4);
    ext.execute_with(|| {
        set_stalled_live_shape();
        let bonded = fixture
            .validators
            .iter()
            .map(|validator| (validator.clone(), Staking::bonded(validator)))
            .collect::<Vec<_>>();
        let ledgers = fixture
            .validators
            .iter()
            .map(|validator| {
                (
                    validator.clone(),
                    pallet_staking::Ledger::<Runtime>::get(validator),
                )
            })
            .collect::<Vec<_>>();
        let preferences = fixture
            .validators
            .iter()
            .map(|validator| (validator.clone(), Staking::validators(validator)))
            .collect::<Vec<_>>();
        let nominations = fixture
            .nominators
            .iter()
            .map(|nominator| (nominator.clone(), Staking::nominators(nominator)))
            .collect::<Vec<_>>();

        recover_next_era();
        for (validator, value) in bonded {
            assert_eq!(Staking::bonded(&validator), value);
        }
        for (validator, value) in ledgers {
            assert_eq!(pallet_staking::Ledger::<Runtime>::get(&validator), value);
        }
        for (validator, value) in preferences {
            assert_eq!(Staking::validators(&validator), value);
        }
        for (nominator, value) in nominations {
            assert_eq!(Staking::nominators(&nominator), value);
        }
    });
}

#[test]
fn validator_replacement_selects_the_higher_backed_candidate() {
    let (mut ext, fixture) = new_test_ext(5, 6, 4, 4);
    ext.execute_with(|| {
        set_stalled_live_shape();
        let elected = recover_next_era();
        assert_eq!(elected.len(), 4);
        assert!(elected.contains(fixture.validators.last().expect("fifth validator")));
        assert!(elected
            .iter()
            .all(|validator| fixture.validators.contains(validator)));
    });
}

#[test]
fn three_candidates_fail_safely_without_emptying_session() {
    let (mut ext, fixture) = new_test_ext(3, 3, 4, 4);
    ext.execute_with(|| {
        set_stalled_live_shape();
        let mut existing = fixture.validators.clone();
        existing.push(account(4));
        pallet_session::Validators::<Runtime>::put(existing.clone());
        assert_eq!(existing.len(), 4);
        assert!(<Staking as SessionManager<AccountId>>::new_session(RECOVERY_SESSION).is_none());
        assert_eq!(Staking::current_era(), Some(LIVE_ERA));
        assert_eq!(pallet_session::Validators::<Runtime>::get(), existing);
        assert!(!pallet_session::Validators::<Runtime>::get().is_empty());
        assert!(has_staking_event(|event| matches!(
            event,
            pallet_staking::Event::StakingElectionFailed
        )));
    });
}

#[test]
fn voter_and_target_bounds_are_enforced() {
    let (mut voter_ext, _) = new_test_ext(4, 61, 4, 4);
    voter_ext.execute_with(|| {
        let voters = <Staking as ElectionDataProvider>::electing_voters(
            StakingElectionBounds::get().voters,
            0,
        )
        .expect("bounded voter collection");
        assert_eq!(voters.len(), 64);
    });

    let (mut target_ext, _) = new_test_ext(17, 0, 4, 4);
    target_ext.execute_with(|| {
        assert!(<Staking as ElectionDataProvider>::electable_targets(
            StakingElectionBounds::get().targets,
            0,
        )
        .is_err());
    });
}

#[test]
fn election_weight_fits_the_runtime_max_block() {
    type Solver = SequentialPhragmen<AccountId, Perbill>;
    type Weights = frame_election_provider_support::weights::SubstrateWeight<Runtime>;
    let election_weight = Solver::weight::<Weights>(64, 16, 16)
        .saturating_add(<() as pallet_staking::WeightInfo>::get_npos_voters(16, 48))
        .saturating_add(<() as pallet_staking::WeightInfo>::get_npos_targets(16));
    let max_block = <<Runtime as frame_system::Config>::BlockWeights as Get<
        frame_system::limits::BlockWeights,
    >>::get()
    .max_block;
    assert!(election_weight.ref_time() < max_block.ref_time());
    assert!(election_weight.proof_size() <= max_block.proof_size());
}

#[test]
fn zero_era_payout_is_unchanged() {
    let (validator_payout, remainder) =
        <() as EraPayout<Balance>>::era_payout(400_000 * DECIMALS, 1_000_000 * DECIMALS, 1_000);
    assert_eq!(validator_payout, 0);
    assert_eq!(remainder, 0);
}

#[test]
fn unbonding_unlocks_after_era_progression() {
    let (mut ext, fixture) = new_test_ext(4, 0, 4, 4);
    ext.execute_with(|| {
        pallet_staking::CurrentEra::<Runtime>::put(10);
        let validator = fixture.validators[0].clone();
        let before = pallet_staking::Ledger::<Runtime>::get(&validator).expect("staking ledger");
        assert_ok!(Staking::unbond(
            RuntimeOrigin::signed(validator.clone()),
            100 * DECIMALS
        ));
        let scheduled = pallet_staking::Ledger::<Runtime>::get(&validator).expect("updated ledger");
        assert_eq!(scheduled.unlocking.len(), 1);
        assert_eq!(scheduled.unlocking[0].era, 10 + BondingDuration::get());
        assert_eq!(scheduled.total, before.total);

        pallet_staking::CurrentEra::<Runtime>::put(10 + BondingDuration::get());
        assert_ok!(Staking::withdraw_unbonded(
            RuntimeOrigin::signed(validator.clone()),
            0
        ));
        let withdrawn =
            pallet_staking::Ledger::<Runtime>::get(&validator).expect("remaining ledger");
        assert!(withdrawn.unlocking.is_empty());
        assert_eq!(withdrawn.total, before.total - 100 * DECIMALS);
    });
}

#[test]
fn slash_defer_and_runtime_integrity_pass() {
    assert_eq!(BondingDuration::get(), 24);
    assert_eq!(SlashDeferDuration::get(), 23);
    assert!(SlashDeferDuration::get() < BondingDuration::get());
    <AllPalletsWithSystem as IntegrityTest>::integrity_test();
}

#[test]
fn session_keys_and_consensus_authorities_remain_complete() {
    let (mut ext, fixture) = new_test_ext(4, 6, 4, 4);
    ext.execute_with(|| {
        set_stalled_live_shape();
        let elected = recover_next_era();
        let elected_with_keys = elected
            .iter()
            .filter_map(|validator| {
                pallet_session::NextKeys::<Runtime>::get(validator)
                    .map(|session_keys| (validator.clone(), session_keys))
            })
            .collect::<Vec<_>>();
        assert_eq!(elected_with_keys.len(), 4);

        <Babe as frame_support::traits::OneSessionHandler<AccountId>>::on_genesis_session(
            elected_with_keys
                .iter()
                .map(|(validator, session_keys)| (validator, session_keys.babe.clone())),
        );
        <Grandpa as frame_support::traits::OneSessionHandler<AccountId>>::on_genesis_session(
            elected_with_keys
                .iter()
                .map(|(validator, session_keys)| (validator, session_keys.grandpa.clone())),
        );
        assert_eq!(pallet_babe::Authorities::<Runtime>::get().len(), 4);
        assert_eq!(pallet_grandpa::Authorities::<Runtime>::get().len(), 4);
        assert_eq!(pallet_session::QueuedKeys::<Runtime>::get().len(), 4);
        assert_eq!(fixture.session_keys.len(), 4);
    });
}

#[test]
fn historical_session_root_is_recorded_for_recovered_set() {
    type HistoricalManager = pallet_session::historical::NoteHistoricalRoot<Runtime, Staking>;
    let (mut ext, _) = new_test_ext(4, 6, 4, 4);
    ext.execute_with(|| {
        set_stalled_live_shape();
        let elected =
            <HistoricalManager as SessionManager<AccountId>>::new_session(RECOVERY_SESSION)
                .expect("historical wrapper returns elected validators");
        assert_eq!(elected.len(), 4);
        assert!(
            pallet_session::historical::HistoricalSessions::<Runtime>::contains_key(
                RECOVERY_SESSION
            )
        );
        assert_eq!(
            pallet_session::historical::StoredRange::<Runtime>::get(),
            Some((RECOVERY_SESSION, RECOVERY_SESSION + 1))
        );
    });
}

#[test]
fn data_provider_sources_match_runtime_staking_maps() {
    let (mut ext, fixture) = new_test_ext(4, 6, 4, 4);
    ext.execute_with(|| {
        assert_eq!(<Staking as ElectionDataProvider>::desired_targets(), Ok(4));
        assert_eq!(
            <Runtime as pallet_staking::Config>::VoterList::count(),
            (fixture.validators.len() + fixture.nominators.len()) as u32
        );
        assert_eq!(
            <Runtime as pallet_staking::Config>::TargetList::count(),
            fixture.validators.len() as u32
        );
        let unbounded = DataProviderBounds::default();
        assert_eq!(
            <Staking as ElectionDataProvider>::electable_targets(unbounded, 0)
                .expect("targets")
                .len(),
            4
        );
    });
}

fn route_synthetic_fee(
    payer: &AccountId,
    normal_fee: Balance,
    tip: Balance,
    author: Option<AccountId>,
) {
    use frame_support::traits::tokens::{Fortitude, Precision, Preservation};
    let fee_credit = <Balances as Balanced<AccountId>>::withdraw(
        payer,
        normal_fee,
        Precision::Exact,
        Preservation::Preserve,
        Fortitude::Polite,
    )
    .expect("synthetic corrected normal fee");
    let tip_credit = if tip == 0 {
        None
    } else {
        Some(
            <Balances as Balanced<AccountId>>::withdraw(
                payer,
                tip,
                Precision::Exact,
                Preservation::Preserve,
                Fortitude::Polite,
            )
            .expect("synthetic tip"),
        )
    };
    DealWithFees::route_credits(fee_credit, tip_credit, author);
}

fn fund_reward_pot(source: &AccountId, amount: Balance) {
    let pot = RewardReserve::reward_pot_account();
    assert_ok!(Balances::transfer_allow_death(
        RuntimeOrigin::signed(source.clone()),
        MultiAddress::Id(pot),
        amount,
    ));
}

fn prepare_and_activate_reward_system(source: &AccountId) {
    for (destination, amount) in [
        (
            RewardReserve::reward_pot_account(),
            InitialRewardReserve::get() + RewardPotSafetyFloor::get(),
        ),
        (
            RewardReserve::ecosystem_treasury_account(),
            TreasuryPotSafetyFloor::get(),
        ),
        (RewardReserve::fee_collection_account(), EXISTENTIAL_DEPOSIT),
    ] {
        assert_ok!(Balances::transfer_allow_death(
            RuntimeOrigin::signed(source.clone()),
            MultiAddress::Id(destination),
            amount,
        ));
    }
    if pallet_staking::ActiveEra::<Runtime>::get().is_none() {
        pallet_staking::ActiveEra::<Runtime>::put(pallet_staking::ActiveEraInfo {
            index: 10,
            start: Some(1_000_000),
        });
    }
    assert_ok!(RewardReserve::activate_rewards_and_fee_routing(
        RuntimeOrigin::root()
    ));
    assert!(RewardReserve::fee_destinations_ready());
}

fn install_single_page_reward(
    era: sp_staking::EraIndex,
    validator: &AccountId,
    nominators: &[(AccountId, Balance)],
    own: Balance,
    budget: Balance,
    commission: Perbill,
) {
    let nominator_total = nominators.iter().map(|(_, value)| *value).sum::<Balance>();
    let total = own + nominator_total;
    let mut individual = BTreeMap::new();
    individual.insert(validator.clone(), 100);
    pallet_staking::ErasRewardPoints::<Runtime>::insert(
        era,
        EraRewardPoints {
            total: 100,
            individual,
        },
    );
    pallet_staking::ErasValidatorPrefs::<Runtime>::insert(
        era,
        validator,
        ValidatorPrefs {
            commission,
            blocked: false,
        },
    );
    pallet_staking::ErasStakersOverview::<Runtime>::insert(
        era,
        validator,
        PagedExposureMetadata {
            total,
            own,
            nominator_count: nominators.len() as u32,
            page_count: 1,
        },
    );
    pallet_staking::ErasStakersPaged::<Runtime>::insert(
        (era, validator, 0),
        ExposurePage {
            page_total: nominator_total,
            others: nominators
                .iter()
                .map(|(who, value)| IndividualExposure {
                    who: who.clone(),
                    value: *value,
                })
                .collect(),
        },
    );
    pallet_reward_reserve::EraRewardBudgets::<Runtime>::insert(era, budget);
    pallet_reward_reserve::EraRewardLiabilities::<Runtime>::insert(era, budget);
    pallet_reward_reserve::CommittedLiabilities::<Runtime>::put(budget);
    pallet_reward_reserve::RewardSystemActive::<Runtime>::put(true);
    pallet_reward_reserve::ActivationEra::<Runtime>::put(era.saturating_sub(1));
    pallet_reward_reserve::FirstEligibleEra::<Runtime>::put(era);
    pallet_staking::ActiveEra::<Runtime>::put(pallet_staking::ActiveEraInfo {
        index: era + 1,
        start: Some(1_800_000_000_000),
    });
}

#[test]
fn production_authorship_must_write_staking_reward_points() {
    let (mut ext, fixture) = new_test_ext(5, 0, 4, 4);
    ext.execute_with(|| {
        let era = 42;
        pallet_staking::CurrentEra::<Runtime>::put(era);
        pallet_staking::ActiveEra::<Runtime>::put(pallet_staking::ActiveEraInfo {
            index: era,
            start: Some(1_000_000),
        });

        for (block, authority_index) in [(2, 0), (3, 1), (4, 2), (5, 3), (6, 0)] {
            author_block_with_babe_digest(block, authority_index);
        }
        let points = Staking::eras_reward_points(era);
        assert_eq!(points.total, 100);
        assert_eq!(points.individual.get(&fixture.validators[0]), Some(&40));
        for validator in fixture.validators.iter().take(4).skip(1) {
            assert_eq!(points.individual.get(validator), Some(&20));
        }
        assert_eq!(points.individual.get(&fixture.validators[4]), None);
        assert_eq!(
            points.individual.values().copied().sum::<u32>(),
            points.total
        );

        author_block_with_babe_digest(7, 99);
        assert_eq!(Staking::eras_reward_points(era), points);

        pallet_session::CurrentIndex::<Runtime>::put(1);
        pallet_session::Validators::<Runtime>::put(vec![
            fixture.validators[4].clone(),
            fixture.validators[1].clone(),
            fixture.validators[2].clone(),
            fixture.validators[3].clone(),
        ]);
        author_block_with_babe_digest(8, 0);
        let transitioned = Staking::eras_reward_points(era);
        assert_eq!(transitioned.total, 120);
        assert_eq!(
            transitioned.individual.get(&fixture.validators[4]),
            Some(&20)
        );
        assert_eq!(transitioned.individual.values().copied().sum::<u32>(), 120);

        let next_era = era + 1;
        pallet_staking::CurrentEra::<Runtime>::put(next_era);
        pallet_staking::ActiveEra::<Runtime>::put(pallet_staking::ActiveEraInfo {
            index: next_era,
            start: Some(22_600_000),
        });
        author_block_with_babe_digest(9, 1);
        let next_points = Staking::eras_reward_points(next_era);
        assert_eq!(next_points.total, 20);
        assert_eq!(
            next_points.individual.get(&fixture.validators[1]),
            Some(&20)
        );
        assert_eq!(Staking::eras_reward_points(era), transitioned);
    });
}

#[test]
fn zero_point_guard_rejects_absent_empty_malformed_and_inactive_only_points() {
    for case in 0..4 {
        let (mut ext, fixture) = new_test_ext(4, 0, 4, 4);
        ext.execute_with(|| {
            let era = 100 + case;
            fund_reward_pot(&account(250), 1_000_000 * DECIMALS);
            pallet_reward_reserve::RewardSystemActive::<Runtime>::put(true);
            pallet_reward_reserve::ActivationEra::<Runtime>::put(era - 1);
            pallet_reward_reserve::FirstEligibleEra::<Runtime>::put(era);
            pallet_reward_reserve::AllocationPaused::<Runtime>::put(false);
            match case {
                0 => {}
                1 => pallet_staking::ErasRewardPoints::<Runtime>::insert(
                    era,
                    EraRewardPoints {
                        total: 0,
                        individual: BTreeMap::new(),
                    },
                ),
                2 => pallet_staking::ErasRewardPoints::<Runtime>::insert(
                    era,
                    EraRewardPoints {
                        total: 100,
                        individual: BTreeMap::new(),
                    },
                ),
                _ => {
                    let mut individual = BTreeMap::new();
                    individual.insert(account(200), 100);
                    pallet_staking::ErasRewardPoints::<Runtime>::insert(
                        era,
                        EraRewardPoints {
                            total: 100,
                            individual,
                        },
                    );
                    assert!(!fixture.validators.contains(&account(200)));
                }
            }
            let pot_before = RewardReserve::pot_balance();
            let available_before = RewardReserve::available_reward_balance();
            let issuance_before = Balances::total_issuance();
            assert_ok!(RewardReserve::finalize_completed_era(
                era,
                21_600_000,
                2_000_000_000_000,
            ));
            assert_eq!(RewardReserve::era_reward_budget(era), Some(0));
            assert_eq!(RewardReserve::era_reward_liability(era), 0);
            assert_eq!(RewardReserve::annual_budget_used(), 0);
            assert_eq!(RewardReserve::committed_liabilities(), 0);
            assert_eq!(RewardReserve::pot_balance(), pot_before);
            assert_eq!(RewardReserve::available_reward_balance(), available_before);
            assert_eq!(Balances::total_issuance(), issuance_before);
            assert!(RewardReserve::allocation_paused());
            assert_eq!(
                RewardReserve::skipped_reward_era(era),
                Some(pallet_reward_reserve::RewardEraSkipReason::NoRewardPoints)
            );
            assert!(System::events().iter().any(|record| matches!(
                record.event,
                RuntimeEvent::RewardReserve(
                    pallet_reward_reserve::Event::EraRewardSkippedNoPoints { era: event_era, .. }
                ) if event_era == era
            )));
            assert_noop!(
                RewardReserve::finalize_completed_era(era, 21_600_000, 2_000_000_000_000),
                pallet_reward_reserve::Error::<Runtime>::EraAlreadyFinalized
            );
        });
    }
}

#[test]
fn live_v1_to_v2_migration_preserves_accounting_and_skips_only_partial_era() {
    let (mut ext, _) = new_test_ext(4, 0, 4, 4);
    ext.execute_with(|| {
        prepare_and_activate_reward_system(&account(250));
        let liability: Balance = 570_397_451_566_298_291_419;
        pallet_reward_reserve::FirstEligibleEra::<Runtime>::put(1_782);
        pallet_reward_reserve::BudgetPeriodStartMillis::<Runtime>::put(1_900_000_000_000);
        pallet_reward_reserve::EraRewardBudgets::<Runtime>::insert(1_782, liability);
        pallet_reward_reserve::EraRewardLiabilities::<Runtime>::insert(1_782, liability);
        pallet_reward_reserve::AnnualBudgetUsed::<Runtime>::put(liability);
        pallet_reward_reserve::CommittedLiabilities::<Runtime>::put(liability);
        pallet_staking::ActiveEra::<Runtime>::put(pallet_staking::ActiveEraInfo {
            index: 1_792,
            start: Some(1_900_100_000_000),
        });
        StorageVersion::new(1).put::<RewardReserve>();
        let issuance = Balances::total_issuance();
        let balances = (
            RewardReserve::pot_balance(),
            RewardReserve::ecosystem_treasury_balance(),
            RewardReserve::fee_collection_balance(),
        );

        <RewardReserve as Hooks<BlockNumber>>::on_runtime_upgrade();

        assert_eq!(
            StorageVersion::get::<RewardReserve>(),
            StorageVersion::new(2)
        );
        assert!(RewardReserve::reward_system_active());
        assert!(RewardReserve::fee_routing_active());
        assert_eq!(RewardReserve::point_source_operational_era(), Some(1_792));
        assert_eq!(RewardReserve::first_eligible_era(), Some(1_793));
        assert_eq!(
            RewardReserve::legacy_zero_point_era_range(),
            Some((1_782, 1_791))
        );
        assert_eq!(
            RewardReserve::skipped_reward_era(1_792),
            Some(pallet_reward_reserve::RewardEraSkipReason::PartialPointSourceEra)
        );
        assert_eq!(RewardReserve::era_reward_liability(1_782), liability);
        assert_eq!(RewardReserve::cancelled_zero_point_liability(1_782), None);
        assert_eq!(RewardReserve::annual_budget_used(), liability);
        assert_eq!(RewardReserve::committed_liabilities(), liability);
        assert_eq!(RewardReserve::pending_normal_fee(), 0);
        assert_eq!(RewardReserve::pending_tip_total(), 0);
        assert_eq!(
            (
                RewardReserve::pot_balance(),
                RewardReserve::ecosystem_treasury_balance(),
                RewardReserve::fee_collection_balance(),
            ),
            balances
        );
        assert_eq!(Balances::total_issuance(), issuance);

        pallet_staking::ActiveEra::<Runtime>::put(pallet_staking::ActiveEraInfo {
            index: 1_793,
            start: Some(1_900_121_600_000),
        });
        <RewardReserve as Hooks<BlockNumber>>::on_initialize(2);
        assert_eq!(RewardReserve::era_reward_budget(1_792), Some(0));
        assert_eq!(
            RewardReserve::skipped_reward_era(1_792),
            Some(pallet_reward_reserve::RewardEraSkipReason::PartialPointSourceEra)
        );
        assert_eq!(RewardReserve::era_reward_liability(1_782), liability);
        assert_eq!(RewardReserve::committed_liabilities(), liability);
        assert_eq!(Balances::total_issuance(), issuance);
    });
}

#[test]
fn bounded_zero_point_cancellation_releases_only_storage_derived_unpaid_liability() {
    let (mut ext, _) = new_test_ext(4, 0, 4, 4);
    ext.execute_with(|| {
        let era = 1_782;
        let liability: Balance = 570_397_451_566_298_291_419;
        fund_reward_pot(&account(250), liability + EXISTENTIAL_DEPOSIT);
        pallet_reward_reserve::RewardSystemActive::<Runtime>::put(true);
        pallet_reward_reserve::LegacyZeroPointEraRange::<Runtime>::put((1_782, 1_791));
        pallet_reward_reserve::LegacyLiabilityBudgetPeriodStartMillis::<Runtime>::put(10_000);
        pallet_reward_reserve::BudgetPeriodStartMillis::<Runtime>::put(10_000);
        pallet_reward_reserve::EraRewardBudgets::<Runtime>::insert(era, liability);
        pallet_reward_reserve::EraRewardLiabilities::<Runtime>::insert(era, liability);
        pallet_reward_reserve::AnnualBudgetUsed::<Runtime>::put(liability);
        pallet_reward_reserve::CommittedLiabilities::<Runtime>::put(liability);

        assert_noop!(
            RewardReserve::cancel_legacy_zero_point_liability(RuntimeOrigin::root(), era - 1),
            pallet_reward_reserve::Error::<Runtime>::InvalidLegacyZeroPointEra
        );
        assert_noop!(
            RewardReserve::cancel_legacy_zero_point_liability(
                RuntimeOrigin::signed(account(1)),
                era,
            ),
            DispatchError::BadOrigin
        );
        pallet_reward_reserve::BudgetPeriodStartMillis::<Runtime>::put(10_001);
        assert_noop!(
            RewardReserve::cancel_legacy_zero_point_liability(RuntimeOrigin::root(), era),
            pallet_reward_reserve::Error::<Runtime>::LegacyLiabilityPeriodChanged
        );
        pallet_reward_reserve::BudgetPeriodStartMillis::<Runtime>::put(10_000);
        let mut points = BTreeMap::new();
        points.insert(account(1), 20);
        pallet_staking::ErasRewardPoints::<Runtime>::insert(
            era,
            EraRewardPoints {
                total: 20,
                individual: points,
            },
        );
        assert_noop!(
            RewardReserve::cancel_legacy_zero_point_liability(RuntimeOrigin::root(), era),
            pallet_reward_reserve::Error::<Runtime>::InvalidRewardPoints
        );
        pallet_staking::ErasRewardPoints::<Runtime>::remove(era);
        pallet_reward_reserve::EraRewardPaid::<Runtime>::insert(era, 1);
        assert_noop!(
            RewardReserve::cancel_legacy_zero_point_liability(RuntimeOrigin::root(), era),
            pallet_reward_reserve::Error::<Runtime>::LiabilityAlreadyPaid
        );
        pallet_reward_reserve::EraRewardPaid::<Runtime>::remove(era);
        pallet_reward_reserve::ClaimedRewardPages::<Runtime>::insert(era, (account(1), 0), ());
        assert_noop!(
            RewardReserve::cancel_legacy_zero_point_liability(RuntimeOrigin::root(), era),
            pallet_reward_reserve::Error::<Runtime>::LiabilityHasClaims
        );
        pallet_reward_reserve::ClaimedRewardPages::<Runtime>::remove(era, (account(1), 0));

        // This deliberately stale off-chain observation cannot enter the encoded call.
        let stale_operator_observation = liability - 1;
        assert_ne!(stale_operator_observation, liability);
        let pot = RewardReserve::pot_balance();
        let available = RewardReserve::available_reward_balance();
        let issuance = Balances::total_issuance();
        assert_ok!(RewardReserve::cancel_legacy_zero_point_liability(
            RuntimeOrigin::root(),
            era,
        ));
        assert_eq!(RewardReserve::era_reward_liability(era), 0);
        assert_eq!(RewardReserve::committed_liabilities(), 0);
        assert_eq!(RewardReserve::annual_budget_used(), 0);
        assert_eq!(
            RewardReserve::cancelled_zero_point_liability(era),
            Some(liability)
        );
        assert_eq!(RewardReserve::pot_balance(), pot);
        assert_eq!(
            RewardReserve::available_reward_balance(),
            available + liability
        );
        assert_eq!(Balances::total_issuance(), issuance);
        assert!(System::events().iter().any(|record| matches!(
            record.event,
            RuntimeEvent::RewardReserve(
                pallet_reward_reserve::Event::EraRewardLiabilityCancelled {
                    era: event_era,
                    amount,
                    reason: pallet_reward_reserve::LiabilityCancellationReason::NoRewardPoints,
                }
            ) if event_era == era && amount == liability
        )));
        assert_noop!(
            RewardReserve::claim_reward_page(
                RuntimeOrigin::signed(account(210)),
                era,
                account(1),
                0,
            ),
            pallet_reward_reserve::Error::<Runtime>::RewardEraCancelled
        );
        assert_noop!(
            RewardReserve::cancel_legacy_zero_point_liability(RuntimeOrigin::root(), era),
            pallet_reward_reserve::Error::<Runtime>::LiabilityAlreadyCancelled
        );
    });
}

#[test]
fn zero_point_cancellation_preserves_other_funded_liabilities_and_period_boundaries() {
    let (mut ext, _) = new_test_ext(4, 0, 4, 4);
    ext.execute_with(|| {
        let first_era = 1_782;
        let second_era = 1_783;
        let first_liability: Balance = 400 * DECIMALS;
        let second_liability: Balance = 600 * DECIMALS;
        let total = first_liability + second_liability;
        fund_reward_pot(&account(250), total + EXISTENTIAL_DEPOSIT);
        pallet_reward_reserve::RewardSystemActive::<Runtime>::put(true);
        pallet_reward_reserve::LegacyZeroPointEraRange::<Runtime>::put((first_era, second_era));
        pallet_reward_reserve::LegacyLiabilityBudgetPeriodStartMillis::<Runtime>::put(10_000);
        pallet_reward_reserve::BudgetPeriodStartMillis::<Runtime>::put(10_000);
        pallet_reward_reserve::EraRewardBudgets::<Runtime>::insert(first_era, first_liability);
        pallet_reward_reserve::EraRewardBudgets::<Runtime>::insert(second_era, second_liability);
        pallet_reward_reserve::EraRewardLiabilities::<Runtime>::insert(first_era, first_liability);
        pallet_reward_reserve::EraRewardLiabilities::<Runtime>::insert(
            second_era,
            second_liability,
        );
        pallet_reward_reserve::AnnualBudgetUsed::<Runtime>::put(total);
        pallet_reward_reserve::CommittedLiabilities::<Runtime>::put(total);
        let pot = RewardReserve::pot_balance();
        let issuance = Balances::total_issuance();

        assert_ok!(RewardReserve::cancel_legacy_zero_point_liability(
            RuntimeOrigin::root(),
            first_era,
        ));
        assert_eq!(
            RewardReserve::cancelled_zero_point_liability(first_era),
            Some(first_liability)
        );
        assert_eq!(
            RewardReserve::era_reward_liability(second_era),
            second_liability
        );
        assert_eq!(
            RewardReserve::era_reward_budget(second_era),
            Some(second_liability)
        );
        assert_eq!(RewardReserve::committed_liabilities(), second_liability);
        assert_eq!(RewardReserve::annual_budget_used(), second_liability);
        assert_eq!(RewardReserve::pot_balance(), pot);
        assert_eq!(Balances::total_issuance(), issuance);

        pallet_reward_reserve::BudgetPeriodStartMillis::<Runtime>::put(10_001);
        assert_noop!(
            RewardReserve::cancel_legacy_zero_point_liability(RuntimeOrigin::root(), second_era),
            pallet_reward_reserve::Error::<Runtime>::LegacyLiabilityPeriodChanged
        );
        assert_eq!(
            RewardReserve::era_reward_liability(second_era),
            second_liability
        );
        assert_eq!(RewardReserve::committed_liabilities(), second_liability);
        assert_eq!(RewardReserve::pot_balance(), pot);
        assert_eq!(Balances::total_issuance(), issuance);

        pallet_reward_reserve::BudgetPeriodStartMillis::<Runtime>::put(10_000);
        assert_ok!(RewardReserve::cancel_legacy_zero_point_liability(
            RuntimeOrigin::root(),
            second_era,
        ));
        assert_eq!(
            RewardReserve::cancelled_zero_point_liability(first_era),
            Some(first_liability)
        );
        assert_eq!(
            RewardReserve::cancelled_zero_point_liability(second_era),
            Some(second_liability)
        );
        assert_eq!(RewardReserve::committed_liabilities(), 0);
        assert_eq!(RewardReserve::annual_budget_used(), 0);
        assert_eq!(RewardReserve::pot_balance(), pot);
        assert_eq!(Balances::total_issuance(), issuance);
    });
}

#[test]
fn zero_point_liability_cancellation_failure_matrix_is_atomic() {
    for case in 0u8..9 {
        let (mut ext, _) = new_test_ext(4, 0, 4, 4);
        ext.execute_with(|| {
            let era = 1_782;
            let liability: Balance = 1_000 * DECIMALS;
            pallet_reward_reserve::LegacyZeroPointEraRange::<Runtime>::put((era, era));
            pallet_reward_reserve::LegacyLiabilityBudgetPeriodStartMillis::<Runtime>::put(10_000);
            pallet_reward_reserve::BudgetPeriodStartMillis::<Runtime>::put(10_000);
            pallet_reward_reserve::EraRewardBudgets::<Runtime>::insert(era, liability);
            pallet_reward_reserve::EraRewardLiabilities::<Runtime>::insert(era, liability);
            pallet_reward_reserve::AnnualBudgetUsed::<Runtime>::put(liability);
            pallet_reward_reserve::CommittedLiabilities::<Runtime>::put(liability);
            if case != 4 {
                fund_reward_pot(&account(250), liability + EXISTENTIAL_DEPOSIT);
            }

            let expected: DispatchError = match case {
                0 => {
                    pallet_reward_reserve::EraRewardLiabilities::<Runtime>::remove(era);
                    pallet_reward_reserve::Error::<Runtime>::EraNotFinalized.into()
                }
                1 => {
                    pallet_reward_reserve::EraRewardLiabilities::<Runtime>::insert(era, 0);
                    pallet_reward_reserve::Error::<Runtime>::LiabilityAmountMismatch.into()
                }
                2 => {
                    pallet_reward_reserve::AnnualBudgetUsed::<Runtime>::put(liability - 1);
                    pallet_reward_reserve::Error::<Runtime>::LiabilityAmountMismatch.into()
                }
                3 => {
                    pallet_reward_reserve::CommittedLiabilities::<Runtime>::put(liability - 1);
                    pallet_reward_reserve::Error::<Runtime>::LiabilityAmountMismatch.into()
                }
                4 => pallet_reward_reserve::Error::<Runtime>::InsufficientRewardPot.into(),
                5 => {
                    pallet_reward_reserve::ExpiredEras::<Runtime>::insert(era, ());
                    pallet_reward_reserve::Error::<Runtime>::ClaimExpired.into()
                }
                6 => {
                    let mut malformed = BTreeMap::new();
                    malformed.insert(account(200), 20);
                    pallet_staking::ErasRewardPoints::<Runtime>::insert(
                        era,
                        EraRewardPoints {
                            total: 0,
                            individual: malformed,
                        },
                    );
                    pallet_reward_reserve::Error::<Runtime>::InvalidRewardPoints.into()
                }
                7 => {
                    pallet_reward_reserve::BudgetPeriodStartMillis::<Runtime>::kill();
                    pallet_reward_reserve::Error::<Runtime>::LegacyLiabilityPeriodChanged.into()
                }
                _ => {
                    pallet_reward_reserve::EraRewardBudgets::<Runtime>::insert(era, liability - 1);
                    pallet_reward_reserve::Error::<Runtime>::LiabilityAmountMismatch.into()
                }
            };
            let before = (
                pallet_reward_reserve::EraRewardLiabilities::<Runtime>::try_get(era).ok(),
                pallet_reward_reserve::EraRewardBudgets::<Runtime>::get(era),
                RewardReserve::committed_liabilities(),
                RewardReserve::annual_budget_used(),
                RewardReserve::cancelled_zero_point_liability(era),
                RewardReserve::pot_balance(),
                Balances::total_issuance(),
            );
            assert_eq!(
                RewardReserve::cancel_legacy_zero_point_liability(RuntimeOrigin::root(), era),
                Err(expected)
            );
            let after = (
                pallet_reward_reserve::EraRewardLiabilities::<Runtime>::try_get(era).ok(),
                pallet_reward_reserve::EraRewardBudgets::<Runtime>::get(era),
                RewardReserve::committed_liabilities(),
                RewardReserve::annual_budget_used(),
                RewardReserve::cancelled_zero_point_liability(era),
                RewardReserve::pot_balance(),
                Balances::total_issuance(),
            );
            assert_eq!(after, before);
        });
    }
}

#[test]
fn fee_routing_remains_operational_while_reward_allocation_is_paused() {
    let (mut ext, fixture) = new_test_ext(4, 0, 4, 4);
    ext.execute_with(|| {
        prepare_and_activate_reward_system(&account(250));
        pallet_reward_reserve::AllocationPaused::<Runtime>::put(true);
        let reward_before = RewardReserve::pot_balance();
        let treasury_before = RewardReserve::ecosystem_treasury_balance();
        let issuance = Balances::total_issuance();
        route_synthetic_fee(&fixture.validators[0], 101, 0, None);
        assert_eq!(RewardReserve::pot_balance(), reward_before + 70);
        assert_eq!(
            RewardReserve::ecosystem_treasury_balance(),
            treasury_before + 31
        );
        assert_eq!(Balances::total_issuance(), issuance);
        assert!(RewardReserve::allocation_paused());
        assert!(RewardReserve::fee_routing_active());
    });
}

#[test]
fn reward_configuration_and_pot_derivation_are_exact() {
    assert_eq!(AnnualRewardCap::get(), 5_000_000 * DECIMALS);
    assert_eq!(RewardMillisecondsPerYear::get(), 31_556_952_000);
    assert_eq!(
        RewardMaxValidatorCommission::get(),
        Perbill::from_percent(10)
    );
    assert_eq!(InitialRewardPotFeeShare::get(), 70);
    assert_eq!(InitialEcosystemTreasuryFeeShare::get(), 30);
    assert_eq!(InitialBurnFeeShare::get(), 0);
    assert_eq!(InitialTipToAuthorShare::get(), 100);
    assert_eq!(
        InitialRewardReserve::get() + RewardPotSafetyFloor::get(),
        20_000_000 * DECIMALS
    );
    assert_eq!(RewardPotSafetyFloor::get(), EXISTENTIAL_DEPOSIT);
    assert_eq!(TreasuryPotSafetyFloor::get(), EXISTENTIAL_DEPOSIT);
    assert_eq!(
        RewardReserve::reward_pot_account(),
        RewardReservePalletId::get().into_account_truncating()
    );
    assert_eq!(
        RewardReserve::reward_pot_account().to_ss58check(),
        "5EYCAe5gXa2LQWf2bWCpyLA2mvKQzbWBvLuaHX7peLSVSpjF"
    );
    assert_eq!(
        RewardReserve::ecosystem_treasury_account().to_ss58check(),
        "5EYCAe5gXa2LQaB33EXQpzvXybJQeNEtLY3giwnbv4bWvuAV"
    );
    assert_eq!(
        RewardReserve::fee_collection_account().to_ss58check(),
        "5EYCAe5gXa2LQ9DgJxNV4QemAFndm9wgQrUSUukPQbgM9baG"
    );
}

#[test]
fn migration_is_dormant_and_activation_skips_partial_era_restart_safely() {
    let (mut ext, fixture) = new_test_ext(4, 0, 4, 4);
    ext.execute_with(|| {
        pallet_staking::ActiveEra::<Runtime>::put(pallet_staking::ActiveEraInfo {
            index: 10,
            start: Some(1_000_000),
        });
        StorageVersion::new(0).put::<RewardReserve>();
        let issuance_before = Balances::total_issuance();
        <RewardReserve as Hooks<BlockNumber>>::on_runtime_upgrade();
        assert!(!RewardReserve::reward_system_active());
        assert!(!RewardReserve::fee_routing_active());
        assert_eq!(RewardReserve::activation_block(), None);
        assert_eq!(RewardReserve::activation_era(), None);
        assert_eq!(RewardReserve::first_eligible_era(), None);
        assert_eq!(RewardReserve::annual_budget_used(), 0);
        assert_eq!(RewardReserve::committed_liabilities(), 0);
        assert_eq!(Balances::total_issuance(), issuance_before);

        prepare_and_activate_reward_system(&account(250));
        assert_eq!(RewardReserve::activation_block(), Some(1));
        assert_eq!(RewardReserve::activation_era(), Some(10));
        assert_eq!(RewardReserve::first_eligible_era(), Some(11));
        assert_eq!(Balances::total_issuance(), issuance_before);

        pallet_staking::ActiveEra::<Runtime>::put(pallet_staking::ActiveEraInfo {
            index: 11,
            start: Some(1_000_000 + 21_600_000),
        });
        <RewardReserve as Hooks<BlockNumber>>::on_initialize(2);
        assert_eq!(RewardReserve::era_reward_budget(10), Some(0));

        install_valid_points_and_exposure(11, &fixture.validators);
        pallet_staking::ActiveEra::<Runtime>::put(pallet_staking::ActiveEraInfo {
            index: 12,
            start: Some(1_000_000 + 43_200_000),
        });
        <RewardReserve as Hooks<BlockNumber>>::on_initialize(3);
        assert_eq!(
            RewardReserve::era_reward_budget(11),
            RewardReserve::target_era_reward(21_600_000)
        );
        let state = (
            RewardReserve::activation_block(),
            RewardReserve::activation_era(),
            RewardReserve::first_eligible_era(),
            RewardReserve::era_reward_budget(11),
        );
        <RewardReserve as Hooks<BlockNumber>>::on_runtime_upgrade();
        assert_eq!(
            state,
            (
                RewardReserve::activation_block(),
                RewardReserve::activation_era(),
                RewardReserve::first_eligible_era(),
                RewardReserve::era_reward_budget(11),
            )
        );
        assert_noop!(
            RewardReserve::activate_rewards_and_fee_routing(RuntimeOrigin::root()),
            pallet_reward_reserve::Error::<Runtime>::AlreadyActivated
        );
        assert_eq!(Balances::total_issuance(), issuance_before);
    });
}

#[test]
fn dormant_migration_preserves_accounts_balances_and_zero_liabilities() {
    let (mut ext, _) = new_test_ext(4, 0, 4, 4);
    ext.execute_with(|| {
        let accounts = [
            RewardReserve::reward_pot_account(),
            RewardReserve::ecosystem_treasury_account(),
            RewardReserve::fee_collection_account(),
        ];
        let before = accounts
            .iter()
            .map(|account| {
                (
                    System::account_exists(account),
                    Balances::free_balance(account),
                )
            })
            .collect::<Vec<_>>();
        let issuance = Balances::total_issuance();
        StorageVersion::new(0).put::<RewardReserve>();
        System::reset_events();
        <RewardReserve as Hooks<BlockNumber>>::on_runtime_upgrade();
        let after = accounts
            .iter()
            .map(|account| {
                (
                    System::account_exists(account),
                    Balances::free_balance(account),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(after, before);
        assert_eq!(Balances::total_issuance(), issuance);
        assert_eq!(
            StorageVersion::get::<RewardReserve>(),
            StorageVersion::new(2)
        );
        assert!(!RewardReserve::reward_system_active());
        assert!(!RewardReserve::fee_routing_active());
        assert_eq!(RewardReserve::activation_block(), None);
        assert_eq!(RewardReserve::activation_era(), None);
        assert_eq!(RewardReserve::first_eligible_era(), None);
        assert_eq!(RewardReserve::annual_budget_used(), 0);
        assert_eq!(RewardReserve::committed_liabilities(), 0);
        assert_eq!(RewardReserve::pending_normal_fee(), 0);
        assert_eq!(RewardReserve::pending_tip_total(), 0);
        assert!(System::events().iter().all(|record| !matches!(
            record.event,
            RuntimeEvent::RewardReserve(
                pallet_reward_reserve::Event::RewardSystemActivated { .. }
                    | pallet_reward_reserve::Event::FeeRoutingActivated { .. }
                    | pallet_reward_reserve::Event::EraRewardLiabilityReserved { .. }
                    | pallet_reward_reserve::Event::TransactionFeeRouted { .. }
            )
        )));
        let state = (
            RewardReserve::reward_system_active(),
            RewardReserve::fee_routing_active(),
            RewardReserve::activation_block(),
            RewardReserve::activation_era(),
            RewardReserve::first_eligible_era(),
            Balances::total_issuance(),
        );
        <RewardReserve as Hooks<BlockNumber>>::on_runtime_upgrade();
        assert_eq!(
            state,
            (
                RewardReserve::reward_system_active(),
                RewardReserve::fee_routing_active(),
                RewardReserve::activation_block(),
                RewardReserve::activation_era(),
                RewardReserve::first_eligible_era(),
                Balances::total_issuance(),
            )
        );
    });
}

#[cfg(feature = "try-runtime")]
#[test]
fn dormant_migration_try_runtime_pre_and_post_checks_pass() {
    let (mut ext, _) = new_test_ext(4, 0, 4, 4);
    ext.execute_with(|| {
        StorageVersion::new(0).put::<RewardReserve>();
        let state = <RewardReserve as Hooks<BlockNumber>>::pre_upgrade()
            .expect("pre-upgrade safety snapshot");
        <RewardReserve as Hooks<BlockNumber>>::on_runtime_upgrade();
        <RewardReserve as Hooks<BlockNumber>>::post_upgrade(state)
            .expect("post-upgrade safety invariants");
    });
}

#[test]
fn activation_rejects_every_incomplete_gate_without_partial_state() {
    let (mut ext, fixture) = new_test_ext(4, 0, 4, 4);
    ext.execute_with(|| {
        pallet_staking::ActiveEra::<Runtime>::put(pallet_staking::ActiveEraInfo {
            index: 10,
            start: Some(1_000_000),
        });
        let source = account(250);
        let inactive = || {
            (
                RewardReserve::reward_system_active(),
                RewardReserve::fee_routing_active(),
                RewardReserve::activation_block(),
                RewardReserve::activation_era(),
                RewardReserve::first_eligible_era(),
                RewardReserve::committed_liabilities(),
            )
        };
        let expected = inactive();
        assert_noop!(
            RewardReserve::activate_rewards_and_fee_routing(RuntimeOrigin::signed(
                fixture.validators[0].clone()
            )),
            DispatchError::BadOrigin
        );
        assert_eq!(inactive(), expected);
        assert_noop!(
            RewardReserve::activate_rewards_and_fee_routing(RuntimeOrigin::root()),
            pallet_reward_reserve::Error::<Runtime>::RewardPotMissing
        );
        assert_eq!(inactive(), expected);

        fund_reward_pot(&source, RewardPotSafetyFloor::get());
        assert_noop!(
            RewardReserve::activate_rewards_and_fee_routing(RuntimeOrigin::root()),
            pallet_reward_reserve::Error::<Runtime>::RewardPotBelowActivationReserve
        );
        assert_eq!(inactive(), expected);
        fund_reward_pot(&source, InitialRewardReserve::get());
        assert_noop!(
            RewardReserve::activate_rewards_and_fee_routing(RuntimeOrigin::root()),
            pallet_reward_reserve::Error::<Runtime>::TreasuryPotMissing
        );
        assert_eq!(inactive(), expected);
        assert_ok!(Balances::transfer_allow_death(
            RuntimeOrigin::signed(source.clone()),
            MultiAddress::Id(RewardReserve::ecosystem_treasury_account()),
            TreasuryPotSafetyFloor::get(),
        ));
        assert_noop!(
            RewardReserve::activate_rewards_and_fee_routing(RuntimeOrigin::root()),
            pallet_reward_reserve::Error::<Runtime>::FeeCollectionPotMissing
        );
        assert_eq!(inactive(), expected);

        pallet_reward_reserve::PendingRewardFee::<Runtime>::put(1);
        assert_noop!(
            RewardReserve::activate_rewards_and_fee_routing(RuntimeOrigin::root()),
            pallet_reward_reserve::Error::<Runtime>::PendingRoutingInconsistent
        );
        pallet_reward_reserve::PendingRewardFee::<Runtime>::kill();
        assert_eq!(inactive(), expected);

        let invalid = pallet_reward_reserve::FeeRoutingConfiguration {
            reward_pot_percent: 69,
            ecosystem_treasury_percent: 31,
            burn_percent: 0,
            tip_to_author_percent: 100,
        };
        pallet_reward_reserve::FeeRoutingConfig::<Runtime>::put(invalid);
        assert_noop!(
            RewardReserve::activate_rewards_and_fee_routing(RuntimeOrigin::root()),
            pallet_reward_reserve::Error::<Runtime>::InvalidFeeRoutingConfiguration
        );
        pallet_reward_reserve::FeeRoutingConfig::<Runtime>::put(
            RewardReserve::initial_fee_routing_configuration(),
        );
        assert_eq!(inactive(), expected);

        assert_ok!(Balances::transfer_allow_death(
            RuntimeOrigin::signed(source),
            MultiAddress::Id(RewardReserve::fee_collection_account()),
            EXISTENTIAL_DEPOSIT,
        ));
        pallet_reward_reserve::EraRewardBudgets::<Runtime>::insert(9, 1);
        assert_noop!(
            RewardReserve::activate_rewards_and_fee_routing(RuntimeOrigin::root()),
            pallet_reward_reserve::Error::<Runtime>::ActivationStateInvalid
        );
        pallet_reward_reserve::EraRewardBudgets::<Runtime>::remove(9);
        assert_eq!(inactive(), expected);

        assert_ok!(RewardReserve::activate_rewards_and_fee_routing(
            RuntimeOrigin::root()
        ));
        assert!(RewardReserve::reward_system_active());
        assert!(RewardReserve::fee_routing_active());
        assert_eq!(RewardReserve::activation_era(), Some(10));
        assert_eq!(RewardReserve::first_eligible_era(), Some(11));
    });
}

#[test]
fn activation_distinguishes_below_ed_accounts() {
    let (mut ext, _) = new_test_ext(4, 0, 4, 4);
    ext.execute_with(|| {
        pallet_staking::ActiveEra::<Runtime>::put(pallet_staking::ActiveEraInfo {
            index: 10,
            start: Some(1_000_000),
        });
        let reward = RewardReserve::reward_pot_account();
        let _provider_status = System::inc_providers(&reward);
        pallet_balances::Account::<Runtime>::mutate(&reward, |data| {
            data.free = EXISTENTIAL_DEPOSIT - 1;
        });
        assert_noop!(
            RewardReserve::activate_rewards_and_fee_routing(RuntimeOrigin::root()),
            pallet_reward_reserve::Error::<Runtime>::RewardPotBelowExistentialDeposit
        );
        assert!(!RewardReserve::reward_system_active());
    });
}

#[test]
fn reward_funding_and_paged_claims_are_transfer_only() {
    let (mut ext, fixture) = new_test_ext(4, 2, 4, 4);
    ext.execute_with(|| {
        let source = fixture.validators[0].clone();
        let validator = fixture.validators[1].clone();
        let nominator_a = fixture.nominators[0].clone();
        let nominator_b = fixture.nominators[1].clone();
        let pot = RewardReserve::reward_pot_account();
        let issuance_before = Balances::total_issuance();
        let source_before = Balances::free_balance(&source);
        fund_reward_pot(&source, 10_000 * DECIMALS);
        assert_eq!(Balances::total_issuance(), issuance_before);
        assert_eq!(Balances::free_balance(&pot), 10_000 * DECIMALS);
        assert_eq!(Balances::free_balance(&source), source_before - 10_000 * DECIMALS);

        let budget = 1_000 * DECIMALS;
        install_single_page_reward(
            20,
            &validator,
            &[(nominator_a.clone(), 300), (nominator_b.clone(), 600)],
            100,
            budget,
            Perbill::from_percent(50),
        );
        let validator_before = Balances::free_balance(&validator);
        let nominator_a_before = Balances::free_balance(&nominator_a);
        let nominator_b_before = Balances::free_balance(&nominator_b);
        assert_ok!(RewardReserve::claim_reward_page(
            RuntimeOrigin::signed(account(200)),
            20,
            validator.clone(),
            0,
        ));
        assert_eq!(Balances::total_issuance(), issuance_before);
        assert_eq!(Balances::free_balance(&validator) - validator_before, 190 * DECIMALS);
        assert_eq!(Balances::free_balance(&nominator_a) - nominator_a_before, 270 * DECIMALS);
        assert_eq!(Balances::free_balance(&nominator_b) - nominator_b_before, 540 * DECIMALS);
        assert_eq!(RewardReserve::era_reward_paid(20), budget);
        assert_eq!(RewardReserve::era_reward_liability(20), 0);
        assert_eq!(RewardReserve::committed_liabilities(), 0);
        assert_eq!(Balances::free_balance(&pot), 9_000 * DECIMALS);

        assert_noop!(
            RewardReserve::claim_reward_page(
                RuntimeOrigin::signed(account(201)),
                20,
                validator,
                0,
            ),
            pallet_reward_reserve::Error::<Runtime>::AlreadyClaimed
        );
        assert_eq!(Balances::total_issuance(), issuance_before);
    });
}

#[test]
fn failed_expired_and_exhausted_rewards_preserve_issuance() {
    let (mut ext, fixture) = new_test_ext(4, 0, 4, 4);
    ext.execute_with(|| {
        let validator = fixture.validators[1].clone();
        let budget = 1_000 * DECIMALS;
        install_single_page_reward(
            30,
            &validator,
            &[],
            100,
            budget,
            Perbill::zero(),
        );
        let issuance_before = Balances::total_issuance();
        assert_noop!(
            RewardReserve::claim_reward_page(
                RuntimeOrigin::signed(account(202)),
                30,
                validator.clone(),
                0,
            ),
            pallet_reward_reserve::Error::<Runtime>::RewardPotBelowExistentialDeposit
        );
        assert_eq!(Balances::total_issuance(), issuance_before);
        assert_eq!(RewardReserve::era_reward_liability(30), budget);

        pallet_staking::ActiveEra::<Runtime>::put(pallet_staking::ActiveEraInfo {
            index: 30 + HistoryDepth::get() + 1,
            start: Some(1_900_000_000_000),
        });
        pallet_reward_reserve::NextExpiryEra::<Runtime>::put(30);
        <RewardReserve as Hooks<BlockNumber>>::on_initialize(4);
        assert_eq!(RewardReserve::era_reward_liability(30), 0);
        assert_eq!(RewardReserve::committed_liabilities(), 0);
        assert_noop!(
            RewardReserve::claim_reward_page(
                RuntimeOrigin::signed(account(203)),
                30,
                validator,
                0,
            ),
            pallet_reward_reserve::Error::<Runtime>::ClaimExpired
        );
        assert_eq!(Balances::total_issuance(), issuance_before);
    });
}

#[test]
fn annual_cap_duration_and_fee_splits_are_bounded() {
    let (mut ext, _) = new_test_ext(4, 0, 4, 4);
    ext.execute_with(|| {
        let year = RewardMillisecondsPerYear::get();
        assert_eq!(
            RewardReserve::target_era_reward(year),
            Some(5_000_000 * DECIMALS)
        );
        assert_eq!(
            RewardReserve::target_era_reward(year / 2),
            Some(2_500_000 * DECIMALS)
        );
        assert!(
            RewardReserve::target_era_reward(year + 21_600_000).unwrap() > 5_000_000 * DECIMALS
        );
        for percent in [0u8, 30, 50, 70, 100] {
            let (pot, remainder) = RewardReserve::fee_split(101, percent).unwrap();
            assert_eq!(pot + remainder, 101);
            assert_eq!(pot, 101 * percent as u128 / 100);
        }
        assert_eq!(RewardReserve::fee_split(101, 101), None);
    });
}

#[test]
fn two_exposure_pages_pay_exact_budget_and_release_liability() {
    let (mut ext, fixture) = new_test_ext(4, 2, 4, 4);
    ext.execute_with(|| {
        let source = fixture.validators[0].clone();
        let validator = fixture.validators[1].clone();
        let nominator_a = fixture.nominators[0].clone();
        let nominator_b = fixture.nominators[1].clone();
        let era = 40;
        let budget = 1_000 * DECIMALS;
        fund_reward_pot(&source, 2_000 * DECIMALS);
        let mut individual = BTreeMap::new();
        individual.insert(validator.clone(), 100);
        pallet_staking::ErasRewardPoints::<Runtime>::insert(
            era,
            EraRewardPoints {
                total: 100,
                individual,
            },
        );
        pallet_staking::ErasValidatorPrefs::<Runtime>::insert(
            era,
            &validator,
            ValidatorPrefs {
                commission: Perbill::from_percent(10),
                blocked: false,
            },
        );
        pallet_staking::ErasStakersOverview::<Runtime>::insert(
            era,
            &validator,
            PagedExposureMetadata {
                total: 1_000,
                own: 100,
                nominator_count: 2,
                page_count: 2,
            },
        );
        pallet_staking::ErasStakersPaged::<Runtime>::insert(
            (era, &validator, 0),
            ExposurePage {
                page_total: 300,
                others: vec![IndividualExposure {
                    who: nominator_a,
                    value: 300,
                }],
            },
        );
        pallet_staking::ErasStakersPaged::<Runtime>::insert(
            (era, &validator, 1),
            ExposurePage {
                page_total: 600,
                others: vec![IndividualExposure {
                    who: nominator_b,
                    value: 600,
                }],
            },
        );
        pallet_reward_reserve::EraRewardBudgets::<Runtime>::insert(era, budget);
        pallet_reward_reserve::EraRewardLiabilities::<Runtime>::insert(era, budget);
        pallet_reward_reserve::CommittedLiabilities::<Runtime>::put(budget);
        pallet_reward_reserve::RewardSystemActive::<Runtime>::put(true);
        pallet_reward_reserve::ActivationEra::<Runtime>::put(era - 1);
        pallet_reward_reserve::FirstEligibleEra::<Runtime>::put(era);
        pallet_staking::ActiveEra::<Runtime>::put(pallet_staking::ActiveEraInfo {
            index: era + 1,
            start: Some(1_800_000_000_000),
        });
        let issuance = Balances::total_issuance();
        assert_ok!(RewardReserve::claim_reward_page(
            RuntimeOrigin::signed(account(210)),
            era,
            validator.clone(),
            0,
        ));
        assert_eq!(RewardReserve::era_reward_paid(era), 400 * DECIMALS);
        assert_eq!(RewardReserve::era_reward_liability(era), 600 * DECIMALS);
        assert_ok!(RewardReserve::claim_reward_page(
            RuntimeOrigin::signed(account(211)),
            era,
            validator,
            1,
        ));
        assert_eq!(RewardReserve::era_reward_paid(era), budget);
        assert_eq!(RewardReserve::era_reward_liability(era), 0);
        assert_eq!(RewardReserve::committed_liabilities(), 0);
        assert_eq!(Balances::total_issuance(), issuance);
    });
}

#[test]
fn sixty_four_nominator_rounding_never_exceeds_budget() {
    let (mut ext, fixture) = new_test_ext(4, 64, 4, 4);
    ext.execute_with(|| {
        let source = fixture.validators[0].clone();
        let validator = fixture.validators[1].clone();
        let era = 50;
        let budget = 1_000 * DECIMALS;
        fund_reward_pot(&source, 2_000 * DECIMALS);
        let nominations = fixture
            .nominators
            .iter()
            .cloned()
            .map(|who| (who, 10u128))
            .collect::<Vec<_>>();
        install_single_page_reward(
            era,
            &validator,
            &nominations,
            100,
            budget,
            Perbill::from_percent(5),
        );
        let issuance = Balances::total_issuance();
        assert_ok!(RewardReserve::claim_reward_page(
            RuntimeOrigin::signed(account(212)),
            era,
            validator,
            0,
        ));
        assert!(RewardReserve::era_reward_paid(era) <= budget);
        assert_eq!(RewardReserve::era_reward_liability(era), 0);
        assert_eq!(RewardReserve::committed_liabilities(), 0);
        assert_eq!(Balances::total_issuance(), issuance);
    });
}

#[test]
fn oversized_reward_page_fails_before_any_transfer() {
    let (mut ext, fixture) = new_test_ext(4, 64, 4, 4);
    ext.execute_with(|| {
        let validator = fixture.validators[1].clone();
        let mut nominations = fixture
            .nominators
            .iter()
            .cloned()
            .map(|who| (who, 10u128))
            .collect::<Vec<_>>();
        nominations.push((account(220), 10));
        install_single_page_reward(
            60,
            &validator,
            &nominations,
            100,
            1_000 * DECIMALS,
            Perbill::zero(),
        );
        let issuance = Balances::total_issuance();
        assert_noop!(
            RewardReserve::claim_reward_page(
                RuntimeOrigin::signed(account(213)),
                60,
                validator,
                0,
            ),
            pallet_reward_reserve::Error::<Runtime>::TooManyRewardRecipients
        );
        assert_eq!(RewardReserve::era_reward_paid(60), 0);
        assert_eq!(Balances::total_issuance(), issuance);
    });
}

#[test]
fn solvency_limit_and_annual_cap_bound_new_liabilities() {
    let (mut ext, fixture) = new_test_ext(4, 0, 4, 4);
    ext.execute_with(|| {
        let source = account(250);
        let issuance = Balances::total_issuance();
        fund_reward_pot(&source, 6_000_000 * DECIMALS);
        assert_eq!(Balances::total_issuance(), issuance);
        pallet_reward_reserve::ActivationEra::<Runtime>::put(0);
        pallet_reward_reserve::FirstEligibleEra::<Runtime>::put(1);
        install_valid_points_and_exposure(1, &fixture.validators);
        assert_ok!(RewardReserve::finalize_completed_era(
            1,
            RewardMillisecondsPerYear::get() + 21_600_000,
            2_000_000_000_000,
        ));
        assert_eq!(
            RewardReserve::era_reward_budget(1),
            Some(5_000_000 * DECIMALS)
        );
        assert_eq!(RewardReserve::annual_budget_used(), 5_000_000 * DECIMALS);
        assert_eq!(RewardReserve::committed_liabilities(), 5_000_000 * DECIMALS);
        assert!(RewardReserve::available_reward_balance() < 1_000_000 * DECIMALS);
        assert_eq!(Balances::total_issuance(), issuance);
    });
}

#[test]
fn corrected_normal_fee_routes_seventy_thirty_and_tip_routes_once() {
    use frame_support::traits::tokens::{Fortitude, Precision, Preservation};
    let (mut ext, fixture) = new_test_ext(4, 0, 4, 4);
    ext.execute_with(|| {
        let payer = fixture.validators[0].clone();
        let author = fixture.validators[1].clone();
        prepare_and_activate_reward_system(&account(250));
        let pot = RewardReserve::reward_pot_account();
        let treasury = RewardReserve::ecosystem_treasury_account();
        let issuance = Balances::total_issuance();
        let payer_before = Balances::free_balance(&payer);
        let author_before = Balances::free_balance(&author);
        let fee = <Balances as Balanced<AccountId>>::withdraw(
            &payer,
            101,
            Precision::Exact,
            Preservation::Preserve,
            Fortitude::Polite,
        )
        .expect("synthetic corrected normal fee");
        let tip = <Balances as Balanced<AccountId>>::withdraw(
            &payer,
            11,
            Precision::Exact,
            Preservation::Preserve,
            Fortitude::Polite,
        )
        .expect("synthetic tip");
        DealWithFees::route_credits(fee, Some(tip), Some(author.clone()));
        assert_eq!(
            Balances::free_balance(&pot),
            InitialRewardReserve::get() + RewardPotSafetyFloor::get() + 70
        );
        assert_eq!(Balances::free_balance(&treasury), EXISTENTIAL_DEPOSIT + 31);
        assert_eq!(Balances::free_balance(&author) - author_before, 11);
        assert_eq!(payer_before - Balances::free_balance(&payer), 112);
        assert_eq!(RewardReserve::fee_contribution_total(), 70);
        assert_eq!(RewardReserve::ecosystem_treasury_fee_total(), 31);
        assert_eq!(RewardReserve::normal_fee_routed_total(), 101);
        assert_eq!(RewardReserve::author_tip_total(), 11);
        assert_eq!(RewardReserve::fee_collection_balance(), EXISTENTIAL_DEPOSIT);
        assert_eq!(RewardReserve::pending_normal_fee(), 0);
        assert_eq!(RewardReserve::pending_tip_total(), 0);
        assert_eq!(Balances::total_issuance(), issuance);
    });
}

#[test]
fn postactivation_minimum_and_rounding_fees_allocate_the_exact_remainder() {
    let (mut ext, fixture) = new_test_ext(4, 0, 4, 4);
    ext.execute_with(|| {
        prepare_and_activate_reward_system(&account(250));
        let reward_start = RewardReserve::pot_balance();
        let treasury_start = RewardReserve::ecosystem_treasury_balance();
        let issuance = Balances::total_issuance();
        let mut expected_reward = 0u128;
        let mut expected_treasury = 0u128;
        for fee in [1u128, 2, 3, 10, 101, 1_000_000_000_000_000_001] {
            route_synthetic_fee(&fixture.validators[0], fee, 0, None);
            let reward = fee * 70 / 100;
            let treasury = fee - reward;
            expected_reward += reward;
            expected_treasury += treasury;
        }
        assert_eq!(RewardReserve::pot_balance() - reward_start, expected_reward);
        assert_eq!(
            RewardReserve::ecosystem_treasury_balance() - treasury_start,
            expected_treasury
        );
        assert_eq!(
            RewardReserve::normal_fee_routed_total(),
            expected_reward + expected_treasury
        );
        assert_eq!(RewardReserve::pending_normal_fee(), 0);
        assert_eq!(RewardReserve::fee_collection_balance(), EXISTENTIAL_DEPOSIT);
        assert_eq!(Balances::total_issuance(), issuance);
    });
}

#[test]
fn transaction_adapter_splits_only_the_corrected_post_refund_fee() {
    use frame_support::dispatch::{GetDispatchInfo, Pays, PostDispatchInfo};
    use pallet_transaction_payment::OnChargeTransaction;
    type Adapter = <Runtime as pallet_transaction_payment::Config>::OnChargeTransaction;
    let (mut ext, fixture) = new_test_ext(4, 0, 4, 4);
    ext.execute_with(|| {
        let payer = fixture.validators[0].clone();
        prepare_and_activate_reward_system(&account(250));
        let call = RuntimeCall::System(frame_system::Call::remark { remark: vec![] });
        let info = call.get_dispatch_info();
        let payer_before = Balances::free_balance(&payer);
        let issuance = Balances::total_issuance();
        let withdrawn =
            <Adapter as OnChargeTransaction<Runtime>>::withdraw_fee(&payer, &call, &info, 1_000, 0)
                .expect("predicted fee withdrawal");
        <Adapter as OnChargeTransaction<Runtime>>::correct_and_deposit_fee(
            &payer,
            &info,
            &PostDispatchInfo {
                actual_weight: Some(frame_support::weights::Weight::from_parts(1, 0)),
                pays_fee: Pays::Yes,
            },
            101,
            0,
            withdrawn,
        )
        .expect("post-dispatch refund and routing");
        assert_eq!(payer_before - Balances::free_balance(&payer), 101);
        assert_eq!(
            Balances::free_balance(RewardReserve::reward_pot_account()),
            InitialRewardReserve::get() + RewardPotSafetyFloor::get() + 70
        );
        assert_eq!(
            Balances::free_balance(RewardReserve::ecosystem_treasury_account()),
            EXISTENTIAL_DEPOSIT + 31
        );
        assert_eq!(Balances::total_issuance(), issuance);
    });
}

#[test]
fn failed_transaction_routes_only_its_corrected_post_dispatch_charge() {
    use frame_support::dispatch::GetDispatchInfo;
    use pallet_transaction_payment::OnChargeTransaction;
    use sp_runtime::traits::Dispatchable;
    type Adapter = <Runtime as pallet_transaction_payment::Config>::OnChargeTransaction;
    let (mut ext, fixture) = new_test_ext(4, 0, 4, 4);
    ext.execute_with(|| {
        let payer = fixture.validators[0].clone();
        prepare_and_activate_reward_system(&account(250));
        let call = RuntimeCall::Balances(pallet_balances::Call::transfer_allow_death {
            dest: MultiAddress::Id(account(240)),
            value: Balance::MAX,
        });
        let info = call.get_dispatch_info();
        let payer_before = Balances::free_balance(&payer);
        let issuance = Balances::total_issuance();
        let withdrawn =
            <Adapter as OnChargeTransaction<Runtime>>::withdraw_fee(&payer, &call, &info, 1_000, 0)
                .expect("predicted fee withdrawal");
        let failed = call
            .dispatch(RuntimeOrigin::signed(payer.clone()))
            .expect_err("balance transfer must fail");
        <Adapter as OnChargeTransaction<Runtime>>::correct_and_deposit_fee(
            &payer,
            &info,
            &failed.post_info,
            101,
            0,
            withdrawn,
        )
        .expect("failed call fee correction and routing");
        assert_eq!(payer_before - Balances::free_balance(&payer), 101);
        assert_eq!(
            Balances::free_balance(RewardReserve::reward_pot_account()),
            InitialRewardReserve::get() + RewardPotSafetyFloor::get() + 70
        );
        assert_eq!(
            Balances::free_balance(RewardReserve::ecosystem_treasury_account()),
            EXISTENTIAL_DEPOSIT + 31
        );
        assert_eq!(Balances::total_issuance(), issuance);
    });
}

#[test]
fn fee_configuration_and_treasury_authority_are_enforced() {
    let (mut ext, fixture) = new_test_ext(4, 0, 4, 4);
    ext.execute_with(|| {
        let valid = pallet_reward_reserve::FeeRoutingConfiguration {
            reward_pot_percent: 70,
            ecosystem_treasury_percent: 30,
            burn_percent: 0,
            tip_to_author_percent: 100,
        };
        assert!(RewardReserve::valid_fee_routing_configuration(&valid));
        let invalid = pallet_reward_reserve::FeeRoutingConfiguration {
            reward_pot_percent: 69,
            ecosystem_treasury_percent: 30,
            burn_percent: 0,
            tip_to_author_percent: 100,
        };
        assert!(!RewardReserve::valid_fee_routing_configuration(&invalid));
        assert_noop!(
            RewardReserve::set_fee_routing_configuration(
                RuntimeOrigin::signed(fixture.validators[0].clone()),
                valid,
            ),
            DispatchError::BadOrigin
        );
        assert_noop!(
            RewardReserve::set_fee_routing_configuration(RuntimeOrigin::root(), invalid),
            pallet_reward_reserve::Error::<Runtime>::InvalidFeeRoutingConfiguration
        );
        assert_ok!(RewardReserve::set_fee_routing_configuration(
            RuntimeOrigin::root(),
            valid
        ));
    });
}

#[test]
fn treasury_and_reward_liabilities_are_account_isolated() {
    use frame_support::traits::tokens::{Fortitude, Precision, Preservation};
    let (mut ext, fixture) = new_test_ext(4, 0, 4, 4);
    ext.execute_with(|| {
        let payer = fixture.validators[0].clone();
        let beneficiary = fixture.validators[1].clone();
        prepare_and_activate_reward_system(&account(250));
        let fee = <Balances as Balanced<AccountId>>::withdraw(
            &payer,
            101,
            Precision::Exact,
            Preservation::Preserve,
            Fortitude::Polite,
        )
        .unwrap();
        DealWithFees::route_credits(fee, None, None);
        let validator = fixture.validators[2].clone();
        install_single_page_reward(
            70,
            &validator,
            &[],
            100,
            InitialRewardReserve::get() + 101,
            Perbill::zero(),
        );
        let reward_before = Balances::free_balance(RewardReserve::reward_pot_account());
        assert_noop!(
            RewardReserve::claim_reward_page(
                RuntimeOrigin::signed(payer.clone()),
                70,
                validator,
                0,
            ),
            pallet_reward_reserve::Error::<Runtime>::InsufficientRewardPot
        );
        assert_noop!(
            RewardReserve::spend_ecosystem_treasury(
                RuntimeOrigin::root(),
                RewardReserve::reward_pot_account(),
                1,
            ),
            pallet_reward_reserve::Error::<Runtime>::InvalidTreasuryBeneficiary
        );
        assert_noop!(
            RewardReserve::spend_ecosystem_treasury(
                RuntimeOrigin::signed(payer.clone()),
                beneficiary.clone(),
                31,
            ),
            DispatchError::BadOrigin
        );
        assert_ok!(RewardReserve::spend_ecosystem_treasury(
            RuntimeOrigin::root(),
            beneficiary,
            31,
        ));
        assert_eq!(
            Balances::free_balance(RewardReserve::ecosystem_treasury_account()),
            EXISTENTIAL_DEPOSIT
        );
        assert_eq!(
            Balances::free_balance(RewardReserve::reward_pot_account()),
            reward_before
        );
        assert_eq!(
            RewardReserve::committed_liabilities(),
            InitialRewardReserve::get() + 101
        );
    });
}

#[test]
fn preactivation_fees_and_tips_preserve_deliberate_spec10_burn() {
    use frame_support::traits::tokens::{Fortitude, Precision, Preservation};
    let (mut ext, fixture) = new_test_ext(4, 0, 4, 4);
    ext.execute_with(|| {
        let payer = fixture.validators[0].clone();
        let author = fixture.validators[1].clone();
        let author_before = Balances::free_balance(&author);
        let issuance = Balances::total_issuance();
        let fee = <Balances as Balanced<AccountId>>::withdraw(
            &payer,
            101,
            Precision::Exact,
            Preservation::Preserve,
            Fortitude::Polite,
        )
        .unwrap();
        let tip = <Balances as Balanced<AccountId>>::withdraw(
            &payer,
            11,
            Precision::Exact,
            Preservation::Preserve,
            Fortitude::Polite,
        )
        .unwrap();
        DealWithFees::route_credits(fee, Some(tip), Some(author.clone()));
        assert_eq!(Balances::free_balance(&author), author_before);
        assert_eq!(
            Balances::free_balance(RewardReserve::reward_pot_account()),
            0
        );
        assert_eq!(
            Balances::free_balance(RewardReserve::ecosystem_treasury_account()),
            0
        );
        assert_eq!(
            Balances::free_balance(RewardReserve::fee_collection_account()),
            0
        );
        assert_eq!(Balances::total_issuance(), issuance - 112);
        assert_eq!(RewardReserve::committed_liabilities(), 0);
        assert!(!RewardReserve::reward_system_active());
        assert!(!RewardReserve::fee_routing_active());
    });
}

#[test]
fn every_funded_but_inactive_account_combination_keeps_spec10_fee_behavior() {
    // Reward Pot only.
    let (mut reward_only, fixture) = new_test_ext(4, 0, 4, 4);
    reward_only.execute_with(|| {
        let source = account(250);
        fund_reward_pot(
            &source,
            InitialRewardReserve::get() + RewardPotSafetyFloor::get(),
        );
        let reward_before = RewardReserve::pot_balance();
        let issuance = Balances::total_issuance();
        let author_before = Balances::free_balance(&fixture.validators[1]);
        route_synthetic_fee(
            &fixture.validators[0],
            101,
            11,
            Some(fixture.validators[1].clone()),
        );
        assert_eq!(RewardReserve::pot_balance(), reward_before);
        assert_eq!(RewardReserve::ecosystem_treasury_balance(), 0);
        assert_eq!(RewardReserve::fee_collection_balance(), 0);
        assert_eq!(
            Balances::free_balance(&fixture.validators[1]),
            author_before
        );
        assert_eq!(Balances::total_issuance(), issuance - 112);
    });

    // Treasury Pot only.
    let (mut treasury_only, fixture) = new_test_ext(4, 0, 4, 4);
    treasury_only.execute_with(|| {
        let source = account(250);
        assert_ok!(Balances::transfer_allow_death(
            RuntimeOrigin::signed(source),
            MultiAddress::Id(RewardReserve::ecosystem_treasury_account()),
            TreasuryPotSafetyFloor::get(),
        ));
        let treasury_before = RewardReserve::ecosystem_treasury_balance();
        let issuance = Balances::total_issuance();
        route_synthetic_fee(
            &fixture.validators[0],
            101,
            11,
            Some(fixture.validators[1].clone()),
        );
        assert_eq!(RewardReserve::pot_balance(), 0);
        assert_eq!(RewardReserve::ecosystem_treasury_balance(), treasury_before);
        assert_eq!(RewardReserve::fee_collection_balance(), 0);
        assert_eq!(Balances::total_issuance(), issuance - 112);
    });

    // Every account operational, but Root activation deliberately not called.
    let (mut fully_funded, fixture) = new_test_ext(4, 0, 4, 4);
    fully_funded.execute_with(|| {
        let source = account(250);
        for (destination, amount) in [
            (
                RewardReserve::reward_pot_account(),
                InitialRewardReserve::get() + RewardPotSafetyFloor::get(),
            ),
            (
                RewardReserve::ecosystem_treasury_account(),
                TreasuryPotSafetyFloor::get(),
            ),
            (RewardReserve::fee_collection_account(), EXISTENTIAL_DEPOSIT),
        ] {
            assert_ok!(Balances::transfer_allow_death(
                RuntimeOrigin::signed(source.clone()),
                MultiAddress::Id(destination),
                amount,
            ));
        }
        let balances_before = (
            RewardReserve::pot_balance(),
            RewardReserve::ecosystem_treasury_balance(),
            RewardReserve::fee_collection_balance(),
        );
        let issuance = Balances::total_issuance();
        route_synthetic_fee(
            &fixture.validators[0],
            101,
            11,
            Some(fixture.validators[1].clone()),
        );
        assert_eq!(
            balances_before,
            (
                RewardReserve::pot_balance(),
                RewardReserve::ecosystem_treasury_balance(),
                RewardReserve::fee_collection_balance(),
            )
        );
        assert_eq!(Balances::total_issuance(), issuance - 112);
        assert_eq!(RewardReserve::pending_normal_fee(), 0);
        assert_eq!(RewardReserve::pending_tip_total(), 0);
        assert!(!RewardReserve::fee_routing_active());
    });
}

#[test]
fn preactivation_below_ed_and_underreserved_destinations_keep_spec10_behavior() {
    // Both destinations and the collection account exist below ED.
    let (mut below_ed, fixture) = new_test_ext(4, 0, 4, 4);
    below_ed.execute_with(|| {
        let accounts = [
            RewardReserve::reward_pot_account(),
            RewardReserve::ecosystem_treasury_account(),
            RewardReserve::fee_collection_account(),
        ];
        for account in &accounts {
            let _provider_status = System::inc_providers(account);
            pallet_balances::Account::<Runtime>::mutate(account, |data| {
                data.free = EXISTENTIAL_DEPOSIT - 1;
            });
        }
        let before = accounts
            .iter()
            .map(Balances::free_balance)
            .collect::<Vec<_>>();
        let issuance = Balances::total_issuance();
        route_synthetic_fee(
            &fixture.validators[0],
            101,
            11,
            Some(fixture.validators[1].clone()),
        );
        assert_eq!(
            before,
            accounts
                .iter()
                .map(Balances::free_balance)
                .collect::<Vec<_>>()
        );
        assert_eq!(Balances::total_issuance(), issuance - 112);
        assert_eq!(RewardReserve::pending_normal_fee(), 0);
        assert_eq!(RewardReserve::pending_tip_total(), 0);
        assert!(!RewardReserve::reward_system_active());
        assert!(!RewardReserve::fee_routing_active());
    });

    // All deterministic accounts meet ED, but the Reward Pot is below the reserve gate.
    let (mut underreserved, fixture) = new_test_ext(4, 0, 4, 4);
    underreserved.execute_with(|| {
        let source = account(250);
        for (destination, amount) in [
            (
                RewardReserve::reward_pot_account(),
                RewardPotSafetyFloor::get() + 1,
            ),
            (
                RewardReserve::ecosystem_treasury_account(),
                TreasuryPotSafetyFloor::get(),
            ),
            (RewardReserve::fee_collection_account(), EXISTENTIAL_DEPOSIT),
        ] {
            assert_ok!(Balances::transfer_allow_death(
                RuntimeOrigin::signed(source.clone()),
                MultiAddress::Id(destination),
                amount,
            ));
        }
        let before = (
            RewardReserve::pot_balance(),
            RewardReserve::ecosystem_treasury_balance(),
            RewardReserve::fee_collection_balance(),
        );
        let issuance = Balances::total_issuance();
        route_synthetic_fee(
            &fixture.validators[0],
            101,
            11,
            Some(fixture.validators[1].clone()),
        );
        assert_eq!(
            before,
            (
                RewardReserve::pot_balance(),
                RewardReserve::ecosystem_treasury_balance(),
                RewardReserve::fee_collection_balance(),
            )
        );
        assert_eq!(Balances::total_issuance(), issuance - 112);
        assert_eq!(RewardReserve::committed_liabilities(), 0);
        assert!(!RewardReserve::reward_system_active());
        assert!(!RewardReserve::fee_routing_active());
    });
}

#[test]
fn activation_rejects_treasury_only_and_persists_across_restart_boundaries() {
    // Treasury and collection are operational, but Reward is absent.
    let (mut treasury_only, _) = new_test_ext(4, 0, 4, 4);
    treasury_only.execute_with(|| {
        let source = account(250);
        pallet_staking::ActiveEra::<Runtime>::put(pallet_staking::ActiveEraInfo {
            index: 10,
            start: Some(1_000_000),
        });
        for (destination, amount) in [
            (
                RewardReserve::ecosystem_treasury_account(),
                TreasuryPotSafetyFloor::get(),
            ),
            (RewardReserve::fee_collection_account(), EXISTENTIAL_DEPOSIT),
        ] {
            assert_ok!(Balances::transfer_allow_death(
                RuntimeOrigin::signed(source.clone()),
                MultiAddress::Id(destination),
                amount,
            ));
        }
        assert_noop!(
            RewardReserve::activate_rewards_and_fee_routing(RuntimeOrigin::root()),
            pallet_reward_reserve::Error::<Runtime>::RewardPotMissing
        );
        assert!(!RewardReserve::reward_system_active());
        assert!(!RewardReserve::fee_routing_active());
        assert_eq!(RewardReserve::activation_block(), None);
    });

    // Separate externalities entries model state reload immediately before and after activation.
    let (mut restarted, _) = new_test_ext(4, 0, 4, 4);
    restarted.execute_with(|| {
        let source = account(250);
        pallet_staking::ActiveEra::<Runtime>::put(pallet_staking::ActiveEraInfo {
            index: 10,
            start: Some(1_000_000),
        });
        for (destination, amount) in [
            (
                RewardReserve::reward_pot_account(),
                InitialRewardReserve::get() + RewardPotSafetyFloor::get(),
            ),
            (
                RewardReserve::ecosystem_treasury_account(),
                TreasuryPotSafetyFloor::get(),
            ),
            (RewardReserve::fee_collection_account(), EXISTENTIAL_DEPOSIT),
        ] {
            assert_ok!(Balances::transfer_allow_death(
                RuntimeOrigin::signed(source.clone()),
                MultiAddress::Id(destination),
                amount,
            ));
        }
        assert!(!RewardReserve::reward_system_active());
    });
    restarted.execute_with(|| {
        assert_ok!(RewardReserve::activate_rewards_and_fee_routing(
            RuntimeOrigin::root()
        ));
        assert_eq!(RewardReserve::activation_era(), Some(10));
        assert_eq!(RewardReserve::first_eligible_era(), Some(11));
    });
    restarted.execute_with(|| {
        assert!(RewardReserve::reward_system_active());
        assert!(RewardReserve::fee_routing_active());
        assert_noop!(
            RewardReserve::activate_rewards_and_fee_routing(RuntimeOrigin::root()),
            pallet_reward_reserve::Error::<Runtime>::AlreadyActivated
        );
    });
}

#[test]
fn deferred_fee_obligation_survives_restart_and_remains_retryable() {
    let (mut ext, fixture) = new_test_ext(4, 0, 4, 4);
    let mut issuance = 0;
    ext.execute_with(|| {
        prepare_and_activate_reward_system(&account(250));
        assert_ok!(Balances::force_set_balance(
            RuntimeOrigin::root(),
            MultiAddress::Id(RewardReserve::ecosystem_treasury_account()),
            0,
        ));
        issuance = Balances::total_issuance();
        route_synthetic_fee(&fixture.validators[0], 101, 0, None);
        assert_eq!(RewardReserve::pending_reward_fee(), 70);
        assert_eq!(RewardReserve::pending_treasury_fee(), 31);
        assert_eq!(
            RewardReserve::fee_collection_balance(),
            EXISTENTIAL_DEPOSIT + 101
        );
    });
    ext.execute_with(|| {
        assert_eq!(RewardReserve::pending_reward_fee(), 70);
        assert_eq!(RewardReserve::pending_treasury_fee(), 31);
        assert_ok!(Balances::transfer_allow_death(
            RuntimeOrigin::signed(account(250)),
            MultiAddress::Id(RewardReserve::ecosystem_treasury_account()),
            EXISTENTIAL_DEPOSIT,
        ));
        assert_ok!(RewardReserve::retry_deferred_fee_routing(
            RuntimeOrigin::signed(fixture.validators[2].clone())
        ));
        assert_eq!(RewardReserve::pending_normal_fee(), 0);
        assert_eq!(RewardReserve::fee_collection_balance(), EXISTENTIAL_DEPOSIT);
        assert_eq!(Balances::total_issuance(), issuance);
    });
}

#[test]
fn treasury_destination_failure_rolls_back_reward_and_retries_losslessly() {
    let (mut ext, fixture) = new_test_ext(4, 0, 4, 4);
    ext.execute_with(|| {
        prepare_and_activate_reward_system(&account(250));
        let reward_before = RewardReserve::pot_balance();
        assert_ok!(Balances::force_set_balance(
            RuntimeOrigin::root(),
            MultiAddress::Id(RewardReserve::ecosystem_treasury_account()),
            0,
        ));
        let issuance = Balances::total_issuance();
        route_synthetic_fee(&fixture.validators[0], 101, 0, None);
        assert_eq!(RewardReserve::pot_balance(), reward_before);
        assert_eq!(RewardReserve::ecosystem_treasury_balance(), 0);
        assert_eq!(
            RewardReserve::fee_collection_balance(),
            EXISTENTIAL_DEPOSIT + 101
        );
        assert_eq!(RewardReserve::pending_reward_fee(), 70);
        assert_eq!(RewardReserve::pending_treasury_fee(), 31);
        assert_eq!(Balances::total_issuance(), issuance);
        assert!(System::events().iter().any(|record| matches!(
            record.event,
            RuntimeEvent::RewardReserve(pallet_reward_reserve::Event::FeeRoutingFailed {
                reason: pallet_reward_reserve::FeeRoutingFailure::EcosystemTreasuryResolution,
                ..
            })
        )));

        assert_ok!(Balances::transfer_allow_death(
            RuntimeOrigin::signed(account(250)),
            MultiAddress::Id(RewardReserve::ecosystem_treasury_account()),
            EXISTENTIAL_DEPOSIT,
        ));
        assert_ok!(RewardReserve::retry_deferred_fee_routing(
            RuntimeOrigin::signed(fixture.validators[2].clone())
        ));
        assert_eq!(RewardReserve::pot_balance(), reward_before + 70);
        assert_eq!(
            RewardReserve::ecosystem_treasury_balance(),
            EXISTENTIAL_DEPOSIT + 31
        );
        assert_eq!(RewardReserve::fee_collection_balance(), EXISTENTIAL_DEPOSIT);
        assert_eq!(RewardReserve::pending_normal_fee(), 0);
        assert_eq!(Balances::total_issuance(), issuance);
    });
}

#[test]
fn reward_and_dual_destination_failures_retain_complete_obligations() {
    // Reward failure with an operational Treasury: no Treasury partial transfer may commit.
    let (mut reward_failure, fixture) = new_test_ext(4, 0, 4, 4);
    reward_failure.execute_with(|| {
        prepare_and_activate_reward_system(&account(250));
        assert_ok!(Balances::force_set_balance(
            RuntimeOrigin::root(),
            MultiAddress::Id(RewardReserve::reward_pot_account()),
            0,
        ));
        let treasury_before = RewardReserve::ecosystem_treasury_balance();
        let issuance = Balances::total_issuance();
        route_synthetic_fee(&fixture.validators[0], 101, 0, None);
        assert_eq!(RewardReserve::pot_balance(), 0);
        assert_eq!(RewardReserve::ecosystem_treasury_balance(), treasury_before);
        assert_eq!(
            RewardReserve::fee_collection_balance(),
            EXISTENTIAL_DEPOSIT + 101
        );
        assert_eq!(RewardReserve::pending_reward_fee(), 70);
        assert_eq!(RewardReserve::pending_treasury_fee(), 31);
        assert_eq!(Balances::total_issuance(), issuance);
    });

    // Both destinations unavailable: the same full obligation remains staged.
    let (mut dual_failure, fixture) = new_test_ext(4, 0, 4, 4);
    dual_failure.execute_with(|| {
        prepare_and_activate_reward_system(&account(250));
        for destination in [
            RewardReserve::reward_pot_account(),
            RewardReserve::ecosystem_treasury_account(),
        ] {
            assert_ok!(Balances::force_set_balance(
                RuntimeOrigin::root(),
                MultiAddress::Id(destination),
                0,
            ));
        }
        let issuance = Balances::total_issuance();
        route_synthetic_fee(&fixture.validators[0], 101, 0, None);
        assert_eq!(RewardReserve::pot_balance(), 0);
        assert_eq!(RewardReserve::ecosystem_treasury_balance(), 0);
        assert_eq!(
            RewardReserve::fee_collection_balance(),
            EXISTENTIAL_DEPOSIT + 101
        );
        assert_eq!(RewardReserve::pending_reward_fee(), 70);
        assert_eq!(RewardReserve::pending_treasury_fee(), 31);
        assert_eq!(Balances::total_issuance(), issuance);
    });
}

#[test]
fn failed_author_tip_delivery_is_held_and_retryable() {
    let (mut ext, fixture) = new_test_ext(4, 0, 4, 4);
    ext.execute_with(|| {
        prepare_and_activate_reward_system(&account(250));
        let absent_author = account(245);
        assert!(!System::account_exists(&absent_author));
        let issuance = Balances::total_issuance();
        route_synthetic_fee(&fixture.validators[0], 101, 11, Some(absent_author.clone()));
        assert_eq!(RewardReserve::pending_normal_fee(), 0);
        assert_eq!(RewardReserve::pending_author_tip(&absent_author), 11);
        assert_eq!(RewardReserve::pending_tip_total(), 11);
        assert_eq!(
            RewardReserve::fee_collection_balance(),
            EXISTENTIAL_DEPOSIT + 11
        );
        assert_eq!(Balances::total_issuance(), issuance);

        assert_ok!(Balances::transfer_allow_death(
            RuntimeOrigin::signed(account(250)),
            MultiAddress::Id(absent_author.clone()),
            EXISTENTIAL_DEPOSIT,
        ));
        assert_ok!(RewardReserve::retry_deferred_author_tip(
            RuntimeOrigin::signed(fixture.validators[2].clone()),
            absent_author.clone(),
        ));
        assert_eq!(
            Balances::free_balance(&absent_author),
            EXISTENTIAL_DEPOSIT + 11
        );
        assert_eq!(RewardReserve::pending_author_tip(&absent_author), 0);
        assert_eq!(RewardReserve::pending_tip_total(), 0);
        assert_eq!(RewardReserve::author_tip_total(), 11);
        assert_eq!(RewardReserve::fee_collection_balance(), EXISTENTIAL_DEPOSIT);
        assert_eq!(Balances::total_issuance(), issuance);
    });
}

#[test]
fn treasury_and_reward_floor_attacks_fail_without_reaping_or_issuance_change() {
    let (mut ext, fixture) = new_test_ext(4, 0, 4, 4);
    ext.execute_with(|| {
        prepare_and_activate_reward_system(&account(250));
        let treasury = RewardReserve::ecosystem_treasury_account();
        let reward = RewardReserve::reward_pot_account();
        let issuance = Balances::total_issuance();
        assert_noop!(
            RewardReserve::spend_ecosystem_treasury(
                RuntimeOrigin::root(),
                fixture.validators[1].clone(),
                1,
            ),
            pallet_reward_reserve::Error::<Runtime>::TreasuryPotBelowExistentialDeposit
        );
        assert_eq!(
            Balances::free_balance(&treasury),
            TreasuryPotSafetyFloor::get()
        );
        assert!(System::account_exists(&treasury));

        let validator = fixture.validators[2].clone();
        let spendable = RewardReserve::spendable_pot_balance();
        install_single_page_reward(91, &validator, &[], 100, spendable + 1, Perbill::zero());
        assert_noop!(
            RewardReserve::claim_reward_page(
                RuntimeOrigin::signed(fixture.validators[3].clone()),
                91,
                validator,
                0,
            ),
            pallet_reward_reserve::Error::<Runtime>::InsufficientRewardPot
        );
        assert_eq!(
            Balances::free_balance(&reward),
            InitialRewardReserve::get() + RewardPotSafetyFloor::get()
        );
        assert!(System::account_exists(&reward));
        assert_eq!(Balances::total_issuance(), issuance);
    });
}

#[test]
fn fee_contribution_restores_reward_availability_after_floor_state() {
    let (mut ext, fixture) = new_test_ext(4, 0, 4, 4);
    ext.execute_with(|| {
        prepare_and_activate_reward_system(&account(250));
        assert_ok!(Balances::force_set_balance(
            RuntimeOrigin::root(),
            MultiAddress::Id(RewardReserve::reward_pot_account()),
            RewardPotSafetyFloor::get(),
        ));
        pallet_reward_reserve::AllocationPaused::<Runtime>::put(true);
        assert_eq!(RewardReserve::available_reward_balance(), 0);
        let issuance = Balances::total_issuance();
        route_synthetic_fee(&fixture.validators[0], 1_000, 0, None);
        assert_eq!(RewardReserve::available_reward_balance(), 700);
        assert_eq!(RewardReserve::fee_contribution_total(), 700);
        assert_eq!(RewardReserve::ecosystem_treasury_fee_total(), 300);
        assert_eq!(Balances::total_issuance(), issuance);
        assert!(System::account_exists(&RewardReserve::reward_pot_account()));
    });
}

#[test]
fn elected_validator_can_claim_and_inactive_candidate_cannot() {
    let (mut ext, fixture) = new_test_ext(5, 6, 4, 4);
    ext.execute_with(|| {
        let supports = <StakingElectionProvider as ElectionProvider>::elect(0)
            .expect("bounded election succeeds")
            .0;
        let elected = supports
            .iter()
            .map(|(validator, _)| validator.clone())
            .collect::<Vec<_>>();
        let active = elected[0].clone();
        let inactive = fixture
            .validators
            .iter()
            .find(|candidate| !elected.contains(candidate))
            .expect("one unelected candidate")
            .clone();
        fund_reward_pot(&account(250), 2_000 * DECIMALS);
        install_single_page_reward(80, &active, &[], 100, 1_000 * DECIMALS, Perbill::zero());
        assert_ok!(RewardReserve::claim_reward_page(
            RuntimeOrigin::signed(account(230)),
            80,
            active,
            0,
        ));
        assert_noop!(
            RewardReserve::claim_reward_page(RuntimeOrigin::signed(account(231)), 80, inactive, 0,),
            pallet_reward_reserve::Error::<Runtime>::ValidatorNotRewarded
        );
    });
}

#[test]
fn upgrade13_c2_pair_matches_real_runtime_vesting_at_every_required_boundary() {
    let (mut ext, _) = new_test_ext(4, 0, 4, 4);
    ext.execute_with(|| {
        let starting_block: BlockNumber = 100;
        let completion_block =
            starting_block + upgrade13_policy::FOUNDING_VESTING_RELEASE_INTERVALS;
        let last_release_block = completion_block - 1;
        let targets = upgrade13_policy::FOUNDING_ALLOCATION_TARGETS;
        let mut fixtures = Vec::new();

        for (index, target_amount) in targets.into_iter().enumerate() {
            let target = account(200 + index as u8);
            assert_ok!(Balances::force_set_balance(
                RuntimeOrigin::root(),
                MultiAddress::Id(target.clone()),
                target_amount,
            ));
            let terms = upgrade13_policy::provisional_linear_vesting_terms(target_amount)
                .expect("approved C2 arithmetic");
            let pair =
                upgrade13_policy::provisional_c2_two_schedule_terms(target_amount, starting_block)
                    .expect("approved C2 pair");
            assert_ok!(add_upgrade13_c2_pair_atomically(&target, pair));
            assert_eq!(
                pallet_vesting::Vesting::<Runtime>::get(&target)
                    .expect("two schedules")
                    .len(),
                2
            );
            fixtures.push((
                target,
                target_amount,
                terms.floor_release_per_interval,
                terms.final_residual,
            ));
        }

        let assert_locked =
            |block: BlockNumber, expected: fn(Balance, Balance, Balance) -> Balance| {
                System::set_block_number(block);
                for (target, amount, floor, residual) in &fixtures {
                    assert_eq!(
                        <Vesting as VestingSchedule<AccountId>>::vesting_balance(target),
                        Some(expected(*amount, *floor, *residual))
                    );
                }
            };

        assert_locked(starting_block - 1, |amount, _, _| amount);
        assert_locked(starting_block, |amount, _, _| amount);
        assert_locked(starting_block + 1, |amount, floor, _| amount - floor);
        assert_locked(last_release_block, |_, floor, residual| floor + residual);
        assert_locked(completion_block, |_, _, _| 0);
        assert_locked(completion_block + 1, |_, _, _| 0);
    });
}

#[test]
fn upgrade13_c2_pair_respects_existing_schedule_limit_and_rolls_back_atomically() {
    let (mut ext, _) = new_test_ext(4, 0, 4, 4);
    ext.execute_with(|| {
        let max_schedules = <Runtime as pallet_vesting::Config>::MAX_VESTING_SCHEDULES;
        assert_eq!(max_schedules, 128);
        let dummy_locked = <Runtime as pallet_vesting::Config>::MinVestedTransfer::get();
        let pair = upgrade13_policy::provisional_c2_two_schedule_terms(
            upgrade13_policy::FOUNDING_ALLOCATION_TARGETS[0],
            100,
        )
        .expect("approved pair");

        let exactly_fits = account(220);
        assert_ok!(Balances::force_set_balance(
            RuntimeOrigin::root(),
            MultiAddress::Id(exactly_fits.clone()),
            9_000_000 * DECIMALS,
        ));
        for _ in 0..(max_schedules - 2) {
            assert_ok!(
                <Vesting as VestingSchedule<AccountId>>::add_vesting_schedule(
                    &exactly_fits,
                    dummy_locked,
                    dummy_locked,
                    10,
                )
            );
        }
        assert_ok!(add_upgrade13_c2_pair_atomically(&exactly_fits, pair));
        assert_eq!(
            pallet_vesting::Vesting::<Runtime>::get(&exactly_fits)
                .expect("full schedule vector")
                .len() as u32,
            max_schedules
        );

        let cannot_fit_pair = account(221);
        assert_ok!(Balances::force_set_balance(
            RuntimeOrigin::root(),
            MultiAddress::Id(cannot_fit_pair.clone()),
            9_000_000 * DECIMALS,
        ));
        for _ in 0..(max_schedules - 1) {
            assert_ok!(
                <Vesting as VestingSchedule<AccountId>>::add_vesting_schedule(
                    &cannot_fit_pair,
                    dummy_locked,
                    dummy_locked,
                    10,
                )
            );
        }
        let schedule_count_before = pallet_vesting::Vesting::<Runtime>::get(&cannot_fit_pair)
            .expect("existing schedules")
            .len();
        let lock_before =
            <Vesting as VestingSchedule<AccountId>>::vesting_balance(&cannot_fit_pair);
        let balance_before = Balances::free_balance(&cannot_fit_pair);

        assert_eq!(
            add_upgrade13_c2_pair_atomically(&cannot_fit_pair, pair),
            Err(pallet_vesting::Error::<Runtime>::AtMaxVestingSchedules.into())
        );
        assert_eq!(
            pallet_vesting::Vesting::<Runtime>::get(&cannot_fit_pair)
                .expect("unchanged existing schedules")
                .len(),
            schedule_count_before
        );
        assert_eq!(
            <Vesting as VestingSchedule<AccountId>>::vesting_balance(&cannot_fit_pair),
            lock_before
        );
        assert_eq!(Balances::free_balance(&cannot_fit_pair), balance_before);
    });
}

#[test]
fn application_pallet_indices_and_call_prefixes_are_stable() {
    assert_eq!(<PalletInfo as PalletInfoTrait>::index::<System>(), Some(0));
    assert_eq!(
        <PalletInfo as PalletInfoTrait>::index::<RewardReserve>(),
        Some(13)
    );
    assert_eq!(
        <PalletInfo as PalletInfoTrait>::index::<Multisig>(),
        Some(14)
    );
    assert_eq!(<PalletInfo as PalletInfoTrait>::index::<Proxy>(), Some(15));
    assert_eq!(<PalletInfo as PalletInfoTrait>::index::<Assets>(), Some(16));
    assert_eq!(<PalletInfo as PalletInfoTrait>::index::<Nfts>(), Some(17));
    assert_eq!(
        <PalletInfo as PalletInfoTrait>::index::<EraWorlds>(),
        Some(18)
    );
    assert_eq!(
        <PalletInfo as PalletInfoTrait>::index::<IssuanceCap>(),
        Some(19)
    );

    let owner = account(1);
    let asset_call = RuntimeCall::Assets(pallet_assets::Call::create {
        id: 7,
        admin: MultiAddress::Id(owner.clone()),
        min_balance: 1,
    });
    let nft_call = RuntimeCall::Nfts(pallet_nfts::Call::create {
        admin: MultiAddress::Id(owner),
        config: Default::default(),
    });
    let world_call = RuntimeCall::EraWorlds(pallet_era_worlds::Call::register_world {
        world_id: b"encoding-proof"
            .to_vec()
            .try_into()
            .expect("bounded world id"),
        commitment: [9; 32],
    });
    assert_eq!(asset_call.encode()[0], 16);
    assert_eq!(nft_call.encode()[0], 17);
    assert_eq!(world_call.encode()[0], 18);
    assert_eq!(asset_call.encode()[1], 0);
    assert_eq!(nft_call.encode()[1], 0);
    assert_eq!(world_call.encode()[1], 0);
}

#[test]
fn asset_supply_is_disjoint_from_native_etkn_and_reward_accounting() {
    let (mut ext, _) = new_test_ext(4, 0, 4, 4);
    ext.execute_with(|| {
        let owner = account(1);
        let recipient = account(2);
        let delegate = account(3);
        let native_issuance = Balances::total_issuance();
        let reward_liabilities = pallet_reward_reserve::CommittedLiabilities::<Runtime>::get();
        let reward_budget_used = pallet_reward_reserve::AnnualBudgetUsed::<Runtime>::get();

        assert_ok!(Assets::create(
            RuntimeOrigin::signed(owner.clone()),
            7,
            MultiAddress::Id(owner.clone()),
            1,
        ));
        assert_ok!(Assets::set_metadata(
            RuntimeOrigin::signed(owner.clone()),
            7,
            b"Independent application asset".to_vec(),
            b"APP".to_vec(),
            6,
        ));
        assert_ok!(Assets::mint(
            RuntimeOrigin::signed(owner.clone()),
            7,
            MultiAddress::Id(owner.clone()),
            1_000_000,
        ));
        assert_eq!(Assets::total_supply(7), 1_000_000);
        assert_eq!(Balances::total_issuance(), native_issuance);

        assert_ok!(Assets::transfer(
            RuntimeOrigin::signed(owner.clone()),
            7,
            MultiAddress::Id(recipient.clone()),
            100_000,
        ));
        assert_ok!(Assets::approve_transfer(
            RuntimeOrigin::signed(recipient.clone()),
            7,
            MultiAddress::Id(delegate.clone()),
            10_000,
        ));
        assert_ok!(Assets::transfer_approved(
            RuntimeOrigin::signed(delegate),
            7,
            MultiAddress::Id(recipient),
            MultiAddress::Id(owner.clone()),
            10_000,
        ));
        assert_ok!(Assets::burn(
            RuntimeOrigin::signed(owner.clone()),
            7,
            MultiAddress::Id(owner),
            50_000,
        ));
        assert_eq!(Assets::total_supply(7), 950_000);
        assert_eq!(Balances::total_issuance(), native_issuance);
        assert_eq!(
            pallet_reward_reserve::CommittedLiabilities::<Runtime>::get(),
            reward_liabilities
        );
        assert_eq!(
            pallet_reward_reserve::AnnualBudgetUsed::<Runtime>::get(),
            reward_budget_used
        );
    });
}

#[test]
fn application_force_origins_and_metadata_bounds_fail_closed() {
    let (mut ext, _) = new_test_ext(4, 0, 4, 4);
    ext.execute_with(|| {
        let owner = account(1);
        assert_noop!(
            Assets::force_create(
                RuntimeOrigin::root(),
                99,
                MultiAddress::Id(owner.clone()),
                false,
                1,
            ),
            DispatchError::BadOrigin
        );
        assert_noop!(
            Nfts::force_create(
                RuntimeOrigin::root(),
                MultiAddress::Id(owner.clone()),
                Default::default(),
            ),
            DispatchError::BadOrigin
        );
        assert_noop!(
            EraWorlds::pause(RuntimeOrigin::root()),
            DispatchError::BadOrigin
        );
        assert_noop!(
            EraWorlds::pause(RuntimeOrigin::signed(owner.clone())),
            DispatchError::BadOrigin
        );

        assert_ok!(Assets::create(
            RuntimeOrigin::signed(owner.clone()),
            55,
            MultiAddress::Id(owner.clone()),
            1,
        ));
        assert_noop!(
            Assets::set_metadata(
                RuntimeOrigin::signed(owner),
                55,
                vec![b'x'; 65],
                b"X".to_vec(),
                0,
            ),
            pallet_assets::Error::<Runtime>::BadMetadata
        );
        assert_eq!(Assets::total_supply(55), 0);
    });
}

#[test]
fn nft_ownership_metadata_and_locks_are_bounded_and_native_neutral() {
    let (mut ext, _) = new_test_ext(4, 0, 4, 4);
    ext.execute_with(|| {
        let owner = account(1);
        let recipient = account(2);
        let native_issuance = Balances::total_issuance();
        assert_ok!(Nfts::create(
            RuntimeOrigin::signed(owner.clone()),
            MultiAddress::Id(owner.clone()),
            Default::default(),
        ));
        assert_ok!(Nfts::mint(
            RuntimeOrigin::signed(owner.clone()),
            0,
            1,
            MultiAddress::Id(owner.clone()),
            None,
        ));
        let commitment: frame_support::BoundedVec<u8, NftStringLimit> =
            b"bafy-content-addressed-commitment"
                .to_vec()
                .try_into()
                .expect("bounded metadata");
        assert_ok!(Nfts::set_metadata(
            RuntimeOrigin::signed(owner.clone()),
            0,
            1,
            commitment,
        ));
        assert!(frame_support::BoundedVec::<u8, NftStringLimit>::try_from(vec![0; 129]).is_err());
        assert_ok!(Nfts::lock_item_transfer(
            RuntimeOrigin::signed(owner.clone()),
            0,
            1
        ));
        assert_noop!(
            Nfts::transfer(
                RuntimeOrigin::signed(owner.clone()),
                0,
                1,
                MultiAddress::Id(recipient.clone()),
            ),
            pallet_nfts::Error::<Runtime>::ItemLocked
        );
        assert_ok!(Nfts::unlock_item_transfer(
            RuntimeOrigin::signed(owner.clone()),
            0,
            1
        ));
        assert_ok!(Nfts::transfer(
            RuntimeOrigin::signed(owner),
            0,
            1,
            MultiAddress::Id(recipient.clone()),
        ));
        assert_eq!(
            pallet_nfts::Item::<Runtime>::get(0, 1)
                .expect("item exists")
                .owner,
            recipient
        );
        assert_eq!(Balances::total_issuance(), native_issuance);
    });
}

#[test]
fn world_registration_is_bounded_owner_controlled_and_native_neutral() {
    let (mut ext, _) = new_test_ext(4, 0, 4, 4);
    ext.execute_with(|| {
        let owner = account(1);
        let other = account(2);
        let world_id: pallet_era_worlds::WorldIdOf<Runtime> = b"era-world-alpha"
            .to_vec()
            .try_into()
            .expect("bounded world id");
        let native_issuance = Balances::total_issuance();
        assert_ok!(EraWorlds::register_world(
            RuntimeOrigin::signed(owner.clone()),
            world_id.clone(),
            [10; 32],
        ));
        assert_noop!(
            EraWorlds::update_commitment(RuntimeOrigin::signed(other), world_id.clone(), [11; 32],),
            pallet_era_worlds::Error::<Runtime>::NotOwner
        );
        assert_ok!(EraWorlds::update_commitment(
            RuntimeOrigin::signed(owner.clone()),
            world_id.clone(),
            [12; 32],
        ));
        assert_eq!(
            EraWorlds::worlds(&world_id)
                .expect("world exists")
                .commitment,
            [12; 32]
        );
        assert_eq!(Balances::total_issuance(), native_issuance);
        assert_ok!(EraWorlds::deregister_world(
            RuntimeOrigin::signed(owner),
            world_id
        ));
        assert_eq!(Balances::total_issuance(), native_issuance);
    });
}
const CAP_TEST_ETKN: Balance = 1_000_000_000_000_000_000;
const CAP_TEST_BASELINE: Balance = 100_000_000 * CAP_TEST_ETKN;
const CAP_TEST_ALLOWANCE: Balance = 900_000_000 * CAP_TEST_ETKN;
const CAP_TEST_LIFETIME_CEILING: Balance = 1_000_000_000 * CAP_TEST_ETKN;

fn issuance_cap_test_ext(
    balances: Vec<(AccountId, Balance)>,
    sudo_key: Option<AccountId>,
) -> sp_io::TestExternalities {
    let mut storage = frame_system::GenesisConfig::<Runtime>::default()
        .build_storage()
        .expect("system genesis storage");
    pallet_balances::GenesisConfig::<Runtime> {
        balances,
        dev_accounts: None,
    }
    .assimilate_storage(&mut storage)
    .expect("balances genesis storage");
    pallet_sudo::GenesisConfig::<Runtime> { key: sudo_key }
        .assimilate_storage(&mut storage)
        .expect("sudo genesis storage");

    let mut ext = sp_io::TestExternalities::new(storage);
    ext.execute_with(|| {
        System::set_block_number(1);
        StorageVersion::new(1).put::<IssuanceCap>();
        issuance_cap::RemainingAllowance::<Runtime>::kill();
        issuance_cap::V13MigrationCompleted::<Runtime>::kill();
        frame_support::storage::unhashed::kill(&issuance_cap::v13_migration_input_key());
        System::reset_events();
    });
    ext
}

fn initialize_issuance_cap() {
    StorageVersion::new(1).put::<IssuanceCap>();
    issuance_cap::RemainingAllowance::<Runtime>::put(CAP_TEST_ALLOWANCE);
    assert_eq!(StorageVersion::get::<IssuanceCap>(), StorageVersion::new(1));
    assert_eq!(IssuanceCap::remaining_allowance(), Some(CAP_TEST_ALLOWANCE));
}

fn v13_migration_test_ext() -> sp_io::TestExternalities {
    let balances = v13_migration::test_pre_migration_balances();
    assert_eq!(
        balances.iter().map(|(_, amount)| *amount).sum::<Balance>(),
        v13_migration::R6R3_PRE_MIGRATION_ISSUANCE
    );
    let mut storage = frame_system::GenesisConfig::<Runtime>::default()
        .build_storage()
        .expect("system genesis storage");
    pallet_balances::GenesisConfig::<Runtime> {
        balances,
        dev_accounts: None,
    }
    .assimilate_storage(&mut storage)
    .expect("exact V13 balances genesis storage");

    let mut ext = sp_io::TestExternalities::new(storage);
    ext.execute_with(|| {
        System::set_block_number(1_830_990);
        v13_migration::test_initialize_community_staking();
        frame_support::storage::unhashed::put(
            &issuance_cap::v13_migration_input_key(),
            &v13_migration::test_input(),
        );
        <AllPalletsWithSystem as BeforeAllRuntimeMigrations>::before_all_runtime_migrations();
        assert_eq!(StorageVersion::get::<IssuanceCap>(), StorageVersion::new(1));
        assert_eq!(IssuanceCap::remaining_allowance(), None);
        assert_eq!(issuance_cap::V13MigrationCompleted::<Runtime>::get(), None);
        System::reset_events();
    });
    ext
}

fn v13_storage_root() -> Vec<u8> {
    sp_io::storage::root(StateVersion::V1)
}

fn assert_single_block_migration_panics() {
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            <<Runtime as frame_system::Config>::SingleBlockMigrations as OnRuntimeUpgrade>::on_runtime_upgrade()
        }))
        .is_err()
    );
}

#[test]
fn v13_single_block_migration_repairs_the_r6r3_state_in_the_required_order() {
    assert_same_type::<
        <Runtime as frame_system::Config>::SingleBlockMigrations,
        v13_migration::V13Migration,
    >();
    assert_eq!(
        CAP_TEST_BASELINE,
        upgrade13_policy::TARGET_RETAINED_ISSUANCE
    );
    assert_eq!(
        CAP_TEST_ALLOWANCE,
        upgrade13_policy::MAXIMUM_POST_CORRECTION_NEW_ISSUANCE
    );
    assert_eq!(
        CAP_TEST_LIFETIME_CEILING,
        upgrade13_policy::ABSOLUTE_LIFETIME_CAP
    );

    let mut ext = v13_migration_test_ext();
    ext.execute_with(|| {
        // This is the exact R6R3 failure shape: FRAME already wrote version 1 while both
        // independent economic-state values remain absent.
        assert_eq!(StorageVersion::get::<IssuanceCap>(), StorageVersion::new(1));
        assert_eq!(IssuanceCap::remaining_allowance(), None);
        assert_eq!(issuance_cap::V13MigrationCompleted::<Runtime>::get(), None);
        assert_eq!(
            Balances::total_issuance(),
            v13_migration::R6R3_PRE_MIGRATION_ISSUANCE
        );

        let weight =
            <<Runtime as frame_system::Config>::SingleBlockMigrations as OnRuntimeUpgrade>::on_runtime_upgrade();
        assert_eq!(weight, v13_migration::declared_weight());
        assert_eq!(Balances::total_issuance(), CAP_TEST_BASELINE);
        assert_eq!(IssuanceCap::remaining_allowance(), Some(CAP_TEST_ALLOWANCE));
        let marker = issuance_cap::V13MigrationCompleted::<Runtime>::get()
            .expect("completion marker written after exact reconciliation");
        assert_eq!(marker.migration_version, 13);
        assert_eq!(marker.completed_at, 1_830_990);
        assert_eq!(marker.input, v13_migration::test_input());
        let sudo = v13_migration::test_sudo_account();
        let sudo_account = System::account(&sudo);
        assert_eq!(
            Balances::free_balance(&sudo),
            upgrade13_policy::SUDO_TARGET_FREE
        );
        assert_eq!(sudo_account.data.reserved, 0);
        assert_eq!(sudo_account.data.frozen, 0);
        assert_eq!(
            Balances::free_balance(&sudo) - EXISTENTIAL_DEPOSIT,
            upgrade13_policy::SUDO_OPERATIONAL_RESERVE
        );
        assert_eq!(
            Balances::free_balance(&marker.input.ecosystem_custody),
            19_999_998_999_700_000_000_000_000
        );
        let infrastructure_deposits = v13_migration::test_ecosystem_preserved_accounts()
            .iter()
            .map(Balances::free_balance)
            .sum::<Balance>();
        assert_eq!(infrastructure_deposits, 200_000_000_000_000);
        assert_eq!(
            Balances::free_balance(&marker.input.ecosystem_custody)
                + Balances::free_balance(&sudo)
                + infrastructure_deposits,
            upgrade13_policy::ECOSYSTEM_POOL
        );
        assert_eq!(pallet_reward_reserve::CommittedLiabilities::<Runtime>::get(), 0);
        let community = v13_migration::test_community_source();
        assert_eq!(Balances::total_balance(&community), 0);
        assert!(pallet_staking::Bonded::<Runtime>::get(&community).is_none());
        assert!(pallet_staking::Ledger::<Runtime>::get(&community).is_none());
        assert!(pallet_staking::Nominators::<Runtime>::get(&community).is_none());
        assert!(pallet_staking::Payee::<Runtime>::get(&community).is_none());
        assert!(pallet_balances::Holds::<Runtime>::get(&community).is_empty());
        assert!(
            issuance_cap::v13_migration_input::<Runtime>().is_none(),
            "deployment input is consumed"
        );
        assert_eq!(
            Balances::free_balance(&marker.input.active_presale_custody),
            upgrade13_policy::ACTIVE_PRESALE_POOL
        );
        assert_eq!(
            Balances::free_balance(&marker.input.liquidity_custody),
            upgrade13_policy::LIQUIDITY_RESERVE_POOL
        );
    });
}

#[test]
fn v13_custody_destination_collisions_roll_back() {
    let mut duplicate = v13_migration_test_ext();
    duplicate.execute_with(|| {
        let mut input = v13_migration::test_input();
        input.ecosystem_custody = input.active_presale_custody.clone();
        frame_support::storage::unhashed::put(&issuance_cap::v13_migration_input_key(), &input);
        let before = v13_storage_root();
        assert_single_block_migration_panics();
        assert_eq!(v13_storage_root(), before);
        assert_eq!(issuance_cap::V13MigrationCompleted::<Runtime>::get(), None);
    });

    let mut alias = v13_migration_test_ext();
    alias.execute_with(|| {
        let mut input = v13_migration::test_input();
        input.ecosystem_custody = v13_migration::test_sudo_account();
        frame_support::storage::unhashed::put(&issuance_cap::v13_migration_input_key(), &input);
        let before = v13_storage_root();
        assert_single_block_migration_panics();
        assert_eq!(v13_storage_root(), before);
        assert_eq!(issuance_cap::V13MigrationCompleted::<Runtime>::get(), None);
    });
}

#[test]
fn v13_unexpected_precondition_rolls_back_and_marker_is_success_only() {
    let mut ext = v13_migration_test_ext();
    ext.execute_with(|| {
        pallet_balances::TotalIssuance::<Runtime>::put(
            v13_migration::R6R3_PRE_MIGRATION_ISSUANCE + 1,
        );
        let before = v13_storage_root();
        assert_single_block_migration_panics();
        assert_eq!(v13_storage_root(), before);
        assert_eq!(IssuanceCap::remaining_allowance(), None);
        assert_eq!(issuance_cap::V13MigrationCompleted::<Runtime>::get(), None);
        assert_eq!(
            issuance_cap::v13_migration_input::<Runtime>(),
            Some(v13_migration::test_input())
        );
    });
}

#[test]
fn v13_partial_allowance_version_and_marker_states_fail_closed() {
    let mut allowance = v13_migration_test_ext();
    allowance.execute_with(|| {
        issuance_cap::RemainingAllowance::<Runtime>::put(1);
        let before = v13_storage_root();
        assert_single_block_migration_panics();
        assert_eq!(v13_storage_root(), before);
        assert_eq!(issuance_cap::V13MigrationCompleted::<Runtime>::get(), None);
    });

    let mut version = v13_migration_test_ext();
    version.execute_with(|| {
        StorageVersion::new(0).put::<IssuanceCap>();
        let before = v13_storage_root();
        assert_single_block_migration_panics();
        assert_eq!(v13_storage_root(), before);
        assert_eq!(IssuanceCap::remaining_allowance(), None);
    });

    let mut marker = v13_migration_test_ext();
    marker.execute_with(|| {
        issuance_cap::V13MigrationCompleted::<Runtime>::put(issuance_cap::V13Completion {
            migration_version: 13,
            completed_at: System::block_number(),
            input: v13_migration::test_input(),
        });
        let before = v13_storage_root();
        assert_single_block_migration_panics();
        assert_eq!(v13_storage_root(), before);
        assert_eq!(IssuanceCap::remaining_allowance(), None);
    });
}

#[test]
fn v13_correct_second_run_changes_zero_keys_and_never_resets_allowance() {
    let mut ext = v13_migration_test_ext();
    ext.execute_with(|| {
        <v13_migration::V13Migration as OnRuntimeUpgrade>::on_runtime_upgrade();
        let marker =
            issuance_cap::V13MigrationCompleted::<Runtime>::get().expect("completed first run");
        let before = v13_storage_root();
        let weight = <v13_migration::V13Migration as OnRuntimeUpgrade>::on_runtime_upgrade();
        assert_eq!(weight, v13_migration::declared_weight());
        assert_eq!(v13_storage_root(), before);
        assert_eq!(Balances::total_issuance(), CAP_TEST_BASELINE);
        assert_eq!(IssuanceCap::remaining_allowance(), Some(CAP_TEST_ALLOWANCE));
        assert_eq!(
            issuance_cap::V13MigrationCompleted::<Runtime>::get(),
            Some(marker)
        );
    });
}

#[test]
fn v13_completed_marker_with_incorrect_issuance_or_allowance_fails_closed() {
    let mut issuance = v13_migration_test_ext();
    issuance.execute_with(|| {
        <v13_migration::V13Migration as OnRuntimeUpgrade>::on_runtime_upgrade();
        pallet_balances::TotalIssuance::<Runtime>::put(CAP_TEST_BASELINE + 1);
        let before = v13_storage_root();
        assert_single_block_migration_panics();
        assert_eq!(v13_storage_root(), before);
    });

    let mut allowance = v13_migration_test_ext();
    allowance.execute_with(|| {
        <v13_migration::V13Migration as OnRuntimeUpgrade>::on_runtime_upgrade();
        issuance_cap::RemainingAllowance::<Runtime>::put(CAP_TEST_ALLOWANCE - 1);
        let before = v13_storage_root();
        assert_single_block_migration_panics();
        assert_eq!(v13_storage_root(), before);
    });
}

#[test]
fn v13_declared_weight_covers_the_bounded_database_work() {
    let database_bound =
        frame_support::weights::constants::RocksDbWeight::get().reads_writes(640, 160);
    let declared = v13_migration::declared_weight();
    assert!(declared.ref_time() >= 750_000_000_000 + database_bound.ref_time());
    assert!(declared.proof_size() >= 3_670_016 + database_bound.proof_size());
}

#[test]
fn obsolete_issuance_cap_pallet_hook_is_neutralized() {
    let mut ext = issuance_cap_test_ext(vec![(account(1), CAP_TEST_BASELINE)], None);
    ext.execute_with(|| {
        assert_eq!(IssuanceCap::remaining_allowance(), None);
        assert_eq!(
            <IssuanceCap as Hooks<BlockNumber>>::on_runtime_upgrade(),
            Weight::zero()
        );
        assert_eq!(IssuanceCap::remaining_allowance(), None);
    });
}

#[test]
fn controlled_issuance_decrements_exactly_emits_and_old_hook_cannot_reset_it() {
    let beneficiary = account(1);
    let mut ext = issuance_cap_test_ext(vec![(beneficiary.clone(), CAP_TEST_BASELINE)], None);
    ext.execute_with(|| {
        initialize_issuance_cap();
        System::reset_events();

        let amount = 17;
        assert_eq!(
            IssuanceCap::controlled_mint_into(&beneficiary, amount),
            Ok(amount)
        );
        assert_eq!(Balances::total_issuance(), CAP_TEST_BASELINE + amount);
        assert_eq!(
            IssuanceCap::remaining_allowance(),
            Some(CAP_TEST_ALLOWANCE - amount)
        );
        assert_eq!(
            CAP_TEST_ALLOWANCE - IssuanceCap::remaining_allowance().expect("initialized"),
            amount
        );
        assert_eq!(
            System::events().last().expect("cap event").event,
            RuntimeEvent::IssuanceCap(issuance_cap::Event::Issued {
                beneficiary: beneficiary.clone(),
                amount,
                remaining_allowance: CAP_TEST_ALLOWANCE - amount,
            })
        );

        <IssuanceCap as Hooks<BlockNumber>>::on_runtime_upgrade();
        assert_eq!(StorageVersion::get::<IssuanceCap>(), StorageVersion::new(1));
        assert_eq!(
            IssuanceCap::remaining_allowance(),
            Some(CAP_TEST_ALLOWANCE - amount)
        );
        assert_eq!(Balances::total_issuance(), CAP_TEST_BASELINE + amount);
    });
}

#[test]
fn issuance_cap_exact_boundary_succeeds_and_excess_is_atomic() {
    let beneficiary = account(1);
    let mut boundary = issuance_cap_test_ext(vec![(beneficiary.clone(), CAP_TEST_BASELINE)], None);
    boundary.execute_with(|| {
        initialize_issuance_cap();
        assert_eq!(
            IssuanceCap::controlled_mint_into(&beneficiary, CAP_TEST_ALLOWANCE),
            Ok(CAP_TEST_ALLOWANCE)
        );
        assert_eq!(Balances::total_issuance(), CAP_TEST_LIFETIME_CEILING);
        assert_eq!(IssuanceCap::remaining_allowance(), Some(0));
        assert_noop!(
            IssuanceCap::controlled_mint_into(&beneficiary, 1),
            issuance_cap::Error::<Runtime>::AllowanceExceeded
        );
        assert_eq!(Balances::total_issuance(), CAP_TEST_LIFETIME_CEILING);
        assert_eq!(IssuanceCap::remaining_allowance(), Some(0));
    });

    let mut excess = issuance_cap_test_ext(vec![(account(1), CAP_TEST_BASELINE)], None);
    excess.execute_with(|| {
        initialize_issuance_cap();
        System::reset_events();
        assert_noop!(
            IssuanceCap::controlled_mint_into(&account(1), CAP_TEST_ALLOWANCE + 1),
            issuance_cap::Error::<Runtime>::AllowanceExceeded
        );
        assert_eq!(Balances::total_issuance(), CAP_TEST_BASELINE);
        assert_eq!(IssuanceCap::remaining_allowance(), Some(CAP_TEST_ALLOWANCE));
        assert!(System::events().is_empty());
    });
}

#[test]
fn issuance_cap_arithmetic_and_downstream_failures_roll_back() {
    let mut ext = issuance_cap_test_ext(vec![(account(1), CAP_TEST_BASELINE)], None);
    ext.execute_with(|| {
        initialize_issuance_cap();
        System::reset_events();

        assert_noop!(
            IssuanceCap::controlled_mint_into(&account(1), 0),
            issuance_cap::Error::<Runtime>::ZeroIssuance
        );

        pallet_balances::TotalIssuance::<Runtime>::put(Balance::MAX);
        issuance_cap::RemainingAllowance::<Runtime>::put(1);
        assert_noop!(
            IssuanceCap::controlled_mint_into(&account(1), 1),
            issuance_cap::Error::<Runtime>::ArithmeticOverflow
        );

        pallet_balances::TotalIssuance::<Runtime>::put(CAP_TEST_LIFETIME_CEILING);
        assert_noop!(
            IssuanceCap::controlled_mint_into(&account(1), 1),
            issuance_cap::Error::<Runtime>::LifetimeCeilingExceeded
        );

        pallet_balances::TotalIssuance::<Runtime>::put(CAP_TEST_BASELINE);
        issuance_cap::RemainingAllowance::<Runtime>::put(CAP_TEST_ALLOWANCE + 1);
        assert_noop!(
            IssuanceCap::controlled_mint_into(&account(1), 1),
            issuance_cap::Error::<Runtime>::InvalidStoredAllowance
        );

        issuance_cap::RemainingAllowance::<Runtime>::put(CAP_TEST_ALLOWANCE);
        let events_before = System::events().len();
        assert_noop!(
            IssuanceCap::controlled_mint_into(&account(2), EXISTENTIAL_DEPOSIT - 1),
            issuance_cap::Error::<Runtime>::DownstreamMintFailed
        );
        assert_eq!(Balances::total_issuance(), CAP_TEST_BASELINE);
        assert_eq!(IssuanceCap::remaining_allowance(), Some(CAP_TEST_ALLOWANCE));
        assert_eq!(Balances::free_balance(account(2)), 0);
        assert_eq!(System::events().len(), events_before);
    });
}

#[test]
fn burns_slashes_and_reaping_never_restore_allowance() {
    let doomed = EXISTENTIAL_DEPOSIT + 7;
    let survivor = account(1);
    let reaped = account(2);
    let mut ext = issuance_cap_test_ext(
        vec![
            (survivor.clone(), CAP_TEST_BASELINE - doomed),
            (reaped.clone(), doomed),
        ],
        None,
    );
    ext.execute_with(|| {
        initialize_issuance_cap();
        let allowance = IssuanceCap::remaining_allowance();
        let mut issuance = Balances::total_issuance();

        assert_ok!(Balances::burn(
            RuntimeOrigin::signed(survivor.clone()),
            11,
            true
        ));
        assert_eq!(Balances::total_issuance(), issuance - 11);
        assert_eq!(IssuanceCap::remaining_allowance(), allowance);
        issuance -= 11;

        let (slashed, leftover) = <Balances as Currency<AccountId>>::slash(&survivor, 13);
        assert_eq!(leftover, 0);
        drop(slashed);
        assert_eq!(Balances::total_issuance(), issuance - 13);
        assert_eq!(IssuanceCap::remaining_allowance(), allowance);
        issuance -= 13;

        assert_ok!(Balances::burn(
            RuntimeOrigin::signed(reaped.clone()),
            8,
            false
        ));
        assert_eq!(Balances::free_balance(&reaped), 0);
        assert!(Balances::total_issuance() < issuance);
        assert_eq!(IssuanceCap::remaining_allowance(), allowance);
    });
}

#[test]
fn transfers_reserves_and_unreserves_do_not_consume_allowance() {
    let source = account(1);
    let destination = account(2);
    let mut ext = issuance_cap_test_ext(vec![(source.clone(), CAP_TEST_BASELINE)], None);
    ext.execute_with(|| {
        initialize_issuance_cap();
        let allowance = IssuanceCap::remaining_allowance();
        let issuance = Balances::total_issuance();

        assert_ok!(<Balances as Currency<AccountId>>::transfer(
            &source,
            &destination,
            2 * EXISTENTIAL_DEPOSIT,
            ExistenceRequirement::AllowDeath,
        ));
        assert_ok!(<Balances as ReservableCurrency<AccountId>>::reserve(
            &destination,
            EXISTENTIAL_DEPOSIT
        ));
        assert_eq!(
            <Balances as ReservableCurrency<AccountId>>::unreserve(
                &destination,
                EXISTENTIAL_DEPOSIT
            ),
            0
        );
        assert_eq!(Balances::total_issuance(), issuance);
        assert_eq!(IssuanceCap::remaining_allowance(), allowance);
    });
}

#[test]
fn root_sudo_and_raw_storage_cannot_bypass_the_issuance_cap() {
    let sudo_key = account(1);
    let beneficiary = account(2);
    let mut ext = issuance_cap_test_ext(
        vec![(sudo_key.clone(), CAP_TEST_BASELINE)],
        Some(sudo_key.clone()),
    );
    ext.execute_with(|| {
        initialize_issuance_cap();
        let issuance = Balances::total_issuance();
        let allowance = IssuanceCap::remaining_allowance();

        let force_increase =
            RuntimeCall::Balances(pallet_balances::Call::force_adjust_total_issuance {
                direction: pallet_balances::AdjustmentDirection::Increase,
                delta: 1,
            });
        assert!(!<IssuanceCallFilter as Contains<RuntimeCall>>::contains(
            &force_increase
        ));
        assert_eq!(
            force_increase
                .clone()
                .dispatch(RuntimeOrigin::signed(sudo_key.clone()))
                .expect_err("the root-only issuance call must be filtered before origin checking")
                .error,
            DispatchError::from(frame_system::Error::<Runtime>::CallFiltered)
        );

        let force_set = RuntimeCall::Balances(pallet_balances::Call::force_set_balance {
            who: MultiAddress::Id(beneficiary),
            new_free: EXISTENTIAL_DEPOSIT,
        });
        let sudo = RuntimeCall::Sudo(pallet_sudo::Call::sudo {
            call: Box::new(force_set),
        });
        assert!(!<IssuanceCallFilter as Contains<RuntimeCall>>::contains(
            &sudo
        ));
        assert_eq!(
            sudo.dispatch(RuntimeOrigin::signed(sudo_key.clone()))
                .expect_err("sudo issuance must be filtered")
                .error,
            DispatchError::from(frame_system::Error::<Runtime>::CallFiltered)
        );

        let nested_sudo = RuntimeCall::Sudo(pallet_sudo::Call::sudo_as {
            who: MultiAddress::Id(sudo_key),
            call: Box::new(RuntimeCall::Sudo(pallet_sudo::Call::sudo {
                call: Box::new(RuntimeCall::Balances(
                    pallet_balances::Call::force_adjust_total_issuance {
                        direction: pallet_balances::AdjustmentDirection::Increase,
                        delta: 1,
                    },
                )),
            })),
        });
        assert!(!<IssuanceCallFilter as Contains<RuntimeCall>>::contains(
            &nested_sudo
        ));

        for protected_key in [
            frame_support::storage::storage_prefix(b"Balances", b"TotalIssuance").to_vec(),
            frame_support::storage::storage_prefix(b"IssuanceCap", b"RemainingAllowance").to_vec(),
            issuance_cap::v13_migration_input_key().to_vec(),
            frame_support::storage::storage_prefix(b"System", b"Account").to_vec(),
        ] {
            let raw_write = RuntimeCall::System(frame_system::Call::set_storage {
                items: vec![(protected_key, vec![0])],
            });
            assert!(!<IssuanceCallFilter as Contains<RuntimeCall>>::contains(
                &raw_write
            ));
        }

        for protected_prefix in [
            sp_io::hashing::twox_128(b"IssuanceCap").to_vec(),
            sp_io::hashing::twox_128(issuance_cap::V13_MIGRATION_INPUT_PALLET_PREFIX).to_vec(),
        ] {
            let raw_kill = RuntimeCall::System(frame_system::Call::kill_prefix {
                prefix: protected_prefix,
                subkeys: 1,
            });
            assert!(!<IssuanceCallFilter as Contains<RuntimeCall>>::contains(
                &raw_kill
            ));
        }

        let unrelated = RuntimeCall::System(frame_system::Call::set_storage {
            items: vec![(
                frame_support::storage::storage_prefix(b"Sudo", b"Key").to_vec(),
                vec![0],
            )],
        });
        assert!(<IssuanceCallFilter as Contains<RuntimeCall>>::contains(
            &unrelated
        ));

        let decrease = RuntimeCall::Balances(pallet_balances::Call::force_adjust_total_issuance {
            direction: pallet_balances::AdjustmentDirection::Decrease,
            delta: 1,
        });
        assert!(<IssuanceCallFilter as Contains<RuntimeCall>>::contains(
            &decrease
        ));
        assert_eq!(Balances::total_issuance(), issuance);
        assert_eq!(IssuanceCap::remaining_allowance(), allowance);
    });
}

#[test]
fn submit_prediction_scale_call_shape_remains_unchanged() {
    let domain = pallet_ai_predictions::PredictionDomain::Enterprise;
    let call = pallet_ai_predictions::Call::<Runtime>::submit_prediction {
        model_id: 7,
        domain,
        category_code: vec![1, 2],
        prediction_hash: vec![3, 4],
        metadata_uri: vec![5, 6],
        confidence: 99,
        expires_at: 42,
    };
    assert_eq!(
        call.encode(),
        (
            2u8,
            7u64,
            domain,
            vec![1u8, 2],
            vec![3u8, 4],
            vec![5u8, 6],
            99u8,
            42u32,
        )
            .encode()
    );
}
