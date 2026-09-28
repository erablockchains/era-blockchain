//! Production weights for `pallet_security_budget`.
//!
//! Generated with Substrate Benchmark CLI 49.1.0 on 2026-09-06 using 50 steps, 100 repeats,
//! three external repeats, compiled Wasm, a 1,000,000-entry worst-case map, and a 1024 MiB
//! database cache. Benchmark binary SHA-256:
//! `93a81edff8fbe575601adedb871854b13f194958d4e9e9d40b0ced2c901a17f9`.
//!
//! These values are deterministic component-wise maxima of accepted Runs A and B. Both runs use
//! identical maximum proof-size and database-access models. Routing parameter `z` is the measured
//! 0..=100 fraction of the maximum accepted amount; production uses 100.

use core::marker::PhantomData;
use frame_support::{traits::Get, weights::Weight};

pub trait WeightInfo {
    fn activate() -> Weight;
    fn claim_reward_page(n: u32) -> Weight;
    fn retry_normal_fee() -> Weight;
    fn retry_tip() -> Weight;
    fn route_normal_fee(z: u32) -> Weight;
    fn route_tip_author(z: u32) -> Weight;
    fn route_tip_fallback(z: u32) -> Weight;
    fn on_initialize(v: u32) -> Weight;
    fn migration_initialize() -> Weight;
}

/// Benchmark-generated production weights backed by the runtime database schedule.
pub struct SubstrateWeight<T>(PhantomData<T>);

impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
    fn activate() -> Weight {
        Weight::from_parts(112842000, 13825)
            .saturating_add(T::DbWeight::get().reads(21))
            .saturating_add(T::DbWeight::get().writes(12))
    }

    fn claim_reward_page(n: u32) -> Weight {
        Weight::from_parts(239194758, 6196)
            .saturating_add(Weight::from_parts(63334452, 2603).saturating_mul(n.into()))
            .saturating_add(T::DbWeight::get().reads(22))
            .saturating_add(T::DbWeight::get().reads((1_u64).saturating_mul(n.into())))
            .saturating_add(T::DbWeight::get().writes(9))
            .saturating_add(T::DbWeight::get().writes((1_u64).saturating_mul(n.into())))
    }

    fn retry_normal_fee() -> Weight {
        Weight::from_parts(167233000, 8799)
            .saturating_add(T::DbWeight::get().reads(14))
            .saturating_add(T::DbWeight::get().writes(9))
    }

    fn retry_tip() -> Weight {
        Weight::from_parts(178714000, 8799)
            .saturating_add(T::DbWeight::get().reads(15))
            .saturating_add(T::DbWeight::get().writes(9))
    }

    fn route_normal_fee(z: u32) -> Weight {
        Weight::from_parts(160938493, 8799)
            .saturating_add(Weight::from_parts(276575, 0).saturating_mul(z.into()))
            .saturating_add(T::DbWeight::get().reads(15))
            .saturating_add(T::DbWeight::get().writes(9))
    }

    fn route_tip_author(z: u32) -> Weight {
        Weight::from_parts(160112643, 8799)
            .saturating_add(Weight::from_parts(231620, 0).saturating_mul(z.into()))
            .saturating_add(T::DbWeight::get().reads(11))
            .saturating_add(T::DbWeight::get().writes(7))
    }

    fn route_tip_fallback(z: u32) -> Weight {
        Weight::from_parts(162574285, 8799)
            .saturating_add(Weight::from_parts(194501, 0).saturating_mul(z.into()))
            .saturating_add(T::DbWeight::get().reads(16))
            .saturating_add(T::DbWeight::get().writes(9))
    }

    fn on_initialize(v: u32) -> Weight {
        Weight::from_parts(179871777, 6196)
            .saturating_add(Weight::from_parts(18061951, 2567).saturating_mul(v.into()))
            .saturating_add(T::DbWeight::get().reads(24))
            .saturating_add(T::DbWeight::get().reads((2_u64).saturating_mul(v.into())))
            .saturating_add(T::DbWeight::get().writes(17))
            .saturating_add(T::DbWeight::get().writes((1_u64).saturating_mul(v.into())))
    }

    fn migration_initialize() -> Weight {
        Weight::from_parts(9458000, 1501)
            .saturating_add(T::DbWeight::get().reads(2))
            .saturating_add(T::DbWeight::get().writes(4))
    }
}
