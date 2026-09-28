//! Policy-parametric FRAME storage kernel for the dormant V14 AMM.
//!
//! There is deliberately no production Config, dispatchable, storage version, runtime index,
//! genesis installation, API registration, fee destination or WeightInfo placeholder. Asset keys,
//! record encoding, hashers, economics, admissions and event encoding are supplied by Config.
//! The adopted v1 source contract is instantiated only by synthetic test/benchmark runtimes.
//! Entry methods take an already authenticated account; a future dispatch wrapper must authenticate
//! it. Each mutation owns its FRAME transaction, including backend transfers, deposits and events.
#![cfg_attr(not(feature = "std"), no_std)]
extern crate alloc;

use alloc::vec::Vec;
use codec::{DecodeAll, Encode, MaxEncodedLen};
use era_v14_application_primitives::{
    amm::{self, Event, PoolAccountDeriver, PoolCreationPolicy, PoolId, Rate},
    assets::{Error as AssetError, FungibleAsset, FungibleInspect, FungibleTransfer},
};
use frame_support::{
    pallet_prelude::*,
    storage::{transactional::with_transaction_opaque_err, StoragePrefixedMap, TransactionOutcome},
    traits::Get,
    ReversibleStorageHasher, StorageHasher,
};

pub use pallet::*;
pub type Pair<T> = (<T as Config>::Asset, <T as Config>::Asset);
pub type Page<T> = (Vec<Pair<T>>, Option<Pair<T>>);
/// Maximum global LP records/account indices implied by both independently bounded dimensions.
pub struct TotalPositions<T>(core::marker::PhantomData<T>);
impl<T: Config> Get<u32> for TotalPositions<T> {
    fn get() -> u32 {
        T::MaxPools::get()
            .checked_mul(T::MaxProviders::get())
            .unwrap_or(0)
    }
}

pub struct MapBound<B>(core::marker::PhantomData<B>);
impl<B: Get<u32>> Get<Option<u32>> for MapBound<B> {
    fn get() -> Option<u32> {
        Some(B::get())
    }
}

/// No SCALE discriminants are allocated by this internal error type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    Reference(amm::Error),
    Unconfigured,
    Bound,
    InvalidLimit,
    InvalidCursor,
    Deposit,
    Liveness,
    Reentrant,
    CorruptState,
    TransactionLimit,
}
impl From<amm::Error> for Fault {
    fn from(e: amm::Error) -> Self {
        Self::Reference(e)
    }
}
impl From<AssetError> for Fault {
    fn from(e: AssetError) -> Self {
        Self::Reference(amm::Error::Asset(e))
    }
}
pub type Result<T, E = Fault> = core::result::Result<T, E>;

/// Config owns key encoding. The round trip rejects aliases and anything outside FungibleAsset.
pub trait AssetKey: Copy + Ord {
    fn fungible(self) -> Result<FungibleAsset<u32>>;
    fn from_fungible(asset: FungibleAsset<u32>) -> Self;
}
/// Data independent of any production record encoding or storage version.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot<Account> {
    pub custody: Account,
    pub creator: Account,
    pub deposit: u128,
    pub reserves: [u128; 2],
    pub total_lp: u128,
    pub locked_lp: u128,
    pub user_lp: u128,
}
/// The frozen asset interface plus explicit native reservation and custody-liveness contracts.
/// Implementations may write only transactional runtime storage, never external side effects.
pub trait Backend<Account>: FungibleTransfer<Account, AssetId = u32> + Default {
    fn minimum(&self, asset: FungibleAsset<u32>) -> core::result::Result<u128, AssetError>;
    fn reserved_native(&self, account: &Account) -> u128;
    fn reserve_native(
        &mut self,
        account: &Account,
        amount: u128,
    ) -> core::result::Result<(), AssetError>;
    /// Reject protocol/authority identities, not ordinary balances or foreign references.
    fn identity_conflict(&self, account: &Account) -> bool;
    fn providers(&self, account: &Account) -> u32;
    /// Spendable under the frozen Preserve/Polite backend contract, including foreign restrictions.
    fn reducible(&self, asset: FungibleAsset<u32>, account: &Account) -> u128;
    fn establish(&mut self, account: &Account) -> core::result::Result<(), AssetError>;
}
pub trait Admission {
    fn ensure(pair: PoolId<u32>) -> Result<()>;
}
pub struct EmptyPairs;
impl Admission for EmptyPairs {
    fn ensure(_: PoolId<u32>) -> Result<()> {
        Err(AssetError::UnsupportedAsset.into())
    }
}
/// The adopted source event encoding is v1::EventV1; no runtime event is registered. The sink belongs to the same transaction as all pool changes.
pub trait EventSink<Account> {
    fn emit(event: Event<Account, u32>) -> Result<()>;
}

/// Rounding is a Config choice. This kernel supplies the historical checked reference convention.
pub trait Arithmetic {
    fn validate_reserves(reserves: [u128; 2]) -> Result<()>;
    fn invariant(before: [u128; 2], after: [u128; 2]) -> Result<()>;
    fn quote(
        reserves: [u128; 2],
        amount: u128,
        fee: Rate,
        exact_output: bool,
    ) -> Result<(u128, u128, u128)>;
    fn add(
        reserves: [u128; 2],
        supply: u128,
        desired: [u128; 2],
        lock: u128,
    ) -> Result<([u128; 2], u128, u128)>;
    fn remove(reserves: [u128; 2], supply: u128, lp: u128) -> Result<[u128; 2]>;
}
pub struct ReferenceArithmetic;
impl Arithmetic for ReferenceArithmetic {
    fn validate_reserves(r: [u128; 2]) -> Result<()> {
        amm::checked_product(r[0], r[1])?;
        Ok(())
    }
    fn invariant(before: [u128; 2], after: [u128; 2]) -> Result<()> {
        if amm::checked_product(after[0], after[1])? < amm::checked_product(before[0], before[1])? {
            return Err(amm::Error::InvariantViolation.into());
        }
        Ok(())
    }
    fn quote(r: [u128; 2], amount: u128, fee: Rate, exact: bool) -> Result<(u128, u128, u128)> {
        if fee.denominator == 0 || fee.numerator >= fee.denominator {
            return Err(amm::Error::InvalidConfiguration.into());
        }
        let retained = fee.denominator - fee.numerator;
        let (input, output) = if exact {
            if amount == 0 || amount >= r[1] {
                return Err(amm::Error::InsufficientLiquidity.into());
            }
            let effective = amm::mul_div_ceil(r[0], amount, r[1] - amount)?;
            (
                amm::mul_div_ceil(effective, fee.denominator, retained)?,
                amount,
            )
        } else {
            let effective = amm::mul_div_floor(amount, retained, fee.denominator)?;
            (amount, amm::quote_output(r[0], r[1], effective)?)
        };
        let effective = amm::mul_div_floor(input, retained, fee.denominator)?;
        if r[0] == 0 || input == 0 || effective == 0 || output == 0 || output >= r[1] {
            return Err(amm::Error::InsufficientLiquidity.into());
        }
        Ok((input, output, input - effective))
    }
    fn add(
        r: [u128; 2],
        supply: u128,
        desired: [u128; 2],
        lock: u128,
    ) -> Result<([u128; 2], u128, u128)> {
        if supply == 0 {
            let root = amm::integer_sqrt(amm::checked_product(desired[0], desired[1])?);
            if root <= lock {
                return Err(amm::Error::InsufficientInitialLiquidity.into());
            }
            return Ok((desired, root - lock, lock));
        }
        let optimal = amm::mul_div_floor(desired[0], r[1], r[0])?;
        let amounts = if optimal <= desired[1] {
            [desired[0], optimal]
        } else {
            [amm::mul_div_floor(desired[1], r[0], r[1])?, desired[1]]
        };
        let lp = amm::mul_div_floor(amounts[0], supply, r[0])?
            .min(amm::mul_div_floor(amounts[1], supply, r[1])?);
        if lp == 0 {
            return Err(amm::Error::InsufficientLiquidity.into());
        }
        Ok((amounts, lp, lock))
    }
    fn remove(r: [u128; 2], supply: u128, lp: u128) -> Result<[u128; 2]> {
        Ok([
            amm::mul_div_floor(lp, r[0], supply)?,
            amm::mul_div_floor(lp, r[1], supply)?,
        ])
    }
}

/// Policy-selectable widening of intermediates only. Stored balances and LP remain u128.
/// Not selected by any production Config. All fitting reference calculations retain their rounding.
pub struct WideArithmetic;
impl WideArithmetic {
    fn narrow(value: sp_core::U256) -> Result<u128> {
        if value > sp_core::U256::from(u128::MAX) {
            return Err(amm::Error::ArithmeticOverflow.into());
        }
        Ok(value.low_u128())
    }
    fn product(a: u128, b: u128) -> Result<sp_core::U256> {
        sp_core::U256::from(a)
            .checked_mul(sp_core::U256::from(b))
            .ok_or(amm::Error::ArithmeticOverflow.into())
    }
    fn divide(a: u128, b: u128, d: u128, ceil: bool) -> Result<u128> {
        if d == 0 {
            return Err(amm::Error::InvariantViolation.into());
        }
        let p = Self::product(a, b)?;
        let d = sp_core::U256::from(d);
        let q = p / d;
        Self::narrow(if ceil && p % d != sp_core::U256::zero() {
            q.checked_add(sp_core::U256::one())
                .ok_or(amm::Error::ArithmeticOverflow)?
        } else {
            q
        })
    }
    fn root(a: u128, b: u128) -> Result<u128> {
        let p = Self::product(a, b)?;
        let (mut lo, mut hi) = (0u128, u128::MAX);
        // Exactly bounded binary search; every midpoint and product is checked.
        for _ in 0..128 {
            if lo == hi {
                break;
            }
            let distance = hi - lo;
            let mid = lo
                .checked_add(distance / 2 + distance % 2)
                .ok_or(amm::Error::ArithmeticOverflow)?;
            if Self::product(mid, mid)? <= p {
                lo = mid;
            } else {
                hi = mid - 1;
            }
        }
        if lo != hi {
            return Err(amm::Error::InvariantViolation.into());
        }
        Ok(lo)
    }
}
impl Arithmetic for WideArithmetic {
    fn validate_reserves(r: [u128; 2]) -> Result<()> {
        Self::product(r[0], r[1])?;
        Ok(())
    }
    fn invariant(before: [u128; 2], after: [u128; 2]) -> Result<()> {
        if Self::product(after[0], after[1])? < Self::product(before[0], before[1])? {
            return Err(amm::Error::InvariantViolation.into());
        }
        Ok(())
    }
    fn quote(r: [u128; 2], amount: u128, fee: Rate, exact: bool) -> Result<(u128, u128, u128)> {
        if fee.denominator == 0 || fee.numerator >= fee.denominator {
            return Err(amm::Error::InvalidConfiguration.into());
        }
        let retained = fee.denominator - fee.numerator;
        let (input, output) = if exact {
            if amount == 0 || amount >= r[1] {
                return Err(amm::Error::InsufficientLiquidity.into());
            }
            let effective = Self::divide(r[0], amount, r[1] - amount, true)?;
            (
                Self::divide(effective, fee.denominator, retained, true)?,
                amount,
            )
        } else {
            let effective = Self::divide(amount, retained, fee.denominator, false)?;
            (
                amount,
                Self::divide(effective, r[1], plus(r[0], effective)?, false)?,
            )
        };
        let effective = Self::divide(input, retained, fee.denominator, false)?;
        if r[0] == 0 || r[1] == 0 || input == 0 || effective == 0 || output == 0 || output >= r[1] {
            return Err(amm::Error::InsufficientLiquidity.into());
        }
        Ok((input, output, input - effective))
    }
    fn add(
        r: [u128; 2],
        supply: u128,
        desired: [u128; 2],
        lock: u128,
    ) -> Result<([u128; 2], u128, u128)> {
        if supply == 0 {
            let root = Self::root(desired[0], desired[1])?;
            if root <= lock {
                return Err(amm::Error::InsufficientInitialLiquidity.into());
            }
            return Ok((desired, root - lock, lock));
        }
        let optimal = Self::divide(desired[0], r[1], r[0], false)?;
        let amounts = if optimal <= desired[1] {
            [desired[0], optimal]
        } else {
            [Self::divide(desired[1], r[0], r[1], false)?, desired[1]]
        };
        let lp = Self::divide(amounts[0], supply, r[0], false)?
            .min(Self::divide(amounts[1], supply, r[1], false)?);
        if lp == 0 {
            return Err(amm::Error::InsufficientLiquidity.into());
        }
        Ok((amounts, lp, lock))
    }
    fn remove(r: [u128; 2], supply: u128, lp: u128) -> Result<[u128; 2]> {
        Ok([
            Self::divide(lp, r[0], supply, false)?,
            Self::divide(lp, r[1], supply, false)?,
        ])
    }
}

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    #[pallet::config]
    pub trait Config: frame_system::Config {
        type Asset: Parameter + MaxEncodedLen + AssetKey;
        type Record: Parameter
            + MaxEncodedLen
            + From<Snapshot<Self::AccountId>>
            + Into<Snapshot<Self::AccountId>>;
        type PoolHasher: ReversibleStorageHasher;
        type AccountHasher: ReversibleStorageHasher;
        type Assets: Backend<Self::AccountId>;
        type Admission: Admission;
        type Creator: PoolCreationPolicy<Self::AccountId, u32> + Default;
        type Custody: PoolAccountDeriver<Self::AccountId, u32> + Default;
        type Events: EventSink<Self::AccountId>;
        type Math: Arithmetic;
        type Clock: Get<u64>;
        type Fee: Get<Rate>;
        type FeeLimit: Get<Rate>;
        type LockedLiquidity: Get<u128>;
        type MinimumPosition: Get<u128>;
        type MinimumTrade: Get<u128>;
        type CreationDeposit: Get<u128>;
        type Horizon: Get<u64>;
        type MaxPools: Get<u32>;
        type MaxPositions: Get<u32>;
        type MaxProviders: Get<u32>;
        type MaxPage: Get<u32>;
    }
    #[pallet::pallet]
    pub struct Pallet<T>(_);
    #[pallet::storage]
    pub type Pools<T: Config> = StorageMap<
        _,
        T::PoolHasher,
        Pair<T>,
        T::Record,
        OptionQuery,
        GetDefault,
        MapBound<T::MaxPools>,
    >;
    #[pallet::storage]
    pub type Custodies<T: Config> = StorageMap<
        _,
        T::AccountHasher,
        T::AccountId,
        Pair<T>,
        OptionQuery,
        GetDefault,
        MapBound<T::MaxPools>,
    >;
    #[pallet::storage]
    pub type PoolCount<T> = StorageValue<_, u32, ValueQuery>;
    #[pallet::storage]
    pub type PoolIndex<T: Config> =
        StorageMap<_, Twox64Concat, u32, Pair<T>, OptionQuery, GetDefault, MapBound<T::MaxPools>>;
    #[pallet::storage]
    pub type PoolOrdinal<T: Config> =
        StorageMap<_, T::PoolHasher, Pair<T>, u32, OptionQuery, GetDefault, MapBound<T::MaxPools>>;
    #[pallet::storage]
    pub type Positions<T: Config> = StorageDoubleMap<
        _,
        T::PoolHasher,
        Pair<T>,
        T::AccountHasher,
        T::AccountId,
        u128,
        OptionQuery,
        GetDefault,
        MapBound<TotalPositions<T>>,
    >;
    #[pallet::storage]
    pub type AccountPools<T: Config> = StorageMap<
        _,
        T::AccountHasher,
        T::AccountId,
        BoundedVec<Pair<T>, T::MaxPositions>,
        ValueQuery,
        GetDefault,
        MapBound<TotalPositions<T>>,
    >;
    #[pallet::storage]
    pub type Providers<T: Config> = StorageMap<
        _,
        T::PoolHasher,
        Pair<T>,
        BoundedVec<T::AccountId, T::MaxProviders>,
        ValueQuery,
        GetDefault,
        MapBound<T::MaxPools>,
    >;
    #[pallet::storage]
    pub type Busy<T> = StorageValue<_, bool, ValueQuery>;
}

fn plus(a: u128, b: u128) -> Result<u128> {
    a.checked_add(b)
        .ok_or(amm::Error::ArithmeticOverflow.into())
}
fn minus(a: u128, b: u128) -> Result<u128> {
    a.checked_sub(b).ok_or(Fault::CorruptState)
}

/// Smallest feasible positive initial burn, calculated without searching LP amounts.
/// Positive output i requires burn >= ceil(supply/reserve_i). Outputs are monotone in burn,
/// so failure of this smallest admissible burn at a balance floor rules out every larger burn.
/// Surplus stays intact; it may support account liveness but never increases the amount returned.
pub fn initial_exit<M: Arithmetic>(
    reserves: [u128; 2],
    supply: u128,
    user_lp: u128,
    surplus: [u128; 2],
    minimums: [u128; 2],
    position_minimum: u128,
) -> Result<(u128, [u128; 2])> {
    let reject = Fault::Reference(amm::Error::InsufficientInitialLiquidity);
    if supply == 0 || reserves.contains(&0) || user_lp == 0 || user_lp >= supply {
        return Err(reject);
    }
    let ceil = |d: u128| -> Result<u128> { plus(supply / d, u128::from(supply % d != 0)) };
    let mut burn = ceil(reserves[0])?.max(ceil(reserves[1])?).max(1);
    if burn > user_lp {
        return Err(reject);
    }
    let left = user_lp - burn;
    if left != 0 && left < position_minimum {
        burn = user_lp;
    }
    let outputs = M::remove(reserves, supply, burn)?;
    for i in 0..2 {
        if outputs[i] == 0
            || outputs[i] > reserves[i]
            || plus(reserves[i] - outputs[i], surplus[i])? < minimums[i]
        {
            return Err(reject);
        }
    }
    Ok((burn, outputs))
}

/// Internal moves are bounded to two legs and at most three distinct participants.
type Movement<'a, Account> = (FungibleAsset<u32>, &'a Account, &'a Account, u128);

fn stored<V: DecodeAll + Encode + MaxEncodedLen>(key: &[u8]) -> Result<Option<V>> {
    let mut bytes = alloc::vec![0u8; V::max_encoded_len()];
    let Some(length) = sp_io::storage::read(key, &mut bytes, 0) else {
        return Ok(None);
    };
    if length as usize > bytes.len() {
        return Err(Fault::CorruptState);
    }
    bytes.truncate(length as usize);
    let value = V::decode_all(&mut &bytes[..]).map_err(|_| Fault::CorruptState)?;
    if value.encode() != bytes {
        return Err(Fault::CorruptState);
    }
    Ok(Some(value))
}

impl<T: Config> Pallet<T> {
    fn count() -> Result<u32> {
        Ok(stored(&PoolCount::<T>::hashed_key())?.unwrap_or(0))
    }
    fn custody_pair(who: &T::AccountId) -> Result<Option<Pair<T>>> {
        stored(&Custodies::<T>::hashed_key_for(who))
    }
    fn ordinal(pair: Pair<T>) -> Result<Option<u32>> {
        stored(&PoolOrdinal::<T>::hashed_key_for(pair))
    }
    fn indexed_pool(index: u32) -> Result<Option<Pair<T>>> {
        stored(&PoolIndex::<T>::hashed_key_for(index))
    }
    fn position_balance(pair: Pair<T>, who: &T::AccountId) -> Result<Option<u128>> {
        stored(&Positions::<T>::hashed_key_for(pair, who))
    }
    fn account_pools(who: &T::AccountId) -> Result<BoundedVec<Pair<T>, T::MaxPositions>> {
        Ok(stored(&AccountPools::<T>::hashed_key_for(who))?.unwrap_or_default())
    }
    fn providers(pair: Pair<T>) -> Result<BoundedVec<T::AccountId, T::MaxProviders>> {
        Ok(stored(&Providers::<T>::hashed_key_for(pair))?.unwrap_or_default())
    }

    fn transaction<R>(f: impl FnOnce() -> Result<R>) -> Result<R> {
        with_transaction_opaque_err(|| {
            let result = (|| {
                if stored::<bool>(&Busy::<T>::hashed_key())?.unwrap_or(false) {
                    return Err(Fault::Reentrant);
                }
                Busy::<T>::put(true);
                let output = f()?;
                Busy::<T>::kill();
                Ok(output)
            })();
            match result {
                Ok(x) => TransactionOutcome::Commit(Ok(x)),
                Err(e) => TransactionOutcome::Rollback(Err(e)),
            }
        })
        .map_err(|_| Fault::TransactionLimit)?
    }
    fn config() -> Result<()> {
        let fee = T::Fee::get();
        let max = T::FeeLimit::get();
        if fee.denominator == 0
            || fee.numerator >= fee.denominator
            || max.denominator == 0
            || max.numerator >= max.denominator
            || T::LockedLiquidity::get() == 0
            || T::MinimumPosition::get() == 0
            || T::MinimumTrade::get() == 0
            || T::MaxPools::get() == 0
            || T::MaxPositions::get() == 0
            || T::MaxProviders::get() == 0
            || T::MaxPage::get() == 0
            || TotalPositions::<T>::get() == 0
            || amm::checked_product(fee.numerator, max.denominator)?
                > amm::checked_product(max.numerator, fee.denominator)?
        {
            return Err(amm::Error::InvalidConfiguration.into());
        }
        Ok(())
    }
    fn deadline(deadline: u64) -> Result<()> {
        let distance = deadline
            .checked_sub(T::Clock::get())
            .ok_or(amm::Error::DeadlineExpired)?;
        if distance > T::Horizon::get() {
            return Err(Fault::Bound);
        }
        Ok(())
    }
    fn pair(a: T::Asset, b: T::Asset) -> Result<(Pair<T>, PoolId<u32>)> {
        let fa = a.fungible()?;
        let fb = b.fungible()?;
        if T::Asset::from_fungible(fa) != a || T::Asset::from_fungible(fb) != b {
            return Err(Fault::CorruptState);
        }
        let id = PoolId::new(fa, fb)?;
        T::Admission::ensure(id)?;
        let backend = T::Assets::default();
        for asset in [fa, fb] {
            if backend.minimum(asset)? == 0 {
                return Err(amm::Error::InvalidConfiguration.into());
            }
            backend.total_issuance(asset)?;
        }
        Ok((
            (
                T::Asset::from_fungible(id.asset_0),
                T::Asset::from_fungible(id.asset_1),
            ),
            id,
        ))
    }
    fn read(pair: Pair<T>) -> Result<Snapshot<T::AccountId>> {
        stored::<T::Record>(&Pools::<T>::hashed_key_for(pair))?
            .map(Into::into)
            .ok_or(amm::Error::PoolNotFound.into())
    }
    fn check(pair: Pair<T>, p: &Snapshot<T::AccountId>) -> Result<()> {
        let b = T::Assets::default();
        if Self::custody_pair(&p.custody)? != Some(pair)
            || p.creator == p.custody
            || p.deposit != T::CreationDeposit::get()
            || T::Custody::default()
                .derive_pool_account(&PoolId::new(pair.0.fungible()?, pair.1.fungible()?)?)?
                != p.custody
            || plus(p.user_lp, p.locked_lp)? != p.total_lp
        {
            return Err(Fault::CorruptState);
        }
        if b.providers(&p.custody) == 0 {
            return Err(Fault::Liveness);
        }
        Self::backing(pair, p)?;
        if p.total_lp == 0 {
            if p.reserves != [0, 0] || p.locked_lp != 0 || p.user_lp != 0 {
                return Err(Fault::CorruptState);
            }
        } else if p.locked_lp != T::LockedLiquidity::get() || p.reserves.contains(&0) {
            return Err(Fault::CorruptState);
        }
        T::Math::validate_reserves(p.reserves)?;
        Ok(())
    }
    fn backing(pair: Pair<T>, p: &Snapshot<T::AccountId>) -> Result<[u128; 2]> {
        let b = T::Assets::default();
        let surplus = |asset, reserve| {
            b.balance(asset, &p.custody)
                .checked_sub(reserve)
                .ok_or(Fault::Reference(amm::Error::InvariantViolation))
        };
        Ok([
            surplus(pair.0.fungible()?, p.reserves[0])?,
            surplus(pair.1.fungible()?, p.reserves[1])?,
        ])
    }
    fn preserve_surplus(
        pair: Pair<T>,
        p: &Snapshot<T::AccountId>,
        before: [u128; 2],
    ) -> Result<()> {
        if Self::backing(pair, p)? != before {
            return Err(amm::Error::CustodyBalanceMismatch.into());
        }
        Ok(())
    }
    /// Derived read-only surplus. It is never credited to reserves or LP, nor stored as a claim.
    pub fn surplus(assets: Pair<T>) -> Result<[u128; 2]> {
        let (pair, _) = Self::pair(assets.0, assets.1)?;
        let p = Self::read(pair)?;
        Self::check(pair, &p)?;
        Self::backing(pair, &p)
    }
    fn participant(p: &Snapshot<T::AccountId>, account: &T::AccountId) -> Result<()> {
        // No pool custody account can act as trader, provider or recipient in any pool.
        if p.custody == *account || Custodies::<T>::contains_key(account) {
            return Err(amm::Error::CustodyAccountCollision.into());
        }
        Ok(())
    }
    fn transfer(
        asset: FungibleAsset<u32>,
        from: &T::AccountId,
        to: &T::AccountId,
        amount: u128,
    ) -> Result<()> {
        if from == to || amount == 0 {
            return Err(amm::Error::ZeroAmount.into());
        }
        let mut b = T::Assets::default();
        let supply = b.total_issuance(asset)?;
        let left = b.balance(asset, from);
        let right = b.balance(asset, to);
        let expected_left = left
            .checked_sub(amount)
            .ok_or(AssetError::InsufficientBalance)?;
        let expected_right = plus(right, amount)?;
        b.transfer(asset, from, to, amount)?;
        if b.balance(asset, from) != expected_left
            || b.balance(asset, to) != expected_right
            || b.total_issuance(asset)? != supply
        {
            return Err(AssetError::AccountingInvariant.into());
        }
        let minimum = b.minimum(asset)?;
        // Match the frozen Preserve contract even if an injected backend silently reaps dust.
        if expected_left < minimum || expected_right < minimum {
            return Err(AssetError::BelowMinimum.into());
        }
        Ok(())
    }
    fn transfers(moves: [Movement<'_, T::AccountId>; 2]) -> Result<()> {
        let b = T::Assets::default();
        let mut assets = alloc::vec![FungibleAsset::NativeEtkn];
        let mut accounts = Vec::new();
        for (asset, from, to, _) in moves {
            if !assets.contains(&asset) {
                assets.push(asset);
            }
            for who in [from, to] {
                if !accounts.contains(&who) {
                    accounts.push(who);
                }
            }
        }
        if accounts.len() > 3 {
            return Err(Fault::CorruptState);
        }
        let mut balances = Vec::new();
        let mut supplies = Vec::new();
        let reservations: Vec<_> = accounts.iter().map(|who| b.reserved_native(who)).collect();
        for asset in &assets {
            supplies.push(b.total_issuance(*asset)?);
            for who in &accounts {
                balances.push(b.balance(*asset, who));
            }
        }
        for (asset, from, to, amount) in moves {
            Self::transfer(asset, from, to, amount)?;
        }
        for (i, asset) in assets.iter().enumerate() {
            if b.total_issuance(*asset)? != supplies[i] {
                return Err(AssetError::AccountingInvariant.into());
            }
            for (j, who) in accounts.iter().enumerate() {
                let mut expected = balances[i * accounts.len() + j];
                for (moved, from, to, amount) in moves {
                    if moved == *asset {
                        if from == *who {
                            expected = minus(expected, amount)?;
                        }
                        if to == *who {
                            expected = plus(expected, amount)?;
                        }
                    }
                }
                if b.balance(*asset, who) != expected || b.reserved_native(who) != reservations[j] {
                    return Err(AssetError::AccountingInvariant.into());
                }
            }
        }
        Ok(())
    }
    fn position(pair: Pair<T>, who: &T::AccountId, previous: u128, next: u128) -> Result<()> {
        if next != 0 && next < T::MinimumPosition::get() {
            return Err(Fault::Bound);
        }
        let mut account = Self::account_pools(who)?;
        let mut providers = Self::providers(pair)?;
        if account
            .iter()
            .collect::<alloc::collections::BTreeSet<_>>()
            .len()
            != account.len()
            || providers
                .iter()
                .collect::<alloc::collections::BTreeSet<_>>()
                .len()
                != providers.len()
        {
            return Err(Fault::CorruptState);
        }
        let ai = account.iter().position(|p| *p == pair);
        let pi = providers.iter().position(|a| a == who);
        if (previous == 0) != ai.is_none() || (previous == 0) != pi.is_none() {
            return Err(Fault::CorruptState);
        }
        if previous == 0 && next != 0 {
            account.try_push(pair).map_err(|_| Fault::Bound)?;
            providers.try_push(who.clone()).map_err(|_| Fault::Bound)?;
        } else if previous != 0 && next == 0 {
            account.remove(ai.ok_or(Fault::CorruptState)?);
            providers.remove(pi.ok_or(Fault::CorruptState)?);
        }
        if next == 0 {
            Positions::<T>::remove(pair, who);
        } else {
            Positions::<T>::insert(pair, who, next);
        }
        if account.is_empty() {
            AccountPools::<T>::remove(who);
        } else {
            AccountPools::<T>::insert(who, account);
        }
        if providers.is_empty() {
            Providers::<T>::remove(pair);
        } else {
            Providers::<T>::insert(pair, providers);
        }
        Ok(())
    }
    pub fn create(who: &T::AccountId, a: T::Asset, b: T::Asset, deadline: u64) -> Result<Pair<T>> {
        Self::transaction(|| {
            Self::config()?;
            Self::deadline(deadline)?;
            let (pair, id) = Self::pair(a, b)?;
            if Pools::<T>::contains_key(pair) {
                return Err(amm::Error::PoolAlreadyExists.into());
            }
            T::Creator::default().ensure_can_create(who, &id)?;
            let count = Self::count()?;
            if count >= T::MaxPools::get() {
                return Err(Fault::Bound);
            }
            if PoolIndex::<T>::contains_key(count) || PoolOrdinal::<T>::contains_key(pair) {
                return Err(Fault::CorruptState);
            }
            let custody = T::Custody::default().derive_pool_account(&id)?;
            if custody == *who
                || Custodies::<T>::contains_key(&custody)
                || Custodies::<T>::contains_key(who)
                || !Self::account_pools(&custody)?.is_empty()
            {
                return Err(amm::Error::CustodyAccountCollision.into());
            }
            let mut backend = T::Assets::default();
            if backend.identity_conflict(&custody) {
                return Err(amm::Error::CustodyAccountCollision.into());
            }
            let next_providers = backend
                .providers(&custody)
                .checked_add(1)
                .ok_or(Fault::Liveness)?;
            let prefunding = [
                backend.balance(id.asset_0, &custody),
                backend.balance(id.asset_1, &custody),
            ];
            let custody_native = backend.balance(FungibleAsset::NativeEtkn, &custody);
            let custody_reserved = backend.reserved_native(&custody);
            let pair_issuance = [
                backend.total_issuance(id.asset_0)?,
                backend.total_issuance(id.asset_1)?,
            ];
            let deposit = T::CreationDeposit::get();
            let free = backend.balance(FungibleAsset::NativeEtkn, who);
            let reserved = backend.reserved_native(who);
            let issuance = backend.total_issuance(FungibleAsset::NativeEtkn)?;
            let after_free = free.checked_sub(deposit).ok_or(Fault::Deposit)?;
            let after_reserved = plus(reserved, deposit)?;
            backend.reserve_native(who, deposit)?;
            backend.establish(&custody)?;
            if backend.providers(&custody) != next_providers {
                return Err(Fault::Liveness);
            }
            if backend.balance(FungibleAsset::NativeEtkn, who) != after_free
                || backend.reserved_native(who) != after_reserved
                || backend.total_issuance(FungibleAsset::NativeEtkn)? != issuance
                || backend.balance(FungibleAsset::NativeEtkn, &custody) != custody_native
                || backend.reserved_native(&custody) != custody_reserved
                || backend.balance(id.asset_0, &custody) != prefunding[0]
                || backend.balance(id.asset_1, &custody) != prefunding[1]
                || backend.total_issuance(id.asset_0)? != pair_issuance[0]
                || backend.total_issuance(id.asset_1)? != pair_issuance[1]
            {
                return Err(Fault::Deposit);
            }
            let p = Snapshot {
                custody: custody.clone(),
                creator: who.clone(),
                deposit,
                reserves: [0, 0],
                total_lp: 0,
                locked_lp: 0,
                user_lp: 0,
            };
            Custodies::<T>::insert(&custody, pair);
            Self::check(pair, &p)?;
            Pools::<T>::insert(pair, T::Record::from(p));
            PoolIndex::<T>::insert(count, pair);
            PoolOrdinal::<T>::insert(pair, count);
            PoolCount::<T>::put(count.checked_add(1).ok_or(Fault::Bound)?);
            T::Events::emit(Event::PoolCreated {
                pool: id,
                creator: who.clone(),
                custody_account: custody,
            })?;
            Ok(pair)
        })
    }
    /// Amounts and minima follow the caller's asset order; the outcome follows canonical order.
    pub fn add(
        who: &T::AccountId,
        assets: Pair<T>,
        desired: [u128; 2],
        minima: [u128; 2],
        deadline: u64,
    ) -> Result<amm::LiquidityOutcome<u32>> {
        Self::transaction(|| {
            Self::config()?;
            Self::deadline(deadline)?;
            let (pair, id) = Self::pair(assets.0, assets.1)?;
            let mut p = Self::read(pair)?;
            Self::check(pair, &p)?;
            let surplus = Self::backing(pair, &p)?;
            Self::participant(&p, who)?;
            let orient = |v: [u128; 2]| if assets.0 == pair.0 { v } else { [v[1], v[0]] };
            let desired = orient(desired);
            let minima = orient(minima);
            if desired.contains(&0) {
                return Err(amm::Error::ZeroAmount.into());
            }
            let initial = p.total_lp == 0;
            let (amounts, minted, locked) = T::Math::add(
                p.reserves,
                p.total_lp,
                desired,
                if initial {
                    T::LockedLiquidity::get()
                } else {
                    p.locked_lp
                },
            )?;
            for (i, asset) in [id.asset_0, id.asset_1].into_iter().enumerate() {
                if amounts[i] == 0 {
                    return Err(amm::Error::InsufficientLiquidity.into());
                }
                if initial && amounts[i] < T::Assets::default().minimum(asset)? {
                    return Err(AssetError::BelowMinimum.into());
                }
                if amounts[i] < minima[i] || amounts[i] > desired[i] {
                    return Err(amm::Error::SlippageExceeded.into());
                }
            }
            let old = Self::position_balance(pair, who)?.unwrap_or(0);
            Self::position(pair, who, old, plus(old, minted)?)?;
            p.user_lp = plus(p.user_lp, minted)?;
            p.locked_lp = locked;
            p.total_lp = plus(p.user_lp, locked)?;
            p.reserves = [
                plus(p.reserves[0], amounts[0])?,
                plus(p.reserves[1], amounts[1])?,
            ];
            T::Math::validate_reserves(p.reserves)?;
            Self::transfers([
                (id.asset_0, who, &p.custody, amounts[0]),
                (id.asset_1, who, &p.custody, amounts[1]),
            ])?;
            if initial {
                let backend = T::Assets::default();
                let minimums = [backend.minimum(id.asset_0)?, backend.minimum(id.asset_1)?];
                let (_, outputs) = initial_exit::<T::Math>(
                    p.reserves,
                    p.total_lp,
                    minted,
                    surplus,
                    minimums,
                    T::MinimumPosition::get(),
                )?;
                if backend.reducible(id.asset_0, &p.custody) < outputs[0]
                    || backend.reducible(id.asset_1, &p.custody) < outputs[1]
                {
                    return Err(amm::Error::InsufficientInitialLiquidity.into());
                }
            }
            Self::check(pair, &p)?;
            Self::preserve_surplus(pair, &p, surplus)?;
            Pools::<T>::insert(pair, T::Record::from(p));
            T::Events::emit(Event::LiquidityAdded {
                pool: id,
                provider: who.clone(),
                amount_0: amounts[0],
                amount_1: amounts[1],
                lp_minted: minted,
            })?;
            Ok(amm::LiquidityOutcome {
                pool: id,
                amount_0: amounts[0],
                amount_1: amounts[1],
                lp_amount: minted,
            })
        })
    }
    pub fn remove(
        who: &T::AccountId,
        assets: Pair<T>,
        lp: u128,
        minima: [u128; 2],
        recipient: &T::AccountId,
        deadline: u64,
    ) -> Result<amm::LiquidityOutcome<u32>> {
        Self::transaction(|| {
            Self::config()?;
            Self::deadline(deadline)?;
            if lp == 0 {
                return Err(amm::Error::ZeroAmount.into());
            }
            let (pair, id) = Self::pair(assets.0, assets.1)?;
            let mut p = Self::read(pair)?;
            Self::check(pair, &p)?;
            let surplus = Self::backing(pair, &p)?;
            Self::participant(&p, who)?;
            Self::participant(&p, recipient)?;
            let old = Self::position_balance(pair, who)?.unwrap_or(0);
            let next = old
                .checked_sub(lp)
                .ok_or(amm::Error::InsufficientLpBalance)?;
            let amounts = T::Math::remove(p.reserves, p.total_lp, lp)?;
            let minima = if assets.0 == pair.0 {
                minima
            } else {
                [minima[1], minima[0]]
            };
            if amounts.contains(&0) {
                return Err(amm::Error::InsufficientLiquidity.into());
            }
            if amounts[0] < minima[0] || amounts[1] < minima[1] {
                return Err(amm::Error::SlippageExceeded.into());
            }
            Self::position(pair, who, old, next)?;
            p.user_lp = minus(p.user_lp, lp)?;
            p.total_lp = minus(p.total_lp, lp)?;
            p.reserves = [
                minus(p.reserves[0], amounts[0])?,
                minus(p.reserves[1], amounts[1])?,
            ];
            Self::transfers([
                (id.asset_0, &p.custody, recipient, amounts[0]),
                (id.asset_1, &p.custody, recipient, amounts[1]),
            ])?;
            Self::check(pair, &p)?;
            Self::preserve_surplus(pair, &p, surplus)?;
            Pools::<T>::insert(pair, T::Record::from(p));
            T::Events::emit(Event::LiquidityRemoved {
                pool: id,
                provider: who.clone(),
                recipient: recipient.clone(),
                amount_0: amounts[0],
                amount_1: amounts[1],
                lp_burned: lp,
            })?;
            Ok(amm::LiquidityOutcome {
                pool: id,
                amount_0: amounts[0],
                amount_1: amounts[1],
                lp_amount: lp,
            })
        })
    }
    fn quoted(
        p: &Snapshot<T::AccountId>,
        reverse: bool,
        amount: u128,
        exact_output: bool,
    ) -> Result<(u128, u128, u128)> {
        let r = if reverse {
            [p.reserves[1], p.reserves[0]]
        } else {
            p.reserves
        };
        let q = T::Math::quote(r, amount, T::Fee::get(), exact_output)?;
        if q.0 < T::MinimumTrade::get() || q.1 < T::MinimumTrade::get() {
            return Err(amm::Error::InsufficientLiquidity.into());
        }
        let mut after = p.reserves;
        let index = usize::from(reverse);
        after[index] = plus(after[index], q.0)?;
        after[1 - index] = minus(after[1 - index], q.1)?;
        T::Math::invariant(p.reserves, after)?;
        Ok(q)
    }
    /// Exact-input returns (input, output, retained LP fee); exact-output uses the same shape.
    pub fn quote(assets: Pair<T>, amount: u128, exact_output: bool) -> Result<(u128, u128, u128)> {
        Self::config()?;
        let (pair, _) = Self::pair(assets.0, assets.1)?;
        let p = Self::read(pair)?;
        Self::check(pair, &p)?;
        Self::quoted(&p, assets.0 != pair.0, amount, exact_output)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn swap(
        who: &T::AccountId,
        assets: Pair<T>,
        amount: u128,
        limit: u128,
        recipient: &T::AccountId,
        deadline: u64,
        exact_output: bool,
    ) -> Result<amm::SwapOutcome<u32>> {
        Self::transaction(|| {
            Self::config()?;
            Self::deadline(deadline)?;
            let (pair, id) = Self::pair(assets.0, assets.1)?;
            let mut p = Self::read(pair)?;
            Self::check(pair, &p)?;
            let surplus = Self::backing(pair, &p)?;
            Self::participant(&p, who)?;
            Self::participant(&p, recipient)?;
            let (input, output, fee) = Self::quoted(&p, assets.0 != pair.0, amount, exact_output)?;
            if (exact_output && input > limit) || (!exact_output && output < limit) {
                return Err(amm::Error::SlippageExceeded.into());
            }
            let index = usize::from(assets.0 != pair.0);
            let old_reserves = p.reserves;
            p.reserves[index] = plus(p.reserves[index], input)?;
            p.reserves[1 - index] = minus(p.reserves[1 - index], output)?;
            T::Math::invariant(old_reserves, p.reserves)?;
            let asset_in = assets.0.fungible()?;
            let asset_out = assets.1.fungible()?;
            Self::transfers([
                (asset_in, who, &p.custody, input),
                (asset_out, &p.custody, recipient, output),
            ])?;
            Self::check(pair, &p)?;
            Self::preserve_surplus(pair, &p, surplus)?;
            Pools::<T>::insert(pair, T::Record::from(p));
            T::Events::emit(Event::SwapExecuted {
                pool: id,
                trader: who.clone(),
                recipient: recipient.clone(),
                asset_in,
                asset_out,
                amount_in: input,
                amount_out: output,
                total_fee: fee,
                protocol_fee: 0,
                exact_output,
            })?;
            Ok(amm::SwapOutcome {
                pool: id,
                asset_in,
                asset_out,
                amount_in: input,
                amount_out: output,
                total_fee: fee,
                protocol_fee: 0,
            })
        })
    }
    /// Bounded reconciliation for a selected pool, suitable for migration/rehearsal validation.
    /// Never invoked as an unbounded on-chain scan or implicit runtime hook.
    pub fn reconcile(assets: Pair<T>) -> Result<()> {
        let (pair, _) = Self::pair(assets.0, assets.1)?;
        let p = Self::read(pair)?;
        Self::check(pair, &p)?;
        let providers = Self::providers(pair)?;
        let mut total = 0;
        for (index, who) in providers.iter().enumerate() {
            if providers[..index].contains(who) {
                return Err(Fault::CorruptState);
            }
            let lp = Self::position_balance(pair, who)?.ok_or(Fault::CorruptState)?;
            if lp < T::MinimumPosition::get()
                || Self::account_pools(who)?
                    .iter()
                    .filter(|id| **id == pair)
                    .count()
                    != 1
            {
                return Err(Fault::CorruptState);
            }
            total = plus(total, lp)?;
        }
        if total != p.user_lp {
            return Err(Fault::CorruptState);
        }
        // Count actual position keys, including orphan or malformed entries, with at most
        // MaxProviders+1 probes. FRAME iterators skip undecodable records, so use raw traversal.
        let mut prefix = Positions::<T>::final_prefix().to_vec();
        prefix.extend_from_slice(T::PoolHasher::hash(&pair.encode()).as_ref());
        let mut previous = prefix.clone();
        let mut count = 0u32;
        while let Some(key) = sp_io::storage::next_key(&previous) {
            if !key.starts_with(&prefix) {
                break;
            }
            count = count.checked_add(1).ok_or(Fault::Bound)?;
            if count > T::MaxProviders::get() {
                return Err(Fault::Bound);
            }
            let suffix = &key[prefix.len()..];
            if suffix.len() < T::AccountHasher::hash(&[]).as_ref().len()
                || suffix.len() > T::AccountHasher::max_len::<T::AccountId>()
            {
                return Err(Fault::CorruptState);
            }
            let encoded = T::AccountHasher::reverse(suffix);
            let who =
                T::AccountId::decode_all(&mut &encoded[..]).map_err(|_| Fault::CorruptState)?;
            if who.encode() != encoded
                || Positions::<T>::hashed_key_for(pair, &who) != key
                || !providers.contains(&who)
            {
                return Err(Fault::CorruptState);
            }
            if stored::<u128>(&key)?.unwrap_or(0) < T::MinimumPosition::get() {
                return Err(Fault::CorruptState);
            }
            previous = key;
        }
        if count as usize != providers.len() {
            return Err(Fault::CorruptState);
        }
        let ordinal = Self::ordinal(pair)?.ok_or(Fault::CorruptState)?;
        if ordinal >= Self::count()? || Self::indexed_pool(ordinal)? != Some(pair) {
            return Err(Fault::CorruptState);
        }
        Ok(())
    }
    pub fn pool(assets: Pair<T>) -> Result<Snapshot<T::AccountId>> {
        let (pair, _) = Self::pair(assets.0, assets.1)?;
        let p = Self::read(pair)?;
        Self::check(pair, &p)?;
        Ok(p)
    }
    pub fn lp(assets: Pair<T>, who: &T::AccountId) -> Result<u128> {
        let (pair, _) = Self::pair(assets.0, assets.1)?;
        let pool = Self::read(pair)?;
        Self::check(pair, &pool)?;
        Ok(Self::position_balance(pair, who)?.unwrap_or(0))
    }
    fn limit(limit: u32) -> Result<()> {
        if limit == 0 || limit > T::MaxPage::get() {
            return Err(Fault::InvalidLimit);
        }
        Ok(())
    }
    /// Append-only creation order, exclusive existing pair cursor, pinned-state semantics.
    pub fn pools(cursor: Option<Pair<T>>, limit: u32) -> Result<Page<T>> {
        Self::limit(limit)?;
        let start = match cursor {
            None => 0,
            Some(pair) => Self::ordinal(pair)?
                .ok_or(Fault::InvalidCursor)?
                .checked_add(1)
                .ok_or(Fault::CorruptState)?,
        };
        let count = Self::count()?;
        if count > T::MaxPools::get() || start > count {
            return Err(Fault::CorruptState);
        }
        let end = start.checked_add(limit).ok_or(Fault::Bound)?.min(count);
        let mut values = Vec::new();
        for i in start..end {
            let pair = Self::indexed_pool(i)?.ok_or(Fault::CorruptState)?;
            if Self::ordinal(pair)? != Some(i) {
                return Err(Fault::CorruptState);
            }
            Self::pool(pair)?;
            values.push(pair);
        }
        let next = if end < count {
            values.last().copied()
        } else {
            None
        };
        Ok((values, next))
    }
    /// Bounded position insertion order; a fully removed position is reaped from this index.
    pub fn positions(who: &T::AccountId, cursor: Option<Pair<T>>, limit: u32) -> Result<Page<T>> {
        Self::limit(limit)?;
        let pairs = Self::account_pools(who)?;
        let start = match cursor {
            None => 0,
            Some(p) => {
                pairs
                    .iter()
                    .position(|x| *x == p)
                    .ok_or(Fault::InvalidCursor)?
                    + 1
            }
        };
        let end = start
            .checked_add(limit as usize)
            .ok_or(Fault::Bound)?
            .min(pairs.len());
        let mut values = Vec::new();
        for pair in &pairs[start..end] {
            if Self::lp(*pair, who)? == 0 {
                return Err(Fault::CorruptState);
            }
            values.push(*pair);
        }
        let next = if end < pairs.len() {
            values.last().copied()
        } else {
            None
        };
        Ok((values, next))
    }
}

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarking;
#[cfg(any(test, feature = "test-fixtures"))]
pub mod mock;
#[cfg(test)]
mod tests;

/// Owner-adopted dormant V1 source contract, including the surplus/initial-exit amendments.
/// None of these constants, codecs or ordinary Rust adapters register a runtime pallet/API/call.
pub mod v1 {
    use super::*;
    use codec::{Decode, DecodeWithMemTracking};
    use frame_support::{
        parameter_types,
        traits::{ConstU128, ConstU32, ConstU64},
        PalletId,
    };
    use sp_runtime::{traits::AccountIdConversion, AccountId32};

    pub const PALLET_INDEX: u8 = 24; // Final V14 dispatch;22/23 already occupied.
    pub const STORAGE_VERSION: u16 = 1;
    pub const STORAGE_PREFIX: &str = "EraV14Amm";
    pub const API_NAME: &str = "EraV14AmmApiV1";
    pub const API_VERSION: u32 = 1;
    pub const RETAIN_SPEC_TRANSITIONS: u32 = 2;
    pub const CUSTODY_DOMAIN: [u8; 8] = *b"era/vamm";
    pub const CUSTODY_VERSION: u8 = 1;
    pub const ETKN: u128 = 1_000_000_000_000_000_000;
    pub const PROTOCOL_FEE: Rate = Rate {
        numerator: 0,
        denominator: 1,
    };
    pub const ADMITTED_PAIRS: &[PoolId] = &[];
    pub type AccountId = [u8; 32];
    pub type MaxPools = ConstU32<1024>;
    pub type MaxPositions = ConstU32<64>;
    pub type MaxProviders = ConstU32<1024>;
    pub type MaxPage = ConstU32<64>;
    pub type LockedLiquidity = ConstU128<1000>;
    pub type MinimumPosition = ConstU128<1>;
    pub type MinimumTrade = ConstU128<1>;
    pub type CreationDeposit = ConstU128<10_000_000_000_000_000_000>;
    pub type Horizon = ConstU64<14400>;
    parameter_types! {
        pub Fee: Rate = Rate { numerator: 25, denominator: 10000 };
        pub FeeLimit: Rate = Rate { numerator: 100, denominator: 10000 };
    }

    #[derive(
        Clone,
        Copy,
        Debug,
        PartialEq,
        Eq,
        PartialOrd,
        Ord,
        Encode,
        Decode,
        DecodeWithMemTracking,
        MaxEncodedLen,
        TypeInfo,
    )]
    pub enum Asset {
        #[codec(index = 0)]
        Native,
        #[codec(index = 1)]
        Registered(u32),
    }
    impl AssetKey for Asset {
        fn fungible(self) -> super::Result<FungibleAsset<u32>> {
            Ok(match self {
                Self::Native => FungibleAsset::NativeEtkn,
                Self::Registered(id) => FungibleAsset::Registered(id),
            })
        }
        fn from_fungible(asset: FungibleAsset<u32>) -> Self {
            match asset {
                FungibleAsset::NativeEtkn => Self::Native,
                FungibleAsset::Registered(id) => Self::Registered(id),
            }
        }
    }
    #[derive(
        Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Encode, MaxEncodedLen, TypeInfo,
    )]
    pub struct PoolId {
        pub asset_0: Asset,
        pub asset_1: Asset,
    }
    impl PoolId {
        pub fn new(a: Asset, b: Asset) -> ApiResult<Self> {
            if a == b {
                return Err(AmmErrorV1::IdenticalAssets);
            }
            Ok(if a < b {
                Self {
                    asset_0: a,
                    asset_1: b,
                }
            } else {
                Self {
                    asset_0: b,
                    asset_1: a,
                }
            })
        }
        pub fn canonical(self) -> bool {
            self.asset_0 < self.asset_1
        }
        pub fn pair(self) -> (Asset, Asset) {
            (self.asset_0, self.asset_1)
        }
    }
    impl Decode for PoolId {
        fn decode<I: codec::Input>(input: &mut I) -> core::result::Result<Self, codec::Error> {
            let value = Self {
                asset_0: Asset::decode(input)?,
                asset_1: Asset::decode(input)?,
            };
            if !value.canonical() {
                return Err("noncanonical AMM pair".into());
            }
            Ok(value)
        }
    }
    impl DecodeWithMemTracking for PoolId {}

    #[derive(
        Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo,
    )]
    pub struct PoolRecord {
        pub custody: AccountId,
        pub creator: AccountId,
        pub deposit: u128,
        pub reserve_0: u128,
        pub reserve_1: u128,
        pub total_lp: u128,
        pub locked_lp: u128,
        pub user_lp: u128,
    }
    impl From<Snapshot<AccountId32>> for PoolRecord {
        fn from(p: Snapshot<AccountId32>) -> Self {
            Self {
                custody: p.custody.into(),
                creator: p.creator.into(),
                deposit: p.deposit,
                reserve_0: p.reserves[0],
                reserve_1: p.reserves[1],
                total_lp: p.total_lp,
                locked_lp: p.locked_lp,
                user_lp: p.user_lp,
            }
        }
    }
    impl From<PoolRecord> for Snapshot<AccountId32> {
        fn from(p: PoolRecord) -> Self {
            Self {
                custody: p.custody.into(),
                creator: p.creator.into(),
                deposit: p.deposit,
                reserves: [p.reserve_0, p.reserve_1],
                total_lp: p.total_lp,
                locked_lp: p.locked_lp,
                user_lp: p.user_lp,
            }
        }
    }
    #[derive(
        Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo,
    )]
    pub struct PoolV1 {
        pub pool: PoolId,
        pub record: PoolRecord,
    }
    #[derive(
        Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo,
    )]
    pub struct LpPositionV1 {
        pub pool: PoolId,
        pub account: AccountId,
        pub lp: u128,
    }
    #[derive(
        Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo,
    )]
    pub struct PageV1<Entry> {
        pub entries: BoundedVec<Entry, MaxPage>,
        pub next: Option<PoolId>,
    }
    #[derive(
        Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo,
    )]
    pub struct QuoteV1 {
        pub pool: PoolId,
        pub asset_in: Asset,
        pub asset_out: Asset,
        pub amount_in: u128,
        pub amount_out: u128,
        pub total_fee: u128,
        pub protocol_fee: u128,
    }
    #[derive(
        Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo,
    )]
    pub enum CallV1 {
        #[codec(index = 0)]
        CreatePool {
            asset_a: Asset,
            asset_b: Asset,
            deadline: u32,
        },
        #[codec(index = 1)]
        AddLiquidity {
            asset_a: Asset,
            asset_b: Asset,
            desired_a: u128,
            desired_b: u128,
            min_a: u128,
            min_b: u128,
            deadline: u32,
        },
        #[codec(index = 2)]
        RemoveLiquidity {
            asset_a: Asset,
            asset_b: Asset,
            lp: u128,
            min_a: u128,
            min_b: u128,
            recipient: AccountId,
            deadline: u32,
        },
        #[codec(index = 3)]
        SwapExactInput {
            asset_in: Asset,
            asset_out: Asset,
            amount_in: u128,
            min_out: u128,
            recipient: AccountId,
            deadline: u32,
        },
        #[codec(index = 4)]
        SwapExactOutput {
            asset_in: Asset,
            asset_out: Asset,
            amount_out: u128,
            max_in: u128,
            recipient: AccountId,
            deadline: u32,
        },
    }
    #[derive(
        Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo,
    )]
    pub enum EventV1 {
        #[codec(index = 0)]
        PoolCreated {
            pool: PoolId,
            creator: AccountId,
            custody_account: AccountId,
        },
        #[codec(index = 1)]
        LiquidityAdded {
            pool: PoolId,
            provider: AccountId,
            amount_0: u128,
            amount_1: u128,
            lp_minted: u128,
        },
        #[codec(index = 2)]
        LiquidityRemoved {
            pool: PoolId,
            provider: AccountId,
            recipient: AccountId,
            amount_0: u128,
            amount_1: u128,
            lp_burned: u128,
        },
        #[codec(index = 3)]
        SwapExecuted {
            pool: PoolId,
            trader: AccountId,
            recipient: AccountId,
            asset_in: Asset,
            asset_out: Asset,
            amount_in: u128,
            amount_out: u128,
            total_fee: u128,
            protocol_fee: u128,
            exact_output: bool,
        },
    }
    #[derive(
        Clone,
        Copy,
        Debug,
        PartialEq,
        Eq,
        Encode,
        Decode,
        DecodeWithMemTracking,
        MaxEncodedLen,
        TypeInfo,
    )]
    pub enum AmmErrorV1 {
        #[codec(index = 0)]
        NotFound,
        #[codec(index = 1)]
        InvalidLimit,
        #[codec(index = 2)]
        InvalidCursor,
        #[codec(index = 3)]
        UnsupportedAsset,
        #[codec(index = 4)]
        Unconfigured,
        #[codec(index = 5)]
        Arithmetic,
        #[codec(index = 6)]
        BackendInvariant,
        #[codec(index = 7)]
        IdenticalAssets,
        #[codec(index = 8)]
        Unauthorized,
        #[codec(index = 9)]
        PoolAlreadyExists,
        #[codec(index = 10)]
        CustodyCollision,
        #[codec(index = 11)]
        CustodyNotEmpty,
        #[codec(index = 12)]
        CustodyMismatch,
        #[codec(index = 13)]
        CustodyNotLive,
        #[codec(index = 14)]
        DeadlineExpired,
        #[codec(index = 15)]
        BoundExceeded,
        #[codec(index = 16)]
        ZeroAmount,
        #[codec(index = 17)]
        InsufficientInitialLiquidity,
        #[codec(index = 18)]
        InsufficientLiquidity,
        #[codec(index = 19)]
        InsufficientLp,
        #[codec(index = 20)]
        SlippageExceeded,
        #[codec(index = 21)]
        InvalidConfiguration,
        #[codec(index = 22)]
        BelowMinimum,
        #[codec(index = 23)]
        InsufficientBalance,
        #[codec(index = 24)]
        Frozen,
        #[codec(index = 25)]
        DepositFailure,
        #[codec(index = 26)]
        BackendRejected,
        #[codec(index = 27)]
        Reentrant,
        #[codec(index = 28)]
        TransactionLimit,
        #[codec(index = 29)]
        CorruptState,
    }
    pub type ApiResult<T> = core::result::Result<T, AmmErrorV1>;
    impl From<AssetError> for AmmErrorV1 {
        fn from(e: AssetError) -> Self {
            match e {
                AssetError::UnknownAsset => Self::NotFound,
                AssetError::UnsupportedAsset => Self::UnsupportedAsset,
                AssetError::ArithmeticOverflow => Self::Arithmetic,
                AssetError::AccountingInvariant => Self::BackendInvariant,
                AssetError::InsufficientBalance => Self::InsufficientBalance,
                AssetError::Frozen | AssetError::AccountFrozen => Self::Frozen,
                AssetError::BelowMinimum => Self::BelowMinimum,
                AssetError::AlreadyExists
                | AssetError::UnknownCollection
                | AssetError::UnknownItem
                | AssetError::NotOwner
                | AssetError::NotIssuer
                | AssetError::NotAdmin
                | AssetError::NotFreezer
                | AssetError::OwnershipBoundary
                | AssetError::MetadataTooLong
                | AssetError::NativeMutationProhibited
                | AssetError::BackendRejected => Self::BackendRejected,
            }
        }
    }
    impl From<Fault> for AmmErrorV1 {
        fn from(e: Fault) -> Self {
            match e {
                Fault::Unconfigured => Self::Unconfigured,
                Fault::Bound => Self::BoundExceeded,
                Fault::InvalidLimit => Self::InvalidLimit,
                Fault::InvalidCursor => Self::InvalidCursor,
                Fault::Deposit => Self::DepositFailure,
                Fault::Liveness => Self::CustodyNotLive,
                Fault::Reentrant => Self::Reentrant,
                Fault::CorruptState => Self::CorruptState,
                Fault::TransactionLimit => Self::TransactionLimit,
                Fault::Reference(e) => match e {
                    amm::Error::Asset(e) => e.into(),
                    amm::Error::InvalidConfiguration => Self::InvalidConfiguration,
                    amm::Error::IdenticalAssets => Self::IdenticalAssets,
                    amm::Error::Unauthorized => Self::Unauthorized,
                    amm::Error::PoolAlreadyExists => Self::PoolAlreadyExists,
                    amm::Error::PoolNotFound => Self::NotFound,
                    amm::Error::CustodyAccountCollision => Self::CustodyCollision,
                    amm::Error::CustodyNotEmpty => Self::CustodyNotEmpty,
                    amm::Error::CustodyBalanceMismatch => Self::CustodyMismatch,
                    amm::Error::DeadlineExpired => Self::DeadlineExpired,
                    amm::Error::ZeroAmount => Self::ZeroAmount,
                    amm::Error::InsufficientInitialLiquidity => Self::InsufficientInitialLiquidity,
                    amm::Error::InsufficientLiquidity => Self::InsufficientLiquidity,
                    amm::Error::InsufficientLpBalance => Self::InsufficientLp,
                    amm::Error::SlippageExceeded => Self::SlippageExceeded,
                    amm::Error::ArithmeticOverflow => Self::Arithmetic,
                    amm::Error::InvariantViolation => Self::BackendInvariant,
                    amm::Error::FeeRoutingFailed => Self::BackendRejected,
                },
            }
        }
    }
    /// Whole-input canonical SCALE decoding; semantic eligibility still belongs to the contract.
    pub fn decode_exact<T: DecodeAll + Encode + MaxEncodedLen>(bytes: &[u8]) -> ApiResult<T> {
        if bytes.len() > T::max_encoded_len() {
            return Err(AmmErrorV1::CorruptState);
        }
        let value = T::decode_all(&mut &bytes[..]).map_err(|_| AmmErrorV1::CorruptState)?;
        if value.encode() != bytes {
            return Err(AmmErrorV1::CorruptState);
        }
        Ok(value)
    }
    fn canonical_pair(id: PoolId) -> ApiResult<(Asset, Asset)> {
        if !id.canonical() {
            return Err(AmmErrorV1::CorruptState);
        }
        Ok(id.pair())
    }
    fn pool_id(id: amm::PoolId<u32>) -> ApiResult<PoolId> {
        let value = PoolId {
            asset_0: Asset::from_fungible(id.asset_0),
            asset_1: Asset::from_fungible(id.asset_1),
        };
        canonical_pair(value)?;
        Ok(value)
    }
    impl TryFrom<Event<AccountId32, u32>> for EventV1 {
        type Error = AmmErrorV1;
        fn try_from(e: Event<AccountId32, u32>) -> ApiResult<Self> {
            Ok(match e {
                Event::PoolCreated {
                    pool,
                    creator,
                    custody_account,
                } => Self::PoolCreated {
                    pool: pool_id(pool)?,
                    creator: creator.into(),
                    custody_account: custody_account.into(),
                },
                Event::LiquidityAdded {
                    pool,
                    provider,
                    amount_0,
                    amount_1,
                    lp_minted,
                } => Self::LiquidityAdded {
                    pool: pool_id(pool)?,
                    provider: provider.into(),
                    amount_0,
                    amount_1,
                    lp_minted,
                },
                Event::LiquidityRemoved {
                    pool,
                    provider,
                    recipient,
                    amount_0,
                    amount_1,
                    lp_burned,
                } => Self::LiquidityRemoved {
                    pool: pool_id(pool)?,
                    provider: provider.into(),
                    recipient: recipient.into(),
                    amount_0,
                    amount_1,
                    lp_burned,
                },
                Event::SwapExecuted {
                    pool,
                    trader,
                    recipient,
                    asset_in,
                    asset_out,
                    amount_in,
                    amount_out,
                    total_fee,
                    protocol_fee,
                    exact_output,
                } => {
                    let pool = pool_id(pool)?;
                    let asset_in = Asset::from_fungible(asset_in);
                    let asset_out = Asset::from_fungible(asset_out);
                    if protocol_fee != 0 || PoolId::new(asset_in, asset_out)? != pool {
                        return Err(AmmErrorV1::BackendInvariant);
                    }
                    Self::SwapExecuted {
                        pool,
                        trader: trader.into(),
                        recipient: recipient.into(),
                        asset_in,
                        asset_out,
                        amount_in,
                        amount_out,
                        total_fee,
                        protocol_fee,
                        exact_output,
                    }
                }
            })
        }
    }
    #[derive(Default)]
    pub struct SignedCreator;
    impl PoolCreationPolicy<AccountId32, u32> for SignedCreator {
        fn ensure_can_create(
            &self,
            _: &AccountId32,
            _: &amm::PoolId<u32>,
        ) -> core::result::Result<(), amm::Error> {
            Ok(())
        }
    }
    #[derive(Default)]
    pub struct CheckedCustody;
    impl PoolAccountDeriver<AccountId32, u32> for CheckedCustody {
        fn derive_pool_account(
            &self,
            id: &amm::PoolId<u32>,
        ) -> core::result::Result<AccountId32, amm::Error> {
            let id = pool_id(*id).map_err(|_| amm::Error::CustodyAccountCollision)?;
            PalletId(CUSTODY_DOMAIN)
                .try_into_sub_account((CUSTODY_VERSION, id))
                .ok_or(amm::Error::CustodyAccountCollision)
        }
    }
    /// This bound fixes every adopted economic/encoding parameter without configuring production.
    pub trait ContractConfig:
        Config<
        AccountId = AccountId32,
        Asset = Asset,
        Record = PoolRecord,
        PoolHasher = Blake2_128Concat,
        AccountHasher = Blake2_128Concat,
        Creator = SignedCreator,
        Custody = CheckedCustody,
        Math = WideArithmetic,
        Fee = Fee,
        FeeLimit = FeeLimit,
        LockedLiquidity = LockedLiquidity,
        MinimumPosition = MinimumPosition,
        MinimumTrade = MinimumTrade,
        CreationDeposit = CreationDeposit,
        Horizon = Horizon,
        MaxPools = MaxPools,
        MaxPositions = MaxPositions,
        MaxProviders = MaxProviders,
        MaxPage = MaxPage,
    >
    {
    }
    impl<T> ContractConfig for T where
        T: Config<
            AccountId = AccountId32,
            Asset = Asset,
            Record = PoolRecord,
            PoolHasher = Blake2_128Concat,
            AccountHasher = Blake2_128Concat,
            Creator = SignedCreator,
            Custody = CheckedCustody,
            Math = WideArithmetic,
            Fee = Fee,
            FeeLimit = FeeLimit,
            LockedLiquidity = LockedLiquidity,
            MinimumPosition = MinimumPosition,
            MinimumTrade = MinimumTrade,
            CreationDeposit = CreationDeposit,
            Horizon = Horizon,
            MaxPools = MaxPools,
            MaxPositions = MaxPositions,
            MaxProviders = MaxProviders,
            MaxPage = MaxPage,
        >
    {
    }

    /// Ordinary Rust source adapter, not Dispatchable, RuntimeCall or a registered extrinsic.
    pub fn execute_signed<T: ContractConfig>(
        origin: T::RuntimeOrigin,
        call: CallV1,
    ) -> ApiResult<()> {
        let who = frame_system::ensure_signed(origin).map_err(|_| AmmErrorV1::Unauthorized)?;
        match call {
            CallV1::CreatePool {
                asset_a,
                asset_b,
                deadline,
            } => Pallet::<T>::create(&who, asset_a, asset_b, deadline.into()).map(|_| ()),
            CallV1::AddLiquidity {
                asset_a,
                asset_b,
                desired_a,
                desired_b,
                min_a,
                min_b,
                deadline,
            } => Pallet::<T>::add(
                &who,
                (asset_a, asset_b),
                [desired_a, desired_b],
                [min_a, min_b],
                deadline.into(),
            )
            .map(|_| ()),
            CallV1::RemoveLiquidity {
                asset_a,
                asset_b,
                lp,
                min_a,
                min_b,
                recipient,
                deadline,
            } => Pallet::<T>::remove(
                &who,
                (asset_a, asset_b),
                lp,
                [min_a, min_b],
                &recipient.into(),
                deadline.into(),
            )
            .map(|_| ()),
            CallV1::SwapExactInput {
                asset_in,
                asset_out,
                amount_in,
                min_out,
                recipient,
                deadline,
            } => Pallet::<T>::swap(
                &who,
                (asset_in, asset_out),
                amount_in,
                min_out,
                &recipient.into(),
                deadline.into(),
                false,
            )
            .map(|_| ()),
            CallV1::SwapExactOutput {
                asset_in,
                asset_out,
                amount_out,
                max_in,
                recipient,
                deadline,
            } => Pallet::<T>::swap(
                &who,
                (asset_in, asset_out),
                amount_out,
                max_in,
                &recipient.into(),
                deadline.into(),
                true,
            )
            .map(|_| ()),
        }
        .map_err(Into::into)
    }
    pub trait EraV14AmmApiV1 {
        fn pool_v1(pool: PoolId) -> ApiResult<PoolV1>;
        fn pools_v1(cursor: Option<PoolId>, limit: u32) -> ApiResult<PageV1<PoolV1>>;
        fn lp_position_v1(pool: PoolId, account: AccountId) -> ApiResult<LpPositionV1>;
        fn positions_v1(
            account: AccountId,
            cursor: Option<PoolId>,
            limit: u32,
        ) -> ApiResult<PageV1<LpPositionV1>>;
        fn quote_exact_input_v1(
            asset_in: Asset,
            asset_out: Asset,
            amount_in: u128,
        ) -> ApiResult<QuoteV1>;
        fn quote_exact_output_v1(
            asset_in: Asset,
            asset_out: Asset,
            amount_out: u128,
        ) -> ApiResult<QuoteV1>;
    }
    pub struct Api<T>(core::marker::PhantomData<T>);
    impl<T: ContractConfig> Api<T> {
        fn cursor(cursor: Option<PoolId>, limit: u32) -> ApiResult<Option<(Asset, Asset)>> {
            if limit == 0 || limit > <MaxPage as Get<u32>>::get() {
                return Err(AmmErrorV1::InvalidLimit);
            }
            cursor
                .map(|c| canonical_pair(c).map_err(|_| AmmErrorV1::InvalidCursor))
                .transpose()
        }
        fn quote(assets: (Asset, Asset), amount: u128, exact: bool) -> ApiResult<QuoteV1> {
            let (input, output, fee) =
                Pallet::<T>::quote(assets, amount, exact).map_err(AmmErrorV1::from)?;
            Ok(QuoteV1 {
                pool: PoolId::new(assets.0, assets.1)?,
                asset_in: assets.0,
                asset_out: assets.1,
                amount_in: input,
                amount_out: output,
                total_fee: fee,
                protocol_fee: 0,
            })
        }
    }
    impl<T: ContractConfig> EraV14AmmApiV1 for Api<T> {
        fn pool_v1(pool: PoolId) -> ApiResult<PoolV1> {
            let p = Pallet::<T>::pool(canonical_pair(pool)?).map_err(AmmErrorV1::from)?;
            Ok(PoolV1 {
                pool,
                record: p.into(),
            })
        }
        fn pools_v1(cursor: Option<PoolId>, limit: u32) -> ApiResult<PageV1<PoolV1>> {
            let (pairs, next) = Pallet::<T>::pools(Self::cursor(cursor, limit)?, limit)
                .map_err(AmmErrorV1::from)?;
            let mut entries = BoundedVec::default();
            for (a, b) in pairs {
                entries
                    .try_push(Self::pool_v1(PoolId::new(a, b)?)?)
                    .map_err(|_| AmmErrorV1::BackendInvariant)?;
            }
            Ok(PageV1 {
                entries,
                next: next.map(|(a, b)| PoolId::new(a, b)).transpose()?,
            })
        }
        fn lp_position_v1(pool: PoolId, account: AccountId) -> ApiResult<LpPositionV1> {
            let lp = Pallet::<T>::lp(canonical_pair(pool)?, &account.into())
                .map_err(AmmErrorV1::from)?;
            Ok(LpPositionV1 { pool, account, lp })
        }
        fn positions_v1(
            account: AccountId,
            cursor: Option<PoolId>,
            limit: u32,
        ) -> ApiResult<PageV1<LpPositionV1>> {
            let cursor = Self::cursor(cursor, limit)?;
            let (pairs, next) =
                Pallet::<T>::positions(&account.into(), cursor, limit).map_err(AmmErrorV1::from)?;
            let mut entries = BoundedVec::default();
            for (a, b) in pairs {
                entries
                    .try_push(Self::lp_position_v1(PoolId::new(a, b)?, account)?)
                    .map_err(|_| AmmErrorV1::BackendInvariant)?;
            }
            Ok(PageV1 {
                entries,
                next: next.map(|(a, b)| PoolId::new(a, b)).transpose()?,
            })
        }
        fn quote_exact_input_v1(a: Asset, b: Asset, amount: u128) -> ApiResult<QuoteV1> {
            Self::quote((a, b), amount, false)
        }
        fn quote_exact_output_v1(a: Asset, b: Asset, amount: u128) -> ApiResult<QuoteV1> {
            Self::quote((a, b), amount, true)
        }
    }
}

// Registered runtime APIs for the integrated final V14 candidate. No write interface.
sp_api::decl_runtime_apis! {
    pub trait EraV14AmmRuntimeApi {
        fn pool_v1(pool:v1::PoolId)->v1::ApiResult<v1::PoolV1>;
        fn pools_v1(cursor:Option<v1::PoolId>,limit:u32)->v1::ApiResult<v1::PageV1<v1::PoolV1>>;
        fn lp_position_v1(pool:v1::PoolId,account:v1::AccountId)->v1::ApiResult<v1::LpPositionV1>;
        fn positions_v1(account:v1::AccountId,cursor:Option<v1::PoolId>,limit:u32)->v1::ApiResult<v1::PageV1<v1::LpPositionV1>>;
        fn quote_exact_input_v1(asset_in:v1::Asset,asset_out:v1::Asset,amount:u128)->v1::ApiResult<v1::QuoteV1>;
        fn quote_exact_output_v1(asset_in:v1::Asset,asset_out:v1::Asset,amount:u128)->v1::ApiResult<v1::QuoteV1>;
    }
}
