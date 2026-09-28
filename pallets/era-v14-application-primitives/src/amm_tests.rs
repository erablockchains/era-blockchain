use alloc::vec::Vec;

use crate::{
    amm::{
        Amm, AmmConfig, Error, PoolAccountDeriver, PoolCreationPolicy, PoolId, ProtocolFeeRouter,
        Rate,
    },
    assets::{
        AssetMetadata, AssetRoles, FungibleAsset, FungibleInspect, FungibleTransfer,
        InterfaceLimits, ReferenceLedger,
    },
};

type Ledger = ReferenceLedger<u64, u32, u32, u32>;
type Engine = Amm<u64, u32, Ledger, CreationGate, TestDeriver, TestRouter>;

const CREATOR: u64 = 1;
const SECOND_PROVIDER: u64 = 2;
const PARTIAL_PROVIDER: u64 = 3;
const FEE_SINK: u64 = 9_000;

fn registered(id: u32) -> FungibleAsset<u32> {
    FungibleAsset::Registered(id)
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CreationGate {
    permitted: u64,
}

impl PoolCreationPolicy<u64, u32> for CreationGate {
    fn ensure_can_create(&self, who: &u64, _pool: &PoolId<u32>) -> Result<(), Error> {
        if *who == self.permitted {
            Ok(())
        } else {
            Err(Error::Unauthorized)
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TestDeriver;

impl TestDeriver {
    fn asset_code(asset: FungibleAsset<u32>) -> u64 {
        match asset {
            FungibleAsset::NativeEtkn => 1,
            FungibleAsset::Registered(id) => u64::from(id) + 100,
        }
    }
}

impl PoolAccountDeriver<u64, u32> for TestDeriver {
    fn derive_pool_account(&self, pool: &PoolId<u32>) -> Result<u64, Error> {
        let left = Self::asset_code(pool.asset_0);
        let right = Self::asset_code(pool.asset_1);
        left.checked_mul(1_000_000)
            .and_then(|value| value.checked_add(right))
            .and_then(|value| value.checked_add(1_000_000_000))
            .ok_or(Error::ArithmeticOverflow)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TestRouter {
    sink: u64,
    fail: bool,
    routed: Vec<(FungibleAsset<u32>, u128)>,
}

impl ProtocolFeeRouter<u64, u32> for TestRouter {
    fn route<L>(
        &mut self,
        ledger: &mut L,
        pool_account: &u64,
        asset: FungibleAsset<u32>,
        amount: u128,
    ) -> Result<(), Error>
    where
        L: FungibleTransfer<u64, AssetId = u32>,
    {
        if self.fail {
            return Err(Error::FeeRoutingFailed);
        }
        ledger.transfer(asset, pool_account, &self.sink, amount)?;
        self.routed.push((asset, amount));
        Ok(())
    }
}

fn asset_roles() -> AssetRoles<u64> {
    AssetRoles {
        owner: 10,
        issuer: 11,
        admin: 12,
        freezer: 13,
    }
}

fn metadata(symbol: &[u8]) -> AssetMetadata {
    AssetMetadata {
        name: b"Application asset".to_vec(),
        symbol: symbol.to_vec(),
        decimals: 6,
    }
}

fn base_ledger() -> Ledger {
    let mut ledger = Ledger::new(InterfaceLimits {
        max_asset_name_bytes: 32,
        max_asset_symbol_bytes: 8,
        max_collection_metadata_bytes: 32,
        max_item_metadata_bytes: 32,
    });
    ledger
        .create_asset(&10, 1, asset_roles(), metadata(b"ONE"))
        .unwrap();
    ledger
        .create_asset(&10, 2, asset_roles(), metadata(b"TWO"))
        .unwrap();
    for (account, first, second) in [
        (CREATOR, 200_000, 200_000),
        (SECOND_PROVIDER, 20_000, 20_000),
        (PARTIAL_PROVIDER, 1_000, 0),
    ] {
        ledger.mint_asset(&11, 1, &account, first).unwrap();
        ledger.mint_asset(&11, 2, &account, second).unwrap();
    }
    ledger.seed_native_for_reference(&CREATOR, 200_000).unwrap();
    ledger
        .seed_native_for_reference(&SECOND_PROVIDER, 20_000)
        .unwrap();
    ledger
}

fn fixture_config() -> AmmConfig {
    // Test-only values prove configurability; they are not V14 economic defaults.
    AmmConfig::new(Rate::new(3, 1_000).unwrap(), Rate::new(1, 2).unwrap(), 10).unwrap()
}

fn engine_with(ledger: Ledger, router_fails: bool, config: AmmConfig) -> Engine {
    Engine::new(
        config,
        ledger,
        CreationGate { permitted: CREATOR },
        TestDeriver,
        TestRouter {
            sink: FEE_SINK,
            fail: router_fails,
            routed: Vec::new(),
        },
    )
    .unwrap()
}

fn engine() -> Engine {
    engine_with(base_ledger(), false, fixture_config())
}

fn initialized_pool() -> (Engine, PoolId<u32>) {
    let mut amm = engine();
    let pool = amm
        .create_pool(&CREATOR, registered(1), registered(2), 5, 10)
        .unwrap();
    let added = amm
        .add_liquidity(
            &CREATOR,
            registered(1),
            registered(2),
            10_000,
            10_000,
            10_000,
            10_000,
            5,
            10,
        )
        .unwrap();
    assert_eq!(added.lp_amount, 9_990);
    (amm, pool)
}

#[test]
fn pool_identity_is_order_independent_and_creation_is_authorized_and_bounded() {
    assert_eq!(
        PoolId::new(registered(1), registered(2)),
        PoolId::new(registered(2), registered(1))
    );
    assert_eq!(
        PoolId::new(registered(1), registered(1)),
        Err(Error::IdenticalAssets)
    );

    let mut amm = engine();
    let original = amm.clone();
    assert_eq!(
        amm.create_pool(&SECOND_PROVIDER, registered(1), registered(2), 1, 2),
        Err(Error::Unauthorized)
    );
    assert_eq!(amm, original);
    assert_eq!(
        amm.create_pool(&CREATOR, registered(1), registered(2), 3, 2),
        Err(Error::DeadlineExpired)
    );
    assert_eq!(amm, original);

    let pool = amm
        .create_pool(&CREATOR, registered(2), registered(1), 2, 2)
        .unwrap();
    let created = amm.clone();
    assert_eq!(
        amm.create_pool(&CREATOR, registered(1), registered(2), 2, 2),
        Err(Error::PoolAlreadyExists)
    );
    assert_eq!(amm, created);
    assert_eq!(amm.pool(pool).unwrap().reserve_0, 0);
    assert_eq!(amm.verify_invariants(), Ok(()));
}

#[test]
fn contaminated_custody_account_fails_closed() {
    let mut ledger = base_ledger();
    let pool = PoolId::new(registered(1), registered(2)).unwrap();
    let custody = TestDeriver.derive_pool_account(&pool).unwrap();
    ledger.mint_asset(&11, 1, &custody, 1).unwrap();
    let mut amm = engine_with(ledger, false, fixture_config());
    let snapshot = amm.clone();
    assert_eq!(
        amm.create_pool(&CREATOR, registered(1), registered(2), 1, 1),
        Err(Error::CustodyNotEmpty)
    );
    assert_eq!(amm, snapshot);
}

#[test]
fn initial_liquidity_locks_the_configured_minimum_and_overflow_rolls_back() {
    let mut amm = engine();
    let pool = amm
        .create_pool(&CREATOR, registered(1), registered(2), 1, 1)
        .unwrap();
    let created = amm.clone();
    assert_eq!(
        amm.add_liquidity(&CREATOR, registered(1), registered(2), 10, 10, 0, 0, 1, 1,),
        Err(Error::InsufficientInitialLiquidity)
    );
    assert_eq!(amm, created);

    let mut ledger = Ledger::new(InterfaceLimits {
        max_asset_name_bytes: 32,
        max_asset_symbol_bytes: 8,
        max_collection_metadata_bytes: 32,
        max_item_metadata_bytes: 32,
    });
    ledger
        .create_asset(&10, 1, asset_roles(), metadata(b"ONE"))
        .unwrap();
    ledger
        .create_asset(&10, 2, asset_roles(), metadata(b"TWO"))
        .unwrap();
    ledger.mint_asset(&11, 1, &CREATOR, u128::MAX).unwrap();
    ledger.mint_asset(&11, 2, &CREATOR, u128::MAX).unwrap();
    let mut overflow = engine_with(ledger, false, fixture_config());
    overflow
        .create_pool(&CREATOR, registered(1), registered(2), 1, 1)
        .unwrap();
    let before_overflow = overflow.clone();
    assert_eq!(
        overflow.add_liquidity(
            &CREATOR,
            registered(1),
            registered(2),
            u128::MAX,
            u128::MAX,
            0,
            0,
            1,
            1,
        ),
        Err(Error::ArithmeticOverflow)
    );
    assert_eq!(overflow, before_overflow);
    assert_eq!(amm.pool(pool).unwrap().locked_liquidity, 0);
}

#[test]
fn proportional_addition_and_removal_preserve_lp_accounting() {
    let (mut amm, pool) = initialized_pool();
    let added = amm
        .add_liquidity(
            &SECOND_PROVIDER,
            registered(2),
            registered(1),
            2_500,
            2_000,
            1_900,
            1_900,
            7,
            7,
        )
        .unwrap();
    assert_eq!((added.amount_0, added.amount_1), (2_000, 2_000));
    assert_eq!(added.lp_amount, 2_000);
    assert_eq!(amm.lp_balance(pool, &SECOND_PROVIDER), 2_000);

    let before_first = amm.ledger().balance(registered(1), &SECOND_PROVIDER);
    let before_second = amm.ledger().balance(registered(2), &SECOND_PROVIDER);
    let removed = amm
        .remove_liquidity(
            &SECOND_PROVIDER,
            registered(2),
            registered(1),
            1_000,
            900,
            900,
            &SECOND_PROVIDER,
            8,
            8,
        )
        .unwrap();
    assert_eq!(removed.lp_amount, 1_000);
    assert_eq!(amm.lp_balance(pool, &SECOND_PROVIDER), 1_000);
    assert_eq!(
        amm.ledger().balance(registered(1), &SECOND_PROVIDER),
        before_first + removed.amount_0
    );
    assert_eq!(
        amm.ledger().balance(registered(2), &SECOND_PROVIDER),
        before_second + removed.amount_1
    );
    assert_eq!(amm.verify_invariants(), Ok(()));
}

#[test]
fn exact_input_swap_preserves_product_and_routes_the_configured_fee_share() {
    let (mut amm, pool) = initialized_pool();
    let quote = amm
        .quote_exact_input(registered(1), registered(2), 1_000)
        .unwrap();
    let state_before = amm.pool(pool).unwrap().clone();
    let product_before = state_before.reserve_0 * state_before.reserve_1;
    let sink_before = amm.ledger().balance(registered(1), &FEE_SINK);

    let outcome = amm
        .swap_exact_input(
            &CREATOR,
            registered(1),
            registered(2),
            1_000,
            quote,
            &CREATOR,
            9,
            9,
        )
        .unwrap();

    assert_eq!(outcome.amount_out, quote);
    assert_eq!(outcome.total_fee, 3);
    assert_eq!(outcome.protocol_fee, 1);
    assert_eq!(
        amm.ledger().balance(registered(1), &FEE_SINK),
        sink_before + 1
    );
    assert_eq!(amm.fee_router().routed, vec![(registered(1), 1)]);
    let state_after = amm.pool(pool).unwrap();
    assert!(state_after.reserve_0 * state_after.reserve_1 >= product_before);
    assert_eq!(amm.verify_invariants(), Ok(()));
}

#[test]
fn exact_output_quote_is_enforced_as_a_maximum_input() {
    let (mut amm, _) = initialized_pool();
    let required = amm
        .quote_exact_output(registered(1), registered(2), 500)
        .unwrap();
    let snapshot = amm.clone();
    assert_eq!(
        amm.swap_exact_output(
            &CREATOR,
            registered(1),
            registered(2),
            500,
            required - 1,
            &SECOND_PROVIDER,
            9,
            9,
        ),
        Err(Error::SlippageExceeded)
    );
    assert_eq!(amm, snapshot);

    let outcome = amm
        .swap_exact_output(
            &CREATOR,
            registered(1),
            registered(2),
            500,
            required,
            &SECOND_PROVIDER,
            9,
            9,
        )
        .unwrap();
    assert_eq!(outcome.amount_in, required);
    assert_eq!(outcome.amount_out, 500);
    assert_eq!(amm.verify_invariants(), Ok(()));
}

#[test]
fn slippage_deadline_and_insufficient_lp_failures_are_atomic() {
    let (mut amm, pool) = initialized_pool();
    let quote = amm
        .quote_exact_input(registered(1), registered(2), 100)
        .unwrap();
    let snapshot = amm.clone();
    assert_eq!(
        amm.swap_exact_input(
            &CREATOR,
            registered(1),
            registered(2),
            100,
            quote + 1,
            &CREATOR,
            5,
            5,
        ),
        Err(Error::SlippageExceeded)
    );
    assert_eq!(amm, snapshot);
    assert_eq!(
        amm.swap_exact_input(
            &CREATOR,
            registered(1),
            registered(2),
            100,
            0,
            &CREATOR,
            6,
            5,
        ),
        Err(Error::DeadlineExpired)
    );
    assert_eq!(amm, snapshot);
    assert_eq!(
        amm.remove_liquidity(
            &CREATOR,
            registered(1),
            registered(2),
            amm.lp_balance(pool, &CREATOR) + 1,
            0,
            0,
            &CREATOR,
            5,
            5,
        ),
        Err(Error::InsufficientLpBalance)
    );
    assert_eq!(amm, snapshot);
}

#[test]
fn failed_second_asset_transfer_rolls_back_the_first_transfer_and_lp_state() {
    let (mut amm, pool) = initialized_pool();
    let snapshot = amm.clone();
    assert_eq!(
        amm.add_liquidity(
            &PARTIAL_PROVIDER,
            registered(1),
            registered(2),
            100,
            100,
            0,
            0,
            5,
            5,
        ),
        Err(Error::Asset(crate::assets::Error::InsufficientBalance))
    );
    assert_eq!(amm, snapshot);
    assert_eq!(amm.lp_balance(pool, &PARTIAL_PROVIDER), 0);
}

#[test]
fn failed_fee_hook_rolls_back_transfers_reserves_events_and_fees() {
    let mut amm = engine_with(base_ledger(), true, fixture_config());
    let pool = amm
        .create_pool(&CREATOR, registered(1), registered(2), 1, 1)
        .unwrap();
    amm.add_liquidity(
        &CREATOR,
        registered(1),
        registered(2),
        10_000,
        10_000,
        0,
        0,
        1,
        1,
    )
    .unwrap();
    let snapshot = amm.clone();
    assert_eq!(
        amm.swap_exact_input(
            &CREATOR,
            registered(1),
            registered(2),
            1_000,
            0,
            &CREATOR,
            1,
            1,
        ),
        Err(Error::FeeRoutingFailed)
    );
    assert_eq!(amm, snapshot);
    assert_eq!(amm.pool(pool), snapshot.pool(pool));
}

#[test]
fn native_etkn_can_be_transferred_through_a_pool_but_never_issued_by_the_amm() {
    let mut amm = engine();
    let native_issuance = amm
        .ledger()
        .total_issuance(FungibleAsset::NativeEtkn)
        .unwrap();
    let registered_issuance = amm.ledger().total_issuance(registered(1)).unwrap();
    let pool = amm
        .create_pool(&CREATOR, FungibleAsset::NativeEtkn, registered(1), 1, 1)
        .unwrap();
    amm.add_liquidity(
        &CREATOR,
        FungibleAsset::NativeEtkn,
        registered(1),
        10_000,
        10_000,
        0,
        0,
        1,
        1,
    )
    .unwrap();
    amm.swap_exact_input(
        &SECOND_PROVIDER,
        FungibleAsset::NativeEtkn,
        registered(1),
        1_000,
        0,
        &SECOND_PROVIDER,
        1,
        1,
    )
    .unwrap();
    let burnable_lp = amm.lp_balance(pool, &CREATOR) / 2;
    amm.remove_liquidity(
        &CREATOR,
        FungibleAsset::NativeEtkn,
        registered(1),
        burnable_lp,
        0,
        0,
        &CREATOR,
        1,
        1,
    )
    .unwrap();

    assert_eq!(
        amm.ledger()
            .total_issuance(FungibleAsset::NativeEtkn)
            .unwrap(),
        native_issuance
    );
    assert_eq!(
        amm.ledger().total_issuance(registered(1)).unwrap(),
        registered_issuance
    );
    assert_eq!(amm.ledger().verify_accounting(), Ok(()));
    assert_eq!(amm.verify_invariants(), Ok(()));
}

#[test]
fn configuration_has_no_implicit_or_invalid_fee_policy() {
    assert_eq!(Rate::new(1, 0), Err(Error::InvalidConfiguration));
    assert_eq!(Rate::new(2, 1), Err(Error::InvalidConfiguration));
    assert_eq!(
        AmmConfig::new(Rate::new(1, 1).unwrap(), Rate::new(0, 1).unwrap(), 0),
        Err(Error::InvalidConfiguration)
    );
}
