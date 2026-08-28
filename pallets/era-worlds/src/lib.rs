//! Bounded, owner-controlled world provenance registry.
//!
//! Only identifiers and fixed-size content/provenance commitments are stored on-chain. Media,
//! mutable URLs, personal data, biometrics, voice, private messages, and real-time presence stay
//! off-chain. This pallet has no native issuance path and no Root dependency in pallet logic.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub use pallet::*;
pub mod weights;

#[frame_support::pallet]
pub mod pallet {
    use super::weights::WeightInfo;
    use codec::{Decode, Encode, MaxEncodedLen};
    use frame_support::{
        pallet_prelude::*,
        traits::{Currency, EnsureOrigin, ReservableCurrency, StorageVersion},
        transactional,
    };
    use frame_system::pallet_prelude::*;
    use scale_info::TypeInfo;
    use sp_runtime::{traits::Zero, RuntimeDebug};

    pub type BalanceOf<T> =
        <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;
    pub type WorldIdOf<T> = BoundedVec<u8, <T as Config>::MaxWorldIdLength>;

    const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

    #[derive(
        Encode,
        Decode,
        DecodeWithMemTracking,
        Clone,
        PartialEq,
        Eq,
        RuntimeDebug,
        TypeInfo,
        MaxEncodedLen,
    )]
    #[scale_info(skip_type_params(T))]
    pub struct WorldRecord<T: Config> {
        pub owner: T::AccountId,
        pub commitment: [u8; 32],
        pub deposit: BalanceOf<T>,
    }

    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    #[pallet::config]
    pub trait Config: frame_system::Config<RuntimeEvent: From<Event<Self>>> {
        type Currency: ReservableCurrency<Self::AccountId>;
        /// Scoped origin for registry-only removal. It cannot transfer ownership.
        type RegistryAdminOrigin: EnsureOrigin<Self::RuntimeOrigin>;
        /// Scoped origin for pausing only this pallet's registration and update calls.
        type EmergencyPauseOrigin: EnsureOrigin<Self::RuntimeOrigin>;
        #[pallet::constant]
        type RegistrationDeposit: Get<BalanceOf<Self>>;
        #[pallet::constant]
        type MaxWorldIdLength: Get<u32>;
        type WeightInfo: WeightInfo;
    }

    #[pallet::storage]
    #[pallet::getter(fn worlds)]
    pub type Worlds<T: Config> =
        StorageMap<_, Blake2_128Concat, WorldIdOf<T>, WorldRecord<T>, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn paused)]
    pub type Paused<T: Config> = StorageValue<_, bool, ValueQuery>;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        WorldRegistered {
            world_id: WorldIdOf<T>,
            owner: T::AccountId,
            commitment: [u8; 32],
        },
        CommitmentUpdated {
            world_id: WorldIdOf<T>,
            owner: T::AccountId,
            commitment: [u8; 32],
        },
        WorldRemoved {
            world_id: WorldIdOf<T>,
            owner: T::AccountId,
            by_admin: bool,
        },
        Paused,
        Unpaused,
    }

    #[pallet::error]
    pub enum Error<T> {
        RegistryPaused,
        AlreadyRegistered,
        UnknownWorld,
        NotOwner,
        DepositInvariant,
        AlreadyPaused,
        NotPaused,
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        #[cfg(feature = "try-runtime")]
        fn pre_upgrade() -> Result<alloc::vec::Vec<u8>, sp_runtime::TryRuntimeError> {
            if Worlds::<T>::iter_keys().next().is_some() || Paused::<T>::get() {
                return Err("new ERA Worlds storage must be empty and unpaused".into());
            }
            Ok(alloc::vec::Vec::new())
        }

        #[cfg(feature = "try-runtime")]
        fn post_upgrade(_state: alloc::vec::Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
            if Worlds::<T>::iter_keys().next().is_some() || Paused::<T>::get() {
                return Err("ERA Worlds storage changed during empty-storage upgrade check".into());
            }
            Ok(())
        }
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::call_index(0)]
        #[pallet::weight(T::WeightInfo::register_world())]
        pub fn register_world(
            origin: OriginFor<T>,
            world_id: WorldIdOf<T>,
            commitment: [u8; 32],
        ) -> DispatchResult {
            let owner = ensure_signed(origin)?;
            ensure!(!Paused::<T>::get(), Error::<T>::RegistryPaused);
            ensure!(
                !Worlds::<T>::contains_key(&world_id),
                Error::<T>::AlreadyRegistered
            );
            let deposit = T::RegistrationDeposit::get();
            T::Currency::reserve(&owner, deposit)?;
            Worlds::<T>::insert(
                &world_id,
                WorldRecord::<T> {
                    owner: owner.clone(),
                    commitment,
                    deposit,
                },
            );
            Self::deposit_event(Event::WorldRegistered {
                world_id,
                owner,
                commitment,
            });
            Ok(())
        }

        #[pallet::call_index(1)]
        #[pallet::weight(T::WeightInfo::update_commitment())]
        pub fn update_commitment(
            origin: OriginFor<T>,
            world_id: WorldIdOf<T>,
            commitment: [u8; 32],
        ) -> DispatchResult {
            let owner = ensure_signed(origin)?;
            ensure!(!Paused::<T>::get(), Error::<T>::RegistryPaused);
            Worlds::<T>::try_mutate(&world_id, |maybe_world| -> DispatchResult {
                let world = maybe_world.as_mut().ok_or(Error::<T>::UnknownWorld)?;
                ensure!(world.owner == owner, Error::<T>::NotOwner);
                world.commitment = commitment;
                Ok(())
            })?;
            Self::deposit_event(Event::CommitmentUpdated {
                world_id,
                owner,
                commitment,
            });
            Ok(())
        }

        #[pallet::call_index(2)]
        #[pallet::weight(T::WeightInfo::deregister_world())]
        #[transactional]
        pub fn deregister_world(origin: OriginFor<T>, world_id: WorldIdOf<T>) -> DispatchResult {
            let owner = ensure_signed(origin)?;
            let world = Worlds::<T>::get(&world_id).ok_or(Error::<T>::UnknownWorld)?;
            ensure!(world.owner == owner, Error::<T>::NotOwner);
            Self::remove_and_refund(&world_id, &world)?;
            Self::deposit_event(Event::WorldRemoved {
                world_id,
                owner,
                by_admin: false,
            });
            Ok(())
        }

        #[pallet::call_index(3)]
        #[pallet::weight(T::WeightInfo::admin_remove_world())]
        #[transactional]
        pub fn admin_remove_world(origin: OriginFor<T>, world_id: WorldIdOf<T>) -> DispatchResult {
            T::RegistryAdminOrigin::ensure_origin(origin)?;
            let world = Worlds::<T>::get(&world_id).ok_or(Error::<T>::UnknownWorld)?;
            Self::remove_and_refund(&world_id, &world)?;
            Self::deposit_event(Event::WorldRemoved {
                world_id,
                owner: world.owner,
                by_admin: true,
            });
            Ok(())
        }

        #[pallet::call_index(4)]
        #[pallet::weight(T::WeightInfo::pause())]
        pub fn pause(origin: OriginFor<T>) -> DispatchResult {
            T::EmergencyPauseOrigin::ensure_origin(origin)?;
            ensure!(!Paused::<T>::get(), Error::<T>::AlreadyPaused);
            Paused::<T>::put(true);
            Self::deposit_event(Event::Paused);
            Ok(())
        }

        #[pallet::call_index(5)]
        #[pallet::weight(T::WeightInfo::unpause())]
        pub fn unpause(origin: OriginFor<T>) -> DispatchResult {
            T::EmergencyPauseOrigin::ensure_origin(origin)?;
            ensure!(Paused::<T>::get(), Error::<T>::NotPaused);
            Paused::<T>::put(false);
            Self::deposit_event(Event::Unpaused);
            Ok(())
        }
    }

    impl<T: Config> Pallet<T> {
        fn remove_and_refund(world_id: &WorldIdOf<T>, world: &WorldRecord<T>) -> DispatchResult {
            Worlds::<T>::remove(world_id);
            let remainder = T::Currency::unreserve(&world.owner, world.deposit);
            ensure!(remainder.is_zero(), Error::<T>::DepositInvariant);
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests;
