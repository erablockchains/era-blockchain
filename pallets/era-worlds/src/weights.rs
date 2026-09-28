//! Production weights for `pallet_era_worlds`.
//!
//! Generated with Substrate Benchmark CLI 49.1.0 on 2026-09-08 using 50 steps, 100 repeats,
//! three external repeats, compiled Wasm, a 1,000,000-entry worst-case map, and a 1024 MiB
//! database cache. Both runs used benchmark binary SHA-256
//! `5addf22ed7cb9048380f714a68f4b53060bb6c85bf38df6c4e2434fdd77432e5` and runtime Wasm SHA-256
//! `114c01187283a2045fbc01fa2477d16821133ecfe329e684442e0943a62be9e2`.
//!
//! These constants are the component-wise maxima of independent accepted runs A and B. The raw
//! result SHA-256 values are `40413119249cc5890ae0d01cc3d8ab35663067ad0393895e0f539cf95de7d5d1`
//! and `094b391e86b26d9776ebcb5860c8cdc49c94384201d24653e4101583eee770c9`.
//! The generated proof bounds and database access counts are also selected component-wise.

use core::marker::PhantomData;
use frame_support::{traits::Get, weights::Weight};

pub trait WeightInfo {
    fn register_world() -> Weight;
    fn update_commitment() -> Weight;
    fn deregister_world() -> Weight;
    fn admin_remove_world() -> Weight;
    fn pause() -> Weight;
    fn unpause() -> Weight;
}

/// Benchmark-generated production weights backed by the runtime database schedule.
pub struct SubstrateWeight<T>(PhantomData<T>);

impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
    /// Storage: `EraWorlds::Paused` (r:1 w:0), `EraWorlds::Worlds` (r:1 w:1),
    /// `System::Account` (r:1 w:1).
    fn register_world() -> Weight {
        Weight::from_parts(41_448_000, 3_627)
            .saturating_add(T::DbWeight::get().reads(3))
            .saturating_add(T::DbWeight::get().writes(2))
    }

    /// Storage: `EraWorlds::Paused` (r:1 w:0), `EraWorlds::Worlds` (r:1 w:1).
    fn update_commitment() -> Weight {
        Weight::from_parts(18_886_000, 3_627)
            .saturating_add(T::DbWeight::get().reads(2))
            .saturating_add(T::DbWeight::get().writes(1))
    }

    /// Storage: `EraWorlds::Worlds` (r:1 w:1), `System::Account` (r:1 w:1).
    fn deregister_world() -> Weight {
        Weight::from_parts(43_771_000, 3_627)
            .saturating_add(T::DbWeight::get().reads(2))
            .saturating_add(T::DbWeight::get().writes(2))
    }

    /// Configured fail-closed path for `RegistryAdminOrigin = EnsureNever` (r:0 w:0).
    fn admin_remove_world() -> Weight {
        Weight::from_parts(4_899_000, 0)
    }

    /// Configured fail-closed path for `EmergencyPauseOrigin = EnsureNever` (r:0 w:0).
    fn pause() -> Weight {
        Weight::from_parts(3_045_000, 0)
    }

    /// Configured fail-closed path for `EmergencyPauseOrigin = EnsureNever` (r:0 w:0).
    fn unpause() -> Weight {
        Weight::from_parts(3_036_000, 0)
    }
}
