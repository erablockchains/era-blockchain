//! Actual-runtime compiled-Wasm measurements. Public synthetic fixtures only.
use crate::{*,v14_allocator::{self as a,*}};
use alloc::{vec,vec::Vec};
use core::marker::PhantomData;
use frame_benchmarking::v1::{account,benchmarks};
use frame_support::{assert_ok,traits::{Currency,Contains,EnsureOriginWithArg}};
use sp_runtime::MultiAddress;
use era_v14_application_primitives::assets::v1::EraV14AssetsApiV1;
pub use frame_system::Config;
pub struct Pallet<T:Config>(PhantomData<T>);
fn who(n:u32)->AccountId{account("allocator",n,0)}
fn setup(){System::set_block_number(100);for n in 0..5{let _=Balances::make_free_balance_be(&who(n),1_000_000*DECIMALS);}}
fn init(){assert_ok!(EraV14AssetAllocator::initialize(RuntimeOrigin::root(),Some(1),Some(1),[1;32]));}
fn asset(){assert_ok!(EraV14AssetAllocator::permit_asset(RuntimeOrigin::root(),who(0),1));assert_ok!(EraV14AssetAllocator::create_asset(RuntimeOrigin::signed(who(0)),who(1),who(2),who(3)));}
fn collection(){assert_ok!(EraV14AssetAllocator::permit_collection(RuntimeOrigin::root(),who(0)));assert_ok!(EraV14AssetAllocator::create_collection(RuntimeOrigin::signed(who(0)),who(1),who(2),who(3)));}
fn mint(){assert_ok!(EraV14AssetAllocator::mint_item(RuntimeOrigin::signed(who(1)),1,who(4)));}
fn asset_page(){for id in 1..=65{asset();assert_ok!(Assets::set_metadata(RuntimeOrigin::signed(who(0)),id,vec![b'n';64],vec![b's';16],18));}}
fn collection_page(){for id in 1..=65{collection();assert_ok!(Nfts::set_collection_metadata(RuntimeOrigin::signed(who(2)),id,vec![b'm';128].try_into().unwrap()));}}
fn item_page(){collection();for id in 1..=65{mint();assert_ok!(Nfts::set_metadata(RuntimeOrigin::signed(who(2)),1,id,vec![b'm';128].try_into().unwrap()));}}
benchmarks! {
 initialize {setup();}: {EraV14AssetAllocator::initialize(RuntimeOrigin::root(),Some(1),Some(1),[1;32])?;}
 verify {assert!(a::schema_valid());}
 initialize_orphan {setup();NextItemId::<Runtime>::insert(10,Some(1));}: {assert!(EraV14AssetAllocator::initialize(RuntimeOrigin::root(),Some(1),Some(1),[1;32]).is_err());}
 permit_asset {setup();init();}: {EraV14AssetAllocator::permit_asset(RuntimeOrigin::root(),who(0),1)?;}
 permit_collection {setup();init();}: {EraV14AssetAllocator::permit_collection(RuntimeOrigin::root(),who(0))?;}
 create_asset {setup();init();assert_ok!(EraV14AssetAllocator::permit_asset(RuntimeOrigin::root(),who(0),1));}: {EraV14AssetAllocator::create_asset(RuntimeOrigin::signed(who(0)),who(1),who(2),who(3))?;}
 verify {assert_eq!(NextAssetId::<Runtime>::get(),Some(Some(2)));}
 create_collection {setup();init();assert_ok!(EraV14AssetAllocator::permit_collection(RuntimeOrigin::root(),who(0)));}: {EraV14AssetAllocator::create_collection(RuntimeOrigin::signed(who(0)),who(1),who(2),who(3))?;}
 verify {assert_eq!(NextItemId::<Runtime>::get(1),Some(Some(1)));}
 mint_item {setup();init();collection();}: {EraV14AssetAllocator::mint_item(RuntimeOrigin::signed(who(1)),1,who(4))?;}
 verify {assert_eq!(NextItemId::<Runtime>::get(1),Some(Some(2)));}
 create_asset_rollback {setup();init();assert_ok!(EraV14AssetAllocator::permit_asset(RuntimeOrigin::root(),who(0),1));let _=Balances::make_free_balance_be(&who(0),EXISTENTIAL_DEPOSIT);}: {assert!(EraV14AssetAllocator::create_asset(RuntimeOrigin::signed(who(0)),who(1),who(2),who(3)).is_err());}
 verify {assert!(NextAssetPermit::<Runtime>::exists());assert!(!CreationGuard::<Runtime>::exists());}
 mint_item_rollback {setup();init();collection();let _=Balances::make_free_balance_be(&who(1),EXISTENTIAL_DEPOSIT);}: {assert!(EraV14AssetAllocator::mint_item(RuntimeOrigin::signed(who(1)),1,who(4)).is_err());}
 verify {assert_eq!(NextItemId::<Runtime>::get(1),Some(Some(1)));assert!(!MintGuard::<Runtime>::exists());}
 admit_existing_asset {setup();assert_ok!(Assets::create(RuntimeOrigin::signed(who(0)),7,MultiAddress::Id(who(1)),1));assert_ok!(Assets::set_metadata(RuntimeOrigin::signed(who(0)),7,vec![b'n';64],vec![b's';16],18));assert_ok!(EraV14AssetAllocator::initialize(RuntimeOrigin::root(),Some(8),Some(1),[1;32]));}: {EraV14AssetAllocator::admit_existing_asset(RuntimeOrigin::root(),7,1)?;}
 admit_existing_collection {setup();pallet_nfts::NextCollectionId::<Runtime>::put(1);assert_ok!(Nfts::create(RuntimeOrigin::signed(who(0)),MultiAddress::Id(who(2)),Default::default()));assert_ok!(Nfts::set_team(RuntimeOrigin::signed(who(0)),1,Some(MultiAddress::Id(who(1))),Some(MultiAddress::Id(who(2))),Some(MultiAddress::Id(who(3)))));assert_ok!(Nfts::set_collection_metadata(RuntimeOrigin::signed(who(2)),1,vec![b'm';128].try_into().unwrap()));assert_ok!(EraV14AssetAllocator::initialize(RuntimeOrigin::root(),Some(1),Some(2),[1;32]));}: {EraV14AssetAllocator::admit_existing_collection(RuntimeOrigin::root(),1,Some(1),[2;32])?;}
 legacy_create_absent {setup();}: {assert!(a::CreateOrigin::<0>::try_origin(RuntimeOrigin::signed(who(0)),&7).is_ok());}
 legacy_create_present {setup();init();CreationGuard::<Runtime>::put((0,7));}: {assert!(a::CreateOrigin::<0>::try_origin(RuntimeOrigin::signed(who(0)),&7).is_ok());}
 legacy_item_absent {setup();}: {assert!(a::ItemGuard::contains(&(1,1)));}
 legacy_item_present {setup();init();collection();MintGuard::<Runtime>::put((1,1));}: {assert!(a::ItemGuard::contains(&(1,1)));}
 api_assets_max {setup();init();asset_page();}: {let p=a::Api::assets_v1(None,64).unwrap();assert_eq!(p.entries.len(),64);assert!(p.next.is_some());}
 api_collections_max {setup();init();collection_page();}: {let p=a::Api::collections_v1(None,64).unwrap();assert_eq!(p.entries.len(),64);assert!(p.next.is_some());}
 api_items_max {setup();init();item_page();}: {let p=a::Api::items_v1(1,None,64).unwrap();assert_eq!(p.entries.len(),64);assert!(p.next.is_some());}
}
