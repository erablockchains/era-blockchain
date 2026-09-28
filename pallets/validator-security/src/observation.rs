//! Bounded observe-only storage for SDK-validated BABE and GRANDPA equivocations.
//!
//! This pallet has no calls. Consensus proof validation stays in the pinned SDK report systems.
//! Production code cannot select an observation source: source zero is derived only by the
//! validated consensus entrypoints, while source one exists only in test/benchmark entrypoints.

use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::weights::Weight;
use scale_info::TypeInfo;
use sp_runtime::RuntimeDebug;

pub const OBSERVATION_DOMAIN: [u8; 30] = *b"era/v14/offence-observation/v1";
pub const OBSERVATION_VERSION: u8 = 1;
pub const MAX_RETAINED_OBSERVATIONS: u32 = 8_192;
pub const MAX_NEW_OBSERVATIONS_PER_BLOCK: u32 = 16;
pub const MAX_OBSERVATION_BATCH: u32 = 16;
pub const MAX_PRUNE_PER_REPORT: u32 = 64;
pub const OBSERVATION_LONGEVITY: u32 = 14_400;
pub const BABE_BENCHMARK_MAX_AUTHORITIES: u32 = 1_000;
pub const GRANDPA_BENCHMARK_MAX_AUTHORITIES: u32 = 1_000;

#[derive(
    Encode,
    Decode,
    codec::DecodeWithMemTracking,
    Clone,
    Copy,
    PartialEq,
    Eq,
    RuntimeDebug,
    TypeInfo,
    MaxEncodedLen,
)]
pub enum ObservationKind {
    #[codec(index = 0)]
    Babe,
    #[codec(index = 1)]
    Grandpa,
}

#[derive(
    Encode,
    Decode,
    codec::DecodeWithMemTracking,
    Clone,
    Copy,
    PartialEq,
    Eq,
    RuntimeDebug,
    TypeInfo,
    MaxEncodedLen,
)]
pub enum ObservationSource {
    /// Existing consensus unsigned intake admitted as Local or InBlock.
    #[codec(index = 0)]
    LocalUnsignedValidation,
    /// Non-dispatchable synthetic test/benchmark input only.
    #[codec(index = 1)]
    RehearsalImport,
}

#[derive(
    Encode,
    Decode,
    codec::DecodeWithMemTracking,
    Clone,
    PartialEq,
    Eq,
    RuntimeDebug,
    TypeInfo,
    MaxEncodedLen,
)]
pub struct ObservationKeyPreimage<AccountId> {
    pub domain: [u8; 30],
    pub version: u8,
    pub kind: ObservationKind,
    pub offender: AccountId,
    pub session_index: u32,
    pub babe_slot: Option<u64>,
    pub grandpa_coordinate: Option<(u64, u64)>,
    pub proof_hash: [u8; 32],
}

#[derive(
    Encode,
    Decode,
    codec::DecodeWithMemTracking,
    Clone,
    PartialEq,
    Eq,
    RuntimeDebug,
    TypeInfo,
    MaxEncodedLen,
)]
pub struct OffenceIdentityPreimage<AccountId> {
    pub domain: [u8; 30],
    pub version: u8,
    pub kind: ObservationKind,
    pub offender: AccountId,
    pub session_index: u32,
    pub babe_slot: Option<u64>,
    pub grandpa_coordinate: Option<(u64, u64)>,
}

#[derive(
    Encode,
    Decode,
    codec::DecodeWithMemTracking,
    Clone,
    PartialEq,
    Eq,
    RuntimeDebug,
    TypeInfo,
    MaxEncodedLen,
)]
pub struct OffenceObservationV1<AccountId> {
    pub version: u8,
    pub kind: ObservationKind,
    pub offender: AccountId,
    pub session_index: u32,
    pub babe_slot: Option<u64>,
    pub grandpa_coordinate: Option<(u64, u64)>,
    pub proof_hash: [u8; 32],
    pub observed_at: u32,
    pub source: ObservationSource,
}

#[derive(
    Encode,
    Decode,
    codec::DecodeWithMemTracking,
    Clone,
    PartialEq,
    Eq,
    RuntimeDebug,
    TypeInfo,
    MaxEncodedLen,
)]
pub struct IdentityIndexEntryV1 {
    pub observation_key: [u8; 32],
    pub ring_index: u32,
}

#[derive(
    Encode,
    Decode,
    codec::DecodeWithMemTracking,
    Clone,
    PartialEq,
    Eq,
    RuntimeDebug,
    TypeInfo,
    MaxEncodedLen,
)]
pub struct RingEntry {
    pub identity_key: [u8; 32],
    pub observation_key: [u8; 32],
    pub expires_at: u32,
}

#[derive(
    Encode,
    Decode,
    codec::DecodeWithMemTracking,
    Clone,
    PartialEq,
    Eq,
    RuntimeDebug,
    TypeInfo,
    MaxEncodedLen,
)]
pub struct EvidenceContext {
    pub kind: ObservationKind,
    pub proof_hash: [u8; 32],
}

#[cfg(feature = "runtime-benchmarks")]
pub type ObserverCountersAndTransients =
    (u32, Option<(u32, u32)>, Option<EvidenceContext>, Option<u8>);

/// Private-path input. It intentionally has no source field.
#[derive(Clone, PartialEq, Eq, RuntimeDebug)]
pub struct ObservationInput<AccountId> {
    pub kind: ObservationKind,
    pub offender: AccountId,
    pub session_index: u32,
    pub babe_slot: Option<u64>,
    pub grandpa_coordinate: Option<(u64, u64)>,
    pub proof_hash: [u8; 32],
}

type ValidatedObservation<AccountId> = ([u8; 32], [u8; 32], ObservationInput<AccountId>);

#[derive(Clone, Copy, PartialEq, Eq, RuntimeDebug)]
pub struct RecordOutcome {
    pub stored: u32,
    pub duplicates: u32,
    pub pruned: u32,
}

pub trait WeightInfo {
    fn babe_new(validator_count: u32) -> Weight;
    fn babe_duplicate(validator_count: u32) -> Weight;
    fn babe_invalid(validator_count: u32) -> Weight;
    fn babe_capacity_full(validator_count: u32) -> Weight;
    fn grandpa_new(validator_count: u32) -> Weight;
    fn grandpa_duplicate(validator_count: u32) -> Weight;
    fn grandpa_invalid(validator_count: u32) -> Weight;
    fn grandpa_capacity_full(validator_count: u32) -> Weight;
    fn migration_initialize_absent() -> Weight;
    fn migration_reject_dirty(contamination: u32) -> Weight;
    fn migration_reject_version(version_case: u32) -> Weight;
    fn migration_repeated_v1() -> Weight;
}

#[cfg(feature = "runtime-benchmarks")]
pub trait BenchmarkHelper<T: pallet::Config> {
    type BabeEvidence: Clone;
    type GrandpaEvidence: Clone;
    type PrivateBatch: Clone;

    fn babe_max_authorities() -> u32;
    fn grandpa_max_authorities() -> u32;
    fn setup_babe(
        validator_count: u32,
        max_nominators_per_validator: u32,
        prune: u32,
    ) -> Result<Self::BabeEvidence, sp_runtime::DispatchError>;
    fn setup_grandpa(
        validator_count: u32,
        max_nominators_per_validator: u32,
        prune: u32,
    ) -> Result<Self::GrandpaEvidence, sp_runtime::DispatchError>;
    fn setup_invalid_babe(
        validator_count: u32,
    ) -> Result<Self::BabeEvidence, sp_runtime::DispatchError>;
    fn setup_invalid_grandpa(
        validator_count: u32,
    ) -> Result<Self::GrandpaEvidence, sp_runtime::DispatchError>;
    fn invalid_babe_error() -> sp_runtime::DispatchError;
    fn invalid_grandpa_error() -> sp_runtime::DispatchError;
    fn check_babe(
        evidence: &Self::BabeEvidence,
    ) -> Result<(), sp_runtime::transaction_validity::TransactionValidityError>;
    fn check_grandpa(
        evidence: &Self::GrandpaEvidence,
    ) -> Result<(), sp_runtime::transaction_validity::TransactionValidityError>;
    fn process_babe(evidence: Self::BabeEvidence) -> sp_runtime::DispatchResult;
    fn process_grandpa(evidence: Self::GrandpaEvidence) -> sp_runtime::DispatchResult;
    fn setup_private_batch(
        n: u32,
        unique_new: u32,
        prune: u32,
    ) -> Result<Self::PrivateBatch, sp_runtime::DispatchError>;
    fn setup_capacity_full_babe(
        validator_count: u32,
    ) -> Result<Self::BabeEvidence, sp_runtime::DispatchError>;
    fn setup_capacity_full_grandpa(
        validator_count: u32,
    ) -> Result<Self::GrandpaEvidence, sp_runtime::DispatchError>;
    fn process_private_batch(
        batch: Self::PrivateBatch,
    ) -> Result<RecordOutcome, sp_runtime::DispatchError>;
    fn observer_state_commitment() -> [u8; 32];
    fn observer_counters_and_transients() -> ObserverCountersAndTransients;
    fn observer_event_count() -> u32;
}

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_support::{
        pallet_prelude::*,
        storage::{transactional::with_transaction_opaque_err, TransactionOutcome},
        traits::{ConstU32, GetDefault, PalletInfoAccess, StorageVersion},
    };
    use frame_system::pallet_prelude::*;

    const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

    #[cfg(feature = "try-runtime")]
    #[derive(Encode, Decode)]
    struct UpgradeSnapshot {
        initial_transition: bool,
        head: u32,
        tail: u32,
        count: u32,
        namespace_commitment: [u8; 32],
    }

    enum RawStorageVersion {
        Absent,
        Exact(StorageVersion),
        Malformed,
    }

    #[derive(Clone, Copy, PartialEq, Eq)]
    #[cfg_attr(test, derive(Debug))]
    pub(super) enum RawNamespaceState {
        Absent,
        ExactPrefix,
        SuccessorPrefix,
    }

    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    #[pallet::config]
    pub trait Config: frame_system::Config<RuntimeEvent: From<Event<Self>>> {
        type ObserverWeightInfo: WeightInfo;
        #[pallet::constant]
        type BabeMaxAuthorities: Get<u32>;
        #[pallet::constant]
        type GrandpaMaxAuthorities: Get<u32>;

        #[cfg(feature = "runtime-benchmarks")]
        type BenchmarkHelper: super::BenchmarkHelper<Self>;
    }

    #[pallet::storage]
    #[pallet::getter(fn observations)]
    pub type Observations<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        [u8; 32],
        OffenceObservationV1<T::AccountId>,
        OptionQuery,
        GetDefault,
        ConstU32<MAX_RETAINED_OBSERVATIONS>,
    >;

    #[pallet::storage]
    #[pallet::getter(fn offence_identity_index)]
    pub type OffenceIdentityIndex<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        [u8; 32],
        IdentityIndexEntryV1,
        OptionQuery,
        GetDefault,
        ConstU32<MAX_RETAINED_OBSERVATIONS>,
    >;

    #[pallet::storage]
    #[pallet::getter(fn observation_ring)]
    pub type ObservationRing<T: Config> = StorageMap<
        _,
        Twox64Concat,
        u32,
        RingEntry,
        OptionQuery,
        GetDefault,
        ConstU32<MAX_RETAINED_OBSERVATIONS>,
    >;

    #[pallet::storage]
    #[pallet::getter(fn ring_head)]
    pub type RingHead<T> = StorageValue<_, u32, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn ring_tail)]
    pub type RingTail<T> = StorageValue<_, u32, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn observation_count)]
    pub type ObservationCount<T> = StorageValue<_, u32, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn new_observations_in_block)]
    pub type NewObservationsInBlock<T> = StorageValue<_, (u32, u32), OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn pending_evidence_context)]
    pub type PendingEvidenceContext<T> = StorageValue<_, EvidenceContext, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn pending_adapter_error)]
    pub type PendingAdapterError<T> = StorageValue<_, u8, OptionQuery>;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        ObservationStored {
            identity_key: [u8; 32],
            observation_key: [u8; 32],
            record: OffenceObservationV1<T::AccountId>,
        },
        ExpiredObservationsPruned {
            count: u32,
            new_head: u32,
            remaining: u32,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        UnsupportedStorageVersion,
        InvalidCoordinateShape,
        EmptyObservationBatch,
        ObservationBatchTooLarge,
        TooManyNewObservationsThisBlock,
        CapacityFull,
        ObservationKeyPreimageMismatch,
        IdentityKeyPreimageMismatch,
        IdentityIndexInvariant,
        BlockNumberOverflow,
        ExpiryOverflow,
        RingIndexOverflow,
        RingInvariantViolation,
        EvidenceContextOccupied,
        EvidenceContextMissing,
        EvidenceContextMismatch,
        SignedIntakeRejected,
        AdapterFailure,
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        fn on_runtime_upgrade() -> Weight {
            match Self::raw_storage_version() {
                RawStorageVersion::Absent => match Self::raw_namespace_state() {
                    RawNamespaceState::Absent => {
                        STORAGE_VERSION.put::<Pallet<T>>();
                        T::ObserverWeightInfo::migration_initialize_absent()
                    }
                    RawNamespaceState::ExactPrefix => {
                        T::ObserverWeightInfo::migration_reject_dirty(0)
                    }
                    RawNamespaceState::SuccessorPrefix => {
                        T::ObserverWeightInfo::migration_reject_dirty(1)
                    }
                },
                RawStorageVersion::Exact(version) if version == STORAGE_VERSION => {
                    // Idempotent: an already-v1 observer, including retained records, is untouched.
                    T::ObserverWeightInfo::migration_repeated_v1()
                }
                RawStorageVersion::Exact(version) if version == StorageVersion::new(0) => {
                    T::ObserverWeightInfo::migration_reject_version(0)
                }
                RawStorageVersion::Exact(_) => T::ObserverWeightInfo::migration_reject_version(1),
                RawStorageVersion::Malformed => T::ObserverWeightInfo::migration_reject_version(2),
            }
        }

        #[cfg(feature = "try-runtime")]
        fn pre_upgrade() -> Result<alloc::vec::Vec<u8>, sp_runtime::TryRuntimeError> {
            if matches!(Self::raw_storage_version(), RawStorageVersion::Absent) {
                if Self::raw_namespace_state() != RawNamespaceState::Absent {
                    return Err("ValidatorSecurity absent-version namespace is not empty".into());
                }
                return Ok(UpgradeSnapshot {
                    initial_transition: true,
                    head: 0,
                    tail: 0,
                    count: 0,
                    namespace_commitment: Self::namespace_commitment(),
                }
                .encode());
            }
            if matches!(
                Self::raw_storage_version(),
                RawStorageVersion::Exact(version) if version == STORAGE_VERSION
            ) {
                Self::validate_complete_state()
                    .map_err(|_| "ValidatorSecurity retained v1 state is inconsistent")?;
                return Ok(UpgradeSnapshot {
                    initial_transition: false,
                    head: RingHead::<T>::get(),
                    tail: RingTail::<T>::get(),
                    count: ObservationCount::<T>::get(),
                    namespace_commitment: Self::namespace_commitment(),
                }
                .encode());
            }
            Err(
                "ValidatorSecurity storage version is explicit zero, unexpected, or malformed"
                    .into(),
            )
        }

        #[cfg(feature = "try-runtime")]
        fn post_upgrade(state: alloc::vec::Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
            let mut input = state.as_slice();
            let snapshot = UpgradeSnapshot::decode(&mut input)
                .map_err(|_| "ValidatorSecurity prestate snapshot is invalid")?;
            if !input.is_empty()
                || !matches!(
                    Self::raw_storage_version(),
                    RawStorageVersion::Exact(version) if version == STORAGE_VERSION
                )
            {
                return Err("ValidatorSecurity post-upgrade version mismatch".into());
            }
            if snapshot.initial_transition {
                Self::validate_complete_state()
                    .map_err(|_| "ValidatorSecurity initial v1 poststate is inconsistent")?;
                if !Self::namespace_contains_only_storage_version() {
                    return Err("ValidatorSecurity initial v1 poststate is not empty".into());
                }
                return Ok(());
            }
            Self::validate_complete_state()
                .map_err(|_| "ValidatorSecurity repeated v1 state is inconsistent")?;
            if RingHead::<T>::get() != snapshot.head
                || RingTail::<T>::get() != snapshot.tail
                || ObservationCount::<T>::get() != snapshot.count
                || Self::namespace_commitment() != snapshot.namespace_commitment
            {
                return Err("ValidatorSecurity repeated v1 upgrade mutated state".into());
            }
            Ok(())
        }
    }

    impl<T: Config> Pallet<T> {
        pub fn observation_key(record: &OffenceObservationV1<T::AccountId>) -> [u8; 32] {
            sp_io::hashing::blake2_256(
                &ObservationKeyPreimage {
                    domain: OBSERVATION_DOMAIN,
                    version: record.version,
                    kind: record.kind,
                    offender: record.offender.clone(),
                    session_index: record.session_index,
                    babe_slot: record.babe_slot,
                    grandpa_coordinate: record.grandpa_coordinate,
                    proof_hash: record.proof_hash,
                }
                .encode(),
            )
        }

        pub fn identity_key(record: &OffenceObservationV1<T::AccountId>) -> [u8; 32] {
            sp_io::hashing::blake2_256(
                &OffenceIdentityPreimage {
                    domain: OBSERVATION_DOMAIN,
                    version: record.version,
                    kind: record.kind,
                    offender: record.offender.clone(),
                    session_index: record.session_index,
                    babe_slot: record.babe_slot,
                    grandpa_coordinate: record.grandpa_coordinate,
                }
                .encode(),
            )
        }

        fn validate_shape(
            kind: ObservationKind,
            babe_slot: Option<u64>,
            grandpa_coordinate: Option<(u64, u64)>,
        ) -> Result<(), Error<T>> {
            match (kind, babe_slot, grandpa_coordinate) {
                (ObservationKind::Babe, Some(_), None)
                | (ObservationKind::Grandpa, None, Some(_)) => Ok(()),
                _ => Err(Error::<T>::InvalidCoordinateShape),
            }
        }

        fn next_ring_index(index: u32) -> Result<u32, Error<T>> {
            ensure!(
                index < MAX_RETAINED_OBSERVATIONS,
                Error::<T>::RingIndexOverflow
            );
            if index + 1 == MAX_RETAINED_OBSERVATIONS {
                Ok(0)
            } else {
                index.checked_add(1).ok_or(Error::<T>::RingIndexOverflow)
            }
        }

        fn ring_index_at(head: u32, offset: u32) -> Result<u32, Error<T>> {
            ensure!(
                head < MAX_RETAINED_OBSERVATIONS && offset <= MAX_RETAINED_OBSERVATIONS,
                Error::<T>::RingIndexOverflow
            );
            Ok(((head as u64 + offset as u64) % MAX_RETAINED_OBSERVATIONS as u64) as u32)
        }

        fn validate_ring_scalars() -> Result<(u32, u32, u32), Error<T>> {
            let head = RingHead::<T>::get();
            let tail = RingTail::<T>::get();
            let count = ObservationCount::<T>::get();
            ensure!(
                head < MAX_RETAINED_OBSERVATIONS
                    && tail < MAX_RETAINED_OBSERVATIONS
                    && count <= MAX_RETAINED_OBSERVATIONS,
                Error::<T>::RingInvariantViolation
            );
            ensure!(
                Self::ring_index_at(head, count)? == tail,
                Error::<T>::RingInvariantViolation
            );
            Ok((head, tail, count))
        }

        fn validate_record_link(
            identity_key: [u8; 32],
            index_entry: &IdentityIndexEntryV1,
            ring: &RingEntry,
            record: &OffenceObservationV1<T::AccountId>,
        ) -> Result<(), Error<T>> {
            ensure!(
                record.version == OBSERVATION_VERSION,
                Error::<T>::UnsupportedStorageVersion
            );
            Self::validate_shape(record.kind, record.babe_slot, record.grandpa_coordinate)?;
            ensure!(
                Self::identity_key(record) == identity_key,
                Error::<T>::IdentityKeyPreimageMismatch
            );
            ensure!(
                Self::observation_key(record) == index_entry.observation_key,
                Error::<T>::ObservationKeyPreimageMismatch
            );
            ensure!(
                ring.identity_key == identity_key
                    && ring.observation_key == index_entry.observation_key,
                Error::<T>::IdentityIndexInvariant
            );
            let expected_expiry = record
                .observed_at
                .checked_add(OBSERVATION_LONGEVITY)
                .ok_or(Error::<T>::ExpiryOverflow)?;
            ensure!(
                ring.expires_at == expected_expiry,
                Error::<T>::RingInvariantViolation
            );
            Ok(())
        }

        fn checked_existing(
            identity_key: [u8; 32],
        ) -> Result<Option<IdentityIndexEntryV1>, Error<T>> {
            ensure!(
                StorageVersion::get::<Pallet<T>>() == STORAGE_VERSION,
                Error::<T>::UnsupportedStorageVersion
            );
            Self::validate_ring_scalars()?;
            let Some(index_entry) = OffenceIdentityIndex::<T>::get(identity_key) else {
                return Ok(None);
            };
            ensure!(
                index_entry.ring_index < MAX_RETAINED_OBSERVATIONS,
                Error::<T>::IdentityIndexInvariant
            );
            let record = Observations::<T>::get(index_entry.observation_key)
                .ok_or(Error::<T>::IdentityIndexInvariant)?;
            let ring = ObservationRing::<T>::get(index_entry.ring_index)
                .ok_or(Error::<T>::IdentityIndexInvariant)?;
            Self::validate_record_link(identity_key, &index_entry, &ring, &record)?;
            Ok(Some(index_entry))
        }

        pub fn identity_known_babe(
            offender: T::AccountId,
            session_index: u32,
            slot: u64,
        ) -> Result<bool, Error<T>> {
            let record = OffenceObservationV1 {
                version: OBSERVATION_VERSION,
                kind: ObservationKind::Babe,
                offender,
                session_index,
                babe_slot: Some(slot),
                grandpa_coordinate: None,
                proof_hash: [0; 32],
                observed_at: 0,
                source: ObservationSource::LocalUnsignedValidation,
            };
            Self::checked_existing(Self::identity_key(&record)).map(|entry| entry.is_some())
        }

        pub fn identity_known_grandpa(
            offender: T::AccountId,
            session_index: u32,
            set_id: u64,
            round: u64,
        ) -> Result<bool, Error<T>> {
            let record = OffenceObservationV1 {
                version: OBSERVATION_VERSION,
                kind: ObservationKind::Grandpa,
                offender,
                session_index,
                babe_slot: None,
                grandpa_coordinate: Some((set_id, round)),
                proof_hash: [0; 32],
                observed_at: 0,
                source: ObservationSource::LocalUnsignedValidation,
            };
            Self::checked_existing(Self::identity_key(&record)).map(|entry| entry.is_some())
        }

        pub fn begin_evidence(kind: ObservationKind, proof_hash: [u8; 32]) -> Result<(), Error<T>> {
            ensure!(
                PendingEvidenceContext::<T>::get().is_none()
                    && PendingAdapterError::<T>::get().is_none(),
                Error::<T>::EvidenceContextOccupied
            );
            PendingEvidenceContext::<T>::put(EvidenceContext { kind, proof_hash });
            Ok(())
        }

        pub fn transient_state_is_clear() -> bool {
            PendingEvidenceContext::<T>::get().is_none()
                && PendingAdapterError::<T>::get().is_none()
        }

        pub fn adapter_error_code(error: Error<T>) -> u8 {
            match error {
                Error::<T>::UnsupportedStorageVersion => 0,
                Error::<T>::InvalidCoordinateShape => 1,
                Error::<T>::EmptyObservationBatch => 2,
                Error::<T>::ObservationBatchTooLarge => 3,
                Error::<T>::TooManyNewObservationsThisBlock => 4,
                Error::<T>::CapacityFull => 5,
                Error::<T>::ObservationKeyPreimageMismatch => 6,
                Error::<T>::IdentityKeyPreimageMismatch => 7,
                Error::<T>::IdentityIndexInvariant => 8,
                Error::<T>::BlockNumberOverflow => 9,
                Error::<T>::ExpiryOverflow => 10,
                Error::<T>::RingIndexOverflow => 11,
                Error::<T>::RingInvariantViolation => 12,
                Error::<T>::EvidenceContextOccupied => 13,
                Error::<T>::EvidenceContextMissing => 14,
                Error::<T>::EvidenceContextMismatch => 15,
                Error::<T>::SignedIntakeRejected => 16,
                Error::<T>::AdapterFailure => 17,
            }
        }

        pub fn dispatch_error_for_code(code: u8) -> sp_runtime::DispatchError {
            match code {
                0 => Error::<T>::UnsupportedStorageVersion.into(),
                1 => Error::<T>::InvalidCoordinateShape.into(),
                2 => Error::<T>::EmptyObservationBatch.into(),
                3 => Error::<T>::ObservationBatchTooLarge.into(),
                4 => Error::<T>::TooManyNewObservationsThisBlock.into(),
                5 => Error::<T>::CapacityFull.into(),
                6 => Error::<T>::ObservationKeyPreimageMismatch.into(),
                7 => Error::<T>::IdentityKeyPreimageMismatch.into(),
                8 => Error::<T>::IdentityIndexInvariant.into(),
                9 => Error::<T>::BlockNumberOverflow.into(),
                10 => Error::<T>::ExpiryOverflow.into(),
                11 => Error::<T>::RingIndexOverflow.into(),
                12 => Error::<T>::RingInvariantViolation.into(),
                13 => Error::<T>::EvidenceContextOccupied.into(),
                14 => Error::<T>::EvidenceContextMissing.into(),
                15 => Error::<T>::EvidenceContextMismatch.into(),
                16 => Error::<T>::SignedIntakeRejected.into(),
                _ => Error::<T>::AdapterFailure.into(),
            }
        }

        fn transactional<R>(
            operation: impl FnOnce() -> Result<R, Error<T>>,
        ) -> Result<R, Error<T>> {
            with_transaction_opaque_err(|| {
                let result = operation();
                if result.is_ok() {
                    TransactionOutcome::Commit(result)
                } else {
                    TransactionOutcome::Rollback(result)
                }
            })
            .map_err(|_| Error::<T>::AdapterFailure)?
        }

        fn consume_context(kind: ObservationKind) -> Result<[u8; 32], Error<T>> {
            let context =
                PendingEvidenceContext::<T>::take().ok_or(Error::<T>::EvidenceContextMissing)?;
            ensure!(context.kind == kind, Error::<T>::EvidenceContextMismatch);
            Ok(context.proof_hash)
        }

        pub fn record_validated_babe(
            offender: T::AccountId,
            session_index: u32,
            slot: u64,
        ) -> Result<RecordOutcome, Error<T>>
        where
            BlockNumberFor<T>: TryInto<u32>,
        {
            Self::transactional(|| {
                let proof_hash = Self::consume_context(ObservationKind::Babe)?;
                let batch =
                    BoundedVec::<_, ConstU32<MAX_OBSERVATION_BATCH>>::try_from(alloc::vec![
                        ObservationInput {
                            kind: ObservationKind::Babe,
                            offender,
                            session_index,
                            babe_slot: Some(slot),
                            grandpa_coordinate: None,
                            proof_hash,
                        }
                    ])
                    .map_err(|_| Error::<T>::ObservationBatchTooLarge)?;
                Self::do_record_batch(batch, ObservationSource::LocalUnsignedValidation)
            })
        }

        pub fn record_validated_grandpa(
            offender: T::AccountId,
            session_index: u32,
            set_id: u64,
            round: u64,
        ) -> Result<RecordOutcome, Error<T>>
        where
            BlockNumberFor<T>: TryInto<u32>,
        {
            Self::transactional(|| {
                let proof_hash = Self::consume_context(ObservationKind::Grandpa)?;
                let batch =
                    BoundedVec::<_, ConstU32<MAX_OBSERVATION_BATCH>>::try_from(alloc::vec![
                        ObservationInput {
                            kind: ObservationKind::Grandpa,
                            offender,
                            session_index,
                            babe_slot: None,
                            grandpa_coordinate: Some((set_id, round)),
                            proof_hash,
                        }
                    ])
                    .map_err(|_| Error::<T>::ObservationBatchTooLarge)?;
                Self::do_record_batch(batch, ObservationSource::LocalUnsignedValidation)
            })
        }

        #[cfg(any(test, feature = "runtime-benchmarks"))]
        pub fn record_rehearsal_batch(
            batch: BoundedVec<ObservationInput<T::AccountId>, ConstU32<MAX_OBSERVATION_BATCH>>,
        ) -> Result<RecordOutcome, Error<T>>
        where
            BlockNumberFor<T>: TryInto<u32>,
        {
            Self::transactional(|| Self::do_record_batch(batch, ObservationSource::RehearsalImport))
        }

        fn do_record_batch(
            batch: BoundedVec<ObservationInput<T::AccountId>, ConstU32<MAX_OBSERVATION_BATCH>>,
            source: ObservationSource,
        ) -> Result<RecordOutcome, Error<T>>
        where
            BlockNumberFor<T>: TryInto<u32>,
        {
            ensure!(
                StorageVersion::get::<Pallet<T>>() == STORAGE_VERSION,
                Error::<T>::UnsupportedStorageVersion
            );
            ensure!(!batch.is_empty(), Error::<T>::EmptyObservationBatch);
            let (_, _, _) = Self::validate_ring_scalars()?;

            let now: u32 = frame_system::Pallet::<T>::block_number()
                .try_into()
                .map_err(|_| Error::<T>::BlockNumberOverflow)?;
            let expires_at = now
                .checked_add(OBSERVATION_LONGEVITY)
                .ok_or(Error::<T>::ExpiryOverflow)?;

            let mut duplicates = 0u32;
            let mut unique: alloc::vec::Vec<ValidatedObservation<T::AccountId>> =
                alloc::vec::Vec::with_capacity(batch.len());
            for item in batch {
                Self::validate_shape(item.kind, item.babe_slot, item.grandpa_coordinate)?;
                let record = OffenceObservationV1 {
                    version: OBSERVATION_VERSION,
                    kind: item.kind,
                    offender: item.offender.clone(),
                    session_index: item.session_index,
                    babe_slot: item.babe_slot,
                    grandpa_coordinate: item.grandpa_coordinate,
                    proof_hash: item.proof_hash,
                    observed_at: now,
                    source,
                };
                let identity_key = Self::identity_key(&record);
                if unique.iter().any(|(seen, _, _)| *seen == identity_key) {
                    duplicates = duplicates.saturating_add(1);
                    continue;
                }
                if Self::checked_existing(identity_key)?.is_some() {
                    duplicates = duplicates.saturating_add(1);
                    continue;
                }
                let observation_key = Self::observation_key(&record);
                ensure!(
                    !Observations::<T>::contains_key(observation_key),
                    Error::<T>::ObservationKeyPreimageMismatch
                );
                unique.push((identity_key, observation_key, item));
            }

            if unique.is_empty() {
                return Ok(RecordOutcome {
                    stored: 0,
                    duplicates,
                    pruned: 0,
                });
            }

            let current_new = match NewObservationsInBlock::<T>::get() {
                Some((block, count)) if block == now => count,
                _ => 0,
            };
            let unique_count = unique.len() as u32;
            let next_new = current_new
                .checked_add(unique_count)
                .ok_or(Error::<T>::TooManyNewObservationsThisBlock)?;
            ensure!(
                next_new <= MAX_NEW_OBSERVATIONS_PER_BLOCK,
                Error::<T>::TooManyNewObservationsThisBlock
            );

            let (pruned, mut count) = Self::prune_expired(now)?;
            ensure!(
                count
                    .checked_add(unique_count)
                    .filter(|total| *total <= MAX_RETAINED_OBSERVATIONS)
                    .is_some(),
                Error::<T>::CapacityFull
            );

            if pruned > 0 {
                Self::deposit_event(Event::<T>::ExpiredObservationsPruned {
                    count: pruned,
                    new_head: RingHead::<T>::get(),
                    remaining: count,
                });
            }

            let mut tail = RingTail::<T>::get();
            for (identity_key, observation_key, item) in unique {
                ensure!(
                    !ObservationRing::<T>::contains_key(tail),
                    Error::<T>::RingInvariantViolation
                );
                ensure!(
                    !OffenceIdentityIndex::<T>::contains_key(identity_key),
                    Error::<T>::IdentityIndexInvariant
                );
                ensure!(
                    !Observations::<T>::contains_key(observation_key),
                    Error::<T>::ObservationKeyPreimageMismatch
                );
                let record = OffenceObservationV1 {
                    version: OBSERVATION_VERSION,
                    kind: item.kind,
                    offender: item.offender,
                    session_index: item.session_index,
                    babe_slot: item.babe_slot,
                    grandpa_coordinate: item.grandpa_coordinate,
                    proof_hash: item.proof_hash,
                    observed_at: now,
                    source,
                };
                Observations::<T>::insert(observation_key, &record);
                OffenceIdentityIndex::<T>::insert(
                    identity_key,
                    IdentityIndexEntryV1 {
                        observation_key,
                        ring_index: tail,
                    },
                );
                ObservationRing::<T>::insert(
                    tail,
                    RingEntry {
                        identity_key,
                        observation_key,
                        expires_at,
                    },
                );
                Self::deposit_event(Event::<T>::ObservationStored {
                    identity_key,
                    observation_key,
                    record,
                });
                tail = Self::next_ring_index(tail)?;
                count = count
                    .checked_add(1)
                    .ok_or(Error::<T>::RingInvariantViolation)?;
            }
            RingTail::<T>::put(tail);
            ObservationCount::<T>::put(count);
            NewObservationsInBlock::<T>::put((now, next_new));
            Self::validate_ring_scalars()?;

            Ok(RecordOutcome {
                stored: unique_count,
                duplicates,
                pruned,
            })
        }

        fn prune_expired(now: u32) -> Result<(u32, u32), Error<T>> {
            let (mut head, _, mut count) = Self::validate_ring_scalars()?;
            let mut pruned = 0u32;
            while pruned < MAX_PRUNE_PER_REPORT && count > 0 {
                let entry =
                    ObservationRing::<T>::get(head).ok_or(Error::<T>::RingInvariantViolation)?;
                let index_entry = OffenceIdentityIndex::<T>::get(entry.identity_key)
                    .ok_or(Error::<T>::IdentityIndexInvariant)?;
                ensure!(
                    index_entry.ring_index == head
                        && index_entry.observation_key == entry.observation_key,
                    Error::<T>::IdentityIndexInvariant
                );
                let record = Observations::<T>::get(entry.observation_key)
                    .ok_or(Error::<T>::RingInvariantViolation)?;
                Self::validate_record_link(entry.identity_key, &index_entry, &entry, &record)?;
                if now < entry.expires_at {
                    break;
                }

                ObservationRing::<T>::remove(head);
                Observations::<T>::remove(entry.observation_key);
                OffenceIdentityIndex::<T>::remove(entry.identity_key);
                head = Self::next_ring_index(head)?;
                count = count
                    .checked_sub(1)
                    .ok_or(Error::<T>::RingInvariantViolation)?;
                pruned = pruned
                    .checked_add(1)
                    .ok_or(Error::<T>::RingInvariantViolation)?;
            }
            RingHead::<T>::put(head);
            ObservationCount::<T>::put(count);
            ensure!(
                Self::ring_index_at(head, count)? == RingTail::<T>::get(),
                Error::<T>::RingInvariantViolation
            );
            Ok((pruned, count))
        }

        fn raw_storage_version() -> RawStorageVersion {
            let key = StorageVersion::storage_key::<Pallet<T>>();
            let mut encoded = [0u8; 2];
            match sp_io::storage::read(&key, &mut encoded, 0) {
                None => RawStorageVersion::Absent,
                Some(2) => {
                    let mut input = encoded.as_slice();
                    match StorageVersion::decode(&mut input) {
                        Ok(version) if input.is_empty() => RawStorageVersion::Exact(version),
                        _ => RawStorageVersion::Malformed,
                    }
                }
                Some(_) => RawStorageVersion::Malformed,
            }
        }

        pub(super) fn raw_namespace_state() -> RawNamespaceState {
            let prefix = Self::pallet_prefix();
            // Both raw reads are intentional and are covered by every absent-version
            // migration benchmark: an exact 16-byte key is not returned by next_key.
            let exact_prefix_exists = sp_io::storage::exists(&prefix);
            let successor_inside = matches!(
                sp_io::storage::next_key(&prefix),
                Some(key) if key.starts_with(&prefix)
            );
            if exact_prefix_exists {
                RawNamespaceState::ExactPrefix
            } else if successor_inside {
                RawNamespaceState::SuccessorPrefix
            } else {
                RawNamespaceState::Absent
            }
        }

        fn pallet_prefix() -> [u8; 16] {
            sp_io::hashing::twox_128(<Pallet<T> as PalletInfoAccess>::name().as_bytes())
        }

        #[cfg(any(test, feature = "try-runtime"))]
        pub(super) fn namespace_commitment() -> [u8; 32] {
            let prefix = Self::pallet_prefix();
            let mut cursor = prefix.to_vec();
            let mut encoded = alloc::vec::Vec::new();
            if let Some(value) = sp_io::storage::get(&prefix) {
                prefix.encode_to(&mut encoded);
                value.encode_to(&mut encoded);
            }
            while let Some(key) = sp_io::storage::next_key(&cursor) {
                if !key.starts_with(&prefix) {
                    break;
                }
                cursor = key.clone();
                key.encode_to(&mut encoded);
                sp_io::storage::get(&key)
                    .unwrap_or_default()
                    .encode_to(&mut encoded);
            }
            sp_io::hashing::blake2_256(&encoded)
        }

        #[cfg(feature = "try-runtime")]
        fn namespace_contains_only_storage_version() -> bool {
            let prefix = Self::pallet_prefix();
            if sp_io::storage::exists(&prefix) {
                return false;
            }
            let version_key = StorageVersion::storage_key::<Pallet<T>>();
            let mut cursor = prefix.to_vec();
            let mut seen_version = false;
            while let Some(key) = sp_io::storage::next_key(&cursor) {
                if !key.starts_with(&prefix) {
                    break;
                }
                cursor = key.clone();
                if key == version_key && !seen_version {
                    seen_version = true;
                } else {
                    return false;
                }
            }
            seen_version
        }

        #[cfg(any(test, feature = "try-runtime"))]
        pub(super) fn validate_complete_state() -> Result<(), Error<T>> {
            let (head, tail, count) = Self::validate_ring_scalars()?;
            ensure!(
                matches!(
                    Self::raw_storage_version(),
                    RawStorageVersion::Exact(version) if version == STORAGE_VERSION
                ),
                Error::<T>::UnsupportedStorageVersion
            );
            ensure!(
                NewObservationsInBlock::<T>::get()
                    .map(|(_, new_count)| new_count <= MAX_NEW_OBSERVATIONS_PER_BLOCK)
                    .unwrap_or(true),
                Error::<T>::TooManyNewObservationsThisBlock
            );
            ensure!(
                PendingEvidenceContext::<T>::get().is_none()
                    && PendingAdapterError::<T>::get().is_none(),
                Error::<T>::EvidenceContextOccupied
            );
            let mut index = head;
            for _ in 0..count {
                let ring =
                    ObservationRing::<T>::get(index).ok_or(Error::<T>::RingInvariantViolation)?;
                let identity = OffenceIdentityIndex::<T>::get(ring.identity_key)
                    .ok_or(Error::<T>::IdentityIndexInvariant)?;
                ensure!(
                    identity.ring_index == index
                        && identity.observation_key == ring.observation_key,
                    Error::<T>::IdentityIndexInvariant
                );
                let record = Observations::<T>::get(ring.observation_key)
                    .ok_or(Error::<T>::RingInvariantViolation)?;
                Self::validate_record_link(ring.identity_key, &identity, &ring, &record)?;
                index = Self::next_ring_index(index)?;
            }
            ensure!(index == tail, Error::<T>::RingInvariantViolation);
            ensure!(
                Observations::<T>::iter_keys().count() == count as usize
                    && OffenceIdentityIndex::<T>::iter_keys().count() == count as usize
                    && ObservationRing::<T>::iter_keys().count() == count as usize,
                Error::<T>::RingInvariantViolation
            );
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::pallet::RawNamespaceState;
    use super::*;
    use frame_support::{
        assert_noop, assert_ok, construct_runtime, derive_impl,
        traits::{ConstU32, Hooks, PalletInfoAccess, StorageVersion},
        BoundedVec,
    };
    use sp_runtime::{BuildStorage, StateVersion};

    type Block = frame_system::mocking::MockBlock<Test>;

    construct_runtime! {
        pub enum Test {
            System: frame_system,
            ValidatorSecurity: crate::observation::{Pallet, Storage, Event<T>},
        }
    }

    #[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
    impl frame_system::Config for Test {
        type Block = Block;
        type AccountId = u64;
    }

    impl Config for Test {
        #[cfg(feature = "runtime-benchmarks")]
        type BenchmarkHelper = StandaloneBenchmarkGuard;
        type ObserverWeightInfo = ();
        type BabeMaxAuthorities = ConstU32<16>;
        type GrandpaMaxAuthorities = ConstU32<16>;
    }

    // These unit tests exercise storage, never consensus benchmarking. Keep them enabled
    // with runtime-benchmarks; any accidental use of the end-to-end helper fails loudly.
    // Actual report measurements continue to use the real runtime helper unchanged.
    #[cfg(feature = "runtime-benchmarks")]
    pub struct StandaloneBenchmarkGuard;
    #[cfg(feature = "runtime-benchmarks")]
    impl BenchmarkHelper<Test> for StandaloneBenchmarkGuard {
        type BabeEvidence = ();
        type GrandpaEvidence = ();
        type PrivateBatch = ();
        fn babe_max_authorities() -> u32 {
            panic!("standalone storage tests must not invoke consensus benchmarks")
        }
        fn grandpa_max_authorities() -> u32 {
            panic!("standalone storage tests must not invoke consensus benchmarks")
        }
        fn setup_babe(
            _validator_count: u32,
            _max_nominators_per_validator: u32,
            _prune: u32,
        ) -> Result<Self::BabeEvidence, sp_runtime::DispatchError> {
            panic!("standalone storage tests must not invoke consensus benchmarks")
        }
        fn setup_grandpa(
            _validator_count: u32,
            _max_nominators_per_validator: u32,
            _prune: u32,
        ) -> Result<Self::GrandpaEvidence, sp_runtime::DispatchError> {
            panic!("standalone storage tests must not invoke consensus benchmarks")
        }
        fn setup_invalid_babe(
            _validator_count: u32,
        ) -> Result<Self::BabeEvidence, sp_runtime::DispatchError> {
            panic!("standalone storage tests must not invoke consensus benchmarks")
        }
        fn setup_invalid_grandpa(
            _validator_count: u32,
        ) -> Result<Self::GrandpaEvidence, sp_runtime::DispatchError> {
            panic!("standalone storage tests must not invoke consensus benchmarks")
        }
        fn invalid_babe_error() -> sp_runtime::DispatchError {
            panic!("standalone storage tests must not invoke consensus benchmarks")
        }
        fn invalid_grandpa_error() -> sp_runtime::DispatchError {
            panic!("standalone storage tests must not invoke consensus benchmarks")
        }
        fn check_babe(
            _evidence: &Self::BabeEvidence,
        ) -> Result<(), sp_runtime::transaction_validity::TransactionValidityError> {
            panic!("standalone storage tests must not invoke consensus benchmarks")
        }
        fn check_grandpa(
            _evidence: &Self::GrandpaEvidence,
        ) -> Result<(), sp_runtime::transaction_validity::TransactionValidityError> {
            panic!("standalone storage tests must not invoke consensus benchmarks")
        }
        fn process_babe(_evidence: Self::BabeEvidence) -> sp_runtime::DispatchResult {
            panic!("standalone storage tests must not invoke consensus benchmarks")
        }
        fn process_grandpa(_evidence: Self::GrandpaEvidence) -> sp_runtime::DispatchResult {
            panic!("standalone storage tests must not invoke consensus benchmarks")
        }
        fn setup_private_batch(
            _n: u32,
            _unique_new: u32,
            _prune: u32,
        ) -> Result<Self::PrivateBatch, sp_runtime::DispatchError> {
            panic!("standalone storage tests must not invoke consensus benchmarks")
        }
        fn setup_capacity_full_babe(
            _validator_count: u32,
        ) -> Result<Self::BabeEvidence, sp_runtime::DispatchError> {
            panic!("standalone storage tests must not invoke consensus benchmarks")
        }
        fn setup_capacity_full_grandpa(
            _validator_count: u32,
        ) -> Result<Self::GrandpaEvidence, sp_runtime::DispatchError> {
            panic!("standalone storage tests must not invoke consensus benchmarks")
        }
        fn process_private_batch(
            _batch: Self::PrivateBatch,
        ) -> Result<RecordOutcome, sp_runtime::DispatchError> {
            panic!("standalone storage tests must not invoke consensus benchmarks")
        }
        fn observer_state_commitment() -> [u8; 32] {
            panic!("standalone storage tests must not invoke consensus benchmarks")
        }
        fn observer_counters_and_transients() -> ObserverCountersAndTransients {
            panic!("standalone storage tests must not invoke consensus benchmarks")
        }
        fn observer_event_count() -> u32 {
            panic!("standalone storage tests must not invoke consensus benchmarks")
        }
    }

    // FRAME pallet errors have no PartialEq implementation. Compare their exact SCALE
    // representation in this mock only; keep every result and no-op assertion intact.
    impl PartialEq for Error<Test> {
        fn eq(&self, other: &Self) -> bool {
            self.encode() == other.encode()
        }
    }

    impl WeightInfo for () {
        fn babe_new(_: u32) -> Weight {
            Weight::zero()
        }
        fn babe_duplicate(_: u32) -> Weight {
            Weight::zero()
        }
        fn babe_invalid(_: u32) -> Weight {
            Weight::zero()
        }
        fn babe_capacity_full(_: u32) -> Weight {
            Weight::zero()
        }
        fn grandpa_new(_: u32) -> Weight {
            Weight::zero()
        }
        fn grandpa_duplicate(_: u32) -> Weight {
            Weight::zero()
        }
        fn grandpa_invalid(_: u32) -> Weight {
            Weight::zero()
        }
        fn grandpa_capacity_full(_: u32) -> Weight {
            Weight::zero()
        }
        fn migration_initialize_absent() -> Weight {
            Weight::zero()
        }
        fn migration_reject_dirty(_: u32) -> Weight {
            Weight::zero()
        }
        fn migration_reject_version(_: u32) -> Weight {
            Weight::zero()
        }
        fn migration_repeated_v1() -> Weight {
            Weight::zero()
        }
    }

    fn ext() -> sp_io::TestExternalities {
        frame_system::GenesisConfig::<Test>::default()
            .build_storage()
            .expect("system genesis")
            .into()
    }

    fn initialize_v1(block: u64) {
        StorageVersion::new(1).put::<ValidatorSecurity>();
        System::set_block_number(block);
    }

    fn input(offender: u64, proof: u8) -> ObservationInput<u64> {
        ObservationInput {
            kind: ObservationKind::Babe,
            offender,
            session_index: 0,
            babe_slot: Some(offender),
            grandpa_coordinate: None,
            proof_hash: [proof; 32],
        }
    }

    fn batch(
        items: alloc::vec::Vec<ObservationInput<u64>>,
    ) -> BoundedVec<ObservationInput<u64>, ConstU32<MAX_OBSERVATION_BATCH>> {
        items.try_into().expect("bounded test batch")
    }

    fn record(item: &ObservationInput<u64>, observed_at: u32) -> OffenceObservationV1<u64> {
        OffenceObservationV1 {
            version: OBSERVATION_VERSION,
            kind: item.kind,
            offender: item.offender,
            session_index: item.session_index,
            babe_slot: item.babe_slot,
            grandpa_coordinate: item.grandpa_coordinate,
            proof_hash: item.proof_hash,
            observed_at,
            source: ObservationSource::RehearsalImport,
        }
    }

    fn insert_consistent(
        item: &ObservationInput<u64>,
        observed_at: u32,
        ring_index: u32,
    ) -> ([u8; 32], [u8; 32]) {
        let record = record(item, observed_at);
        let identity_key = ValidatorSecurity::identity_key(&record);
        let observation_key = ValidatorSecurity::observation_key(&record);
        Observations::<Test>::insert(observation_key, &record);
        OffenceIdentityIndex::<Test>::insert(
            identity_key,
            IdentityIndexEntryV1 {
                observation_key,
                ring_index,
            },
        );
        ObservationRing::<Test>::insert(
            ring_index,
            RingEntry {
                identity_key,
                observation_key,
                expires_at: observed_at + OBSERVATION_LONGEVITY,
            },
        );
        (identity_key, observation_key)
    }

    fn fill_consistent_ring(
        count: u32,
        head: u32,
        first_observed_at: u32,
        remaining_observed_at: u32,
    ) -> ([u8; 32], [u8; 32]) {
        assert!(count > 0 && count <= MAX_RETAINED_OBSERVATIONS);
        let mut first = None;
        for offset in 0..count {
            let ring_index =
                ((head as u64 + offset as u64) % MAX_RETAINED_OBSERVATIONS as u64) as u32;
            let item = input(1_000 + offset as u64, (offset % 251) as u8);
            let keys = insert_consistent(
                &item,
                if offset == 0 {
                    first_observed_at
                } else {
                    remaining_observed_at
                },
                ring_index,
            );
            if offset == 0 {
                first = Some(keys);
            }
        }
        RingHead::<Test>::put(head);
        RingTail::<Test>::put(
            ((head as u64 + count as u64) % MAX_RETAINED_OBSERVATIONS as u64) as u32,
        );
        ObservationCount::<Test>::put(count);
        first.expect("nonempty consistent ring")
    }

    fn state_root() -> alloc::vec::Vec<u8> {
        sp_io::storage::root(StateVersion::V1)
    }

    fn version_key() -> [u8; 32] {
        StorageVersion::storage_key::<ValidatorSecurity>()
    }

    fn pallet_prefix() -> [u8; 16] {
        sp_io::hashing::twox_128(<ValidatorSecurity as PalletInfoAccess>::name().as_bytes())
    }

    fn dirty_namespace_key() -> alloc::vec::Vec<u8> {
        let mut key = pallet_prefix().to_vec();
        key.extend_from_slice(b":rev4-dirty-successor:");
        key
    }

    fn first_key_outside_prefix() -> alloc::vec::Vec<u8> {
        let mut key = pallet_prefix().to_vec();
        for byte in key.iter_mut().rev() {
            if *byte != u8::MAX {
                *byte += 1;
                return key;
            }
            *byte = 0;
        }
        panic!("16-byte pallet prefix has a lexicographic successor");
    }

    #[test]
    fn source_is_derived_only_by_gated_entrypoint() {
        ext().execute_with(|| {
            initialize_v1(1);
            assert_ok!(ValidatorSecurity::begin_evidence(
                ObservationKind::Babe,
                [1; 32],
            ));
            assert_ok!(ValidatorSecurity::record_validated_babe(1, 0, 1));
            let production = Observations::<Test>::iter_values()
                .next()
                .expect("production record");
            assert_eq!(
                production.source,
                ObservationSource::LocalUnsignedValidation
            );

            System::set_block_number(2);
            assert_ok!(ValidatorSecurity::record_rehearsal_batch(batch(
                alloc::vec![input(2, 2)],
            )));
            assert!(Observations::<Test>::iter_values()
                .any(|record| record.source == ObservationSource::RehearsalImport));
        });
    }

    #[test]
    fn partial_insert_failure_rolls_back_record_counter_transient_and_event() {
        ext().execute_with(|| {
            initialize_v1(1);
            ObservationRing::<Test>::insert(
                1,
                RingEntry {
                    identity_key: [7; 32],
                    observation_key: [8; 32],
                    expires_at: 100,
                },
            );
            let events_before = System::events();
            assert_noop!(
                ValidatorSecurity::record_rehearsal_batch(batch(alloc::vec![
                    input(1, 1),
                    input(2, 2),
                ])),
                Error::<Test>::RingInvariantViolation
            );
            assert_eq!(Observations::<Test>::iter_keys().count(), 0);
            assert_eq!(OffenceIdentityIndex::<Test>::iter_keys().count(), 0);
            assert_eq!(ObservationCount::<Test>::get(), 0);
            assert_eq!(RingTail::<Test>::get(), 0);
            assert!(ObservationRing::<Test>::contains_key(1));
            assert_eq!(System::events(), events_before);
            assert!(ValidatorSecurity::transient_state_is_clear());
        });
    }

    #[test]
    fn partial_prune_then_capacity_failure_rolls_back_every_delete_and_event() {
        ext().execute_with(|| {
            initialize_v1(14_401);
            let (identity_key, observation_key) =
                fill_consistent_ring(MAX_RETAINED_OBSERVATIONS, 0, 1, 2);
            assert_eq!(Observations::<Test>::iter_keys().count(), 8_192);
            assert_eq!(OffenceIdentityIndex::<Test>::iter_keys().count(), 8_192);
            assert_eq!(ObservationRing::<Test>::iter_keys().count(), 8_192);
            assert_eq!(
                ObservationRing::<Test>::get(0).expect("head").expires_at,
                14_401
            );
            assert_eq!(
                ObservationRing::<Test>::get(1).expect("live").expires_at,
                14_402
            );
            let events_before = System::events();
            let root_before = state_root();
            assert_noop!(
                ValidatorSecurity::record_rehearsal_batch(batch(alloc::vec![
                    input(20_000, 1),
                    input(20_001, 2),
                ])),
                Error::<Test>::CapacityFull
            );
            assert_eq!(state_root(), root_before);
            assert!(Observations::<Test>::contains_key(observation_key));
            assert!(OffenceIdentityIndex::<Test>::contains_key(identity_key));
            assert!(ObservationRing::<Test>::contains_key(0));
            assert_eq!(ObservationCount::<Test>::get(), MAX_RETAINED_OBSERVATIONS);
            assert_eq!(RingHead::<Test>::get(), 0);
            assert_eq!(RingTail::<Test>::get(), 0);
            assert!(NewObservationsInBlock::<Test>::get().is_none());
            assert_eq!(System::events(), events_before);
        });
    }

    #[test]
    fn deliberately_inconsistent_full_ring_returns_invariant_not_capacity() {
        ext().execute_with(|| {
            initialize_v1(14_401);
            insert_consistent(&input(99, 99), 1, 0);
            RingHead::<Test>::put(0);
            RingTail::<Test>::put(0);
            ObservationCount::<Test>::put(MAX_RETAINED_OBSERVATIONS);
            let root_before = state_root();
            let events_before = System::events();
            assert_noop!(
                ValidatorSecurity::record_rehearsal_batch(batch(alloc::vec![input(100, 100)])),
                Error::<Test>::RingInvariantViolation
            );
            assert_eq!(state_root(), root_before);
            assert_eq!(System::events(), events_before);
        });
    }

    #[test]
    fn identity_observation_ring_and_expiry_corruption_are_detected_o1() {
        ext().execute_with(|| {
            initialize_v1(1);
            assert_ok!(ValidatorSecurity::record_rehearsal_batch(batch(
                alloc::vec![input(1, 1)],
            )));
            let identity_key = OffenceIdentityIndex::<Test>::iter_keys()
                .next()
                .expect("identity");
            let original_identity = OffenceIdentityIndex::<Test>::get(identity_key).expect("entry");
            let original_ring =
                ObservationRing::<Test>::get(original_identity.ring_index).expect("ring");
            let original_record =
                Observations::<Test>::get(original_identity.observation_key).expect("record");

            OffenceIdentityIndex::<Test>::mutate(identity_key, |entry| {
                entry.as_mut().expect("entry").ring_index = 1;
            });
            assert_noop!(
                ValidatorSecurity::identity_known_babe(1, 0, 1),
                Error::<Test>::IdentityIndexInvariant
            );
            OffenceIdentityIndex::<Test>::insert(identity_key, &original_identity);

            ObservationRing::<Test>::mutate(original_identity.ring_index, |entry| {
                entry.as_mut().expect("ring").expires_at += 1;
            });
            assert_noop!(
                ValidatorSecurity::identity_known_babe(1, 0, 1),
                Error::<Test>::RingInvariantViolation
            );
            ObservationRing::<Test>::insert(original_identity.ring_index, &original_ring);

            Observations::<Test>::mutate(original_identity.observation_key, |record| {
                record.as_mut().expect("record").babe_slot = None;
            });
            assert_noop!(
                ValidatorSecurity::identity_known_babe(1, 0, 1),
                Error::<Test>::InvalidCoordinateShape
            );
            Observations::<Test>::insert(original_identity.observation_key, &original_record);

            Observations::<Test>::mutate(original_identity.observation_key, |record| {
                record.as_mut().expect("record").proof_hash = [3; 32];
            });
            assert_noop!(
                ValidatorSecurity::identity_known_babe(1, 0, 1),
                Error::<Test>::ObservationKeyPreimageMismatch
            );
        });
    }

    #[test]
    fn head_tail_and_count_corruption_fail_before_mutation() {
        for corrupt in 0..3 {
            ext().execute_with(|| {
                initialize_v1(1);
                match corrupt {
                    0 => RingHead::<Test>::put(MAX_RETAINED_OBSERVATIONS),
                    1 => RingTail::<Test>::put(1),
                    _ => ObservationCount::<Test>::put(MAX_RETAINED_OBSERVATIONS + 1),
                }
                assert_noop!(
                    ValidatorSecurity::record_rehearsal_batch(batch(alloc::vec![input(1, 1)],)),
                    Error::<Test>::RingInvariantViolation
                );
                assert_eq!(Observations::<Test>::iter_keys().count(), 0);
                assert_eq!(OffenceIdentityIndex::<Test>::iter_keys().count(), 0);
            });
        }
    }

    #[test]
    fn codec_identity_and_canonical_hash_golden_vectors_are_exact() {
        assert_eq!(ObservationKind::Babe.encode(), alloc::vec![0]);
        assert_eq!(ObservationKind::Grandpa.encode(), alloc::vec![1]);
        assert_eq!(
            ObservationSource::LocalUnsignedValidation.encode(),
            alloc::vec![0]
        );
        assert_eq!(ObservationSource::RehearsalImport.encode(), alloc::vec![1]);

        let babe_identity = OffenceIdentityPreimage {
            domain: OBSERVATION_DOMAIN,
            version: 1,
            kind: ObservationKind::Babe,
            offender: 1u64,
            session_index: 2,
            babe_slot: Some(3),
            grandpa_coordinate: None,
        };
        let babe_identity_bytes = [
            b"era/v14/offence-observation/v1".as_slice(),
            &[
                1, 0, 1, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 1, 3, 0, 0, 0, 0, 0, 0, 0, 0,
            ],
        ]
        .concat();
        assert_eq!(babe_identity.encode(), babe_identity_bytes);
        assert_eq!(
            sp_io::hashing::blake2_256(&babe_identity.encode()),
            [
                0xa0, 0x65, 0xc8, 0xe0, 0xee, 0xab, 0x55, 0x88, 0x80, 0xb1, 0xfa, 0x05, 0xcc, 0x67,
                0x88, 0x68, 0x48, 0xfe, 0xbd, 0x32, 0xef, 0x15, 0xd9, 0xd0, 0xb5, 0x93, 0xbe, 0x74,
                0x09, 0x3e, 0x36, 0x16,
            ]
        );
        let babe_observation = ObservationKeyPreimage {
            domain: OBSERVATION_DOMAIN,
            version: 1,
            kind: ObservationKind::Babe,
            offender: 1u64,
            session_index: 2,
            babe_slot: Some(3),
            grandpa_coordinate: None,
            proof_hash: [0x11; 32],
        };
        let mut babe_observation_bytes = babe_identity_bytes.clone();
        babe_observation_bytes.extend_from_slice(&[0x11; 32]);
        assert_eq!(babe_observation.encode(), babe_observation_bytes);
        assert_eq!(
            sp_io::hashing::blake2_256(&babe_observation.encode()),
            [
                0xee, 0x0a, 0xe7, 0x89, 0xf2, 0xf6, 0x21, 0x79, 0xd0, 0x03, 0x33, 0x9d, 0xac, 0x54,
                0xc1, 0xb9, 0xa0, 0x75, 0xbe, 0x77, 0x12, 0x37, 0x82, 0x37, 0x80, 0x6f, 0xf5, 0x23,
                0x7a, 0x3d, 0xfa, 0x87,
            ]
        );

        let grandpa_identity = OffenceIdentityPreimage {
            domain: OBSERVATION_DOMAIN,
            version: 1,
            kind: ObservationKind::Grandpa,
            offender: 1u64,
            session_index: 2,
            babe_slot: None,
            grandpa_coordinate: Some((3, 4)),
        };
        let grandpa_identity_bytes = [
            b"era/v14/offence-observation/v1".as_slice(),
            &[
                1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 1, 3, 0, 0, 0, 0, 0, 0, 0, 4, 0, 0, 0,
                0, 0, 0, 0,
            ],
        ]
        .concat();
        assert_eq!(grandpa_identity.encode(), grandpa_identity_bytes);
        assert_eq!(
            sp_io::hashing::blake2_256(&grandpa_identity.encode()),
            [
                0xd0, 0xed, 0xad, 0x7a, 0x2e, 0x57, 0x5e, 0x1f, 0x44, 0xde, 0x41, 0x65, 0xc3, 0xe8,
                0x38, 0xfa, 0x50, 0x77, 0xef, 0x41, 0x35, 0xf6, 0x95, 0x6c, 0xd0, 0x02, 0x33, 0x6f,
                0x46, 0x2a, 0x90, 0x70,
            ]
        );
        let grandpa_observation = ObservationKeyPreimage {
            domain: OBSERVATION_DOMAIN,
            version: 1,
            kind: ObservationKind::Grandpa,
            offender: 1u64,
            session_index: 2,
            babe_slot: None,
            grandpa_coordinate: Some((3, 4)),
            proof_hash: [0x22; 32],
        };
        let mut grandpa_observation_bytes = grandpa_identity_bytes.clone();
        grandpa_observation_bytes.extend_from_slice(&[0x22; 32]);
        assert_eq!(grandpa_observation.encode(), grandpa_observation_bytes);
        assert_eq!(
            sp_io::hashing::blake2_256(&grandpa_observation.encode()),
            [
                0x17, 0x5c, 0x40, 0xa9, 0x8e, 0x16, 0xa2, 0xc6, 0xbf, 0xcb, 0x02, 0x33, 0xff, 0x7f,
                0x99, 0x7d, 0x01, 0x3c, 0xff, 0x65, 0x1f, 0x34, 0x08, 0x20, 0x6f, 0x75, 0x71, 0xc5,
                0x89, 0x7e, 0xa2, 0xee,
            ]
        );

        let record = OffenceObservationV1 {
            version: 1,
            kind: ObservationKind::Babe,
            offender: 1u64,
            session_index: 2,
            babe_slot: Some(3),
            grandpa_coordinate: None,
            proof_hash: [0x11; 32],
            observed_at: 4,
            source: ObservationSource::LocalUnsignedValidation,
        };
        let record_bytes = [
            &[
                1, 0, 1, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 1, 3, 0, 0, 0, 0, 0, 0, 0, 0,
            ][..],
            &[0x11; 32],
            &[4, 0, 0, 0, 0],
        ]
        .concat();
        assert_eq!(record.encode(), record_bytes);
        let mut record_input = record_bytes.as_slice();
        assert_eq!(
            OffenceObservationV1::<u64>::decode(&mut record_input),
            Ok(record.clone())
        );
        assert!(record_input.is_empty());

        let identity_entry = IdentityIndexEntryV1 {
            observation_key: [0x22; 32],
            ring_index: 5,
        };
        let identity_entry_bytes = [&[0x22; 32][..], &[5, 0, 0, 0]].concat();
        assert_eq!(identity_entry.encode(), identity_entry_bytes);
        let mut identity_input = identity_entry_bytes.as_slice();
        assert_eq!(
            IdentityIndexEntryV1::decode(&mut identity_input),
            Ok(identity_entry.clone())
        );
        assert!(identity_input.is_empty());

        let ring_entry = RingEntry {
            identity_key: [0x33; 32],
            observation_key: [0x44; 32],
            expires_at: 6,
        };
        let ring_entry_bytes = [&[0x33; 32][..], &[0x44; 32], &[6, 0, 0, 0]].concat();
        assert_eq!(ring_entry.encode(), ring_entry_bytes);
        let mut ring_input = ring_entry_bytes.as_slice();
        assert_eq!(RingEntry::decode(&mut ring_input), Ok(ring_entry.clone()));
        assert!(ring_input.is_empty());

        let stored_event = Event::<Test>::ObservationStored {
            identity_key: [0x55; 32],
            observation_key: [0x66; 32],
            record: record.clone(),
        };
        let stored_event_bytes =
            [&[0][..], &[0x55; 32], &[0x66; 32], record_bytes.as_slice()].concat();
        assert_eq!(stored_event.encode(), stored_event_bytes);
        let mut stored_event_input = stored_event_bytes.as_slice();
        assert_eq!(
            Event::<Test>::decode(&mut stored_event_input),
            Ok(stored_event)
        );
        assert!(stored_event_input.is_empty());

        let pruned_event = Event::<Test>::ExpiredObservationsPruned {
            count: 7,
            new_head: 8,
            remaining: 9,
        };
        let pruned_event_bytes = alloc::vec![1, 7, 0, 0, 0, 8, 0, 0, 0, 9, 0, 0, 0];
        assert_eq!(pruned_event.encode(), pruned_event_bytes);
        let mut pruned_event_input = pruned_event_bytes.as_slice();
        assert_eq!(
            Event::<Test>::decode(&mut pruned_event_input),
            Ok(pruned_event)
        );
        assert!(pruned_event_input.is_empty());

        let stable_errors = [
            Error::<Test>::UnsupportedStorageVersion,
            Error::<Test>::InvalidCoordinateShape,
            Error::<Test>::EmptyObservationBatch,
            Error::<Test>::ObservationBatchTooLarge,
            Error::<Test>::TooManyNewObservationsThisBlock,
            Error::<Test>::CapacityFull,
            Error::<Test>::ObservationKeyPreimageMismatch,
            Error::<Test>::IdentityKeyPreimageMismatch,
            Error::<Test>::IdentityIndexInvariant,
            Error::<Test>::BlockNumberOverflow,
            Error::<Test>::ExpiryOverflow,
            Error::<Test>::RingIndexOverflow,
            Error::<Test>::RingInvariantViolation,
            Error::<Test>::EvidenceContextOccupied,
            Error::<Test>::EvidenceContextMissing,
            Error::<Test>::EvidenceContextMismatch,
            Error::<Test>::SignedIntakeRejected,
            Error::<Test>::AdapterFailure,
        ];
        for (index, error) in stable_errors.into_iter().enumerate() {
            assert_eq!(error.encode(), alloc::vec![index as u8]);
            assert_eq!(ValidatorSecurity::adapter_error_code(error), index as u8);
        }
    }

    #[test]
    fn duplicates_are_immutable_across_later_blocks_and_source_contexts() {
        ext().execute_with(|| {
            initialize_v1(1);
            assert_ok!(ValidatorSecurity::begin_evidence(
                ObservationKind::Babe,
                [1; 32]
            ));
            assert_eq!(
                ValidatorSecurity::record_validated_babe(1, 0, 1),
                Ok(RecordOutcome {
                    stored: 1,
                    duplicates: 0,
                    pruned: 0
                })
            );
            let before = ValidatorSecurity::namespace_commitment();
            let events_before = System::events();
            System::set_block_number(2);
            assert_eq!(
                ValidatorSecurity::record_rehearsal_batch(batch(alloc::vec![input(1, 2)])),
                Ok(RecordOutcome {
                    stored: 0,
                    duplicates: 1,
                    pruned: 0
                })
            );
            assert_eq!(ValidatorSecurity::namespace_commitment(), before);
            assert_eq!(System::events(), events_before);
            let stored = Observations::<Test>::iter_values()
                .next()
                .expect("production record");
            assert_eq!(stored.proof_hash, [1; 32]);
            assert_eq!(stored.observed_at, 1);
            assert_eq!(stored.source, ObservationSource::LocalUnsignedValidation);
        });

        ext().execute_with(|| {
            initialize_v1(1);
            assert_ok!(ValidatorSecurity::record_rehearsal_batch(batch(
                alloc::vec![input(1, 3)]
            )));
            let before = ValidatorSecurity::namespace_commitment();
            let events_before = System::events();
            System::set_block_number(9);
            assert_ok!(ValidatorSecurity::begin_evidence(
                ObservationKind::Babe,
                [4; 32]
            ));
            assert_eq!(
                ValidatorSecurity::record_validated_babe(1, 0, 1),
                Ok(RecordOutcome {
                    stored: 0,
                    duplicates: 1,
                    pruned: 0
                })
            );
            assert_eq!(ValidatorSecurity::namespace_commitment(), before);
            assert_eq!(System::events(), events_before);
            let stored = Observations::<Test>::iter_values()
                .next()
                .expect("rehearsal record");
            assert_eq!(stored.proof_hash, [3; 32]);
            assert_eq!(stored.observed_at, 1);
            assert_eq!(stored.source, ObservationSource::RehearsalImport);
            assert!(ValidatorSecurity::transient_state_is_clear());
        });
    }

    #[test]
    fn first_valid_proof_wins_and_mixed_duplicate_new_batch_is_atomic() {
        ext().execute_with(|| {
            initialize_v1(1);
            let first = input(1, 1);
            let replay = input(1, 2);
            let outcome = ValidatorSecurity::record_rehearsal_batch(batch(alloc::vec![
                first.clone(),
                replay,
            ]))
            .expect("first valid proof wins");
            assert_eq!(
                outcome,
                RecordOutcome {
                    stored: 1,
                    duplicates: 1,
                    pruned: 0
                }
            );
            let stored = Observations::<Test>::iter_values().next().expect("stored");
            assert_eq!(stored.proof_hash, first.proof_hash);
            assert_eq!(ObservationCount::<Test>::get(), 1);
            assert_eq!(System::events().len(), 1);

            let mixed = ValidatorSecurity::record_rehearsal_batch(batch(alloc::vec![
                input(1, 9),
                input(2, 2),
            ]))
            .expect("mixed duplicate/new");
            assert_eq!(
                mixed,
                RecordOutcome {
                    stored: 1,
                    duplicates: 1,
                    pruned: 0
                }
            );
            assert_eq!(ObservationCount::<Test>::get(), 2);
            assert_eq!(NewObservationsInBlock::<Test>::get(), Some((1, 2)));
            assert_eq!(System::events().len(), 2);
        });
    }

    #[test]
    fn per_block_fifteen_plus_one_sixteen_and_sixteen_plus_one_are_exact() {
        ext().execute_with(|| {
            initialize_v1(1);
            let first_fifteen = (0..15).map(|i| input(10 + i, i as u8)).collect();
            let fifteen =
                ValidatorSecurity::record_rehearsal_batch(batch(first_fifteen)).expect("15 new");
            assert_eq!(fifteen.stored, 15);
            assert_eq!(
                ValidatorSecurity::record_rehearsal_batch(batch(alloc::vec![input(30, 30)]))
                    .expect("15+1")
                    .stored,
                1
            );
            let root_before = state_root();
            let events_before = System::events();
            assert_noop!(
                ValidatorSecurity::record_rehearsal_batch(batch(alloc::vec![input(31, 31)])),
                Error::<Test>::TooManyNewObservationsThisBlock
            );
            assert_eq!(NewObservationsInBlock::<Test>::get(), Some((1, 16)));
            assert_eq!(ObservationCount::<Test>::get(), 16);
            assert_eq!(state_root(), root_before);
            assert_eq!(System::events(), events_before);
        });

        ext().execute_with(|| {
            initialize_v1(9);
            let sixteen = (0..16).map(|i| input(100 + i, i as u8)).collect();
            assert_eq!(
                ValidatorSecurity::record_rehearsal_batch(batch(sixteen))
                    .expect("16 exact")
                    .stored,
                16
            );
            assert_noop!(
                ValidatorSecurity::record_rehearsal_batch(batch(alloc::vec![input(200, 1)])),
                Error::<Test>::TooManyNewObservationsThisBlock
            );
            assert_eq!(ObservationCount::<Test>::get(), 16);
            assert_eq!(System::events().len(), 16);
        });
    }

    #[test]
    fn capacity_8191_8192_and_overflow_are_exact_and_event_atomic() {
        ext().execute_with(|| {
            initialize_v1(1);
            fill_consistent_ring(8_191, 0, 1, 1);
            System::set_block_number(2);
            assert_eq!(
                ValidatorSecurity::record_rehearsal_batch(batch(alloc::vec![input(20_000, 1)]))
                    .expect("8191 to 8192")
                    .stored,
                1
            );
            assert_eq!(ObservationCount::<Test>::get(), 8_192);
            let root_before = state_root();
            let events_before = System::events();
            assert_noop!(
                ValidatorSecurity::record_rehearsal_batch(batch(alloc::vec![input(20_001, 2)])),
                Error::<Test>::CapacityFull
            );
            assert_eq!(ObservationCount::<Test>::get(), 8_192);
            assert_eq!(state_root(), root_before);
            assert_eq!(System::events(), events_before);
        });
    }

    #[test]
    fn expiry_before_at_after_and_successful_ring_wrap_are_exact() {
        for (now, expected_pruned, expected_count) in
            [(14_400u64, 0u32, 2u32), (14_401, 1, 1), (14_402, 1, 1)]
        {
            ext().execute_with(|| {
                initialize_v1(now);
                insert_consistent(&input(1, 1), 1, 0);
                RingHead::<Test>::put(0);
                RingTail::<Test>::put(1);
                ObservationCount::<Test>::put(1);
                let outcome =
                    ValidatorSecurity::record_rehearsal_batch(batch(alloc::vec![input(2, 2)]))
                        .expect("expiry boundary");
                assert_eq!(outcome.pruned, expected_pruned);
                assert_eq!(ObservationCount::<Test>::get(), expected_count);
                assert_eq!(
                    System::events().len(),
                    if expected_pruned == 0 { 1 } else { 2 }
                );
            });
        }

        ext().execute_with(|| {
            initialize_v1(2);
            fill_consistent_ring(1, MAX_RETAINED_OBSERVATIONS - 1, 1, 1);
            let outcome =
                ValidatorSecurity::record_rehearsal_batch(batch(alloc::vec![input(9_999, 9)]))
                    .expect("wrap insertion");
            assert_eq!(
                outcome,
                RecordOutcome {
                    stored: 1,
                    duplicates: 0,
                    pruned: 0
                }
            );
            assert_eq!(RingHead::<Test>::get(), MAX_RETAINED_OBSERVATIONS - 1);
            assert_eq!(RingTail::<Test>::get(), 1);
            assert_eq!(ObservationCount::<Test>::get(), 2);
            assert!(ObservationRing::<Test>::contains_key(0));
        });
    }

    #[test]
    fn proof_identity_and_observation_collisions_fail_closed_without_events() {
        ext().execute_with(|| {
            initialize_v1(1);
            let candidate = input(1, 1);
            let candidate_record = record(&candidate, 1);
            let collision_key = ValidatorSecurity::observation_key(&candidate_record);
            Observations::<Test>::insert(collision_key, record(&input(2, 2), 1));
            let root_before = state_root();
            let events_before = System::events();
            assert_noop!(
                ValidatorSecurity::record_rehearsal_batch(batch(alloc::vec![candidate])),
                Error::<Test>::ObservationKeyPreimageMismatch
            );
            assert_eq!(state_root(), root_before);
            assert_eq!(System::events(), events_before);
        });

        ext().execute_with(|| {
            initialize_v1(1);
            let candidate = input(1, 1);
            let identity_key = ValidatorSecurity::identity_key(&record(&candidate, 1));
            OffenceIdentityIndex::<Test>::insert(
                identity_key,
                IdentityIndexEntryV1 {
                    observation_key: [9; 32],
                    ring_index: 0,
                },
            );
            let root_before = state_root();
            let events_before = System::events();
            assert_noop!(
                ValidatorSecurity::record_rehearsal_batch(batch(alloc::vec![candidate])),
                Error::<Test>::IdentityIndexInvariant
            );
            assert_eq!(state_root(), root_before);
            assert_eq!(System::events(), events_before);
        });
    }

    #[test]
    fn empty_malformed_and_counter_overflow_classes_emit_no_event() {
        ext().execute_with(|| {
            initialize_v1(1);
            let root_before = state_root();
            assert_noop!(
                ValidatorSecurity::record_rehearsal_batch(batch(alloc::vec![])),
                Error::<Test>::EmptyObservationBatch
            );
            let mut malformed = input(1, 1);
            malformed.babe_slot = None;
            assert_noop!(
                ValidatorSecurity::record_rehearsal_batch(batch(alloc::vec![malformed])),
                Error::<Test>::InvalidCoordinateShape
            );
            NewObservationsInBlock::<Test>::put((1, u32::MAX));
            let overflow_root = state_root();
            assert_noop!(
                ValidatorSecurity::record_rehearsal_batch(batch(alloc::vec![input(2, 2)])),
                Error::<Test>::TooManyNewObservationsThisBlock
            );
            assert_eq!(state_root(), overflow_root);
            NewObservationsInBlock::<Test>::kill();
            assert_eq!(state_root(), root_before);
            assert!(System::events().is_empty());
        });

        ext().execute_with(|| {
            initialize_v1(u64::from(u32::MAX) + 1);
            assert_noop!(
                ValidatorSecurity::record_rehearsal_batch(batch(alloc::vec![input(1, 1)])),
                Error::<Test>::BlockNumberOverflow
            );
            assert!(System::events().is_empty());
        });

        ext().execute_with(|| {
            initialize_v1(u64::from(u32::MAX - OBSERVATION_LONGEVITY + 1));
            assert_noop!(
                ValidatorSecurity::record_rehearsal_batch(batch(alloc::vec![input(1, 1)])),
                Error::<Test>::ExpiryOverflow
            );
            assert!(System::events().is_empty());
        });
    }

    #[test]
    fn complete_state_validator_covers_maps_counters_transients_and_expiry() {
        ext().execute_with(|| {
            initialize_v1(1);
            assert_ok!(ValidatorSecurity::record_rehearsal_batch(batch(
                alloc::vec![input(1, 1),]
            )));
            assert_ok!(ValidatorSecurity::validate_complete_state());

            NewObservationsInBlock::<Test>::put((1, 17));
            assert_eq!(
                ValidatorSecurity::validate_complete_state(),
                Err(Error::<Test>::TooManyNewObservationsThisBlock)
            );
            NewObservationsInBlock::<Test>::put((1, 1));

            PendingEvidenceContext::<Test>::put(EvidenceContext {
                kind: ObservationKind::Babe,
                proof_hash: [1; 32],
            });
            assert_eq!(
                ValidatorSecurity::validate_complete_state(),
                Err(Error::<Test>::EvidenceContextOccupied)
            );
            PendingEvidenceContext::<Test>::kill();

            PendingAdapterError::<Test>::put(17);
            assert_eq!(
                ValidatorSecurity::validate_complete_state(),
                Err(Error::<Test>::EvidenceContextOccupied)
            );
            PendingAdapterError::<Test>::kill();

            let index = RingHead::<Test>::get();
            ObservationRing::<Test>::mutate(index, |entry| {
                entry.as_mut().expect("ring").expires_at += 1;
            });
            assert_eq!(
                ValidatorSecurity::validate_complete_state(),
                Err(Error::<Test>::RingInvariantViolation)
            );
        });
    }

    #[test]
    fn migration_exact_prefix_successor_outside_and_absence_are_distinct() {
        ext().execute_with(|| {
            let prefix = pallet_prefix();
            sp_io::storage::set(&prefix, b"exact-prefix-contamination");
            assert!(matches!(
                ValidatorSecurity::raw_namespace_state(),
                RawNamespaceState::ExactPrefix
            ));
            let before = ValidatorSecurity::namespace_commitment();
            let events_before = System::events();
            <ValidatorSecurity as Hooks<u64>>::on_runtime_upgrade();
            assert_eq!(ValidatorSecurity::namespace_commitment(), before);
            assert_eq!(
                sp_io::storage::get(&prefix).map(|bytes| bytes.to_vec()),
                Some(b"exact-prefix-contamination".to_vec())
            );
            assert!(!StorageVersion::exists::<ValidatorSecurity>());
            assert_eq!(System::events(), events_before);
        });

        ext().execute_with(|| {
            let dirty = dirty_namespace_key();
            sp_io::storage::set(&dirty, b"successor-prefix-contamination");
            assert!(matches!(
                ValidatorSecurity::raw_namespace_state(),
                RawNamespaceState::SuccessorPrefix
            ));
            let before = ValidatorSecurity::namespace_commitment();
            let events_before = System::events();
            <ValidatorSecurity as Hooks<u64>>::on_runtime_upgrade();
            assert_eq!(ValidatorSecurity::namespace_commitment(), before);
            assert_eq!(
                sp_io::storage::get(&dirty).map(|bytes| bytes.to_vec()),
                Some(b"successor-prefix-contamination".to_vec())
            );
            assert!(!StorageVersion::exists::<ValidatorSecurity>());
            assert_eq!(System::events(), events_before);
        });

        ext().execute_with(|| {
            let prefix = pallet_prefix();
            let outside = first_key_outside_prefix();
            sp_io::storage::set(&outside, b"outside");
            let successor = sp_io::storage::next_key(&prefix).expect("outside successor");
            assert!(!successor.starts_with(&prefix));
            assert!(matches!(
                ValidatorSecurity::raw_namespace_state(),
                RawNamespaceState::Absent
            ));
            <ValidatorSecurity as Hooks<u64>>::on_runtime_upgrade();
            assert_eq!(
                StorageVersion::get::<ValidatorSecurity>(),
                StorageVersion::new(1)
            );
            assert_eq!(
                sp_io::storage::get(&outside).map(|bytes| bytes.to_vec()),
                Some(b"outside".to_vec())
            );
            assert!(System::events().is_empty());
        });

        ext().execute_with(|| {
            assert!(!sp_io::storage::exists(&pallet_prefix()));
            assert!(matches!(
                ValidatorSecurity::raw_namespace_state(),
                RawNamespaceState::Absent
            ));
            <ValidatorSecurity as Hooks<u64>>::on_runtime_upgrade();
            assert_eq!(
                StorageVersion::get::<ValidatorSecurity>(),
                StorageVersion::new(1)
            );
            assert!(System::events().is_empty());
        });
    }

    #[test]
    fn migration_absent_explicit_zero_dirty_v1_unexpected_and_malformed_are_distinct() {
        ext().execute_with(|| {
            assert!(!StorageVersion::exists::<ValidatorSecurity>());
            <ValidatorSecurity as Hooks<u64>>::on_runtime_upgrade();
            assert_eq!(
                StorageVersion::get::<ValidatorSecurity>(),
                StorageVersion::new(1)
            );
            assert!(StorageVersion::exists::<ValidatorSecurity>());
            System::set_block_number(1);
            assert_ok!(ValidatorSecurity::record_rehearsal_batch(batch(
                alloc::vec![input(1, 1)],
            )));
            let v1_root = state_root();
            <ValidatorSecurity as Hooks<u64>>::on_runtime_upgrade();
            assert_eq!(ObservationCount::<Test>::get(), 1);
            assert_eq!(state_root(), v1_root);
        });

        ext().execute_with(|| {
            StorageVersion::new(0).put::<ValidatorSecurity>();
            let before = state_root();
            <ValidatorSecurity as Hooks<u64>>::on_runtime_upgrade();
            assert_eq!(state_root(), before);
            assert_eq!(
                sp_io::storage::get(&version_key())
                    .expect("explicit zero")
                    .to_vec(),
                0u16.encode()
            );
        });

        ext().execute_with(|| {
            sp_io::storage::set(&dirty_namespace_key(), b"dirty");
            let before = state_root();
            <ValidatorSecurity as Hooks<u64>>::on_runtime_upgrade();
            assert_eq!(state_root(), before);
            assert!(!StorageVersion::exists::<ValidatorSecurity>());
        });

        ext().execute_with(|| {
            StorageVersion::new(2).put::<ValidatorSecurity>();
            let before = state_root();
            <ValidatorSecurity as Hooks<u64>>::on_runtime_upgrade();
            assert_eq!(state_root(), before);
            assert_eq!(
                StorageVersion::get::<ValidatorSecurity>(),
                StorageVersion::new(2)
            );
        });

        ext().execute_with(|| {
            sp_io::storage::set(&version_key(), &[1]);
            let before = state_root();
            <ValidatorSecurity as Hooks<u64>>::on_runtime_upgrade();
            assert_eq!(state_root(), before);
            assert_eq!(
                sp_io::storage::get(&version_key()).map(|bytes| bytes.to_vec()),
                Some(alloc::vec![1])
            );
        });

        ext().execute_with(|| {
            initialize_v1(1);
            assert_ok!(ValidatorSecurity::record_rehearsal_batch(batch(
                alloc::vec![input(1, 1)]
            )));
            ObservationRing::<Test>::remove(0);
            let before = state_root();
            <ValidatorSecurity as Hooks<u64>>::on_runtime_upgrade();
            assert_eq!(state_root(), before);
            assert_eq!(
                ValidatorSecurity::validate_complete_state(),
                Err(Error::<Test>::RingInvariantViolation)
            );
        });
    }

    #[cfg(feature = "try-runtime")]
    #[test]
    fn try_runtime_distinguishes_initial_and_repeated_nonempty_v1() {
        ext().execute_with(|| {
            let initial =
                <ValidatorSecurity as Hooks<u64>>::pre_upgrade().expect("initial pre-upgrade");
            <ValidatorSecurity as Hooks<u64>>::on_runtime_upgrade();
            <ValidatorSecurity as Hooks<u64>>::post_upgrade(initial).expect("initial post-upgrade");

            System::set_block_number(1);
            assert_ok!(ValidatorSecurity::record_rehearsal_batch(batch(
                alloc::vec![input(1, 1)],
            )));
            let repeated =
                <ValidatorSecurity as Hooks<u64>>::pre_upgrade().expect("repeated pre-upgrade");
            <ValidatorSecurity as Hooks<u64>>::on_runtime_upgrade();
            <ValidatorSecurity as Hooks<u64>>::post_upgrade(repeated)
                .expect("repeated post-upgrade");
            assert_eq!(ObservationCount::<Test>::get(), 1);
        });
    }

    #[cfg(feature = "try-runtime")]
    #[test]
    fn try_runtime_rejects_every_ambiguous_or_corrupt_migration_state() {
        ext().execute_with(|| {
            StorageVersion::new(0).put::<ValidatorSecurity>();
            assert!(<ValidatorSecurity as Hooks<u64>>::pre_upgrade().is_err());
        });
        ext().execute_with(|| {
            sp_io::storage::set(&dirty_namespace_key(), b"dirty");
            assert!(<ValidatorSecurity as Hooks<u64>>::pre_upgrade().is_err());
        });
        ext().execute_with(|| {
            StorageVersion::new(2).put::<ValidatorSecurity>();
            assert!(<ValidatorSecurity as Hooks<u64>>::pre_upgrade().is_err());
        });
        ext().execute_with(|| {
            sp_io::storage::set(&version_key(), &[1]);
            assert!(<ValidatorSecurity as Hooks<u64>>::pre_upgrade().is_err());
        });
        ext().execute_with(|| {
            initialize_v1(1);
            assert_ok!(ValidatorSecurity::record_rehearsal_batch(batch(
                alloc::vec![input(1, 1),]
            )));
            ObservationRing::<Test>::remove(0);
            assert!(<ValidatorSecurity as Hooks<u64>>::pre_upgrade().is_err());
        });
    }

    #[cfg(feature = "try-runtime")]
    #[test]
    fn try_runtime_post_upgrade_detects_initial_and_repeated_state_tampering() {
        ext().execute_with(|| {
            let initial =
                <ValidatorSecurity as Hooks<u64>>::pre_upgrade().expect("clean absent prestate");
            sp_io::storage::set(&dirty_namespace_key(), b"dirty");
            <ValidatorSecurity as Hooks<u64>>::on_runtime_upgrade();
            assert!(<ValidatorSecurity as Hooks<u64>>::post_upgrade(initial).is_err());
        });
        ext().execute_with(|| {
            initialize_v1(1);
            assert_ok!(ValidatorSecurity::record_rehearsal_batch(batch(
                alloc::vec![input(1, 1),]
            )));
            let repeated =
                <ValidatorSecurity as Hooks<u64>>::pre_upgrade().expect("valid v1 prestate");
            let observation_key = Observations::<Test>::iter_keys()
                .next()
                .expect("observation");
            Observations::<Test>::mutate(observation_key, |entry| {
                entry.as_mut().expect("record").proof_hash = [9; 32];
            });
            <ValidatorSecurity as Hooks<u64>>::on_runtime_upgrade();
            assert!(<ValidatorSecurity as Hooks<u64>>::post_upgrade(repeated).is_err());
        });
    }
}
