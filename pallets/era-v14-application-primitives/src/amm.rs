//! Bounded constant-product AMM logic built only on the frozen V14 asset interface.
//!
//! The engine selects no fee, minimum liquidity, privileged origin, custody account, or protocol
//! destination. Callers must supply each through validated configuration and policy hooks. Every
//! mutation is applied to a clone and committed only after all ledger transfers and hooks succeed.

use alloc::{
    collections::{BTreeMap, BTreeSet},
    vec::Vec,
};

use crate::assets::{Balance, Error as AssetError, FungibleAsset, FungibleTransfer};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rate {
    pub numerator: u128,
    pub denominator: u128,
}

impl Rate {
    pub fn new(numerator: u128, denominator: u128) -> Result<Self, Error> {
        if denominator == 0 || numerator > denominator {
            return Err(Error::InvalidConfiguration);
        }
        Ok(Self {
            numerator,
            denominator,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AmmConfig {
    pub pool_fee: Rate,
    pub protocol_fee_share: Rate,
    pub minimum_liquidity: Balance,
}

impl AmmConfig {
    pub fn new(
        pool_fee: Rate,
        protocol_fee_share: Rate,
        minimum_liquidity: Balance,
    ) -> Result<Self, Error> {
        let config = Self {
            pool_fee,
            protocol_fee_share,
            minimum_liquidity,
        };
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<(), Error> {
        if self.pool_fee.denominator == 0
            || self.pool_fee.numerator >= self.pool_fee.denominator
            || self.protocol_fee_share.denominator == 0
            || self.protocol_fee_share.numerator > self.protocol_fee_share.denominator
        {
            return Err(Error::InvalidConfiguration);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PoolId<AssetId> {
    pub asset_0: FungibleAsset<AssetId>,
    pub asset_1: FungibleAsset<AssetId>,
}

impl<AssetId: Copy + Ord> PoolId<AssetId> {
    pub fn new(
        asset_a: FungibleAsset<AssetId>,
        asset_b: FungibleAsset<AssetId>,
    ) -> Result<Self, Error> {
        if asset_a == asset_b {
            return Err(Error::IdenticalAssets);
        }
        let (asset_0, asset_1) = if asset_a < asset_b {
            (asset_a, asset_b)
        } else {
            (asset_b, asset_a)
        };
        Ok(Self { asset_0, asset_1 })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PoolState<Account, AssetId> {
    pub id: PoolId<AssetId>,
    pub custody_account: Account,
    pub reserve_0: Balance,
    pub reserve_1: Balance,
    pub total_lp: Balance,
    pub locked_liquidity: Balance,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiquidityOutcome<AssetId> {
    pub pool: PoolId<AssetId>,
    pub amount_0: Balance,
    pub amount_1: Balance,
    pub lp_amount: Balance,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SwapOutcome<AssetId> {
    pub pool: PoolId<AssetId>,
    pub asset_in: FungibleAsset<AssetId>,
    pub asset_out: FungibleAsset<AssetId>,
    pub amount_in: Balance,
    pub amount_out: Balance,
    pub total_fee: Balance,
    pub protocol_fee: Balance,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event<Account, AssetId> {
    PoolCreated {
        pool: PoolId<AssetId>,
        creator: Account,
        custody_account: Account,
    },
    LiquidityAdded {
        pool: PoolId<AssetId>,
        provider: Account,
        amount_0: Balance,
        amount_1: Balance,
        lp_minted: Balance,
    },
    LiquidityRemoved {
        pool: PoolId<AssetId>,
        provider: Account,
        recipient: Account,
        amount_0: Balance,
        amount_1: Balance,
        lp_burned: Balance,
    },
    SwapExecuted {
        pool: PoolId<AssetId>,
        trader: Account,
        recipient: Account,
        asset_in: FungibleAsset<AssetId>,
        asset_out: FungibleAsset<AssetId>,
        amount_in: Balance,
        amount_out: Balance,
        total_fee: Balance,
        protocol_fee: Balance,
        exact_output: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Asset(AssetError),
    InvalidConfiguration,
    IdenticalAssets,
    Unauthorized,
    PoolAlreadyExists,
    PoolNotFound,
    CustodyAccountCollision,
    CustodyNotEmpty,
    CustodyBalanceMismatch,
    DeadlineExpired,
    ZeroAmount,
    InsufficientInitialLiquidity,
    InsufficientLiquidity,
    InsufficientLpBalance,
    SlippageExceeded,
    ArithmeticOverflow,
    InvariantViolation,
    FeeRoutingFailed,
}

impl From<AssetError> for Error {
    fn from(error: AssetError) -> Self {
        Self::Asset(error)
    }
}

pub trait PoolCreationPolicy<Account, AssetId> {
    fn ensure_can_create(&self, who: &Account, pool: &PoolId<AssetId>) -> Result<(), Error>;
}

pub trait PoolAccountDeriver<Account, AssetId> {
    fn derive_pool_account(&self, pool: &PoolId<AssetId>) -> Result<Account, Error>;
}

pub trait ProtocolFeeRouter<Account, AssetId> {
    fn route<L>(
        &mut self,
        ledger: &mut L,
        pool_account: &Account,
        asset: FungibleAsset<AssetId>,
        amount: Balance,
    ) -> Result<(), Error>
    where
        L: FungibleTransfer<Account, AssetId = AssetId>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Amm<Account, AssetId, Ledger, CreationPolicy, AccountDeriver, FeeRouter> {
    config: AmmConfig,
    ledger: Ledger,
    creation_policy: CreationPolicy,
    account_deriver: AccountDeriver,
    fee_router: FeeRouter,
    pools: BTreeMap<PoolId<AssetId>, PoolState<Account, AssetId>>,
    lp_balances: BTreeMap<(PoolId<AssetId>, Account), Balance>,
    events: Vec<Event<Account, AssetId>>,
}

impl<Account, AssetId, Ledger, CreationPolicy, AccountDeriver, FeeRouter>
    Amm<Account, AssetId, Ledger, CreationPolicy, AccountDeriver, FeeRouter>
where
    Account: Clone + Ord,
    AssetId: Copy + Ord,
    Ledger: FungibleTransfer<Account, AssetId = AssetId> + Clone,
    CreationPolicy: PoolCreationPolicy<Account, AssetId> + Clone,
    AccountDeriver: PoolAccountDeriver<Account, AssetId> + Clone,
    FeeRouter: ProtocolFeeRouter<Account, AssetId> + Clone,
{
    pub fn new(
        config: AmmConfig,
        ledger: Ledger,
        creation_policy: CreationPolicy,
        account_deriver: AccountDeriver,
        fee_router: FeeRouter,
    ) -> Result<Self, Error> {
        config.validate()?;
        Ok(Self {
            config,
            ledger,
            creation_policy,
            account_deriver,
            fee_router,
            pools: BTreeMap::new(),
            lp_balances: BTreeMap::new(),
            events: Vec::new(),
        })
    }

    pub fn config(&self) -> AmmConfig {
        self.config
    }

    pub fn ledger(&self) -> &Ledger {
        &self.ledger
    }

    pub fn fee_router(&self) -> &FeeRouter {
        &self.fee_router
    }

    pub fn events(&self) -> &[Event<Account, AssetId>] {
        &self.events
    }

    pub fn pool(&self, pool: PoolId<AssetId>) -> Option<&PoolState<Account, AssetId>> {
        self.pools.get(&pool)
    }

    pub fn lp_balance(&self, pool: PoolId<AssetId>, account: &Account) -> Balance {
        self.lp_balances
            .get(&(pool, account.clone()))
            .copied()
            .unwrap_or(0)
    }

    pub fn create_pool(
        &mut self,
        who: &Account,
        asset_a: FungibleAsset<AssetId>,
        asset_b: FungibleAsset<AssetId>,
        now: u64,
        deadline: u64,
    ) -> Result<PoolId<AssetId>, Error> {
        self.transactional(|next| next.create_pool_inner(who, asset_a, asset_b, now, deadline))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn add_liquidity(
        &mut self,
        who: &Account,
        asset_a: FungibleAsset<AssetId>,
        asset_b: FungibleAsset<AssetId>,
        amount_a_desired: Balance,
        amount_b_desired: Balance,
        amount_a_min: Balance,
        amount_b_min: Balance,
        now: u64,
        deadline: u64,
    ) -> Result<LiquidityOutcome<AssetId>, Error> {
        self.transactional(|next| {
            next.add_liquidity_inner(
                who,
                asset_a,
                asset_b,
                amount_a_desired,
                amount_b_desired,
                amount_a_min,
                amount_b_min,
                now,
                deadline,
            )
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn remove_liquidity(
        &mut self,
        who: &Account,
        asset_a: FungibleAsset<AssetId>,
        asset_b: FungibleAsset<AssetId>,
        lp_amount: Balance,
        amount_a_min: Balance,
        amount_b_min: Balance,
        recipient: &Account,
        now: u64,
        deadline: u64,
    ) -> Result<LiquidityOutcome<AssetId>, Error> {
        self.transactional(|next| {
            next.remove_liquidity_inner(
                who,
                asset_a,
                asset_b,
                lp_amount,
                amount_a_min,
                amount_b_min,
                recipient,
                now,
                deadline,
            )
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn swap_exact_input(
        &mut self,
        who: &Account,
        asset_in: FungibleAsset<AssetId>,
        asset_out: FungibleAsset<AssetId>,
        amount_in: Balance,
        minimum_amount_out: Balance,
        recipient: &Account,
        now: u64,
        deadline: u64,
    ) -> Result<SwapOutcome<AssetId>, Error> {
        self.transactional(|next| {
            next.swap_inner(
                who,
                asset_in,
                asset_out,
                amount_in,
                minimum_amount_out,
                recipient,
                now,
                deadline,
                false,
            )
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn swap_exact_output(
        &mut self,
        who: &Account,
        asset_in: FungibleAsset<AssetId>,
        asset_out: FungibleAsset<AssetId>,
        amount_out: Balance,
        maximum_amount_in: Balance,
        recipient: &Account,
        now: u64,
        deadline: u64,
    ) -> Result<SwapOutcome<AssetId>, Error> {
        self.transactional(|next| {
            next.swap_inner(
                who,
                asset_in,
                asset_out,
                amount_out,
                maximum_amount_in,
                recipient,
                now,
                deadline,
                true,
            )
        })
    }

    pub fn quote_exact_input(
        &self,
        asset_in: FungibleAsset<AssetId>,
        asset_out: FungibleAsset<AssetId>,
        amount_in: Balance,
    ) -> Result<Balance, Error> {
        let pool_id = PoolId::new(asset_in, asset_out)?;
        let pool = self.pools.get(&pool_id).ok_or(Error::PoolNotFound)?;
        let (reserve_in, reserve_out) = oriented_reserves(pool, asset_in)?;
        let effective = self.amount_after_fee(amount_in)?;
        quote_output(reserve_in, reserve_out, effective)
    }

    pub fn quote_exact_output(
        &self,
        asset_in: FungibleAsset<AssetId>,
        asset_out: FungibleAsset<AssetId>,
        amount_out: Balance,
    ) -> Result<Balance, Error> {
        let pool_id = PoolId::new(asset_in, asset_out)?;
        let pool = self.pools.get(&pool_id).ok_or(Error::PoolNotFound)?;
        let (reserve_in, reserve_out) = oriented_reserves(pool, asset_in)?;
        self.input_for_exact_output(reserve_in, reserve_out, amount_out)
    }

    pub fn verify_invariants(&self) -> Result<(), Error> {
        self.config.validate()?;
        let mut custody_accounts = BTreeSet::new();
        for (pool_id, pool) in &self.pools {
            if pool_id != &pool.id || pool_id.asset_0 >= pool_id.asset_1 {
                return Err(Error::InvariantViolation);
            }
            if !custody_accounts.insert(pool.custody_account.clone()) {
                return Err(Error::CustodyAccountCollision);
            }
            self.ensure_custody_matches(pool)?;
            let mut lp_sum = pool.locked_liquidity;
            for ((balance_pool, _), balance) in &self.lp_balances {
                if balance_pool == pool_id {
                    lp_sum = lp_sum
                        .checked_add(*balance)
                        .ok_or(Error::ArithmeticOverflow)?;
                }
            }
            if lp_sum != pool.total_lp {
                return Err(Error::InvariantViolation);
            }
            if pool.total_lp == 0 {
                if pool.reserve_0 != 0 || pool.reserve_1 != 0 || pool.locked_liquidity != 0 {
                    return Err(Error::InvariantViolation);
                }
            } else if pool.reserve_0 == 0 || pool.reserve_1 == 0 {
                return Err(Error::InvariantViolation);
            }
            checked_product(pool.reserve_0, pool.reserve_1)?;
        }
        Ok(())
    }

    fn transactional<Output>(
        &mut self,
        operation: impl FnOnce(&mut Self) -> Result<Output, Error>,
    ) -> Result<Output, Error> {
        let mut next = self.clone();
        let output = operation(&mut next)?;
        next.verify_invariants()?;
        *self = next;
        Ok(output)
    }

    fn create_pool_inner(
        &mut self,
        who: &Account,
        asset_a: FungibleAsset<AssetId>,
        asset_b: FungibleAsset<AssetId>,
        now: u64,
        deadline: u64,
    ) -> Result<PoolId<AssetId>, Error> {
        ensure_deadline(now, deadline)?;
        let pool = PoolId::new(asset_a, asset_b)?;
        if self.pools.contains_key(&pool) {
            return Err(Error::PoolAlreadyExists);
        }
        self.creation_policy.ensure_can_create(who, &pool)?;
        let custody_account = self.account_deriver.derive_pool_account(&pool)?;
        if self
            .pools
            .values()
            .any(|existing| existing.custody_account == custody_account)
        {
            return Err(Error::CustodyAccountCollision);
        }
        if self.ledger.balance(pool.asset_0, &custody_account) != 0
            || self.ledger.balance(pool.asset_1, &custody_account) != 0
        {
            return Err(Error::CustodyNotEmpty);
        }
        self.pools.insert(
            pool,
            PoolState {
                id: pool,
                custody_account: custody_account.clone(),
                reserve_0: 0,
                reserve_1: 0,
                total_lp: 0,
                locked_liquidity: 0,
            },
        );
        self.events.push(Event::PoolCreated {
            pool,
            creator: who.clone(),
            custody_account,
        });
        Ok(pool)
    }

    #[allow(clippy::too_many_arguments)]
    fn add_liquidity_inner(
        &mut self,
        who: &Account,
        asset_a: FungibleAsset<AssetId>,
        asset_b: FungibleAsset<AssetId>,
        amount_a_desired: Balance,
        amount_b_desired: Balance,
        amount_a_min: Balance,
        amount_b_min: Balance,
        now: u64,
        deadline: u64,
    ) -> Result<LiquidityOutcome<AssetId>, Error> {
        ensure_deadline(now, deadline)?;
        if amount_a_desired == 0 || amount_b_desired == 0 {
            return Err(Error::ZeroAmount);
        }
        let pool_id = PoolId::new(asset_a, asset_b)?;
        let mut pool = self
            .pools
            .get(&pool_id)
            .cloned()
            .ok_or(Error::PoolNotFound)?;
        self.ensure_custody_matches(&pool)?;
        let (desired_0, desired_1) =
            ordered_amounts(pool_id, asset_a, amount_a_desired, amount_b_desired)?;
        let (minimum_0, minimum_1) = ordered_amounts(pool_id, asset_a, amount_a_min, amount_b_min)?;

        let (amount_0, amount_1, lp_minted, next_total, next_locked) = if pool.total_lp == 0 {
            let product = checked_product(desired_0, desired_1)?;
            let root = integer_sqrt(product);
            if root <= self.config.minimum_liquidity {
                return Err(Error::InsufficientInitialLiquidity);
            }
            (
                desired_0,
                desired_1,
                root - self.config.minimum_liquidity,
                root,
                self.config.minimum_liquidity,
            )
        } else {
            let amount_1_optimal = mul_div_floor(desired_0, pool.reserve_1, pool.reserve_0)?;
            let (amount_0, amount_1) = if amount_1_optimal <= desired_1 {
                (desired_0, amount_1_optimal)
            } else {
                (
                    mul_div_floor(desired_1, pool.reserve_0, pool.reserve_1)?,
                    desired_1,
                )
            };
            let lp_0 = mul_div_floor(amount_0, pool.total_lp, pool.reserve_0)?;
            let lp_1 = mul_div_floor(amount_1, pool.total_lp, pool.reserve_1)?;
            let lp = core::cmp::min(lp_0, lp_1);
            if lp == 0 {
                return Err(Error::InsufficientLiquidity);
            }
            (
                amount_0,
                amount_1,
                lp,
                pool.total_lp
                    .checked_add(lp)
                    .ok_or(Error::ArithmeticOverflow)?,
                pool.locked_liquidity,
            )
        };
        if amount_0 < minimum_0 || amount_1 < minimum_1 {
            return Err(Error::SlippageExceeded);
        }
        let next_reserve_0 = pool
            .reserve_0
            .checked_add(amount_0)
            .ok_or(Error::ArithmeticOverflow)?;
        let next_reserve_1 = pool
            .reserve_1
            .checked_add(amount_1)
            .ok_or(Error::ArithmeticOverflow)?;
        let previous_lp = self.lp_balance(pool_id, who);
        let next_lp = previous_lp
            .checked_add(lp_minted)
            .ok_or(Error::ArithmeticOverflow)?;

        self.ledger
            .transfer(pool.id.asset_0, who, &pool.custody_account, amount_0)?;
        self.ledger
            .transfer(pool.id.asset_1, who, &pool.custody_account, amount_1)?;
        pool.reserve_0 = next_reserve_0;
        pool.reserve_1 = next_reserve_1;
        pool.total_lp = next_total;
        pool.locked_liquidity = next_locked;
        self.ensure_custody_matches(&pool)?;
        self.lp_balances.insert((pool_id, who.clone()), next_lp);
        self.pools.insert(pool_id, pool);
        self.events.push(Event::LiquidityAdded {
            pool: pool_id,
            provider: who.clone(),
            amount_0,
            amount_1,
            lp_minted,
        });
        Ok(LiquidityOutcome {
            pool: pool_id,
            amount_0,
            amount_1,
            lp_amount: lp_minted,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn remove_liquidity_inner(
        &mut self,
        who: &Account,
        asset_a: FungibleAsset<AssetId>,
        asset_b: FungibleAsset<AssetId>,
        lp_amount: Balance,
        amount_a_min: Balance,
        amount_b_min: Balance,
        recipient: &Account,
        now: u64,
        deadline: u64,
    ) -> Result<LiquidityOutcome<AssetId>, Error> {
        ensure_deadline(now, deadline)?;
        if lp_amount == 0 {
            return Err(Error::ZeroAmount);
        }
        let pool_id = PoolId::new(asset_a, asset_b)?;
        let mut pool = self
            .pools
            .get(&pool_id)
            .cloned()
            .ok_or(Error::PoolNotFound)?;
        self.ensure_custody_matches(&pool)?;
        if pool.total_lp == 0 {
            return Err(Error::InsufficientLiquidity);
        }
        let provider_lp = self.lp_balance(pool_id, who);
        let next_provider_lp = provider_lp
            .checked_sub(lp_amount)
            .ok_or(Error::InsufficientLpBalance)?;
        let amount_0 = mul_div_floor(lp_amount, pool.reserve_0, pool.total_lp)?;
        let amount_1 = mul_div_floor(lp_amount, pool.reserve_1, pool.total_lp)?;
        if amount_0 == 0 || amount_1 == 0 {
            return Err(Error::InsufficientLiquidity);
        }
        let (minimum_0, minimum_1) = ordered_amounts(pool_id, asset_a, amount_a_min, amount_b_min)?;
        if amount_0 < minimum_0 || amount_1 < minimum_1 {
            return Err(Error::SlippageExceeded);
        }
        let next_total = pool
            .total_lp
            .checked_sub(lp_amount)
            .ok_or(Error::InvariantViolation)?;
        if next_total < pool.locked_liquidity {
            return Err(Error::InvariantViolation);
        }
        pool.reserve_0 = pool
            .reserve_0
            .checked_sub(amount_0)
            .ok_or(Error::InvariantViolation)?;
        pool.reserve_1 = pool
            .reserve_1
            .checked_sub(amount_1)
            .ok_or(Error::InvariantViolation)?;
        pool.total_lp = next_total;

        self.ledger
            .transfer(pool.id.asset_0, &pool.custody_account, recipient, amount_0)?;
        self.ledger
            .transfer(pool.id.asset_1, &pool.custody_account, recipient, amount_1)?;
        self.ensure_custody_matches(&pool)?;
        if next_provider_lp == 0 {
            self.lp_balances.remove(&(pool_id, who.clone()));
        } else {
            self.lp_balances
                .insert((pool_id, who.clone()), next_provider_lp);
        }
        self.pools.insert(pool_id, pool);
        self.events.push(Event::LiquidityRemoved {
            pool: pool_id,
            provider: who.clone(),
            recipient: recipient.clone(),
            amount_0,
            amount_1,
            lp_burned: lp_amount,
        });
        Ok(LiquidityOutcome {
            pool: pool_id,
            amount_0,
            amount_1,
            lp_amount,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn swap_inner(
        &mut self,
        who: &Account,
        asset_in: FungibleAsset<AssetId>,
        asset_out: FungibleAsset<AssetId>,
        requested_amount: Balance,
        limit: Balance,
        recipient: &Account,
        now: u64,
        deadline: u64,
        exact_output: bool,
    ) -> Result<SwapOutcome<AssetId>, Error> {
        ensure_deadline(now, deadline)?;
        if requested_amount == 0 {
            return Err(Error::ZeroAmount);
        }
        let pool_id = PoolId::new(asset_in, asset_out)?;
        let mut pool = self
            .pools
            .get(&pool_id)
            .cloned()
            .ok_or(Error::PoolNotFound)?;
        self.ensure_custody_matches(&pool)?;
        let input_is_zero = asset_in == pool.id.asset_0;
        let (reserve_in, reserve_out) = oriented_reserves(&pool, asset_in)?;
        if reserve_in == 0 || reserve_out == 0 {
            return Err(Error::InsufficientLiquidity);
        }
        let (amount_in, amount_out) = if exact_output {
            let amount_out = requested_amount;
            let amount_in = self.input_for_exact_output(reserve_in, reserve_out, amount_out)?;
            if amount_in > limit {
                return Err(Error::SlippageExceeded);
            }
            (amount_in, amount_out)
        } else {
            let amount_in = requested_amount;
            let effective = self.amount_after_fee(amount_in)?;
            let amount_out = quote_output(reserve_in, reserve_out, effective)?;
            if amount_out < limit {
                return Err(Error::SlippageExceeded);
            }
            (amount_in, amount_out)
        };
        if amount_out == 0 || amount_out >= reserve_out {
            return Err(Error::InsufficientLiquidity);
        }
        let effective_input = self.amount_after_fee(amount_in)?;
        let total_fee = amount_in
            .checked_sub(effective_input)
            .ok_or(Error::InvariantViolation)?;
        let protocol_fee = mul_div_floor(
            total_fee,
            self.config.protocol_fee_share.numerator,
            self.config.protocol_fee_share.denominator,
        )?;
        let retained_input = amount_in
            .checked_sub(protocol_fee)
            .ok_or(Error::InvariantViolation)?;
        let next_reserve_in = reserve_in
            .checked_add(retained_input)
            .ok_or(Error::ArithmeticOverflow)?;
        let next_reserve_out = reserve_out
            .checked_sub(amount_out)
            .ok_or(Error::InvariantViolation)?;
        let old_product = checked_product(reserve_in, reserve_out)?;
        let new_product = checked_product(next_reserve_in, next_reserve_out)?;
        if new_product < old_product {
            return Err(Error::InvariantViolation);
        }

        self.ledger
            .transfer(asset_in, who, &pool.custody_account, amount_in)?;
        self.ledger
            .transfer(asset_out, &pool.custody_account, recipient, amount_out)?;
        if protocol_fee != 0 {
            self.fee_router.route(
                &mut self.ledger,
                &pool.custody_account,
                asset_in,
                protocol_fee,
            )?;
        }
        if input_is_zero {
            pool.reserve_0 = next_reserve_in;
            pool.reserve_1 = next_reserve_out;
        } else {
            pool.reserve_1 = next_reserve_in;
            pool.reserve_0 = next_reserve_out;
        }
        self.ensure_custody_matches(&pool)?;
        self.pools.insert(pool_id, pool);
        self.events.push(Event::SwapExecuted {
            pool: pool_id,
            trader: who.clone(),
            recipient: recipient.clone(),
            asset_in,
            asset_out,
            amount_in,
            amount_out,
            total_fee,
            protocol_fee,
            exact_output,
        });
        Ok(SwapOutcome {
            pool: pool_id,
            asset_in,
            asset_out,
            amount_in,
            amount_out,
            total_fee,
            protocol_fee,
        })
    }

    fn amount_after_fee(&self, amount_in: Balance) -> Result<Balance, Error> {
        let retained_numerator = self
            .config
            .pool_fee
            .denominator
            .checked_sub(self.config.pool_fee.numerator)
            .ok_or(Error::InvalidConfiguration)?;
        let amount = mul_div_floor(
            amount_in,
            retained_numerator,
            self.config.pool_fee.denominator,
        )?;
        if amount == 0 {
            return Err(Error::InsufficientLiquidity);
        }
        Ok(amount)
    }

    fn input_for_exact_output(
        &self,
        reserve_in: Balance,
        reserve_out: Balance,
        amount_out: Balance,
    ) -> Result<Balance, Error> {
        if amount_out == 0 || amount_out >= reserve_out {
            return Err(Error::InsufficientLiquidity);
        }
        let remaining_out = reserve_out
            .checked_sub(amount_out)
            .ok_or(Error::InsufficientLiquidity)?;
        let effective_input = mul_div_ceil(reserve_in, amount_out, remaining_out)?;
        let retained_numerator = self
            .config
            .pool_fee
            .denominator
            .checked_sub(self.config.pool_fee.numerator)
            .ok_or(Error::InvalidConfiguration)?;
        let gross = mul_div_ceil(
            effective_input,
            self.config.pool_fee.denominator,
            retained_numerator,
        )?;
        if gross == 0 {
            return Err(Error::InsufficientLiquidity);
        }
        Ok(gross)
    }

    fn ensure_custody_matches(&self, pool: &PoolState<Account, AssetId>) -> Result<(), Error> {
        if self.ledger.balance(pool.id.asset_0, &pool.custody_account) != pool.reserve_0
            || self.ledger.balance(pool.id.asset_1, &pool.custody_account) != pool.reserve_1
        {
            return Err(Error::CustodyBalanceMismatch);
        }
        Ok(())
    }
}

fn ensure_deadline(now: u64, deadline: u64) -> Result<(), Error> {
    if now > deadline {
        return Err(Error::DeadlineExpired);
    }
    Ok(())
}

fn ordered_amounts<AssetId: Copy + Ord>(
    pool: PoolId<AssetId>,
    asset_a: FungibleAsset<AssetId>,
    amount_a: Balance,
    amount_b: Balance,
) -> Result<(Balance, Balance), Error> {
    if asset_a == pool.asset_0 {
        Ok((amount_a, amount_b))
    } else if asset_a == pool.asset_1 {
        Ok((amount_b, amount_a))
    } else {
        Err(Error::InvariantViolation)
    }
}

fn oriented_reserves<Account, AssetId: Copy + Ord>(
    pool: &PoolState<Account, AssetId>,
    asset_in: FungibleAsset<AssetId>,
) -> Result<(Balance, Balance), Error> {
    if asset_in == pool.id.asset_0 {
        Ok((pool.reserve_0, pool.reserve_1))
    } else if asset_in == pool.id.asset_1 {
        Ok((pool.reserve_1, pool.reserve_0))
    } else {
        Err(Error::InvariantViolation)
    }
}

/// Checked reference arithmetic; supplying a policy remains the caller's responsibility.
pub fn quote_output(
    reserve_in: Balance,
    reserve_out: Balance,
    effective_input: Balance,
) -> Result<Balance, Error> {
    if reserve_in == 0 || reserve_out == 0 || effective_input == 0 {
        return Err(Error::InsufficientLiquidity);
    }
    let denominator = reserve_in
        .checked_add(effective_input)
        .ok_or(Error::ArithmeticOverflow)?;
    let output = mul_div_floor(effective_input, reserve_out, denominator)?;
    if output == 0 {
        return Err(Error::InsufficientLiquidity);
    }
    Ok(output)
}

/// Checked reference arithmetic; supplying a policy remains the caller's responsibility.
pub fn checked_product(left: Balance, right: Balance) -> Result<Balance, Error> {
    left.checked_mul(right).ok_or(Error::ArithmeticOverflow)
}

/// Checked reference arithmetic; supplying a policy remains the caller's responsibility.
pub fn mul_div_floor(
    left: Balance,
    right: Balance,
    denominator: Balance,
) -> Result<Balance, Error> {
    if denominator == 0 {
        return Err(Error::InvariantViolation);
    }
    checked_product(left, right).map(|product| product / denominator)
}

/// Checked reference arithmetic; supplying a policy remains the caller's responsibility.
pub fn mul_div_ceil(left: Balance, right: Balance, denominator: Balance) -> Result<Balance, Error> {
    if denominator == 0 {
        return Err(Error::InvariantViolation);
    }
    let product = checked_product(left, right)?;
    let quotient = product / denominator;
    if product % denominator == 0 {
        Ok(quotient)
    } else {
        quotient.checked_add(1).ok_or(Error::ArithmeticOverflow)
    }
}

/// Checked reference arithmetic; supplying a policy remains the caller's responsibility.
pub fn integer_sqrt(value: Balance) -> Balance {
    if value < 2 {
        return value;
    }
    let bits = u128::BITS - value.leading_zeros();
    let mut estimate = 1u128 << bits.div_ceil(2);
    loop {
        let next = (estimate + value / estimate) / 2;
        if next >= estimate {
            return estimate;
        }
        estimate = next;
    }
}
