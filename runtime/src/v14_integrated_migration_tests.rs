//! Actual Executive migration rehearsal over disposable externalities.
//! Compiled V13 public account constants are fixture keys only: no signing or RPC.
use super::*;

fn commitment(label: &str) -> Vec<u8> {
    let root = sp_io::storage::root(StateVersion::V1);
    println!(
        "V14_6_STATE {label} root={root:02x?} events={:?} issuance={} allowance={:?}",
        System::events(),
        Balances::total_issuance(),
        IssuanceCap::remaining_allowance()
    );
    root
}

/// Construct the exact historical allocation using the actual V13 hook, then add
/// synthetic staking records through normal bonding without changing allocations.
fn integrated_ext() -> (sp_io::TestExternalities, Vec<AccountId>) {
    integrated_fixture(true)
}

fn integrated_fixture(completed_v13: bool) -> (sp_io::TestExternalities, Vec<AccountId>) {
    let mut ext = v13_migration_test_ext();
    let validators = v13_migration::test_pre_migration_balances()
        .into_iter()
        .skip(11)
        .filter(|(_, amount)| *amount > 10_000 * DECIMALS)
        .take(4)
        .map(|(who, _)| who)
        .collect::<Vec<_>>();
    assert_eq!(validators.len(), 4);
    ext.execute_with(|| {
        if completed_v13 {
            <v13_migration::V13Migration as OnRuntimeUpgrade>::on_runtime_upgrade();
        } else {
            sp_io::storage::clear(&StorageVersion::storage_key::<IssuanceCap>());
        }
        // This fixture assimilates Balances directly, without construct_runtime OnGenesis.
        // Record the actual installed SDK schema; balances/version migration is not this gate.
        StorageVersion::new(1).put::<Balances>();
        StorageVersion::new(2).put::<RewardReserve>();
        StorageVersion::new(0).put::<SecurityBudget>();
        StorageVersion::new(16).put::<Staking>();
        sp_io::storage::clear(&StorageVersion::storage_key::<ValidatorSecurity>());
        pallet_staking::ValidatorCount::<Runtime>::put(4);
        pallet_staking::MinimumValidatorCount::<Runtime>::put(4);
        pallet_staking::ActiveEra::<Runtime>::put(pallet_staking::ActiveEraInfo {
            index: 10,
            start: Some(1_000_000),
        });
        pallet_staking::CurrentEra::<Runtime>::put(10);
        for (i, who) in validators.iter().enumerate() {
            assert_ok!(Staking::bond(
                RuntimeOrigin::signed(who.clone()),
                10_000 * DECIMALS,
                pallet_staking::RewardDestination::Stash
            ));
            assert_ok!(Staking::validate(
                RuntimeOrigin::signed(who.clone()),
                ValidatorPrefs {
                    commission: Perbill::zero(),
                    blocked: false
                }
            ));
            assert_ok!(Session::set_keys(
                RuntimeOrigin::signed(who.clone()),
                keys(i as u8 + 1),
                vec![]
            ));
        }
        pallet_session::Validators::<Runtime>::put(&validators);
        let queued = validators
            .iter()
            .enumerate()
            .map(|(i, who)| (who.clone(), keys(i as u8 + 1)))
            .collect::<Vec<_>>();
        pallet_session::QueuedKeys::<Runtime>::put(&queued);
        <Babe as frame_support::traits::OneSessionHandler<AccountId>>::on_genesis_session(
            queued.iter().map(|(who, key)| (who, key.babe.clone())),
        );
        <Grandpa as frame_support::traits::OneSessionHandler<AccountId>>::on_genesis_session(
            queued.iter().map(|(who, key)| (who, key.grandpa.clone())),
        );
        install_v14_equal_points(10, &validators);
        let liability = SecurityBudgetExistingEarnedRewardLiability::get();
        pallet_reward_reserve::EraRewardBudgets::<Runtime>::insert(5, liability);
        pallet_reward_reserve::EraRewardLiabilities::<Runtime>::insert(5, liability);
        pallet_reward_reserve::CommittedLiabilities::<Runtime>::put(liability);
        pallet_reward_reserve::RewardSystemActive::<Runtime>::put(true);
        pallet_reward_reserve::FeeRoutingActive::<Runtime>::put(true);
        System::reset_events();
        if completed_v13 {
            assert_eq!(Balances::total_issuance(), 100_000_000 * DECIMALS);
            // Prove this fixture satisfies the *first* registered hook too.
            <v13_migration::V13Migration as OnRuntimeUpgrade>::on_runtime_upgrade();
        }
    });
    (ext, validators)
}

fn namespace(name: &[u8]) -> Vec<(Vec<u8>, Vec<u8>)> {
    let prefix = sp_io::hashing::twox_128(name);
    let mut cursor = prefix.to_vec();
    let mut values = Vec::new();
    if let Some(value) = sp_io::storage::get(&cursor) {
        values.push((cursor.clone(), value.to_vec()));
    }
    while let Some(key) = sp_io::storage::next_key(&cursor) {
        if !key.starts_with(&prefix) {
            break;
        }
        values.push((key.clone(), sp_io::storage::get(&key).unwrap().to_vec()));
        cursor = key;
    }
    values
}

#[test]
fn v14_live_liability_sum_and_capacity_are_exact() {
    const SUPERSEDED_ANALYTICAL_LIABILITY: Balance = 47_913_372_622_298_883_618_388;
    const SUPERSEDED_COMPILED_LIABILITY: Balance = 47_913_372_780_742_576_152_446;
    const COMPILED_TO_LIVE_DIFFERENCE: Balance = 158_443_692_534_059;
    const EXPECTED_REMAINING_CAPACITY: Balance = 19_952_086_627_377_701_116_381_613;
    const LIVE_LIABILITY_FREQUENCIES: [(Balance, u32); 3] = [
        (570_397_293_122_605_757_362, 42),
        (570_397_451_566_298_291_419, 21),
        (570_397_134_678_913_223_304, 21),
    ];

    let (entry_count, live_sum) = LIVE_LIABILITY_FREQUENCIES.iter().fold(
        (0u32, 0u128),
        |(count, sum), (liability, occurrences)| {
            (
                count
                    .checked_add(*occurrences)
                    .expect("bounded entry count"),
                sum.checked_add(
                    liability
                        .checked_mul((*occurrences).into())
                        .expect("bounded liability product"),
                )
                .expect("bounded liability sum"),
            )
        },
    );
    let total_cap = SecurityBudgetTotalStakingRewards::get();

    assert_eq!(entry_count, 84);
    assert_eq!(live_sum, SecurityBudgetExistingEarnedRewardLiability::get());
    assert_eq!(
        SUPERSEDED_ANALYTICAL_LIABILITY.checked_sub(live_sum),
        Some(1)
    );
    assert_eq!(
        SUPERSEDED_COMPILED_LIABILITY.checked_sub(live_sum),
        Some(COMPILED_TO_LIVE_DIFFERENCE)
    );
    assert_eq!(
        total_cap.checked_sub(live_sum),
        Some(EXPECTED_REMAINING_CAPACITY)
    );
    assert_eq!(
        live_sum.checked_add(EXPECTED_REMAINING_CAPACITY),
        Some(20_000_000 * DECIMALS)
    );
}

#[test]
fn v14_integrated_actual_executive_migrates_and_replays_preserving_authorities_and_custody() {
    let (mut ext, validators) = integrated_ext();
    ext.execute_with(|| {
        let preserved = [b"Session".as_slice(), b"Babe", b"Grandpa", b"Historical"].map(namespace);
        let allocations = v13_migration::test_pre_migration_balances()
            .into_iter()
            .map(|(who, _)| (who.clone(), System::account(who)))
            .collect::<Vec<_>>();
        let input = v13_migration::test_input();
        let custody = v13_migration::custody_destinations();
        let destination_accounts = [
            custody[0].clone(),
            custody[1].clone(),
            custody[2].clone(),
            input.community_onboarding_destination.clone(),
        ];
        let custody_before = destination_accounts.each_ref().map(System::account);
        commitment("pre-executive");
        Executive::execute_on_runtime_upgrade();
        commitment("post-executive");
        assert_eq!(Session::validators(), validators);
        assert_eq!(
            preserved,
            [b"Session".as_slice(), b"Babe", b"Grandpa", b"Historical"].map(namespace)
        );
        for (who, before) in allocations {
            assert_eq!(System::account(who), before);
        }
        assert_eq!(
            custody_before,
            destination_accounts.each_ref().map(System::account)
        );
        assert_eq!(
            StorageVersion::get::<ValidatorSecurity>(),
            StorageVersion::new(1)
        );
        assert_eq!(
            SecurityBudget::remaining_reward_budget(),
            Ok(19_952_086_627_377_701_116_381_613)
        );
        assert!(!SecurityBudget::active());
        for prefix in [b"EraV14Amm".as_slice(), b"EraV14AssetAllocator"] {
            assert_eq!(namespace(prefix),vec![(frame_support::storage::storage_prefix(prefix,frame_support::traits::STORAGE_VERSION_STORAGE_KEY_POSTFIX).to_vec(),0u16.encode())]);
        }
        assert_eq!(
            System::events()
                .into_iter()
                .map(|r| r.event)
                .collect::<Vec<_>>(),
            vec![
                RuntimeEvent::System(frame_system::Event::NewAccount {
                    account: SecurityBudget::staking_pot_account()
                }),
                RuntimeEvent::RewardReserve(pallet_reward_reserve::Event::LegacyClaimOnlyEntered {
                    cutoff_era: 10
                }),
                RuntimeEvent::SecurityBudget(pallet_security_budget::Event::MigrationCompleted {
                    at: System::block_number(),
                    staking_pot: SecurityBudget::staking_pot_account(),
                    minimum_validator_bond: 10_000 * DECIMALS,
                    active_validator_count: 4,
                    legacy_reward_liability: 47_913_372_622_298_883_618_387,
                    total_reward_budget: 20_000_000 * DECIMALS,
                }),
            ]
        );
        System::reset_events();
        let before = commitment("pre-replay");
        Executive::execute_on_runtime_upgrade();
        assert_eq!(commitment("post-replay"), before);
        assert!(System::events().is_empty());
    });
}

#[test]
fn v14_integrated_replay_after_payable_reward_must_not_reapply_historical_snapshot() {
    let (mut ext, validators) = integrated_ext();
    ext.execute_with(|| {
        Executive::execute_on_runtime_upgrade();
        assert_ok!(SecurityBudget::activate(RuntimeOrigin::root()));
        let era = SecurityBudget::first_eligible_era().unwrap();
        install_v14_equal_points(era, &validators);
        let year = SecurityBudgetMillisecondsPerYear::get();
        assert_ok!(SecurityBudget::finalize_completed_era(
            era,
            year,
            1_000_000 + year
        ));
        let budget = SecurityBudget::era_budget(era).unwrap();
        assert!(budget.gross_issuance > 0);
        assert_eq!(
            Balances::total_issuance() + IssuanceCap::remaining_allowance().unwrap(),
            1_000_000_000 * DECIMALS
        );
        let issuance = Balances::total_issuance();
        let allowance = IssuanceCap::remaining_allowance();
        pallet_staking::ActiveEra::<Runtime>::put(pallet_staking::ActiveEraInfo {
            index: era + 1,
            start: Some(1_000_000 + year),
        });
        // Advancing ActiveEra in a fixture must also install that era's exposure,
        // as the real session/era transition does. Replay still validates it.
        install_v14_equal_points(era + 1, &validators);
        let beneficiary = validators[0].clone();
        let balance = Balances::free_balance(&beneficiary);
        assert_ok!(SecurityBudget::claim_reward_page(
            RuntimeOrigin::signed(account(230)),
            era,
            beneficiary.clone(),
            0
        ));
        assert!(Balances::free_balance(&beneficiary) > balance);
        assert_noop!(
            SecurityBudget::claim_reward_page(
                RuntimeOrigin::signed(account(230)),
                era,
                beneficiary,
                0
            ),
            pallet_security_budget::Error::<Runtime>::AlreadyClaimed
        );
        assert_eq!(Balances::total_issuance(), issuance);
        assert_eq!(IssuanceCap::remaining_allowance(), allowance);
        assert_eq!(
            SecurityBudget::legacy_reward_liability(),
            47_913_372_622_298_883_618_387
        );
        assert_eq!(
            SecurityBudget::era_paid(era) + SecurityBudget::era_liability(era),
            budget.total
        );
        System::reset_events();
        let before = commitment("payable-reward-before-replay");
        let result = std::panic::catch_unwind(Executive::execute_on_runtime_upgrade);
        assert_eq!(commitment("payable-reward-after-replay"), before);
        assert!(
            result.is_ok(),
            "registered replay rejected legitimate payable rewards"
        );
    });
}

#[test]
fn v14_integrated_vesting_normal_unlock_then_replay() {
    let (mut ext, _) = integrated_ext();
    ext.execute_with(|| {
        Executive::execute_on_runtime_upgrade();
        let (who, schedules) = pallet_vesting::Vesting::<Runtime>::iter().next().unwrap();
        let end = schedules
            .iter()
            .map(|s| s.ending_block_as_balance::<sp_runtime::traits::ConvertInto>())
            .max()
            .unwrap();
        System::set_block_number(end.try_into().unwrap());
        // Direct dispatch does not charge a fee; funded payer is required for a signed extrinsic.
        let payer = v13_migration::test_sudo_account();
        // Every founding schedule matured when the fixture jumped to this block.
        // Refresh all through the normal route so same-block replay is truly read-only.
        for beneficiary in pallet_vesting::Vesting::<Runtime>::iter_keys().collect::<Vec<_>>() {
            if beneficiary != who {
                assert_ok!(Vesting::vest_other(
                    RuntimeOrigin::signed(payer.clone()),
                    MultiAddress::Id(beneficiary)
                ));
            }
        }
        assert_ok!(Vesting::vest_other(
            RuntimeOrigin::signed(payer),
            MultiAddress::Id(who.clone())
        ));
        assert!(Balances::locks(&who)
            .iter()
            .all(|lock| lock.id != *b"vesting "));
        assert_ok!(Balances::transfer_allow_death(
            RuntimeOrigin::signed(who),
            MultiAddress::Id(account(230)),
            DECIMALS
        ));
        System::reset_events();
        let before = commitment("normal-unlock-before-replay");
        let result = std::panic::catch_unwind(Executive::execute_on_runtime_upgrade);
        assert_eq!(commitment("normal-unlock-after-replay"), before);
        assert!(
            result.is_ok(),
            "registered replay rejected normal completed vesting"
        );
    });
}

#[test]
fn v14_integrated_observer_version_matrix_preserves_rejected_namespaces() {
    for scenario in 0..7 {
        let (mut ext, _) = integrated_ext();
        ext.execute_with(|| {
            let key = StorageVersion::storage_key::<ValidatorSecurity>();
            match scenario {
                0 => {} // absent, clean -> initialized
                1 => StorageVersion::new(1).put::<ValidatorSecurity>(),
                2 => StorageVersion::new(0).put::<ValidatorSecurity>(),
                3 => StorageVersion::new(2).put::<ValidatorSecurity>(),
                4 => sp_io::storage::set(&key, &[255]),
                5 => sp_io::storage::set(&sp_io::hashing::twox_128(b"ValidatorSecurity"), &[1]),
                6 => {
                    let mut dirty = sp_io::hashing::twox_128(b"ValidatorSecurity").to_vec();
                    dirty.push(0);
                    sp_io::storage::set(&dirty, &[1]);
                }
                _ => unreachable!(),
            }
            let before = namespace(b"ValidatorSecurity");
            commitment(&format!("observer-{scenario}-before"));
            Executive::execute_on_runtime_upgrade();
            commitment(&format!("observer-{scenario}-after"));
            if scenario == 0 {
                assert_eq!(
                    StorageVersion::get::<ValidatorSecurity>(),
                    StorageVersion::new(1)
                );
            } else {
                assert_eq!(namespace(b"ValidatorSecurity"), before);
            }
            assert!(System::events()
                .iter()
                .all(|r| !matches!(r.event, RuntimeEvent::ValidatorSecurity(_))));
            assert!(SecurityBudget::migration_completed_at().is_some());
        });
    }
}

#[test]
fn v14_integrated_late_economic_failure_rolls_back_vesting_and_events() {
    let (mut ext, _) = integrated_ext();
    ext.execute_with(|| {
        let (who, _) = pallet_vesting::Vesting::<Runtime>::iter().next().unwrap();
        <Balances as frame_support::traits::LockableCurrency<AccountId>>::remove_lock(
            *b"vesting ",
            &who,
        );
        // Pin the already-installed observer so this assertion covers the V14 transaction.
        StorageVersion::new(1).put::<ValidatorSecurity>();
        System::reset_events();
        // V13 accepts the schedule bytes; V14 restores its lock, then fails bond validation.
        pallet_staking::MinValidatorBond::<Runtime>::put(10_000 * DECIMALS + 1);
        let before = commitment("late-failure-before");
        assert!(std::panic::catch_unwind(Executive::execute_on_runtime_upgrade).is_err());
        assert_eq!(commitment("late-failure-after"), before);
        assert!(System::events().is_empty());
        assert!(SecurityBudget::migration_completed_at().is_none());
    });
}

#[test]
fn v14_integrated_malformed_economic_version_must_reject_without_writes() {
    let mut rejected = Vec::new();
    for malformed in [vec![255], vec![0, 0, 255]] {
        let (mut ext, _) = integrated_ext();
        ext.execute_with(|| {
            sp_io::storage::set(&StorageVersion::storage_key::<SecurityBudget>(), &malformed);
            let before = commitment("malformed-economic-version-before");
            let result = std::panic::catch_unwind(Executive::execute_on_runtime_upgrade);
            let after = commitment("malformed-economic-version-after");
            rejected.push(result.is_err() && after == before);
            println!(
                "V14_6_MALFORMED bytes={malformed:?} rejected={} unchanged={}",
                result.is_err(),
                after == before
            );
        });
    }
    assert!(
        rejected.into_iter().all(|ok| ok),
        "malformed SecurityBudget version was accepted and overwritten"
    );
}

#[test]
fn v14_integrated_unexpected_economic_version_rejects_without_writes() {
    for version in [1, 2, 255, u16::MAX] {
        let (mut ext, _) = integrated_ext();
        ext.execute_with(|| {
            StorageVersion::new(version).put::<SecurityBudget>();
            StorageVersion::new(1).put::<ValidatorSecurity>();
            let before = commitment(&format!("unexpected-economic-{version}-before"));
            assert!(std::panic::catch_unwind(Executive::execute_on_runtime_upgrade).is_err());
            assert_eq!(
                commitment(&format!("unexpected-economic-{version}-after")),
                before
            );
        });
    }
}

#[cfg(feature = "try-runtime")]
#[test]
fn v14_integrated_try_runtime_actual_executive_pre_and_post() {
    let (mut diagnostic, _) = integrated_ext();
    diagnostic.execute_with(|| {
        assert_ok!(<v13_migration::V13Migration as OnRuntimeUpgrade>::try_on_runtime_upgrade(true));
    });
    let (mut ext, _) = integrated_ext();
    ext.execute_with(|| {
        commitment("try-runtime-before");
        let result = TryRuntimeExecutive::try_runtime_upgrade(frame_try_runtime::UpgradeCheckSelect::PreAndPost);
        if result.is_err() {
            macro_rules! versions { ($($p:ty),*) => { $(println!("V14_VERSION {} chain={:?} code={:?}", stringify!($p),
                <$p as GetStorageVersion>::on_chain_storage_version(), <$p as GetStorageVersion>::in_code_storage_version());)* }; }
            versions!(System, Timestamp, Balances, TransactionPayment, Sudo, Session, Historical, Babe, Grandpa,
                Authorship, Staking, Vesting, AiPredictions, RewardReserve, Multisig, Proxy, Assets, Nfts, EraWorlds,
                IssuanceCap, SecurityBudget, ValidatorSecurity);
        }
        assert_ok!(result);
        commitment("try-runtime-after");
        assert_eq!(
            SecurityBudget::legacy_reward_liability(),
            47_913_372_622_298_883_618_387
        );
    });
}

#[test]
fn v14_integrated_absent_new_security_budget_must_migrate() {
    let (mut ext, _) = integrated_ext();
    ext.execute_with(|| {
        sp_io::storage::clear(&StorageVersion::storage_key::<SecurityBudget>());
        assert!(namespace(b"SecurityBudget").is_empty());
        commitment("absent-security-budget-before");
        let result = std::panic::catch_unwind(Executive::execute_on_runtime_upgrade);
        commitment("absent-security-budget-after");
        assert!(
            result.is_ok(),
            "Executive pre-hook initialized the new pallet before the coordinated migration"
        );
        assert_eq!(
            SecurityBudget::legacy_reward_liability(),
            47_913_372_622_298_883_618_387
        );
    });
}

#[test]
fn v14_integrated_before_all_writes_are_outside_v14_transaction() {
    let (mut ext, _) = integrated_ext();
    ext.execute_with(|| {
        StorageVersion::new(2).put::<SecurityBudget>();
        let before = commitment("outer-boundary-before");
        assert!(std::panic::catch_unwind(Executive::execute_on_runtime_upgrade).is_err());
        // Regression strengthened for the correction: no generated observer write
        // may precede a rejected economic migration. No diagnostic deletion is needed.
        assert_eq!(commitment("outer-boundary-after"), before);
        assert!(!sp_io::storage::exists(&StorageVersion::storage_key::<
            ValidatorSecurity,
        >()));
        assert!(System::events().is_empty());
    });
}

#[test]
fn v14_correction_all_economic_versions_strictly_reject_malformed_and_unknown_bytes() {
    let keys = [
        StorageVersion::storage_key::<IssuanceCap>(),
        StorageVersion::storage_key::<RewardReserve>(),
        StorageVersion::storage_key::<SecurityBudget>(),
        StorageVersion::storage_key::<Staking>(),
    ];
    for key in keys {
        for bytes in [
            vec![],
            vec![0],
            vec![255],
            vec![0, 0, 255],
            vec![4, 0],
            vec![255, 0],
            vec![255, 255],
            vec![255; 4096],
        ] {
            let (mut ext, _) = integrated_ext();
            ext.execute_with(|| {
                sp_io::storage::set(&key, &bytes);
                let before = sp_io::storage::root(StateVersion::V1);
                assert!(std::panic::catch_unwind(Executive::execute_on_runtime_upgrade).is_err());
                assert_eq!(sp_io::storage::root(StateVersion::V1), before);
                #[cfg(feature = "try-runtime")]
                {
                    let rejected = std::panic::catch_unwind(|| {
                        TryRuntimeExecutive::try_runtime_upgrade(
                            frame_try_runtime::UpgradeCheckSelect::PreAndPost,
                        )
                    });
                    assert!(!matches!(rejected, Ok(Ok(_))));
                    assert_eq!(sp_io::storage::root(StateVersion::V1), before);
                }
            });
        }
    }
}

#[test]
fn v14_correction_partial_security_accounting_is_never_initialized_or_reset() {
    for absent_version in [false, true] {
        for scenario in 0..4 {
            let (mut ext, _) = integrated_ext();
            ext.execute_with(|| {
                if absent_version {
                    sp_io::storage::clear(&StorageVersion::storage_key::<SecurityBudget>());
                }
                match scenario {
                    0 => pallet_security_budget::StakingPaidTotal::<Runtime>::put(1),
                    1 => pallet_security_budget::TreasuryIssuanceTotal::<Runtime>::put(1),
                    2 => pallet_security_budget::Active::<Runtime>::put(false), // even explicit default is partial state
                    3 => sp_io::storage::set(&sp_io::hashing::twox_128(b"SecurityBudget"), &[1]),
                    _ => unreachable!(),
                }
                let before = sp_io::storage::root(StateVersion::V1);
                assert!(std::panic::catch_unwind(Executive::execute_on_runtime_upgrade).is_err());
                assert_eq!(sp_io::storage::root(StateVersion::V1), before);
            });
        }
    }
}

#[test]
fn v14_correction_marker_corruption_and_burn_restored_allowance_reject() {
    for scenario in 0..6 {
        let (mut ext, validators) = integrated_ext();
        ext.execute_with(|| {
            Executive::execute_on_runtime_upgrade();
            assert_ok!(SecurityBudget::activate(RuntimeOrigin::root()));
            install_v14_equal_points(11, &validators);
            let year = SecurityBudgetMillisecondsPerYear::get();
            assert_ok!(SecurityBudget::finalize_completed_era(
                11,
                year,
                1_000_000 + year
            ));
            match scenario {
                0 => issuance_cap::V13MigrationCompleted::<Runtime>::mutate(|m| {
                    m.as_mut().unwrap().migration_version = 14
                }),
                1 => issuance_cap::V13MigrationCompleted::<Runtime>::mutate(|m| {
                    m.as_mut().unwrap().input.reconciliation_vector_hash = [0; 32]
                }),
                2 => sp_io::storage::set(
                    &issuance_cap::V13MigrationCompleted::<Runtime>::hashed_key(),
                    &[0; 167],
                ),
                3 => issuance_cap::V13MigrationCompleted::<Runtime>::kill(),
                4 => {
                    // A real burn reduces supply, never the gross consumed issuance evidence.
                    assert_ok!(Balances::burn(
                        RuntimeOrigin::signed(validators[0].clone()),
                        DECIMALS,
                        true
                    ));
                    issuance_cap::RemainingAllowance::<Runtime>::mutate(|v| {
                        *v = v.map(|n| n + DECIMALS)
                    });
                }
                5 => pallet_security_budget::IssuanceSplitCarry::<Runtime>::put(10),
                _ => unreachable!(),
            }
            let before = sp_io::storage::root(StateVersion::V1);
            let result = std::panic::catch_unwind(Executive::execute_on_runtime_upgrade);
            if scenario == 2 {
                // Oversized marker bytes are rejected by the bounded classifier without a panic.
                assert!(result.is_ok());
            } else {
                assert!(result.is_err());
            }
            assert_eq!(sp_io::storage::root(StateVersion::V1), before);
        });
    }
}

#[test]
fn v14_correction_genesis_and_unaffected_pre_hooks_are_preserved() {
    let mut storage = RuntimeGenesisConfig::default().build_storage().unwrap();
    // Full construct_runtime genesis, not an upgrade, still initializes declared versions.
    let mut ext = sp_io::TestExternalities::new(core::mem::take(&mut storage));
    ext.execute_with(|| {
        assert_eq!(StorageVersion::get::<SecurityBudget>(), StorageVersion::new(1));
        assert_eq!(StorageVersion::get::<ValidatorSecurity>(), StorageVersion::new(1));
        sp_io::storage::clear(&StorageVersion::storage_key::<Assets>());
        <crate::v14_migration_lifecycle::MigrationLifecycle as BeforeAllRuntimeMigrations>::before_all_runtime_migrations();
        assert_eq!(StorageVersion::get::<Assets>(), StorageVersion::new(1));
    });
}

#[cfg(feature = "try-runtime")]
#[test]
fn v14_correction_try_runtime_absence_then_claim_burn_and_completed_vesting_replay() {
    let (mut ext, validators) = integrated_ext();
    ext.execute_with(|| {
        sp_io::storage::clear(&StorageVersion::storage_key::<SecurityBudget>());
        assert_ok!(TryRuntimeExecutive::try_runtime_upgrade(
            frame_try_runtime::UpgradeCheckSelect::PreAndPost
        ));
        assert_ok!(SecurityBudget::activate(RuntimeOrigin::root()));
        install_v14_equal_points(11, &validators);
        let year = SecurityBudgetMillisecondsPerYear::get();
        assert_ok!(SecurityBudget::finalize_completed_era(
            11,
            year,
            1_000_000 + year
        ));
        pallet_staking::ActiveEra::<Runtime>::put(pallet_staking::ActiveEraInfo {
            index: 12,
            start: Some(1_000_000 + year),
        });
        install_v14_equal_points(12, &validators);
        assert_ok!(SecurityBudget::claim_reward_page(
            RuntimeOrigin::signed(account(230)),
            11,
            validators[0].clone(),
            0
        ));
        let allowance = IssuanceCap::remaining_allowance();
        assert_ok!(Balances::burn(
            RuntimeOrigin::signed(validators[0].clone()),
            DECIMALS,
            true
        ));
        assert_eq!(IssuanceCap::remaining_allowance(), allowance);
        let beneficiaries = pallet_vesting::Vesting::<Runtime>::iter_keys().collect::<Vec<_>>();
        System::set_block_number(1_830_990 + upgrade13_policy::FOUNDING_VESTING_RELEASE_INTERVALS);
        for who in &beneficiaries {
            assert_ok!(Vesting::vest_other(
                RuntimeOrigin::signed(account(230)),
                MultiAddress::Id(who.clone())
            ));
            assert!(pallet_vesting::Vesting::<Runtime>::get(who).is_none());
        }
        assert_ok!(Balances::transfer_allow_death(
            RuntimeOrigin::signed(beneficiaries[0].clone()),
            MultiAddress::Id(account(230)),
            DECIMALS
        ));
        System::reset_events();
        let before = commitment("corrected-try-replay-before");
        assert_ok!(TryRuntimeExecutive::try_runtime_upgrade(
            frame_try_runtime::UpgradeCheckSelect::PreAndPost
        ));
        assert_eq!(commitment("corrected-try-replay-after"), before);
    });
}

#[cfg(feature = "try-runtime")]
#[test]
fn v14_correction_initial_v13_try_retains_exact_reconciliation_checks() {
    v13_migration_test_ext().execute_with(|| {
        assert!(issuance_cap::V13MigrationCompleted::<Runtime>::get().is_none());
        assert_ok!(<v13_migration::V13Migration as OnRuntimeUpgrade>::try_on_runtime_upgrade(true));
        assert_eq!(Balances::total_issuance(), 100_000_000 * DECIMALS);
        assert_eq!(
            IssuanceCap::remaining_allowance(),
            Some(900_000_000 * DECIMALS)
        );
        let before = sp_io::storage::root(StateVersion::V1);
        assert_ok!(<v13_migration::V13Migration as OnRuntimeUpgrade>::try_on_runtime_upgrade(true));
        assert_eq!(sp_io::storage::root(StateVersion::V1), before);
    });
}

#[test]
fn v14_correction_pristine_weight_uses_existing_measured_branch_envelopes() {
    use era_validator_security::observation::WeightInfo;
    type W = era_validator_security::weights::SubstrateWeight<Runtime>;
    let envelope = crate::v14_migration_lifecycle::pristine_weight();
    assert!(v14_migration::declared_weight().all_gte(W::migration_initialize_absent()));
    assert!(envelope.all_gte(W::migration_reject_dirty(0)));
    assert!(envelope.all_gte(W::migration_reject_dirty(1)));
    assert!(envelope.all_gte(W::migration_reject_version(2)));
    assert!(envelope.all_gte(W::migration_repeated_v1()));
    // This is the same bounded installation envelope assertion as the WS3 gate,
    // now including the added pristine-state probes; it is not a timing benchmark.
    let limits: frame_system::limits::BlockWeights =
        <Runtime as frame_system::Config>::BlockWeights::get();
    assert!(v14_migration::declared_weight().all_lte(limits.max_block));
}

#[test]
fn v14_correction_actual_initial_v13_absence_preserves_supported_predecessor() {
    let (mut ext, validators) = integrated_fixture(false);
    ext.execute_with(|| {
        assert!(namespace(b"IssuanceCap").is_empty());
        assert_eq!(
            Balances::total_issuance(),
            v13_migration::R6R3_PRE_MIGRATION_ISSUANCE
        );
        #[cfg(not(feature = "try-runtime"))]
        Executive::execute_on_runtime_upgrade();
        #[cfg(feature = "try-runtime")]
        assert_ok!(TryRuntimeExecutive::try_runtime_upgrade(
            frame_try_runtime::UpgradeCheckSelect::PreAndPost
        ));
        assert_eq!(Balances::total_issuance(), 100_000_000 * DECIMALS);
        assert_eq!(
            IssuanceCap::remaining_allowance(),
            Some(900_000_000 * DECIMALS)
        );
        assert_eq!(StorageVersion::get::<IssuanceCap>(), StorageVersion::new(1));
        assert_eq!(
            SecurityBudget::legacy_reward_liability(),
            47_913_372_622_298_883_618_387
        );
        assert_eq!(Session::validators(), validators);
    });
}

#[test]
fn v14_correction_initial_issuance_namespace_corruption_is_not_initialization() {
    for exact_prefix in [false, true] {
        let (mut ext, _) = integrated_fixture(false);
        ext.execute_with(|| {
            if exact_prefix {
                sp_io::storage::set(&sp_io::hashing::twox_128(b"IssuanceCap"), &[1]);
            } else {
                sp_io::storage::set(
                    &issuance_cap::RemainingAllowance::<Runtime>::hashed_key(),
                    &[],
                );
            }
            let before = sp_io::storage::root(StateVersion::V1);
            assert!(std::panic::catch_unwind(Executive::execute_on_runtime_upgrade).is_err());
            assert_eq!(sp_io::storage::root(StateVersion::V1), before);
        });
    }
}

#[test]
fn nft_v2_actual_executive_rejects_bad_version_and_new_prefix_before_initialization() {
    for malformed in [
        None,
        Some(vec![1]),
        Some(vec![1, 0, 0]),
        Some(vec![0, 0]),
        Some(vec![3, 0]),
    ] {
        let (mut ext, _) = integrated_ext();
        ext.execute_with(|| {
            if let Some(bytes) = malformed {
                sp_io::storage::set(&StorageVersion::storage_key::<Nfts>(), &bytes);
            } else {
                StorageVersion::new(1).put::<Nfts>();
                pallet_nfts::DelegateCleanup::<Runtime>::insert(
                    7,
                    (9, AccountId::new([71; 32])),
                    pallet_nfts::CleanupProgress {
                        version: 1,
                        phase: 0,
                    },
                );
            }
            let before = sp_io::storage::root(StateVersion::V1);
            assert!(std::panic::catch_unwind(Executive::execute_on_runtime_upgrade).is_err());
            assert_eq!(sp_io::storage::root(StateVersion::V1), before);
            #[cfg(feature = "try-runtime")]
            {
                assert!(TryRuntimeExecutive::try_runtime_upgrade(
                    frame_try_runtime::UpgradeCheckSelect::PreAndPost
                )
                .is_err());
                assert_eq!(sp_io::storage::root(StateVersion::V1), before);
            }
        });
    }
}

#[test]
fn nft_v2_actual_executive_installs_and_replays_without_scanning_progress() {
    let (mut ext, _) = integrated_ext();
    ext.execute_with(|| {
        StorageVersion::new(1).put::<Nfts>();
        Executive::execute_on_runtime_upgrade();
        assert_eq!(StorageVersion::get::<Nfts>(), StorageVersion::new(2));
        // Existing v2 progress must survive replay, not be mistaken for an initialization request.
        let progress = pallet_nfts::CleanupProgress {
            version: 1,
            phase: 3,
        };
        pallet_nfts::CollectionRetirement::<Runtime>::insert(7, progress);
        Executive::execute_on_runtime_upgrade();
        assert_eq!(
            pallet_nfts::CollectionRetirement::<Runtime>::get(7),
            Some(progress)
        );
        assert_eq!(StorageVersion::get::<Nfts>(), StorageVersion::new(2));
    });
}
