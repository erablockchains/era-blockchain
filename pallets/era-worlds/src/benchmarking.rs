//! Benchmarks for the bounded ERA World registry.

use super::*;
use crate::Pallet as EraWorlds;
use alloc::vec;
use frame_benchmarking::v2::*;
use frame_support::traits::{Currency, Get, ReservableCurrency};
use frame_system::RawOrigin;
use sp_runtime::{
    traits::{Saturating, Zero},
    DispatchError,
};

fn maximum_world_id<T: Config>() -> WorldIdOf<T> {
    vec![b'w'; T::MaxWorldIdLength::get() as usize]
        .try_into()
        .expect("MaxWorldIdLength defines a valid bounded identifier")
}

fn benchmark_owner<T: Config>() -> T::AccountId {
    // Deliberately not `whitelisted_caller`: reserve/unreserve mutate the owner's native
    // System::Account, and that database work is part of the dispatch worst case.
    account("world-owner", 0, 0)
}

fn fund_registration<T: Config>(owner: &T::AccountId) {
    let balance = T::RegistrationDeposit::get().saturating_add(T::Currency::minimum_balance());
    let _ = T::Currency::make_free_balance_be(owner, balance);
}

fn register_maximum_world<T: Config>(owner: &T::AccountId, world_id: &WorldIdOf<T>) {
    fund_registration::<T>(owner);
    EraWorlds::<T>::register_world(
        RawOrigin::Signed(owner.clone()).into(),
        world_id.clone(),
        [1; 32],
    )
    .expect("benchmark setup uses the public registration invariant");
}

#[benchmarks]
mod benchmarks {
    use super::*;

    #[benchmark]
    fn register_world() {
        let owner: T::AccountId = benchmark_owner::<T>();
        let world_id = maximum_world_id::<T>();
        fund_registration::<T>(&owner);

        #[extrinsic_call]
        register_world(RawOrigin::Signed(owner.clone()), world_id.clone(), [2; 32]);

        let world = Worlds::<T>::get(&world_id).expect("world was registered");
        assert_eq!(world.owner, owner);
        assert_eq!(world.commitment, [2; 32]);
        assert_eq!(T::Currency::reserved_balance(&owner), world.deposit);
    }

    #[benchmark]
    fn update_commitment() {
        let owner: T::AccountId = benchmark_owner::<T>();
        let world_id = maximum_world_id::<T>();
        register_maximum_world::<T>(&owner, &world_id);

        #[extrinsic_call]
        update_commitment(RawOrigin::Signed(owner.clone()), world_id.clone(), [3; 32]);

        let world = Worlds::<T>::get(&world_id).expect("world remains registered");
        assert_eq!(world.owner, owner);
        assert_eq!(world.commitment, [3; 32]);
    }

    #[benchmark]
    fn deregister_world() {
        let owner: T::AccountId = benchmark_owner::<T>();
        let world_id = maximum_world_id::<T>();
        register_maximum_world::<T>(&owner, &world_id);

        #[extrinsic_call]
        deregister_world(RawOrigin::Signed(owner.clone()), world_id.clone());

        assert!(!Worlds::<T>::contains_key(&world_id));
        assert!(T::Currency::reserved_balance(&owner).is_zero());
    }

    // The production runtime configures RegistryAdminOrigin as EnsureNever. Measure the complete
    // configured dispatch path and require the fail-closed result; an origin-policy change must
    // regenerate this weight before activation.
    #[benchmark]
    fn admin_remove_world() {
        let world_id = maximum_world_id::<T>();

        #[block]
        {
            let result = EraWorlds::<T>::admin_remove_world(RawOrigin::Root.into(), world_id);
            assert_eq!(result, Err(DispatchError::BadOrigin));
        }
    }

    // The production runtime configures EmergencyPauseOrigin as EnsureNever. These benchmarks
    // cover the actual rejected paths without substituting a benchmark-only privileged origin.
    #[benchmark]
    fn pause() {
        #[block]
        {
            let result = EraWorlds::<T>::pause(RawOrigin::Root.into());
            assert_eq!(result, Err(DispatchError::BadOrigin));
        }
    }

    #[benchmark]
    fn unpause() {
        #[block]
        {
            let result = EraWorlds::<T>::unpause(RawOrigin::Root.into());
            assert_eq!(result, Err(DispatchError::BadOrigin));
        }
    }

    impl_benchmark_test_suite!(EraWorlds, crate::tests::new_test_ext(), crate::tests::Test);
}
