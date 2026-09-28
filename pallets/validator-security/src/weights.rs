
//! Production weights for `era_validator_security::observation`.
//!
//! Generated from two independent 50-step/20-repeat captures. Each executable ref-time base,
//! ref-time slope, and proof-size component below is the larger of runs A and B, yielding a
//! conservative model that dominates both captures throughout every accepted linear range.
//! The remaining generator comments record the run B provenance.
//!
//! THIS FILE WAS AUTO-GENERATED USING THE SUBSTRATE BENCHMARK CLI VERSION 49.1.0
//! DATE: 2026-09-04, STEPS: `50`, REPEAT: `20`, LOW RANGE: `[]`, HIGH RANGE: `[]`
//! WORST CASE MAP SIZE: `1000000`
//! HOSTNAME: `[private-benchmark-host]`, CPU: `AMD EPYC-Milan Processor`
//! WASM-EXECUTION: `Compiled`, CHAIN: `None`, DB CACHE: 1024

// Executed Command:
// [private-benchmark-artifact-path]
// benchmark
// pallet
// --runtime
// [private-benchmark-artifact-path]
// --genesis-builder
// none
// --pallets
// era_validator_security::observation
// --extrinsic
// *
// --steps
// 50
// --repeat
// 20
// --base-path
// [disposable-benchmark-path]
// --json-file
// [disposable-benchmark-path]
// --output
// [disposable-benchmark-path]
// --output-analysis
// max
// --output-pov-analysis
// max

#![cfg_attr(rustfmt, rustfmt_skip)]
#![allow(unused_parens)]
#![allow(unused_imports)]
#![allow(missing_docs)]

use frame_support::{traits::Get, weights::Weight};
use core::marker::PhantomData;

/// Weight functions for `era_validator_security::observation`.
pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> crate::observation::WeightInfo for SubstrateWeight<T> {
	/// Storage: `Session::CurrentIndex` (r:1 w:0)
	/// Proof: `Session::CurrentIndex` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `Historical::HistoricalSessions` (r:1 w:0)
	/// Proof: `Historical::HistoricalSessions` (`max_values`: None, `max_size`: Some(48), added: 2523, mode: `MaxEncodedLen`)
	/// Storage: UNKNOWN KEY `0xf8d03e8b4e6f1e1952503771969ef3994e7b9012096b41c4eb3aaf947f6ea429` (r:1 w:0)
	/// Proof: UNKNOWN KEY `0xf8d03e8b4e6f1e1952503771969ef3994e7b9012096b41c4eb3aaf947f6ea429` (r:1 w:0)
	/// Storage: `ValidatorSecurity::RingHead` (r:1 w:1)
	/// Proof: `ValidatorSecurity::RingHead` (`max_values`: Some(1), `max_size`: Some(4), added: 499, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorSecurity::RingTail` (r:1 w:1)
	/// Proof: `ValidatorSecurity::RingTail` (`max_values`: Some(1), `max_size`: Some(4), added: 499, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorSecurity::ObservationCount` (r:1 w:1)
	/// Proof: `ValidatorSecurity::ObservationCount` (`max_values`: Some(1), `max_size`: Some(4), added: 499, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorSecurity::OffenceIdentityIndex` (r:65 w:65)
	/// Proof: `ValidatorSecurity::OffenceIdentityIndex` (`max_values`: Some(8192), `max_size`: Some(84), added: 2064, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorSecurity::PendingEvidenceContext` (r:1 w:1)
	/// Proof: `ValidatorSecurity::PendingEvidenceContext` (`max_values`: Some(1), `max_size`: Some(33), added: 528, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorSecurity::PendingAdapterError` (r:1 w:0)
	/// Proof: `ValidatorSecurity::PendingAdapterError` (`max_values`: Some(1), `max_size`: Some(1), added: 496, mode: `MaxEncodedLen`)
	/// Storage: `Babe::GenesisSlot` (r:1 w:0)
	/// Proof: `Babe::GenesisSlot` (`max_values`: Some(1), `max_size`: Some(8), added: 503, mode: `MaxEncodedLen`)
	/// Storage: `Babe::SkippedEpochs` (r:1 w:0)
	/// Proof: `Babe::SkippedEpochs` (`max_values`: Some(1), `max_size`: Some(1202), added: 1697, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorSecurity::Observations` (r:65 w:65)
	/// Proof: `ValidatorSecurity::Observations` (`max_values`: Some(8192), `max_size`: Some(149), added: 2129, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorSecurity::NewObservationsInBlock` (r:1 w:1)
	/// Proof: `ValidatorSecurity::NewObservationsInBlock` (`max_values`: Some(1), `max_size`: Some(8), added: 503, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorSecurity::ObservationRing` (r:65 w:65)
	/// Proof: `ValidatorSecurity::ObservationRing` (`max_values`: Some(8192), `max_size`: Some(80), added: 2060, mode: `MaxEncodedLen`)
	/// The range of component `v` is `[100, 1000]`.
	fn babe_new(v: u32, ) -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `20806`
		//  Estimated: `139375`
		// Minimum execution time: 2_017_001_000 picoseconds.
		Weight::from_parts(2_179_073_285, 0)
			.saturating_add(Weight::from_parts(0, 139375))
			// Standard Error: 8_160
			.saturating_add(Weight::from_parts(44_000, 0).saturating_mul(v.into()))
			.saturating_add(T::DbWeight::get().reads(206))
			.saturating_add(T::DbWeight::get().writes(200))
	}
	/// Storage: `Session::CurrentIndex` (r:1 w:0)
	/// Proof: `Session::CurrentIndex` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `Historical::HistoricalSessions` (r:1 w:0)
	/// Proof: `Historical::HistoricalSessions` (`max_values`: None, `max_size`: Some(48), added: 2523, mode: `MaxEncodedLen`)
	/// Storage: UNKNOWN KEY `0xf8d03e8b4e6f1e1952503771969ef3994e7b9012096b41c4eb3aaf947f6ea429` (r:1 w:0)
	/// Proof: UNKNOWN KEY `0xf8d03e8b4e6f1e1952503771969ef3994e7b9012096b41c4eb3aaf947f6ea429` (r:1 w:0)
	/// Storage: `ValidatorSecurity::RingHead` (r:1 w:0)
	/// Proof: `ValidatorSecurity::RingHead` (`max_values`: Some(1), `max_size`: Some(4), added: 499, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorSecurity::RingTail` (r:1 w:0)
	/// Proof: `ValidatorSecurity::RingTail` (`max_values`: Some(1), `max_size`: Some(4), added: 499, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorSecurity::ObservationCount` (r:1 w:0)
	/// Proof: `ValidatorSecurity::ObservationCount` (`max_values`: Some(1), `max_size`: Some(4), added: 499, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorSecurity::OffenceIdentityIndex` (r:1 w:0)
	/// Proof: `ValidatorSecurity::OffenceIdentityIndex` (`max_values`: Some(8192), `max_size`: Some(84), added: 2064, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorSecurity::Observations` (r:1 w:0)
	/// Proof: `ValidatorSecurity::Observations` (`max_values`: Some(8192), `max_size`: Some(149), added: 2129, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorSecurity::ObservationRing` (r:1 w:0)
	/// Proof: `ValidatorSecurity::ObservationRing` (`max_values`: Some(8192), `max_size`: Some(80), added: 2060, mode: `MaxEncodedLen`)
	/// The range of component `v` is `[100, 1000]`.
	fn babe_duplicate(v: u32, ) -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `562`
		//  Estimated: `4027`
		// Minimum execution time: 80_832_000 picoseconds.
		Weight::from_parts(91_017_982, 0)
			.saturating_add(Weight::from_parts(0, 4027))
			// Standard Error: 968
			.saturating_add(Weight::from_parts(18_485, 0).saturating_mul(v.into()))
			.saturating_add(T::DbWeight::get().reads(9))
	}
	/// Storage: `Session::CurrentIndex` (r:1 w:0)
	/// Proof: `Session::CurrentIndex` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `Historical::HistoricalSessions` (r:1 w:0)
	/// Proof: `Historical::HistoricalSessions` (`max_values`: None, `max_size`: Some(48), added: 2523, mode: `MaxEncodedLen`)
	/// Storage: UNKNOWN KEY `0xf8d03e8b4e6f1e1952503771969ef3994e7b9012096b41c4eb3aaf947f6ea429` (r:1 w:0)
	/// Proof: UNKNOWN KEY `0xf8d03e8b4e6f1e1952503771969ef3994e7b9012096b41c4eb3aaf947f6ea429` (r:1 w:0)
	/// Storage: `ValidatorSecurity::RingHead` (r:1 w:0)
	/// Proof: `ValidatorSecurity::RingHead` (`max_values`: Some(1), `max_size`: Some(4), added: 499, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorSecurity::RingTail` (r:1 w:0)
	/// Proof: `ValidatorSecurity::RingTail` (`max_values`: Some(1), `max_size`: Some(4), added: 499, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorSecurity::ObservationCount` (r:1 w:0)
	/// Proof: `ValidatorSecurity::ObservationCount` (`max_values`: Some(1), `max_size`: Some(4), added: 499, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorSecurity::OffenceIdentityIndex` (r:1 w:0)
	/// Proof: `ValidatorSecurity::OffenceIdentityIndex` (`max_values`: Some(8192), `max_size`: Some(84), added: 2064, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorSecurity::PendingEvidenceContext` (r:1 w:0)
	/// Proof: `ValidatorSecurity::PendingEvidenceContext` (`max_values`: Some(1), `max_size`: Some(33), added: 528, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorSecurity::PendingAdapterError` (r:1 w:0)
	/// Proof: `ValidatorSecurity::PendingAdapterError` (`max_values`: Some(1), `max_size`: Some(1), added: 496, mode: `MaxEncodedLen`)
	/// The range of component `v` is `[100, 1000]`.
	fn babe_invalid(v: u32, ) -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `90`
		//  Estimated: `3555`
		// Minimum execution time: 94_787_000 picoseconds.
		Weight::from_parts(106_061_027, 0)
			.saturating_add(Weight::from_parts(0, 3555))
			// Standard Error: 957
			.saturating_add(Weight::from_parts(17_347, 0).saturating_mul(v.into()))
			.saturating_add(T::DbWeight::get().reads(9))
	}
	/// Storage: `Session::CurrentIndex` (r:1 w:0)
	/// Proof: `Session::CurrentIndex` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `Historical::HistoricalSessions` (r:1 w:0)
	/// Proof: `Historical::HistoricalSessions` (`max_values`: None, `max_size`: Some(48), added: 2523, mode: `MaxEncodedLen`)
	/// Storage: UNKNOWN KEY `0xf8d03e8b4e6f1e1952503771969ef3994e7b9012096b41c4eb3aaf947f6ea429` (r:1 w:0)
	/// Proof: UNKNOWN KEY `0xf8d03e8b4e6f1e1952503771969ef3994e7b9012096b41c4eb3aaf947f6ea429` (r:1 w:0)
	/// Storage: `ValidatorSecurity::RingHead` (r:1 w:0)
	/// Proof: `ValidatorSecurity::RingHead` (`max_values`: Some(1), `max_size`: Some(4), added: 499, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorSecurity::RingTail` (r:1 w:0)
	/// Proof: `ValidatorSecurity::RingTail` (`max_values`: Some(1), `max_size`: Some(4), added: 499, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorSecurity::ObservationCount` (r:1 w:0)
	/// Proof: `ValidatorSecurity::ObservationCount` (`max_values`: Some(1), `max_size`: Some(4), added: 499, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorSecurity::OffenceIdentityIndex` (r:2 w:0)
	/// Proof: `ValidatorSecurity::OffenceIdentityIndex` (`max_values`: Some(8192), `max_size`: Some(84), added: 2064, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorSecurity::PendingEvidenceContext` (r:1 w:0)
	/// Proof: `ValidatorSecurity::PendingEvidenceContext` (`max_values`: Some(1), `max_size`: Some(33), added: 528, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorSecurity::PendingAdapterError` (r:1 w:0)
	/// Proof: `ValidatorSecurity::PendingAdapterError` (`max_values`: Some(1), `max_size`: Some(1), added: 496, mode: `MaxEncodedLen`)
	/// Storage: `Babe::GenesisSlot` (r:1 w:0)
	/// Proof: `Babe::GenesisSlot` (`max_values`: Some(1), `max_size`: Some(8), added: 503, mode: `MaxEncodedLen`)
	/// Storage: `Babe::SkippedEpochs` (r:1 w:0)
	/// Proof: `Babe::SkippedEpochs` (`max_values`: Some(1), `max_size`: Some(1202), added: 1697, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorSecurity::Observations` (r:2 w:0)
	/// Proof: `ValidatorSecurity::Observations` (`max_values`: Some(8192), `max_size`: Some(149), added: 2129, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorSecurity::NewObservationsInBlock` (r:1 w:0)
	/// Proof: `ValidatorSecurity::NewObservationsInBlock` (`max_values`: Some(1), `max_size`: Some(8), added: 503, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorSecurity::ObservationRing` (r:1 w:0)
	/// Proof: `ValidatorSecurity::ObservationRing` (`max_values`: Some(8192), `max_size`: Some(80), added: 2060, mode: `MaxEncodedLen`)
	/// The range of component `v` is `[100, 1000]`.
	fn babe_capacity_full(v: u32, ) -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `7159`
		//  Estimated: `10624`
		// Minimum execution time: 428_913_000 picoseconds.
		Weight::from_parts(486_457_952, 0)
			.saturating_add(Weight::from_parts(0, 10624))
			// Standard Error: 2_602
			.saturating_add(Weight::from_parts(15_923, 0).saturating_mul(v.into()))
			.saturating_add(T::DbWeight::get().reads(16))
	}
	/// Storage: `Session::CurrentIndex` (r:1 w:0)
	/// Proof: `Session::CurrentIndex` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `Historical::HistoricalSessions` (r:1 w:0)
	/// Proof: `Historical::HistoricalSessions` (`max_values`: None, `max_size`: Some(48), added: 2523, mode: `MaxEncodedLen`)
	/// Storage: UNKNOWN KEY `0xf8d03e8b4e6f1e1952503771969ef3994e7b9012096b41c4eb3aaf947f6ea429` (r:1 w:0)
	/// Proof: UNKNOWN KEY `0xf8d03e8b4e6f1e1952503771969ef3994e7b9012096b41c4eb3aaf947f6ea429` (r:1 w:0)
	/// Storage: `ValidatorSecurity::RingHead` (r:1 w:1)
	/// Proof: `ValidatorSecurity::RingHead` (`max_values`: Some(1), `max_size`: Some(4), added: 499, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorSecurity::RingTail` (r:1 w:1)
	/// Proof: `ValidatorSecurity::RingTail` (`max_values`: Some(1), `max_size`: Some(4), added: 499, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorSecurity::ObservationCount` (r:1 w:1)
	/// Proof: `ValidatorSecurity::ObservationCount` (`max_values`: Some(1), `max_size`: Some(4), added: 499, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorSecurity::OffenceIdentityIndex` (r:65 w:65)
	/// Proof: `ValidatorSecurity::OffenceIdentityIndex` (`max_values`: Some(8192), `max_size`: Some(84), added: 2064, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorSecurity::PendingEvidenceContext` (r:1 w:1)
	/// Proof: `ValidatorSecurity::PendingEvidenceContext` (`max_values`: Some(1), `max_size`: Some(33), added: 528, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorSecurity::PendingAdapterError` (r:1 w:0)
	/// Proof: `ValidatorSecurity::PendingAdapterError` (`max_values`: Some(1), `max_size`: Some(1), added: 496, mode: `MaxEncodedLen`)
	/// Storage: `Grandpa::SetIdSession` (r:1 w:0)
	/// Proof: `Grandpa::SetIdSession` (`max_values`: None, `max_size`: Some(20), added: 2495, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorSecurity::Observations` (r:65 w:65)
	/// Proof: `ValidatorSecurity::Observations` (`max_values`: Some(8192), `max_size`: Some(149), added: 2129, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorSecurity::NewObservationsInBlock` (r:1 w:1)
	/// Proof: `ValidatorSecurity::NewObservationsInBlock` (`max_values`: Some(1), `max_size`: Some(8), added: 503, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorSecurity::ObservationRing` (r:65 w:65)
	/// Proof: `ValidatorSecurity::ObservationRing` (`max_values`: Some(8192), `max_size`: Some(80), added: 2060, mode: `MaxEncodedLen`)
	/// The range of component `v` is `[100, 1000]`.
	fn grandpa_new(v: u32, ) -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `20818`
		//  Estimated: `139375`
		// Minimum execution time: 2_053_783_000 picoseconds.
		Weight::from_parts(2_153_033_890, 0)
			.saturating_add(Weight::from_parts(0, 139375))
			// Standard Error: 8_797
			.saturating_add(Weight::from_parts(85_135, 0).saturating_mul(v.into()))
			.saturating_add(T::DbWeight::get().reads(205))
			.saturating_add(T::DbWeight::get().writes(200))
	}
	/// Storage: `Session::CurrentIndex` (r:1 w:0)
	/// Proof: `Session::CurrentIndex` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `Historical::HistoricalSessions` (r:1 w:0)
	/// Proof: `Historical::HistoricalSessions` (`max_values`: None, `max_size`: Some(48), added: 2523, mode: `MaxEncodedLen`)
	/// Storage: UNKNOWN KEY `0xf8d03e8b4e6f1e1952503771969ef3994e7b9012096b41c4eb3aaf947f6ea429` (r:1 w:0)
	/// Proof: UNKNOWN KEY `0xf8d03e8b4e6f1e1952503771969ef3994e7b9012096b41c4eb3aaf947f6ea429` (r:1 w:0)
	/// Storage: `ValidatorSecurity::RingHead` (r:1 w:0)
	/// Proof: `ValidatorSecurity::RingHead` (`max_values`: Some(1), `max_size`: Some(4), added: 499, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorSecurity::RingTail` (r:1 w:0)
	/// Proof: `ValidatorSecurity::RingTail` (`max_values`: Some(1), `max_size`: Some(4), added: 499, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorSecurity::ObservationCount` (r:1 w:0)
	/// Proof: `ValidatorSecurity::ObservationCount` (`max_values`: Some(1), `max_size`: Some(4), added: 499, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorSecurity::OffenceIdentityIndex` (r:1 w:0)
	/// Proof: `ValidatorSecurity::OffenceIdentityIndex` (`max_values`: Some(8192), `max_size`: Some(84), added: 2064, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorSecurity::Observations` (r:1 w:0)
	/// Proof: `ValidatorSecurity::Observations` (`max_values`: Some(8192), `max_size`: Some(149), added: 2129, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorSecurity::ObservationRing` (r:1 w:0)
	/// Proof: `ValidatorSecurity::ObservationRing` (`max_values`: Some(8192), `max_size`: Some(80), added: 2060, mode: `MaxEncodedLen`)
	/// The range of component `v` is `[100, 1000]`.
	fn grandpa_duplicate(v: u32, ) -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `570`
		//  Estimated: `4035`
		// Minimum execution time: 80_250_000 picoseconds.
		Weight::from_parts(89_473_071, 0)
			.saturating_add(Weight::from_parts(0, 4035))
			// Standard Error: 1_063
			.saturating_add(Weight::from_parts(13_254, 0).saturating_mul(v.into()))
			.saturating_add(T::DbWeight::get().reads(9))
	}
	/// Storage: `Session::CurrentIndex` (r:1 w:0)
	/// Proof: `Session::CurrentIndex` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `Historical::HistoricalSessions` (r:1 w:0)
	/// Proof: `Historical::HistoricalSessions` (`max_values`: None, `max_size`: Some(48), added: 2523, mode: `MaxEncodedLen`)
	/// Storage: UNKNOWN KEY `0xf8d03e8b4e6f1e1952503771969ef3994e7b9012096b41c4eb3aaf947f6ea429` (r:1 w:0)
	/// Proof: UNKNOWN KEY `0xf8d03e8b4e6f1e1952503771969ef3994e7b9012096b41c4eb3aaf947f6ea429` (r:1 w:0)
	/// Storage: `ValidatorSecurity::RingHead` (r:1 w:0)
	/// Proof: `ValidatorSecurity::RingHead` (`max_values`: Some(1), `max_size`: Some(4), added: 499, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorSecurity::RingTail` (r:1 w:0)
	/// Proof: `ValidatorSecurity::RingTail` (`max_values`: Some(1), `max_size`: Some(4), added: 499, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorSecurity::ObservationCount` (r:1 w:0)
	/// Proof: `ValidatorSecurity::ObservationCount` (`max_values`: Some(1), `max_size`: Some(4), added: 499, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorSecurity::OffenceIdentityIndex` (r:1 w:0)
	/// Proof: `ValidatorSecurity::OffenceIdentityIndex` (`max_values`: Some(8192), `max_size`: Some(84), added: 2064, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorSecurity::PendingEvidenceContext` (r:1 w:0)
	/// Proof: `ValidatorSecurity::PendingEvidenceContext` (`max_values`: Some(1), `max_size`: Some(33), added: 528, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorSecurity::PendingAdapterError` (r:1 w:0)
	/// Proof: `ValidatorSecurity::PendingAdapterError` (`max_values`: Some(1), `max_size`: Some(1), added: 496, mode: `MaxEncodedLen`)
	/// The range of component `v` is `[100, 1000]`.
	fn grandpa_invalid(v: u32, ) -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `90`
		//  Estimated: `3555`
		// Minimum execution time: 81_813_000 picoseconds.
		Weight::from_parts(96_490_204, 0)
			.saturating_add(Weight::from_parts(0, 3555))
			// Standard Error: 999
			.saturating_add(Weight::from_parts(13_133, 0).saturating_mul(v.into()))
			.saturating_add(T::DbWeight::get().reads(9))
	}
	/// Storage: `Session::CurrentIndex` (r:1 w:0)
	/// Proof: `Session::CurrentIndex` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `Historical::HistoricalSessions` (r:1 w:0)
	/// Proof: `Historical::HistoricalSessions` (`max_values`: None, `max_size`: Some(48), added: 2523, mode: `MaxEncodedLen`)
	/// Storage: UNKNOWN KEY `0xf8d03e8b4e6f1e1952503771969ef3994e7b9012096b41c4eb3aaf947f6ea429` (r:1 w:0)
	/// Proof: UNKNOWN KEY `0xf8d03e8b4e6f1e1952503771969ef3994e7b9012096b41c4eb3aaf947f6ea429` (r:1 w:0)
	/// Storage: `ValidatorSecurity::RingHead` (r:1 w:0)
	/// Proof: `ValidatorSecurity::RingHead` (`max_values`: Some(1), `max_size`: Some(4), added: 499, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorSecurity::RingTail` (r:1 w:0)
	/// Proof: `ValidatorSecurity::RingTail` (`max_values`: Some(1), `max_size`: Some(4), added: 499, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorSecurity::ObservationCount` (r:1 w:0)
	/// Proof: `ValidatorSecurity::ObservationCount` (`max_values`: Some(1), `max_size`: Some(4), added: 499, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorSecurity::OffenceIdentityIndex` (r:2 w:0)
	/// Proof: `ValidatorSecurity::OffenceIdentityIndex` (`max_values`: Some(8192), `max_size`: Some(84), added: 2064, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorSecurity::PendingEvidenceContext` (r:1 w:0)
	/// Proof: `ValidatorSecurity::PendingEvidenceContext` (`max_values`: Some(1), `max_size`: Some(33), added: 528, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorSecurity::PendingAdapterError` (r:1 w:0)
	/// Proof: `ValidatorSecurity::PendingAdapterError` (`max_values`: Some(1), `max_size`: Some(1), added: 496, mode: `MaxEncodedLen`)
	/// Storage: `Grandpa::SetIdSession` (r:1 w:0)
	/// Proof: `Grandpa::SetIdSession` (`max_values`: None, `max_size`: Some(20), added: 2495, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorSecurity::Observations` (r:2 w:0)
	/// Proof: `ValidatorSecurity::Observations` (`max_values`: Some(8192), `max_size`: Some(149), added: 2129, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorSecurity::NewObservationsInBlock` (r:1 w:0)
	/// Proof: `ValidatorSecurity::NewObservationsInBlock` (`max_values`: Some(1), `max_size`: Some(8), added: 503, mode: `MaxEncodedLen`)
	/// Storage: `ValidatorSecurity::ObservationRing` (r:1 w:0)
	/// Proof: `ValidatorSecurity::ObservationRing` (`max_values`: Some(8192), `max_size`: Some(80), added: 2060, mode: `MaxEncodedLen`)
	/// The range of component `v` is `[100, 1000]`.
	fn grandpa_capacity_full(v: u32, ) -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `7126`
		//  Estimated: `10591`
		// Minimum execution time: 410_558_000 picoseconds.
		Weight::from_parts(459_599_134, 0)
			.saturating_add(Weight::from_parts(0, 10591))
			// Standard Error: 2_911
			.saturating_add(Weight::from_parts(16_795, 0).saturating_mul(v.into()))
			.saturating_add(T::DbWeight::get().reads(15))
	}
	/// Storage: UNKNOWN KEY `0xf8d03e8b4e6f1e1952503771969ef3994e7b9012096b41c4eb3aaf947f6ea429` (r:1 w:1)
	/// Proof: UNKNOWN KEY `0xf8d03e8b4e6f1e1952503771969ef3994e7b9012096b41c4eb3aaf947f6ea429` (r:1 w:1)
	/// Storage: UNKNOWN KEY `0xf8d03e8b4e6f1e1952503771969ef399` (r:1 w:0)
	/// Proof: UNKNOWN KEY `0xf8d03e8b4e6f1e1952503771969ef399` (r:1 w:0)
	fn migration_initialize_absent() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `0`
		//  Estimated: `3465`
		// Minimum execution time: 5_741_000 picoseconds.
		Weight::from_parts(6_682_000, 0)
			.saturating_add(Weight::from_parts(0, 3465))
			.saturating_add(T::DbWeight::get().reads(2))
			.saturating_add(T::DbWeight::get().writes(1))
	}
	/// Storage: UNKNOWN KEY `0xf8d03e8b4e6f1e1952503771969ef3994e7b9012096b41c4eb3aaf947f6ea429` (r:1 w:0)
	/// Proof: UNKNOWN KEY `0xf8d03e8b4e6f1e1952503771969ef3994e7b9012096b41c4eb3aaf947f6ea429` (r:1 w:0)
	/// Storage: UNKNOWN KEY `0xf8d03e8b4e6f1e1952503771969ef399` (r:1 w:0)
	/// Proof: UNKNOWN KEY `0xf8d03e8b4e6f1e1952503771969ef399` (r:1 w:0)
	/// The range of component `c` is `[0, 1]`.
	fn migration_reject_dirty(c: u32, ) -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `0 + c * (27 ±0)`
		//  Estimated: `3465 + c * (27 ±0)`
		// Minimum execution time: 5_190_000 picoseconds.
		Weight::from_parts(6_615_779, 0)
			.saturating_add(Weight::from_parts(0, 3465))
			// Standard Error: 361_498
			.saturating_add(Weight::from_parts(4_657_420, 0).saturating_mul(c.into()))
			.saturating_add(T::DbWeight::get().reads(2))
			.saturating_add(Weight::from_parts(0, 27).saturating_mul(c.into()))
	}
	/// Storage: UNKNOWN KEY `0xf8d03e8b4e6f1e1952503771969ef3994e7b9012096b41c4eb3aaf947f6ea429` (r:1 w:0)
	/// Proof: UNKNOWN KEY `0xf8d03e8b4e6f1e1952503771969ef3994e7b9012096b41c4eb3aaf947f6ea429` (r:1 w:0)
	/// The range of component `v` is `[0, 2]`.
	fn migration_reject_version(_v: u32, ) -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `6`
		//  Estimated: `3471`
		// Minimum execution time: 3_467_000 picoseconds.
		Weight::from_parts(3_992_236, 0)
			.saturating_add(Weight::from_parts(0, 3471))
			.saturating_add(T::DbWeight::get().reads(1))
	}
	/// Storage: UNKNOWN KEY `0xf8d03e8b4e6f1e1952503771969ef3994e7b9012096b41c4eb3aaf947f6ea429` (r:1 w:0)
	/// Proof: UNKNOWN KEY `0xf8d03e8b4e6f1e1952503771969ef3994e7b9012096b41c4eb3aaf947f6ea429` (r:1 w:0)
	fn migration_repeated_v1() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `182`
		//  Estimated: `3647`
		// Minimum execution time: 6_913_000 picoseconds.
		Weight::from_parts(7_644_000, 0)
			.saturating_add(Weight::from_parts(0, 3647))
			.saturating_add(T::DbWeight::get().reads(1))
	}
}
