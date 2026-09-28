//! WS3 synthetic migration/payment integration; no real identities or keys.
use super::*;
use crate::ws3_vesting;
use era_v14_custody_governance::{exact_vesting_schedules, FOUNDING_ALLOCATION_TARGETS};
use frame_support::traits::{LockableCurrency, WithdrawReasons};
use sp_runtime::traits::ConvertInto;

fn install(who: &AccountId, amount: Balance) {
    assert_ok!(<Balances as Currency<AccountId>>::transfer(
        &account(250),
        who,
        amount,
        ExistenceRequirement::KeepAlive
    ));
    let pair = exact_vesting_schedules(amount, 1_000).unwrap();
    for schedule in [pair.schedule_a, pair.schedule_b] {
        assert_ok!(
            <Vesting as VestingSchedule<AccountId>>::add_vesting_schedule(
                who,
                schedule.locked,
                schedule.per_block,
                schedule.starting_block
            )
        );
    }
}

fn ws3_ext() -> sp_io::TestExternalities {
    let (mut ext, _) = v14_migration_test_ext(10_000 * DECIMALS);
    ext.execute_with(|| {
        let other = Balances::total_issuance() - Balances::free_balance(account(250));
        assert_ok!(Balances::force_set_balance(
            RuntimeOrigin::root(),
            MultiAddress::Id(account(250)),
            100_000_000 * DECIMALS - other
        ));
        assert_ok!(<Balances as Currency<AccountId>>::transfer(
            &account(250),
            &account(240),
            10_000_000 * DECIMALS,
            ExistenceRequirement::KeepAlive
        ));
    });
    ext
}

#[test]
fn ws3_installation_restores_missing_and_permissive_locks_before_withdrawals_and_replays() {
    for already_migrated in [false, true] {
        for block in [100, 1_000, 1_001, 2_629_000, 5_257_000, 5_257_001] {
            let mut ext = ws3_ext();
            ext.execute_with(|| {
                if already_migrated {
                    <v14_migration::V14Migration as OnRuntimeUpgrade>::on_runtime_upgrade();
                }
                let mut snapshots = Vec::new();
                for (index, amount) in FOUNDING_ALLOCATION_TARGETS.into_iter().enumerate() {
                    let who = account(201 + index as u8);
                    install(&who, amount);
                    if index % 2 == 0 {
                        Balances::remove_lock(*b"vesting ", &who);
                    } else {
                        Balances::set_lock(*b"vesting ", &who, 1, WithdrawReasons::TRANSFER);
                    }
                    Balances::set_lock(*b"ws3other", &who, 7, WithdrawReasons::all());
                    snapshots.push((
                        who.clone(),
                        pallet_vesting::Vesting::<Runtime>::get(&who).unwrap(),
                        Balances::free_balance(&who),
                    ));
                }
                System::set_block_number(block);
                let supply = Balances::total_issuance();
                let allowance = IssuanceCap::remaining_allowance();
                let reward = RewardReserve::pot_balance();
                let community = Balances::free_balance(account(240));
                #[cfg(feature = "try-runtime")]
                let before =
                    <v14_migration::V14Migration as OnRuntimeUpgrade>::pre_upgrade().unwrap();
                <v14_migration::V14Migration as OnRuntimeUpgrade>::on_runtime_upgrade();
                #[cfg(feature = "try-runtime")]
                assert_ok!(<v14_migration::V14Migration as OnRuntimeUpgrade>::post_upgrade(before));
                for (who, schedules, balance) in &snapshots {
                    assert_eq!(
                        pallet_vesting::Vesting::<Runtime>::get(who).as_ref(),
                        Some(schedules)
                    );
                    assert_eq!(Balances::free_balance(who), *balance);
                    let locked = schedules
                        .iter()
                        .map(|s| s.locked_at::<ConvertInto>(block))
                        .sum::<Balance>();
                    let locks = Balances::locks(who);
                    assert!(locks
                        .iter()
                        .any(|lock| lock.id == *b"ws3other" && lock.amount == 7));
                    if locked > 0 {
                        assert!(locks.iter().any(|lock| lock.id == *b"vesting "
                            && lock.amount == locked
                            && lock.reasons == pallet_balances::Reasons::All));
                        let unlocked = balance - locked;
                        assert_eq!(
                            Balances::transfer_allow_death(
                                RuntimeOrigin::signed(who.clone()),
                                MultiAddress::Id(account(250)),
                                unlocked + 1
                            ),
                            Err(sp_runtime::TokenError::Frozen.into())
                        );
                        assert!(<Balances as ReservableCurrency<AccountId>>::reserve(
                            who,
                            unlocked + 1
                        )
                        .is_err());
                    } else {
                        assert!(locks.iter().all(|lock| lock.id != *b"vesting "));
                    }
                }
                assert_eq!(Balances::total_issuance(), supply);
                assert_eq!(supply, 100_000_000 * DECIMALS);
                assert_eq!(IssuanceCap::remaining_allowance(), allowance);
                assert_eq!(RewardReserve::pot_balance(), reward);
                assert_eq!(Balances::free_balance(account(240)), community);
                assert_eq!(community, 10_000_000 * DECIMALS);
                assert_eq!(Session::validators().len(), 4);
                assert_eq!(Staking::validator_count(), 4);
                assert_eq!(
                    SecurityBudget::legacy_reward_liability(),
                    47_913_372_622_298_883_618_387
                );
                assert_eq!(
                    SecurityBudget::remaining_reward_budget().unwrap(),
                    19_952_086_627_377_701_116_381_613
                );
                let root = sp_io::storage::root(StateVersion::V1);
                <v14_migration::V14Migration as OnRuntimeUpgrade>::on_runtime_upgrade();
                assert_eq!(sp_io::storage::root(StateVersion::V1), root);
            });
        }
    }
}

#[test]
fn ws3_payment_extension_rejects_locked_fees_and_tips_and_spends_only_unlocked_funds() {
    use frame_support::dispatch::{GetDispatchInfo, PostDispatchInfo};
    use sp_runtime::traits::TransactionExtension;
    use sp_runtime::transaction_validity::{InvalidTransaction, TransactionSource};
    type Charge = pallet_transaction_payment::ChargeTransactionPayment<Runtime>;
    for tip in [0, DECIMALS] {
        ws3_ext().execute_with(|| {
            let who = account(201);
            install(&who, FOUNDING_ALLOCATION_TARGETS[0]);
            <v14_migration::V14Migration as OnRuntimeUpgrade>::on_runtime_upgrade();
            assert_ok!(SecurityBudget::activate(RuntimeOrigin::root()));
            let call = RuntimeCall::System(frame_system::Call::remark { remark: b"ws3 fee proof".to_vec() });
            let info = call.get_dispatch_info();
            let len = call.encoded_size();
            let origin = RuntimeOrigin::signed(who.clone());
            let validate = || Charge::from(tip).validate(origin.clone(), &call, &info, len, (), &sp_runtime::traits::TxBaseImplication(()), TransactionSource::External);
            assert!(matches!(validate(), Err(error) if error == InvalidTransaction::Payment.into()));
            let fee = TransactionPayment::compute_fee(len as u32, &info, tip);
            assert!(fee > tip);
            assert_ok!(<Balances as Currency<AccountId>>::transfer(&account(250), &who, fee - 1, ExistenceRequirement::KeepAlive));
            assert!(validate().is_err());
            assert_ok!(<Balances as Currency<AccountId>>::transfer(&account(250), &who, 1, ExistenceRequirement::KeepAlive));
            let supply = Balances::total_issuance();
            let allowance = IssuanceCap::remaining_allowance();
            let (_, val, validated_origin) = validate().expect("exact unlocked fee is usable");
            let pre = Charge::from(tip).prepare(val, &validated_origin, &call, &info, len).unwrap();
            assert_eq!(Balances::free_balance(&who), FOUNDING_ALLOCATION_TARGETS[0]);
            assert_ok!(Charge::post_dispatch_details(pre, &info, &PostDispatchInfo::default(), len, &Ok(())));
            assert_eq!(Balances::free_balance(&who), FOUNDING_ALLOCATION_TARGETS[0]);
            assert_eq!(Balances::total_issuance(), supply);
            assert_eq!(IssuanceCap::remaining_allowance(), allowance);
            assert_eq!(Balances::free_balance(account(240)), 10_000_000 * DECIMALS);
            assert!(System::events().iter().any(|record| matches!(record.event,
                RuntimeEvent::TransactionPayment(pallet_transaction_payment::Event::TransactionFeePaid { actual_fee, tip: paid_tip, .. }) if actual_fee == fee && paid_tip == tip)));
        });
    }
}

#[test]
fn ws3_bounded_restoration_rejects_overflow_corruption_and_late_failures_atomically() {
    for scenario in 0..6 {
        ws3_ext().execute_with(|| {
            let who = account(201);
            install(&who, FOUNDING_ALLOCATION_TARGETS[0]);
            Balances::remove_lock(*b"vesting ", &who);
            let other = account(202);
            match scenario {
                0 => {
                    let schedule = pallet_vesting::Vesting::<Runtime>::get(&who).unwrap();
                    for index in 0..ws3_vesting::MAX_RESTORED_ACCOUNTS {
                        let mut id = [0x99; 32];
                        id[..4].copy_from_slice(&index.to_le_bytes());
                        pallet_vesting::Vesting::<Runtime>::insert(
                            AccountId::new(id),
                            schedule.clone(),
                        );
                    }
                }
                1 => sp_io::storage::set(
                    &pallet_vesting::Vesting::<Runtime>::hashed_key_for(&other),
                    &[255],
                ),
                2 => {
                    let invalid: frame_support::BoundedVec<
                        _,
                        pallet_vesting::MaxVestingSchedulesGet<Runtime>,
                    > = vec![
                        pallet_vesting::VestingInfo::new(u128::MAX, 1, 1_000),
                        pallet_vesting::VestingInfo::new(1, 1, 1_000),
                    ]
                    .try_into()
                    .unwrap();
                    pallet_vesting::Vesting::<Runtime>::insert(&other, invalid);
                }
                3 => {
                    install(&other, FOUNDING_ALLOCATION_TARGETS[1]);
                    Balances::remove_lock(*b"vesting ", &other);
                    for index in 0u64..50 {
                        Balances::set_lock(index.to_le_bytes(), &other, 1, WithdrawReasons::all());
                    }
                }
                4 => {
                    let oversized = vec![
                        pallet_balances::BalanceLock {
                            id: *b"ws3locks",
                            amount: 1u128,
                            reasons: pallet_balances::Reasons::All
                        };
                        51
                    ];
                    sp_io::storage::set(
                        &pallet_balances::Locks::<Runtime>::hashed_key_for(&who),
                        &oversized.encode(),
                    );
                }
                _ => {
                    StorageVersion::new(9).put::<SecurityBudget>();
                }
            }
            let root = sp_io::storage::root(StateVersion::V1);
            assert_v14_migration_panics();
            assert_eq!(sp_io::storage::root(StateVersion::V1), root);
            assert!(Balances::locks(&who)
                .iter()
                .all(|lock| lock.id != *b"vesting "));
        });
    }
}

#[test]
fn ws3_maximum_bounded_state_restores_and_repeated_refresh_is_read_only() {
    new_test_ext(4, 0, 4, 4).0.execute_with(|| {
        System::set_block_number(100);
        let schedules: frame_support::BoundedVec<
            _,
            pallet_vesting::MaxVestingSchedulesGet<Runtime>,
        > = vec![pallet_vesting::VestingInfo::new(DECIMALS, DECIMALS / 100, 1_000); 128]
            .try_into()
            .unwrap();
        for index in 0..ws3_vesting::MAX_RESTORED_ACCOUNTS {
            let mut id = [0x98; 32];
            id[..4].copy_from_slice(&index.to_le_bytes());
            let who = AccountId::new(id);
            assert_ok!(Balances::force_set_balance(
                RuntimeOrigin::root(),
                MultiAddress::Id(who.clone()),
                200 * DECIMALS
            ));
            pallet_vesting::Vesting::<Runtime>::insert(&who, schedules.clone());
            for lock in 0u64..49 {
                Balances::set_lock(lock.to_le_bytes(), &who, 1, WithdrawReasons::all());
            }
        }
        assert_ok!(ws3_vesting::restore_locks());
        let root = sp_io::storage::root(StateVersion::V1);
        assert_ok!(ws3_vesting::restore_locks());
        assert_eq!(sp_io::storage::root(StateVersion::V1), root);
        assert!(ws3_vesting::declared_weight().ref_time() > 0);
    });
}

#[test]
fn ws3_restoration_preserves_other_holds_locks_and_repairs_inconsistent_frozen_cache() {
    use frame_support::traits::fungible::{InspectHold, MutateHold};
    ws3_ext().execute_with(|| {
        let who = account(201);
        install(&who, FOUNDING_ALLOCATION_TARGETS[0]);
        // Allocate an independently unlocked amount for the existing hold.
        assert_ok!(<Balances as Currency<AccountId>>::transfer(
            &account(250),
            &who,
            DECIMALS,
            ExistenceRequirement::KeepAlive
        ));
        let reason = RuntimeHoldReason::Staking(pallet_staking::HoldReason::Staking);
        assert_ok!(<Balances as MutateHold<AccountId>>::hold(
            &reason, &who, DECIMALS
        ));
        Balances::set_lock(*b"ws3other", &who, 11, WithdrawReasons::all());
        // A persisted lock with a stale account cache must not be accepted as protection.
        frame_system::Account::<Runtime>::mutate(&who, |info| info.data.frozen = 0);
        let schedules = pallet_vesting::Vesting::<Runtime>::get(&who);
        let holds = pallet_balances::Holds::<Runtime>::get(&who);
        let free = Balances::free_balance(&who);
        let reserved = Balances::reserved_balance(&who);
        assert_ok!(ws3_vesting::restore_locks());
        assert_eq!(
            System::account(&who).data.frozen,
            FOUNDING_ALLOCATION_TARGETS[0]
        );
        assert_eq!(
            <Balances as InspectHold<AccountId>>::balance_on_hold(&reason, &who),
            DECIMALS
        );
        assert_eq!(pallet_balances::Holds::<Runtime>::get(&who), holds);
        assert_eq!(Balances::free_balance(&who), free);
        assert_eq!(Balances::reserved_balance(&who), reserved);
        assert_eq!(pallet_vesting::Vesting::<Runtime>::get(&who), schedules);
        assert!(Balances::locks(&who)
            .iter()
            .any(|l| l.id == *b"ws3other" && l.amount == 11));
        // This runtime has no user freeze slots; WS3 does not add a freeze authority or slot.
        assert_eq!(
            <<Runtime as pallet_balances::Config>::MaxFreezes as Get<u32>>::get(),
            0
        );
        assert!(pallet_balances::Freezes::<Runtime>::get(&who).is_empty());
    });
}

#[test]
fn ws3_installation_bound_and_bad_key_fail_closed_without_scanning_past_the_bound() {
    use frame_support::traits::Get;
    let migration_weight = v14_migration::declared_weight();
    let block_weights: frame_system::limits::BlockWeights =
        <Runtime as frame_system::Config>::BlockWeights::get();
    let maximum = block_weights.max_block;
    assert!(
        migration_weight.all_lte(maximum),
        "declared migration weight must fit one block"
    );
    for suffix in [vec![1], vec![0; 48]] {
        ws3_ext().execute_with(|| {
            install(&account(201), FOUNDING_ALLOCATION_TARGETS[0]);
            Balances::remove_lock(*b"vesting ", &account(201));
            let mut key = frame_support::storage::storage_prefix(b"Vesting", b"Vesting").to_vec();
            key.extend(suffix);
            sp_io::storage::set(&key, &[0]);
            let root = sp_io::storage::root(StateVersion::V1);
            assert!(ws3_vesting::restore_locks().is_err());
            assert_eq!(sp_io::storage::root(StateVersion::V1), root);
        });
    }
}
