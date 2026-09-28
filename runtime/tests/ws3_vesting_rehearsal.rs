//! Synthetic-only WS3 gate: an unreleased founding allocation must remain unspendable.
//! No keys, real identities, RPC, or runtime migration are used.

use era_runtime::{AccountId, Balances, Runtime, RuntimeOrigin, System, Vesting, DECIMALS};
use era_v14_custody_governance::{exact_vesting_schedules, FOUNDING_ALLOCATION_TARGETS};
use frame_support::{
    assert_ok,
    traits::{ReservableCurrency, VestingSchedule},
};
use sp_runtime::{BuildStorage, MultiAddress, TokenError};

#[test]
fn ws3_unreleased_founding_allocation_cannot_be_transferred() {
    let beneficiary = AccountId::new([201; 32]);
    let recipient = AccountId::new([202; 32]);
    let allocation = FOUNDING_ALLOCATION_TARGETS[0];
    let retained_supply = 100_000_000 * DECIMALS;
    let mut storage = frame_system::GenesisConfig::<Runtime>::default()
        .build_storage()
        .expect("synthetic system genesis");
    pallet_balances::GenesisConfig::<Runtime> {
        balances: vec![
            (beneficiary.clone(), allocation),
            (recipient.clone(), retained_supply - allocation),
        ],
        dev_accounts: None,
    }
    .assimilate_storage(&mut storage)
    .expect("synthetic allocation conservation");

    sp_io::TestExternalities::new(storage).execute_with(|| {
        System::set_block_number(100);
        let pair = exact_vesting_schedules(allocation, 1_000).expect("approved exact C2 terms");
        for schedule in [pair.schedule_a, pair.schedule_b] {
            assert_ok!(
                <Vesting as VestingSchedule<AccountId>>::add_vesting_schedule(
                    &beneficiary,
                    schedule.locked,
                    schedule.per_block,
                    schedule.starting_block,
                )
            );
        }
        assert_eq!(
            <Vesting as VestingSchedule<AccountId>>::vesting_balance(&beneficiary),
            Some(allocation)
        );
        assert_eq!(Balances::total_issuance(), retained_supply);

        let result = Balances::transfer_allow_death(
            RuntimeOrigin::signed(beneficiary.clone()),
            MultiAddress::Id(recipient),
            DECIMALS,
        );
        assert!(
            result.is_err(),
            "WS3 vesting boundary violated: before start block 1000, at block 100, \
             an 8M ETKN fully unvested beneficiary transferred 1 ETKN; \
             balance={}, vesting_balance={:?}, locks={:?}, total_issuance={}",
            Balances::free_balance(&beneficiary),
            <Vesting as VestingSchedule<AccountId>>::vesting_balance(&beneficiary),
            Balances::locks(&beneficiary),
            Balances::total_issuance(),
        );
        assert_eq!(result, Err(TokenError::Frozen.into()));
        assert_eq!(Balances::free_balance(&beneficiary), allocation);
        assert_eq!(Balances::total_issuance(), retained_supply);
    });
}

#[test]
fn ws3_all_five_exact_schedules_release_only_matured_principal_via_funded_third_party() {
    use era_runtime::upgrade13_policy;
    use era_v14_custody_governance::{linear_vesting_terms, FOUNDING_VESTING_RELEASE_INTERVALS};
    assert_eq!(
        FOUNDING_ALLOCATION_TARGETS,
        upgrade13_policy::FOUNDING_ALLOCATION_TARGETS
    );
    assert_eq!(
        FOUNDING_VESTING_RELEASE_INTERVALS,
        upgrade13_policy::FOUNDING_VESTING_RELEASE_INTERVALS
    );
    let payer = AccountId::new([220; 32]);
    let accounts: Vec<_> = (201..206).map(|id| AccountId::new([id; 32])).collect();
    let mut balances: Vec<_> = accounts
        .iter()
        .cloned()
        .zip(FOUNDING_ALLOCATION_TARGETS)
        .collect();
    balances.push((payer.clone(), 80_000_000 * DECIMALS));
    let mut storage = frame_system::GenesisConfig::<Runtime>::default()
        .build_storage()
        .unwrap();
    pallet_balances::GenesisConfig::<Runtime> {
        balances,
        dev_accounts: None,
    }
    .assimilate_storage(&mut storage)
    .unwrap();
    sp_io::TestExternalities::new(storage).execute_with(|| {
        System::set_block_number(100);
        for (who, amount) in accounts.iter().zip(FOUNDING_ALLOCATION_TARGETS) {
            let pair = exact_vesting_schedules(amount, 1_000).unwrap();
            let old = upgrade13_policy::provisional_c2_two_schedule_terms(amount, 1_000).unwrap();
            assert_eq!(pair.schedule_a.locked, old.schedule_a.locked);
            assert_eq!(pair.schedule_b.locked, old.schedule_b.locked);
            assert_eq!(pair.schedule_a.per_block, old.schedule_a.per_block);
            assert_eq!(
                pair.schedule_b.starting_block,
                old.schedule_b.starting_block
            );
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
            assert!(<Balances as ReservableCurrency<AccountId>>::reserve(who, 1).is_err());
        }
        let total = Balances::total_issuance();
        let intervals = FOUNDING_VESTING_RELEASE_INTERVALS;
        for block in [
            999,
            1_000,
            1_001,
            1_000 + intervals / 2,
            1_000 + intervals - 1,
            1_000 + intervals,
        ] {
            System::set_block_number(block);
            for (who, amount) in accounts.iter().zip(FOUNDING_ALLOCATION_TARGETS) {
                let terms = linear_vesting_terms(amount).unwrap();
                let elapsed = block.saturating_sub(1_000);
                let matured = if elapsed >= intervals {
                    amount
                } else {
                    terms
                        .floor_release_per_interval
                        .checked_mul(u128::from(elapsed))
                        .unwrap()
                };
                assert_ok!(Vesting::vest_other(
                    RuntimeOrigin::signed(payer.clone()),
                    MultiAddress::Id(who.clone())
                ));
                let locked = Balances::locks(who)
                    .iter()
                    .find(|lock| lock.id == *b"vesting ")
                    .map_or(0, |lock| lock.amount);
                assert_eq!(locked, amount.checked_sub(matured).unwrap());
                if matured < amount {
                    assert_eq!(
                        Balances::transfer_allow_death(
                            RuntimeOrigin::signed(who.clone()),
                            MultiAddress::Id(payer.clone()),
                            matured + 1
                        ),
                        Err(TokenError::Frozen.into())
                    );
                    assert!(
                        <Balances as ReservableCurrency<AccountId>>::reserve(who, matured + 1)
                            .is_err()
                    );
                    assert_ok!(<Balances as ReservableCurrency<AccountId>>::reserve(
                        who, matured
                    ));
                    assert_eq!(
                        <Balances as ReservableCurrency<AccountId>>::unreserve(who, matured),
                        0
                    );
                    let before = Balances::free_balance(who);
                    assert_ok!(Vesting::vest_other(
                        RuntimeOrigin::signed(payer.clone()),
                        MultiAddress::Id(who.clone())
                    ));
                    assert_eq!(Balances::free_balance(who), before);
                } else {
                    assert!(pallet_vesting::Vesting::<Runtime>::get(who).is_none());
                    assert_ok!(Balances::transfer_allow_death(
                        RuntimeOrigin::signed(who.clone()),
                        MultiAddress::Id(payer.clone()),
                        amount
                    ));
                }
            }
            assert_eq!(Balances::total_issuance(), total);
        }
        System::set_block_number(1_001 + intervals);
        for who in &accounts {
            assert_eq!(
                Vesting::vest_other(
                    RuntimeOrigin::signed(payer.clone()),
                    MultiAddress::Id(who.clone())
                ),
                Err(pallet_vesting::Error::<Runtime>::NotVesting.into())
            );
        }
        assert_eq!(Balances::free_balance(payer), 100_000_000 * DECIMALS);
    });
}
