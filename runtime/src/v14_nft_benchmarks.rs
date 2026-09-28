//! Measurements of the local owning NFT helpers with the actual runtime bounds and currency.
//! Fixtures deliberately include independently funded payers; none are whitelisted.
use crate::{AccountId, Balance, Balances, Nfts, Runtime, RuntimeOrigin, System, DECIMALS};
use alloc::{vec, vec::Vec};
use core::marker::PhantomData;
use frame_benchmarking::v1::{account, benchmarks};
use frame_support::{
    assert_ok,
    traits::{Currency, ReservableCurrency, StorageVersion},
    BoundedVec,
};
pub use frame_system::Config;
use pallet_nfts::*;
use sp_runtime::MultiAddress;
pub struct Pallet<T: Config>(PhantomData<T>);

fn owner() -> AccountId {
    account("nft-retirement-owner", 0, 0)
}
fn delegate() -> AccountId {
    account("nft-retirement-delegate", 0, 0)
}
fn fund(who: &AccountId) {
    let _ = Balances::make_free_balance_be(who, 1_000_000 * DECIMALS);
}
fn setup(live: bool) -> AccountId {
    System::set_block_number(1);
    let who = owner();
    fund(&who);
    assert_ok!(Nfts::create(
        RuntimeOrigin::signed(who.clone()),
        MultiAddress::Id(who.clone()),
        Default::default()
    ));
    assert_ok!(Nfts::set_collection_metadata(
        RuntimeOrigin::signed(who.clone()),
        0,
        vec![255; crate::NftStringLimit::get() as usize]
            .try_into()
            .unwrap()
    ));
    assert_ok!(Nfts::set_team(
        RuntimeOrigin::signed(who.clone()),
        0,
        Some(MultiAddress::Id(who.clone())),
        Some(MultiAddress::Id(account("nft-team", 2, 0))),
        Some(MultiAddress::Id(account("nft-team", 3, 0)))
    ));
    if live {
        assert_ok!(Nfts::mint(
            RuntimeOrigin::signed(who.clone()),
            0,
            0,
            MultiAddress::Id(who.clone()),
            None
        ));
        Item::<Runtime>::mutate(0, 0, |item| {
            let item = item.as_mut().unwrap();
            for i in 0..crate::NftApprovalsLimit::get() {
                item.approvals
                    .try_insert(account("nft-transfer-approval", i, 0), Some(u32::MAX))
                    .unwrap();
            }
        });
        let mut approvals = ItemAttributesApprovals::<Runtime>::default();
        approvals.try_insert(delegate()).unwrap();
        for i in 1..crate::NftItemAttributesApprovalsLimit::get() {
            approvals
                .try_insert(account("nft-attribute-approval", i, 0))
                .unwrap();
        }
        ItemAttributesApprovalsOf::<Runtime>::insert(0, 0, approvals);
    }
    who
}
fn deposit(i: u32, sentinel: bool) -> Option<AccountId> {
    let payer = if sentinel {
        owner()
    } else {
        account("nft-independent-payer", i, 0)
    };
    if !sentinel {
        fund(&payer);
    }
    assert_ok!(Balances::reserve(&payer, DECIMALS));
    if sentinel {
        Collection::<Runtime>::mutate(0, |d| d.as_mut().unwrap().owner_deposit += DECIMALS);
        None
    } else {
        Some(payer)
    }
}
fn attribute(i: u32, sentinel: bool) {
    let payer = deposit(i + 10_000, sentinel);
    let mut key = vec![255u8; crate::NftKeyLimit::get() as usize];
    key[..4].copy_from_slice(&i.to_le_bytes());
    let key: BoundedVec<u8, crate::NftKeyLimit> = key.try_into().unwrap();
    let value: BoundedVec<u8, crate::NftValueLimit> =
        vec![255u8; crate::NftValueLimit::get() as usize]
            .try_into()
            .unwrap();
    Attribute::<Runtime>::insert(
        (
            0,
            Some(0),
            if sentinel {
                AttributeNamespace::CollectionOwner
            } else {
                AttributeNamespace::Account(delegate())
            },
            key,
        ),
        (
            value,
            AttributeDeposit {
                account: payer,
                amount: DECIMALS,
            },
        ),
    );
    Collection::<Runtime>::mutate(0, |d| d.as_mut().unwrap().attributes += 1);
}
fn metadata(i: u32, sentinel: bool) {
    let payer = deposit(i, sentinel);
    ItemMetadataOf::<Runtime>::insert(
        0,
        i,
        ItemMetadata::<_, crate::NftStringLimit> {
            deposit: ItemMetadataDeposit {
                account: payer,
                amount: DECIMALS,
            },
            data: vec![255; crate::NftStringLimit::get() as usize]
                .try_into()
                .unwrap(),
        },
    );
    Collection::<Runtime>::mutate(0, |d| d.as_mut().unwrap().item_metadatas += 1);
}
fn config(i: u32) {
    ItemConfigOf::<Runtime>::insert(
        0,
        i,
        ItemConfig {
            settings: ItemSettings::from_disabled(ItemSetting::UnlockedMetadata.into()),
        },
    );
    Collection::<Runtime>::mutate(0, |d| d.as_mut().unwrap().item_configs += 1);
}
fn phase(n: u32, p: u32, sentinel: bool) -> AccountId {
    let who = setup(false);
    // Isolate the chosen variable phase; fixed finalization is included in the timed path.
    let role_keys: Vec<_> = CollectionRoleOf::<Runtime>::iter_prefix(0)
        .map(|(a, _)| a)
        .collect();
    for a in role_keys {
        CollectionRoleOf::<Runtime>::remove(0, a);
    }
    for i in 0..n {
        match p {
            0 => metadata(i, sentinel),
            1 => attribute(i, sentinel),
            2 => config(i),
            3 => ItemPriceOf::<Runtime>::insert(
                0,
                i,
                (Balance::MAX, Some(account::<AccountId>("nft-price", i, 0))),
            ),
            4 => PendingSwapOf::<Runtime>::insert(
                0,
                i,
                PendingSwap {
                    desired_collection: u32::MAX,
                    desired_item: Some(u32::MAX),
                    price: Some(PriceWithDirection {
                        amount: Balance::MAX,
                        direction: PriceDirection::Send,
                    }),
                    deadline: u32::MAX,
                },
            ),
            5 => {
                let mut approvals = ItemAttributesApprovals::<Runtime>::default();
                for j in 0..crate::NftItemAttributesApprovalsLimit::get() {
                    approvals.try_insert(account("nft-approval", j, 0)).unwrap();
                }
                ItemAttributesApprovalsOf::<Runtime>::insert(0, i, approvals);
            }
            6 => CollectionRoleOf::<Runtime>::insert(
                0,
                account::<AccountId>("nft-role", i, 0),
                CollectionRoles::none(),
            ),
            7 => DelegateCleanup::<Runtime>::insert(
                0,
                (i, account::<AccountId>("nft-marker", i, 0)),
                CleanupProgress {
                    version: 1,
                    phase: 0,
                },
            ),
            _ => unreachable!(),
        }
    }
    assert_ok!(Nfts::do_start_collection_retirement(who.clone(), 0));
    who
}

benchmarks! {
    retirement_start {
        let who = setup(false);
    }: { Nfts::do_start_collection_retirement(who, 0)?; }
    verify { assert!(CollectionRetirement::<Runtime>::contains_key(0)); }

    retirement_phase_0 {
        let n in 0 .. CLEANUP_LIMIT;
        let p = 0u32;
        let who = phase(n, p, false);
    }: { Nfts::do_continue_collection_retirement(who, 0, n.max(1))?; }
    retirement_phase_1 {
        let n in 0 .. CLEANUP_LIMIT;
        let p = 1u32;
        let who = phase(n, p, false);
    }: { Nfts::do_continue_collection_retirement(who, 0, n.max(1))?; }
    retirement_phase_2 {
        let n in 0 .. CLEANUP_LIMIT;
        let p = 2u32;
        let who = phase(n, p, false);
    }: { Nfts::do_continue_collection_retirement(who, 0, n.max(1))?; }
    retirement_phase_3 {
        let n in 0 .. CLEANUP_LIMIT;
        let p = 3u32;
        let who = phase(n, p, false);
    }: { Nfts::do_continue_collection_retirement(who, 0, n.max(1))?; }
    retirement_phase_4 {
        let n in 0 .. CLEANUP_LIMIT;
        let p = 4u32;
        let who = phase(n, p, false);
    }: { Nfts::do_continue_collection_retirement(who, 0, n.max(1))?; }
    retirement_phase_5 {
        let n in 0 .. CLEANUP_LIMIT;
        let p = 5u32;
        let who = phase(n, p, false);
    }: { Nfts::do_continue_collection_retirement(who, 0, n.max(1))?; }
    retirement_phase_6 {
        let n in 0 .. CLEANUP_LIMIT;
        let p = 6u32;
        let who = phase(n, p, false);
    }: { Nfts::do_continue_collection_retirement(who, 0, n.max(1))?; }
    retirement_phase_7 {
        let n in 0 .. CLEANUP_LIMIT;
        let p = 7u32;
        let who = phase(n, p, false);
    }: { Nfts::do_continue_collection_retirement(who, 0, n.max(1))?; }
    verify { assert!(!Collection::<Runtime>::contains_key(0)); }

    retirement_owner_0 {
        let n in 0 .. CLEANUP_LIMIT;
        let p = 0u32;
        let who = phase(n, p, true);
    }: { Nfts::do_continue_collection_retirement(who, 0, n.max(1))?; }
    retirement_owner_1 {
        let n in 0 .. CLEANUP_LIMIT;
        let p = 1u32;
        let who = phase(n, p, true);
    }: { Nfts::do_continue_collection_retirement(who, 0, n.max(1))?; }
    verify { assert_eq!(Balances::reserved_balance(owner()), 0); }

    retirement_pending {
        let n in 1 .. CLEANUP_LIMIT;
        let who = phase(n + 1, 1, false);
    }: { Nfts::do_continue_collection_retirement(who, 0, n)?; }
    verify { assert_eq!(Collection::<Runtime>::get(0).unwrap().attributes, 1); }

    destroy_independent {
        let m in 0 .. CLEANUP_LIMIT;
        let c in 0 .. CLEANUP_LIMIT;
        let a in 0 .. CLEANUP_LIMIT;
        let who = setup(false);
        for i in 0..m { metadata(i, false); }
        for i in 0..c { config(i); }
        for i in 0..a { attribute(i, false); }
        let witness = Collection::<Runtime>::get(0).unwrap().destroy_witness();
    }: { Nfts::do_destroy_collection(0, witness, Some(who))?; }
    verify { assert!(!Collection::<Runtime>::contains_key(0)); }

    destroy_owner {
        let m in 0 .. CLEANUP_LIMIT;
        let c in 0 .. CLEANUP_LIMIT;
        let a in 0 .. CLEANUP_LIMIT;
        let who = setup(false);
        for i in 0..m { metadata(i, true); }
        for i in 0..c { config(i); }
        for i in 0..a { attribute(i, true); }
        let witness = Collection::<Runtime>::get(0).unwrap().destroy_witness();
    }: { Nfts::do_destroy_collection(0, witness, Some(who))?; }
    verify { assert_eq!(Balances::reserved_balance(owner()), 0); }

    cancel_complete {
        let n in 0 .. CLEANUP_LIMIT;
        let who = setup(true);
        for i in 0..n { attribute(i, false); }
    }: { Nfts::cancel_item_attributes_approval(RuntimeOrigin::signed(who), 0, 0, MultiAddress::Id(delegate()), CancelAttributesApprovalWitness { account_attributes: n })?; }
    verify { assert_eq!(Collection::<Runtime>::get(0).unwrap().attributes, 0); }

    delegate_complete {
        let n in 0 .. CLEANUP_LIMIT;
        let who = setup(true);
        for i in 0..n { attribute(i, false); }
        DelegateCleanup::<Runtime>::insert(0, (0, delegate()), CleanupProgress { version: 1, phase: 0 });
    }: { Nfts::do_continue_delegate_cleanup(who, 0, 0, delegate(), n.max(1))?; }
    verify { assert!(!DelegateCleanup::<Runtime>::contains_key(0, (0, delegate()))); }

    delegate_pending {
        let n in 1 .. CLEANUP_LIMIT;
        let who = setup(true);
        for i in 0..=n { attribute(i, false); }
    }: { Nfts::do_continue_delegate_cleanup(who, 0, 0, delegate(), n)?; }
    verify { assert_eq!(Collection::<Runtime>::get(0).unwrap().attributes, 1); }

    underwitness {
        let n in 0 .. CLEANUP_LIMIT;
        let who = setup(true);
        for i in 0..=n { attribute(i, false); }
    }: { assert!(Nfts::cancel_item_attributes_approval(RuntimeOrigin::signed(who), 0, 0, MultiAddress::Id(delegate()), CancelAttributesApprovalWitness { account_attributes: n }).is_err()); }
    verify { assert_eq!(Collection::<Runtime>::get(0).unwrap().attributes, n + 1); }

    rollback_deficit {
        let n in 1 .. CLEANUP_LIMIT;
        let who = phase(n, 1, true);
        let reserved = Balances::reserved_balance(&who);
        // All but the last refund can succeed. The enclosing helper must roll them back.
        assert_eq!(Balances::unreserve(&who, reserved - ((n - 1) as Balance * DECIMALS)), 0);
    }: { assert!(Nfts::do_continue_collection_retirement(who, 0, n).is_err()); }
    verify { assert_eq!(Collection::<Runtime>::get(0).unwrap().attributes, n); }

    malformed_last {
        let n in 1 .. CLEANUP_LIMIT;
        let who = phase(n, 1, false);
        let key = Attribute::<Runtime>::iter_keys().map(Attribute::<Runtime>::hashed_key_for).max().unwrap();
        sp_io::storage::set(&key, &[255]);
    }: { assert!(Nfts::do_continue_collection_retirement(who, 0, n).is_err()); }
    verify { assert_eq!(Collection::<Runtime>::get(0).unwrap().attributes, n); }

    oversize {
        let who = setup(false);
    }: {
        assert!(Nfts::do_destroy_collection(0, DestroyWitness { item_metadatas: CLEANUP_LIMIT + 1, item_configs: 0, attributes: 0 }, Some(who.clone())).is_err());
        assert!(Nfts::do_continue_collection_retirement(who.clone(), 0, CLEANUP_LIMIT + 1).is_err());
        assert!(Nfts::do_continue_delegate_cleanup(who, 0, 0, delegate(), CLEANUP_LIMIT + 1).is_err());
    }

    reconcile {
        let who = phase(0, 0, false);
        Collection::<Runtime>::mutate(0, |d| d.as_mut().unwrap().attributes = u32::MAX);
    }: { Nfts::do_continue_collection_retirement(who, 0, 1)?; }
    verify { assert!(!Collection::<Runtime>::contains_key(0)); }

    migration_install {
        StorageVersion::new(1).put::<Nfts>();
    }: { pallet_nfts::migration::v2::migrate::<Runtime, ()>()?; }
    verify { assert_eq!(StorageVersion::get::<Nfts>(), StorageVersion::new(2)); }

    migration_replay {
        StorageVersion::new(2).put::<Nfts>();
    }: { pallet_nfts::migration::v2::migrate::<Runtime, ()>()?; }

    migration_reject {
        StorageVersion::new(1).put::<Nfts>();
        DelegateCleanup::<Runtime>::insert(0, (0, delegate()), CleanupProgress { version: 1, phase: 0 });
    }: { assert!(pallet_nfts::migration::v2::migrate::<Runtime, ()>().is_err()); }
    delegate_races_0 {
        let p = 0u32;
        let who = setup(true);
        for i in 0..2 { attribute(i, false); }
        assert_ok!(Nfts::do_continue_delegate_cleanup(who.clone(), 0, 0, delegate(), 1));
        let target: AccountId = account("nft-new-owner", 0, 0); fund(&target);
    }: {
        match p {
            0 => {
                Nfts::transfer(RuntimeOrigin::signed(who), 0, 0, MultiAddress::Id(target.clone()))?;
                Nfts::do_continue_delegate_cleanup(target, 0, 0, delegate(), 1)?;
            }
            1 => {
                Nfts::burn(RuntimeOrigin::signed(who.clone()), 0, 0)?;
                Nfts::do_start_collection_retirement(who.clone(), 0)?;
                Nfts::do_continue_collection_retirement(who, 0, 5)?;
            }
            2 => { assert!(Nfts::approve_item_attributes(RuntimeOrigin::signed(who), 0, 0, MultiAddress::Id(delegate())).is_err()); }
            _ => { assert!(Nfts::set_attribute(RuntimeOrigin::signed(delegate()), 0, Some(0), AttributeNamespace::Account(delegate()), vec![9; 64].try_into().unwrap(), vec![9; 128].try_into().unwrap()).is_err()); }
        }
    }

    delegate_races_1 {
        let p = 1u32;
        let who = setup(true);
        for i in 0..2 { attribute(i, false); }
        assert_ok!(Nfts::do_continue_delegate_cleanup(who.clone(), 0, 0, delegate(), 1));
        let target: AccountId = account("nft-new-owner", 0, 0); fund(&target);
    }: {
        match p {
            0 => {
                Nfts::transfer(RuntimeOrigin::signed(who), 0, 0, MultiAddress::Id(target.clone()))?;
                Nfts::do_continue_delegate_cleanup(target, 0, 0, delegate(), 1)?;
            }
            1 => {
                Nfts::burn(RuntimeOrigin::signed(who.clone()), 0, 0)?;
                Nfts::do_start_collection_retirement(who.clone(), 0)?;
                Nfts::do_continue_collection_retirement(who, 0, 5)?;
            }
            2 => { assert!(Nfts::approve_item_attributes(RuntimeOrigin::signed(who), 0, 0, MultiAddress::Id(delegate())).is_err()); }
            _ => { assert!(Nfts::set_attribute(RuntimeOrigin::signed(delegate()), 0, Some(0), AttributeNamespace::Account(delegate()), vec![9; 64].try_into().unwrap(), vec![9; 128].try_into().unwrap()).is_err()); }
        }
    }

    delegate_races_2 {
        let p = 2u32;
        let who = setup(true);
        for i in 0..2 { attribute(i, false); }
        assert_ok!(Nfts::do_continue_delegate_cleanup(who.clone(), 0, 0, delegate(), 1));
        let target: AccountId = account("nft-new-owner", 0, 0); fund(&target);
    }: {
        match p {
            0 => {
                Nfts::transfer(RuntimeOrigin::signed(who), 0, 0, MultiAddress::Id(target.clone()))?;
                Nfts::do_continue_delegate_cleanup(target, 0, 0, delegate(), 1)?;
            }
            1 => {
                Nfts::burn(RuntimeOrigin::signed(who.clone()), 0, 0)?;
                Nfts::do_start_collection_retirement(who.clone(), 0)?;
                Nfts::do_continue_collection_retirement(who, 0, 5)?;
            }
            2 => { assert!(Nfts::approve_item_attributes(RuntimeOrigin::signed(who), 0, 0, MultiAddress::Id(delegate())).is_err()); }
            _ => { assert!(Nfts::set_attribute(RuntimeOrigin::signed(delegate()), 0, Some(0), AttributeNamespace::Account(delegate()), vec![9; 64].try_into().unwrap(), vec![9; 128].try_into().unwrap()).is_err()); }
        }
    }

    delegate_races_3 {
        let p = 3u32;
        let who = setup(true);
        for i in 0..2 { attribute(i, false); }
        assert_ok!(Nfts::do_continue_delegate_cleanup(who.clone(), 0, 0, delegate(), 1));
        let target: AccountId = account("nft-new-owner", 0, 0); fund(&target);
    }: {
        match p {
            0 => {
                Nfts::transfer(RuntimeOrigin::signed(who), 0, 0, MultiAddress::Id(target.clone()))?;
                Nfts::do_continue_delegate_cleanup(target, 0, 0, delegate(), 1)?;
            }
            1 => {
                Nfts::burn(RuntimeOrigin::signed(who.clone()), 0, 0)?;
                Nfts::do_start_collection_retirement(who.clone(), 0)?;
                Nfts::do_continue_collection_retirement(who, 0, 5)?;
            }
            2 => { assert!(Nfts::approve_item_attributes(RuntimeOrigin::signed(who), 0, 0, MultiAddress::Id(delegate())).is_err()); }
            _ => { assert!(Nfts::set_attribute(RuntimeOrigin::signed(delegate()), 0, Some(0), AttributeNamespace::Account(delegate()), vec![9; 64].try_into().unwrap(), vec![9; 128].try_into().unwrap()).is_err()); }
        }
    }

    destroy_underwitness_0 {
        let n in 0 .. CLEANUP_LIMIT;
        let p = 0u32;
        let who = setup(false);
        for i in 0..=n { match p { 0 => metadata(i, false), 1 => attribute(i, false), _ => config(i) } }
        Collection::<Runtime>::mutate(0, |d| { let d = d.as_mut().unwrap(); match p { 0 => d.item_metadatas = n, 1 => d.attributes = n, _ => d.item_configs = n } });
        let witness = Collection::<Runtime>::get(0).unwrap().destroy_witness();
    }: { assert!(Nfts::do_destroy_collection(0, witness, Some(who)).is_err()); }

    destroy_underwitness_1 {
        let n in 0 .. CLEANUP_LIMIT;
        let p = 1u32;
        let who = setup(false);
        for i in 0..=n { match p { 0 => metadata(i, false), 1 => attribute(i, false), _ => config(i) } }
        Collection::<Runtime>::mutate(0, |d| { let d = d.as_mut().unwrap(); match p { 0 => d.item_metadatas = n, 1 => d.attributes = n, _ => d.item_configs = n } });
        let witness = Collection::<Runtime>::get(0).unwrap().destroy_witness();
    }: { assert!(Nfts::do_destroy_collection(0, witness, Some(who)).is_err()); }

    destroy_underwitness_2 {
        let n in 0 .. CLEANUP_LIMIT;
        let p = 2u32;
        let who = setup(false);
        for i in 0..=n { match p { 0 => metadata(i, false), 1 => attribute(i, false), _ => config(i) } }
        Collection::<Runtime>::mutate(0, |d| { let d = d.as_mut().unwrap(); match p { 0 => d.item_metadatas = n, 1 => d.attributes = n, _ => d.item_configs = n } });
        let witness = Collection::<Runtime>::get(0).unwrap().destroy_witness();
    }: { assert!(Nfts::do_destroy_collection(0, witness, Some(who)).is_err()); }

    migration_absent {
        sp_io::storage::clear(&StorageVersion::storage_key::<Nfts>());
    }: { pallet_nfts::migration::v2::migrate::<Runtime, ()>()?; }
    verify { assert_eq!(StorageVersion::get::<Nfts>(), StorageVersion::new(2)); }

    guard_team {
        let who = setup(false);
        let two: AccountId = account("nft-team", 2, 0);
        let three: AccountId = account("nft-team", 3, 0);
        Nfts::set_team(RuntimeOrigin::signed(who.clone()), 0, Some(MultiAddress::Id(who.clone())), Some(MultiAddress::Id(two.clone())), Some(MultiAddress::Id(three.clone())))?;
    }: {
        Nfts::set_team(RuntimeOrigin::signed(who.clone()), 0, Some(MultiAddress::Id(three)), Some(MultiAddress::Id(who)), Some(MultiAddress::Id(two)))?;
    }

    invalid_entitlement {
        let who = phase(1, 1, false);
        let key = Attribute::<Runtime>::iter_keys().next().unwrap();
        Attribute::<Runtime>::mutate(key, |a| a.as_mut().unwrap().1.account = None);
    }: { assert!(Nfts::do_continue_collection_retirement(who, 0, 1).is_err()); }

    malformed_oversized {
        let who = phase(1, 1, false);
        let key = Attribute::<Runtime>::hashed_key_for(Attribute::<Runtime>::iter_keys().next().unwrap());
        sp_io::storage::set(&key, &vec![255; 4096]);
    }: { assert!(Nfts::do_continue_collection_retirement(who, 0, 1).is_err()); }

    destroy_rollback {
        let m in 0 .. CLEANUP_LIMIT;
        let c in 0 .. CLEANUP_LIMIT;
        let a in 0 .. CLEANUP_LIMIT;
        let who = setup(false);
        for i in 0..m { metadata(i, false); }
        for i in 0..c { config(i); }
        for i in 0..a { attribute(i, false); }
        let witness = Collection::<Runtime>::get(0).unwrap().destroy_witness();
        let reserved = Balances::reserved_balance(&who);
        assert_eq!(Balances::unreserve(&who, reserved), 0);
    }: { assert!(Nfts::do_destroy_collection(0, witness, Some(who)).is_err()); }
    verify { assert!(Collection::<Runtime>::contains_key(0)); }

    burn_owner {
        let who = setup(true);
        metadata(0, true);
    }: { Nfts::burn(RuntimeOrigin::signed(who), 0, 0)?; }
    verify { assert!(!Item::<Runtime>::contains_key(0, 0)); }

    burn_independent {
        let who = setup(true);
        metadata(0, false);
    }: { Nfts::burn(RuntimeOrigin::signed(who), 0, 0)?; }
    verify { assert!(!Item::<Runtime>::contains_key(0, 0)); }

    burn_rollback {
        let who = setup(true);
        metadata(0, false);
        let payer: AccountId = account("nft-independent-payer", 0, 0);
        assert_eq!(Balances::unreserve(&payer, DECIMALS), 0);
    }: { assert!(Nfts::burn(RuntimeOrigin::signed(who), 0, 0).is_err()); }
    verify { assert!(Item::<Runtime>::contains_key(0, 0)); }

}
