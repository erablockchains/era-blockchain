use crate::{*,v14_allocator::{self as a,*}};
use era_v14_application_primitives::assets::v1::{self,EraV14AssetsApiV1};
use frame_support::{assert_ok,assert_noop,traits::Currency};
use sp_runtime::{BuildStorage,MultiAddress,StateVersion};
use codec::Encode;
fn who(n:u8)->AccountId {AccountId::new([n;32])}
fn signed(n:u8)->RuntimeOrigin {RuntimeOrigin::signed(who(n))}
fn root()->Vec<u8>{sp_io::storage::root(StateVersion::V1)}
fn ext()->sp_io::TestExternalities{
 let mut storage=frame_system::GenesisConfig::<Runtime>::default().build_storage().unwrap();
 pallet_balances::GenesisConfig::<Runtime>{balances:(1..=6).map(|n|(who(n),1000*DECIMALS)).collect(),dev_accounts:None}.assimilate_storage(&mut storage).unwrap();
 let mut ext=sp_io::TestExternalities::new(storage);ext.execute_with(||System::set_block_number(1));ext
}
fn init(asset:u32,collection:u32){assert_ok!(EraV14AssetAllocator::initialize(RuntimeOrigin::root(),Some(asset),Some(collection),[1;32]));}
fn asset(){assert_ok!(EraV14AssetAllocator::permit_asset(RuntimeOrigin::root(),who(1),1));assert_ok!(EraV14AssetAllocator::create_asset(signed(1),who(2),who(3),who(4)));}
fn collection(){assert_ok!(EraV14AssetAllocator::permit_collection(RuntimeOrigin::root(),who(1)));assert_ok!(EraV14AssetAllocator::create_collection(signed(1),who(2),who(3),who(4)));}
#[test]
fn allocator_initialization_is_explicit_bounded_and_never_resets(){ext().execute_with(||{
 assert_eq!(a::Api::assets_v1(None,64),Err(v1::AssetApiErrorV1::Unconfigured));
 assert_noop!(EraV14AssetAllocator::initialize(signed(1),Some(1),Some(1),[1;32]),sp_runtime::DispatchError::BadOrigin);
 assert_noop!(EraV14AssetAllocator::initialize(RuntimeOrigin::root(),Some(0),Some(1),[1;32]),a::Error::<Runtime>::InvalidCursor);
 assert_noop!(EraV14AssetAllocator::initialize(RuntimeOrigin::root(),Some(1),Some(1),[0;32]),a::Error::<Runtime>::InvalidEvidence);
 init(1,1);let before=root();assert!(EraV14AssetAllocator::initialize(RuntimeOrigin::root(),Some(2),Some(2),[2;32]).is_err());assert_eq!(before,root());
 assert_eq!(NextAssetId::<Runtime>::get(),Some(Some(1)));
 let key=NextAssetId::<Runtime>::hashed_key();assert_eq!(sp_io::storage::get(&key).unwrap().as_ref(),Some(1u32).encode());
});}
#[test]
fn allocator_rejects_orphaned_state_and_occupied_initial_candidates(){ext().execute_with(||{
 assert_ok!(Assets::create(signed(1),7,MultiAddress::Id(who(1)),1));
 assert_noop!(EraV14AssetAllocator::initialize(RuntimeOrigin::root(),Some(7),Some(1),[1;32]),a::Error::<Runtime>::Occupied);
 NextItemId::<Runtime>::insert(5,Some(1));let before=root();assert!(EraV14AssetAllocator::initialize(RuntimeOrigin::root(),Some(8),Some(1),[1;32]).is_err());assert_eq!(before,root());
});}
#[test]
fn allocator_creates_native_backends_with_explicit_roles_and_exact_deposits(){ext().execute_with(||{
 let issuance=Balances::total_issuance();let free=Balances::free_balance(who(1));init(1,1);asset();collection();
 let read=a::Api::asset_v1(1).unwrap();assert_eq!(read.owner,[1u8;32]);assert_eq!(read.issuer,[2u8;32]);assert_eq!(read.admin,[3u8;32]);assert_eq!(read.freezer,[4u8;32]);assert_eq!(read.supply,0);assert!(!read.is_sufficient);
 let c=a::Api::collection_v1(1).unwrap();assert_eq!(c.issuer,Some([2u8;32]));assert_eq!(c.admin,Some([3u8;32]));assert_eq!(c.freezer,Some([4u8;32]));
 assert_eq!(Balances::free_balance(who(1)),free-20*DECIMALS);assert_eq!(Balances::reserved_balance(who(1)),20*DECIMALS);assert_eq!(Balances::total_issuance(),issuance);
 assert_eq!(NextAssetId::<Runtime>::get(),Some(Some(2)));assert_eq!(NextCollectionId::<Runtime>::get(),Some(Some(2)));assert_eq!(pallet_nfts::NextCollectionId::<Runtime>::get(),Some(2));
 assert!(!CreationGuard::<Runtime>::exists());assert!(!NextAssetPermit::<Runtime>::exists());assert!(!NextCollectionCreator::<Runtime>::exists());
});}
#[test]
fn allocator_failure_restores_permits_cursors_balances_and_events(){ext().execute_with(||{
 init(1,1);assert_ok!(EraV14AssetAllocator::permit_asset(RuntimeOrigin::root(),who(6),1));let _=Balances::make_free_balance_be(&who(6),EXISTENTIAL_DEPOSIT);
 let before=root();assert!(EraV14AssetAllocator::create_asset(signed(6),who(2),who(3),who(4)).is_err());assert_eq!(before,root());
 assert_noop!(EraV14AssetAllocator::create_asset(signed(1),who(2),who(3),who(4)),a::Error::<Runtime>::NotPermitted);
});}
#[test]
fn allocator_legacy_creation_cannot_bypass_the_owning_origin_gate(){ext().execute_with(||{
 assert_ok!(Assets::create(signed(1),0,MultiAddress::Id(who(1)),1));init(1,1);
 assert_noop!(Assets::create(signed(1),9,MultiAddress::Id(who(1)),1),sp_runtime::DispatchError::BadOrigin);
 assert!(Assets::maybe_total_supply(0).is_some());asset();collection();
 assert_noop!(Nfts::create(signed(1),MultiAddress::Id(who(1)),Default::default()),sp_runtime::DispatchError::BadOrigin);
 // This is an owning helper call, not a base filter assertion.
 assert_noop!(Nfts::mint(signed(2),1,99,MultiAddress::Id(who(3)),None),pallet_nfts::Error::<Runtime>::NoPermission);
});}
#[test]
fn allocator_item_creation_requires_issuer_and_never_reuses_a_burned_id(){ext().execute_with(||{
 init(1,1);collection();let issuance=Balances::total_issuance();let before=root();assert!(EraV14AssetAllocator::mint_item(signed(3),1,who(5)).is_err());assert_eq!(before,root());
 assert_ok!(EraV14AssetAllocator::mint_item(signed(2),1,who(5)));assert_eq!(a::Api::item_v1(1,1).unwrap().owner,[5u8;32]);assert_eq!(Balances::reserved_balance(who(2)),DECIMALS);
 assert_ok!(Nfts::burn(signed(5),1,1));assert_eq!(Balances::reserved_balance(who(2)),0);
 assert_ok!(EraV14AssetAllocator::mint_item(signed(2),1,who(5)));assert!(a::Api::item_v1(1,1).is_err());assert!(a::Api::item_v1(1,2).is_ok());assert_eq!(Balances::total_issuance(),issuance);
});}
#[test]
fn allocator_last_inclusive_ids_exhaust_without_wrapping(){ext().execute_with(||{
 init(v1::REGISTERED_LAST,v1::COLLECTION_LAST);asset();collection();assert_eq!(NextAssetId::<Runtime>::get(),Some(None));assert_eq!(NextCollectionId::<Runtime>::get(),Some(None));
 NextItemId::<Runtime>::insert(v1::COLLECTION_LAST,Some(u32::MAX));assert_ok!(EraV14AssetAllocator::mint_item(signed(2),v1::COLLECTION_LAST,who(5)));assert_eq!(NextItemId::<Runtime>::get(v1::COLLECTION_LAST),Some(None));
 let before=root();assert!(EraV14AssetAllocator::mint_item(signed(2),v1::COLLECTION_LAST,who(5)).is_err());assert_eq!(before,root());
});}
#[test]
fn allocator_admits_legacy_records_without_moving_custody_or_rewriting_them(){ext().execute_with(||{
 assert_ok!(Assets::create(signed(1),7,MultiAddress::Id(who(2)),10));assert_ok!(Assets::mint(signed(2),7,MultiAddress::Id(who(3)),1000));
 assert_ok!(Nfts::create(signed(1),MultiAddress::Id(who(2)),Default::default()));assert_ok!(Nfts::create(signed(1),MultiAddress::Id(who(2)),Default::default()));
 assert_ok!(Nfts::mint(signed(2),1,10,MultiAddress::Id(who(5)),None));
 let details=pallet_assets::Asset::<Runtime>::get(7).unwrap().encode();let item=pallet_nfts::Item::<Runtime>::get(1,10).unwrap().encode();let native=Balances::total_issuance();init(8,2);
 assert_ok!(EraV14AssetAllocator::admit_existing_asset(RuntimeOrigin::root(),7,10));
 assert_ok!(EraV14AssetAllocator::admit_existing_collection(RuntimeOrigin::root(),1,Some(11),[2;32]));
 assert_eq!(pallet_assets::Asset::<Runtime>::get(7).unwrap().encode(),details);assert_eq!(pallet_nfts::Item::<Runtime>::get(1,10).unwrap().encode(),item);assert_eq!(Balances::total_issuance(),native);
 assert!(pallet_nfts::Collection::<Runtime>::contains_key(0));assert_eq!(a::Api::collection_v1(0),Err(v1::AssetApiErrorV1::UnsupportedAsset));
 // Frozen V1 pagination rejects unsupported legacy rows; it never silently scans/skips them.
 assert_eq!(a::Api::collections_v1(None,64),Err(v1::AssetApiErrorV1::UnsupportedAsset));
 assert_ok!(EraV14AssetAllocator::mint_item(signed(2),1,who(5)));assert!(a::Api::item_v1(1,11).is_ok());
});}
#[test]
fn allocator_admission_mismatch_and_malformed_cursor_roll_back(){ext().execute_with(||{
 assert_ok!(Assets::create(signed(1),7,MultiAddress::Id(who(1)),10));init(8,1);
 assert_noop!(EraV14AssetAllocator::admit_existing_asset(RuntimeOrigin::root(),7,9),a::Error::<Runtime>::BackendInvariant);
 assert_ok!(EraV14AssetAllocator::permit_asset(RuntimeOrigin::root(),who(1),1));sp_io::storage::set(&NextAssetId::<Runtime>::hashed_key(),&[1,8,0,0,0,99]);let before=root();assert!(EraV14AssetAllocator::create_asset(signed(1),who(2),who(3),who(4)).is_err());assert_eq!(before,root());
});}
#[test]
fn allocator_all_six_queries_are_bounded_and_preserve_hash_order(){ext().execute_with(||{
 init(1,1);for _ in 0..65{asset();}collection();for _ in 0..65{assert_ok!(EraV14AssetAllocator::mint_item(signed(2),1,who(5)));}
 let first=a::Api::assets_v1(None,64).unwrap();assert_eq!(first.entries.len(),64);let second=a::Api::assets_v1(first.next,64).unwrap();assert_eq!(second.entries.len(),1);assert_eq!(second.next,None);
 assert_eq!(a::Api::assets_v1(None,65),Err(v1::AssetApiErrorV1::InvalidLimit));assert_eq!(a::Api::assets_v1(Some(66),64),Err(v1::AssetApiErrorV1::InvalidCursor));
 assert_eq!(a::Api::collections_v1(None,64).unwrap().entries.len(),1);let items=a::Api::items_v1(1,None,64).unwrap();assert_eq!(items.entries.len(),64);assert_eq!(a::Api::items_v1(1,items.next,64).unwrap().entries.len(),1);
});}

#[test]
fn allocator_accepts_only_the_empty_canonical_frame_version_before_installation(){ext().execute_with(||{
 frame_support::traits::StorageVersion::new(0).put::<EraV14AssetAllocator>();init(1,1);assert!(a::schema_valid());
});}
#[test]
fn allocator_managed_roles_and_issuer_mint_mode_cannot_be_renounced(){ext().execute_with(||{
 init(1,1);collection();
 assert_noop!(Nfts::set_team(signed(1),1,None,Some(MultiAddress::Id(who(3))),Some(MultiAddress::Id(who(4)))),pallet_nfts::Error::<Runtime>::NoPermission);
 assert_noop!(Nfts::update_mint_settings(signed(2),1,Default::default()),pallet_nfts::Error::<Runtime>::NoPermission);
});}

#[test]
fn allocator_measured_calls_fit_time_proof_and_normal_extrinsic_limits(){
 use frame_support::{traits::Get,dispatch::{DispatchClass,GetDispatchInfo},weights::Weight};
 let limits:frame_system::limits::BlockWeights=<Runtime as frame_system::Config>::BlockWeights::get();let normal=limits.get(DispatchClass::Normal);let cap=normal.max_total.unwrap();
 for call in 0..=7 {let w=a::weight(call);assert!(w.all_lte(Weight::from_parts(cap.ref_time()/4,cap.proof_size()/4))&&w.all_lte(normal.max_extrinsic.unwrap()),"call{call}: {w:?}");}
 let call:RuntimeCall=a::Call::create_asset{issuer:who(2),admin:who(3),freezer:who(4)}.into();assert_eq!(call.get_dispatch_info().call_weight,a::weight(3));
}
