//! End-to-end benchmark declarations for observe-only consensus reporting.

use crate::observation::{
    BenchmarkHelper, Config, NewObservationsInBlock, ObservationCount, Pallet, PendingAdapterError,
    PendingEvidenceContext, RingHead, RingTail, BABE_BENCHMARK_MAX_AUTHORITIES,
    GRANDPA_BENCHMARK_MAX_AUTHORITIES, MAX_RETAINED_OBSERVATIONS,
};
use frame_benchmarking::v2::*;
use frame_support::traits::{Hooks, PalletInfoAccess, StorageVersion};
use frame_system::pallet_prelude::BlockNumberFor;
use sp_runtime::transaction_validity::InvalidTransaction;

#[benchmarks]
mod benchmarks {
    use super::*;

    #[benchmark]
    fn babe_new(v: Linear<100, BABE_BENCHMARK_MAX_AUTHORITIES>) {
        assert_eq!(
            T::BenchmarkHelper::babe_max_authorities(),
            BABE_BENCHMARK_MAX_AUTHORITIES,
        );
        let evidence = T::BenchmarkHelper::setup_babe(v, 0, 64)
            .expect("complete retained-history BABE fixture with worst-case pruning");

        #[block]
        {
            T::BenchmarkHelper::check_babe(&evidence)
                .expect("BABE aggregate pre-dispatch and historical ownership path");
            T::BenchmarkHelper::process_babe(evidence)
                .expect("BABE SDK processing and observer commit");
        }

        assert_eq!(ObservationCount::<T>::get(), 1);
        assert_eq!(
            NewObservationsInBlock::<T>::get().map(|(_, count)| count),
            Some(1)
        );
    }

    #[benchmark]
    fn babe_duplicate(v: Linear<100, BABE_BENCHMARK_MAX_AUTHORITIES>) {
        let evidence = T::BenchmarkHelper::setup_babe(v, 0, 0).expect("BABE duplicate fixture");
        T::BenchmarkHelper::process_babe(evidence.clone()).expect("first valid BABE proof wins");
        let before = T::BenchmarkHelper::observer_state_commitment();
        let counters = T::BenchmarkHelper::observer_counters_and_transients();
        let events = T::BenchmarkHelper::observer_event_count();

        #[block]
        {
            assert_eq!(
                T::BenchmarkHelper::check_babe(&evidence),
                Err(InvalidTransaction::Stale.into())
            );
        }

        assert_eq!(T::BenchmarkHelper::observer_state_commitment(), before);
        assert_eq!(
            T::BenchmarkHelper::observer_counters_and_transients(),
            counters
        );
        assert_eq!(T::BenchmarkHelper::observer_event_count(), events);
    }

    #[benchmark]
    fn babe_invalid(v: Linear<100, BABE_BENCHMARK_MAX_AUTHORITIES>) {
        let evidence = T::BenchmarkHelper::setup_invalid_babe(v)
            .expect("invalid BABE equivocation fixture with valid retained ownership");
        let before = T::BenchmarkHelper::observer_state_commitment();
        let counters = T::BenchmarkHelper::observer_counters_and_transients();
        let events = T::BenchmarkHelper::observer_event_count();

        #[block]
        {
            T::BenchmarkHelper::check_babe(&evidence)
                .expect("BABE observe-only adapter pre-dispatch path reached");
            assert_eq!(
                T::BenchmarkHelper::process_babe(evidence),
                Err(T::BenchmarkHelper::invalid_babe_error())
            );
        }

        assert_eq!(T::BenchmarkHelper::observer_state_commitment(), before);
        assert_eq!(
            T::BenchmarkHelper::observer_counters_and_transients(),
            counters
        );
        assert_eq!(T::BenchmarkHelper::observer_event_count(), events);
    }

    #[benchmark]
    fn babe_capacity_full(v: Linear<100, BABE_BENCHMARK_MAX_AUTHORITIES>) {
        let evidence =
            T::BenchmarkHelper::setup_capacity_full_babe(v).expect("full-capacity BABE fixture");
        let before = T::BenchmarkHelper::observer_state_commitment();
        let counters = T::BenchmarkHelper::observer_counters_and_transients();
        let events = T::BenchmarkHelper::observer_event_count();

        #[block]
        {
            T::BenchmarkHelper::check_babe(&evidence)
                .expect("BABE capacity aggregate pre-dispatch path");
            assert_eq!(
                T::BenchmarkHelper::process_babe(evidence),
                Err(crate::observation::Error::<T>::CapacityFull.into())
            );
        }

        assert_eq!(T::BenchmarkHelper::observer_state_commitment(), before);
        assert_eq!(
            T::BenchmarkHelper::observer_counters_and_transients(),
            counters
        );
        assert_eq!(T::BenchmarkHelper::observer_event_count(), events);
        assert_eq!(ObservationCount::<T>::get(), MAX_RETAINED_OBSERVATIONS);
    }

    #[benchmark]
    fn grandpa_new(v: Linear<100, GRANDPA_BENCHMARK_MAX_AUTHORITIES>) {
        assert_eq!(
            T::BenchmarkHelper::grandpa_max_authorities(),
            GRANDPA_BENCHMARK_MAX_AUTHORITIES,
        );
        let evidence = T::BenchmarkHelper::setup_grandpa(v, 0, 64)
            .expect("complete retained-history GRANDPA fixture with worst-case pruning");

        #[block]
        {
            T::BenchmarkHelper::check_grandpa(&evidence)
                .expect("GRANDPA aggregate pre-dispatch and historical ownership path");
            T::BenchmarkHelper::process_grandpa(evidence)
                .expect("GRANDPA SDK processing and observer commit");
        }

        assert_eq!(ObservationCount::<T>::get(), 1);
        assert_eq!(
            NewObservationsInBlock::<T>::get().map(|(_, count)| count),
            Some(1)
        );
    }

    #[benchmark]
    fn grandpa_duplicate(v: Linear<100, GRANDPA_BENCHMARK_MAX_AUTHORITIES>) {
        let evidence =
            T::BenchmarkHelper::setup_grandpa(v, 0, 0).expect("GRANDPA duplicate fixture");
        T::BenchmarkHelper::process_grandpa(evidence.clone())
            .expect("first valid GRANDPA proof wins");
        let before = T::BenchmarkHelper::observer_state_commitment();
        let counters = T::BenchmarkHelper::observer_counters_and_transients();
        let events = T::BenchmarkHelper::observer_event_count();

        #[block]
        {
            assert_eq!(
                T::BenchmarkHelper::check_grandpa(&evidence),
                Err(InvalidTransaction::Stale.into())
            );
        }

        assert_eq!(T::BenchmarkHelper::observer_state_commitment(), before);
        assert_eq!(
            T::BenchmarkHelper::observer_counters_and_transients(),
            counters
        );
        assert_eq!(T::BenchmarkHelper::observer_event_count(), events);
    }

    #[benchmark]
    fn grandpa_invalid(v: Linear<100, GRANDPA_BENCHMARK_MAX_AUTHORITIES>) {
        let evidence = T::BenchmarkHelper::setup_invalid_grandpa(v)
            .expect("invalid GRANDPA equivocation fixture with valid retained ownership");
        let before = T::BenchmarkHelper::observer_state_commitment();
        let counters = T::BenchmarkHelper::observer_counters_and_transients();
        let events = T::BenchmarkHelper::observer_event_count();

        #[block]
        {
            T::BenchmarkHelper::check_grandpa(&evidence)
                .expect("GRANDPA observe-only adapter pre-dispatch path reached");
            assert_eq!(
                T::BenchmarkHelper::process_grandpa(evidence),
                Err(T::BenchmarkHelper::invalid_grandpa_error())
            );
        }

        assert_eq!(T::BenchmarkHelper::observer_state_commitment(), before);
        assert_eq!(
            T::BenchmarkHelper::observer_counters_and_transients(),
            counters
        );
        assert_eq!(T::BenchmarkHelper::observer_event_count(), events);
    }

    #[benchmark]
    fn grandpa_capacity_full(v: Linear<100, GRANDPA_BENCHMARK_MAX_AUTHORITIES>) {
        let evidence = T::BenchmarkHelper::setup_capacity_full_grandpa(v)
            .expect("full-capacity GRANDPA fixture");
        let before = T::BenchmarkHelper::observer_state_commitment();
        let counters = T::BenchmarkHelper::observer_counters_and_transients();
        let events = T::BenchmarkHelper::observer_event_count();

        #[block]
        {
            T::BenchmarkHelper::check_grandpa(&evidence)
                .expect("GRANDPA capacity aggregate pre-dispatch path");
            assert_eq!(
                T::BenchmarkHelper::process_grandpa(evidence),
                Err(crate::observation::Error::<T>::CapacityFull.into())
            );
        }

        assert_eq!(T::BenchmarkHelper::observer_state_commitment(), before);
        assert_eq!(
            T::BenchmarkHelper::observer_counters_and_transients(),
            counters
        );
        assert_eq!(T::BenchmarkHelper::observer_event_count(), events);
        assert_eq!(ObservationCount::<T>::get(), MAX_RETAINED_OBSERVATIONS);
    }

    #[benchmark]
    fn migration_initialize_absent() {
        let prefix = sp_io::hashing::twox_128(<Pallet<T> as PalletInfoAccess>::name().as_bytes());
        sp_io::storage::clear(&prefix);
        sp_io::storage::clear(&StorageVersion::storage_key::<Pallet<T>>());
        RingHead::<T>::kill();
        RingTail::<T>::kill();
        ObservationCount::<T>::kill();
        NewObservationsInBlock::<T>::kill();
        PendingEvidenceContext::<T>::kill();
        PendingAdapterError::<T>::kill();
        assert!(!sp_io::storage::exists(&prefix));
        assert!(!StorageVersion::exists::<Pallet<T>>());

        #[block]
        {
            <Pallet<T> as Hooks<BlockNumberFor<T>>>::on_runtime_upgrade();
        }

        assert_eq!(StorageVersion::get::<Pallet<T>>(), StorageVersion::new(1));
    }

    #[benchmark]
    fn migration_reject_dirty(c: Linear<0, 1>) {
        sp_io::storage::clear(&StorageVersion::storage_key::<Pallet<T>>());
        let prefix = sp_io::hashing::twox_128(<Pallet<T> as PalletInfoAccess>::name().as_bytes());
        if c == 0 {
            sp_io::storage::set(&prefix, b"exact-prefix");
        } else {
            let mut dirty = prefix.to_vec();
            dirty.extend_from_slice(b":rev4-successor-prefix:");
            sp_io::storage::set(&dirty, b"successor-prefix");
        }
        let before = T::BenchmarkHelper::observer_state_commitment();
        let counters = T::BenchmarkHelper::observer_counters_and_transients();
        let events = T::BenchmarkHelper::observer_event_count();

        #[block]
        {
            <Pallet<T> as Hooks<BlockNumberFor<T>>>::on_runtime_upgrade();
        }

        assert_eq!(T::BenchmarkHelper::observer_state_commitment(), before);
        assert_eq!(
            T::BenchmarkHelper::observer_counters_and_transients(),
            counters
        );
        assert_eq!(T::BenchmarkHelper::observer_event_count(), events);
        assert!(!StorageVersion::exists::<Pallet<T>>());
    }

    #[benchmark]
    fn migration_reject_version(v: Linear<0, 2>) {
        match v {
            0 => StorageVersion::new(0).put::<Pallet<T>>(),
            1 => StorageVersion::new(2).put::<Pallet<T>>(),
            _ => sp_io::storage::set(&StorageVersion::storage_key::<Pallet<T>>(), &[1]),
        }
        let before = T::BenchmarkHelper::observer_state_commitment();
        let counters = T::BenchmarkHelper::observer_counters_and_transients();
        let events = T::BenchmarkHelper::observer_event_count();

        #[block]
        {
            <Pallet<T> as Hooks<BlockNumberFor<T>>>::on_runtime_upgrade();
        }

        assert_eq!(T::BenchmarkHelper::observer_state_commitment(), before);
        assert_eq!(
            T::BenchmarkHelper::observer_counters_and_transients(),
            counters
        );
        assert_eq!(T::BenchmarkHelper::observer_event_count(), events);
    }

    #[benchmark]
    fn migration_repeated_v1() {
        StorageVersion::new(1).put::<Pallet<T>>();
        let batch = T::BenchmarkHelper::setup_private_batch(1, 1, 0).expect("retained v1 fixture");
        T::BenchmarkHelper::process_private_batch(batch).expect("retained v1 record");
        let before = T::BenchmarkHelper::observer_state_commitment();
        let counters = T::BenchmarkHelper::observer_counters_and_transients();
        let events = T::BenchmarkHelper::observer_event_count();

        #[block]
        {
            <Pallet<T> as Hooks<BlockNumberFor<T>>>::on_runtime_upgrade();
        }

        assert_eq!(T::BenchmarkHelper::observer_state_commitment(), before);
        assert_eq!(
            T::BenchmarkHelper::observer_counters_and_transients(),
            counters
        );
        assert_eq!(T::BenchmarkHelper::observer_event_count(), events);
        assert_eq!(StorageVersion::get::<Pallet<T>>(), StorageVersion::new(1));
        assert_eq!(ObservationCount::<T>::get(), 1);
    }
}
