//! Synthetic fixtures only. None of the numeric values, domains or SCALE layouts here is policy.
use super::*;
use codec::{Decode, DecodeWithMemTracking, Encode};
use era_v14_application_primitives::assets::FungibleInspect;
use frame_support::{construct_runtime, derive_impl, parameter_types, traits::ConstU32, PalletId};
use sp_runtime::{traits::AccountIdConversion, AccountId32, BuildStorage};

pub type Account = AccountId32;
pub fn account(id: u8) -> Account {
    Account::new([id; 32])
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
    Native,
    Registered(u32),
    Unique(u32, u32),
}
impl AssetKey for Asset {
    fn fungible(self) -> Result<FungibleAsset<u32>> {
        match self {
            Self::Native => Ok(FungibleAsset::NativeEtkn),
            Self::Registered(id) => Ok(FungibleAsset::Registered(id)),
            Self::Unique(..) => Err(AssetError::UnsupportedAsset.into()),
        }
    }
    fn from_fungible(a: FungibleAsset<u32>) -> Self {
        match a {
            FungibleAsset::NativeEtkn => Self::Native,
            FungibleAsset::Registered(id) => Self::Registered(id),
        }
    }
}
#[derive(
    Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo,
)]
pub struct Record<A> {
    pub custody: A,
    pub creator: A,
    pub deposit: u128,
    pub reserves: [u128; 2],
    pub total_lp: u128,
    pub locked_lp: u128,
    pub user_lp: u128,
}
impl<A> From<Snapshot<A>> for Record<A> {
    fn from(p: Snapshot<A>) -> Self {
        Self {
            custody: p.custody,
            creator: p.creator,
            deposit: p.deposit,
            reserves: p.reserves,
            total_lp: p.total_lp,
            locked_lp: p.locked_lp,
            user_lp: p.user_lp,
        }
    }
}
impl<A> From<Record<A>> for Snapshot<A> {
    fn from(p: Record<A>) -> Self {
        Self {
            custody: p.custody,
            creator: p.creator,
            deposit: p.deposit,
            reserves: p.reserves,
            total_lp: p.total_lp,
            locked_lp: p.locked_lp,
            user_lp: p.user_lp,
        }
    }
}
construct_runtime!(pub enum Test { System: frame_system, Amm: crate });
#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
    type Block = frame_system::mocking::MockBlock<Self>;
    type AccountId = Account;
    type Lookup = sp_runtime::traits::IdentityLookup<Account>;
}
parameter_types! {
    pub Fee: Rate = Rate::new(3, 1000).unwrap();
    pub FeeLimit: Rate = Rate::new(1, 100).unwrap();
    pub Lock: u128 = 10;
    pub PositionMinimum: u128 = 1;
    pub TradeMinimum: u128 = 1;
    pub Deposit: u128 = 7;
    pub Horizon: u64 = 20;
}
impl Config for Test {
    type Asset = Asset;
    type Record = Record<Account>;
    type PoolHasher = Blake2_128Concat;
    type AccountHasher = Blake2_128Concat;
    type Assets = Ledger;
    type Admission = Registry;
    type Creator = Creator;
    type Custody = Custody;
    type Events = Notices;
    type Math = FixtureArithmetic;
    type Clock = Clock;
    type Fee = Fee;
    type FeeLimit = FeeLimit;
    type LockedLiquidity = Lock;
    type MinimumPosition = PositionMinimum;
    type MinimumTrade = TradeMinimum;
    type CreationDeposit = Deposit;
    type Horizon = Horizon;
    type MaxPools = ConstU32<3>;
    type MaxPositions = ConstU32<2>;
    type MaxProviders = ConstU32<3>;
    type MaxPage = ConstU32<2>;
}
// All fixture state is in externalities so failure snapshots also cover injected side effects.
pub fn put<V: Encode>(key: &[u8], value: V) {
    sp_io::storage::set(key, &value.encode());
}
pub fn get<V: Decode + Default>(key: &[u8]) -> V {
    sp_io::storage::get(key)
        .map(|v| V::decode(&mut &v[..]).unwrap())
        .unwrap_or_default()
}
fn key(kind: u8, asset: Asset, who: &Account) -> Vec<u8> {
    (b"synthetic-amm", kind, asset, who).encode()
}
pub fn balance(asset: Asset, who: &Account) -> u128 {
    get(&key(0, asset, who))
}
pub fn seed(asset: Asset, who: &Account, value: u128) {
    let old = balance(asset, who);
    let supply: u128 = get(&supply_key(asset));
    put(&key(0, asset, who), value);
    put(
        &supply_key(asset),
        supply.checked_sub(old).unwrap().checked_add(value).unwrap(),
    );
    put(&key(2, Asset::Native, who), true);
}
fn supply_key(asset: Asset) -> Vec<u8> {
    (b"synthetic-supply", asset).encode()
}
pub fn conflicting_identity(who: &Account) {
    put(&key(3, Asset::Native, who), true);
}
pub fn event() {
    put(b"synthetic-events", get::<u32>(b"synthetic-events") + 1);
}
pub struct Clock;
impl Get<u64> for Clock {
    fn get() -> u64 {
        get(b"synthetic-now")
    }
}
pub struct Registry;
impl Admission for Registry {
    fn ensure(pair: PoolId<u32>) -> Result<()> {
        if !get::<bool>(b"synthetic-admitted") {
            return Err(Fault::Unconfigured);
        }
        for a in [pair.asset_0, pair.asset_1] {
            if matches!(a, FungibleAsset::Registered(id) if !(1..=3).contains(&id)) {
                return Err(AssetError::UnsupportedAsset.into());
            }
        }
        Ok(())
    }
}
#[derive(Default, Clone)]
pub struct Creator;
impl PoolCreationPolicy<Account, u32> for Creator {
    fn ensure_can_create(
        &self,
        who: &Account,
        _: &PoolId<u32>,
    ) -> core::result::Result<(), amm::Error> {
        if *who != account(1) {
            return Err(amm::Error::Unauthorized);
        }
        Ok(())
    }
}
#[derive(Default, Clone)]
pub struct Custody;
impl PoolAccountDeriver<Account, u32> for Custody {
    fn derive_pool_account(&self, pair: &PoolId<u32>) -> core::result::Result<Account, amm::Error> {
        if let Some(value) = sp_io::storage::get(b"synthetic-custody-override") {
            return Ok(Account::decode(&mut &value[..]).unwrap());
        }
        PalletId(*b"tst/amm0")
            .try_into_sub_account((
                9u8,
                Asset::from_fungible(pair.asset_0),
                Asset::from_fungible(pair.asset_1),
            ))
            .ok_or(amm::Error::CustodyAccountCollision)
    }
}
#[derive(Default, Clone)]
pub struct Ledger;
impl FungibleInspect<Account> for Ledger {
    type AssetId = u32;
    fn balance(&self, asset: FungibleAsset<u32>, who: &Account) -> u128 {
        balance(Asset::from_fungible(asset), who)
    }
    fn total_issuance(&self, asset: FungibleAsset<u32>) -> core::result::Result<u128, AssetError> {
        Ok(get(&supply_key(Asset::from_fungible(asset))))
    }
}
impl FungibleTransfer<Account> for Ledger {
    fn transfer(
        &mut self,
        asset: FungibleAsset<u32>,
        from: &Account,
        to: &Account,
        amount: u128,
    ) -> core::result::Result<(), AssetError> {
        let asset = Asset::from_fungible(asset);
        let before = balance(asset, from);
        let target = balance(asset, to);
        let next = before
            .checked_sub(amount)
            .ok_or(AssetError::InsufficientBalance)?;
        if next < self.minimum(asset.fungible().unwrap())? {
            return Err(AssetError::BelowMinimum);
        }
        put(&key(0, asset, from), next);
        put(
            &key(0, asset, to),
            target
                .checked_add(amount)
                .ok_or(AssetError::ArithmeticOverflow)?,
        );
        event();
        let count = get::<u32>(b"synthetic-transfers") + 1;
        put(b"synthetic-transfers", count);
        if count == get::<u32>(b"synthetic-fail-transfer") {
            return Err(AssetError::BackendRejected);
        }
        match get::<u8>(b"synthetic-malicious") {
            1 => put(&key(0, asset, to), target + amount - 1), // transfer fee
            2 => put(
                &supply_key(asset),
                self.total_issuance(asset.fungible().unwrap())? + 1,
            ), // rebase
            3 => {
                assert_eq!(
                    Amm::create(&account(1), Asset::Native, Asset::Registered(3), 100),
                    Err(Fault::Reentrant)
                );
                return Err(AssetError::BackendRejected);
            }
            _ => (),
        }
        Ok(())
    }
}
impl Backend<Account> for Ledger {
    fn minimum(&self, _: FungibleAsset<u32>) -> core::result::Result<u128, AssetError> {
        Ok(1)
    }
    fn reserved_native(&self, who: &Account) -> u128 {
        get(&key(1, Asset::Native, who))
    }
    fn reserve_native(
        &mut self,
        who: &Account,
        amount: u128,
    ) -> core::result::Result<(), AssetError> {
        let free = balance(Asset::Native, who)
            .checked_sub(amount)
            .ok_or(AssetError::InsufficientBalance)?;
        put(&key(0, Asset::Native, who), free);
        put(
            &key(1, Asset::Native, who),
            self.reserved_native(who) + amount,
        );
        event();
        if get::<bool>(b"synthetic-bad-reserve") {
            return Err(AssetError::BackendRejected);
        }
        Ok(())
    }
    fn identity_conflict(&self, who: &Account) -> bool {
        get(&key(3, Asset::Native, who))
    }
    fn providers(&self, who: &Account) -> u32 {
        get(&key(4, Asset::Native, who))
    }
    fn reducible(&self, asset: FungibleAsset<u32>, who: &Account) -> u128 {
        match self.balance(asset, who) {
            0 => 0,
            balance => balance - 1,
        }
    }
    fn establish(&mut self, who: &Account) -> core::result::Result<(), AssetError> {
        if !get::<bool>(b"synthetic-no-provider") {
            put(
                &key(4, Asset::Native, who),
                self.providers(who)
                    .checked_add(1)
                    .ok_or(AssetError::ArithmeticOverflow)?,
            );
        }
        Ok(())
    }
}
pub struct Notices;
impl EventSink<Account> for Notices {
    fn emit(event_value: Event<Account, u32>) -> Result<()> {
        // Preserve the full payload for root-based rollback assertions, using fixture text only.
        let mut events: Vec<Vec<u8>> = get(b"synthetic-notices");
        events.push(alloc::format!("{event_value:?}").into_bytes());
        put(b"synthetic-notices", events);
        event();
        if get::<bool>(b"synthetic-fail-event") {
            return Err(Fault::CorruptState);
        }
        Ok(())
    }
}
pub fn ext() -> sp_io::TestExternalities {
    let storage = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .unwrap();
    let mut ext = sp_io::TestExternalities::new(storage);
    ext.execute_with(|| {
        put(b"synthetic-now", 100u64);
        put(b"synthetic-admitted", true);
        for who in 1..=5 {
            for asset in [
                Asset::Native,
                Asset::Registered(1),
                Asset::Registered(2),
                Asset::Registered(3),
            ] {
                seed(asset, &account(who), 1_000_000);
            }
        }
    });
    ext
}
pub fn root() -> Vec<u8> {
    sp_io::storage::root(sp_runtime::StateVersion::V1)
}
pub fn initialized() -> (Asset, Asset) {
    let pair = (Asset::Native, Asset::Registered(1));
    Amm::create(&account(1), pair.0, pair.1, 100).unwrap();
    Amm::add(&account(1), pair, [10_000, 10_000], [0, 0], 100).unwrap();
    pair
}

/// Runtime/primitive integration can explicitly exercise either width in disposable fixtures.
pub struct FixtureArithmetic;
impl Arithmetic for FixtureArithmetic {
    fn validate_reserves(r: [u128; 2]) -> Result<()> {
        if get::<bool>(b"synthetic-wide") {
            WideArithmetic::validate_reserves(r)
        } else {
            ReferenceArithmetic::validate_reserves(r)
        }
    }
    fn invariant(a: [u128; 2], b: [u128; 2]) -> Result<()> {
        if get::<bool>(b"synthetic-wide") {
            WideArithmetic::invariant(a, b)
        } else {
            ReferenceArithmetic::invariant(a, b)
        }
    }
    fn quote(r: [u128; 2], a: u128, fee: Rate, exact: bool) -> Result<(u128, u128, u128)> {
        if get::<bool>(b"synthetic-wide") {
            WideArithmetic::quote(r, a, fee, exact)
        } else {
            ReferenceArithmetic::quote(r, a, fee, exact)
        }
    }
    fn add(r: [u128; 2], s: u128, d: [u128; 2], l: u128) -> Result<([u128; 2], u128, u128)> {
        if get::<bool>(b"synthetic-wide") {
            WideArithmetic::add(r, s, d, l)
        } else {
            ReferenceArithmetic::add(r, s, d, l)
        }
    }
    fn remove(r: [u128; 2], s: u128, l: u128) -> Result<[u128; 2]> {
        if get::<bool>(b"synthetic-wide") {
            WideArithmetic::remove(r, s, l)
        } else {
            ReferenceArithmetic::remove(r, s, l)
        }
    }
}
