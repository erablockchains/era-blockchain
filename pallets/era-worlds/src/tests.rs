use crate as pallet_era_worlds;
use crate::{Error, WorldIdOf};
use frame_support::{
    assert_noop, assert_ok, derive_impl, parameter_types,
    traits::{ConstU32, GetStorageVersion, SortedMembers},
};
use sp_runtime::{traits::IdentityLookup, BuildStorage};

type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
    pub enum Test {
        System: frame_system,
        Balances: pallet_balances,
        EraWorlds: pallet_era_worlds,
    }
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
    type AccountId = u64;
    type Lookup = IdentityLookup<Self::AccountId>;
    type Block = Block;
    type AccountData = pallet_balances::AccountData<u64>;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
    type AccountStore = System;
}

pub struct RegistryAdmins;
impl SortedMembers<u64> for RegistryAdmins {
    fn sorted_members() -> Vec<u64> {
        vec![98]
    }
}

pub struct EmergencyOperators;
impl SortedMembers<u64> for EmergencyOperators {
    fn sorted_members() -> Vec<u64> {
        vec![99]
    }
}

parameter_types! {
    pub const RegistrationDeposit: u64 = 100;
}

impl crate::Config for Test {
    type Currency = Balances;
    type RegistryAdminOrigin = frame_system::EnsureSignedBy<RegistryAdmins, u64>;
    type EmergencyPauseOrigin = frame_system::EnsureSignedBy<EmergencyOperators, u64>;
    type RegistrationDeposit = RegistrationDeposit;
    type MaxWorldIdLength = ConstU32<16>;
    type WeightInfo = ();
}

fn new_test_ext() -> sp_io::TestExternalities {
    let mut storage = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .expect("system genesis builds");
    pallet_balances::GenesisConfig::<Test> {
        balances: vec![(1, 1_000), (2, 1_000), (98, 1_000), (99, 1_000)],
        dev_accounts: None,
    }
    .assimilate_storage(&mut storage)
    .expect("balances genesis builds");
    storage.into()
}

fn world_id(value: &[u8]) -> WorldIdOf<Test> {
    value.to_vec().try_into().expect("bounded test world id")
}

#[test]
fn owner_registers_updates_and_recovers_deposit() {
    new_test_ext().execute_with(|| {
        let id = world_id(b"world-one");
        let issuance_before = Balances::total_issuance();
        assert_ok!(EraWorlds::register_world(
            RuntimeOrigin::signed(1),
            id.clone(),
            [1; 32],
        ));
        assert_eq!(Balances::reserved_balance(1), 100);
        assert_eq!(Balances::total_issuance(), issuance_before);
        assert_eq!(EraWorlds::worlds(&id).expect("world exists").owner, 1);

        assert_ok!(EraWorlds::update_commitment(
            RuntimeOrigin::signed(1),
            id.clone(),
            [2; 32],
        ));
        assert_eq!(
            EraWorlds::worlds(&id).expect("world exists").commitment,
            [2; 32]
        );

        assert_ok!(EraWorlds::deregister_world(
            RuntimeOrigin::signed(1),
            id.clone()
        ));
        assert!(EraWorlds::worlds(&id).is_none());
        assert_eq!(Balances::reserved_balance(1), 0);
        assert_eq!(Balances::total_issuance(), issuance_before);
    });
}

#[test]
fn duplicate_unauthorized_and_insufficient_deposit_fail_atomically() {
    new_test_ext().execute_with(|| {
        let id = world_id(b"unique");
        assert_ok!(EraWorlds::register_world(
            RuntimeOrigin::signed(1),
            id.clone(),
            [3; 32],
        ));
        let reserved = Balances::reserved_balance(1);
        assert_noop!(
            EraWorlds::register_world(RuntimeOrigin::signed(1), id.clone(), [4; 32]),
            Error::<Test>::AlreadyRegistered
        );
        assert_eq!(Balances::reserved_balance(1), reserved);
        assert_noop!(
            EraWorlds::update_commitment(RuntimeOrigin::signed(2), id.clone(), [5; 32]),
            Error::<Test>::NotOwner
        );
        assert_eq!(
            EraWorlds::worlds(&id).expect("world exists").commitment,
            [3; 32]
        );

        let poor = world_id(b"poor");
        assert_noop!(
            EraWorlds::register_world(RuntimeOrigin::signed(7), poor.clone(), [6; 32]),
            pallet_balances::Error::<Test>::InsufficientBalance
        );
        assert!(EraWorlds::worlds(&poor).is_none());
    });
}

#[test]
fn pause_is_application_only_and_scoped() {
    new_test_ext().execute_with(|| {
        let id = world_id(b"paused");
        assert_noop!(
            EraWorlds::pause(RuntimeOrigin::signed(1)),
            sp_runtime::DispatchError::BadOrigin
        );
        assert_ok!(EraWorlds::pause(RuntimeOrigin::signed(99)));
        assert_noop!(
            EraWorlds::register_world(RuntimeOrigin::signed(1), id.clone(), [7; 32]),
            Error::<Test>::RegistryPaused
        );
        assert_eq!(Balances::free_balance(1), 1_000);
        assert_noop!(
            EraWorlds::unpause(RuntimeOrigin::signed(98)),
            sp_runtime::DispatchError::BadOrigin
        );
        assert_ok!(EraWorlds::unpause(RuntimeOrigin::signed(99)));
        assert_ok!(EraWorlds::register_world(
            RuntimeOrigin::signed(1),
            id,
            [7; 32]
        ));
    });
}

#[test]
fn registry_admin_can_remove_but_cannot_take_ownership() {
    new_test_ext().execute_with(|| {
        let id = world_id(b"admin-remove");
        assert_ok!(EraWorlds::register_world(
            RuntimeOrigin::signed(1),
            id.clone(),
            [8; 32]
        ));
        assert_noop!(
            EraWorlds::admin_remove_world(RuntimeOrigin::signed(2), id.clone()),
            sp_runtime::DispatchError::BadOrigin
        );
        assert_eq!(EraWorlds::worlds(&id).expect("world exists").owner, 1);
        assert_ok!(EraWorlds::admin_remove_world(
            RuntimeOrigin::signed(98),
            id.clone()
        ));
        assert!(EraWorlds::worlds(&id).is_none());
        assert_eq!(Balances::reserved_balance(1), 0);
    });
}

#[test]
fn identifiers_are_bounded_and_storage_starts_empty() {
    new_test_ext().execute_with(|| {
        assert!(WorldIdOf::<Test>::try_from(vec![0; 17]).is_err());
        assert!(!EraWorlds::paused());
        assert_eq!(crate::Worlds::<Test>::iter_keys().count(), 0);
        assert_eq!(
            <crate::Pallet<Test> as GetStorageVersion>::in_code_storage_version(),
            frame_support::traits::StorageVersion::new(1)
        );
    });
}
