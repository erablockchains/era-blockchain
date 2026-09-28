//! Versioned asset allocation. Initialization is an explicit bounded, evidence-bound Root action.
//! No migration guesses historical cursors or changes existing objects/balances.
use crate::{AccountId,Assets,Nfts,Runtime,RuntimeOrigin};
use crate::v14_assets::{self as backend,AdmissionLookup};
use era_v14_application_primitives::assets::{v1,IdRange};
use codec::{Encode,Decode,DecodeWithMemTracking,MaxEncodedLen};
use scale_info::TypeInfo;
use frame_support::{pallet_prelude::*,traits::{Contains,EnsureOriginWithArg},weights::Weight};
use sp_runtime::{DispatchError,MultiAddress};

#[derive(Encode,Decode,DecodeWithMemTracking,MaxEncodedLen,TypeInfo,Clone,PartialEq,Eq,Debug)]
pub struct AssetPermit {pub creator:AccountId,pub minimum:u128}
impl pallet::Config for Runtime {}
pub fn schema_valid()->bool {
 let mut bytes=[0u8;2];
 sp_io::storage::read(&SchemaVersion::<Runtime>::hashed_key(),&mut bytes,0)==Some(2) && bytes==v1::ALLOCATOR_VERSION.to_le_bytes()
}
pub struct Admissions;
impl AdmissionLookup for Admissions {
 const CONFIGURED:bool=true;
 fn configured()->bool {schema_valid()}
 fn asset(id:u32)->v1::ApiResult<v1::AssetAdmission>{
  if !Self::configured(){return Err(v1::AssetApiErrorV1::Unconfigured)}
  AdmittedAssets::<Runtime>::get(id).map(|minimum_balance|v1::AssetAdmission{id,minimum_balance}).ok_or(v1::AssetApiErrorV1::UnsupportedAsset)
 }
 fn collection(id:u32)->v1::ApiResult<()>{
  if !Self::configured(){return Err(v1::AssetApiErrorV1::Unconfigured)}
  if AdmittedCollections::<Runtime>::contains_key(id){Ok(())}else{Err(v1::AssetApiErrorV1::UnsupportedAsset)}
 }
}
pub type Api=backend::SdkReader<Admissions>;
// The same owning-origin gate runs even through sudo_as / dispatch_bypass_filter.
// Before explicit initialization, retained legacy creation permissions are unchanged.
pub struct CreateOrigin<const KIND:u8>;
impl<const KIND:u8> EnsureOriginWithArg<RuntimeOrigin,u32> for CreateOrigin<KIND>{
 type Success=AccountId;
 fn try_origin(origin:RuntimeOrigin,id:&u32)->Result<AccountId,RuntimeOrigin>{
  if SchemaVersion::<Runtime>::exists() && CreationGuard::<Runtime>::get()!=Some((KIND,*id)){return Err(origin)}
  frame_system::ensure_signed(origin.clone()).map_err(|_|origin)
 }
 #[cfg(feature="runtime-benchmarks")]
 fn try_successful_origin(_: &u32)->Result<RuntimeOrigin,()>{Ok(RuntimeOrigin::signed(AccountId::new([1;32])))}
}
pub struct ManagedCollection;
impl Contains<u32> for ManagedCollection {fn contains(id:&u32)->bool {NextItemId::<Runtime>::contains_key(id)}}
pub struct ItemGuard;
impl Contains<(u32,u32)> for ItemGuard {
 fn contains(&(collection,item):&(u32,u32))->bool{
  !NextItemId::<Runtime>::contains_key(collection) || MintGuard::<Runtime>::get()==Some((collection,item))
 }
}
// Static measured weights; no state reads or placeholder MAX weights on valid call indices.
fn envelope(paths:&[Weight])->Weight{let w=paths.iter().fold(Weight::zero(),|a,b|Weight::from_parts(a.ref_time().max(b.ref_time()),a.proof_size().max(b.proof_size())));w.saturating_add(Weight::from_parts(w.ref_time()/4,w.proof_size()/4))}
pub fn weight(call:u8)->Weight {use crate::v14_allocator_weights::WeightInfo as M;match call {
 0=>envelope(&[M::<Runtime>::initialize(),M::<Runtime>::initialize_orphan()]),
 1=>envelope(&[M::<Runtime>::permit_asset()]),2=>envelope(&[M::<Runtime>::permit_collection()]),
 3=>envelope(&[M::<Runtime>::create_asset(),M::<Runtime>::create_asset_rollback()]),
 4=>envelope(&[M::<Runtime>::create_collection()]),
 5=>envelope(&[M::<Runtime>::mint_item(),M::<Runtime>::mint_item_rollback()]),
 6=>envelope(&[M::<Runtime>::admit_existing_asset()]),7=>envelope(&[M::<Runtime>::admit_existing_collection()]),
 _=>Weight::MAX}}
pub fn legacy_create_weight()->Weight {use crate::v14_allocator_weights::WeightInfo as M;envelope(&[M::<Runtime>::legacy_create_absent(),M::<Runtime>::legacy_create_present()])}
pub fn legacy_item_weight()->Weight {use crate::v14_allocator_weights::WeightInfo as M;envelope(&[M::<Runtime>::legacy_item_absent(),M::<Runtime>::legacy_item_present()])}
fn range(first:u32,last:u32)->IdRange{IdRange::new(first,last).expect("constant nonempty range")}

#[frame_support::pallet]
pub mod pallet {
 use super::*;
 use frame_system::pallet_prelude::*;
 #[pallet::config]
 pub trait Config:frame_system::Config<AccountId=AccountId,RuntimeEvent:From<Event<Self>>>{}
 #[pallet::pallet] pub struct Pallet<T>(_);
 #[pallet::storage] pub type SchemaVersion<T> = StorageValue<_,u16,OptionQuery>;
 // OptionQuery distinguishes absence from stored None (exhaustion).
 #[pallet::storage] pub type NextAssetId<T> = StorageValue<_,Option<u32>,OptionQuery>;
 #[pallet::storage] pub type NextCollectionId<T> = StorageValue<_,Option<u32>,OptionQuery>;
 #[pallet::storage] pub type NextItemId<T> = StorageMap<_,Blake2_128Concat,u32,Option<u32>,OptionQuery>;
 #[pallet::storage] pub type InitializationEvidence<T> = StorageValue<_,[u8;32],OptionQuery>;
 #[pallet::storage] pub type ItemInitializationEvidence<T> = StorageMap<_,Blake2_128Concat,u32,[u8;32],OptionQuery>;
 #[pallet::storage] pub type NextAssetPermit<T> = StorageValue<_,AssetPermit,OptionQuery>;
 #[pallet::storage] pub type NextCollectionCreator<T:Config> = StorageValue<_,AccountId,OptionQuery>;
 #[pallet::storage] pub type AdmittedAssets<T> = StorageMap<_,Blake2_128Concat,u32,u128,OptionQuery>;
 #[pallet::storage] pub type AdmittedCollections<T> = StorageMap<_,Blake2_128Concat,u32,(),OptionQuery>;
 #[pallet::storage] pub type CreationGuard<T> = StorageValue<_,(u8,u32),OptionQuery>;
 #[pallet::storage] pub type MintGuard<T> = StorageValue<_,(u32,u32),OptionQuery>;
 #[pallet::event]
 #[pallet::generate_deposit(pub(super) fn deposit_event)]
 pub enum Event<T:Config>{
  Initialized{next_asset:Option<u32>,next_collection:Option<u32>,evidence:[u8;32]},
  AssetPermitted{creator:AccountId,minimum:u128},CollectionPermitted{creator:AccountId},
  AssetCreated{id:u32,creator:AccountId},CollectionCreated{id:u32,creator:AccountId},ItemCreated{collection:u32,item:u32,issuer:AccountId},
  LegacyAssetAdmitted{id:u32},LegacyCollectionAdmitted{id:u32,next_item:Option<u32>,evidence:[u8;32]},
 }
 #[pallet::error]
 pub enum Error<T>{Uninitialized,AlreadyInitialized,InvalidEvidence,InvalidCursor,Occupied,NotPermitted,InvalidMinimum,BackendInvariant,AllocationFailed,Reentrant,NativeCursorAhead,Unsupported}
 #[pallet::call]
 impl<T:Config> Pallet<T>{
  /// Cursors must come from owner-approved historical high-water evidence. No scan or reset.
  #[pallet::call_index(0)] #[pallet::weight(super::weight(0))] #[frame_support::transactional]
  pub fn initialize(origin:OriginFor<T>,next_asset:Option<u32>,next_collection:Option<u32>,evidence:[u8;32])->DispatchResult{
   ensure_root(origin)?;
   ensure!(evidence!=[0;32],Error::<T>::InvalidEvidence);
   // One bounded prefix probe also rejects orphaned maps/guards, not just scalar keys.
   let prefix=frame_support::storage::storage_prefix(b"EraV14AssetAllocator",b"");
   let pallet_prefix=&prefix[..16];
   ensure!(!sp_io::storage::exists(pallet_prefix),Error::<T>::AlreadyInitialized);
   let frame_version=StorageVersion::storage_key::<Pallet<T>>();
   let mut first=sp_io::storage::next_key(pallet_prefix);
   if first.as_deref()==Some(&frame_version[..]) {
    let mut bytes=[0u8;2];ensure!(sp_io::storage::read(&frame_version,&mut bytes,0)==Some(2)&&bytes==[0,0],Error::<T>::BackendInvariant);
    first=sp_io::storage::next_key(&frame_version);
   }
   ensure!(!first.is_some_and(|key|key.starts_with(pallet_prefix)),Error::<T>::AlreadyInitialized);
   Self::valid_cursor(next_asset,v1::REGISTERED_FIRST,v1::REGISTERED_LAST)?;
   Self::valid_cursor(next_collection,v1::COLLECTION_FIRST,v1::COLLECTION_LAST)?;
   if let Some(id)=next_asset{ensure!(!pallet_assets::Asset::<Runtime>::contains_key(id),Error::<T>::Occupied)}
   if let Some(id)=next_collection{
    ensure!(!pallet_nfts::Collection::<Runtime>::contains_key(id),Error::<T>::Occupied);
    ensure!(pallet_nfts::NextCollectionId::<Runtime>::get().unwrap_or(0)<=id,Error::<T>::NativeCursorAhead);
   }
   SchemaVersion::<T>::put(v1::ALLOCATOR_VERSION);NextAssetId::<T>::put(next_asset);NextCollectionId::<T>::put(next_collection);InitializationEvidence::<T>::put(evidence);
   Self::deposit_event(Event::Initialized{next_asset,next_collection,evidence});Ok(())
  }
  #[pallet::call_index(1)] #[pallet::weight(super::weight(1))]
  pub fn permit_asset(origin:OriginFor<T>,creator:AccountId,minimum:u128)->DispatchResult{
   ensure_root(origin)?;Self::ready()?;ensure!(minimum>0,Error::<T>::InvalidMinimum);
   NextAssetPermit::<T>::put(AssetPermit{creator:creator.clone(),minimum});Self::deposit_event(Event::AssetPermitted{creator,minimum});Ok(())
  }
  #[pallet::call_index(2)] #[pallet::weight(super::weight(2))]
  pub fn permit_collection(origin:OriginFor<T>,creator:AccountId)->DispatchResult{
   ensure_root(origin)?;Self::ready()?;NextCollectionCreator::<T>::put(&creator);Self::deposit_event(Event::CollectionPermitted{creator});Ok(())
  }
  #[pallet::call_index(3)] #[pallet::weight(super::weight(3))] #[frame_support::transactional]
  pub fn create_asset(origin:OriginFor<T>,issuer:AccountId,admin:AccountId,freezer:AccountId)->DispatchResult{
   let who=ensure_signed(origin)?;Self::ready()?;
   let permit=NextAssetPermit::<T>::get().ok_or(Error::<T>::NotPermitted)?;ensure!(permit.creator==who,Error::<T>::NotPermitted);
   let id=backend::allocate_existing(v1::ASSET_CURSOR_KEY,None,range(v1::REGISTERED_FIRST,v1::REGISTERED_LAST),|id|pallet_assets::Asset::<Runtime>::contains_key(id),|id|{
    CreationGuard::<T>::put((0,id));
    backend::checked_deposit_change(&who,0,v1::ASSET_DEPOSIT,||{
     Assets::create(RuntimeOrigin::signed(who.clone()),id,MultiAddress::Id(admin.clone()),permit.minimum)?;
     Assets::set_team(RuntimeOrigin::signed(who.clone()),id,MultiAddress::Id(issuer.clone()),MultiAddress::Id(admin.clone()),MultiAddress::Id(freezer.clone()))?;Ok(())
    }).map_err(|_|DispatchError::Other("allocator deposit conservation"))?;
    CreationGuard::<T>::kill();Ok(())
   }).map_err(|_|Error::<T>::AllocationFailed)?;
   AdmittedAssets::<T>::insert(id,permit.minimum);NextAssetPermit::<T>::kill();Self::deposit_event(Event::AssetCreated{id,creator:who});Ok(())
  }
  #[pallet::call_index(4)] #[pallet::weight(super::weight(4))] #[frame_support::transactional]
  pub fn create_collection(origin:OriginFor<T>,issuer:AccountId,admin:AccountId,freezer:AccountId)->DispatchResult{
   let who=ensure_signed(origin)?;Self::ready()?;ensure!(NextCollectionCreator::<T>::get()==Some(who.clone()),Error::<T>::NotPermitted);
   let id=backend::allocate_existing(v1::COLLECTION_CURSOR_KEY,None,range(v1::COLLECTION_FIRST,v1::COLLECTION_LAST),|id|pallet_nfts::Collection::<Runtime>::contains_key(id),|id|{
    ensure!(pallet_nfts::NextCollectionId::<Runtime>::get().unwrap_or(0)<=id,Error::<T>::NativeCursorAhead);
    ensure!(!NextItemId::<T>::contains_key(id),Error::<T>::BackendInvariant);
    pallet_nfts::NextCollectionId::<Runtime>::put(id);CreationGuard::<T>::put((1,id));
    backend::checked_deposit_change(&who,0,v1::NFT_COLLECTION_DEPOSIT,||{
     Nfts::create(RuntimeOrigin::signed(who.clone()),MultiAddress::Id(admin.clone()),pallet_nfts::CollectionConfig{settings:pallet_nfts::CollectionSettings::all_enabled(),max_supply:None,mint_settings:Default::default()})?;
     Nfts::set_team(RuntimeOrigin::signed(who.clone()),id,Some(MultiAddress::Id(issuer.clone())),Some(MultiAddress::Id(admin.clone())),Some(MultiAddress::Id(freezer.clone())))?;Ok(())
    }).map_err(|_|DispatchError::Other("allocator collection deposit conservation"))?;
    CreationGuard::<T>::kill();Ok(())
   }).map_err(|_|Error::<T>::AllocationFailed)?;
   AdmittedCollections::<T>::insert(id,());NextItemId::<T>::insert(id,Some(v1::ITEM_FIRST));NextCollectionCreator::<T>::kill();Self::deposit_event(Event::CollectionCreated{id,creator:who});Ok(())
  }
  #[pallet::call_index(5)] #[pallet::weight(super::weight(5))] #[frame_support::transactional]
  pub fn mint_item(origin:OriginFor<T>,collection:u32,recipient:AccountId)->DispatchResult{
   let who=ensure_signed(origin)?;Self::ready()?;ensure!(AdmittedCollections::<T>::contains_key(collection),Error::<T>::NotPermitted);
   ensure!(pallet_nfts::CollectionRoleOf::<Runtime>::get(collection,&who).is_some_and(|r|r.has_role(pallet_nfts::CollectionRole::Issuer)),Error::<T>::NotPermitted);
   let config=pallet_nfts::CollectionConfigOf::<Runtime>::get(collection).ok_or(Error::<T>::BackendInvariant)?;
   ensure!(config.mint_settings.mint_type==pallet_nfts::MintType::Issuer&&config.mint_settings.price.is_none(),Error::<T>::Unsupported);
   let id=backend::allocate_existing(v1::ITEM_CURSOR_KEY,Some(collection),range(v1::ITEM_FIRST,v1::ITEM_LAST),|id|pallet_nfts::Item::<Runtime>::contains_key(collection,id)||pallet_nfts::ItemConfigOf::<Runtime>::contains_key(collection,id),|id|{
    MintGuard::<T>::put((collection,id));
    backend::checked_deposit_change(&who,0,v1::NFT_ITEM_DEPOSIT,||Nfts::mint(RuntimeOrigin::signed(who.clone()),collection,id,MultiAddress::Id(recipient),None)).map_err(|_|DispatchError::Other("allocator mint authority or deposit"))?;
    MintGuard::<T>::kill();Ok(())
   }).map_err(|_|Error::<T>::AllocationFailed)?;
   Self::deposit_event(Event::ItemCreated{collection,item:id,issuer:who});Ok(())
  }
  #[pallet::call_index(6)] #[pallet::weight(super::weight(6))] #[frame_support::transactional]
  pub fn admit_existing_asset(origin:OriginFor<T>,id:u32,minimum:u128)->DispatchResult{
   ensure_root(origin)?;Self::ready()?;
   v1::validate_asset_admission(&v1::AssetAdmission{id,minimum_balance:minimum}).map_err(|_|Error::<T>::Unsupported)?;
   ensure!(!AdmittedAssets::<T>::contains_key(id),Error::<T>::AlreadyInitialized);
   AdmittedAssets::<T>::insert(id,minimum);
   <Api as v1::EraV14AssetsApiV1>::asset_v1(id).map_err(|_|Error::<T>::BackendInvariant)?;
   Self::deposit_event(Event::LegacyAssetAdmitted{id});Ok(())
  }
  #[pallet::call_index(7)] #[pallet::weight(super::weight(7))] #[frame_support::transactional]
  pub fn admit_existing_collection(origin:OriginFor<T>,id:u32,next_item:Option<u32>,evidence:[u8;32])->DispatchResult{
   ensure_root(origin)?;Self::ready()?;ensure!(evidence!=[0;32],Error::<T>::InvalidEvidence);
   ensure!((v1::COLLECTION_FIRST..=v1::COLLECTION_LAST).contains(&id),Error::<T>::Unsupported);
   ensure!(!AdmittedCollections::<T>::contains_key(id)&&!NextItemId::<T>::contains_key(id),Error::<T>::AlreadyInitialized);
   Self::valid_cursor(next_item,v1::ITEM_FIRST,v1::ITEM_LAST)?;
   if let Some(item)=next_item{ensure!(!pallet_nfts::Item::<Runtime>::contains_key(id,item)&&!pallet_nfts::ItemConfigOf::<Runtime>::contains_key(id,item),Error::<T>::Occupied)}
   AdmittedCollections::<T>::insert(id,());
   let record=<Api as v1::EraV14AssetsApiV1>::collection_v1(id).map_err(|_|Error::<T>::BackendInvariant)?;
   ensure!(record.issuer.is_some()&&record.admin.is_some()&&record.freezer.is_some(),Error::<T>::Unsupported);
   let config=pallet_nfts::CollectionConfigOf::<Runtime>::get(id).ok_or(Error::<T>::BackendInvariant)?;
   ensure!(config.mint_settings.mint_type==pallet_nfts::MintType::Issuer&&config.mint_settings.price.is_none()&&!config.has_disabled_setting(pallet_nfts::CollectionSetting::DepositRequired),Error::<T>::Unsupported);
   NextItemId::<T>::insert(id,next_item);ItemInitializationEvidence::<T>::insert(id,evidence);Self::deposit_event(Event::LegacyCollectionAdmitted{id,next_item,evidence});Ok(())
  }
 }
 impl<T:Config> Pallet<T>{
  fn ready()->DispatchResult{ensure!(schema_valid(),Error::<T>::Uninitialized);ensure!(!CreationGuard::<T>::exists()&&!MintGuard::<T>::exists(),Error::<T>::Reentrant);Ok(())}
  fn valid_cursor(cursor:Option<u32>,first:u32,last:u32)->DispatchResult{ensure!(cursor.is_none_or(|id|(first..=last).contains(&id)),Error::<T>::InvalidCursor);Ok(())}
 }
}
pub use pallet::*;
