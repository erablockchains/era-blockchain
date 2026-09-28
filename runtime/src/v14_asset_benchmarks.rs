//! Runtime-local measurements of the pinned Assets implementation. No pallet is installed.
//! Setup uses signed creation and the real Runtime Config, including both 64-byte string bounds.
use crate::{AccountId, Assets, Balance, Balances, Runtime, RuntimeOrigin, System};
use alloc::vec;
use core::marker::PhantomData;
use frame_benchmarking::v1::{account, benchmarks};
use frame_support::traits::Currency;
use sp_runtime::MultiAddress;

pub use frame_system::Config;
pub struct Pallet<T: Config>(PhantomData<T>);

fn setup() -> AccountId {
    System::set_block_number(1);
    let owner: AccountId = account("metadata-owner", 0, 0);
    let _ = Balances::make_free_balance_be(&owner, 1_000_000 * crate::DECIMALS);
    Assets::create(
        RuntimeOrigin::signed(owner.clone()),
        1,
        MultiAddress::Id(owner.clone()),
        1,
    )
    .expect("real signed asset creation");
    owner
}
fn metadata(owner: &AccountId, n: u32, s: u32) {
    Assets::set_metadata(
        RuntimeOrigin::signed(owner.clone()),
        1,
        vec![1; n as usize],
        vec![2; s as usize],
        12,
    )
    .expect("metadata setup within actual limits");
}
fn verify_metadata(n: u32, s: u32) {
    let m = pallet_assets::Metadata::<Runtime>::get(1);
    assert_eq!(m.name.len(), n as usize);
    assert_eq!(m.symbol.len(), s as usize);
    assert_eq!(
        m.deposit,
        crate::AssetMetadataDepositBase::get()
            + Balance::from(n + s) * crate::AssetMetadataDepositPerByte::get()
    );
}

benchmarks! {
    set_metadata_create {
        let n in 0 .. 64;
        let s in 0 .. 64;
        let owner = setup();
        let name = vec![3; n as usize];
        let symbol = vec![4; s as usize];
    }: {
        Assets::set_metadata(RuntimeOrigin::signed(owner), 1, name, symbol, 12)?;
    } verify { verify_metadata(n, s); }

    set_metadata_replace {
        let n in 0 .. 64;
        let s in 0 .. 64;
        let owner = setup();
        metadata(&owner, 64, 64);
        let name = vec![3; n as usize];
        let symbol = vec![4; s as usize];
    }: {
        Assets::set_metadata(RuntimeOrigin::signed(owner), 1, name, symbol, 12)?;
    } verify { verify_metadata(n, s); }

    set_metadata_grow {
        let n in 0 .. 64;
        let s in 0 .. 64;
        let owner = setup();
        let old_total = (n + s).saturating_sub(1);
        metadata(&owner, old_total.min(64), old_total.saturating_sub(64));
        let name = vec![3; n as usize];
        let symbol = vec![4; s as usize];
    }: {
        Assets::set_metadata(RuntimeOrigin::signed(owner), 1, name, symbol, 12)?;
    } verify { verify_metadata(n, s); }

    clear_metadata {
        let owner = setup();
        metadata(&owner, 64, 64);
        let reserved = Balances::reserved_balance(&owner);
        let deposit = pallet_assets::Metadata::<Runtime>::get(1).deposit;
    }: {
        Assets::clear_metadata(RuntimeOrigin::signed(owner.clone()), 1)?;
    } verify {
        assert!(!pallet_assets::Metadata::<Runtime>::contains_key(1));
        assert_eq!(Balances::reserved_balance(&owner), reserved - deposit);
    }

    transfer_ownership {
        let owner = setup();
        metadata(&owner, 64, 64);
        let target: AccountId = account("metadata-target", 0, 0);
        let _ = Balances::make_free_balance_be(&target, 100 * crate::DECIMALS);
        let reserved = Balances::reserved_balance(&owner);
    }: {
        Assets::transfer_ownership(RuntimeOrigin::signed(owner.clone()), 1, MultiAddress::Id(target.clone()))?;
    } verify {
        assert_eq!(pallet_assets::Asset::<Runtime>::get(1).expect("asset").owner, target);
        assert_eq!(Balances::reserved_balance(&owner), 0);
        assert_eq!(Balances::reserved_balance(&target), reserved);
    }

    finish_destroy {
        let owner = setup();
        metadata(&owner, 64, 64);
        Assets::start_destroy(RuntimeOrigin::signed(owner.clone()), 1)?;
    }: {
        Assets::finish_destroy(RuntimeOrigin::signed(owner.clone()), 1)?;
    } verify {
        assert!(!pallet_assets::Asset::<Runtime>::contains_key(1));
        assert!(!pallet_assets::Metadata::<Runtime>::contains_key(1));
        assert_eq!(Balances::reserved_balance(&owner), 0);
    }

    // Rejected Vec inputs are still cloned by the pinned implementation before bound checks.
    // Cover the entire encoded block-length ceiling, conservatively including dispatch overhead.
    reject_name {
        let n in 65 .. 5_242_880;
        let owner = setup();
        let name = vec![3; n as usize];
        let symbol = vec![4; 64];
    }: {
        assert_eq!(Assets::set_metadata(RuntimeOrigin::signed(owner), 1, name, symbol, 12), Err(pallet_assets::Error::<Runtime>::BadMetadata.into()));
    } verify { assert!(!pallet_assets::Metadata::<Runtime>::contains_key(1)); }

    reject_symbol {
        let s in 65 .. 5_242_880;
        let owner = setup();
        let name = vec![3; 64];
        let symbol = vec![4; s as usize];
    }: {
        assert_eq!(Assets::set_metadata(RuntimeOrigin::signed(owner), 1, name, symbol, 12), Err(pallet_assets::Error::<Runtime>::BadMetadata.into()));
    } verify { assert!(!pallet_assets::Metadata::<Runtime>::contains_key(1)); }
}
