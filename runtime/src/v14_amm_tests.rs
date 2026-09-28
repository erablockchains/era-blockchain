//! Synthetic AMM integration through the actual committed V14-4 transfer adapter.
//! This module is cfg(test) only: it adds no pallet/call/API to the ERA runtime.
use crate::v14_assets::{AdmissionLookup, ContractFungibles, SdkReader};
use crate::{
    AccountId, Assets, Balances, Runtime, RuntimeOrigin as EraOrigin, System as EraSystem,
    DECIMALS, EXISTENTIAL_DEPOSIT,
};
use codec::Encode;
use core::any::TypeId;
use era_v14_amm::{
    self as kernel, mock,
    v1::{self as contract, Asset, EraV14AmmApiV1},
    AssetKey, Backend, Fault,
};
use era_v14_application_primitives::{
    amm,
    assets::{
        self,
        v1::{self, EraV14AssetsApiV1},
        FungibleAsset, FungibleInspect, FungibleTransfer,
    },
};
use frame_support::{
    assert_ok, construct_runtime, derive_impl,
    traits::{
        tokens::{fungible, fungibles, Fortitude, Preservation},
        PalletInfo as PalletInfoTrait, ReservableCurrency,
    },
};
use sp_runtime::{traits::AccountIdConversion, BuildStorage, StateVersion};

construct_runtime!(pub enum Fixture { System: frame_system, Amm: kernel });
/// Keep fixture System storage completely separate from the real runtime's System and events.
pub struct FixtureInfo;
impl PalletInfoTrait for FixtureInfo {
    fn index<P: 'static>() -> Option<usize> {
        PalletInfo::index::<P>()
    }
    fn name<P: 'static>() -> Option<&'static str> {
        if TypeId::of::<P>() == TypeId::of::<System>() {
            Some("SyntheticAmmSystem")
        } else if TypeId::of::<P>() == TypeId::of::<Amm>() {
            Some(contract::STORAGE_PREFIX)
        } else {
            PalletInfo::name::<P>()
        }
    }
    fn name_hash<P: 'static>() -> Option<[u8; 16]> {
        Self::name::<P>().map(|n| sp_io::hashing::twox_128(n.as_bytes()))
    }
    fn module_name<P: 'static>() -> Option<&'static str> {
        PalletInfo::module_name::<P>()
    }
    fn crate_version<P: 'static>() -> Option<frame_support::traits::CrateVersion> {
        PalletInfo::crate_version::<P>()
    }
}
#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Fixture {
    type Block = frame_system::mocking::MockBlock<Self>;
    type AccountId = AccountId;
    type Lookup = sp_runtime::traits::IdentityLookup<AccountId>;
    type PalletInfo = FixtureInfo;
}
pub struct Admissions;
impl AdmissionLookup for Admissions {
    const CONFIGURED: bool = true;
    fn asset(id: u32) -> v1::ApiResult<v1::AssetAdmission> {
        if (1..=70).contains(&id) {
            Ok(v1::AssetAdmission {
                id,
                minimum_balance: admitted_minimum(id),
            })
        } else {
            Err(v1::AssetApiErrorV1::UnsupportedAsset)
        }
    }
    fn collection(_: u32) -> v1::ApiResult<()> {
        Err(v1::AssetApiErrorV1::UnsupportedAsset)
    }
}
#[derive(Default)]
pub struct Actual;
impl FungibleInspect<AccountId> for Actual {
    type AssetId = u32;
    fn balance(&self, a: FungibleAsset<u32>, who: &AccountId) -> u128 {
        ContractFungibles::<Admissions>::new().balance(a, who)
    }
    fn total_issuance(&self, a: FungibleAsset<u32>) -> Result<u128, assets::Error> {
        ContractFungibles::<Admissions>::new().total_issuance(a)
    }
}
impl FungibleTransfer<AccountId> for Actual {
    fn transfer(
        &mut self,
        a: FungibleAsset<u32>,
        from: &AccountId,
        to: &AccountId,
        amount: u128,
    ) -> Result<(), assets::Error> {
        ContractFungibles::<Admissions>::new().transfer(a, from, to, amount)?;
        // Deliberately fail after a real SDK transfer, so the enclosing AMM transaction is tested.
        let count = mock::get::<u32>(b"integration-transfers") + 1;
        mock::put(b"integration-transfers", count);
        if count == mock::get::<u32>(b"integration-fail-after") {
            return Err(assets::Error::BackendRejected);
        }
        Ok(())
    }
}
impl Backend<AccountId> for Actual {
    fn minimum(&self, a: FungibleAsset<u32>) -> Result<u128, assets::Error> {
        match a {
            FungibleAsset::NativeEtkn => Ok(EXISTENTIAL_DEPOSIT),
            FungibleAsset::Registered(id) => {
                let record =
                    SdkReader::<Admissions>::asset_v1(id).map_err(|error| match error {
                        v1::AssetApiErrorV1::NotFound => assets::Error::UnknownAsset,
                        v1::AssetApiErrorV1::UnsupportedAsset
                        | v1::AssetApiErrorV1::Unconfigured => assets::Error::UnsupportedAsset,
                        _ => assets::Error::AccountingInvariant,
                    })?;
                if record.status != v1::AssetStatusV1::Live {
                    return Err(assets::Error::Frozen);
                }
                Ok(record.minimum_balance)
            }
        }
    }
    fn reserved_native(&self, who: &AccountId) -> u128 {
        Balances::reserved_balance(who)
    }
    fn reserve_native(&mut self, who: &AccountId, amount: u128) -> Result<(), assets::Error> {
        Balances::reserve(who, amount).map_err(|_| assets::Error::BackendRejected)
    }
    fn identity_conflict(&self, who: &AccountId) -> bool {
        // These are bounded protocol/authority lookups, not a scan or a balance/liveness ban.
        let protocol = [
            crate::AiPredictionsPalletId::get(),
            crate::RewardReservePalletId::get(),
            crate::EcosystemTreasuryPalletId::get(),
            crate::FeeCollectionPalletId::get(),
            crate::SecurityBudgetPalletId::get(),
        ];
        protocol.into_iter().any(|id| {
            let account: AccountId = id.into_account_truncating();
            account == *who
        }) || EraSystem::account(who).nonce != 0
            || pallet_sudo::Key::<Runtime>::get().as_ref() == Some(who)
            || pallet_staking::Bonded::<Runtime>::contains_key(who)
            || pallet_staking::Ledger::<Runtime>::contains_key(who)
            || pallet_session::NextKeys::<Runtime>::contains_key(who)
            || !pallet_proxy::Proxies::<Runtime>::get(who).0.is_empty()
    }
    fn providers(&self, who: &AccountId) -> u32 {
        EraSystem::providers(who)
    }
    fn reducible(&self, asset: FungibleAsset<u32>, who: &AccountId) -> u128 {
        match asset {
            FungibleAsset::NativeEtkn => {
                <Balances as fungible::Inspect<AccountId>>::reducible_balance(
                    who,
                    Preservation::Preserve,
                    Fortitude::Polite,
                )
            }
            FungibleAsset::Registered(id) => {
                <Assets as fungibles::Inspect<AccountId>>::reducible_balance(
                    id,
                    who,
                    Preservation::Preserve,
                    Fortitude::Polite,
                )
            }
        }
    }
    fn establish(&mut self, who: &AccountId) -> Result<(), assets::Error> {
        let before = EraSystem::account(who);
        let next = before
            .providers
            .checked_add(1)
            .ok_or(assets::Error::ArithmeticOverflow)?;
        if !mock::get::<bool>(b"integration-skip-provider") {
            EraSystem::inc_providers(who);
        }
        let after = EraSystem::account(who);
        let mut expected = before;
        expected.providers = next;
        if after != expected {
            return Err(assets::Error::AccountingInvariant);
        }
        Ok(())
    }
}
fn admitted_minimum(id: u32) -> u128 {
    let value: u128 = mock::get(&(b"admitted-minimum", id).encode());
    if value == 0 {
        1
    } else {
        value
    }
}
pub struct PairRegistry;
impl kernel::Admission for PairRegistry {
    fn ensure(pair: amm::PoolId<u32>) -> kernel::Result<()> {
        if !mock::get::<bool>(b"synthetic-admitted") {
            return kernel::EmptyPairs::ensure(pair);
        }
        for a in [pair.asset_0, pair.asset_1] {
            if matches!(a, FungibleAsset::Registered(id) if !(1..=70).contains(&id)) {
                return Err(assets::Error::UnsupportedAsset.into());
            }
        }
        Ok(())
    }
}
pub struct EncodedNotices;
impl kernel::EventSink<AccountId> for EncodedNotices {
    fn emit(event: amm::Event<AccountId, u32>) -> kernel::Result<()> {
        let encoded = contract::EventV1::try_from(event.clone())
            .map_err(|_| Fault::CorruptState)?
            .encode();
        let mut events: Vec<Vec<u8>> = mock::get(b"adopted-amm-events");
        events.push(encoded);
        mock::put(b"adopted-amm-events", events);
        <mock::Notices as kernel::EventSink<AccountId>>::emit(event)
    }
}
pub struct Clock;
impl frame_support::traits::Get<u64> for Clock {
    fn get() -> u64 {
        EraSystem::block_number() as u64
    }
}
impl kernel::Config for Fixture {
    type Asset = Asset;
    type Record = contract::PoolRecord;
    type PoolHasher = frame_support::Blake2_128Concat;
    type AccountHasher = frame_support::Blake2_128Concat;
    type Assets = Actual;
    type Admission = PairRegistry;
    type Creator = contract::SignedCreator;
    type Custody = contract::CheckedCustody;
    type Events = EncodedNotices;
    type Math = kernel::WideArithmetic;
    type Clock = Clock;
    type Fee = contract::Fee;
    type FeeLimit = contract::FeeLimit;
    type LockedLiquidity = contract::LockedLiquidity;
    type MinimumPosition = contract::MinimumPosition;
    type MinimumTrade = contract::MinimumTrade;
    type CreationDeposit = contract::CreationDeposit;
    type Horizon = contract::Horizon;
    type MaxPools = contract::MaxPools;
    type MaxPositions = contract::MaxPositions;
    type MaxProviders = contract::MaxProviders;
    type MaxPage = contract::MaxPage;
}
fn account(id: u8) -> AccountId {
    mock::account(id)
}
fn ext() -> sp_io::TestExternalities {
    let mut storage = frame_system::GenesisConfig::<Runtime>::default()
        .build_storage()
        .unwrap();
    pallet_balances::GenesisConfig::<Runtime> {
        balances: (1..=5).map(|i| (account(i), 1000 * DECIMALS)).collect(),
        dev_accounts: None,
    }
    .assimilate_storage(&mut storage)
    .unwrap();
    let mut ext = sp_io::TestExternalities::new(storage);
    ext.execute_with(|| {
        EraSystem::set_block_number(100);
        mock::put(b"synthetic-admitted", true);
        for id in 1..=3 {
            assert_ok!(Assets::create(
                EraOrigin::signed(account(1)),
                id,
                sp_runtime::MultiAddress::Id(account(1)),
                1
            ));
            for who in 1..=3 {
                assert_ok!(Assets::mint(
                    EraOrigin::signed(account(1)),
                    id,
                    sp_runtime::MultiAddress::Id(account(who)),
                    100 * DECIMALS
                ));
            }
        }
    });
    ext
}
fn root() -> Vec<u8> {
    sp_io::storage::root(StateVersion::V1)
}
fn initialized(pair: (Asset, Asset)) {
    assert_ok!(Amm::create(&account(1), pair.0, pair.1, 100));
    assert_ok!(Amm::add(
        &account(1),
        pair,
        [DECIMALS, DECIMALS],
        [0, 0],
        100
    ));
}
#[test]
fn frozen_sdk_native_and_registered_swaps_match_quotes_and_conserve_both_supplies() {
    ext().execute_with(|| {
        let native = Balances::total_issuance();
        let supplies = (1..=3).map(Assets::total_supply).collect::<Vec<_>>();
        for pair in [
            (Asset::Native, Asset::Registered(1)),
            (Asset::Registered(2), Asset::Registered(3)),
        ] {
            initialized(pair);
            let p = Amm::pool(pair).unwrap();
            assert!(EraSystem::providers(&p.custody) > 0);
            for (assets, exact) in [(pair, false), ((pair.1, pair.0), true)] {
                let amount = DECIMALS / 1000;
                let quote = Amm::quote(assets, amount, exact).unwrap();
                let result = Amm::swap(
                    &account(2),
                    assets,
                    amount,
                    if exact { quote.0 } else { quote.1 },
                    &account(3),
                    120,
                    exact,
                )
                .unwrap();
                assert_eq!(
                    (result.amount_in, result.amount_out, result.total_fee),
                    quote
                );
                assert_eq!(result.protocol_fee, 0);
                assert_ok!(Amm::reconcile(pair));
            }
        }
        assert_eq!(Balances::total_issuance(), native);
        assert_eq!(
            (1..=3).map(Assets::total_supply).collect::<Vec<_>>(),
            supplies
        );
        assert!(EraSystem::events()
            .iter()
            .any(|e| matches!(e.event, crate::RuntimeEvent::Assets(_))));
        assert!(System::events().is_empty());
    });
}
#[test]
fn second_actual_sdk_transfer_failure_restores_assets_deposit_lp_and_all_events() {
    ext().execute_with(|| {
        let pair = (Asset::Native, Asset::Registered(1));
        initialized(pair);
        mock::put(
            b"integration-fail-after",
            mock::get::<u32>(b"integration-transfers") + 2,
        );
        let before = root();
        assert_eq!(
            Amm::add(
                &account(2),
                pair,
                [DECIMALS / 100, DECIMALS / 100],
                [0, 0],
                100
            ),
            Err(assets::Error::BackendRejected.into())
        );
        assert_eq!(root(), before);
        let reserved = Balances::reserved_balance(account(1));
        mock::put(b"synthetic-fail-event", true);
        let before = root();
        assert_eq!(
            Amm::create(&account(1), Asset::Registered(2), Asset::Registered(3), 100),
            Err(Fault::CorruptState)
        );
        assert_eq!(root(), before);
        assert_eq!(Balances::reserved_balance(account(1)), reserved);
    });
}
#[test]
fn frozen_sdk_minima_and_unvested_locks_reject_atomically() {
    use frame_support::traits::VestingSchedule;
    ext().execute_with(|| {
        let pair = (Asset::Native, Asset::Registered(1));
        initialized(pair);
        // Adopted locked LP=1000 base units. Its residual ETKN is below the existing runtime ED.
        let all = Amm::lp(pair, &account(1)).unwrap();
        let before = root();
        assert_eq!(
            Amm::remove(&account(1), pair, all, [0, 0], &account(1), 100),
            Err(assets::Error::BelowMinimum.into())
        );
        assert_eq!(root(), before);
        assert_ok!(crate::Vesting::add_vesting_schedule(
            &account(2),
            1000 * DECIMALS,
            DECIMALS,
            1000
        ));
        let before = root();
        assert_eq!(
            Amm::add(
                &account(2),
                pair,
                [DECIMALS / 100, DECIMALS / 100],
                [0, 0],
                100
            ),
            Err(assets::Error::Frozen.into())
        );
        assert_eq!(root(), before);
    });
}
#[test]
fn adopted_widening_preserves_the_reference_boundary_as_a_differential_characterization() {
    use kernel::Arithmetic;
    assert_eq!(
        kernel::ReferenceArithmetic::add([0, 0], 0, [20 * DECIMALS; 2], 1000),
        Err(amm::Error::ArithmeticOverflow.into())
    );
    assert_eq!(
        kernel::WideArithmetic::add([0, 0], 0, [20 * DECIMALS; 2], 1000),
        Ok(([20 * DECIMALS; 2], 20 * DECIMALS - 1000, 1000))
    );
}
#[test]
fn frozen_admissions_conflicting_authority_and_freeze_remain_fail_closed() {
    ext().execute_with(|| {
        assert_eq!(
            crate::v14_assets::DormantFungibles::new().total_issuance(FungibleAsset::Registered(1)),
            Err(assets::Error::UnsupportedAsset)
        );
        let pair = (Asset::Native, Asset::Registered(1));
        use amm::PoolAccountDeriver;
        let custody = contract::CheckedCustody
            .derive_pool_account(
                &amm::PoolId::new(pair.0.fungible().unwrap(), pair.1.fungible().unwrap()).unwrap(),
            )
            .unwrap();
        pallet_sudo::Key::<Runtime>::put(&custody);
        let before = root();
        assert_eq!(
            Amm::create(&account(1), pair.0, pair.1, 100),
            Err(amm::Error::CustodyAccountCollision.into())
        );
        assert_eq!(root(), before);
        initialized((Asset::Registered(2), Asset::Registered(3)));
        assert_ok!(Assets::freeze_asset(EraOrigin::signed(account(1)), 2));
        let before = root();
        assert_eq!(
            Amm::quote((Asset::Registered(2), Asset::Registered(3)), 1000, false),
            Err(assets::Error::Frozen.into())
        );
        assert_eq!(root(), before);
    });
}

#[test]
fn adopted_widening_uses_actual_sdk_without_production_configuration() {
    ext().execute_with(|| {
        let native = Balances::total_issuance();
        let supply = Assets::total_supply(1);
        let pair = (Asset::Native, Asset::Registered(1));
        assert_ok!(Amm::create(&account(1), pair.0, pair.1, 100));
        assert_ok!(Amm::add(&account(1), pair, [20 * DECIMALS; 2], [0, 0], 100));
        let q = Amm::quote(pair, DECIMALS, false).unwrap();
        let swap = Amm::swap(&account(2), pair, DECIMALS, q.1, &account(3), 100, false).unwrap();
        assert_eq!((swap.amount_in, swap.amount_out, swap.total_fee), q);
        assert_ok!(Amm::reconcile(pair));
        assert_eq!(Balances::total_issuance(), native);
        assert_eq!(Assets::total_supply(1), supply);
        // Unfunded recipient of a non-sufficient registered asset cannot acquire a provider silently.
        let before = root();
        assert!(Amm::swap(
            &account(2),
            pair,
            DECIMALS / 100,
            0,
            &account(200),
            100,
            false
        )
        .is_err());
        assert_eq!(root(), before);
        assert_eq!(EraSystem::providers(&account(200)), 0);
    });
}

fn custody(pair: (Asset, Asset)) -> AccountId {
    use amm::PoolAccountDeriver;
    contract::CheckedCustody
        .derive_pool_account(
            &amm::PoolId::new(pair.0.fungible().unwrap(), pair.1.fungible().unwrap()).unwrap(),
        )
        .unwrap()
}
fn donate(asset: Asset, who: &AccountId, amount: u128) {
    assert_ok!(ContractFungibles::<Admissions>::new().transfer(
        asset.fungible().unwrap(),
        &account(3),
        who,
        amount
    ));
}
#[test]
fn prefunding_acquires_one_owned_provider_and_preserves_foreign_sdk_state() {
    use frame_support::traits::{fungible::MutateHold, LockableCurrency, WithdrawReasons};
    ext().execute_with(|| {
        for pair in [
            (Asset::Native, Asset::Registered(1)),
            (Asset::Registered(2), Asset::Registered(3)),
        ] {
            let who = custody(pair);
            donate(Asset::Native, &who, 5 * DECIMALS);
            donate(pair.1, &who, DECIMALS);
            if pair.0 != Asset::Native {
                donate(pair.0, &who, 2 * DECIMALS);
            }
            EraSystem::inc_providers(&who); // An unrelated owner retains its own reference.
            assert_ok!(Balances::reserve(&who, DECIMALS));
            let reason = crate::RuntimeHoldReason::Staking(pallet_staking::HoldReason::Staking);
            assert_ok!(<Balances as MutateHold<AccountId>>::hold(
                &reason, &who, DECIMALS
            ));
            Balances::set_lock(*b"foreign!", &who, DECIMALS, WithdrawReasons::all());
            let before = EraSystem::account(&who);
            let holds = pallet_balances::Holds::<Runtime>::get(&who);
            let locks = pallet_balances::Locks::<Runtime>::get(&who);
            let balances = [
                Actual.balance(pair.0.fungible().unwrap(), &who),
                Actual.balance(pair.1.fungible().unwrap(), &who),
            ];
            let issuance = (
                Balances::total_issuance(),
                Assets::total_supply(1),
                Assets::total_supply(2),
                Assets::total_supply(3),
            );
            let reserved = Balances::reserved_balance(account(1));
            assert_ok!(Amm::create(&account(1), pair.0, pair.1, 100));
            let mut expected = before;
            expected.providers += 1;
            assert_eq!(EraSystem::account(&who), expected);
            assert_eq!(pallet_balances::Holds::<Runtime>::get(&who), holds);
            assert_eq!(pallet_balances::Locks::<Runtime>::get(&who), locks);
            assert_eq!(
                Balances::reserved_balance(account(1)),
                reserved + 10 * DECIMALS
            );
            let pool = Amm::pool(pair).unwrap();
            assert_eq!(
                (pool.reserves, pool.total_lp, pool.user_lp, pool.locked_lp),
                ([0; 2], 0, 0, 0)
            );
            assert_eq!(Amm::surplus(pair).unwrap(), balances);
            let state = root();
            assert_eq!(
                Amm::create(&account(1), pair.0, pair.1, 100),
                Err(amm::Error::PoolAlreadyExists.into())
            );
            assert_eq!(root(), state);
            assert_ok!(Amm::add(&account(1), pair, [DECIMALS; 2], [0; 2], 100));
            assert_eq!(Amm::pool(pair).unwrap().locked_lp, 1000);
            assert_eq!(Amm::surplus(pair).unwrap(), balances);
            assert_eq!(pallet_balances::Holds::<Runtime>::get(&who), holds);
            assert_eq!(pallet_balances::Locks::<Runtime>::get(&who), locks);
            assert_eq!(EraSystem::providers(&who), expected.providers);
            assert_eq!(
                (
                    Balances::total_issuance(),
                    Assets::total_supply(1),
                    Assets::total_supply(2),
                    Assets::total_supply(3)
                ),
                issuance
            );
        }
    });
}
#[test]
fn provider_acquisition_failure_and_late_creation_failure_restore_prefunds_and_foreign_references()
{
    for mode in 0..3 {
        ext().execute_with(|| {
            let pair = (Asset::Native, Asset::Registered(1));
            let who = custody(pair);
            donate(Asset::Native, &who, DECIMALS);
            donate(pair.1, &who, 100);
            EraSystem::inc_providers(&who);
            match mode {
                0 => mock::put(b"integration-skip-provider", true),
                1 => mock::put(b"synthetic-fail-event", true),
                _ => frame_system::Account::<Runtime>::mutate(&who, |a| a.providers = u32::MAX),
            }
            let state = root();
            assert!(Amm::create(&account(1), pair.0, pair.1, 100).is_err());
            assert_eq!(root(), state);
            assert!(!kernel::Custodies::<Fixture>::contains_key(&who));
            assert!(!kernel::Pools::<Fixture>::contains_key(pair));
        });
    }
}
#[test]
fn donations_never_change_prices_or_lp_and_every_mutation_preserves_surplus() {
    ext().execute_with(|| {
        let issuance = (
            Balances::total_issuance(),
            Assets::total_supply(1),
            Assets::total_supply(2),
            Assets::total_supply(3),
        );
        for pair in [
            (Asset::Native, Asset::Registered(1)),
            (Asset::Registered(2), Asset::Registered(3)),
        ] {
            initialized(pair);
            let p = Amm::pool(pair).unwrap();
            let lp = Amm::lp(pair, &account(1)).unwrap();
            let q = [
                Amm::quote(pair, DECIMALS / 100, false).unwrap(),
                Amm::quote(pair, DECIMALS / 100, true).unwrap(),
            ];
            donate(pair.0, &p.custody, DECIMALS / 10);
            donate(pair.1, &p.custody, DECIMALS / 5);
            assert_eq!(Amm::pool(pair).unwrap(), p);
            assert_eq!(Amm::lp(pair, &account(1)).unwrap(), lp);
            assert_eq!(
                [
                    Amm::quote(pair, DECIMALS / 100, false).unwrap(),
                    Amm::quote(pair, DECIMALS / 100, true).unwrap()
                ],
                q
            );
            let surplus = [DECIMALS / 10, DECIMALS / 5];
            assert_eq!(Amm::surplus(pair).unwrap(), surplus);
            assert_ok!(Amm::add(&account(2), pair, [DECIMALS / 10; 2], [0; 2], 100));
            assert_eq!(Amm::surplus(pair).unwrap(), surplus);
            for exact in [false, true] {
                let q = Amm::quote(pair, DECIMALS / 100, exact).unwrap();
                assert_ok!(Amm::swap(
                    &account(2),
                    pair,
                    DECIMALS / 100,
                    if exact { q.0 } else { q.1 },
                    &account(3),
                    100,
                    exact
                ));
                assert_eq!(Amm::surplus(pair).unwrap(), surplus);
            }
            assert_ok!(Amm::remove(
                &account(2),
                pair,
                Amm::lp(pair, &account(2)).unwrap(),
                [0; 2],
                &account(2),
                100
            ));
            assert_eq!(Amm::surplus(pair).unwrap(), surplus);
            assert_ok!(Amm::reconcile(pair));
            mock::put(
                b"integration-fail-after",
                mock::get::<u32>(b"integration-transfers") + 2,
            );
            let state = root();
            assert!(Amm::add(&account(2), pair, [DECIMALS / 100; 2], [0; 2], 100).is_err());
            assert_eq!(root(), state);
            mock::put(b"integration-fail-after", 0u32);
            // Synthetic external damage, with no AMM repair path: spend all surplus plus one base unit.
            assert_ok!(ContractFungibles::<Admissions>::new().transfer(
                pair.1.fungible().unwrap(),
                &p.custody,
                &account(3),
                surplus[1] + 1
            ));
            let state = root();
            assert_eq!(Amm::pool(pair), Err(amm::Error::InvariantViolation.into()));
            assert_eq!(
                Amm::lp(pair, &account(1)),
                Err(amm::Error::InvariantViolation.into())
            );
            assert!(Amm::swap(
                &account(2),
                pair,
                DECIMALS / 100,
                0,
                &account(3),
                100,
                false
            )
            .is_err());
            assert_eq!(root(), state);
        }
        assert_eq!(
            (
                Balances::total_issuance(),
                Assets::total_supply(1),
                Assets::total_supply(2),
                Assets::total_supply(3)
            ),
            issuance
        );
    });
}
#[test]
fn first_exit_rejects_one_registered_unit_and_exact_native_floor_atomically() {
    for (amounts, prefund, succeeds) in [
        ([DECIMALS, 1], false, false),
        ([DECIMALS, 1], true, false),
        ([EXISTENTIAL_DEPOSIT; 2], false, false),
        ([EXISTENTIAL_DEPOSIT + 1; 2], false, true),
        ([EXISTENTIAL_DEPOSIT; 2], true, true),
    ] {
        ext().execute_with(|| {
            let pair = (Asset::Native, Asset::Registered(1));
            if prefund {
                donate(Asset::Native, &custody(pair), EXISTENTIAL_DEPOSIT);
                donate(pair.1, &custody(pair), 2);
            }
            assert_ok!(Amm::create(&account(1), pair.0, pair.1, 100));
            let state = root();
            let result = Amm::add(&account(1), pair, amounts, [0; 2], 100);
            if !succeeds {
                assert_eq!(result, Err(amm::Error::InsufficientInitialLiquidity.into()));
                assert_eq!(root(), state);
                assert_eq!(Amm::pool(pair).unwrap().locked_lp, 0);
            } else {
                assert_ok!(result);
                let pool = Amm::pool(pair).unwrap();
                let surplus = Amm::surplus(pair).unwrap();
                let (burn, outputs) = kernel::initial_exit::<kernel::WideArithmetic>(
                    pool.reserves,
                    pool.total_lp,
                    pool.user_lp,
                    surplus,
                    [EXISTENTIAL_DEPOSIT, 1],
                    1,
                )
                .unwrap();
                assert!(outputs.iter().all(|x| *x > 0));
                assert_ok!(Amm::remove(
                    &account(1),
                    pair,
                    burn,
                    outputs,
                    &account(1),
                    100
                ));
                assert_eq!(Amm::surplus(pair).unwrap(), surplus);
                assert_ok!(Amm::reconcile(pair));
            }
        });
    }
}
#[test]
fn first_exit_respects_registered_minimum_and_foreign_withdrawal_lock() {
    use frame_support::traits::{LockableCurrency, WithdrawReasons};
    for prefund in [false, true] {
        ext().execute_with(|| {
            mock::put(&(b"admitted-minimum", 4u32).encode(), 2u128);
            assert_ok!(Assets::create(
                EraOrigin::signed(account(1)),
                4,
                sp_runtime::MultiAddress::Id(account(1)),
                2
            ));
            for who in [1, 3] {
                assert_ok!(Assets::mint(
                    EraOrigin::signed(account(1)),
                    4,
                    sp_runtime::MultiAddress::Id(account(who)),
                    100
                ));
            }
            let pair = (Asset::Native, Asset::Registered(4));
            if prefund {
                donate(Asset::Native, &custody(pair), EXISTENTIAL_DEPOSIT);
                donate(pair.1, &custody(pair), 2);
            }
            assert_ok!(Amm::create(&account(1), pair.0, pair.1, 100));
            let state = root();
            let result = Amm::add(&account(1), pair, [DECIMALS, 2], [0; 2], 100);
            if !prefund {
                assert_eq!(result, Err(amm::Error::InsufficientInitialLiquidity.into()));
                assert_eq!(root(), state);
            } else {
                assert_ok!(result);
                let p = Amm::pool(pair).unwrap();
                let surplus = Amm::surplus(pair).unwrap();
                let (burn, out) = kernel::initial_exit::<kernel::WideArithmetic>(
                    p.reserves,
                    p.total_lp,
                    p.user_lp,
                    surplus,
                    [EXISTENTIAL_DEPOSIT, 2],
                    1,
                )
                .unwrap();
                assert_ok!(Amm::remove(&account(1), pair, burn, out, &account(1), 100));
                assert_eq!(Amm::surplus(pair).unwrap(), surplus);
            }
        });
    }
    ext().execute_with(|| {
        let pair = (Asset::Native, Asset::Registered(1));
        donate(Asset::Native, &custody(pair), DECIMALS);
        Balances::set_lock(
            *b"foreign!",
            &custody(pair),
            2 * DECIMALS,
            WithdrawReasons::all(),
        );
        assert_ok!(Amm::create(&account(1), pair.0, pair.1, 100));
        let state = root();
        assert_eq!(
            Amm::add(&account(1), pair, [DECIMALS; 2], [0; 2], 100),
            Err(amm::Error::InsufficientInitialLiquidity.into())
        );
        assert_eq!(root(), state);
    });
}
#[test]
fn dormant_signed_envelopes_and_sdk_outputs_use_adopted_codecs_without_runtime_exposure() {
    use contract::{AmmErrorV1 as E, Api, CallV1 as C};
    ext().execute_with(|| {
        let a = Asset::Native;
        let b = Asset::Registered(1);
        let pool = contract::PoolId::new(a, b).unwrap();
        let create = C::CreatePool {
            asset_a: a,
            asset_b: b,
            deadline: 100,
        };
        let state = root();
        for origin in [RuntimeOrigin::root(), RuntimeOrigin::none()] {
            assert_eq!(
                contract::execute_signed::<Fixture>(origin, create.clone()),
                Err(E::Unauthorized)
            );
            assert_eq!(root(), state);
        }
        contract::execute_signed::<Fixture>(
            RuntimeOrigin::signed(account(1)),
            contract::decode_exact(&create.encode()).unwrap(),
        )
        .unwrap();
        assert_eq!(
            Api::<Fixture>::lp_position_v1(pool, account(2).into())
                .unwrap()
                .lp,
            0
        );
        for call in [
            C::AddLiquidity {
                asset_a: a,
                asset_b: b,
                desired_a: DECIMALS,
                desired_b: DECIMALS,
                min_a: 0,
                min_b: 0,
                deadline: 100,
            },
            C::SwapExactInput {
                asset_in: a,
                asset_out: b,
                amount_in: DECIMALS / 100,
                min_out: 0,
                recipient: account(3).into(),
                deadline: 100,
            },
            C::SwapExactOutput {
                asset_in: b,
                asset_out: a,
                amount_out: DECIMALS / 100,
                max_in: DECIMALS,
                recipient: account(3).into(),
                deadline: 100,
            },
            C::RemoveLiquidity {
                asset_a: a,
                asset_b: b,
                lp: DECIMALS / 100,
                min_a: 0,
                min_b: 0,
                recipient: account(1).into(),
                deadline: 100,
            },
        ] {
            assert_ok!(contract::execute_signed::<Fixture>(
                RuntimeOrigin::signed(account(1)),
                contract::decode_exact(&call.encode()).unwrap()
            ));
        }
        let p = Api::<Fixture>::pool_v1(pool).unwrap();
        assert_eq!(
            contract::decode_exact::<contract::PoolV1>(&p.encode()).unwrap(),
            p
        );
        assert_eq!(
            kernel::Pools::<Fixture>::get(pool.pair()).unwrap().encode(),
            p.record.encode()
        );
        let mut expected_key = sp_io::hashing::twox_128(b"EraV14Amm").to_vec();
        expected_key.extend(sp_io::hashing::twox_128(b"Pools"));
        expected_key.extend(sp_io::hashing::blake2_128(&pool.encode()));
        expected_key.extend(pool.encode());
        assert_eq!(
            kernel::Pools::<Fixture>::hashed_key_for(pool.pair()),
            expected_key
        );
        assert_eq!(
            sp_io::storage::get(&expected_key).unwrap().as_ref(),
            p.record.encode()
        );

        let key = |name: &[u8], tails: Vec<Vec<u8>>| {
            let mut bytes = sp_io::hashing::twox_128(b"EraV14Amm").to_vec();
            bytes.extend(sp_io::hashing::twox_128(name));
            for tail in tails {
                bytes.extend(tail);
            }
            bytes
        };
        let blake = |bytes: Vec<u8>| [sp_io::hashing::blake2_128(&bytes).to_vec(), bytes].concat();
        let ordinal = 0u32.to_le_bytes().to_vec();
        let index_key = key(
            b"PoolIndex",
            vec![[sp_io::hashing::twox_64(&ordinal).to_vec(), ordinal].concat()],
        );
        assert_eq!(kernel::PoolIndex::<Fixture>::hashed_key_for(0), index_key);
        assert_eq!(
            sp_io::storage::get(&index_key).unwrap().as_ref(),
            pool.encode()
        );
        assert_eq!(
            kernel::PoolOrdinal::<Fixture>::hashed_key_for(pool.pair()),
            key(b"PoolOrdinal", vec![blake(pool.encode())])
        );
        assert_eq!(
            kernel::Custodies::<Fixture>::hashed_key_for(AccountId::from(p.record.custody)),
            key(b"Custodies", vec![blake(p.record.custody.to_vec())])
        );
        assert_eq!(
            kernel::Positions::<Fixture>::hashed_key_for(pool.pair(), account(1)),
            key(
                b"Positions",
                vec![blake(pool.encode()), blake(account(1).encode())]
            )
        );
        assert_eq!(
            kernel::AccountPools::<Fixture>::hashed_key_for(account(1)),
            key(b"AccountPools", vec![blake(account(1).encode())])
        );
        assert_eq!(
            kernel::Providers::<Fixture>::hashed_key_for(pool.pair()),
            key(b"Providers", vec![blake(pool.encode())])
        );
        assert_eq!(
            kernel::PoolCount::<Fixture>::hashed_key(),
            key(b"PoolCount", vec![]).as_slice()
        );
        assert!(!kernel::Busy::<Fixture>::exists());
        let q = Api::<Fixture>::quote_exact_input_v1(a, b, DECIMALS / 100).unwrap();
        let exact = Api::<Fixture>::quote_exact_output_v1(a, b, q.amount_out).unwrap();
        assert_eq!(q.protocol_fee, 0);
        assert_eq!(exact.protocol_fee, 0);
        assert_eq!(
            contract::decode_exact::<contract::QuoteV1>(&q.encode()).unwrap(),
            q
        );
        let events: Vec<Vec<u8>> = mock::get(b"adopted-amm-events");
        assert_eq!(
            events.iter().map(|e| e[0]).collect::<Vec<_>>(),
            [0, 1, 3, 3, 2]
        );
        for bytes in events {
            let e: contract::EventV1 = contract::decode_exact(&bytes).unwrap();
            assert_eq!(e.encode(), bytes);
        }
        mock::put(b"synthetic-admitted", false);
        assert_eq!(Api::<Fixture>::pool_v1(pool), Err(E::UnsupportedAsset));
        assert_eq!(
            contract::execute_signed::<Fixture>(RuntimeOrigin::signed(account(1)), create),
            Err(E::UnsupportedAsset)
        );
        assert!(contract::ADMITTED_PAIRS.is_empty());
        assert_eq!(crate::VERSION.spec_version, 15);
        assert_eq!(crate::VERSION.transaction_version, 1);
    });
}
#[test]
fn actual_sdk_pages_bound_64_results_and_positions_and_validate_cursors() {
    use contract::{AmmErrorV1 as E, Api};
    ext().execute_with(|| {
        for id in 4..=12 {
            assert_ok!(Assets::create(
                EraOrigin::signed(account(1)),
                id,
                sp_runtime::MultiAddress::Id(account(1)),
                1
            ));
            assert_ok!(Assets::mint(
                EraOrigin::signed(account(1)),
                id,
                sp_runtime::MultiAddress::Id(account(1)),
                100 * DECIMALS
            ));
        }
        let assets = core::iter::once(Asset::Native)
            .chain((1..=12).map(Asset::Registered))
            .collect::<Vec<_>>();
        let pairs = assets
            .iter()
            .enumerate()
            .flat_map(|(i, a)| assets[i + 1..].iter().map(move |b| (*a, *b)))
            .take(65)
            .collect::<Vec<_>>();
        for (i, pair) in pairs.iter().enumerate() {
            assert_ok!(Amm::create(&account(1), pair.0, pair.1, 100));
            if i < 64 {
                assert_ok!(Amm::add(
                    &account(1),
                    *pair,
                    [2 * EXISTENTIAL_DEPOSIT; 2],
                    [0; 2],
                    100
                ));
            }
        }
        let state = root();
        assert_eq!(
            Amm::add(
                &account(1),
                pairs[64],
                [2 * EXISTENTIAL_DEPOSIT; 2],
                [0; 2],
                100
            ),
            Err(Fault::Bound)
        );
        assert_eq!(root(), state);
        let page = Api::<Fixture>::pools_v1(None, 64).unwrap();
        assert_eq!(page.entries.len(), 64);
        assert_eq!(page.next.unwrap().pair(), pairs[63]);
        assert_eq!(
            contract::decode_exact::<contract::PageV1<contract::PoolV1>>(&page.encode()).unwrap(),
            page
        );
        let last = Api::<Fixture>::pools_v1(page.next, 64).unwrap();
        assert_eq!(last.entries.len(), 1);
        assert_eq!(last.entries[0].pool.pair(), pairs[64]);
        assert_eq!(last.next, None);
        let positions = Api::<Fixture>::positions_v1(account(1).into(), None, 64).unwrap();
        assert_eq!(positions.entries.len(), 64);
        assert_eq!(positions.next, None);
        assert_eq!(
            contract::decode_exact::<contract::PageV1<contract::LpPositionV1>>(&positions.encode())
                .unwrap(),
            positions
        );
        for limit in [0, 65, u32::MAX] {
            assert_eq!(Api::<Fixture>::pools_v1(None, limit), Err(E::InvalidLimit));
            assert_eq!(
                Api::<Fixture>::positions_v1(account(1).into(), None, limit),
                Err(E::InvalidLimit)
            );
        }
        assert_eq!(
            Api::<Fixture>::positions_v1(account(2).into(), page.next, 1),
            Err(E::InvalidCursor)
        );
        assert_eq!(
            Api::<Fixture>::pools_v1(
                Some(contract::PoolId {
                    asset_0: Asset::Registered(1),
                    asset_1: Asset::Native
                }),
                1
            ),
            Err(E::InvalidCursor)
        );
        assert_eq!(
            Api::<Fixture>::pools_v1(
                Some(contract::PoolId::new(Asset::Registered(69), Asset::Registered(70)).unwrap()),
                1
            ),
            Err(E::InvalidCursor)
        );
        assert_eq!(root(), state);
    });
}

#[test]
fn actual_sdk_identity_and_existing_lp_participant_collisions_reject_without_prefund_veto() {
    ext().execute_with(|| {
        for id in [
            crate::AiPredictionsPalletId::get(),
            crate::RewardReservePalletId::get(),
            crate::EcosystemTreasuryPalletId::get(),
            crate::FeeCollectionPalletId::get(),
            crate::SecurityBudgetPalletId::get(),
        ] {
            let who: AccountId = id.into_account_truncating();
            assert!(Actual.identity_conflict(&who));
        }
        let pair = (Asset::Native, Asset::Registered(1));
        let who = custody(pair);
        donate(Asset::Native, &who, DECIMALS);
        frame_system::Account::<Runtime>::mutate(&who, |info| info.nonce = 1);
        let before = root();
        assert_eq!(
            Amm::create(&account(1), pair.0, pair.1, 100),
            Err(amm::Error::CustodyAccountCollision.into())
        );
        assert_eq!(root(), before);
        frame_system::Account::<Runtime>::mutate(&who, |info| info.nonce = 0);
        let other = (Asset::Registered(2), Asset::Registered(3));
        initialized(other);
        donate(other.0, &who, DECIMALS);
        donate(other.1, &who, DECIMALS);
        Amm::add(&who, other, [DECIMALS / 100; 2], [0; 2], 100).unwrap();
        let before = root();
        assert_eq!(
            Amm::create(&account(1), pair.0, pair.1, 100),
            Err(amm::Error::CustodyAccountCollision.into())
        );
        assert_eq!(root(), before);
    });
}
