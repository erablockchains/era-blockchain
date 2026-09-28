//! Synthetic SDK characterization, not adoption of deposits, IDs, origins, or API policy.
use super::*;
use crate::{Nfts, Runtime, RuntimeCall, RuntimeOrigin, System, Vesting, DECIMALS};
use codec::{DecodeAll, Encode};
use frame_support::{
    assert_noop, assert_ok,
    traits::{Contains, VestingSchedule},
};
use sp_runtime::{traits::Dispatchable, BuildStorage, MultiAddress, StateVersion, TokenError};

fn account(id: u8) -> AccountId {
    AccountId::new([id; 32])
}
fn signed(id: u8) -> RuntimeOrigin {
    RuntimeOrigin::signed(account(id))
}
fn address(id: u8) -> MultiAddress<AccountId, ()> {
    MultiAddress::Id(account(id))
}
fn ext() -> sp_io::TestExternalities {
    let mut storage = frame_system::GenesisConfig::<Runtime>::default()
        .build_storage()
        .unwrap();
    pallet_balances::GenesisConfig::<Runtime> {
        balances: (1..=6).map(|id| (account(id), 1_000 * DECIMALS)).collect(),
        dev_accounts: None,
    }
    .assimilate_storage(&mut storage)
    .unwrap();
    let mut ext = sp_io::TestExternalities::new(storage);
    ext.execute_with(|| System::set_block_number(100));
    ext
}
fn root() -> Vec<u8> {
    sp_io::storage::root(StateVersion::V1)
}
fn create_asset(id: u32, minimum: Balance) {
    assert_ok!(Assets::create(signed(1), id, address(1), minimum));
}

#[test]
fn exact_transfers_separate_native_registered_zero_and_unknown_ids() {
    ext().execute_with(|| {
        // Zero is legal in the existing SDK, not a newly approved namespace choice.
        create_asset(0, 10);
        assert_ok!(Assets::mint(signed(1), 0, address(1), 100));
        let sdk = SdkFungibles;
        let native = Balances::total_issuance();
        assert_eq!(sdk.total_issuance(FungibleAsset::Registered(0)), Ok(100));
        assert_eq!(
            sdk.total_issuance(FungibleAsset::Registered(123)),
            Err(Error::UnknownAsset)
        );
        assert_ok!(sdk.transfer_exact(
            FungibleAsset::Registered(0),
            &account(1),
            &account(2),
            20,
            Preservation::Preserve
        ));
        assert_eq!(Assets::balance(0, account(1)), 80);
        assert_eq!(Assets::balance(0, account(2)), 20);
        assert_ok!(sdk.transfer_exact(
            FungibleAsset::NativeEtkn,
            &account(1),
            &account(2),
            DECIMALS,
            Preservation::Preserve
        ));
        assert_eq!(Assets::total_supply(0), 100);
        assert_eq!(Balances::total_issuance(), native);
        let before = root();
        assert_eq!(
            sdk.transfer_exact(
                FungibleAsset::Registered(123),
                &account(1),
                &account(2),
                0,
                Preservation::Preserve
            ),
            Err(TransferError::UnknownAsset)
        );
        assert_eq!(root(), before);
    });
}

#[test]
fn exact_adapter_rolls_back_backend_dust_and_minimum_failures() {
    ext().execute_with(|| {
        create_asset(7, 10);
        assert_ok!(Assets::mint(signed(1), 7, address(1), 100));
        let sdk = SdkFungibles;
        let asset = FungibleAsset::Registered(7);
        for amount in [1, 95, 101] {
            let before = root();
            assert!(sdk
                .transfer_exact(
                    asset,
                    &account(1),
                    &account(2),
                    amount,
                    Preservation::Preserve
                )
                .is_err());
            assert_eq!(root(), before);
        }
        // Expendable SDK paths can reap residual balances. Exact transfers must roll those back.
        let before = root();
        assert!(sdk
            .transfer_exact(
                asset,
                &account(1),
                &account(2),
                95,
                Preservation::Expendable
            )
            .is_err());
        assert_eq!(root(), before);
        let amount = Balances::free_balance(account(6)) - crate::ExistentialDeposit::get() / 2;
        let before = root();
        assert_eq!(
            sdk.transfer_exact(
                FungibleAsset::NativeEtkn,
                &account(6),
                &account(2),
                amount,
                Preservation::Expendable
            ),
            Err(TransferError::Conservation)
        );
        assert_eq!(root(), before);
        assert_ok!(sdk.transfer_exact(asset, &account(1), &account(1), 20, Preservation::Preserve));
        assert_eq!(Assets::balance(7, account(1)), 100);
    });
}

#[test]
fn sdk_transfer_respects_strict_ws3_vesting_and_nested_rollback() {
    ext().execute_with(|| {
        let amount = Balances::free_balance(account(1));
        assert_ok!(
            <Vesting as VestingSchedule<AccountId>>::add_vesting_schedule(
                &account(1),
                amount,
                DECIMALS,
                1000
            )
        );
        let before = root();
        assert_eq!(
            SdkFungibles.transfer_exact(
                FungibleAsset::NativeEtkn,
                &account(1),
                &account(2),
                DECIMALS,
                Preservation::Preserve
            ),
            Err(TransferError::Backend(TokenError::Frozen.into()))
        );
        assert_eq!(root(), before);
        let result: Result<Result<(), ()>, ()> = with_transaction_opaque_err(|| {
            assert_ok!(SdkFungibles.transfer_exact(
                FungibleAsset::NativeEtkn,
                &account(2),
                &account(3),
                DECIMALS,
                Preservation::Preserve
            ));
            TransactionOutcome::Rollback(Err(()))
        });
        assert_eq!(result, Ok(Err(())));
        assert_eq!(root(), before);
    });
}

#[test]
fn sdk_asset_deposits_refunds_and_owner_transition_are_source_evidence() {
    ext().execute_with(|| {
        let issuance = Balances::total_issuance();
        create_asset(7, 10);
        let creation = crate::AssetDeposit::get();
        assert_eq!(Balances::reserved_balance(account(1)), creation);
        assert_noop!(
            Assets::create(signed(1), 7, address(1), 10),
            pallet_assets::Error::<Runtime>::InUse
        );
        assert_noop!(
            Assets::create(signed(1), 8, address(1), 0),
            pallet_assets::Error::<Runtime>::MinBalanceZero
        );
        assert_ok!(Assets::set_metadata(
            signed(1),
            7,
            vec![b'x'; 64],
            vec![b'y'; 16],
            18
        ));
        let metadata = crate::AssetMetadataDepositBase::get()
            .checked_add(
                crate::AssetMetadataDepositPerByte::get()
                    .checked_mul(80)
                    .unwrap(),
            )
            .unwrap();
        assert_eq!(Balances::reserved_balance(account(1)), creation + metadata);
        assert_noop!(
            Assets::set_metadata(signed(1), 7, vec![b'x'; 65], vec![], 18),
            pallet_assets::Error::<Runtime>::BadMetadata
        );
        assert_ok!(Assets::approve_transfer(signed(1), 7, address(2), 10));
        assert_eq!(
            Balances::reserved_balance(account(1)),
            creation + metadata + crate::AssetApprovalDeposit::get()
        );
        assert_ok!(Assets::cancel_approval(signed(1), 7, address(2)));
        assert_eq!(Balances::reserved_balance(account(1)), creation + metadata);
        assert_ok!(Assets::touch(signed(3), 7));
        assert_eq!(
            Balances::reserved_balance(account(3)),
            crate::AssetAccountDeposit::get()
        );
        assert_ok!(Assets::refund(signed(3), 7, false));
        assert_eq!(Balances::reserved_balance(account(3)), 0);
        assert_ok!(Assets::set_team(
            signed(1),
            7,
            address(2),
            address(3),
            address(4)
        ));
        assert_ok!(Assets::transfer_ownership(signed(1), 7, address(5)));
        assert_eq!(Balances::reserved_balance(account(1)), 0);
        assert_eq!(Balances::reserved_balance(account(5)), creation + metadata);
        let details = pallet_assets::Asset::<Runtime>::get(7).unwrap();
        assert_eq!(
            (
                details.owner,
                details.issuer,
                details.admin,
                details.freezer
            ),
            (account(5), account(2), account(3), account(4))
        );
        assert_ok!(Assets::clear_metadata(signed(5), 7));
        assert_ok!(Assets::start_destroy(signed(5), 7));
        assert_ok!(Assets::finish_destroy(signed(5), 7));
        assert_eq!(Balances::reserved_balance(account(5)), 0);
        assert_eq!(Balances::total_issuance(), issuance);
    });
}

#[test]
fn sdk_admin_thaws_and_nft_admin_sets_metadata_not_reference_ledger_roles() {
    ext().execute_with(|| {
        create_asset(7, 10);
        assert_ok!(Assets::set_team(
            signed(1),
            7,
            address(2),
            address(3),
            address(4)
        ));
        assert_noop!(
            Assets::mint(signed(1), 7, address(1), 100),
            pallet_assets::Error::<Runtime>::NoPermission
        );
        assert_ok!(Assets::mint(signed(2), 7, address(1), 100));
        assert_ok!(Assets::freeze_asset(signed(4), 7));
        assert_noop!(
            Assets::thaw_asset(signed(4), 7),
            pallet_assets::Error::<Runtime>::NoPermission
        );
        let before = root();
        assert!(SdkFungibles
            .transfer_exact(
                FungibleAsset::Registered(7),
                &account(1),
                &account(2),
                10,
                Preservation::Preserve
            )
            .is_err());
        assert_eq!(root(), before);
        assert_ok!(Assets::thaw_asset(signed(3), 7));
        assert_ok!(Nfts::create(signed(1), address(1), Default::default()));
        assert_ok!(Nfts::set_team(
            signed(1),
            0,
            Some(address(2)),
            Some(address(3)),
            Some(address(4))
        ));
        assert_ok!(Nfts::mint(signed(2), 0, 1, address(5), None));
        assert_eq!(
            Balances::reserved_balance(account(2)),
            crate::NftItemDeposit::get()
        );
        assert_noop!(
            Nfts::set_metadata(signed(5), 0, 1, vec![1].try_into().unwrap()),
            pallet_nfts::Error::<Runtime>::NoPermission
        );
        assert_ok!(Nfts::set_metadata(
            signed(3),
            0,
            1,
            vec![1; 128].try_into().unwrap()
        ));
        let metadata = crate::NftMetadataDepositBase::get() + 128 * crate::NftDepositPerByte::get();
        assert_eq!(
            Balances::reserved_balance(account(1)),
            crate::AssetDeposit::get() + crate::NftCollectionDeposit::get() + metadata
        );
        assert_noop!(
            Nfts::transfer_ownership(signed(1), 0, address(6)),
            pallet_nfts::Error::<Runtime>::Unaccepted
        );
        assert_ok!(Nfts::set_accept_ownership(signed(6), Some(0)));
        assert_ok!(Nfts::transfer_ownership(signed(1), 0, address(6)));
        assert_eq!(
            Balances::reserved_balance(account(6)),
            crate::NftCollectionDeposit::get() + metadata
        );
        assert_ok!(Nfts::clear_metadata(signed(3), 0, 1));
        assert_ok!(Nfts::burn(signed(5), 0, 1));
        assert_eq!(Balances::reserved_balance(account(2)), 0);
        assert_eq!(
            Balances::reserved_balance(account(6)),
            crate::NftCollectionDeposit::get()
        );
    });
}

#[test]
fn bounded_pages_follow_hash_order_and_reject_missing_or_malformed_cursors() {
    ext().execute_with(|| {
        for id in [3, 19, 200, 900] {
            create_asset(id, 1);
        }
        let prefix = frame_support::storage::storage_prefix(b"Assets", b"Asset");
        let mut expected = [3u32, 19, 200, 900];
        expected.sort_by_key(|id| pallet_assets::Asset::<Runtime>::hashed_key_for(id));
        let (first, cursor) = page_ids::<2>(&prefix, None, 2).unwrap();
        assert_eq!(&first[..], &expected[..2]);
        assert_eq!(cursor, Some(expected[1]));
        let (second, end) = page_ids::<2>(&prefix, cursor, 2).unwrap();
        assert_eq!(&second[..], &expected[2..]);
        assert_eq!(end, None);
        assert_eq!(
            page_ids::<2>(&prefix, Some(123), 2),
            Err(PageError::InvalidCursor)
        );
        assert_eq!(
            page_ids::<2>(&prefix, None, 0),
            Err(PageError::InvalidLimit)
        );
        assert_eq!(
            page_ids::<2>(&prefix, None, 3),
            Err(PageError::InvalidLimit)
        );
        assert!(page_ids::<2>(&prefix, Some(expected[3]), 2)
            .unwrap()
            .0
            .is_empty());
        let mut malformed = prefix.to_vec();
        malformed.extend_from_slice(&[0; 20]);
        sp_io::storage::set(&malformed, &[0]);
        assert_eq!(
            page_ids::<2>(&prefix, None, 2),
            Err(PageError::BackendInvariant)
        );
    });
}

#[test]
fn existing_call_encoding_filter_and_force_rejection_are_preserved() {
    ext().execute_with(|| {
        let call = RuntimeCall::Assets(pallet_assets::Call::create {
            id: 7,
            admin: address(1),
            min_balance: 10,
        });
        let bytes = call.encode();
        let mut expected = vec![16, 0, 7, 0, 0, 0, 0];
        expected.extend_from_slice(&[1; 32]);
        expected.extend_from_slice(&10u128.to_le_bytes()); // SDK argument is plain u128.
        assert_eq!(bytes, expected);
        assert_eq!(
            RuntimeCall::decode_all(&mut &bytes[..]).unwrap().encode(),
            bytes
        );
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(RuntimeCall::decode_all(&mut &trailing[..]).is_err());
        assert!(RuntimeCall::decode_all(&mut &bytes[..bytes.len() - 1]).is_err());
        assert!(crate::IssuanceCallFilter::contains(&call));
        assert_ok!(call.dispatch(signed(1)));
        assert_noop!(
            Assets::force_create(RuntimeOrigin::root(), 8, address(1), false, 1),
            DispatchError::BadOrigin
        );
        assert_noop!(
            Nfts::force_create(RuntimeOrigin::root(), address(1), Default::default()),
            DispatchError::BadOrigin
        );
    });
}

#[test]
fn sdk_nft_role_removal_cannot_be_recovered_through_ensure_never() {
    ext().execute_with(|| {
        assert_ok!(Nfts::create(signed(1), address(1), Default::default()));
        assert_ok!(Nfts::set_team(
            signed(1),
            0,
            None,
            Some(address(1)),
            Some(address(1))
        ));
        assert_noop!(
            Nfts::set_team(
                signed(1),
                0,
                Some(address(2)),
                Some(address(1)),
                Some(address(1))
            ),
            pallet_nfts::Error::<Runtime>::NoPermission
        );
        assert_noop!(
            Nfts::set_team(
                RuntimeOrigin::root(),
                0,
                Some(address(2)),
                Some(address(1)),
                Some(address(1))
            ),
            DispatchError::BadOrigin
        );
    });
}

#[test]
fn item_pages_are_collection_scoped_and_malformed_key_lengths_reject() {
    ext().execute_with(|| {
        for _ in 0..2 {
            assert_ok!(Nfts::create(signed(1), address(1), Default::default()));
        }
        for (collection, item) in [(0, 1), (0, 2), (1, 3)] {
            assert_ok!(Nfts::mint(signed(1), collection, item, address(1), None));
        }
        let key = pallet_nfts::Item::<Runtime>::hashed_key_for(0, 1);
        let prefix = &key[..key.len() - 20];
        let (ids, next) = page_ids::<4>(prefix, None, 4).unwrap();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&1) && ids.contains(&2));
        assert_eq!(next, None);
        assert_eq!(
            page_ids::<4>(prefix, Some(3), 4),
            Err(PageError::InvalidCursor)
        );
        let mut malformed = prefix.to_vec();
        malformed.push(0);
        sp_io::storage::set(&malformed, &[]);
        assert_eq!(
            page_ids::<4>(prefix, None, 4),
            Err(PageError::BackendInvariant)
        );
    });
}

struct FixtureAdmissions;
impl AdmissionLookup for FixtureAdmissions {
    const CONFIGURED: bool = true;
    fn asset(id: u32) -> ApiResult<v1::AssetAdmission> {
        if (1..=100).contains(&id) {
            Ok(v1::AssetAdmission {
                id,
                minimum_balance: 10,
            })
        } else {
            Err(ApiError::UnsupportedAsset)
        }
    }
    fn collection(id: u32) -> ApiResult<()> {
        if (1..=3).contains(&id) {
            Ok(())
        } else {
            Err(ApiError::UnsupportedAsset)
        }
    }
}
type Reader = SdkReader<FixtureAdmissions>;
fn create_collection_one() {
    pallet_nfts::NextCollectionId::<Runtime>::put(1);
    assert_ok!(Nfts::create(signed(1), address(1), Default::default()));
}

#[test]
fn adopted_constants_match_runtime_and_empty_admissions_reject_existing_assets() {
    macro_rules! eq {($($constant:ident=>$binding:ident),* $(,)?)=>{$(assert_eq!(v1::$constant,crate::$binding::get());)*};}
    eq!(ASSET_DEPOSIT=>AssetDeposit,ASSET_ACCOUNT_DEPOSIT=>AssetAccountDeposit,ASSET_METADATA_BASE=>AssetMetadataDepositBase,ASSET_METADATA_BYTE=>AssetMetadataDepositPerByte,ASSET_APPROVAL_DEPOSIT=>AssetApprovalDeposit,NATIVE_MINIMUM=>ExistentialDeposit,NFT_COLLECTION_DEPOSIT=>NftCollectionDeposit,NFT_ITEM_DEPOSIT=>NftItemDeposit,NFT_METADATA_BASE=>NftMetadataDepositBase,NFT_ATTRIBUTE_BASE=>NftAttributeDepositBase,NFT_DEPOSIT_BYTE=>NftDepositPerByte,ASSET_SDK_STRING_LIMIT=>AssetStringLimit,REMOVE_LIMIT=>AssetRemoveItemsLimit,METADATA_LIMIT=>NftStringLimit,KEY_LIMIT=>NftKeyLimit,VALUE_LIMIT=>NftValueLimit,APPROVAL_LIMIT=>NftApprovalsLimit,ATTRIBUTE_APPROVAL_LIMIT=>NftItemAttributesApprovalsLimit,TIP_LIMIT=>NftMaxTips,DEADLINE_LIMIT=>NftMaxDeadlineDuration,ATTRIBUTES_PER_CALL=>NftMaxAttributesPerCall);
    ext().execute_with(|| {
        create_asset(7, 10);
        create_collection_one();
        let before = root();
        assert_eq!(
            DormantAssetsApi::asset_v1(7),
            Err(ApiError::UnsupportedAsset)
        );
        assert_eq!(
            DormantAssetsApi::assets_v1(None, 64),
            Err(ApiError::UnsupportedAsset)
        );
        assert_eq!(
            DormantAssetsApi::collection_v1(1),
            Err(ApiError::UnsupportedAsset)
        );
        assert_eq!(
            DormantAssetsApi::collections_v1(None, 64),
            Err(ApiError::UnsupportedAsset)
        );
        assert_eq!(
            DormantAssetsApi::item_v1(1, 1),
            Err(ApiError::UnsupportedAsset)
        );
        assert_eq!(
            DormantAssetsApi::items_v1(1, None, 64),
            Err(ApiError::UnsupportedAsset)
        );
        let mut adapter = DormantFungibles::new();
        assert_eq!(
            adapter.transfer(FungibleAsset::Registered(7), &account(1), &account(2), 0),
            Err(Error::UnsupportedAsset)
        );
        assert_eq!(
            adapter.balance(FungibleAsset::Registered(7), &account(1)),
            0
        );
        assert_eq!(root(), before);
        assert_ok!(adapter.transfer(
            FungibleAsset::NativeEtkn,
            &account(1),
            &account(2),
            DECIMALS
        ));
    });
}

#[test]
fn api_returns_exact_optional_metadata_roles_and_rejects_corrupt_backend_records() {
    ext().execute_with(|| {
        create_asset(7, 10);
        assert_eq!(Reader::asset_v1(7).unwrap().metadata, None);
        assert_ok!(Assets::set_metadata(signed(1), 7, vec![], vec![], 18));
        assert!(Reader::asset_v1(7).unwrap().metadata.is_some());
        assert_ok!(Assets::set_metadata(
            signed(1),
            7,
            vec![255; 64],
            vec![254; 16],
            18
        ));
        let asset = Reader::asset_v1(7).unwrap();
        assert_eq!(asset.owner, [1; 32]);
        assert_eq!(asset.minimum_balance, 10);
        assert_eq!(v1::decode_exact::<v1::AssetV1>(&asset.encode()), Ok(asset));
        assert_ok!(Assets::set_metadata(signed(1), 7, vec![], vec![1; 17], 0));
        assert_eq!(Reader::asset_v1(7), Err(ApiError::BackendInvariant));
        let key = pallet_assets::Metadata::<Runtime>::hashed_key_for(7);
        sp_io::storage::set(&key, &vec![0; 1000]);
        assert_eq!(Reader::asset_v1(7), Err(ApiError::BackendInvariant));
        sp_io::storage::clear(&key);
        sp_io::storage::set(&pallet_assets::Asset::<Runtime>::hashed_key_for(7), &[0]);
        assert_eq!(Reader::asset_v1(7), Err(ApiError::BackendInvariant));
        assert_eq!(Reader::asset_v1(8), Err(ApiError::NotFound));
        create_collection_one();
        assert_ok!(Nfts::set_team(
            signed(1),
            1,
            Some(address(2)),
            Some(address(3)),
            Some(address(4))
        ));
        let c = Reader::collection_v1(1).unwrap();
        assert_eq!(
            (c.issuer, c.admin, c.freezer),
            (Some([2; 32]), Some([3; 32]), Some([4; 32]))
        );
        assert_ok!(Nfts::mint(signed(2), 1, 1, address(5), None));
        assert_eq!(Reader::item_v1(1, 1).unwrap().owner, [5; 32]);
        assert_eq!(Reader::item_v1(1, 2), Err(ApiError::NotFound));
        let mut extra = pallet_nfts::CollectionRoles::none();
        extra.add_role(pallet_nfts::CollectionRole::Issuer);
        pallet_nfts::CollectionRoleOf::<Runtime>::insert(1, account(5), extra);
        assert_eq!(Reader::collection_v1(1), Err(ApiError::BackendInvariant));
    });
}

#[test]
fn interface_pages_bound_work_and_cursor_has_no_cross_collection_or_block_provenance() {
    ext().execute_with(|| {
        // Same numeric cursor can be valid independently in both collections.
        create_collection_one();
        assert_ok!(Nfts::create(signed(1), address(1), Default::default()));
        for c in 1..=2 {
            for i in 1..=3 {
                assert_ok!(Nfts::mint(signed(1), c, i, address(1), None));
            }
        }
        let cursor = Reader::items_v1(1, None, 1).unwrap().next.unwrap();
        assert!(Reader::items_v1(2, Some(cursor), 1).is_ok());
        assert_eq!(
            Reader::items_v1(2, Some(99), 1),
            Err(ApiError::InvalidCursor)
        );
        assert_ok!(Nfts::burn(signed(1), 2, cursor));
        assert_eq!(
            Reader::items_v1(2, Some(cursor), 1),
            Err(ApiError::InvalidCursor)
        );
        System::set_block_number(101);
        assert_ok!(Nfts::mint(signed(1), 2, cursor, address(1), None));
        assert!(Reader::items_v1(2, Some(cursor), 1).is_ok()); // provenance is not encoded
        for id in 1..=65 {
            create_asset(id, 10);
        }
        let before = root();
        let first = Reader::assets_v1(None, 64).unwrap();
        assert_eq!(first.entries.len(), 64);
        assert!(first.next.is_some());
        let last = Reader::assets_v1(first.next, 64).unwrap();
        assert_eq!(last.entries.len(), 1);
        assert_eq!(last.next, None);
        assert_eq!(Reader::assets_v1(None, 0), Err(ApiError::InvalidLimit));
        assert_eq!(Reader::assets_v1(None, 65), Err(ApiError::InvalidLimit));
        assert_eq!(root(), before);
    });
}

#[test]
fn transfer_contract_preserves_minimum_and_maps_backend_failures_deterministically() {
    ext().execute_with(|| {
        create_asset(7, 10);
        assert_ok!(Assets::mint(signed(1), 7, address(1), 100));
        let mut adapter = ContractFungibles::<FixtureAdmissions>::new();
        let asset = FungibleAsset::Registered(7);
        let before = root();
        assert_eq!(
            adapter.transfer(asset, &account(1), &account(2), 101),
            Err(Error::InsufficientBalance)
        );
        assert_eq!(root(), before);
        assert_eq!(
            adapter.transfer(asset, &account(1), &account(2), 1),
            Err(Error::BelowMinimum)
        );
        assert_eq!(root(), before);
        assert_ok!(adapter.transfer(asset, &account(1), &account(2), 20));
        assert_eq!(Assets::total_supply(7), 100);
        assert_ok!(Assets::freeze(signed(1), 7, address(1)));
        let before = root();
        assert_eq!(
            adapter.transfer(asset, &account(1), &account(2), 20),
            Err(Error::Frozen)
        );
        assert_eq!(root(), before);
        assert_eq!(
            transfer_error(TransferError::Backend(DispatchError::BadOrigin)),
            Error::BackendRejected
        );
        assert_eq!(
            transfer_error(TransferError::TransactionLimit),
            Error::AccountingInvariant
        );
        assert_eq!(
            transfer_error(TransferError::Backend(TokenError::Frozen.into())),
            Error::Frozen
        );
    });
}

#[test]
fn allocator_kernel_never_initializes_and_commits_backend_cursor_or_neither() {
    use era_v14_application_primitives::assets::{AllocationError, IdRange};
    ext().execute_with(|| {
        let name = v1::ASSET_CURSOR_KEY;
        let range = IdRange::new(1, 7).unwrap();
        let before = root();
        assert_eq!(
            allocate_existing(name, None, range, |_| false, |_| panic!("must not run")),
            Err(AllocationFailure::Cursor(v1::CursorError::Missing))
        );
        assert_eq!(root(), before);
        sp_io::storage::set(&allocator_key(v1::SCHEMA_KEY, None), &1u16.encode());
        sp_io::storage::set(&allocator_key(name, None), &Some(7u32).encode());
        let before = root();
        assert_eq!(
            allocate_existing(
                name,
                None,
                range,
                |_| false,
                |id| {
                    crate::v14_allocator::CreationGuard::<Runtime>::put((0,id));
                    Assets::create(signed(1), id, address(1), 10)?;
                    Err(DispatchError::Other("injected after SDK creation"))
                }
            ),
            Err(AllocationFailure::Backend(DispatchError::Other(
                "injected after SDK creation"
            )))
        );
        assert_eq!(root(), before);
        assert_eq!(
            allocate_existing(name, None, range, |_| true, |_| panic!("collision")),
            Err(AllocationFailure::Cursor(v1::CursorError::Allocation(
                AllocationError::Collision
            )))
        );
        assert_eq!(root(), before);
        assert_eq!(
            allocate_existing(
                name,
                None,
                range,
                pallet_assets::Asset::<Runtime>::contains_key,
                |id| {crate::v14_allocator::CreationGuard::<Runtime>::put((0,id));let result=Assets::create(signed(1), id, address(1), 10);crate::v14_allocator::CreationGuard::<Runtime>::kill();result}
            ),
            Ok(7)
        );
        let before = root();
        assert_eq!(
            allocate_existing(
                name,
                None,
                range,
                |_| panic!("exhausted"),
                |_| panic!("exhausted")
            ),
            Err(AllocationFailure::Cursor(v1::CursorError::Allocation(
                AllocationError::Exhausted
            )))
        );
        assert_eq!(root(), before);
        assert_eq!(
            sp_io::storage::get(&allocator_key(name, None))
                .unwrap()
                .to_vec(),
            vec![0]
        );
        sp_io::storage::set(&allocator_key(name, None), &[0, 0]);
        assert_eq!(read_allocator(name, None), Err(v1::CursorError::Malformed));
    });
}

#[test]
fn exact_sdk_refunds_and_ownership_movements_reject_partial_or_missing_reserves() {
    use frame_support::traits::ReservableCurrency;
    ext().execute_with(|| {
        assert_ok!(checked_deposit_change(
            &account(1),
            0,
            v1::ASSET_DEPOSIT,
            || Assets::create(signed(1), 7, address(1), 10)
        ));
        let deposit = v1::asset_metadata_deposit(3, 1).unwrap();
        assert_ok!(checked_deposit_change(&account(1), 0, deposit, || {
            Assets::set_metadata(signed(1), 7, b"abc".to_vec(), b"A".to_vec(), 18)
        }));
        let before = root();
        assert_eq!(
            checked_deposit_change(&account(1), deposit, 0, || {
                <Balances as ReservableCurrency<AccountId>>::unreserve(&account(1), deposit - 1);
                Ok(())
            }),
            Err(DepositError::Conservation)
        );
        assert_eq!(root(), before);
        assert_ok!(checked_reserve_move(
            &account(1),
            &account(2),
            v1::ASSET_DEPOSIT + deposit,
            || Assets::transfer_ownership(signed(1), 7, address(2))
        ));
        assert_ok!(checked_deposit_change(&account(2), deposit, 0, || {
            Assets::clear_metadata(signed(2), 7)
        }));
        let before = root();
        assert_eq!(
            checked_reserve_move(&account(1), &account(3), 1, || panic!("shortfall")),
            Err(DepositError::Shortfall)
        );
        assert_eq!(root(), before);
        assert_ok!(Assets::start_destroy(signed(2), 7));
        assert_ok!(checked_deposit_change(
            &account(2),
            v1::ASSET_DEPOSIT,
            0,
            || Assets::finish_destroy(signed(2), 7)
        ));
    });
}

#[test]
fn sdk_call_mapping_applies_adopted_roles_bounds_and_exclusions() {
    assert_eq!(
        asset_operation(&pallet_assets::Call::thaw {
            id: 7,
            who: address(1)
        })
        .unwrap()
        .authority(),
        Ok(v1::Authority::Admin)
    );
    assert_eq!(
        asset_operation(&pallet_assets::Call::set_metadata {
            id: 7,
            name: vec![],
            symbol: vec![0; 17],
            decimals: 18
        }),
        Err(v1::ContractError::Bound)
    );
    assert_eq!(
        asset_operation(&pallet_assets::Call::force_transfer {
            id: 7,
            source: address(1),
            dest: address(2),
            amount: 1
        }),
        Err(v1::ContractError::Unsupported)
    );
    assert_eq!(
        asset_operation(&pallet_assets::Call::refund {
            id: 7,
            allow_burn: true
        }),
        Err(v1::ContractError::Unsettled)
    );
    assert_eq!(
        nft_operation(&pallet_nfts::Call::set_metadata {
            collection: 1,
            item: 1,
            data: vec![0].try_into().unwrap()
        })
        .unwrap()
        .authority(),
        Ok(v1::Authority::Admin)
    );
    assert_eq!(
        nft_operation(&pallet_nfts::Call::set_team {
            collection: 1,
            issuer: None,
            admin: Some(address(1)),
            freezer: Some(address(1))
        }),
        Err(v1::ContractError::Authority)
    );
    assert_eq!(
        nft_operation(&pallet_nfts::Call::destroy {
            collection: 1,
            witness: pallet_nfts::DestroyWitness {
                item_metadatas: 1001,
                item_configs: 0,
                attributes: 0
            }
        }),
        Err(v1::ContractError::Bound)
    );
    assert_eq!(
        nft_operation(&pallet_nfts::Call::set_price {
            collection: 1,
            item: 1,
            price: None,
            whitelisted_buyer: None
        }),
        Err(v1::ContractError::Unsupported)
    );
}

#[test]
fn missing_bindings_and_inconsistent_minimum_are_distinct_from_empty_admission() {
    struct Missing;
    impl AdmissionLookup for Missing {
        const CONFIGURED: bool = false;
        fn asset(_: u32) -> ApiResult<v1::AssetAdmission> {
            panic!("not configured")
        }
        fn collection(_: u32) -> ApiResult<()> {
            panic!("not configured")
        }
    }
    ext().execute_with(|| {
        assert_eq!(
            SdkReader::<Missing>::asset_v1(1),
            Err(ApiError::Unconfigured)
        );
        assert_eq!(
            SdkReader::<Missing>::assets_v1(None, 1),
            Err(ApiError::Unconfigured)
        );
        assert_eq!(
            SdkReader::<Missing>::collection_v1(1),
            Err(ApiError::Unconfigured)
        );
        assert_eq!(
            SdkReader::<Missing>::collections_v1(None, 1),
            Err(ApiError::Unconfigured)
        );
        assert_eq!(
            SdkReader::<Missing>::item_v1(1, 1),
            Err(ApiError::Unconfigured)
        );
        assert_eq!(
            SdkReader::<Missing>::items_v1(1, None, 1),
            Err(ApiError::Unconfigured)
        );
        create_asset(7, 11);
        assert_eq!(Reader::asset_v1(7), Err(ApiError::BackendInvariant));
        assert_eq!(
            read_allocator(v1::ITEM_CURSOR_KEY, Some(1)),
            Err(v1::CursorError::Missing)
        );
        assert_ne!(
            allocator_key(v1::ITEM_CURSOR_KEY, Some(1)),
            allocator_key(v1::ITEM_CURSOR_KEY, Some(2))
        );
    });
}

#[test]
fn exact_nft_payer_refunds_follow_acceptance_and_preserve_independent_item_deposit() {
    ext().execute_with(|| {
        pallet_nfts::NextCollectionId::<Runtime>::put(1);
        assert_ok!(checked_deposit_change(
            &account(1),
            0,
            v1::NFT_COLLECTION_DEPOSIT,
            || Nfts::create(signed(1), address(1), Default::default())
        ));
        assert_ok!(Nfts::set_team(
            signed(1),
            1,
            Some(address(2)),
            Some(address(3)),
            Some(address(4))
        ));
        assert_ok!(checked_deposit_change(
            &account(2),
            0,
            v1::NFT_ITEM_DEPOSIT,
            || Nfts::mint(signed(2), 1, 1, address(5), None)
        ));
        let deposit = v1::nft_metadata_deposit(1).unwrap();
        assert_ok!(checked_deposit_change(&account(1), 0, deposit, || {
            Nfts::set_metadata(signed(3), 1, 1, vec![1].try_into().unwrap())
        }));
        let before = root();
        assert_eq!(
            checked_reserve_move(
                &account(1),
                &account(6),
                v1::NFT_COLLECTION_DEPOSIT + deposit,
                || Nfts::transfer_ownership(signed(1), 1, address(6))
            ),
            Err(DepositError::Backend(
                pallet_nfts::Error::<Runtime>::Unaccepted.into()
            ))
        );
        assert_eq!(root(), before);
        assert_ok!(Nfts::set_accept_ownership(signed(6), Some(1)));
        assert_ok!(checked_reserve_move(
            &account(1),
            &account(6),
            v1::NFT_COLLECTION_DEPOSIT + deposit,
            || Nfts::transfer_ownership(signed(1), 1, address(6))
        ));
        assert_eq!(Balances::reserved_balance(account(2)), v1::NFT_ITEM_DEPOSIT);
        assert_ok!(checked_deposit_change(&account(6), deposit, 0, || {
            Nfts::clear_metadata(signed(3), 1, 1)
        }));
        assert_ok!(checked_deposit_change(
            &account(2),
            v1::NFT_ITEM_DEPOSIT,
            0,
            || Nfts::burn(signed(5), 1, 1)
        ));
        let d = pallet_nfts::Collection::<Runtime>::get(1).unwrap();
        let w = d.destroy_witness();
        assert_ok!(v1::validate_destroy(
            d.items,
            [d.item_metadatas, d.item_configs, d.attributes],
            [w.item_metadatas, w.item_configs, w.attributes]
        ));
        assert_ok!(checked_deposit_change(
            &account(6),
            v1::NFT_COLLECTION_DEPOSIT,
            0,
            || Nfts::destroy(signed(6), 1, w)
                .map(|_| ())
                .map_err(|e| e.error)
        ));
    });
}

#[test]
fn regular_nft_attributes_charge_collection_owner_or_namespace_caller_and_refund_exactly() {
    ext().execute_with(|| {
        create_collection_one();
        assert_ok!(Nfts::set_team(
            signed(1),
            1,
            Some(address(2)),
            Some(address(3)),
            Some(address(4))
        ));
        assert_ok!(Nfts::mint(signed(2), 1, 1, address(5), None));
        let deposit = v1::attribute_deposit(64, 128).unwrap();
        let key: BoundedVec<u8, crate::NftKeyLimit> = vec![1; 64].try_into().unwrap();
        let value: BoundedVec<u8, crate::NftValueLimit> = vec![2; 128].try_into().unwrap();
        let namespace = pallet_nfts::AttributeNamespace::CollectionOwner;
        assert_ok!(checked_deposit_change(&account(1), 0, deposit, || {
            Nfts::set_attribute(
                signed(3),
                1,
                Some(1),
                namespace.clone(),
                key.clone(),
                value.clone(),
            )
        }));
        assert_eq!(Balances::reserved_balance(account(3)), 0);
        assert_ok!(checked_deposit_change(&account(1), deposit, 0, || {
            Nfts::clear_attribute(signed(3), 1, Some(1), namespace, key.clone())
        }));
        let namespace = pallet_nfts::AttributeNamespace::ItemOwner;
        assert_ok!(checked_deposit_change(&account(5), 0, deposit, || {
            Nfts::set_attribute(
                signed(5),
                1,
                Some(1),
                namespace.clone(),
                key.clone(),
                value.clone(),
            )
        }));
        assert_ok!(checked_deposit_change(&account(5), deposit, 0, || {
            Nfts::clear_attribute(signed(5), 1, Some(1), namespace, key.clone())
        }));
        assert_ok!(Nfts::approve_item_attributes(signed(5), 1, 1, address(6)));
        let namespace = pallet_nfts::AttributeNamespace::Account(account(6));
        assert_ok!(checked_deposit_change(&account(6), 0, deposit, || {
            Nfts::set_attribute(signed(6), 1, Some(1), namespace.clone(), key.clone(), value)
        }));
        assert_ok!(checked_deposit_change(&account(6), deposit, 0, || {
            Nfts::clear_attribute(signed(6), 1, Some(1), namespace, key)
        }));
    });
}

#[test]
fn nested_allocator_cursor_change_rolls_back_instead_of_overwriting_progress() {
    use era_v14_application_primitives::assets::IdRange;
    ext().execute_with(|| {
        sp_io::storage::set(&allocator_key(v1::SCHEMA_KEY, None), &1u16.encode());
        let key = allocator_key(v1::ASSET_CURSOR_KEY, None);
        sp_io::storage::set(&key, &Some(1u32).encode());
        let before = root();
        assert_eq!(
            allocate_existing(
                v1::ASSET_CURSOR_KEY,
                None,
                IdRange::new(1, 100).unwrap(),
                |_| false,
                |id| {
                    crate::v14_allocator::CreationGuard::<Runtime>::put((0,id));
                    Assets::create(signed(1), id, address(1), 10)?;
                    sp_io::storage::set(&key, &Some(2u32).encode());
                    Ok(())
                }
            ),
            Err(AllocationFailure::StateChanged)
        );
        assert_eq!(root(), before);
    });
}
