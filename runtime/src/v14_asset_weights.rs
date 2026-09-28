//! Assets metadata weights for the actual pinned ERA runtime configuration.
//! Generated fixed upper envelopes over the full sampled domains, selected from two independent
//! 50-step, 100-repeat, 3-external-repeat captures. All timing/proof coefficients originate in
//! CLI max-analysis outputs; select-weights.py records evaluation at each domain maximum and
//! proves dominance at every vertex. No extrapolation of successful strings beyond 64 bytes.
//! Evidence: evidence/v14-observer-assets-correction-20260905/selected-weights.json.
//! Both successful string axes are 0..64; rejected vectors cover 65..5242880. Each fixed reader
//! uses stored 64-byte name AND symbol. Both reference time and proof size are charged.
//! All other methods delegate unchanged to the pinned SDK SubstrateWeight (2025-05-18 capture).
//! Force metadata methods still reject at EnsureNever before any metadata access.
//! CLI SHA256: b9443f52eba9da0feeaee9fba76b8aa838659cbe1555eeb0cf4cf81431698093
//! Benchmark Wasm SHA256: 3c40db7382bd2d0236fa7ccabdc222882d7c33577ed31cf6dac37f2d84d2ffff
//! Run A (pair2-a) generated SHA256: 422a4eba6813f289378333641817254ac9062fb5110bbdfbcfca10a0fe79552e
//! Run B (pair2-b) generated SHA256: 8416064cfaddb4a974ea54541a6543668defd549345667060494fe43234a376b
//! Unchanged SDK weights SHA256: bf6556f931e61751a51429d59ef45f36c75e803584dd5f4a0cb3dddf72e1d1eb
//! transfer_ownership alone uses transfer-a / transfer-b (1000 inner repeats each):
//! A SHA256: 6d521253f2f2af580cb67865c3be452ea47d16018b948227bca80e619ec1a83a
//! B SHA256: c465242df3d5a8b3e8659c1ffca9274dc63bed3e9ad5402650220994b044cf76

use core::marker::PhantomData;
use frame_support::{traits::Get, weights::Weight};
use pallet_assets::weights::SubstrateWeight as SdkWeight;

pub struct AssetsWeight<T>(PhantomData<T>);

impl<T: frame_system::Config> AssetsWeight<T> {
    // Fixed envelope: max(run A 46722474, run B 47534215) picoseconds.
    // Full measured domain maximum: {'n': 64, 's': 64}; proof 3675 bytes.
    pub(crate) fn measured_set_metadata_create() -> Weight {
        Weight::from_parts(47534215, 3675)
            .saturating_add(T::DbWeight::get().reads(3))
            .saturating_add(T::DbWeight::get().writes(2))
    }
    // Fixed envelope: max(run A 54753664, run B 50238848) picoseconds.
    // Full measured domain maximum: {'n': 64, 's': 64}; proof 3675 bytes.
    pub(crate) fn measured_set_metadata_replace() -> Weight {
        Weight::from_parts(54753664, 3675)
            .saturating_add(T::DbWeight::get().reads(3))
            .saturating_add(T::DbWeight::get().writes(2))
    }
    // Fixed envelope: max(run A 46811517, run B 47986877) picoseconds.
    // Full measured domain maximum: {'n': 64, 's': 64}; proof 3675 bytes.
    pub(crate) fn measured_set_metadata_grow() -> Weight {
        Weight::from_parts(47986877, 3675)
            .saturating_add(T::DbWeight::get().reads(3))
            .saturating_add(T::DbWeight::get().writes(2))
    }
    // Fixed envelope: max(run A 43301000, run B 43321000) picoseconds.
    // Full measured domain maximum: {}; proof 3675 bytes.
    pub(crate) fn measured_clear_metadata() -> Weight {
        Weight::from_parts(43321000, 3675)
            .saturating_add(T::DbWeight::get().reads(3))
            .saturating_add(T::DbWeight::get().writes(2))
    }
    // Fixed envelope: max(run A 64420000, run B 65533000) picoseconds.
    // Full measured domain maximum: {}; proof 6196 bytes.
    pub(crate) fn measured_transfer_ownership() -> Weight {
        Weight::from_parts(65533000, 6196)
            .saturating_add(T::DbWeight::get().reads(4))
            .saturating_add(T::DbWeight::get().writes(3))
    }
    // Fixed envelope: max(run A 46588000, run B 44662000) picoseconds.
    // Full measured domain maximum: {}; proof 3675 bytes.
    pub(crate) fn measured_finish_destroy() -> Weight {
        Weight::from_parts(46588000, 3675)
            .saturating_add(T::DbWeight::get().reads(3))
            .saturating_add(T::DbWeight::get().writes(3))
    }
    // Fixed envelope: max(run A 908177517, run B 912296100) picoseconds.
    // Full measured domain maximum: {'n': 5242880}; proof 0 bytes.
    pub(crate) fn measured_reject_name() -> Weight {
        Weight::from_parts(912296100, 0)
    }
    // Fixed envelope: max(run A 896396438, run B 907423558) picoseconds.
    // Full measured domain maximum: {'s': 5242880}; proof 0 bytes.
    pub(crate) fn measured_reject_symbol() -> Weight {
        Weight::from_parts(907423558, 0)
    }
}

impl<T: frame_system::Config> pallet_assets::WeightInfo for AssetsWeight<T> {
    fn create() -> Weight {
        <SdkWeight<T> as pallet_assets::WeightInfo>::create().saturating_add(crate::v14_allocator::legacy_create_weight())
    }
    fn force_create() -> Weight {
        <SdkWeight<T> as pallet_assets::WeightInfo>::force_create()
    }
    fn start_destroy() -> Weight {
        <SdkWeight<T> as pallet_assets::WeightInfo>::start_destroy()
    }
    fn destroy_accounts(c: u32) -> Weight {
        <SdkWeight<T> as pallet_assets::WeightInfo>::destroy_accounts(c)
    }
    fn destroy_approvals(a: u32) -> Weight {
        <SdkWeight<T> as pallet_assets::WeightInfo>::destroy_approvals(a)
    }
    fn finish_destroy() -> Weight {
        Self::measured_finish_destroy()
            .max(<SdkWeight<T> as pallet_assets::WeightInfo>::finish_destroy())
    }
    fn mint() -> Weight {
        <SdkWeight<T> as pallet_assets::WeightInfo>::mint()
    }
    fn burn() -> Weight {
        <SdkWeight<T> as pallet_assets::WeightInfo>::burn()
    }
    fn transfer() -> Weight {
        <SdkWeight<T> as pallet_assets::WeightInfo>::transfer()
    }
    fn transfer_keep_alive() -> Weight {
        <SdkWeight<T> as pallet_assets::WeightInfo>::transfer_keep_alive()
    }
    fn force_transfer() -> Weight {
        <SdkWeight<T> as pallet_assets::WeightInfo>::force_transfer()
    }
    fn freeze() -> Weight {
        <SdkWeight<T> as pallet_assets::WeightInfo>::freeze()
    }
    fn thaw() -> Weight {
        <SdkWeight<T> as pallet_assets::WeightInfo>::thaw()
    }
    fn freeze_asset() -> Weight {
        <SdkWeight<T> as pallet_assets::WeightInfo>::freeze_asset()
    }
    fn thaw_asset() -> Weight {
        <SdkWeight<T> as pallet_assets::WeightInfo>::thaw_asset()
    }
    fn transfer_ownership() -> Weight {
        Self::measured_transfer_ownership()
            .max(<SdkWeight<T> as pallet_assets::WeightInfo>::transfer_ownership())
    }
    fn set_team() -> Weight {
        <SdkWeight<T> as pallet_assets::WeightInfo>::set_team()
    }
    fn set_metadata(n: u32, s: u32) -> Weight {
        let measured = if n > 64 {
            Self::measured_reject_name()
        } else if s > 64 {
            Self::measured_reject_symbol()
        } else {
            Self::measured_set_metadata_create()
                .max(Self::measured_set_metadata_replace())
                .max(Self::measured_set_metadata_grow())
        };
        measured.max(<SdkWeight<T> as pallet_assets::WeightInfo>::set_metadata(
            n, s,
        ))
    }
    fn clear_metadata() -> Weight {
        Self::measured_clear_metadata()
            .max(<SdkWeight<T> as pallet_assets::WeightInfo>::clear_metadata())
    }
    fn force_set_metadata(n: u32, s: u32) -> Weight {
        <SdkWeight<T> as pallet_assets::WeightInfo>::force_set_metadata(n, s)
    }
    fn force_clear_metadata() -> Weight {
        <SdkWeight<T> as pallet_assets::WeightInfo>::force_clear_metadata()
    }
    fn force_asset_status() -> Weight {
        <SdkWeight<T> as pallet_assets::WeightInfo>::force_asset_status()
    }
    fn approve_transfer() -> Weight {
        <SdkWeight<T> as pallet_assets::WeightInfo>::approve_transfer()
    }
    fn transfer_approved() -> Weight {
        <SdkWeight<T> as pallet_assets::WeightInfo>::transfer_approved()
    }
    fn cancel_approval() -> Weight {
        <SdkWeight<T> as pallet_assets::WeightInfo>::cancel_approval()
    }
    fn force_cancel_approval() -> Weight {
        <SdkWeight<T> as pallet_assets::WeightInfo>::force_cancel_approval()
    }
    fn set_min_balance() -> Weight {
        <SdkWeight<T> as pallet_assets::WeightInfo>::set_min_balance()
    }
    fn touch() -> Weight {
        <SdkWeight<T> as pallet_assets::WeightInfo>::touch()
    }
    fn touch_other() -> Weight {
        <SdkWeight<T> as pallet_assets::WeightInfo>::touch_other()
    }
    fn refund() -> Weight {
        <SdkWeight<T> as pallet_assets::WeightInfo>::refund()
    }
    fn refund_other() -> Weight {
        <SdkWeight<T> as pallet_assets::WeightInfo>::refund_other()
    }
    fn block() -> Weight {
        <SdkWeight<T> as pallet_assets::WeightInfo>::block()
    }
    fn transfer_all() -> Weight {
        <SdkWeight<T> as pallet_assets::WeightInfo>::transfer_all()
    }
    fn total_issuance() -> Weight {
        <SdkWeight<T> as pallet_assets::WeightInfo>::total_issuance()
    }
    fn balance() -> Weight {
        <SdkWeight<T> as pallet_assets::WeightInfo>::balance()
    }
    fn allowance() -> Weight {
        <SdkWeight<T> as pallet_assets::WeightInfo>::allowance()
    }
}
