use super::{mock::*, *};
#[test]
fn empty_registry_and_unique_items_fail_without_any_write() {
    ext().execute_with(|| {
        put(b"synthetic-admitted", false);
        let before = root();
        assert_eq!(
            Amm::create(&account(1), Asset::Native, Asset::Registered(1), 100),
            Err(Fault::Unconfigured)
        );
        assert_eq!(
            EmptyPairs::ensure(
                PoolId::new(FungibleAsset::NativeEtkn, FungibleAsset::Registered(1)).unwrap()
            ),
            Err(AssetError::UnsupportedAsset.into())
        );
        assert_eq!(root(), before);
        assert_eq!(
            Amm::create(&account(1), Asset::Native, Asset::Unique(1, 1), 100),
            Err(AssetError::UnsupportedAsset.into())
        );
        assert_eq!(root(), before);
    });
}
#[test]
fn create_add_swap_remove_preserve_supply_and_lp() {
    ext().execute_with(|| {
        let pair = initialized();
        assert_eq!(Amm::lp(pair, &account(1)), Ok(9990));
        for exact in [false, true] {
            let quote = Amm::quote(pair, 50, exact).unwrap();
            let outcome = Amm::swap(
                &account(2),
                pair,
                50,
                if exact { quote.0 } else { quote.1 },
                &account(3),
                100,
                exact,
            )
            .unwrap();
            assert_eq!(
                (outcome.amount_in, outcome.amount_out, outcome.total_fee),
                quote
            );
            assert_eq!(outcome.protocol_fee, 0);
            assert_eq!(Amm::reconcile(pair), Ok(()));
        }
        let full = Amm::lp(pair, &account(1)).unwrap();
        Amm::remove(&account(1), pair, full, [0, 0], &account(1), 100).unwrap();
        let pool = Amm::pool(pair).unwrap();
        assert_eq!((pool.total_lp, pool.locked_lp, pool.user_lp), (10, 10, 0));
        assert!(!Positions::<Test>::contains_key(pair, account(1)));
        assert!(AccountPools::<Test>::get(account(1)).is_empty());
        assert_eq!(Amm::reconcile(pair), Ok(()));
    });
}

#[test]
fn every_mutation_rolls_back_on_event_failure_including_creator_deposit_and_provider() {
    for action in 0..4 {
        ext().execute_with(|| {
            let pair = if action == 0 {
                (Asset::Native, Asset::Registered(1))
            } else {
                initialized()
            };
            put(b"synthetic-fail-event", true);
            let before = root();
            let result = match action {
                0 => Amm::create(&account(1), pair.0, pair.1, 100).map(|_| ()),
                1 => Amm::add(&account(2), pair, [100, 100], [0, 0], 100).map(|_| ()),
                2 => Amm::remove(&account(1), pair, 100, [0, 0], &account(2), 100).map(|_| ()),
                _ => Amm::swap(&account(2), pair, 100, 0, &account(3), 100, false).map(|_| ()),
            };
            assert_eq!(result, Err(Fault::CorruptState));
            assert_eq!(root(), before);
        });
    }
}
#[test]
fn both_transfer_failure_points_and_nonstandard_backends_restore_the_complete_root() {
    for action in 0..4 {
        for fault in 0..5 {
            ext().execute_with(|| {
                let pair = initialized();
                if fault < 2 {
                    put(
                        b"synthetic-fail-transfer",
                        get::<u32>(b"synthetic-transfers") + fault + 1,
                    );
                } else {
                    put(b"synthetic-malicious", (fault - 1) as u8);
                }
                let before = root();
                let result = match action {
                    0 => Amm::add(&account(2), pair, [100, 100], [0, 0], 100).map(|_| ()),
                    1 => Amm::remove(&account(1), pair, 100, [0, 0], &account(2), 100).map(|_| ()),
                    _ => Amm::swap(
                        &account(2),
                        pair,
                        100,
                        if action == 3 { 200 } else { 0 },
                        &account(3),
                        100,
                        action == 3,
                    )
                    .map(|_| ()),
                };
                assert!(result.is_err(), "action={action}, fault={fault}");
                assert_eq!(root(), before);
            });
        }
    }
}
#[test]
fn custody_conversion_collisions_contamination_and_liveness_are_fail_closed() {
    use frame_support::PalletId;
    use sp_runtime::traits::AccountIdConversion;
    let short: Option<u64> = PalletId(*b"tst/amm0").try_into_sub_account(1u8);
    assert_eq!(short, None);
    for mode in 0..5 {
        ext().execute_with(|| {
            let pair = (Asset::Native, Asset::Registered(1));
            let id = PoolId::new(pair.0.fungible().unwrap(), pair.1.fungible().unwrap()).unwrap();
            let custody = Custody.derive_pool_account(&id).unwrap();
            match mode {
                0 => put(b"synthetic-custody-override", account(1)),
                1 => conflicting_identity(&custody), // protocol identity, not ordinary prefunding
                2 => put(b"synthetic-no-provider", true),
                3 => put(b"synthetic-bad-reserve", true),
                _ => {
                    initialized();
                    put(b"synthetic-custody-override", custody);
                }
            }
            let before = root();
            let result = if mode == 4 {
                Amm::create(&account(1), Asset::Native, Asset::Registered(2), 100)
            } else {
                Amm::create(&account(1), pair.0, pair.1, 100)
            };
            assert!(result.is_err());
            assert_eq!(root(), before);
        });
    }
}
#[test]
fn custody_cannot_be_a_participant_in_its_own_or_another_pool() {
    ext().execute_with(|| {
        let pair = initialized();
        let custody = Amm::pool(pair).unwrap().custody;
        let second =
            Amm::create(&account(1), Asset::Registered(2), Asset::Registered(3), 100).unwrap();
        Amm::add(&account(1), second, [10000, 10000], [0, 0], 100).unwrap();
        for selected in [pair, second] {
            let before = root();
            assert_eq!(
                Amm::swap(&account(2), selected, 10, 0, &custody, 100, false),
                Err(amm::Error::CustodyAccountCollision.into())
            );
            assert_eq!(
                Amm::add(&custody, selected, [100, 100], [0, 0], 100),
                Err(amm::Error::CustodyAccountCollision.into())
            );
            assert_eq!(root(), before);
        }
    });
}
#[test]
fn admission_authority_duplicate_deadline_slippage_and_overflow_fail_before_commit() {
    ext().execute_with(|| {
        let before = root();
        for (who, a, b, deadline) in [
            (account(2), Asset::Native, Asset::Registered(1), 100),
            (account(1), Asset::Native, Asset::Registered(99), 100),
            (account(1), Asset::Native, Asset::Native, 100),
            (account(1), Asset::Native, Asset::Registered(1), 99),
            (account(1), Asset::Native, Asset::Registered(1), 121),
        ] {
            assert!(Amm::create(&who, a, b, deadline).is_err());
            assert_eq!(root(), before);
        }
        let pair = initialized();
        let before = root();
        assert_eq!(
            Amm::create(&account(1), pair.1, pair.0, 120),
            Err(amm::Error::PoolAlreadyExists.into())
        );
        assert_eq!(root(), before);
        for deadline in [99, 121, u64::MAX] {
            assert!(Amm::swap(&account(2), pair, 100, 0, &account(2), deadline, false).is_err());
            assert_eq!(root(), before);
        }
        assert_eq!(
            Amm::swap(&account(2), pair, 100, u128::MAX, &account(2), 100, false),
            Err(amm::Error::SlippageExceeded.into())
        );
        assert_eq!(
            Amm::swap(&account(2), pair, 100, 0, &account(2), 100, true),
            Err(amm::Error::SlippageExceeded.into())
        );
        assert_eq!(
            Amm::add(&account(2), pair, [100, 100], [101, 101], 100),
            Err(amm::Error::SlippageExceeded.into())
        );
        assert!(Amm::add(&account(2), pair, [u128::MAX, u128::MAX], [0, 0], 100).is_err());
        assert_eq!(root(), before);
        // Inclusive now and inclusive horizon, with a changed block clock.
        Amm::swap(&account(2), pair, 100, 0, &account(2), 120, false).unwrap();
        put(b"synthetic-now", 120u64);
        Amm::swap(&account(2), pair, 100, 0, &account(2), 120, false).unwrap();
    });
}
#[test]
fn pool_account_provider_and_page_bounds_have_no_partial_side_effects() {
    ext().execute_with(|| {
        let first = initialized();
        let second = Amm::create(&account(1), Asset::Native, Asset::Registered(2), 100).unwrap();
        let third =
            Amm::create(&account(1), Asset::Registered(2), Asset::Registered(3), 100).unwrap();
        let before = root();
        assert_eq!(
            Amm::create(&account(1), Asset::Native, Asset::Registered(3), 100),
            Err(Fault::Bound)
        );
        assert_eq!(root(), before);
        assert_eq!(Amm::pools(None, 0), Err(Fault::InvalidLimit));
        assert_eq!(Amm::pools(None, 3), Err(Fault::InvalidLimit));
        assert_eq!(Amm::pools(None, 2), Ok((vec![first, second], Some(second))));
        assert_eq!(Amm::pools(Some(second), 2), Ok((vec![third], None)));
        assert_eq!(Amm::pools(Some(third), 2), Ok((vec![], None)));
        assert_eq!(
            Amm::pools(Some((Asset::Native, Asset::Registered(3))), 2),
            Err(Fault::InvalidCursor)
        );
        Amm::add(&account(1), second, [1000, 1000], [0, 0], 100).unwrap();
        let before = root();
        assert_eq!(
            Amm::add(&account(1), third, [1000, 1000], [0, 0], 100),
            Err(Fault::Bound)
        );
        assert_eq!(root(), before);
        assert_eq!(
            Amm::positions(&account(1), None, 1),
            Ok((vec![first], Some(first)))
        );
        assert_eq!(
            Amm::positions(&account(1), Some(first), 2),
            Ok((vec![second], None))
        );
        assert_eq!(
            Amm::positions(&account(2), Some(first), 2),
            Err(Fault::InvalidCursor)
        );
        for who in 2..=3 {
            Amm::add(&account(who), first, [100, 100], [0, 0], 100).unwrap();
        }
        let before = root();
        assert_eq!(
            Amm::add(&account(4), first, [100, 100], [0, 0], 100),
            Err(Fault::Bound)
        );
        assert_eq!(root(), before);
        let lp = Amm::lp(first, &account(1)).unwrap();
        Amm::remove(&account(1), first, lp, [0, 0], &account(1), 100).unwrap();
        assert_eq!(
            Amm::positions(&account(1), Some(first), 2),
            Err(Fault::InvalidCursor)
        );
        Amm::add(&account(1), third, [1000, 1000], [0, 0], 100).unwrap();
        Amm::add(&account(4), first, [100, 100], [0, 0], 100).unwrap();
        for pair in [first, second, third] {
            assert_eq!(Amm::reconcile(pair), Ok(()));
        }
    });
}
#[test]
fn reconciliation_detects_lp_index_or_reserve_corruption_without_repairing_it() {
    for fault in 0..4 {
        ext().execute_with(|| {
            let pair = initialized();
            match fault {
                0 => Positions::<Test>::insert(pair, account(1), 9991),
                1 => AccountPools::<Test>::remove(account(1)),
                2 => PoolOrdinal::<Test>::insert(pair, 2),
                _ => {
                    let p = Amm::pool(pair).unwrap();
                    seed(Asset::Native, &p.custody, p.reserves[0] - 1);
                }
            }
            let before = root();
            assert!(Amm::reconcile(pair).is_err());
            assert_eq!(root(), before);
        });
    }
}

#[derive(Clone)]
struct MemoryLedger {
    balances: alloc::collections::BTreeMap<(FungibleAsset<u32>, Account), u128>,
}
impl FungibleInspect<Account> for MemoryLedger {
    type AssetId = u32;
    fn balance(&self, a: FungibleAsset<u32>, who: &Account) -> u128 {
        self.balances.get(&(a, who.clone())).copied().unwrap_or(0)
    }
    fn total_issuance(&self, a: FungibleAsset<u32>) -> core::result::Result<u128, AssetError> {
        Ok(self
            .balances
            .iter()
            .filter(|((asset, _), _)| *asset == a)
            .map(|(_, n)| *n)
            .sum())
    }
}
impl FungibleTransfer<Account> for MemoryLedger {
    fn transfer(
        &mut self,
        a: FungibleAsset<u32>,
        from: &Account,
        to: &Account,
        amount: u128,
    ) -> core::result::Result<(), AssetError> {
        let from_balance = self
            .balance(a, from)
            .checked_sub(amount)
            .ok_or(AssetError::InsufficientBalance)?;
        let to_balance = self
            .balance(a, to)
            .checked_add(amount)
            .ok_or(AssetError::ArithmeticOverflow)?;
        self.balances.insert((a, from.clone()), from_balance);
        self.balances.insert((a, to.clone()), to_balance);
        Ok(())
    }
}
#[derive(Clone)]
struct NoProtocolFee;
impl amm::ProtocolFeeRouter<Account, u32> for NoProtocolFee {
    fn route<L: FungibleTransfer<Account, AssetId = u32>>(
        &mut self,
        _: &mut L,
        _: &Account,
        _: FungibleAsset<u32>,
        _: u128,
    ) -> core::result::Result<(), amm::Error> {
        panic!("zero protocol fee must not route")
    }
}
#[test]
fn deterministic_differential_reference_model_sequences_match_pool_lp_quotes_and_balances() {
    // Both widths, 24 reserve ratios, both asset orientations, 32 mixed operations/sequence.
    for wide in [false, true] {
        for seed in 1..=24u64 {
            ext().execute_with(|| {
                put(b"synthetic-wide", wide);
                let pair = (Asset::Native, Asset::Registered(1));
                let a = pair.0.fungible().unwrap();
                let b = pair.1.fungible().unwrap();
                Amm::create(&account(1), pair.0, pair.1, 100).unwrap();
                let memory = MemoryLedger {
                    balances: (1..=5)
                        .flat_map(|id| {
                            [a, b].into_iter().map(move |asset| {
                                ((asset, account(id)), Ledger.balance(asset, &account(id)))
                            })
                        })
                        .collect(),
                };
                let config =
                    amm::AmmConfig::new(Fee::get(), Rate::new(0, 1).unwrap(), Lock::get()).unwrap();
                let mut reference =
                    amm::Amm::new(config, memory, Creator, Custody, NoProtocolFee).unwrap();
                let id = reference.create_pool(&account(1), a, b, 100, 100).unwrap();
                let amounts = [
                    10_000 + u128::from(seed) * 17,
                    10_000 + u128::from(seed) * 43,
                ];
                assert_eq!(
                    Amm::add(&account(1), pair, amounts, [0, 0], 100).unwrap(),
                    reference
                        .add_liquidity(&account(1), a, b, amounts[0], amounts[1], 0, 0, 100, 100)
                        .unwrap()
                );
                let mut random = seed;
                for turn in 0..32 {
                    random = random.wrapping_mul(6364136223846793005).wrapping_add(1);
                    let amount = 20 + u128::from((random >> 32) % 100);
                    let (oriented, left, right) = if turn % 2 == 0 {
                        (pair, a, b)
                    } else {
                        ((pair.1, pair.0), b, a)
                    };
                    match turn % 4 {
                        0 => {
                            assert_eq!(
                                Amm::quote(oriented, amount, false).unwrap().1,
                                reference.quote_exact_input(left, right, amount).unwrap()
                            );
                            assert_eq!(
                                Amm::swap(
                                    &account(2),
                                    oriented,
                                    amount,
                                    0,
                                    &account(3),
                                    100,
                                    false
                                )
                                .unwrap(),
                                reference
                                    .swap_exact_input(
                                        &account(2),
                                        left,
                                        right,
                                        amount,
                                        0,
                                        &account(3),
                                        100,
                                        100
                                    )
                                    .unwrap()
                            );
                        }
                        1 => {
                            assert_eq!(
                                Amm::quote(oriented, amount, true).unwrap().0,
                                reference.quote_exact_output(left, right, amount).unwrap()
                            );
                            assert_eq!(
                                Amm::swap(
                                    &account(2),
                                    oriented,
                                    amount,
                                    1000,
                                    &account(3),
                                    100,
                                    true
                                )
                                .unwrap(),
                                reference
                                    .swap_exact_output(
                                        &account(2),
                                        left,
                                        right,
                                        amount,
                                        1000,
                                        &account(3),
                                        100,
                                        100
                                    )
                                    .unwrap()
                            );
                        }
                        2 => {
                            assert_eq!(
                                Amm::add(&account(2), oriented, [500, 500], [0, 0], 100).unwrap(),
                                reference
                                    .add_liquidity(
                                        &account(2),
                                        left,
                                        right,
                                        500,
                                        500,
                                        0,
                                        0,
                                        100,
                                        100
                                    )
                                    .unwrap()
                            );
                        }
                        _ => {
                            assert_eq!(
                                Amm::remove(&account(2), oriented, 100, [0, 0], &account(3), 100)
                                    .unwrap(),
                                reference
                                    .remove_liquidity(
                                        &account(2),
                                        left,
                                        right,
                                        100,
                                        0,
                                        0,
                                        &account(3),
                                        100,
                                        100
                                    )
                                    .unwrap()
                            );
                        }
                    }
                    let actual = Amm::pool(pair).unwrap();
                    let expected = reference.pool(id).unwrap();
                    assert_eq!(
                        (actual.reserves, actual.total_lp, actual.locked_lp),
                        (
                            [expected.reserve_0, expected.reserve_1],
                            expected.total_lp,
                            expected.locked_liquidity
                        )
                    );
                    for who in [account(1), account(2), account(3), actual.custody] {
                        assert_eq!(Amm::lp(pair, &who).unwrap(), reference.lp_balance(id, &who));
                        for asset in [a, b] {
                            assert_eq!(
                                Ledger.balance(asset, &who),
                                reference.ledger().balance(asset, &who)
                            );
                        }
                    }
                    Amm::reconcile(pair).unwrap();
                    reference.verify_invariants().unwrap();
                }
            });
        }
    }
}

#[test]
fn malformed_bounded_records_and_orphan_positions_fail_without_defaulting_or_repair() {
    for path in 0..8 {
        ext().execute_with(|| {
            let pair = initialized();
            let key = match path {
                0 => Pools::<Test>::hashed_key_for(pair),
                1 => Positions::<Test>::hashed_key_for(pair, account(1)),
                2 => AccountPools::<Test>::hashed_key_for(account(1)),
                3 => Providers::<Test>::hashed_key_for(pair),
                4 => PoolCount::<Test>::hashed_key().to_vec(),
                5 => Busy::<Test>::hashed_key().to_vec(),
                6 => PoolOrdinal::<Test>::hashed_key_for(pair),
                _ => Custodies::<Test>::hashed_key_for(Amm::pool(pair).unwrap().custody),
            };
            // Invalid/truncated SCALE, never an absent value and never silently a default.
            sp_io::storage::set(&key, &[255]);
            let before = root();
            let result = if path == 4 || path == 6 {
                Amm::pools(None, 2).map(|_| ())
            } else {
                Amm::add(&account(1), pair, [100, 100], [0, 0], 100).map(|_| ())
            };
            assert_eq!(result, Err(Fault::CorruptState), "path={path}");
            assert_eq!(root(), before);
        });
    }
    ext().execute_with(|| {
        let pair = initialized();
        Positions::<Test>::insert(pair, account(4), 100);
        let before = root();
        assert_eq!(Amm::reconcile(pair), Err(Fault::CorruptState));
        assert_eq!(root(), before);
        Positions::<Test>::remove(pair, account(4));
        let mut key = Positions::<Test>::hashed_key_for(pair, account(4));
        key[60] ^= 1;
        sp_io::storage::set(&key, &100u128.encode());
        let before = root();
        assert_eq!(Amm::reconcile(pair), Err(Fault::CorruptState));
        assert_eq!(root(), before);
    });
}
#[test]
fn widened_math_retains_all_reference_rounding_and_checks_final_u128_results() {
    let fee = Fee::get();
    for r0 in [11, 999, 10_000, 1_000_000] {
        for r1 in [12, 1500, 11_000, 2_000_000] {
            for amount in 1..=256 {
                for exact in [false, true] {
                    assert_eq!(
                        WideArithmetic::quote([r0, r1], amount, fee, exact),
                        ReferenceArithmetic::quote([r0, r1], amount, fee, exact)
                    );
                }
            }
            for desired in [[11, 21], [1000, 500], [100, 700]] {
                assert_eq!(
                    WideArithmetic::add([r0, r1], 10_000, desired, 10),
                    ReferenceArithmetic::add([r0, r1], 10_000, desired, 10)
                );
                assert_eq!(
                    WideArithmetic::add([0, 0], 0, desired, 10),
                    ReferenceArithmetic::add([0, 0], 0, desired, 10)
                );
            }
            assert_eq!(
                WideArithmetic::remove([r0, r1], 10_000, 91),
                ReferenceArithmetic::remove([r0, r1], 10_000, 91)
            );
        }
    }
    assert_eq!(
        WideArithmetic::add([0, 0], 0, [u128::MAX, u128::MAX], 10),
        Ok(([u128::MAX, u128::MAX], u128::MAX - 10, 10))
    );
    assert_eq!(
        WideArithmetic::divide(u128::MAX, u128::MAX, u128::MAX, false),
        Ok(u128::MAX)
    );
    assert_eq!(
        WideArithmetic::divide(u128::MAX, u128::MAX, 1, false),
        Err(amm::Error::ArithmeticOverflow.into())
    );
    assert_eq!(
        WideArithmetic::quote([u128::MAX, 1000], 999, fee, true),
        Err(amm::Error::ArithmeticOverflow.into())
    );
    assert_eq!(
        WideArithmetic::quote(
            [100, 100],
            1,
            Rate {
                numerator: 0,
                denominator: 0
            },
            false
        ),
        Err(amm::Error::InvalidConfiguration.into())
    );
    // A separately selected fixture exercises widening through actual FRAME state transitions.
    ext().execute_with(|| {
        put(b"synthetic-wide", true);
        for who in 1..=3 {
            for asset in [Asset::Native, Asset::Registered(1)] {
                seed(asset, &account(who), 1000 * 10u128.pow(18));
            }
        }
        let pair = (Asset::Native, Asset::Registered(1));
        Amm::create(&account(1), pair.0, pair.1, 100).unwrap();
        Amm::add(&account(1), pair, [20 * 10u128.pow(18); 2], [0, 0], 100).unwrap();
        let quote = Amm::quote(pair, 10u128.pow(18), false).unwrap();
        let result =
            Amm::swap(&account(2), pair, quote.0, quote.1, &account(3), 100, false).unwrap();
        assert_eq!(
            (result.amount_in, result.amount_out, result.total_fee),
            quote
        );
        Amm::reconcile(pair).unwrap();
    });
}

#[test]
fn initial_exit_constant_work_matches_exhaustive_small_domain_and_wide_boundaries() {
    // The exhaustive search is only an independent test oracle; production uses two divisions.
    for r0 in 1..=12u128 {
        for r1 in 1..=12u128 {
            for supply in 2..=20u128 {
                for minimums in [[1, 1], [2, 3], [12, 12]] {
                    for surplus in [[0, 0], [2, 1]] {
                        let user = supply - 1;
                        let expected = (1..=user).find_map(|burn| {
                            let out = [burn * r0 / supply, burn * r1 / supply];
                            (out[0] > 0
                                && out[1] > 0
                                && r0 - out[0] + surplus[0] >= minimums[0]
                                && r1 - out[1] + surplus[1] >= minimums[1])
                                .then_some((burn, out))
                        });
                        assert_eq!(
                            initial_exit::<WideArithmetic>(
                                [r0, r1],
                                supply,
                                user,
                                surplus,
                                minimums,
                                1
                            )
                            .ok(),
                            expected
                        );
                    }
                }
            }
        }
    }
    assert_eq!(
        initial_exit::<WideArithmetic>(
            [u128::MAX; 2],
            u128::MAX,
            u128::MAX - 1000,
            [0; 2],
            [1; 2],
            1
        ),
        Ok((1, [1; 2]))
    );
    assert_eq!(
        initial_exit::<WideArithmetic>(
            [10u128.pow(18), 1],
            1_000_000_000,
            999_999_000,
            [0; 2],
            [100_000_000_000_000, 1],
            1
        ),
        Err(amm::Error::InsufficientInitialLiquidity.into())
    );
    assert_eq!(
        initial_exit::<WideArithmetic>(
            [u128::MAX; 2],
            u128::MAX,
            u128::MAX - 1000,
            [2; 2],
            [1; 2],
            1
        ),
        Err(amm::Error::ArithmeticOverflow.into())
    );
}

mod adopted_codec {
    use super::*;
    use crate::v1::{self as c, AmmErrorV1 as E, Asset as A, CallV1 as C, EventV1 as V};
    fn le(values: &[u128]) -> Vec<u8> {
        values.iter().flat_map(|n| n.to_le_bytes()).collect()
    }
    fn pair() -> c::PoolId {
        c::PoolId::new(A::Native, A::Registered(0x01020304)).unwrap()
    }
    fn pair_bytes() -> Vec<u8> {
        vec![0, 1, 4, 3, 2, 1]
    }
    fn golden<T: Encode + DecodeAll + MaxEncodedLen + PartialEq + core::fmt::Debug>(
        value: T,
        bytes: Vec<u8>,
    ) {
        assert_eq!(value.encode(), bytes);
        assert_eq!(c::decode_exact::<T>(&bytes).unwrap(), value);
        for end in 0..bytes.len() {
            assert!(c::decode_exact::<T>(&bytes[..end]).is_err());
        }
        let mut extra = bytes;
        extra.push(0);
        assert!(c::decode_exact::<T>(&extra).is_err());
    }
    #[test]
    fn assets_pairs_records_quotes_and_pages_have_fixed_canonical_vectors() {
        golden(A::Native, vec![0]);
        golden(A::Registered(0), vec![1, 0, 0, 0, 0]);
        golden(A::Registered(0x01020304), vec![1, 4, 3, 2, 1]);
        golden(pair(), pair_bytes());
        assert!(c::decode_exact::<A>(&[2, 0, 0, 0, 0]).is_err());
        for bad in [
            vec![0, 0],
            vec![1, 4, 3, 2, 1, 0],
            vec![1, 4, 3, 2, 1, 1, 4, 3, 2, 1],
        ] {
            assert!(c::decode_exact::<c::PoolId>(&bad).is_err());
        }
        let record = c::PoolRecord {
            custody: [1; 32],
            creator: [2; 32],
            deposit: 10 * c::ETKN,
            reserve_0: 41,
            reserve_1: 61,
            total_lp: 1200,
            locked_lp: 1000,
            user_lp: 200,
        };
        let record_bytes = [
            vec![1; 32],
            vec![2; 32],
            le(&[10 * c::ETKN, 41, 61, 1200, 1000, 200]),
        ]
        .concat();
        golden(record.clone(), record_bytes.clone());
        golden(
            c::PoolV1 {
                pool: pair(),
                record,
            },
            [pair_bytes(), record_bytes].concat(),
        );
        let position = c::LpPositionV1 {
            pool: pair(),
            account: [3; 32],
            lp: 200,
        };
        let position_bytes = [pair_bytes(), vec![3; 32], le(&[200])].concat();
        golden(position.clone(), position_bytes.clone());
        golden(
            c::QuoteV1 {
                pool: pair(),
                asset_in: A::Registered(0x01020304),
                asset_out: A::Native,
                amount_in: 401,
                amount_out: 299,
                total_fee: 2,
                protocol_fee: 0,
            },
            [pair_bytes(), vec![1, 4, 3, 2, 1, 0], le(&[401, 299, 2, 0])].concat(),
        );
        golden(
            c::PageV1::<c::LpPositionV1> {
                entries: Default::default(),
                next: None,
            },
            vec![0, 0],
        );
        golden(
            c::PageV1 {
                entries: vec![position.clone()].try_into().unwrap(),
                next: Some(pair()),
            },
            [vec![4], position_bytes.clone(), vec![1], pair_bytes()].concat(),
        );
        let page = c::PageV1 {
            entries: vec![position; 64].try_into().unwrap(),
            next: None,
        };
        let bytes = [vec![1, 1], position_bytes.repeat(64), vec![0]].concat();
        golden(page, bytes);
        let oversized = [vec![5, 1], position_bytes.repeat(65), vec![0]].concat();
        assert!(c::decode_exact::<c::PageV1<c::LpPositionV1>>(&oversized).is_err());
        for malformed in [vec![1, 0, 0], vec![0, 2], vec![2, 0, 0, 0, 0]] {
            assert!(c::decode_exact::<c::PageV1<c::LpPositionV1>>(&malformed).is_err());
        }
        golden::<c::ApiResult<u128>>(Ok(257), [vec![0], le(&[257])].concat());
        golden::<c::ApiResult<u128>>(Err(E::BackendInvariant), vec![1, 6]);
    }
    #[test]
    fn all_five_calls_and_four_events_have_independent_field_order_vectors() {
        let a = A::Native;
        let b = A::Registered(0x01020304);
        let deadline = 0x05060708;
        let tail = vec![8, 7, 6, 5];
        golden(
            C::CreatePool {
                asset_a: a,
                asset_b: b,
                deadline,
            },
            [vec![0], pair_bytes(), tail.clone()].concat(),
        );
        golden(
            C::AddLiquidity {
                asset_a: a,
                asset_b: b,
                desired_a: 11,
                desired_b: 22,
                min_a: 3,
                min_b: 4,
                deadline,
            },
            [vec![1], pair_bytes(), le(&[11, 22, 3, 4]), tail.clone()].concat(),
        );
        golden(
            C::RemoveLiquidity {
                asset_a: a,
                asset_b: b,
                lp: 77,
                min_a: 3,
                min_b: 4,
                recipient: [5; 32],
                deadline,
            },
            [
                vec![2],
                pair_bytes(),
                le(&[77, 3, 4]),
                vec![5; 32],
                tail.clone(),
            ]
            .concat(),
        );
        golden(
            C::SwapExactInput {
                asset_in: a,
                asset_out: b,
                amount_in: 99,
                min_out: 33,
                recipient: [6; 32],
                deadline,
            },
            [
                vec![3],
                pair_bytes(),
                le(&[99, 33]),
                vec![6; 32],
                tail.clone(),
            ]
            .concat(),
        );
        golden(
            C::SwapExactOutput {
                asset_in: a,
                asset_out: b,
                amount_out: 33,
                max_in: 99,
                recipient: [7; 32],
                deadline,
            },
            [vec![4], pair_bytes(), le(&[33, 99]), vec![7; 32], tail].concat(),
        );
        assert!(c::decode_exact::<C>(&[5]).is_err());
        golden(
            V::PoolCreated {
                pool: pair(),
                creator: [1; 32],
                custody_account: [2; 32],
            },
            [vec![0], pair_bytes(), vec![1; 32], vec![2; 32]].concat(),
        );
        golden(
            V::LiquidityAdded {
                pool: pair(),
                provider: [3; 32],
                amount_0: 11,
                amount_1: 22,
                lp_minted: 33,
            },
            [vec![1], pair_bytes(), vec![3; 32], le(&[11, 22, 33])].concat(),
        );
        golden(
            V::LiquidityRemoved {
                pool: pair(),
                provider: [4; 32],
                recipient: [5; 32],
                amount_0: 11,
                amount_1: 22,
                lp_burned: 33,
            },
            [
                vec![2],
                pair_bytes(),
                vec![4; 32],
                vec![5; 32],
                le(&[11, 22, 33]),
            ]
            .concat(),
        );
        let bytes = [
            vec![3],
            pair_bytes(),
            vec![6; 32],
            vec![7; 32],
            pair_bytes(),
            le(&[401, 299, 2, 0]),
            vec![1],
        ]
        .concat();
        golden(
            V::SwapExecuted {
                pool: pair(),
                trader: [6; 32],
                recipient: [7; 32],
                asset_in: a,
                asset_out: b,
                amount_in: 401,
                amount_out: 299,
                total_fee: 2,
                protocol_fee: 0,
                exact_output: true,
            },
            bytes.clone(),
        );
        let mut invalid_bool = bytes;
        *invalid_bool.last_mut().unwrap() = 2;
        assert!(c::decode_exact::<V>(&invalid_bool).is_err());
        assert!(c::decode_exact::<V>(&[4]).is_err());
        let bad = Event::SwapExecuted {
            pool: PoolId::new(FungibleAsset::NativeEtkn, FungibleAsset::Registered(1)).unwrap(),
            trader: account(1),
            recipient: account(2),
            asset_in: FungibleAsset::NativeEtkn,
            asset_out: FungibleAsset::Registered(1),
            amount_in: 10,
            amount_out: 9,
            total_fee: 1,
            protocol_fee: 1,
            exact_output: false,
        };
        assert_eq!(V::try_from(bad), Err(E::BackendInvariant));
    }
    #[test]
    fn all_error_tags_and_every_internal_error_map_without_sdk_payloads() {
        let values = [
            E::NotFound,
            E::InvalidLimit,
            E::InvalidCursor,
            E::UnsupportedAsset,
            E::Unconfigured,
            E::Arithmetic,
            E::BackendInvariant,
            E::IdenticalAssets,
            E::Unauthorized,
            E::PoolAlreadyExists,
            E::CustodyCollision,
            E::CustodyNotEmpty,
            E::CustodyMismatch,
            E::CustodyNotLive,
            E::DeadlineExpired,
            E::BoundExceeded,
            E::ZeroAmount,
            E::InsufficientInitialLiquidity,
            E::InsufficientLiquidity,
            E::InsufficientLp,
            E::SlippageExceeded,
            E::InvalidConfiguration,
            E::BelowMinimum,
            E::InsufficientBalance,
            E::Frozen,
            E::DepositFailure,
            E::BackendRejected,
            E::Reentrant,
            E::TransactionLimit,
            E::CorruptState,
        ];
        for (tag, value) in values.into_iter().enumerate() {
            golden(value, vec![tag as u8]);
        }
        for tag in 30..=255 {
            assert!(c::decode_exact::<E>(&[tag]).is_err());
        }
        for (fault, tag) in [
            (Fault::InvalidLimit, 1),
            (Fault::InvalidCursor, 2),
            (Fault::Unconfigured, 4),
            (Fault::Liveness, 13),
            (Fault::Bound, 15),
            (Fault::Deposit, 25),
            (Fault::Reentrant, 27),
            (Fault::TransactionLimit, 28),
            (Fault::CorruptState, 29),
        ] {
            assert_eq!(E::from(fault).encode(), [tag]);
        }
        for (error, tag) in [
            (amm::Error::PoolNotFound, 0),
            (amm::Error::ArithmeticOverflow, 5),
            (amm::Error::InvariantViolation, 6),
            (amm::Error::IdenticalAssets, 7),
            (amm::Error::Unauthorized, 8),
            (amm::Error::PoolAlreadyExists, 9),
            (amm::Error::CustodyAccountCollision, 10),
            (amm::Error::CustodyNotEmpty, 11),
            (amm::Error::CustodyBalanceMismatch, 12),
            (amm::Error::DeadlineExpired, 14),
            (amm::Error::ZeroAmount, 16),
            (amm::Error::InsufficientInitialLiquidity, 17),
            (amm::Error::InsufficientLiquidity, 18),
            (amm::Error::InsufficientLpBalance, 19),
            (amm::Error::SlippageExceeded, 20),
            (amm::Error::InvalidConfiguration, 21),
            (amm::Error::FeeRoutingFailed, 26),
        ] {
            assert_eq!(E::from(Fault::Reference(error)).encode(), [tag]);
        }
        for (error, tag) in [
            (AssetError::UnknownAsset, 0),
            (AssetError::UnsupportedAsset, 3),
            (AssetError::ArithmeticOverflow, 5),
            (AssetError::AccountingInvariant, 6),
            (AssetError::BelowMinimum, 22),
            (AssetError::InsufficientBalance, 23),
            (AssetError::Frozen, 24),
            (AssetError::AccountFrozen, 24),
            (AssetError::AlreadyExists, 26),
            (AssetError::UnknownCollection, 26),
            (AssetError::UnknownItem, 26),
            (AssetError::NotOwner, 26),
            (AssetError::NotIssuer, 26),
            (AssetError::NotAdmin, 26),
            (AssetError::NotFreezer, 26),
            (AssetError::OwnershipBoundary, 26),
            (AssetError::MetadataTooLong, 26),
            (AssetError::NativeMutationProhibited, 26),
            (AssetError::BackendRejected, 26),
        ] {
            assert_eq!(E::from(error).encode(), [tag]);
            assert_eq!(E::from(Fault::from(error)).encode(), [tag]);
        }
    }
    #[test]
    fn adopted_parameters_and_nontruncating_custody_preimages_are_fixed() {
        use frame_support::traits::Get;
        assert_eq!(
            <c::Fee as Get<Rate>>::get(),
            Rate {
                numerator: 25,
                denominator: 10_000
            }
        );
        assert_eq!(
            <c::FeeLimit as Get<Rate>>::get(),
            Rate {
                numerator: 100,
                denominator: 10_000
            }
        );
        assert_eq!(<c::LockedLiquidity as Get<u128>>::get(), 1000);
        assert_eq!(
            <c::CreationDeposit as Get<u128>>::get(),
            10 * 10u128.pow(18)
        );
        assert_eq!(<c::MinimumPosition as Get<u128>>::get(), 1);
        assert_eq!(<c::Horizon as Get<u64>>::get(), 14_400);
        assert_eq!(
            (
                <c::MaxPools as Get<u32>>::get(),
                <c::MaxPositions as Get<u32>>::get(),
                <c::MaxProviders as Get<u32>>::get(),
                <c::MaxPage as Get<u32>>::get()
            ),
            (1024, 64, 1024, 64)
        );
        assert_eq!(
            (
                c::PALLET_INDEX,
                c::STORAGE_VERSION,
                c::API_VERSION,
                c::RETAIN_SPEC_TRANSITIONS
            ),
            (22, 1, 1, 2)
        );
        assert_eq!(
            (c::PROTOCOL_FEE.numerator, c::PROTOCOL_FEE.denominator),
            (0, 1)
        );
        for (assets, bytes) in [
            (
                (
                    FungibleAsset::NativeEtkn,
                    FungibleAsset::Registered(0x01020304),
                ),
                vec![0, 1, 4, 3, 2, 1],
            ),
            (
                (FungibleAsset::Registered(1), FungibleAsset::Registered(2)),
                vec![1, 1, 0, 0, 0, 1, 2, 0, 0, 0],
            ),
        ] {
            let id = PoolId::new(assets.0, assets.1).unwrap();
            let mut preimage = [b"modlera/vamm".to_vec(), vec![1], bytes].concat();
            assert!([19, 23].contains(&preimage.len()));
            preimage.resize(32, 0);
            let derived: sp_runtime::AccountId32 =
                c::CheckedCustody.derive_pool_account(&id).unwrap();
            assert_eq!(AsRef::<[u8]>::as_ref(&derived), preimage);
        }
    }
}
