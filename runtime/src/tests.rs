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
    new_test_ext_with_validator_bond(
        validator_candidates,
        nominators,
        desired,
        minimum,
        2_000 * DECIMALS,
    )
}

fn new_test_ext_with_validator_bond(
    validator_candidates: usize,
    nominators: usize,
    desired: u32,
    minimum: u32,
    validator_bond: Balance,
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
            validator_bond + index as u128 * 100 * DECIMALS,
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
    assert_eq!(VERSION.spec_version, 15);
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

    #[cfg(all(not(feature = "try-runtime"), not(feature = "runtime-benchmarks")))]
    let expected_api_hash = [
        0x86, 0x78, 0xec, 0x76, 0x6b, 0x36, 0x0e, 0x11, 0xd5, 0xac, 0xcd, 0x6e, 0x3b, 0x17, 0xaf,
        0x37, 0xda, 0xe4, 0xdb, 0x89, 0xa5, 0x79, 0x74, 0x25, 0xf8, 0x32, 0x47, 0x6a, 0xb4, 0x08,
        0x30, 0x7b,
    ];
    #[cfg(all(feature = "try-runtime", not(feature = "runtime-benchmarks")))]
    let expected_api_hash = [
        0xb1, 0xd3, 0xb5, 0x5e, 0x14, 0x5b, 0x55, 0x14, 0xb0, 0x38, 0xdb, 0xff, 0x33, 0xef, 0x4b,
        0xc8, 0x16, 0x3f, 0xef, 0x28, 0x9d, 0xc9, 0xc4, 0xcd, 0x3c, 0xc1, 0xc1, 0xcc, 0xb3, 0x06,
        0xf8, 0x1b,
    ];
    #[cfg(all(feature = "runtime-benchmarks", not(feature = "try-runtime")))]
    let expected_api_hash = [
        0x0e, 0xa5, 0xee, 0xab, 0xbe, 0xac, 0xef, 0xc3, 0x66, 0xf5, 0x6b, 0xf2, 0x86, 0x10, 0x85,
        0x3a, 0xb6, 0xde, 0x33, 0x03, 0x9f, 0x35, 0x15, 0x39, 0xe3, 0xf8, 0xb3, 0x33, 0x14, 0x9b,
        0xa0, 0x5f,
    ];
    #[cfg(all(feature = "runtime-benchmarks", feature = "try-runtime"))]
    let expected_api_hash = [
        0xf8, 0x71, 0x9b, 0x7e, 0xa4, 0x05, 0x2d, 0x0c, 0xe3, 0x13, 0xb7, 0xec, 0xc1, 0xad, 0x75,
        0x39, 0x81, 0x13, 0x28, 0xdd, 0xe4, 0x03, 0xe3, 0x4f, 0x2d, 0xcf, 0x56, 0xb6, 0x8b, 0x71,
        0x60, 0xb7,
    ];
    let amm_id = <dyn era_v14_amm::EraV14AmmRuntimeApi<Block> as sp_api::RuntimeApiInfo>::ID;
    assert_eq!(VERSION.apis.iter().filter(|(id, _)| *id == amm_id).copied().collect::<Vec<_>>(), vec![(amm_id, 1)]);
    let assets_id=<dyn era_v14_application_primitives::assets::runtime_api::EraV14AssetsApiV1<Block> as sp_api::RuntimeApiInfo>::ID;
    assert_eq!(VERSION.apis.iter().filter(|(id,_)|*id==assets_id).copied().collect::<Vec<_>>(),vec![(assets_id,1)]);
    let existing_apis: Vec<_> = VERSION.apis.iter().filter(|(id, _)| *id != amm_id && *id != assets_id).copied().collect();
    assert_eq!(sp_io::hashing::blake2_256(&existing_apis.encode()), expected_api_hash);
    // FreshGenesis and state-backed custody/liability bindings intentionally change metadata.
    // Keep the API/type assertions above and validate the public schema explicitly; the actual
    // complete metadata hash is recorded with the built fresh release, not the old upgrade.
    macro_rules! check_fresh_metadata {
        ($metadata:expr) => {{
            let metadata = $metadata;
            let fresh = metadata.pallets.iter().find(|p| p.name == "FreshGenesis").unwrap();
            assert_eq!(fresh.index, 23);
            assert!(fresh.calls.is_none());
            let entries = &fresh.storage.as_ref().unwrap().entries;
            assert_eq!(entries.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), vec!["Enabled", "Signers"]);
            let custody = metadata.pallets.iter().find(|p| p.name == "FounderCustody").unwrap();
            assert!(!custody.constants.iter().any(|c| c.name == "Signers"));
            let budget = metadata.pallets.iter().find(|p| p.name == "SecurityBudget").unwrap();
            assert!(!budget.constants.iter().any(|c| c.name == "ExistingEarnedRewardLiability"));
        }};
    }
    match Runtime::metadata().1 {
        frame_metadata::RuntimeMetadata::V14(metadata) => check_fresh_metadata!(metadata),
        frame_metadata::RuntimeMetadata::V15(metadata) => check_fresh_metadata!(metadata),
        metadata => panic!("unexpected metadata version: {metadata:?}"),
    }
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
    assert_eq!(Nfts::in_code_storage_version(), StorageVersion::new(2));
    assert_eq!(EraWorlds::in_code_storage_version(), StorageVersion::new(1));
    assert_eq!(Staking::in_code_storage_version(), StorageVersion::new(16));
    assert_eq!(
        AiPredictions::in_code_storage_version(),
        StorageVersion::new(3)
    );
    assert_eq!(
        RewardReserve::in_code_storage_version(),
        StorageVersion::new(3)
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

const LEGACY_REWARD_TEST_BALANCE: Balance = 20_000_000 * DECIMALS;

fn prepare_and_activate_reward_system(source: &AccountId) {
    for (destination, amount) in [
        (
            RewardReserve::reward_pot_account(),
            LEGACY_REWARD_TEST_BALANCE,
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
fn live_v1_to_v3_migration_preserves_accounting_and_skips_only_partial_era() {
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
            StorageVersion::new(3)
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
    assert_eq!(InitialRewardReserve::get(), 0);
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
            StorageVersion::new(3)
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

#[cfg(feature = "try-runtime")]
#[test]
fn reward_reserve_try_runtime_version_matrix_is_strict_and_accepts_v2() {
    for version in 0..=3 {
        let (mut ext, _) = new_test_ext(4, 0, 4, 4);
        ext.execute_with(|| {
            StorageVersion::new(version).put::<RewardReserve>();
            let before = sp_io::storage::root(StateVersion::V1);
            assert!(<RewardReserve as Hooks<BlockNumber>>::pre_upgrade().is_ok());
            assert_eq!(sp_io::storage::root(StateVersion::V1), before);
        });
    }

    let (mut absent, _) = new_test_ext(4, 0, 4, 4);
    absent.execute_with(|| {
        sp_io::storage::clear(&StorageVersion::storage_key::<RewardReserve>());
        let before = sp_io::storage::root(StateVersion::V1);
        assert!(<RewardReserve as Hooks<BlockNumber>>::pre_upgrade().is_ok());
        assert_eq!(sp_io::storage::root(StateVersion::V1), before);
    });

    for bytes in [
        vec![],
        vec![2],
        vec![2, 0, 0],
        vec![4, 0],
        vec![255, 0],
        vec![255, 255],
        vec![0; 4_096],
    ] {
        let (mut ext, _) = new_test_ext(4, 0, 4, 4);
        ext.execute_with(|| {
            sp_io::storage::set(&StorageVersion::storage_key::<RewardReserve>(), &bytes);
            let before = sp_io::storage::root(StateVersion::V1);
            assert!(<RewardReserve as Hooks<BlockNumber>>::pre_upgrade().is_err());
            assert_eq!(sp_io::storage::root(StateVersion::V1), before);
        });
    }
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
            LEGACY_REWARD_TEST_BALANCE + 70
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
            LEGACY_REWARD_TEST_BALANCE + 70
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
            LEGACY_REWARD_TEST_BALANCE + 70
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
            LEGACY_REWARD_TEST_BALANCE - RewardPotSafetyFloor::get() + 101,
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
            DispatchError::BadOrigin
        );
        assert_noop!(
            RewardReserve::spend_ecosystem_treasury(
                RuntimeOrigin::signed(payer.clone()),
                beneficiary.clone(),
                31,
            ),
            DispatchError::BadOrigin
        );
        assert_noop!(
            RewardReserve::spend_ecosystem_treasury(RuntimeOrigin::root(), beneficiary, 31,),
            DispatchError::BadOrigin
        );
        assert_eq!(
            Balances::free_balance(RewardReserve::ecosystem_treasury_account()),
            EXISTENTIAL_DEPOSIT + 31
        );
        assert_eq!(
            Balances::free_balance(RewardReserve::reward_pot_account()),
            reward_before
        );
        assert_eq!(
            RewardReserve::committed_liabilities(),
            LEGACY_REWARD_TEST_BALANCE - RewardPotSafetyFloor::get() + 101
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
        fund_reward_pot(&source, LEGACY_REWARD_TEST_BALANCE);
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
                LEGACY_REWARD_TEST_BALANCE,
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
                LEGACY_REWARD_TEST_BALANCE,
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
            DispatchError::BadOrigin
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
        assert_eq!(Balances::free_balance(&reward), LEGACY_REWARD_TEST_BALANCE);
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
fn v14_native_dapp_call_and_opaque_extrinsic_vectors_match_runtime_codec() {
    let destination = account(0x22);
    let native_transfer = RuntimeCall::Balances(pallet_balances::Call::transfer_keep_alive {
        dest: MultiAddress::Id(destination.clone()),
        value: 42,
    });
    let asset_create = RuntimeCall::Assets(pallet_assets::Call::create {
        id: 7,
        admin: MultiAddress::Id(destination.clone()),
        min_balance: 1,
    });
    let asset_transfer = RuntimeCall::Assets(pallet_assets::Call::transfer {
        id: 7,
        target: MultiAddress::Id(destination.clone()),
        amount: 1_000,
    });
    let nft_create = RuntimeCall::Nfts(pallet_nfts::Call::create {
        admin: MultiAddress::Id(destination.clone()),
        config: Default::default(),
    });
    let nft_mint = RuntimeCall::Nfts(pallet_nfts::Call::mint {
        collection: 3,
        item: 9,
        mint_to: MultiAddress::Id(destination.clone()),
        witness_data: None,
    });
    let nft_retirement = RuntimeCall::Nfts(pallet_nfts::Call::continue_collection_retirement {
        collection: 3,
        limit: 685,
    });
    let world_registration = RuntimeCall::EraWorlds(pallet_era_worlds::Call::register_world {
        world_id: b"world-1".to_vec().try_into().expect("bounded world id"),
        commitment: [0x55; 32],
    });

    let mut native_expected = vec![2, 3, 0];
    native_expected.extend([0x22; 32]);
    native_expected.push(0xa8);
    assert_eq!(native_transfer.encode(), native_expected);

    let mut asset_create_expected = vec![16, 0];
    asset_create_expected.extend(7u32.to_le_bytes());
    asset_create_expected.push(0);
    asset_create_expected.extend([0x22; 32]);
    asset_create_expected.extend(1u128.to_le_bytes());
    assert_eq!(asset_create.encode(), asset_create_expected);

    let mut asset_expected = vec![16, 8];
    asset_expected.extend(7u32.to_le_bytes());
    asset_expected.push(0);
    asset_expected.extend([0x22; 32]);
    asset_expected.extend([0xa1, 0x0f]);
    assert_eq!(asset_transfer.encode(), asset_expected);

    let mut nft_create_expected = vec![17, 0, 0];
    nft_create_expected.extend([0x22; 32]);
    nft_create_expected.extend([0; 21]);
    assert_eq!(nft_create.encode(), nft_create_expected);

    let mut nft_mint_expected = vec![17, 3];
    nft_mint_expected.extend(3u32.to_le_bytes());
    nft_mint_expected.extend(9u32.to_le_bytes());
    nft_mint_expected.push(0);
    nft_mint_expected.extend([0x22; 32]);
    nft_mint_expected.push(0);
    assert_eq!(nft_mint.encode(), nft_mint_expected);

    let mut nft_expected = vec![17, 40];
    nft_expected.extend(3u32.to_le_bytes());
    nft_expected.extend(685u32.to_le_bytes());
    assert_eq!(nft_retirement.encode(), nft_expected);

    let mut world_expected = vec![18, 0, 0x1c];
    world_expected.extend(b"world-1");
    world_expected.extend([0x55; 32]);
    assert_eq!(world_registration.encode(), world_expected);

    let extra: SignedExtra = (
        frame_system::CheckNonZeroSender::new(),
        frame_system::CheckSpecVersion::new(),
        frame_system::CheckTxVersion::new(),
        frame_system::CheckGenesis::new(),
        frame_system::CheckMortality::from(sp_runtime::generic::Era::Immortal),
        frame_system::CheckNonce::from(7),
        frame_system::CheckWeight::new(),
        pallet_transaction_payment::ChargeTransactionPayment::from(11),
    );
    let signed = UncheckedExtrinsic::new_signed(
        asset_transfer.clone(),
        MultiAddress::Id(account(0x11)),
        MultiSignature::Sr25519(sp_core::sr25519::Signature::from_raw([0x44; 64])),
        extra,
    );
    let encoded = signed.encode();
    let mut body = vec![0x84, 0];
    body.extend([0x11; 32]);
    body.push(1);
    body.extend([0x44; 64]);
    body.extend([0, 0x1c, 0x2c]);
    body.extend(asset_expected);
    let mut expected = codec::Compact(body.len() as u32).encode();
    expected.extend(body);
    assert_eq!(encoded, expected);

    let decoded =
        UncheckedExtrinsic::decode(&mut encoded.as_slice()).expect("opaque V14 extrinsic decodes");
    assert_eq!(decoded.function, asset_transfer);
    assert_eq!(decoded.encode(), encoded);
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

fn founding_custody_test_ext(
    category: era_v14_custody_governance::CustodyCategory,
    sudo_key: Option<AccountId>,
) -> sp_io::TestExternalities {
    let source = FounderCustody::custody_account(category);
    issuance_cap_test_ext(
        vec![
            (source, 10 * DECIMALS),
            (
                sudo_key.clone().unwrap_or_else(|| account(249)),
                EXISTENTIAL_DEPOSIT,
            ),
        ],
        sudo_key,
    )
}

#[test]
fn founding_custody_derivation_has_exact_golden_vectors_and_distinct_identities() {
    use era_v14_custody_governance::CustodyCategory::{Ecosystem, Liquidity, Presale};

    let cases = [
        (
            Presale,
            b"modlera/vamm\x02founding\x00".as_slice(),
            AccountId::new([
                0x6d, 0x6f, 0x64, 0x6c, 0x65, 0x72, 0x61, 0x2f, 0x76, 0x61, 0x6d, 0x6d, 0x02, 0x66,
                0x6f, 0x75, 0x6e, 0x64, 0x69, 0x6e, 0x67, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00, 0x00,
            ]),
        ),
        (
            Ecosystem,
            b"modlera/vamm\x02founding\x01".as_slice(),
            AccountId::new([
                0x6d, 0x6f, 0x64, 0x6c, 0x65, 0x72, 0x61, 0x2f, 0x76, 0x61, 0x6d, 0x6d, 0x02, 0x66,
                0x6f, 0x75, 0x6e, 0x64, 0x69, 0x6e, 0x67, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00, 0x00,
            ]),
        ),
        (
            Liquidity,
            b"modlera/vamm\x02founding\x02".as_slice(),
            AccountId::new([
                0x6d, 0x6f, 0x64, 0x6c, 0x65, 0x72, 0x61, 0x2f, 0x76, 0x61, 0x6d, 0x6d, 0x02, 0x66,
                0x6f, 0x75, 0x6e, 0x64, 0x69, 0x6e, 0x67, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00, 0x00,
            ]),
        ),
    ];
    for (category, preimage, account) in cases {
        assert_eq!(FounderCustody::derivation_preimage(category), preimage);
        assert_eq!(FounderCustody::custody_account(category), account);
    }

    let destinations = v13_migration::custody_destinations();
    let sources = v13_migration::test_custody_sources();
    let signers = FounderCustodySigners::get();
    let identities = [
        sources[0].clone(),
        sources[1].clone(),
        sources[2].clone(),
        destinations[0].clone(),
        destinations[1].clone(),
        destinations[2].clone(),
        signers[0].clone(),
        signers[1].clone(),
        signers[2].clone(),
    ];
    for (index, identity) in identities.iter().enumerate() {
        assert_ne!(identity, &AccountId::new([0; 32]));
        assert!(identities[..index]
            .iter()
            .all(|earlier| earlier != identity));
    }
}

#[test]
fn founding_custody_requires_exact_three_of_three_for_every_category() {
    use era_v14_custody_governance::CustodyCategory::{Ecosystem, Liquidity, Presale};
    use era_v14_custody_governance::Error as CustodyError;

    for category in [Presale, Ecosystem, Liquidity] {
        let mut ext = founding_custody_test_ext(category, None);
        ext.execute_with(|| {
            let signers = FounderCustodySigners::get();
            let source = FounderCustody::custody_account(category);
            let destination = account(200);
            let amount = 3 * DECIMALS;
            let source_before = Balances::free_balance(&source);

            assert_noop!(
                FounderCustody::approve_withdrawal(
                    RuntimeOrigin::signed(signers[0].clone()),
                    category,
                    0,
                    destination.clone(),
                    0,
                ),
                CustodyError::<Runtime>::ZeroAmount
            );
            assert_noop!(
                FounderCustody::approve_withdrawal(
                    RuntimeOrigin::signed(account(240)),
                    category,
                    0,
                    destination.clone(),
                    amount,
                ),
                CustodyError::<Runtime>::UnauthorizedSigner
            );
            assert_noop!(
                FounderCustody::approve_withdrawal(
                    RuntimeOrigin::root(),
                    category,
                    0,
                    destination.clone(),
                    amount,
                ),
                DispatchError::BadOrigin
            );

            assert_ok!(FounderCustody::approve_withdrawal(
                RuntimeOrigin::signed(signers[0].clone()),
                category,
                0,
                destination.clone(),
                amount,
            ));
            assert_eq!(Balances::free_balance(&source), source_before);
            assert_eq!(
                FounderCustody::pending_withdrawal(category)
                    .expect("one approval is pending")
                    .approvals,
                0b001
            );

            assert_noop!(
                FounderCustody::approve_withdrawal(
                    RuntimeOrigin::signed(signers[0].clone()),
                    category,
                    0,
                    destination.clone(),
                    amount,
                ),
                CustodyError::<Runtime>::DuplicateApproval
            );
            assert_noop!(
                FounderCustody::approve_withdrawal(
                    RuntimeOrigin::signed(signers[1].clone()),
                    category,
                    0,
                    destination.clone(),
                    amount - 1,
                ),
                CustodyError::<Runtime>::ProposalMismatch
            );
            assert_noop!(
                FounderCustody::approve_withdrawal(
                    RuntimeOrigin::signed(signers[1].clone()),
                    category,
                    0,
                    account(201),
                    amount,
                ),
                CustodyError::<Runtime>::ProposalMismatch
            );
            let other_category = match category {
                Presale => Ecosystem,
                Ecosystem => Liquidity,
                Liquidity => Presale,
            };
            assert_noop!(
                FounderCustody::approve_withdrawal(
                    RuntimeOrigin::signed(signers[1].clone()),
                    category,
                    0,
                    FounderCustody::custody_account(other_category),
                    amount,
                ),
                CustodyError::<Runtime>::InvalidDestination
            );

            assert_ok!(FounderCustody::approve_withdrawal(
                RuntimeOrigin::signed(signers[1].clone()),
                category,
                0,
                destination.clone(),
                amount,
            ));
            assert_eq!(Balances::free_balance(&source), source_before);
            assert_eq!(
                FounderCustody::pending_withdrawal(category)
                    .expect("two approvals are pending")
                    .approvals,
                0b011
            );

            assert_ok!(FounderCustody::approve_withdrawal(
                RuntimeOrigin::signed(signers[2].clone()),
                category,
                0,
                destination.clone(),
                amount,
            ));
            assert_eq!(Balances::free_balance(&source), source_before - amount);
            assert_eq!(Balances::free_balance(&destination), amount);
            assert_eq!(FounderCustody::pending_withdrawal(category), None);
            assert_eq!(FounderCustody::next_request_id(category), 1);

            assert_noop!(
                FounderCustody::approve_withdrawal(
                    RuntimeOrigin::signed(signers[0].clone()),
                    category,
                    0,
                    destination,
                    amount,
                ),
                CustodyError::<Runtime>::RequestIdMismatch
            );
        });
    }
}

#[test]
fn founding_custody_malformed_and_insufficient_paths_roll_back() {
    use era_v14_custody_governance::{
        CustodyCategory, Error as CustodyError, PendingWithdrawal, WithdrawalRequest,
    };

    let category = CustodyCategory::Presale;
    let mut ext = founding_custody_test_ext(category, None);
    ext.execute_with(|| {
        let signers = FounderCustodySigners::get();
        let destination = account(200);
        PendingWithdrawal::<Runtime>::insert(
            category,
            WithdrawalRequest::<Runtime> {
                request_id: 0,
                destination: destination.clone(),
                amount: DECIMALS,
                approvals: 0b1000,
            },
        );
        let before = v13_storage_root();
        assert_noop!(
            FounderCustody::approve_withdrawal(
                RuntimeOrigin::signed(signers[0].clone()),
                category,
                0,
                destination,
                DECIMALS,
            ),
            CustodyError::<Runtime>::MalformedPendingRequest
        );
        assert_eq!(v13_storage_root(), before);
    });

    let mut insufficient = founding_custody_test_ext(category, None);
    insufficient.execute_with(|| {
        let before = v13_storage_root();
        assert_noop!(
            FounderCustody::approve_withdrawal(
                RuntimeOrigin::signed(FounderCustodySigners::get()[0].clone()),
                category,
                0,
                account(200),
                11 * DECIMALS,
            ),
            CustodyError::<Runtime>::InsufficientCustodyBalance
        );
        assert_eq!(v13_storage_root(), before);
    });
}

#[test]
fn founding_custody_discloses_existing_sudo_as_exception() {
    use era_v14_custody_governance::CustodyCategory;

    let category = CustodyCategory::Liquidity;
    let sudo = account(248);
    let mut ext = founding_custody_test_ext(category, Some(sudo.clone()));
    ext.execute_with(|| {
        let signers = FounderCustodySigners::get();
        let destination = account(200);
        let amount = DECIMALS;
        for signer in signers {
            assert_ok!(Sudo::sudo_as(
                RuntimeOrigin::signed(sudo.clone()),
                MultiAddress::Id(signer),
                Box::new(RuntimeCall::FounderCustody(
                    era_v14_custody_governance::Call::approve_withdrawal {
                        category,
                        request_id: 0,
                        destination: destination.clone(),
                        amount,
                    },
                )),
            ));
        }
        assert_eq!(Balances::free_balance(&destination), amount);
        assert_eq!(FounderCustody::next_request_id(category), 1);
    });
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
        // Historical V13 fixture predates the authorized NFT v2 schema.
        StorageVersion::new(1).put::<Nfts>();
        assert_eq!(StorageVersion::get::<IssuanceCap>(), StorageVersion::new(1));
        assert_eq!(IssuanceCap::remaining_allowance(), None);
        assert_eq!(issuance_cap::V13MigrationCompleted::<Runtime>::get(), None);
        System::reset_events();
    });
    ext
}

fn v13_legacy_completed_test_ext() -> sp_io::TestExternalities {
    use frame_support::traits::{fungible::Mutate, tokens::Preservation};

    let mut ext = v13_migration_test_ext();
    ext.execute_with(|| {
        System::set_block_number(v13_migration::test_legacy_completed_at());
        let mut input = v13_migration::test_input();
        input.community_onboarding_destination = v13_migration::test_legacy_community_destination();
        frame_support::storage::unhashed::put(&issuance_cap::v13_migration_input_key(), &input);
        assert_eq!(
            <v13_migration::V13Migration as OnRuntimeUpgrade>::on_runtime_upgrade(),
            v13_migration::declared_weight()
        );

        let current = v13_migration::custody_destinations();
        let legacy = v13_migration::test_legacy_destinations();
        for ((source, destination), amount) in current
            .iter()
            .zip(legacy.iter())
            .zip(v13_migration::test_transfer_amounts())
        {
            assert_eq!(
                <Balances as Mutate<AccountId>>::transfer(
                    source,
                    destination,
                    amount,
                    Preservation::Expendable,
                ),
                Ok(amount)
            );
        }

        let sudo = v13_migration::test_sudo_account();
        pallet_sudo::Key::<Runtime>::put(&sudo);
        // The production set_code extrinsic consumes sealed nonce 22 before this migration runs.
        frame_system::Account::<Runtime>::mutate(&sudo, |info| info.nonce = 23);
        sp_io::storage::set(
            &issuance_cap::V13MigrationCompleted::<Runtime>::hashed_key(),
            &v13_migration::test_legacy_marker_bytes(),
        );
        assert_eq!(Balances::total_issuance(), CAP_TEST_BASELINE);
        assert_eq!(IssuanceCap::remaining_allowance(), Some(CAP_TEST_ALLOWANCE));
        System::reset_events();
    });
    ext
}

fn balance_account_storage(who: &AccountId) -> [Option<Vec<u8>>; 5] {
    [
        frame_system::Account::<Runtime>::hashed_key_for(who),
        pallet_balances::Locks::<Runtime>::hashed_key_for(who),
        pallet_balances::Reserves::<Runtime>::hashed_key_for(who),
        pallet_balances::Holds::<Runtime>::hashed_key_for(who),
        pallet_balances::Freezes::<Runtime>::hashed_key_for(who),
    ]
    .map(|key| sp_io::storage::get(&key).map(|value| value.to_vec()))
}

fn assert_v13_legacy_upgrade_fails_closed() {
    let root = v13_storage_root();
    let events = System::events();
    let marker = sp_io::storage::get(&issuance_cap::V13MigrationCompleted::<Runtime>::hashed_key());
    assert_v13_migration_panics();
    assert_eq!(v13_storage_root(), root);
    assert_eq!(System::events(), events);
    assert_eq!(
        sp_io::storage::get(&issuance_cap::V13MigrationCompleted::<Runtime>::hashed_key()),
        marker
    );
}

fn v13_storage_root() -> Vec<u8> {
    sp_io::storage::root(StateVersion::V1)
}

fn assert_v13_migration_panics() {
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        <v13_migration::V13Migration as OnRuntimeUpgrade>::on_runtime_upgrade()
    }))
    .is_err());
}

fn v14_migration_test_ext(validator_bond: Balance) -> (sp_io::TestExternalities, Fixture) {
    let (mut ext, fixture) = new_test_ext_with_validator_bond(4, 2, 4, 4, validator_bond);
    ext.execute_with(|| {
        System::set_block_number(100);
        StorageVersion::new(1).put::<IssuanceCap>();
        StorageVersion::new(2).put::<RewardReserve>();
        StorageVersion::new(0).put::<SecurityBudget>();
        StorageVersion::new(16).put::<Staking>();
        issuance_cap::RemainingAllowance::<Runtime>::put(CAP_TEST_ALLOWANCE);
        issuance_cap::V13MigrationCompleted::<Runtime>::put(issuance_cap::V13Completion {
            migration_version: 13,
            completed_at: 99,
            input: v13_migration::test_input(),
        });

        let active_era = 10;
        pallet_staking::CurrentEra::<Runtime>::put(active_era);
        pallet_staking::ActiveEra::<Runtime>::put(pallet_staking::ActiveEraInfo {
            index: active_era,
            start: Some(1_000_000),
        });
        for validator in &fixture.validators {
            let own = pallet_staking::Ledger::<Runtime>::get(validator)
                .expect("validator ledger")
                .active;
            pallet_staking::ErasStakersOverview::<Runtime>::insert(
                active_era,
                validator,
                PagedExposureMetadata {
                    total: own,
                    own,
                    nominator_count: 0,
                    page_count: 0,
                },
            );
            pallet_staking::ErasValidatorPrefs::<Runtime>::insert(
                active_era,
                validator,
                ValidatorPrefs {
                    commission: Perbill::zero(),
                    blocked: false,
                },
            );
        }

        <Babe as frame_support::traits::OneSessionHandler<AccountId>>::on_genesis_session(
            fixture
                .session_keys
                .iter()
                .map(|(validator, session_keys)| (validator, session_keys.babe.clone())),
        );
        <Grandpa as frame_support::traits::OneSessionHandler<AccountId>>::on_genesis_session(
            fixture
                .session_keys
                .iter()
                .map(|(validator, session_keys)| (validator, session_keys.grandpa.clone())),
        );

        for (destination, amount) in [
            (RewardReserve::reward_pot_account(), 20_000_000 * DECIMALS),
            (
                RewardReserve::ecosystem_treasury_account(),
                TreasuryPotSafetyFloor::get(),
            ),
            (RewardReserve::fee_collection_account(), EXISTENTIAL_DEPOSIT),
        ] {
            assert_ok!(<Balances as Currency<AccountId>>::transfer(
                &account(250),
                &destination,
                amount,
                ExistenceRequirement::AllowDeath,
            ));
        }
        pallet_reward_reserve::RewardSystemActive::<Runtime>::put(true);
        pallet_reward_reserve::FeeRoutingActive::<Runtime>::put(true);
        let legacy_liability = SecurityBudgetExistingEarnedRewardLiability::get();
        pallet_reward_reserve::EraRewardBudgets::<Runtime>::insert(5, legacy_liability + 77);
        pallet_reward_reserve::EraRewardPaid::<Runtime>::insert(5, 77);
        pallet_reward_reserve::EraRewardLiabilities::<Runtime>::insert(5, legacy_liability);
        pallet_reward_reserve::CommittedLiabilities::<Runtime>::put(legacy_liability);
        pallet_reward_reserve::ClaimedRewardPages::<Runtime>::insert(
            5,
            (fixture.validators[0].clone(), 0),
            (),
        );
        System::reset_events();
    });
    (ext, fixture)
}

fn assert_v14_migration_panics() {
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        <v14_migration::V14Migration as OnRuntimeUpgrade>::on_runtime_upgrade()
    }))
    .is_err());
}

fn install_v14_equal_points(era: sp_staking::EraIndex, validators: &[AccountId]) {
    let mut individual = BTreeMap::new();
    for validator in validators {
        let own = pallet_staking::Ledger::<Runtime>::get(validator)
            .expect("validator ledger")
            .active;
        individual.insert(validator.clone(), 10);
        pallet_staking::ErasStakersOverview::<Runtime>::insert(
            era,
            validator,
            PagedExposureMetadata {
                total: own,
                own,
                nominator_count: 0,
                page_count: 0,
            },
        );
        pallet_staking::ErasValidatorPrefs::<Runtime>::insert(
            era,
            validator,
            ValidatorPrefs {
                commission: Perbill::zero(),
                blocked: false,
            },
        );
    }
    pallet_staking::ErasRewardPoints::<Runtime>::insert(
        era,
        EraRewardPoints {
            total: 10 * validators.len() as u32,
            individual,
        },
    );
}

fn install_v14_exposure(
    era: sp_staking::EraIndex,
    validator: &AccountId,
    own: Balance,
    nominators: &[(AccountId, Balance)],
    commission: Perbill,
) {
    let nominator_total = nominators.iter().map(|(_, value)| *value).sum::<Balance>();
    pallet_staking::ErasStakersOverview::<Runtime>::insert(
        era,
        validator,
        PagedExposureMetadata {
            total: own + nominator_total,
            own,
            nominator_count: nominators.len() as u32,
            page_count: if nominators.is_empty() { 0 } else { 1 },
        },
    );
    if !nominators.is_empty() {
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
    }
    pallet_staking::ErasValidatorPrefs::<Runtime>::insert(
        era,
        validator,
        ValidatorPrefs {
            commission,
            blocked: false,
        },
    );
}

#[test]
fn v14_migration_reconciles_legacy_liability_preserving_supply_and_is_idempotent() {
    let (mut ext, fixture) = v14_migration_test_ext(10_000 * DECIMALS);
    ext.execute_with(|| {
        let issuance = Balances::total_issuance();
        let allowance = IssuanceCap::remaining_allowance();
        let legacy_pot = RewardReserve::pot_balance();
        let legacy_treasury = RewardReserve::ecosystem_treasury_balance();
        let collection = RewardReserve::fee_collection_balance();
        let active_era = pallet_staking::ActiveEra::<Runtime>::get();
        let current_era = pallet_staking::CurrentEra::<Runtime>::get();
        let session_validators = Session::validators();
        let queued_keys = pallet_session::QueuedKeys::<Runtime>::get();
        let babe = pallet_babe::Authorities::<Runtime>::get();
        let grandpa = pallet_grandpa::Authorities::<Runtime>::get();
        let ledgers = fixture
            .validators
            .iter()
            .map(pallet_staking::Ledger::<Runtime>::get)
            .collect::<Vec<_>>();
        let nominations = fixture
            .nominators
            .iter()
            .map(pallet_staking::Nominators::<Runtime>::get)
            .collect::<Vec<_>>();
        let reward_budget = RewardReserve::era_reward_budget(5);
        let reward_paid = RewardReserve::era_reward_paid(5);
        let reward_liability = RewardReserve::era_reward_liability(5);
        let claimed = pallet_reward_reserve::ClaimedRewardPages::<Runtime>::contains_key(
            5,
            (fixture.validators[0].clone(), 0),
        );

        let weight = <v14_migration::V14Migration as OnRuntimeUpgrade>::on_runtime_upgrade();
        assert_eq!(weight, v14_migration::declared_weight());
        assert_eq!(
            StorageVersion::get::<RewardReserve>(),
            StorageVersion::new(3)
        );
        assert_eq!(
            StorageVersion::get::<SecurityBudget>(),
            StorageVersion::new(1)
        );
        assert_eq!(StorageVersion::get::<Staking>(), StorageVersion::new(16));
        assert_eq!(
            pallet_staking::MinValidatorBond::<Runtime>::get(),
            10_000 * DECIMALS
        );
        assert_eq!(SecurityBudget::migration_completed_at(), Some(100));
        assert!(!SecurityBudget::active());
        assert!(pallet_security_budget::AllocationPaused::<Runtime>::get());
        assert_eq!(
            System::account(SecurityBudget::staking_pot_account()).providers,
            1
        );
        assert_eq!(SecurityBudget::staking_pot_balance(), 0);
        assert_eq!(
            SecurityBudget::legacy_reward_liability(),
            SecurityBudgetExistingEarnedRewardLiability::get()
        );
        assert_eq!(
            SecurityBudget::accounted_reward_total(),
            Ok(SecurityBudgetExistingEarnedRewardLiability::get())
        );
        assert_eq!(
            SecurityBudget::remaining_reward_budget(),
            Ok(19_952_086_627_377_701_116_381_613)
        );
        assert!(RewardReserve::legacy_claim_only());
        assert!(!RewardReserve::fee_routing_active());

        assert_eq!(Balances::total_issuance(), issuance);
        assert_eq!(IssuanceCap::remaining_allowance(), allowance);
        assert_eq!(RewardReserve::pot_balance(), legacy_pot);
        assert_eq!(RewardReserve::ecosystem_treasury_balance(), legacy_treasury);
        assert_eq!(RewardReserve::fee_collection_balance(), collection);
        assert_eq!(RewardReserve::era_reward_budget(5), reward_budget);
        assert_eq!(RewardReserve::era_reward_paid(5), reward_paid);
        assert_eq!(RewardReserve::era_reward_liability(5), reward_liability);
        assert_eq!(
            RewardReserve::committed_liabilities(),
            SecurityBudgetExistingEarnedRewardLiability::get()
        );
        assert_eq!(
            pallet_reward_reserve::ClaimedRewardPages::<Runtime>::contains_key(
                5,
                (fixture.validators[0].clone(), 0),
            ),
            claimed
        );
        assert_eq!(pallet_staking::ActiveEra::<Runtime>::get(), active_era);
        assert_eq!(pallet_staking::CurrentEra::<Runtime>::get(), current_era);
        assert_eq!(Session::validators(), session_validators);
        assert_eq!(pallet_session::QueuedKeys::<Runtime>::get(), queued_keys);
        assert_eq!(pallet_babe::Authorities::<Runtime>::get(), babe);
        assert_eq!(pallet_grandpa::Authorities::<Runtime>::get(), grandpa);
        assert_eq!(
            fixture
                .validators
                .iter()
                .map(pallet_staking::Ledger::<Runtime>::get)
                .collect::<Vec<_>>(),
            ledgers
        );
        assert_eq!(
            fixture
                .nominators
                .iter()
                .map(pallet_staking::Nominators::<Runtime>::get)
                .collect::<Vec<_>>(),
            nominations
        );

        System::reset_events();
        let root = sp_io::storage::root(StateVersion::V1);
        let replay = <v14_migration::V14Migration as OnRuntimeUpgrade>::on_runtime_upgrade();
        assert_eq!(
            replay,
            <Runtime as frame_system::Config>::DbWeight::get()
                .reads(32)
                .saturating_add(crate::ws3_vesting::declared_weight())
        );
        assert_eq!(sp_io::storage::root(StateVersion::V1), root);
        assert!(System::events().is_empty());
    });
}

#[test]
fn v14_migration_accepts_the_bond_boundary_and_rolls_back_invalid_inputs() {
    for bond in [10_000 * DECIMALS, 10_001 * DECIMALS] {
        let (mut ext, _) = v14_migration_test_ext(bond);
        ext.execute_with(|| {
            <v14_migration::V14Migration as OnRuntimeUpgrade>::on_runtime_upgrade();
            assert_eq!(
                StorageVersion::get::<SecurityBudget>(),
                StorageVersion::new(1)
            );
        });
    }

    let (mut below, _) = v14_migration_test_ext(10_000 * DECIMALS - 1);
    below.execute_with(|| {
        let root = sp_io::storage::root(StateVersion::V1);
        assert_v14_migration_panics();
        assert_eq!(sp_io::storage::root(StateVersion::V1), root);
        assert_eq!(
            StorageVersion::get::<SecurityBudget>(),
            StorageVersion::new(0)
        );
    });

    let (mut wrong_liability, _) = v14_migration_test_ext(10_000 * DECIMALS);
    wrong_liability.execute_with(|| {
        pallet_reward_reserve::CommittedLiabilities::<Runtime>::put(
            SecurityBudgetExistingEarnedRewardLiability::get() - 1,
        );
        let root = sp_io::storage::root(StateVersion::V1);
        assert_v14_migration_panics();
        assert_eq!(sp_io::storage::root(StateVersion::V1), root);
        assert_eq!(
            StorageVersion::get::<SecurityBudget>(),
            StorageVersion::new(0)
        );
        assert!(!RewardReserve::legacy_claim_only());
    });

    let (mut wrong_version, _) = v14_migration_test_ext(10_000 * DECIMALS);
    wrong_version.execute_with(|| {
        StorageVersion::new(1).put::<SecurityBudget>();
        let root = sp_io::storage::root(StateVersion::V1);
        assert_v14_migration_panics();
        assert_eq!(sp_io::storage::root(StateVersion::V1), root);
    });

    let (mut missing_floor, _) = v14_migration_test_ext(10_000 * DECIMALS);
    missing_floor.execute_with(|| {
        let collection = RewardReserve::fee_collection_account();
        assert_ok!(<Balances as Currency<AccountId>>::transfer(
            &collection,
            &account(250),
            EXISTENTIAL_DEPOSIT,
            ExistenceRequirement::AllowDeath,
        ));
        let root = sp_io::storage::root(StateVersion::V1);
        assert_v14_migration_panics();
        assert_eq!(sp_io::storage::root(StateVersion::V1), root);
    });
}

#[test]
fn v14_activation_is_monetarily_neutral_and_irreversibly_retires_legacy_allocation() {
    let (mut ext, fixture) = v14_migration_test_ext(10_000 * DECIMALS);
    ext.execute_with(|| {
        <v14_migration::V14Migration as OnRuntimeUpgrade>::on_runtime_upgrade();
        let issuance = Balances::total_issuance();
        let allowance = IssuanceCap::remaining_allowance();
        let legacy_pot = RewardReserve::pot_balance();
        let legacy_liability = RewardReserve::committed_liabilities();
        let treasury = RewardReserve::ecosystem_treasury_balance();
        assert_ok!(SecurityBudget::activate(RuntimeOrigin::root()));
        assert!(SecurityBudget::active());
        assert_eq!(SecurityBudget::legacy_cutoff_era(), Some(10));
        assert_eq!(SecurityBudget::first_eligible_era(), Some(11));
        assert!(RewardReserve::legacy_claim_only());
        assert_eq!(RewardReserve::v14_legacy_cutoff_era(), Some(10));
        assert!(!RewardReserve::fee_routing_active());
        assert_eq!(Balances::total_issuance(), issuance);
        assert_eq!(IssuanceCap::remaining_allowance(), allowance);
        assert_eq!(RewardReserve::pot_balance(), legacy_pot);
        assert_eq!(RewardReserve::committed_liabilities(), legacy_liability);
        assert_eq!(RewardReserve::ecosystem_treasury_balance(), treasury);
        assert_noop!(
            RewardReserve::finalize_completed_era(11, 1, 2),
            pallet_reward_reserve::Error::<Runtime>::LegacyClaimOnly
        );
        assert_noop!(
            RewardReserve::spend_ecosystem_treasury(
                RuntimeOrigin::root(),
                fixture.validators[0].clone(),
                1,
            ),
            DispatchError::BadOrigin
        );
        assert_noop!(
            SecurityBudget::activate(RuntimeOrigin::root()),
            pallet_security_budget::Error::<Runtime>::AlreadyActive
        );
    });
}

#[test]
fn v14_issuance_math_covers_policy_crossovers_carries_and_maximum_stake() {
    let (mut ext, _) = v14_migration_test_ext(10_000 * DECIMALS);
    ext.execute_with(|| {
        let year = SecurityBudgetMillisecondsPerYear::get();
        for (stake, expected_staking, expected_gross) in [
            (0, 0, 0),
            (1, 0, 0),
            (
                27_000_000 * DECIMALS,
                2_700_000 * DECIMALS,
                3_000_000 * DECIMALS,
            ),
            (
                45_000_000 * DECIMALS,
                4_500_000 * DECIMALS,
                5_000_000 * DECIMALS,
            ),
            (
                54_000_000 * DECIMALS,
                5_400_000 * DECIMALS,
                6_000_000 * DECIMALS,
            ),
        ] {
            let (staking, _) =
                SecurityBudget::target_staking_issuance(stake, year, 0).expect("bounded target");
            assert_eq!(staking, expected_staking);
            assert_eq!(
                SecurityBudget::gross_for_staking_target(staking, 0).expect("bounded gross"),
                expected_gross
            );
        }

        let (above, _) =
            SecurityBudget::target_staking_issuance(54_000_000 * DECIMALS + 10, year, 0)
                .expect("above crossover");
        assert!(SecurityBudget::gross_for_staking_target(above, 0).unwrap() > 6_000_000 * DECIMALS);
        let (maximum, _) = SecurityBudget::target_staking_issuance(Balance::MAX, year, 0)
            .expect("U256 protects maximum eligible stake");
        assert!(maximum > 5_400_000 * DECIMALS);

        let stake = 10_000_000 * DECIMALS;
        let base_duration = year / 365;
        let mut target_remainder = 0;
        let mut split_carry = 0;
        let mut staking_total = 0;
        let mut gross_total = 0;
        for day in 0..365 {
            let duration = if day == 364 {
                year - base_duration * 364
            } else {
                base_duration
            };
            let (target, next_remainder) =
                SecurityBudget::target_staking_issuance(stake, duration, target_remainder)
                    .expect("daily target");
            let gross =
                SecurityBudget::gross_for_staking_target(target, split_carry).expect("daily gross");
            let (staking, treasury, next_carry) =
                SecurityBudget::issuance_split(gross, split_carry).expect("daily split");
            assert_eq!(staking, target);
            assert_eq!(staking + treasury, gross);
            staking_total += staking;
            gross_total += gross;
            target_remainder = next_remainder;
            split_carry = next_carry;
        }
        assert_eq!(staking_total, 1_000_000 * DECIMALS);
        assert_eq!(gross_total, 1_111_111_111_111_111_111_111_112);
        assert!(split_carry < 10);
    });
}

#[test]
fn v14_multi_era_rewards_are_nonzero_then_stop_at_the_exact_20m_cap() {
    let (mut ext, fixture) = v14_migration_test_ext(10_000 * DECIMALS);
    ext.execute_with(|| {
        let community = account(240);
        assert_ok!(<Balances as Currency<AccountId>>::transfer(
            &account(250),
            &community,
            SecurityBudgetProtectedCommunityOnboarding::get(),
            ExistenceRequirement::AllowDeath,
        ));
        let community_before = Balances::free_balance(&community);
        let legacy_pot_before = RewardReserve::pot_balance();

        <v14_migration::V14Migration as OnRuntimeUpgrade>::on_runtime_upgrade();
        assert_ok!(SecurityBudget::activate(RuntimeOrigin::root()));
        let first = SecurityBudget::first_eligible_era().expect("first eligible era");
        let year = SecurityBudgetMillisecondsPerYear::get();
        let capacity = SecurityBudgetTotalStakingRewards::get()
            - SecurityBudgetExistingEarnedRewardLiability::get();
        assert_eq!(capacity, 19_952_086_627_377_701_116_381_613);
        assert_noop!(
            SecurityBudget::finalize_completed_era(first - 1, year, 1_000_000 + year),
            pallet_security_budget::Error::<Runtime>::InvalidEra
        );

        let issuance_before = Balances::total_issuance();
        let allowance_before = IssuanceCap::remaining_allowance().expect("shared allowance");
        let mut new_rewards = 0u128;
        let mut gross_issuance = 0u128;
        let mut final_payable_era = first;
        for offset in 0..4u32 {
            let era = first + offset;
            let individual = fixture
                .validators
                .iter()
                .cloned()
                .map(|validator| (validator, 100))
                .collect::<BTreeMap<_, _>>();
            pallet_staking::ErasRewardPoints::<Runtime>::insert(
                era,
                EraRewardPoints {
                    total: 400,
                    individual,
                },
            );
            for (index, validator) in fixture.validators.iter().enumerate() {
                install_v14_exposure(
                    era,
                    validator,
                    10_000 * DECIMALS,
                    &[(account(120 + index as u8), 13_490_000 * DECIMALS)],
                    Perbill::zero(),
                );
            }
            assert_ok!(SecurityBudget::finalize_completed_era(
                era,
                year,
                1_000_000 + (offset as u64 + 1) * year,
            ));
            let budget = SecurityBudget::era_budget(era).expect("payable era budget");
            assert!(budget.total > 0);
            assert_eq!(budget.total, budget.staking_issuance);
            new_rewards += budget.total;
            gross_issuance += budget.gross_issuance;
            final_payable_era = era;
        }

        assert_eq!(
            SecurityBudget::era_budget(first).unwrap().total,
            5_400_000 * DECIMALS
        );
        assert_eq!(
            SecurityBudget::era_budget(first + 1).unwrap().total,
            5_400_000 * DECIMALS
        );
        assert_eq!(
            SecurityBudget::era_budget(first + 2).unwrap().total,
            5_400_000 * DECIMALS
        );
        assert_eq!(
            SecurityBudget::era_budget(final_payable_era).unwrap().total,
            capacity - 16_200_000 * DECIMALS
        );
        assert_eq!(new_rewards, capacity);
        assert_eq!(
            SecurityBudgetExistingEarnedRewardLiability::get() + new_rewards,
            SecurityBudgetTotalStakingRewards::get()
        );
        assert_eq!(
            SecurityBudget::accounted_reward_total(),
            Ok(SecurityBudgetTotalStakingRewards::get())
        );
        assert_eq!(SecurityBudget::remaining_reward_budget(), Ok(0));
        assert_eq!(Balances::total_issuance() - issuance_before, gross_issuance);
        let allowance_after = IssuanceCap::remaining_allowance().expect("shared allowance");
        assert_eq!(allowance_before - allowance_after, gross_issuance);
        assert_eq!(
            Balances::total_issuance() + allowance_after,
            issuance_before + allowance_before
        );
        assert!(Balances::total_issuance() <= crate::upgrade13_policy::ABSOLUTE_LIFETIME_CAP);
        assert_eq!(Balances::free_balance(&community), community_before);
        assert_eq!(
            community_before,
            SecurityBudgetProtectedCommunityOnboarding::get()
        );
        assert_eq!(SecurityBudgetRetiredLegacyRewardReserveTarget::get(), 0);
        assert_eq!(RewardReserve::pot_balance(), legacy_pot_before);
        assert!(RewardReserve::legacy_claim_only());
        assert_eq!(Session::validators().len(), 4);
        assert_eq!(Staking::validator_count(), 4);

        let exhausted_era = final_payable_era + 1;
        install_v14_equal_points(exhausted_era, &fixture.validators);
        let issuance_at_exhaustion = Balances::total_issuance();
        let allowance_at_exhaustion = IssuanceCap::remaining_allowance();
        System::reset_events();
        assert_ok!(SecurityBudget::finalize_completed_era(
            exhausted_era,
            year,
            1_000_000 + 5 * year,
        ));
        assert!(SecurityBudget::era_budget(exhausted_era).is_none());
        assert_eq!(Balances::total_issuance(), issuance_at_exhaustion);
        assert_eq!(IssuanceCap::remaining_allowance(), allowance_at_exhaustion);
        assert_eq!(SecurityBudget::remaining_reward_budget(), Ok(0));
        assert!(System::events().iter().any(|record| matches!(
            record.event,
            RuntimeEvent::SecurityBudget(pallet_security_budget::Event::EraSkipped {
                era,
                reason: pallet_security_budget::EraSkipReason::LifetimeRewardBudgetExhausted,
            }) if era == exhausted_era
        )));

        System::reset_events();
        let exhausted_root = sp_io::storage::root(StateVersion::V1);
        let replay = <v14_migration::V14Migration as OnRuntimeUpgrade>::on_runtime_upgrade();
        assert_eq!(
            replay,
            <Runtime as frame_system::Config>::DbWeight::get()
                .reads(32)
                .saturating_add(crate::ws3_vesting::declared_weight())
        );
        assert_eq!(sp_io::storage::root(StateVersion::V1), exhausted_root);
        assert!(System::events().is_empty());
    });
}

#[test]
fn v14_expiry_releases_unpaid_liability_for_single_counted_reuse() {
    let (mut ext, fixture) = v14_migration_test_ext(10_000 * DECIMALS);
    ext.execute_with(|| {
        <v14_migration::V14Migration as OnRuntimeUpgrade>::on_runtime_upgrade();
        assert_ok!(SecurityBudget::activate(RuntimeOrigin::root()));
        let era = SecurityBudget::first_eligible_era().unwrap();
        let year = SecurityBudgetMillisecondsPerYear::get();
        install_v14_equal_points(era, &fixture.validators);
        assert_ok!(SecurityBudget::finalize_completed_era(
            era,
            year,
            1_000_000 + year,
        ));
        let first_budget = SecurityBudget::era_budget(era).unwrap().total;
        assert!(first_budget > 0);
        assert_eq!(
            SecurityBudget::accounted_reward_total(),
            Ok(SecurityBudgetExistingEarnedRewardLiability::get() + first_budget)
        );

        let expiry_active = era + HistoryDepth::get() + 1;
        pallet_staking::ActiveEra::<Runtime>::put(pallet_staking::ActiveEraInfo {
            index: expiry_active,
            start: Some(1_000_000 + year),
        });
        pallet_security_budget::LastObservedEra::<Runtime>::put(expiry_active);
        pallet_security_budget::LastObservedEraStart::<Runtime>::put(1_000_000 + year);
        <SecurityBudget as Hooks<BlockNumber>>::on_initialize(101);
        assert!(SecurityBudget::expired_era(era).is_some());
        assert_eq!(SecurityBudget::era_liability(era), 0);
        assert_eq!(SecurityBudget::committed_liabilities(), 0);
        assert_eq!(SecurityBudget::unallocated_staking_carry(), first_budget);
        assert_eq!(
            SecurityBudget::accounted_reward_total(),
            Ok(SecurityBudgetExistingEarnedRewardLiability::get())
        );
        assert_eq!(
            SecurityBudget::remaining_reward_budget(),
            Ok(SecurityBudgetTotalStakingRewards::get()
                - SecurityBudgetExistingEarnedRewardLiability::get())
        );
        assert_noop!(
            SecurityBudget::claim_reward_page(
                RuntimeOrigin::signed(account(230)),
                era,
                fixture.validators[0].clone(),
                0,
            ),
            pallet_security_budget::Error::<Runtime>::ClaimExpired
        );

        issuance_cap::RemainingAllowance::<Runtime>::put(0);
        let reused_era = expiry_active + 1;
        install_v14_equal_points(reused_era, &fixture.validators);
        pallet_staking::ActiveEra::<Runtime>::put(pallet_staking::ActiveEraInfo {
            index: reused_era + 1,
            start: Some(1_000_000 + 2 * year),
        });
        let issuance = Balances::total_issuance();
        assert_ok!(SecurityBudget::finalize_completed_era(
            reused_era,
            year,
            1_000_000 + 2 * year,
        ));
        let reused = SecurityBudget::era_budget(reused_era).unwrap();
        assert_eq!(reused.gross_issuance, 0);
        assert_eq!(reused.staking_issuance, 0);
        assert_eq!(reused.retained_carry, first_budget);
        assert_eq!(reused.total, first_budget);
        assert_eq!(Balances::total_issuance(), issuance);
        assert_eq!(SecurityBudget::unallocated_staking_carry(), 0);
        assert_eq!(
            SecurityBudget::accounted_reward_total(),
            Ok(SecurityBudgetExistingEarnedRewardLiability::get() + first_budget)
        );
    });
}

#[test]
fn v14_fee_and_tip_routes_conserve_corrected_value_without_issuance() {
    let (mut ext, fixture) = v14_migration_test_ext(10_000 * DECIMALS);
    ext.execute_with(|| {
        <v14_migration::V14Migration as OnRuntimeUpgrade>::on_runtime_upgrade();
        assert_ok!(SecurityBudget::activate(RuntimeOrigin::root()));
        install_v14_equal_points(11, &fixture.validators);
        let year = SecurityBudgetMillisecondsPerYear::get();
        assert_ok!(SecurityBudget::finalize_completed_era(
            11,
            year,
            1_000_000 + year,
        ));
        let payer = fixture.validators[0].clone();
        let author = fixture.validators[1].clone();
        let staking = SecurityBudget::staking_pot_account();
        let treasury = SecurityBudget::treasury_account();
        let issuance = Balances::total_issuance();
        let allowance = IssuanceCap::remaining_allowance();
        let payer_before = Balances::free_balance(&payer);
        let author_before = Balances::free_balance(&author);
        let staking_before = Balances::free_balance(&staking);
        let treasury_before = Balances::free_balance(&treasury);

        route_synthetic_fee(&payer, 101, 101, Some(author.clone()));
        route_synthetic_fee(&payer, 1, 101, None);
        route_synthetic_fee(&payer, 1, 101, Some(staking.clone()));
        route_synthetic_fee(&payer, 1, 1, Some(author.clone()));
        route_synthetic_fee(&payer, 1, 1, None);

        assert_eq!(payer_before - Balances::free_balance(&payer), 410);
        assert_eq!(Balances::free_balance(&author) - author_before, 92);
        assert_eq!(Balances::free_balance(&staking) - staking_before, 198);
        assert_eq!(Balances::free_balance(&treasury) - treasury_before, 120);
        assert_eq!(SecurityBudget::normal_fee_routed_total(), 105);
        assert_eq!(SecurityBudget::author_tip_paid_total(), 92);
        assert_eq!(SecurityBudget::failed_author_tip_total(), 203);
        assert_eq!(SecurityBudget::treasury_normal_fee_total(), 10);
        assert_eq!(SecurityBudget::treasury_tip_total(), 110);
        assert_eq!(SecurityBudget::pending_collection_obligations(), Some(0));
        assert_eq!(RewardReserve::fee_collection_balance(), EXISTENTIAL_DEPOSIT);
        assert_eq!(Balances::total_issuance(), issuance);
        assert_eq!(IssuanceCap::remaining_allowance(), allowance);
    });
}

#[test]
fn controlled_split_issuance_requires_exact_carry_evidence_and_is_atomic() {
    let first = account(1);
    let second = account(2);
    let mut ext = issuance_cap_test_ext(
        vec![
            (first.clone(), CAP_TEST_BASELINE - EXISTENTIAL_DEPOSIT),
            (second.clone(), EXISTENTIAL_DEPOSIT),
        ],
        None,
    );
    ext.execute_with(|| {
        initialize_issuance_cap();
        let issuance = Balances::total_issuance();
        let allowance = IssuanceCap::remaining_allowance();
        assert_noop!(
            IssuanceCap::controlled_mint_split(&first, 1, &second, 0, 0, 0),
            issuance_cap::Error::<Runtime>::InvalidSplitEvidence
        );
        assert_eq!(Balances::total_issuance(), issuance);
        assert_eq!(IssuanceCap::remaining_allowance(), allowance);

        let mut carry = 0;
        let mut staking = 0;
        let mut treasury = 0;
        for _ in 0..10 {
            let (staking_share, treasury_share, next) =
                SecurityBudget::issuance_split(1, carry).unwrap();
            assert_eq!(
                IssuanceCap::controlled_mint_split(
                    &first,
                    staking_share,
                    &second,
                    treasury_share,
                    carry,
                    next,
                ),
                Ok(1)
            );
            staking += staking_share;
            treasury += treasury_share;
            carry = next;
        }
        assert_eq!((staking, treasury, carry), (9, 1, 0));
        assert_eq!(Balances::total_issuance(), issuance + 10);
        assert_eq!(
            IssuanceCap::remaining_allowance(),
            allowance.map(|value| value - 10)
        );
    });
}

#[test]
fn v14_prefunding_small_values_remain_retryable_and_settle_after_first_issuance() {
    let (mut ext, fixture) = v14_migration_test_ext(10_000 * DECIMALS);
    ext.execute_with(|| {
        <v14_migration::V14Migration as OnRuntimeUpgrade>::on_runtime_upgrade();
        assert_ok!(SecurityBudget::activate(RuntimeOrigin::root()));
        let payer = fixture.validators[0].clone();
        let staking = SecurityBudget::staking_pot_account();
        let issuance = Balances::total_issuance();
        let allowance = IssuanceCap::remaining_allowance();
        route_synthetic_fee(&payer, 1, 1, None);
        assert_eq!(SecurityBudget::pending_staking_fee(), 1);
        assert_eq!(SecurityBudget::pending_missing_author_tips(), 1);
        assert_eq!(SecurityBudget::pending_tip_total(), 1);
        assert_eq!(SecurityBudget::pending_collection_obligations(), Some(2));
        assert_eq!(
            RewardReserve::fee_collection_balance(),
            EXISTENTIAL_DEPOSIT + 2
        );
        assert_eq!(Balances::free_balance(&staking), 0);
        assert_eq!(Balances::total_issuance(), issuance);
        assert_eq!(IssuanceCap::remaining_allowance(), allowance);

        install_v14_equal_points(11, &fixture.validators);
        let year = SecurityBudgetMillisecondsPerYear::get();
        assert_ok!(SecurityBudget::finalize_completed_era(
            11,
            year,
            1_000_000 + year,
        ));
        let issuance_after_funding = Balances::total_issuance();
        let allowance_after_funding = IssuanceCap::remaining_allowance();
        let staking_after_funding = Balances::free_balance(&staking);
        assert_ok!(SecurityBudget::retry_normal_fee(RuntimeOrigin::signed(
            payer.clone()
        )));
        assert_ok!(SecurityBudget::retry_tip(
            RuntimeOrigin::signed(payer),
            None,
        ));
        assert_eq!(Balances::free_balance(&staking), staking_after_funding + 2);
        assert_eq!(SecurityBudget::unallocated_fee_staking(), 1);
        assert_eq!(SecurityBudget::unallocated_tip_fallback(), 1);
        assert_eq!(SecurityBudget::pending_collection_obligations(), Some(0));
        assert_eq!(RewardReserve::fee_collection_balance(), EXISTENTIAL_DEPOSIT);
        assert_eq!(Balances::total_issuance(), issuance_after_funding);
        assert_eq!(IssuanceCap::remaining_allowance(), allowance_after_funding);
    });
}

#[test]
fn v14_equal_points_ignore_backing_at_validator_level_then_share_by_exposure() {
    let (mut ext, fixture) = v14_migration_test_ext(10_000 * DECIMALS);
    ext.execute_with(|| {
        <v14_migration::V14Migration as OnRuntimeUpgrade>::on_runtime_upgrade();
        assert_ok!(SecurityBudget::activate(RuntimeOrigin::root()));
        let era = 11;
        let validator_a = fixture.validators[0].clone();
        let validator_b = fixture.validators[1].clone();
        let nominator = fixture.nominators[0].clone();
        install_v14_exposure(era, &validator_a, 10_000 * DECIMALS, &[], Perbill::zero());
        install_v14_exposure(
            era,
            &validator_b,
            10_000 * DECIMALS,
            &[(nominator.clone(), 10_000 * DECIMALS)],
            Perbill::from_percent(20),
        );
        pallet_staking::ErasRewardPoints::<Runtime>::insert(
            era,
            EraRewardPoints {
                total: 100,
                individual: BTreeMap::from([(validator_a.clone(), 50), (validator_b.clone(), 50)]),
            },
        );
        let year = SecurityBudgetMillisecondsPerYear::get();
        assert_ok!(SecurityBudget::finalize_completed_era(
            era,
            year,
            1_000_000 + year,
        ));
        let budget = SecurityBudget::era_budget(era).expect("reserved V14 budget");
        assert_eq!(budget.total, 3_000 * DECIMALS);
        assert_eq!(budget.eligible_stake, 30_000 * DECIMALS);
        assert_eq!(budget.eligible_points, 100);
        let issuance = Balances::total_issuance();
        let allowance = IssuanceCap::remaining_allowance();
        assert_noop!(
            SecurityBudget::claim_reward_page(
                RuntimeOrigin::signed(account(200)),
                era,
                validator_a.clone(),
                0,
            ),
            pallet_security_budget::Error::<Runtime>::InvalidEra
        );
        pallet_staking::ActiveEra::<Runtime>::put(pallet_staking::ActiveEraInfo {
            index: 12,
            start: Some(1_000_000 + year),
        });
        let a_before = Balances::free_balance(&validator_a);
        let b_before = Balances::free_balance(&validator_b);
        let n_before = Balances::free_balance(&nominator);
        assert_noop!(
            SecurityBudget::claim_reward_page(
                RuntimeOrigin::signed(account(200)),
                era,
                validator_a.clone(),
                1,
            ),
            pallet_security_budget::Error::<Runtime>::InvalidPage
        );
        assert_ok!(SecurityBudget::claim_reward_page(
            RuntimeOrigin::signed(account(200)),
            era,
            validator_a.clone(),
            0,
        ));
        assert_ok!(SecurityBudget::claim_reward_page(
            RuntimeOrigin::signed(account(201)),
            era,
            validator_b.clone(),
            0,
        ));
        assert_eq!(
            Balances::free_balance(&validator_a) - a_before,
            1_500 * DECIMALS
        );
        assert_eq!(
            Balances::free_balance(&validator_b) - b_before,
            900 * DECIMALS
        );
        assert_eq!(
            Balances::free_balance(&nominator) - n_before,
            600 * DECIMALS
        );
        assert_eq!(
            SecurityBudget::validator_paid(era, &validator_a),
            1_500 * DECIMALS
        );
        assert_eq!(
            SecurityBudget::validator_paid(era, &validator_b),
            1_500 * DECIMALS
        );
        assert_eq!(SecurityBudget::era_paid(era), budget.total);
        assert_eq!(SecurityBudget::era_liability(era), 0);
        assert_eq!(SecurityBudget::committed_liabilities(), 0);
        assert_noop!(
            SecurityBudget::claim_reward_page(
                RuntimeOrigin::signed(account(202)),
                era,
                validator_a,
                0,
            ),
            pallet_security_budget::Error::<Runtime>::AlreadyClaimed
        );
        assert_eq!(Balances::total_issuance(), issuance);
        assert_eq!(IssuanceCap::remaining_allowance(), allowance);
    });
}

#[test]
fn v14_commission_zero_five_and_twenty_are_historical_and_above_twenty_is_atomic() {
    for percent in [0u32, 5, 20] {
        let (mut ext, fixture) = v14_migration_test_ext(10_000 * DECIMALS);
        ext.execute_with(|| {
            <v14_migration::V14Migration as OnRuntimeUpgrade>::on_runtime_upgrade();
            assert_ok!(SecurityBudget::activate(RuntimeOrigin::root()));
            let era = 11;
            let validator = fixture.validators[0].clone();
            let nominator = fixture.nominators[0].clone();
            install_v14_exposure(
                era,
                &validator,
                10_000 * DECIMALS,
                &[(nominator.clone(), 10_000 * DECIMALS)],
                Perbill::from_percent(percent),
            );
            pallet_staking::ErasRewardPoints::<Runtime>::insert(
                era,
                EraRewardPoints {
                    total: 100,
                    individual: BTreeMap::from([(validator.clone(), 100)]),
                },
            );
            let year = SecurityBudgetMillisecondsPerYear::get();
            assert_ok!(SecurityBudget::finalize_completed_era(
                era,
                year,
                1_000_000 + year,
            ));
            pallet_staking::Validators::<Runtime>::mutate(&validator, |prefs| {
                prefs.commission = Perbill::from_percent(20);
            });
            pallet_staking::ActiveEra::<Runtime>::put(pallet_staking::ActiveEraInfo {
                index: 12,
                start: Some(1_000_000 + year),
            });
            let budget = SecurityBudget::era_budget(era).unwrap().total;
            assert_eq!(budget, 2_000 * DECIMALS);
            let commission = budget * percent as u128 / 100;
            let shared = budget - commission;
            let validator_before = Balances::free_balance(&validator);
            let nominator_before = Balances::free_balance(&nominator);
            assert_ok!(SecurityBudget::claim_reward_page(
                RuntimeOrigin::signed(account(220)),
                era,
                validator.clone(),
                0,
            ));
            assert_eq!(
                Balances::free_balance(&validator) - validator_before,
                commission + shared / 2
            );
            assert_eq!(
                Balances::free_balance(&nominator) - nominator_before,
                shared / 2
            );
        });
    }

    let (mut over, fixture) = v14_migration_test_ext(10_000 * DECIMALS);
    over.execute_with(|| {
        <v14_migration::V14Migration as OnRuntimeUpgrade>::on_runtime_upgrade();
        assert_ok!(SecurityBudget::activate(RuntimeOrigin::root()));
        let validator = fixture.validators[0].clone();
        install_v14_exposure(
            11,
            &validator,
            10_000 * DECIMALS,
            &[],
            Perbill::from_percent(21),
        );
        pallet_staking::ErasRewardPoints::<Runtime>::insert(
            11,
            EraRewardPoints {
                total: 1,
                individual: BTreeMap::from([(validator, 1)]),
            },
        );
        let issuance = Balances::total_issuance();
        let allowance = IssuanceCap::remaining_allowance();
        assert_noop!(
            SecurityBudget::finalize_completed_era(
                11,
                SecurityBudgetMillisecondsPerYear::get(),
                1_000_000 + SecurityBudgetMillisecondsPerYear::get(),
            ),
            pallet_security_budget::Error::<Runtime>::CommissionAboveMaximum
        );
        assert!(SecurityBudget::era_budget(11).is_none());
        assert_eq!(Balances::total_issuance(), issuance);
        assert_eq!(IssuanceCap::remaining_allowance(), allowance);
    });
}

#[test]
fn v14_transaction_adapter_routes_full_partial_and_zero_corrected_fees() {
    use frame_support::dispatch::{GetDispatchInfo, Pays, PostDispatchInfo};
    use pallet_transaction_payment::OnChargeTransaction;
    type Adapter = <Runtime as pallet_transaction_payment::Config>::OnChargeTransaction;

    for corrected in [1_000u128, 101, 0] {
        let (mut ext, fixture) = v14_migration_test_ext(10_000 * DECIMALS);
        ext.execute_with(|| {
            <v14_migration::V14Migration as OnRuntimeUpgrade>::on_runtime_upgrade();
            assert_ok!(SecurityBudget::activate(RuntimeOrigin::root()));
            install_v14_equal_points(11, &fixture.validators);
            let year = SecurityBudgetMillisecondsPerYear::get();
            assert_ok!(SecurityBudget::finalize_completed_era(
                11,
                year,
                1_000_000 + year,
            ));
            let payer = fixture.validators[0].clone();
            let call = RuntimeCall::System(frame_system::Call::remark { remark: vec![] });
            let info = call.get_dispatch_info();
            let payer_before = Balances::free_balance(&payer);
            let issuance = Balances::total_issuance();
            let allowance = IssuanceCap::remaining_allowance();
            let withdrawn = <Adapter as OnChargeTransaction<Runtime>>::withdraw_fee(
                &payer, &call, &info, 1_000, 0,
            )
            .expect("predicted V14 fee withdrawal");
            <Adapter as OnChargeTransaction<Runtime>>::correct_and_deposit_fee(
                &payer,
                &info,
                &PostDispatchInfo {
                    actual_weight: Some(Weight::from_parts(1, 0)),
                    pays_fee: Pays::Yes,
                },
                corrected,
                0,
                withdrawn,
            )
            .expect("V14 corrected fee routing");
            assert_eq!(payer_before - Balances::free_balance(&payer), corrected);
            assert_eq!(SecurityBudget::normal_fee_routed_total(), corrected);
            assert_eq!(SecurityBudget::pending_collection_obligations(), Some(0));
            assert_eq!(Balances::total_issuance(), issuance);
            assert_eq!(IssuanceCap::remaining_allowance(), allowance);
        });
    }
}

#[test]
fn v14_policy_filter_enforces_commission_bond_and_one_at_a_time_growth_to_sixteen() {
    let (mut ext, fixture) = new_test_ext(16, 0, 4, 4);
    ext.execute_with(|| {
        let validator = fixture.validators[0].clone();
        let at_limit = RuntimeCall::Staking(pallet_staking::Call::validate {
            prefs: ValidatorPrefs {
                commission: Perbill::from_percent(20),
                blocked: false,
            },
        });
        let over_limit = RuntimeCall::Staking(pallet_staking::Call::validate {
            prefs: ValidatorPrefs {
                commission: Perbill::from_percent(21),
                blocked: false,
            },
        });
        assert!(<IssuanceCallFilter as Contains<RuntimeCall>>::contains(
            &at_limit
        ));
        assert!(!<IssuanceCallFilter as Contains<RuntimeCall>>::contains(
            &over_limit
        ));
        let remove_bond = RuntimeCall::Staking(pallet_staking::Call::set_staking_configs {
            min_nominator_bond: pallet_staking::ConfigOp::Noop,
            min_validator_bond: pallet_staking::ConfigOp::Remove,
            max_nominator_count: pallet_staking::ConfigOp::Noop,
            max_validator_count: pallet_staking::ConfigOp::Noop,
            chill_threshold: pallet_staking::ConfigOp::Noop,
            min_commission: pallet_staking::ConfigOp::Noop,
            max_staked_rewards: pallet_staking::ConfigOp::Noop,
        });
        let over_min_commission = RuntimeCall::Staking(pallet_staking::Call::set_min_commission {
            new: Perbill::from_percent(21),
        });
        assert!(!<IssuanceCallFilter as Contains<RuntimeCall>>::contains(
            &remove_bond
        ));
        assert!(!<IssuanceCallFilter as Contains<RuntimeCall>>::contains(
            &over_min_commission
        ));

        let jump = RuntimeCall::Staking(pallet_staking::Call::set_validator_count { new: 7 });
        let bulk =
            RuntimeCall::Staking(pallet_staking::Call::increase_validator_count { additional: 2 });
        let scale = RuntimeCall::Staking(pallet_staking::Call::scale_validator_count {
            factor: sp_runtime::Percent::from_percent(25),
        });
        assert!(!<IssuanceCallFilter as Contains<RuntimeCall>>::contains(
            &jump
        ));
        assert!(!<IssuanceCallFilter as Contains<RuntimeCall>>::contains(
            &bulk
        ));
        assert!(!<IssuanceCallFilter as Contains<RuntimeCall>>::contains(
            &scale
        ));
        for expected in 5..=16 {
            let one = RuntimeCall::Staking(pallet_staking::Call::increase_validator_count {
                additional: 1,
            });
            assert!(<IssuanceCallFilter as Contains<RuntimeCall>>::contains(
                &one
            ));
            assert_ok!(one.dispatch(RuntimeOrigin::root()));
            assert_eq!(pallet_staking::ValidatorCount::<Runtime>::get(), expected);
        }
        assert_eq!(pallet_staking::ValidatorCount::<Runtime>::get(), 16);
        assert_noop!(
            Staking::increase_validator_count(RuntimeOrigin::root(), 1),
            pallet_staking::Error::<Runtime>::TooManyValidators
        );
        assert_eq!(pallet_staking::ValidatorCount::<Runtime>::get(), 16);
        assert!(pallet_staking::Validators::<Runtime>::contains_key(
            validator
        ));
    });
}

#[test]
fn v13_single_block_migration_repairs_the_r6r3_state_in_the_required_order() {
    assert_same_type::<
        <Runtime as frame_system::Config>::SingleBlockMigrations,
        (v13_migration::V13Migration, v14_migration::V14Migration),
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

        let weight = <v13_migration::V13Migration as OnRuntimeUpgrade>::on_runtime_upgrade();
        assert_eq!(weight, v13_migration::declared_weight());
        assert_eq!(Balances::total_issuance(), CAP_TEST_BASELINE);
        assert_eq!(IssuanceCap::remaining_allowance(), Some(CAP_TEST_ALLOWANCE));
        let marker = issuance_cap::V13MigrationCompleted::<Runtime>::get()
            .expect("completion marker written after exact reconciliation");
        assert_eq!(marker.migration_version, 13);
        assert_eq!(marker.completed_at, 1_830_990);
        assert_eq!(marker.input, v13_migration::test_input());
        let custody = v13_migration::custody_destinations();
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
            Balances::free_balance(&custody[1]),
            19_999_998_999_700_000_000_000_000
        );
        let infrastructure_deposits = v13_migration::test_ecosystem_preserved_accounts()
            .iter()
            .map(Balances::free_balance)
            .sum::<Balance>();
        assert_eq!(infrastructure_deposits, 200_000_000_000_000);
        assert_eq!(
            Balances::free_balance(&custody[1])
                + Balances::free_balance(&sudo)
                + infrastructure_deposits,
            upgrade13_policy::ECOSYSTEM_POOL
        );
        assert_eq!(
            pallet_reward_reserve::CommittedLiabilities::<Runtime>::get(),
            0
        );
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
            Balances::free_balance(&custody[0]),
            upgrade13_policy::ACTIVE_PRESALE_POOL
        );
        assert_eq!(
            Balances::free_balance(&custody[2]),
            upgrade13_policy::LIQUIDITY_RESERVE_POOL
        );
    });
}

#[test]
fn v13_community_destination_collisions_roll_back() {
    let mut duplicate = v13_migration_test_ext();
    duplicate.execute_with(|| {
        let mut input = v13_migration::test_input();
        input.community_onboarding_destination = v13_migration::custody_destinations()[0].clone();
        frame_support::storage::unhashed::put(&issuance_cap::v13_migration_input_key(), &input);
        let before = v13_storage_root();
        assert_v13_migration_panics();
        assert_eq!(v13_storage_root(), before);
        assert_eq!(issuance_cap::V13MigrationCompleted::<Runtime>::get(), None);
    });

    let mut alias = v13_migration_test_ext();
    alias.execute_with(|| {
        let mut input = v13_migration::test_input();
        input.community_onboarding_destination = v13_migration::test_sudo_account();
        frame_support::storage::unhashed::put(&issuance_cap::v13_migration_input_key(), &input);
        let before = v13_storage_root();
        assert_v13_migration_panics();
        assert_eq!(v13_storage_root(), before);
        assert_eq!(issuance_cap::V13MigrationCompleted::<Runtime>::get(), None);
    });
}

#[test]
fn v13_trailing_deployment_input_fails_without_state_change() {
    let mut ext = v13_migration_test_ext();
    ext.execute_with(|| {
        let mut encoded = v13_migration::test_input().encode();
        encoded.push(0);
        sp_io::storage::set(&issuance_cap::v13_migration_input_key(), &encoded);
        let before = v13_storage_root();
        assert_v13_migration_panics();
        assert_eq!(v13_storage_root(), before);
        assert_eq!(issuance_cap::V13MigrationCompleted::<Runtime>::get(), None);
        assert_eq!(IssuanceCap::remaining_allowance(), None);
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
        assert_v13_migration_panics();
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
        assert_v13_migration_panics();
        assert_eq!(v13_storage_root(), before);
        assert_eq!(issuance_cap::V13MigrationCompleted::<Runtime>::get(), None);
    });

    let mut version = v13_migration_test_ext();
    version.execute_with(|| {
        StorageVersion::new(0).put::<IssuanceCap>();
        let before = v13_storage_root();
        assert_v13_migration_panics();
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
        assert_v13_migration_panics();
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
        assert_v13_migration_panics();
        assert_eq!(v13_storage_root(), before);
    });

    let mut allowance = v13_migration_test_ext();
    allowance.execute_with(|| {
        <v13_migration::V13Migration as OnRuntimeUpgrade>::on_runtime_upgrade();
        issuance_cap::RemainingAllowance::<Runtime>::put(CAP_TEST_ALLOWANCE - 1);
        let before = v13_storage_root();
        assert_v13_migration_panics();
        assert_eq!(v13_storage_root(), before);
    });
}

#[test]
fn v13_legacy_completion_bridge_moves_exact_principals_and_replays_without_writes() {
    let mut ext = v13_legacy_completed_test_ext();
    ext.execute_with(|| {
        let marker_key = issuance_cap::V13MigrationCompleted::<Runtime>::hashed_key();
        let legacy_marker = v13_migration::test_legacy_marker_bytes();
        assert_eq!(
            sp_io::storage::get(&marker_key).map(|value| value.to_vec()),
            Some(legacy_marker)
        );
        assert_eq!(
            sp_io::storage::get(&marker_key).unwrap().len(),
            166,
            "sealed historical marker length"
        );

        let legacy = v13_migration::test_legacy_destinations();
        let current = v13_migration::custody_destinations();
        let amounts = v13_migration::test_transfer_amounts();
        for (destination, expected) in legacy.iter().zip(amounts) {
            assert_eq!(Balances::free_balance(destination), expected);
        }
        for destination in &current {
            assert_eq!(Balances::free_balance(destination), 0);
            assert!(!frame_system::Account::<Runtime>::contains_key(destination));
        }

        let sudo = v13_migration::test_sudo_account();
        let [fee, treasury] = v13_migration::test_ecosystem_preserved_accounts();
        let operational = [sudo.clone(), fee.clone(), treasury.clone()];
        let operational_before = operational.each_ref().map(balance_account_storage);
        assert_eq!(pallet_sudo::Key::<Runtime>::get(), Some(sudo));
        assert_eq!(
            Balances::free_balance(&operational[0]),
            1_000_100_000_000_000_000
        );
        assert_eq!(Balances::free_balance(&operational[1]), 100_000_000_000_000);
        assert_eq!(Balances::free_balance(&operational[2]), 100_000_000_000_000);
        assert!(v13_migration::test_ecosystem_accounting_is_exact());

        let issuance = Balances::total_issuance();
        assert_eq!(
            <v13_migration::V13Migration as OnRuntimeUpgrade>::on_runtime_upgrade(),
            v13_migration::declared_weight()
        );
        assert_eq!(Balances::total_issuance(), issuance);
        assert_eq!(sp_io::storage::get(&marker_key).unwrap().len(), 70);
        let current_marker = issuance_cap::V13MigrationCompleted::<Runtime>::get()
            .expect("canonical current marker after legacy bridge");
        assert_eq!(current_marker.migration_version, 13);
        assert_eq!(
            current_marker.completed_at,
            v13_migration::test_legacy_completed_at()
        );
        assert_eq!(
            current_marker.input.community_onboarding_destination,
            v13_migration::test_legacy_community_destination()
        );
        let mut expected_input = v13_migration::test_input();
        expected_input.community_onboarding_destination =
            v13_migration::test_legacy_community_destination();
        assert_eq!(
            sp_io::storage::get(&marker_key).map(|value| value.to_vec()),
            Some(
                issuance_cap::V13Completion {
                    migration_version: 13,
                    completed_at: v13_migration::test_legacy_completed_at(),
                    input: expected_input,
                }
                .encode()
            )
        );
        for destination in &legacy {
            assert_eq!(Balances::free_balance(destination), 0);
            assert!(
                balance_account_storage(destination)
                    .iter()
                    .all(Option::is_none),
                "legacy account and auxiliary storage removed"
            );
        }
        for (destination, expected) in current.iter().zip(amounts) {
            assert_eq!(Balances::free_balance(destination), expected);
        }
        assert_eq!(
            operational.each_ref().map(balance_account_storage),
            operational_before
        );
        assert_eq!(
            Balances::free_balance(&current[1])
                + Balances::free_balance(&operational[0])
                + Balances::free_balance(&operational[1])
                + Balances::free_balance(&operational[2]),
            upgrade13_policy::ECOSYSTEM_POOL
        );

        System::reset_events();
        let root = v13_storage_root();
        assert_eq!(
            <v13_migration::V13Migration as OnRuntimeUpgrade>::on_runtime_upgrade(),
            v13_migration::declared_weight()
        );
        assert_eq!(v13_storage_root(), root);
        assert!(System::events().is_empty());
    });
}

#[test]
fn v13_sudo_nonce_bridge_accepts_only_the_exact_signed_upgrade_transition() {
    let mut exact = v13_legacy_completed_test_ext();
    exact.execute_with(|| {
        assert_ok!(v13_migration::test_validate_operational_sudo_account_info());
        assert!(v13_migration::test_validate_pre_upgrade_sudo_account_info().is_err());
    });

    for nonce in [22, 24, 25, u32::MAX] {
        let mut invalid = v13_legacy_completed_test_ext();
        invalid.execute_with(|| {
            let sudo = v13_migration::test_sudo_account();
            frame_system::Account::<Runtime>::mutate(&sudo, |info| info.nonce = nonce);
            assert!(v13_migration::test_validate_operational_sudo_account_info().is_err());
            assert_v13_legacy_upgrade_fails_closed();
        });
    }
}

#[test]
fn v13_sudo_nonce_bridge_rejects_every_non_nonce_accountinfo_change() {
    for kind in 0..6 {
        let mut invalid = v13_legacy_completed_test_ext();
        invalid.execute_with(|| {
            let sudo = v13_migration::test_sudo_account();
            frame_system::Account::<Runtime>::mutate(&sudo, |info| match kind {
                0 => info.consumers = 1,
                1 => info.providers = 2,
                2 => info.sufficients = 1,
                3 => info.data.free -= 1,
                4 => info.data.reserved = 1,
                5 => info.data.frozen = 1,
                _ => unreachable!(),
            });
            assert!(v13_migration::test_validate_operational_sudo_account_info().is_err());
            assert_v13_legacy_upgrade_fails_closed();
        });
    }

    let mut unrelated_hash = v13_legacy_completed_test_ext();
    unrelated_hash.execute_with(|| {
        let key =
            frame_system::Account::<Runtime>::hashed_key_for(v13_migration::test_sudo_account());
        let mut bytes = sp_io::storage::get(&key)
            .expect("sealed AccountInfo is present")
            .to_vec();
        bytes[64] ^= 1;
        sp_io::storage::set(&key, &bytes);
        assert!(v13_migration::test_validate_operational_sudo_account_info().is_err());
        assert_v13_legacy_upgrade_fails_closed();
    });
}

#[test]
fn v13_sudo_nonce_bridge_rejects_malformed_accountinfo() {
    for length in [0usize, 79, 81] {
        let mut invalid = v13_legacy_completed_test_ext();
        invalid.execute_with(|| {
            let key = frame_system::Account::<Runtime>::hashed_key_for(
                v13_migration::test_sudo_account(),
            );
            sp_io::storage::set(&key, &vec![0u8; length]);
            assert!(v13_migration::test_validate_operational_sudo_account_info().is_err());
            assert_v13_legacy_upgrade_fails_closed();
        });
    }
}

#[test]
fn v13_legacy_completion_bridge_rolls_back_each_partial_stage() {
    for fail_at in 1..=4 {
        let mut ext = v13_legacy_completed_test_ext();
        ext.execute_with(|| {
            let root = v13_storage_root();
            let events = System::events();
            let marker =
                sp_io::storage::get(&issuance_cap::V13MigrationCompleted::<Runtime>::hashed_key());
            assert!(v13_migration::test_execute_legacy_bridge(Some(fail_at)).is_err());
            assert_eq!(v13_storage_root(), root, "checkpoint {fail_at}");
            assert_eq!(System::events(), events, "checkpoint {fail_at}");
            assert_eq!(
                sp_io::storage::get(&issuance_cap::V13MigrationCompleted::<Runtime>::hashed_key()),
                marker,
                "checkpoint {fail_at}"
            );
        });
    }
}

#[test]
fn v13_legacy_completion_bridge_rejects_wrong_balances_identities_and_destination_state() {
    let mut wrong_legacy_balance = v13_legacy_completed_test_ext();
    wrong_legacy_balance.execute_with(|| {
        let who = v13_migration::test_legacy_destinations()[1].clone();
        frame_system::Account::<Runtime>::mutate(&who, |info| info.data.free -= 1);
        assert_v13_legacy_upgrade_fails_closed();
    });

    let mut wrong_operational_balance = v13_legacy_completed_test_ext();
    wrong_operational_balance.execute_with(|| {
        let who = v13_migration::test_sudo_account();
        frame_system::Account::<Runtime>::mutate(&who, |info| info.data.free -= 1);
        assert_v13_legacy_upgrade_fails_closed();
    });

    let mut wrong_operational_id = v13_legacy_completed_test_ext();
    wrong_operational_id.execute_with(|| {
        pallet_sudo::Key::<Runtime>::put(account(240));
        assert_v13_legacy_upgrade_fails_closed();
    });

    let mut nonempty_destination = v13_legacy_completed_test_ext();
    nonempty_destination.execute_with(|| {
        let who = v13_migration::custody_destinations()[0].clone();
        frame_system::Account::<Runtime>::mutate(&who, |info| {
            info.providers = 1;
            info.data.free = 1;
        });
        assert_v13_legacy_upgrade_fails_closed();
    });

    let mut wrong_legacy_id = v13_legacy_completed_test_ext();
    wrong_legacy_id.execute_with(|| {
        let key = issuance_cap::V13MigrationCompleted::<Runtime>::hashed_key();
        let mut marker = v13_migration::test_legacy_marker_bytes();
        marker[38] ^= 1;
        sp_io::storage::set(&key, &marker);
        assert_v13_legacy_upgrade_fails_closed();
    });
}

#[test]
fn v13_legacy_completion_bridge_rejects_every_auxiliary_balance_state_and_reference() {
    for kind in 0..4 {
        let mut ext = v13_legacy_completed_test_ext();
        ext.execute_with(|| {
            let who = v13_migration::test_legacy_destinations()[kind % 3].clone();
            let key = match kind {
                0 => pallet_balances::Locks::<Runtime>::hashed_key_for(&who),
                1 => pallet_balances::Reserves::<Runtime>::hashed_key_for(&who),
                2 => pallet_balances::Holds::<Runtime>::hashed_key_for(&who),
                3 => pallet_balances::Freezes::<Runtime>::hashed_key_for(&who),
                _ => unreachable!(),
            };
            sp_io::storage::set(&key, &[0]);
            assert_v13_legacy_upgrade_fails_closed();
        });
    }

    let mut unexpected_reference = v13_legacy_completed_test_ext();
    unexpected_reference.execute_with(|| {
        let who = v13_migration::test_legacy_destinations()[0].clone();
        frame_system::Account::<Runtime>::mutate(&who, |info| info.consumers = 1);
        assert_v13_legacy_upgrade_fails_closed();
    });

    let mut partial_custody_state = v13_legacy_completed_test_ext();
    partial_custody_state.execute_with(|| {
        era_v14_custody_governance::NextRequestId::<Runtime>::insert(
            era_v14_custody_governance::CustodyCategory::Presale,
            1,
        );
        assert_v13_legacy_upgrade_fails_closed();
    });
}

#[cfg(feature = "try-runtime")]
#[test]
fn v13_legacy_completion_try_runtime_proves_exact_transition() {
    let mut ext = v13_legacy_completed_test_ext();
    ext.execute_with(|| {
        assert_ok!(<v13_migration::V13Migration as OnRuntimeUpgrade>::try_on_runtime_upgrade(true));
        assert_eq!(
            sp_io::storage::get(&issuance_cap::V13MigrationCompleted::<Runtime>::hashed_key())
                .unwrap()
                .len(),
            70
        );
    });
}
#[test]
fn v13_completion_marker_classifier_fails_closed_without_writes() {
    let mut cases = vec![
        (vec![], false),
        (vec![0u8; 1], false),
        (vec![0u8; 69], false),
        (vec![0u8; 70], true),
        (vec![0u8; 166], true),
    ];
    let mut truncated = v13_migration::test_legacy_marker_bytes();
    truncated.pop();
    cases.push((truncated, false));
    let mut trailing = v13_migration::test_legacy_marker_bytes();
    trailing.push(0);
    cases.push((trailing, false));

    for (bytes, canonical_but_semantically_invalid) in cases {
        let mut ext = v13_legacy_completed_test_ext();
        ext.execute_with(|| {
            let key = issuance_cap::V13MigrationCompleted::<Runtime>::hashed_key();
            sp_io::storage::set(&key, &bytes);
            let root = v13_storage_root();
            let events = System::events();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
                <v13_migration::V13Migration as OnRuntimeUpgrade>::on_runtime_upgrade,
            ));
            if canonical_but_semantically_invalid {
                assert!(result.is_err());
            } else {
                assert!(matches!(
                    result,
                    Ok(weight) if weight == v13_migration::declared_weight()
                ));
            }
            assert_eq!(v13_storage_root(), root);
            assert_eq!(System::events(), events);
            assert_eq!(
                sp_io::storage::get(&key).map(|value| value.to_vec()),
                Some(bytes)
            );
        });
    }
}

#[cfg(feature = "try-runtime")]
#[test]
fn v13_try_runtime_rejects_malformed_and_unknown_marker_branches_without_writes() {
    let mut cases = vec![vec![], vec![0u8; 70], vec![0u8; 166], vec![0u8; 167]];
    let mut truncated = v13_migration::test_legacy_marker_bytes();
    truncated.pop();
    cases.push(truncated);
    let mut trailing = v13_migration::test_legacy_marker_bytes();
    trailing.push(0);
    cases.push(trailing);

    for bytes in cases {
        let mut ext = v13_legacy_completed_test_ext();
        ext.execute_with(|| {
            sp_io::storage::set(
                &issuance_cap::V13MigrationCompleted::<Runtime>::hashed_key(),
                &bytes,
            );
            let root = v13_storage_root();
            assert!(<v13_migration::V13Migration as OnRuntimeUpgrade>::pre_upgrade().is_err());
            assert_eq!(v13_storage_root(), root);
        });
    }
}

#[test]
fn v13_legacy_bridge_preserves_strict_three_of_three_ordinary_control() {
    let mut ext = v13_legacy_completed_test_ext();
    ext.execute_with(|| {
        <v13_migration::V13Migration as OnRuntimeUpgrade>::on_runtime_upgrade();
        let category = era_v14_custody_governance::CustodyCategory::Ecosystem;
        let signers = FounderCustodySigners::get();
        let destination = account(241);
        let amount = DECIMALS;
        assert_ok!(FounderCustody::approve_withdrawal(
            RuntimeOrigin::signed(signers[0].clone()),
            category,
            0,
            destination.clone(),
            amount,
        ));
        assert_ok!(FounderCustody::approve_withdrawal(
            RuntimeOrigin::signed(signers[1].clone()),
            category,
            0,
            destination.clone(),
            amount,
        ));
        assert_eq!(Balances::free_balance(&destination), 0);
        assert_ok!(FounderCustody::approve_withdrawal(
            RuntimeOrigin::signed(signers[2].clone()),
            category,
            0,
            destination.clone(),
            amount,
        ));
        assert_eq!(Balances::free_balance(&destination), amount);
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
fn v13_legacy_bridge_maximum_liability_proof_fits_declared_weight() {
    let mut ext = v13_legacy_completed_test_ext();
    ext.execute_with(|| {
        for era in 0..v13_migration::MAX_REWARD_LIABILITY_ENTRIES {
            pallet_reward_reserve::EraRewardLiabilities::<Runtime>::insert(era, 1);
        }
        pallet_reward_reserve::CommittedLiabilities::<Runtime>::put(
            v13_migration::MAX_REWARD_LIABILITY_ENTRIES as Balance,
        );
    });
    ext.commit_all()
        .expect("maximum-liability fixture commits to the proving backend");

    let (weight, proof) = ext.execute_and_prove(|| {
        <v13_migration::V13Migration as OnRuntimeUpgrade>::on_runtime_upgrade()
    });
    let declared = v13_migration::declared_weight();
    assert_eq!(weight, declared);
    assert!(
        (proof.encoded_size() as u64) <= declared.proof_size(),
        "legacy bridge proof {} exceeds declared {}",
        proof.encoded_size(),
        declared.proof_size()
    );
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

#[test]
fn r3b_observer_is_storage_event_only_at_index_21() {
    assert_eq!(
        <PalletInfo as PalletInfoTrait>::index::<ValidatorSecurity>(),
        Some(21)
    );
    assert_same_type::<
        <Runtime as pallet_babe::Config>::EquivocationReportSystem,
        BabeObserveOnlyReportSystem,
    >();
    assert_same_type::<
        <Runtime as pallet_grandpa::Config>::EquivocationReportSystem,
        GrandpaObserveOnlyReportSystem,
    >();
    assert_eq!(ReportLongevity::get(), 14_400);
    assert_eq!(
        <Runtime as era_validator_security::observation::Config>::BabeMaxAuthorities::get(),
        MaxBabeAuthorities::get()
    );
    assert_eq!(
        <Runtime as era_validator_security::observation::Config>::GrandpaMaxAuthorities::get(),
        MaxGrandpaAuthorities::get()
    );
}

#[cfg(not(feature = "runtime-benchmarks"))]
struct R3bObserverTestSessionManager;

#[cfg(not(feature = "runtime-benchmarks"))]
impl pallet_session::SessionManager<AccountId> for R3bObserverTestSessionManager {
    fn new_session(_: sp_staking::SessionIndex) -> Option<Vec<AccountId>> {
        Some(Session::validators())
    }
    fn new_session_genesis(_: sp_staking::SessionIndex) -> Option<Vec<AccountId>> {
        Some(Session::validators())
    }
    fn start_session(_: sp_staking::SessionIndex) {}
    fn end_session(_: sp_staking::SessionIndex) {}
}

#[cfg(not(feature = "runtime-benchmarks"))]
impl
    pallet_session::historical::SessionManager<
        AccountId,
        pallet_staking::Exposure<AccountId, Balance>,
    > for R3bObserverTestSessionManager
{
    fn new_session(
        _: sp_staking::SessionIndex,
    ) -> Option<Vec<(AccountId, pallet_staking::Exposure<AccountId, Balance>)>> {
        Some(
            Session::validators()
                .into_iter()
                .map(|validator| (validator, Default::default()))
                .collect(),
        )
    }
    fn new_session_genesis(
        _: sp_staking::SessionIndex,
    ) -> Option<Vec<(AccountId, pallet_staking::Exposure<AccountId, Balance>)>> {
        <Self as pallet_session::historical::SessionManager<_, _>>::new_session(0)
    }
    fn start_session(_: sp_staking::SessionIndex) {}
    fn end_session(_: sp_staking::SessionIndex) {}
}

#[cfg(not(feature = "runtime-benchmarks"))]
pub(super) fn r3b_valid_babe_reports(count: u64) -> Vec<BabeObservationEvidence> {
    use frame_support::traits::KeyOwnerProofSystem;
    use pallet_session::SessionManager;
    use sp_consensus_babe::digests::{CompatibleDigestItem, PreDigest, SecondaryPlainPreDigest};
    use sp_core::Pair;
    use sp_runtime::traits::Header as HeaderT;

    let mut validators = Vec::new();
    let mut first_pair = None;
    for index in 0..4u8 {
        let account = AccountId::new([0xa0 + index; 32]);
        let babe = sp_consensus_babe::AuthorityPair::from_seed(&[0xb0 + index; 32]);
        let grandpa = sp_consensus_grandpa::AuthorityPair::from_seed(&[0xc0 + index; 32]);
        pallet_session::NextKeys::<Runtime>::insert(
            &account,
            SessionKeys {
                babe: babe.public(),
                grandpa: grandpa.public(),
            },
        );
        validators.push(account);
        if index == 0 {
            first_pair = Some(babe);
        }
    }
    pallet_session::Validators::<Runtime>::put(validators);
    pallet_session::CurrentIndex::<Runtime>::put(0);
    pallet_babe::GenesisSlot::<Runtime>::put(sp_consensus_babe::Slot::from(0));
    pallet_babe::SkippedEpochs::<Runtime>::kill();
    let pair = first_pair.expect("four validators include first BABE pair");
    type HistoricalRootManager =
        pallet_session::historical::NoteHistoricalRoot<Runtime, R3bObserverTestSessionManager>;
    <HistoricalRootManager as SessionManager<AccountId>>::new_session(0)
        .expect("test historical validator set");
    let key_owner_proof = Historical::prove((sp_consensus_babe::KEY_TYPE, pair.public()))
        .expect("test historical BABE proof");
    pallet_session::CurrentIndex::<Runtime>::put(1);

    let header = |slot: sp_consensus_babe::Slot, marker: u8| {
        let digest = Digest {
            logs: vec![DigestItem::babe_pre_digest(PreDigest::SecondaryPlain(
                SecondaryPlainPreDigest {
                    authority_index: 0,
                    slot,
                },
            ))],
        };
        let mut header = <Header as HeaderT>::new(
            1,
            H256::repeat_byte(marker),
            H256::repeat_byte(marker.wrapping_add(1)),
            H256::zero(),
            digest,
        );
        let prehash = header.hash();
        header
            .digest_mut()
            .push(DigestItem::babe_seal(pair.sign(prehash.as_ref())));
        header
    };

    (1..=count)
        .map(|slot_number| {
            let slot = sp_consensus_babe::Slot::from(slot_number);
            let marker = slot_number as u8;
            (
                sp_consensus_babe::EquivocationProof {
                    offender: pair.public(),
                    slot,
                    first_header: header(slot, marker),
                    second_header: header(slot, marker.wrapping_add(64)),
                },
                key_owner_proof.clone(),
            )
        })
        .collect()
}

#[cfg(not(feature = "runtime-benchmarks"))]
#[test]
fn r3b_verified_reports_respect_block_weight_and_sixteen_record_cap() {
    use frame_support::dispatch::GetDispatchInfo;

    let (mut ext, _) = new_test_ext(4, 0, 4, 4);
    ext.execute_with(|| {
        StorageVersion::new(1).put::<ValidatorSecurity>();
        let mut reports = r3b_valid_babe_reports(17).into_iter();
        let before_weight = System::block_weight().total();
        let mut declared = Weight::zero();
        let mut admitted=0u32;
        for _ in 0..16 {
            let evidence = reports.next().expect("16 distinct BABE reports");
            let call = RuntimeCall::Babe(pallet_babe::Call::report_equivocation_unsigned {
                equivocation_proof: Box::new(evidence.0.clone()),
                key_owner_proof: evidence.1.clone(),
            });
            let extrinsic = UncheckedExtrinsic::new_bare(call);
            let encoded_length_weight = Weight::from_parts(0, extrinsic.encode().len() as u64);
            let info = extrinsic.get_dispatch_info();
            let base = <<Runtime as frame_system::Config>::BlockWeights as Get<
                frame_system::limits::BlockWeights,
            >>::get()
            .get(info.class)
            .base_extrinsic;
            let weight=info.total_weight().saturating_add(base).saturating_add(encoded_length_weight);
            let normal=<<Runtime as frame_system::Config>::BlockWeights as Get<frame_system::limits::BlockWeights>>::get().get(frame_support::dispatch::DispatchClass::Normal).max_total.unwrap();
            if !System::block_weight().get(frame_support::dispatch::DispatchClass::Normal).saturating_add(weight).all_lte(normal) {
                let events=r3b_observer_event_count_unconditional();
                assert_eq!(Executive::apply_extrinsic(extrinsic),Err(sp_runtime::transaction_validity::TransactionValidityError::Invalid(sp_runtime::transaction_validity::InvalidTransaction::ExhaustsResources)));
                assert_eq!(r3b_observer_event_count_unconditional(),events);
                // Independently exercise the unchanged16-record cap through direct dispatch.
                // These extra dispatches do not claim inclusion in this full block.
                assert_ok!(RuntimeCall::Babe(pallet_babe::Call::report_equivocation_unsigned{equivocation_proof:Box::new(evidence.0),key_owner_proof:evidence.1}).dispatch(RuntimeOrigin::none()));
                for evidence in reports.by_ref().take((15-admitted) as usize) {assert_ok!(RuntimeCall::Babe(pallet_babe::Call::report_equivocation_unsigned{equivocation_proof:Box::new(evidence.0),key_owner_proof:evidence.1}).dispatch(RuntimeOrigin::none()));}
                break;
            }
            Executive::apply_extrinsic(extrinsic).expect("weight-admissible BABE report").expect("successful BABE observer dispatch");
            declared=declared.saturating_add(weight);admitted+=1;
        }
        assert!(admitted>0,"at least one fully verified report must fit a normal block");
        assert_eq!(
            System::block_weight().total().saturating_sub(before_weight),
            declared,
        );
        assert_eq!(
            era_validator_security::observation::NewObservationsInBlock::<Runtime>::get(),
            Some((1, 16)),
        );
        assert_eq!(
            era_validator_security::observation::ObservationCount::<Runtime>::get(),
            16,
        );
        assert_eq!(r3b_observer_event_count_unconditional(), 16);

        let seventeenth = reports.next().expect("17th distinct BABE report");
        let events_before = r3b_observer_event_count_unconditional();
        let result = RuntimeCall::Babe(pallet_babe::Call::report_equivocation_unsigned {
            equivocation_proof: Box::new(seventeenth.0),
            key_owner_proof: seventeenth.1,
        })
        .dispatch(RuntimeOrigin::none());
        assert_eq!(
            result
                .expect_err("17th report exceeds per-block observation limit")
                .error,
            era_validator_security::observation::Error::<Runtime>::TooManyNewObservationsThisBlock
                .into(),
        );
        assert_eq!(r3b_observer_event_count_unconditional(), events_before);
        assert_eq!(
            era_validator_security::observation::ObservationCount::<Runtime>::get(),
            16,
        );
    });
}

#[cfg(not(feature = "runtime-benchmarks"))]
fn r3b_observer_event_count_unconditional() -> usize {
    System::events()
        .iter()
        .filter(|record| matches!(&record.event, RuntimeEvent::ValidatorSecurity(_)))
        .count()
}

#[test]
fn r3b_pinned_sdk_babe_and_grandpa_proof_codec_hash_vectors_are_fixed() {
    use sp_runtime::traits::Header as HeaderT;

    // Independently encoded from pinned SDK f3969c7 field order and SCALE primitives.
    let babe_bytes = [
        &[0x11; 32][..],
        &[7, 0, 0, 0, 0, 0, 0, 0],
        &[0x01; 32],
        &[4],
        &[0x02; 32],
        &[0x03; 32],
        &[0],
        &[0x04; 32],
        &[8],
        &[0x05; 32],
        &[0x06; 32],
        &[0],
    ]
    .concat();
    let expected_babe = sp_consensus_babe::EquivocationProof {
        offender: sp_consensus_babe::AuthorityId::from(sp_core::sr25519::Public::from_raw(
            [0x11; 32],
        )),
        slot: sp_consensus_babe::Slot::from(7),
        first_header: <Header as HeaderT>::new(
            1,
            H256::repeat_byte(0x03),
            H256::repeat_byte(0x02),
            H256::repeat_byte(0x01),
            Digest::default(),
        ),
        second_header: <Header as HeaderT>::new(
            2,
            H256::repeat_byte(0x06),
            H256::repeat_byte(0x05),
            H256::repeat_byte(0x04),
            Digest::default(),
        ),
    };
    let mut babe_input = babe_bytes.as_slice();
    let decoded_babe = sp_consensus_babe::EquivocationProof::<Header>::decode(&mut babe_input)
        .expect("fixed BABE proof bytes decode");
    assert!(babe_input.is_empty());
    assert_eq!(decoded_babe, expected_babe);
    assert_eq!(decoded_babe.encode(), babe_bytes);
    const BABE_HASH: [u8; 32] = [
        0xf0, 0x4d, 0x35, 0xcb, 0x88, 0x2e, 0xba, 0xb4, 0x56, 0x03, 0x25, 0x78, 0x07, 0x1f, 0xd3,
        0x7e, 0x9f, 0x84, 0xdf, 0xdb, 0xa6, 0x01, 0x13, 0xbf, 0x4e, 0x40, 0xf7, 0x1e, 0xa8, 0xff,
        0xe3, 0x53,
    ];
    assert_eq!(sp_io::hashing::blake2_256(&babe_bytes), BABE_HASH);
    assert_eq!(canonical_babe_proof_hash(&decoded_babe), BABE_HASH);

    let grandpa_bytes = [
        &[9, 0, 0, 0, 0, 0, 0, 0][..],
        &[0],
        &[10, 0, 0, 0, 0, 0, 0, 0],
        &[0x22; 32],
        &[0x31; 32],
        &[11, 0, 0, 0],
        &[0x41; 64],
        &[0x32; 32],
        &[12, 0, 0, 0],
        &[0x42; 64],
    ]
    .concat();
    let grandpa_id =
        sp_consensus_grandpa::AuthorityId::from(sp_core::ed25519::Public::from_raw([0x22; 32]));
    let expected_grandpa = sp_consensus_grandpa::EquivocationProof::new(
        9,
        sp_consensus_grandpa::Equivocation::Prevote(finality_grandpa::Equivocation {
            round_number: 10,
            identity: grandpa_id,
            first: (
                finality_grandpa::Prevote {
                    target_hash: H256::repeat_byte(0x31),
                    target_number: 11,
                },
                sp_consensus_grandpa::AuthoritySignature::from(
                    sp_core::ed25519::Signature::from_raw([0x41; 64]),
                ),
            ),
            second: (
                finality_grandpa::Prevote {
                    target_hash: H256::repeat_byte(0x32),
                    target_number: 12,
                },
                sp_consensus_grandpa::AuthoritySignature::from(
                    sp_core::ed25519::Signature::from_raw([0x42; 64]),
                ),
            ),
        }),
    );
    let mut grandpa_input = grandpa_bytes.as_slice();
    let decoded_grandpa =
        sp_consensus_grandpa::EquivocationProof::<Hash, BlockNumber>::decode(&mut grandpa_input)
            .expect("fixed GRANDPA proof bytes decode");
    assert!(grandpa_input.is_empty());
    assert_eq!(decoded_grandpa, expected_grandpa);
    assert_eq!(decoded_grandpa.encode(), grandpa_bytes);
    const GRANDPA_HASH: [u8; 32] = [
        0x35, 0xf1, 0x48, 0x3b, 0xce, 0xac, 0x2e, 0xb7, 0x8e, 0x1c, 0x01, 0x7b, 0x3f, 0x0b, 0x7e,
        0xdc, 0x3f, 0xd8, 0xff, 0xd8, 0x22, 0xa7, 0xb6, 0x9a, 0xd9, 0x0b, 0x5a, 0x70, 0xc9, 0xf5,
        0x9e, 0x93,
    ];
    assert_eq!(sp_io::hashing::blake2_256(&grandpa_bytes), GRANDPA_HASH);
    assert_eq!(canonical_grandpa_proof_hash(&decoded_grandpa), GRANDPA_HASH);
}

#[test]
fn r3b_signed_babe_path_rejects_before_observer_storage() {
    use sp_staking::offence::OffenceReportSystem;

    let (mut ext, _) = new_test_ext(4, 0, 4, 4);
    ext.execute_with(|| {
        StorageVersion::new(1).put::<ValidatorSecurity>();
        let proof = sp_consensus_babe::EquivocationProof {
            slot: sp_consensus_babe::Slot::from(1),
            offender: sp_consensus_babe::AuthorityId::from(sp_core::sr25519::Public::from_raw(
                [1; 32],
            )),
            first_header: <Header as sp_runtime::traits::Header>::new(
                1,
                H256::zero(),
                H256::zero(),
                H256::zero(),
                Default::default(),
            ),
            second_header: <Header as sp_runtime::traits::Header>::new(
                1,
                H256::zero(),
                H256::zero(),
                H256::zero(),
                Default::default(),
            ),
        };
        let evidence = (
            proof,
            sp_session::MembershipProof {
                session: 0,
                trie_nodes: Vec::new(),
                validator_count: 4,
            },
        );
        assert_eq!(
            BabeObserveOnlyReportSystem::process_evidence(Some(account(1)), evidence),
            Err(era_validator_security::observation::Error::<Runtime>::SignedIntakeRejected.into())
        );
        assert_eq!(
            era_validator_security::observation::ObservationCount::<Runtime>::get(),
            0
        );
        assert!(ValidatorSecurity::transient_state_is_clear());
    });
}

#[cfg(feature = "runtime-benchmarks")]
#[test]
fn r3b_benchmark_fixtures_use_retained_historical_babe_and_grandpa_proofs() {
    use era_validator_security::observation::BenchmarkHelper;

    let (mut babe_ext, _) = new_test_ext(4, 0, 4, 4);
    babe_ext.execute_with(|| {
        let evidence =
            RuntimeObserverBenchmarkHelper::setup_babe(100, 0, 0).expect("retained BABE fixture");
        assert_eq!(evidence.1.session, 0);
        assert_eq!(pallet_session::CurrentIndex::<Runtime>::get(), 1);
        assert!(pallet_session::historical::HistoricalSessions::<Runtime>::contains_key(0));
        RuntimeObserverBenchmarkHelper::check_babe(&evidence)
            .expect("retained BABE membership proof verifies");
    });

    let (mut grandpa_ext, _) = new_test_ext(4, 0, 4, 4);
    grandpa_ext.execute_with(|| {
        let evidence = RuntimeObserverBenchmarkHelper::setup_grandpa(100, 0, 0)
            .expect("retained GRANDPA fixture");
        assert_eq!(evidence.1.session, 0);
        assert_eq!(pallet_session::CurrentIndex::<Runtime>::get(), 1);
        assert!(pallet_session::historical::HistoricalSessions::<Runtime>::contains_key(0));
        RuntimeObserverBenchmarkHelper::check_grandpa(&evidence)
            .expect("retained GRANDPA membership proof verifies");
    });
}

#[cfg(feature = "runtime-benchmarks")]
fn r3b_babe_unsigned_call(evidence: &BabeObservationEvidence) -> RuntimeCall {
    RuntimeCall::Babe(pallet_babe::Call::report_equivocation_unsigned {
        equivocation_proof: Box::new(evidence.0.clone()),
        key_owner_proof: evidence.1.clone(),
    })
}

#[cfg(feature = "runtime-benchmarks")]
fn r3b_grandpa_unsigned_call(evidence: &GrandpaObservationEvidence) -> RuntimeCall {
    RuntimeCall::Grandpa(pallet_grandpa::Call::report_equivocation_unsigned {
        equivocation_proof: Box::new(evidence.0.clone()),
        key_owner_proof: evidence.1.clone(),
    })
}

#[cfg(feature = "runtime-benchmarks")]
fn r3b_observer_event_count() -> usize {
    System::events()
        .iter()
        .filter(|record| matches!(&record.event, RuntimeEvent::ValidatorSecurity(_)))
        .count()
}

#[cfg(feature = "runtime-benchmarks")]
fn r3b_assert_babe_sdk_error_unchanged(evidence: BabeObservationEvidence, expected: DispatchError) {
    use era_validator_security::observation::BenchmarkHelper;

    let before = RuntimeObserverBenchmarkHelper::observer_state_commitment();
    let counters = RuntimeObserverBenchmarkHelper::observer_counters_and_transients();
    assert_eq!(RuntimeObserverBenchmarkHelper::observer_event_count(), 0);
    assert_eq!(
        RuntimeObserverBenchmarkHelper::process_babe(evidence),
        Err(expected)
    );
    assert_eq!(
        RuntimeObserverBenchmarkHelper::observer_state_commitment(),
        before
    );
    assert_eq!(
        RuntimeObserverBenchmarkHelper::observer_counters_and_transients(),
        counters
    );
    assert_eq!(RuntimeObserverBenchmarkHelper::observer_event_count(), 0);
}

#[cfg(feature = "runtime-benchmarks")]
fn r3b_assert_grandpa_sdk_error_unchanged(
    evidence: GrandpaObservationEvidence,
    expected: DispatchError,
) {
    use era_validator_security::observation::BenchmarkHelper;

    let before = RuntimeObserverBenchmarkHelper::observer_state_commitment();
    let counters = RuntimeObserverBenchmarkHelper::observer_counters_and_transients();
    assert_eq!(RuntimeObserverBenchmarkHelper::observer_event_count(), 0);
    assert_eq!(
        RuntimeObserverBenchmarkHelper::process_grandpa(evidence),
        Err(expected)
    );
    assert_eq!(
        RuntimeObserverBenchmarkHelper::observer_state_commitment(),
        before
    );
    assert_eq!(
        RuntimeObserverBenchmarkHelper::observer_counters_and_transients(),
        counters
    );
    assert_eq!(RuntimeObserverBenchmarkHelper::observer_event_count(), 0);
}

#[cfg(feature = "runtime-benchmarks")]
#[test]
fn r3b_invalid_babe_signature_equivocation_ownership_and_session_are_exact() {
    use era_validator_security::observation::BenchmarkHelper;
    use sp_runtime::traits::Header as HeaderT;

    let (mut ext, _) = new_test_ext(4, 0, 4, 4);
    ext.execute_with(|| {
        let invalid_equivocation = RuntimeObserverBenchmarkHelper::setup_invalid_babe(4)
            .expect("BABE invalid equivocation fixture");
        r3b_assert_babe_sdk_error_unchanged(
            invalid_equivocation,
            pallet_babe::Error::<Runtime>::InvalidEquivocationProof.into(),
        );

        let mut invalid_signature = RuntimeObserverBenchmarkHelper::setup_babe(4, 0, 0)
            .expect("BABE invalid signature base");
        match invalid_signature
            .0
            .first_header
            .digest_mut()
            .logs
            .last_mut()
        {
            Some(DigestItem::Seal(_, signature)) => signature[0] ^= 0x80,
            _ => panic!("BABE fixture has a final seal"),
        }
        r3b_assert_babe_sdk_error_unchanged(
            invalid_signature,
            pallet_babe::Error::<Runtime>::InvalidEquivocationProof.into(),
        );

        let mut invalid_ownership = RuntimeObserverBenchmarkHelper::setup_babe(4, 0, 0)
            .expect("BABE invalid ownership base");
        invalid_ownership.1.trie_nodes.clear();
        r3b_assert_babe_sdk_error_unchanged(
            invalid_ownership,
            pallet_babe::Error::<Runtime>::InvalidKeyOwnershipProof.into(),
        );

        let mut invalid_session =
            RuntimeObserverBenchmarkHelper::setup_babe(4, 0, 0).expect("BABE invalid session base");
        invalid_session.1.session = 1;
        r3b_assert_babe_sdk_error_unchanged(
            invalid_session,
            pallet_babe::Error::<Runtime>::InvalidKeyOwnershipProof.into(),
        );
    });
}

#[cfg(feature = "runtime-benchmarks")]
#[test]
fn r3b_invalid_grandpa_signature_equivocation_ownership_and_session_are_exact() {
    use era_validator_security::observation::BenchmarkHelper;

    let (mut ext, _) = new_test_ext(4, 0, 4, 4);
    ext.execute_with(|| {
        let invalid_equivocation = RuntimeObserverBenchmarkHelper::setup_invalid_grandpa(4)
            .expect("GRANDPA invalid equivocation fixture");
        r3b_assert_grandpa_sdk_error_unchanged(
            invalid_equivocation,
            pallet_grandpa::Error::<Runtime>::InvalidEquivocationProof.into(),
        );
    });

    let (mut ext, _) = new_test_ext(4, 0, 4, 4);
    ext.execute_with(|| {
        let mut invalid_signature = RuntimeObserverBenchmarkHelper::setup_grandpa(4, 0, 0)
            .expect("GRANDPA invalid signature base");
        let mut encoded = invalid_signature.0.encode();
        // Pinned layout: set-id(8), enum(1), round(8), id(32), prevote(36), then signature.
        encoded[85] ^= 0x80;
        invalid_signature.0 = sp_consensus_grandpa::EquivocationProof::<Hash, BlockNumber>::decode(
            &mut encoded.as_slice(),
        )
        .expect("mutated GRANDPA signature decodes");
        r3b_assert_grandpa_sdk_error_unchanged(
            invalid_signature,
            pallet_grandpa::Error::<Runtime>::InvalidEquivocationProof.into(),
        );
    });

    let (mut ext, _) = new_test_ext(4, 0, 4, 4);
    ext.execute_with(|| {
        let mut invalid_ownership = RuntimeObserverBenchmarkHelper::setup_grandpa(4, 0, 0)
            .expect("GRANDPA invalid ownership base");
        invalid_ownership.1.trie_nodes.clear();
        r3b_assert_grandpa_sdk_error_unchanged(
            invalid_ownership,
            pallet_grandpa::Error::<Runtime>::InvalidKeyOwnershipProof.into(),
        );
    });

    let (mut ext, _) = new_test_ext(4, 0, 4, 4);
    ext.execute_with(|| {
        let mut invalid_session = RuntimeObserverBenchmarkHelper::setup_grandpa(4, 0, 0)
            .expect("GRANDPA invalid session base");
        invalid_session.1.session = 1;
        r3b_assert_grandpa_sdk_error_unchanged(
            invalid_session,
            pallet_grandpa::Error::<Runtime>::InvalidEquivocationProof.into(),
        );
    });
}

#[cfg(feature = "runtime-benchmarks")]
#[test]
fn r3b_grandpa_aggregate_local_inblock_external_predispatch_and_stale_are_exact() {
    use era_validator_security::observation::{BenchmarkHelper, ObservationKind, Observations};
    use frame_support::unsigned::ValidateUnsigned;
    use sp_runtime::transaction_validity::{InvalidTransaction, TransactionSource};

    let (mut ext, _) = new_test_ext(4, 0, 4, 4);
    ext.execute_with(|| {
        let evidence = RuntimeObserverBenchmarkHelper::setup_grandpa(4, 0, 0)
            .expect("valid retained GRANDPA evidence");
        assert_eq!(Session::validators().len(), 4);
        let call = r3b_grandpa_unsigned_call(&evidence);
        let local =
            <Runtime as ValidateUnsigned>::validate_unsigned(TransactionSource::Local, &call)
                .expect("GRANDPA Local reaches aggregate validator");
        assert_eq!(local.longevity, ReportLongevity::get());
        assert!(!local.propagate);
        assert!(<Runtime as ValidateUnsigned>::validate_unsigned(
            TransactionSource::InBlock,
            &call,
        )
        .is_ok());
        assert_eq!(
            <Runtime as ValidateUnsigned>::validate_unsigned(TransactionSource::External, &call,),
            Err(InvalidTransaction::Call.into())
        );
        assert_eq!(
            call.clone()
                .dispatch(RuntimeOrigin::root())
                .expect_err("root is not an unsigned GRANDPA origin")
                .error,
            DispatchError::BadOrigin
        );
        assert_ok!(<Runtime as ValidateUnsigned>::pre_dispatch(&call));
        assert_eq!(
            era_validator_security::observation::ObservationCount::<Runtime>::get(),
            0
        );
        assert_eq!(r3b_observer_event_count(), 0);

        assert_ok!(call.clone().dispatch(RuntimeOrigin::none()));
        assert_eq!(
            era_validator_security::observation::ObservationCount::<Runtime>::get(),
            1
        );
        let stored = Observations::<Runtime>::iter_values()
            .next()
            .expect("GRANDPA record");
        assert_eq!(stored.kind, ObservationKind::Grandpa);
        assert_eq!(
            stored.proof_hash,
            sp_io::hashing::blake2_256(&evidence.0.encode())
        );
        assert_eq!(Session::validators().len(), 4);
        assert_eq!(r3b_observer_event_count(), 1);

        let commitment = RuntimeObserverBenchmarkHelper::observer_state_commitment();
        assert_eq!(
            <Runtime as ValidateUnsigned>::validate_unsigned(TransactionSource::Local, &call,),
            Err(InvalidTransaction::Stale.into())
        );
        assert_eq!(
            <Runtime as ValidateUnsigned>::pre_dispatch(&call),
            Err(InvalidTransaction::Stale.into())
        );
        assert_eq!(
            RuntimeObserverBenchmarkHelper::observer_state_commitment(),
            commitment
        );
        assert_eq!(r3b_observer_event_count(), 1);
    });
}

#[cfg(feature = "runtime-benchmarks")]
#[test]
fn r3b_babe_aggregate_behavior_is_unchanged_and_full_sdk_intake_is_exact() {
    use era_validator_security::observation::{BenchmarkHelper, ObservationKind, Observations};
    use frame_support::unsigned::ValidateUnsigned;
    use sp_runtime::transaction_validity::{InvalidTransaction, TransactionSource};

    let (mut ext, _) = new_test_ext(4, 0, 4, 4);
    ext.execute_with(|| {
        let evidence = RuntimeObserverBenchmarkHelper::setup_babe(4, 0, 0)
            .expect("valid retained BABE evidence");
        assert_eq!(Session::validators().len(), 4);
        let call = r3b_babe_unsigned_call(&evidence);
        assert!(
            <Runtime as ValidateUnsigned>::validate_unsigned(TransactionSource::Local, &call,)
                .is_ok()
        );
        assert!(<Runtime as ValidateUnsigned>::validate_unsigned(
            TransactionSource::InBlock,
            &call,
        )
        .is_ok());
        assert_eq!(
            <Runtime as ValidateUnsigned>::validate_unsigned(TransactionSource::External, &call,),
            Err(InvalidTransaction::Call.into())
        );
        assert_eq!(
            call.clone()
                .dispatch(RuntimeOrigin::root())
                .expect_err("root is not an unsigned BABE origin")
                .error,
            DispatchError::BadOrigin
        );
        assert_ok!(<Runtime as ValidateUnsigned>::pre_dispatch(&call));
        assert_eq!(r3b_observer_event_count(), 0);
        assert_ok!(call.clone().dispatch(RuntimeOrigin::none()));
        let stored = Observations::<Runtime>::iter_values()
            .next()
            .expect("BABE record");
        assert_eq!(stored.kind, ObservationKind::Babe);
        assert_eq!(
            stored.proof_hash,
            sp_io::hashing::blake2_256(&evidence.0.encode())
        );
        assert_eq!(Session::validators().len(), 4);
        assert_eq!(r3b_observer_event_count(), 1);
        assert_eq!(
            <Runtime as ValidateUnsigned>::pre_dispatch(&call),
            Err(InvalidTransaction::Stale.into())
        );
    });
}

#[cfg(feature = "runtime-benchmarks")]
#[test]
fn r3b_signed_root_sudo_and_sudo_as_reject_both_consensus_reporters() {
    use era_validator_security::observation::BenchmarkHelper;

    let (mut babe_ext, _) = new_test_ext(4, 0, 4, 4);
    babe_ext.execute_with(|| {
        let evidence = RuntimeObserverBenchmarkHelper::setup_babe(4, 0, 0).expect("BABE evidence");
        let signed = RuntimeCall::Babe(pallet_babe::Call::report_equivocation {
            equivocation_proof: Box::new(evidence.0),
            key_owner_proof: evidence.1,
        });
        assert_eq!(
            signed
                .clone()
                .dispatch(RuntimeOrigin::signed(account(1)))
                .expect_err("signed BABE observer intake")
                .error,
            era_validator_security::observation::Error::<Runtime>::SignedIntakeRejected.into()
        );
        assert_eq!(
            signed
                .clone()
                .dispatch(RuntimeOrigin::root())
                .expect_err("root is not a signed BABE reporter")
                .error,
            DispatchError::BadOrigin
        );
        pallet_sudo::Key::<Runtime>::put(account(1));
        assert_ok!(RuntimeCall::Sudo(pallet_sudo::Call::sudo {
            call: Box::new(signed.clone()),
        })
        .dispatch(RuntimeOrigin::signed(account(1))));
        assert_ok!(RuntimeCall::Sudo(pallet_sudo::Call::sudo_as {
            who: MultiAddress::Id(account(2)),
            call: Box::new(signed),
        })
        .dispatch(RuntimeOrigin::signed(account(1))));
        assert_eq!(
            era_validator_security::observation::ObservationCount::<Runtime>::get(),
            0
        );
        assert_eq!(r3b_observer_event_count(), 0);
    });

    let (mut grandpa_ext, _) = new_test_ext(4, 0, 4, 4);
    grandpa_ext.execute_with(|| {
        let evidence =
            RuntimeObserverBenchmarkHelper::setup_grandpa(4, 0, 0).expect("GRANDPA evidence");
        let signed = RuntimeCall::Grandpa(pallet_grandpa::Call::report_equivocation {
            equivocation_proof: Box::new(evidence.0),
            key_owner_proof: evidence.1,
        });
        assert_eq!(
            signed
                .clone()
                .dispatch(RuntimeOrigin::signed(account(1)))
                .expect_err("signed GRANDPA observer intake")
                .error,
            era_validator_security::observation::Error::<Runtime>::SignedIntakeRejected.into()
        );
        assert_eq!(
            signed
                .clone()
                .dispatch(RuntimeOrigin::root())
                .expect_err("root is not a signed GRANDPA reporter")
                .error,
            DispatchError::BadOrigin
        );
        pallet_sudo::Key::<Runtime>::put(account(1));
        assert_ok!(RuntimeCall::Sudo(pallet_sudo::Call::sudo {
            call: Box::new(signed.clone()),
        })
        .dispatch(RuntimeOrigin::signed(account(1))));
        assert_ok!(RuntimeCall::Sudo(pallet_sudo::Call::sudo_as {
            who: MultiAddress::Id(account(2)),
            call: Box::new(signed),
        })
        .dispatch(RuntimeOrigin::signed(account(1))));
        assert_eq!(
            era_validator_security::observation::ObservationCount::<Runtime>::get(),
            0
        );
        assert_eq!(r3b_observer_event_count(), 0);
    });
}

#[cfg(feature = "runtime-benchmarks")]
#[test]
fn r3b_runtime_api_bodies_generate_proofs_submit_and_decode_unchanged_calls() {
    use era_validator_security::observation::BenchmarkHelper;
    use frame_support::traits::KeyOwnerProofSystem;
    use sp_core::offchain::{testing::TestTransactionPoolExt, TransactionPoolExt};

    let (pool, pool_state) = TestTransactionPoolExt::new();
    let (mut babe_ext, _) = new_test_ext(4, 0, 4, 4);
    babe_ext.register_extension(TransactionPoolExt::new(pool));
    babe_ext.execute_with(|| {
        let evidence = RuntimeObserverBenchmarkHelper::setup_babe(4, 0, 0)
            .expect("BABE API fixture");
        let opaque = <Runtime as sp_consensus_babe::runtime_decl_for_babe_api::BabeApi<Block>>::generate_key_ownership_proof(
            evidence.0.slot,
            evidence.0.offender.clone(),
        )
        .expect("BABE runtime API generated proof");
        assert_eq!(
            <Runtime as sp_consensus_babe::runtime_decl_for_babe_api::BabeApi<Block>>::submit_report_equivocation_unsigned_extrinsic(
                    evidence.0.clone(),
                    opaque,
                ),
            Some(())
        );
        let encoded = pool_state
            .write()
            .transactions
            .pop()
            .expect("BABE submitted opaque extrinsic");
        assert!(pool_state.read().transactions.is_empty());
        let submitted = UncheckedExtrinsic::decode(&mut encoded.as_slice())
            .expect("BABE opaque extrinsic decodes");
        match submitted.function {
            RuntimeCall::Babe(pallet_babe::Call::report_equivocation_unsigned {
                equivocation_proof,
                key_owner_proof,
            }) => {
                assert_eq!(*equivocation_proof, evidence.0);
                assert_eq!(
                    key_owner_proof.session,
                    pallet_session::CurrentIndex::<Runtime>::get(),
                );
                assert_eq!(
                    key_owner_proof.validator_count,
                    Session::validators().len() as u32,
                );
                assert!(Historical::check_proof(
                    (sp_consensus_babe::KEY_TYPE, evidence.0.offender.clone()),
                    key_owner_proof,
                )
                .is_some());
            }
            _ => panic!("BABE runtime API submitted the unchanged unsigned report call"),
        }
    });

    let (pool, pool_state) = TestTransactionPoolExt::new();
    let (mut grandpa_ext, _) = new_test_ext(4, 0, 4, 4);
    grandpa_ext.register_extension(TransactionPoolExt::new(pool));
    grandpa_ext.execute_with(|| {
        let evidence = RuntimeObserverBenchmarkHelper::setup_grandpa(4, 0, 0)
            .expect("GRANDPA API fixture");
        let opaque =
            <Runtime as sp_consensus_grandpa::runtime_decl_for_grandpa_api::GrandpaApi<Block>>::generate_key_ownership_proof(
                evidence.0.set_id(),
                evidence.0.offender().clone(),
            )
            .expect("GRANDPA runtime API generated proof");
        assert_eq!(
            <Runtime as sp_consensus_grandpa::runtime_decl_for_grandpa_api::GrandpaApi<Block>>::submit_report_equivocation_unsigned_extrinsic(
                    evidence.0.clone(),
                    opaque,
                ),
            Some(())
        );
        let encoded = pool_state
            .write()
            .transactions
            .pop()
            .expect("GRANDPA submitted opaque extrinsic");
        assert!(pool_state.read().transactions.is_empty());
        let submitted = UncheckedExtrinsic::decode(&mut encoded.as_slice())
            .expect("GRANDPA opaque extrinsic decodes");
        match submitted.function {
            RuntimeCall::Grandpa(pallet_grandpa::Call::report_equivocation_unsigned {
                equivocation_proof,
                key_owner_proof,
            }) => {
                assert_eq!(*equivocation_proof, evidence.0);
                assert_eq!(
                    key_owner_proof.session,
                    pallet_session::CurrentIndex::<Runtime>::get(),
                );
                assert_eq!(
                    key_owner_proof.validator_count,
                    Session::validators().len() as u32,
                );
                assert!(Historical::check_proof(
                    (sp_consensus_grandpa::KEY_TYPE, evidence.0.offender().clone()),
                    key_owner_proof,
                )
                .is_some());
            }
            _ => panic!("GRANDPA runtime API submitted the unchanged unsigned report call"),
        }
    });
}

#[cfg(feature = "runtime-benchmarks")]
#[test]
fn r3b_report_adapters_are_component_wise_maxima_of_all_reachable_branches() {
    use era_validator_security::observation::WeightInfo;
    type W = era_validator_security::weights::SubstrateWeight<Runtime>;

    let babe_expected = component_wise_weight_max([
        W::babe_new(100),
        W::babe_duplicate(100),
        W::babe_invalid(100),
        W::babe_capacity_full(100),
    ]);
    assert_eq!(
        <BabeObserveOnlyWeightAdapter<Runtime> as pallet_babe::WeightInfo>::report_equivocation(
            4, 0
        ),
        babe_expected,
    );

    let grandpa_expected = component_wise_weight_max([
        W::grandpa_new(100),
        W::grandpa_duplicate(100),
        W::grandpa_invalid(100),
        W::grandpa_capacity_full(100),
    ]);
    assert_eq!(
        <GrandpaObserveOnlyWeightAdapter<Runtime> as pallet_grandpa::WeightInfo>::report_equivocation(4, 0),
        grandpa_expected,
    );

    let normal = <<Runtime as frame_system::Config>::BlockWeights as Get<
        frame_system::limits::BlockWeights,
    >>::get()
    .get(frame_support::dispatch::DispatchClass::Normal)
    .clone();
    let max_report = component_wise_weight_max([
        <BabeObserveOnlyWeightAdapter<Runtime> as pallet_babe::WeightInfo>::report_equivocation(
            MaxBabeAuthorities::get(),
            0,
        ),
        <GrandpaObserveOnlyWeightAdapter<Runtime> as pallet_grandpa::WeightInfo>::report_equivocation(
            MaxGrandpaAuthorities::get(),
            0,
        ),
    ]);
    let max_extrinsic = normal.max_extrinsic.expect("Normal extrinsic capacity");
    let max_total = normal.max_total.expect("Normal block capacity");
    assert!(max_report.all_lte(max_extrinsic));
    let sixteen_reports = max_report
        .saturating_add(normal.base_extrinsic)
        .saturating_mul(16);
    assert!(sixteen_reports.all_lte(max_total));
    assert!(max_report.ref_time() > 0 && max_report.proof_size() > 0);
    assert!(sixteen_reports.ref_time() > 0 && sixteen_reports.proof_size() > 0);
}

#[path = "ws3_migration_tests.rs"]
mod ws3_migration_tests;

#[path = "v14_integrated_migration_tests.rs"]
mod v14_integrated_migration_tests;

#[path = "v14_asset_weight_tests.rs"]
mod v14_asset_weight_tests;

#[path = "v14_world_weight_tests.rs"]
mod v14_world_weight_tests;
#[test]
fn v14_external_candidate_four_to_seven_election_readiness() {
    // Synthetic candidates only: an election test, not independent operators or a network rehearsal.
    for desired in 4..=7 {
        let (mut ext,fixture)=new_test_ext_with_validator_bond(7,0,desired,4,10_000*DECIMALS);
        ext.execute_with(|| {
            set_stalled_live_shape();
            let elected=recover_next_era();
            assert_eq!(elected.len(),desired as usize);
            assert!(elected.iter().all(|who| fixture.validators.contains(who)));
            for who in elected {
                assert!(pallet_session::NextKeys::<Runtime>::contains_key(&who));
                let controller=pallet_staking::Bonded::<Runtime>::get(&who).unwrap();
                assert!(pallet_staking::Ledger::<Runtime>::get(controller).unwrap().active>=10_000*DECIMALS);
            }
        });
    }
}
