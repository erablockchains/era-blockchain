//! Two independent actual-runtime compiled-Wasm runs, 20 September2026.
//! Component maxima; runtime adds25% time/proof. See allocator-selected-weights.json.
use core::marker::PhantomData;
use frame_support::{traits::Get,weights::Weight};
pub struct WeightInfo<T>(PhantomData<T>);
impl<T:frame_system::Config> WeightInfo<T>{
 pub fn initialize()->Weight { Weight::from_parts(415150000,3675).saturating_add(T::DbWeight::get().reads(5)).saturating_add(T::DbWeight::get().writes(4)) }
 pub fn initialize_orphan()->Weight { Weight::from_parts(231347000,3519).saturating_add(T::DbWeight::get().reads(2)).saturating_add(T::DbWeight::get().writes(0)) }
 pub fn permit_asset()->Weight { Weight::from_parts(251345000,1493).saturating_add(T::DbWeight::get().reads(3)).saturating_add(T::DbWeight::get().writes(1)) }
 pub fn permit_collection()->Weight { Weight::from_parts(247620000,1493).saturating_add(T::DbWeight::get().reads(3)).saturating_add(T::DbWeight::get().writes(1)) }
 pub fn create_asset()->Weight { Weight::from_parts(1138834000,3675).saturating_add(T::DbWeight::get().reads(8)).saturating_add(T::DbWeight::get().writes(6)) }
 pub fn create_collection()->Weight { Weight::from_parts(2271569000,6078).saturating_add(T::DbWeight::get().reads(12)).saturating_add(T::DbWeight::get().writes(13)) }
 pub fn mint_item()->Weight { Weight::from_parts(1845524000,4326).saturating_add(T::DbWeight::get().reads(12)).saturating_add(T::DbWeight::get().writes(7)) }
 pub fn create_asset_rollback()->Weight { Weight::from_parts(529948000,3675).saturating_add(T::DbWeight::get().reads(7)).saturating_add(T::DbWeight::get().writes(0)) }
 pub fn mint_item_rollback()->Weight { Weight::from_parts(813187000,4326).saturating_add(T::DbWeight::get().reads(10)).saturating_add(T::DbWeight::get().writes(0)) }
 pub fn admit_existing_asset()->Weight { Weight::from_parts(507178000,3675).saturating_add(T::DbWeight::get().reads(6)).saturating_add(T::DbWeight::get().writes(1)) }
 pub fn admit_existing_collection()->Weight { Weight::from_parts(1151611000,11166).saturating_add(T::DbWeight::get().reads(14)).saturating_add(T::DbWeight::get().writes(3)) }
 pub fn legacy_create_absent()->Weight { Weight::from_parts(78217000,1487).saturating_add(T::DbWeight::get().reads(1)).saturating_add(T::DbWeight::get().writes(0)) }
 pub fn legacy_create_present()->Weight { Weight::from_parts(164066000,1490).saturating_add(T::DbWeight::get().reads(2)).saturating_add(T::DbWeight::get().writes(0)) }
 pub fn legacy_item_absent()->Weight { Weight::from_parts(78387000,3490).saturating_add(T::DbWeight::get().reads(1)).saturating_add(T::DbWeight::get().writes(0)) }
 pub fn legacy_item_present()->Weight { Weight::from_parts(155533000,3490).saturating_add(T::DbWeight::get().reads(2)).saturating_add(T::DbWeight::get().writes(0)) }
 pub fn api_assets_max()->Weight { Weight::from_parts(13640434000,175515).saturating_add(T::DbWeight::get().reads(194)).saturating_add(T::DbWeight::get().writes(0)) }
 pub fn api_collections_max()->Weight { Weight::from_parts(32750249000,652254).saturating_add(T::DbWeight::get().reads(450)).saturating_add(T::DbWeight::get().writes(0)) }
 pub fn api_items_max()->Weight { Weight::from_parts(16495265000,217830).saturating_add(T::DbWeight::get().reads(132)).saturating_add(T::DbWeight::get().writes(0)) }
}
