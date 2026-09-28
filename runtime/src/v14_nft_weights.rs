//! Retain existing NFT measured formulas and add the measured allocation guard envelope only to affected paths.
use core::marker::PhantomData;
use frame_support::weights::Weight;
pub struct NftsWeight<T>(PhantomData<T>);
impl<T:frame_system::Config> pallet_nfts::WeightInfo for NftsWeight<T>{
 fn start_collection_retirement()->Weight { <pallet_nfts::weights::SubstrateWeight<T> as pallet_nfts::WeightInfo>::start_collection_retirement() }
 fn continue_collection_retirement(_limit: u32)->Weight { <pallet_nfts::weights::SubstrateWeight<T> as pallet_nfts::WeightInfo>::continue_collection_retirement(_limit) }
 fn continue_delegate_cleanup(_limit: u32)->Weight { <pallet_nfts::weights::SubstrateWeight<T> as pallet_nfts::WeightInfo>::continue_delegate_cleanup(_limit) }
 fn create()->Weight { <pallet_nfts::weights::SubstrateWeight<T> as pallet_nfts::WeightInfo>::create().saturating_add(crate::v14_allocator::legacy_create_weight()) }
 fn force_create()->Weight { <pallet_nfts::weights::SubstrateWeight<T> as pallet_nfts::WeightInfo>::force_create() }
 fn destroy(m: u32, c: u32, a: u32, )->Weight { <pallet_nfts::weights::SubstrateWeight<T> as pallet_nfts::WeightInfo>::destroy(m,c,a) }
 fn mint()->Weight { <pallet_nfts::weights::SubstrateWeight<T> as pallet_nfts::WeightInfo>::mint().saturating_add(crate::v14_allocator::legacy_item_weight()) }
 fn force_mint()->Weight { <pallet_nfts::weights::SubstrateWeight<T> as pallet_nfts::WeightInfo>::force_mint().saturating_add(crate::v14_allocator::legacy_item_weight()) }
 fn burn()->Weight { <pallet_nfts::weights::SubstrateWeight<T> as pallet_nfts::WeightInfo>::burn() }
 fn transfer()->Weight { <pallet_nfts::weights::SubstrateWeight<T> as pallet_nfts::WeightInfo>::transfer() }
 fn redeposit(i: u32, )->Weight { <pallet_nfts::weights::SubstrateWeight<T> as pallet_nfts::WeightInfo>::redeposit(i) }
 fn lock_item_transfer()->Weight { <pallet_nfts::weights::SubstrateWeight<T> as pallet_nfts::WeightInfo>::lock_item_transfer() }
 fn unlock_item_transfer()->Weight { <pallet_nfts::weights::SubstrateWeight<T> as pallet_nfts::WeightInfo>::unlock_item_transfer() }
 fn lock_collection()->Weight { <pallet_nfts::weights::SubstrateWeight<T> as pallet_nfts::WeightInfo>::lock_collection() }
 fn transfer_ownership()->Weight { <pallet_nfts::weights::SubstrateWeight<T> as pallet_nfts::WeightInfo>::transfer_ownership() }
 fn set_team()->Weight { <pallet_nfts::weights::SubstrateWeight<T> as pallet_nfts::WeightInfo>::set_team().saturating_add(crate::v14_allocator::legacy_item_weight()) }
 fn force_collection_owner()->Weight { <pallet_nfts::weights::SubstrateWeight<T> as pallet_nfts::WeightInfo>::force_collection_owner() }
 fn force_collection_config()->Weight { <pallet_nfts::weights::SubstrateWeight<T> as pallet_nfts::WeightInfo>::force_collection_config() }
 fn lock_item_properties()->Weight { <pallet_nfts::weights::SubstrateWeight<T> as pallet_nfts::WeightInfo>::lock_item_properties() }
 fn set_attribute()->Weight { <pallet_nfts::weights::SubstrateWeight<T> as pallet_nfts::WeightInfo>::set_attribute() }
 fn force_set_attribute()->Weight { <pallet_nfts::weights::SubstrateWeight<T> as pallet_nfts::WeightInfo>::force_set_attribute() }
 fn clear_attribute()->Weight { <pallet_nfts::weights::SubstrateWeight<T> as pallet_nfts::WeightInfo>::clear_attribute() }
 fn approve_item_attributes()->Weight { <pallet_nfts::weights::SubstrateWeight<T> as pallet_nfts::WeightInfo>::approve_item_attributes() }
 fn cancel_item_attributes_approval(n: u32, )->Weight { <pallet_nfts::weights::SubstrateWeight<T> as pallet_nfts::WeightInfo>::cancel_item_attributes_approval(n) }
 fn set_metadata()->Weight { <pallet_nfts::weights::SubstrateWeight<T> as pallet_nfts::WeightInfo>::set_metadata() }
 fn clear_metadata()->Weight { <pallet_nfts::weights::SubstrateWeight<T> as pallet_nfts::WeightInfo>::clear_metadata() }
 fn set_collection_metadata()->Weight { <pallet_nfts::weights::SubstrateWeight<T> as pallet_nfts::WeightInfo>::set_collection_metadata() }
 fn clear_collection_metadata()->Weight { <pallet_nfts::weights::SubstrateWeight<T> as pallet_nfts::WeightInfo>::clear_collection_metadata() }
 fn approve_transfer()->Weight { <pallet_nfts::weights::SubstrateWeight<T> as pallet_nfts::WeightInfo>::approve_transfer() }
 fn cancel_approval()->Weight { <pallet_nfts::weights::SubstrateWeight<T> as pallet_nfts::WeightInfo>::cancel_approval() }
 fn clear_all_transfer_approvals()->Weight { <pallet_nfts::weights::SubstrateWeight<T> as pallet_nfts::WeightInfo>::clear_all_transfer_approvals() }
 fn set_accept_ownership()->Weight { <pallet_nfts::weights::SubstrateWeight<T> as pallet_nfts::WeightInfo>::set_accept_ownership() }
 fn set_collection_max_supply()->Weight { <pallet_nfts::weights::SubstrateWeight<T> as pallet_nfts::WeightInfo>::set_collection_max_supply() }
 fn update_mint_settings()->Weight { <pallet_nfts::weights::SubstrateWeight<T> as pallet_nfts::WeightInfo>::update_mint_settings().saturating_add(crate::v14_allocator::legacy_item_weight()) }
 fn set_price()->Weight { <pallet_nfts::weights::SubstrateWeight<T> as pallet_nfts::WeightInfo>::set_price() }
 fn buy_item()->Weight { <pallet_nfts::weights::SubstrateWeight<T> as pallet_nfts::WeightInfo>::buy_item() }
 fn pay_tips(n: u32, )->Weight { <pallet_nfts::weights::SubstrateWeight<T> as pallet_nfts::WeightInfo>::pay_tips(n) }
 fn create_swap()->Weight { <pallet_nfts::weights::SubstrateWeight<T> as pallet_nfts::WeightInfo>::create_swap() }
 fn cancel_swap()->Weight { <pallet_nfts::weights::SubstrateWeight<T> as pallet_nfts::WeightInfo>::cancel_swap() }
 fn claim_swap()->Weight { <pallet_nfts::weights::SubstrateWeight<T> as pallet_nfts::WeightInfo>::claim_swap() }
 fn mint_pre_signed(n: u32, )->Weight { <pallet_nfts::weights::SubstrateWeight<T> as pallet_nfts::WeightInfo>::mint_pre_signed(n).saturating_add(crate::v14_allocator::legacy_item_weight()) }
 fn set_attributes_pre_signed(n: u32, )->Weight { <pallet_nfts::weights::SubstrateWeight<T> as pallet_nfts::WeightInfo>::set_attributes_pre_signed(n) }
}
