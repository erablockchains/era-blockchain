//! Transfer-only interfaces and the adopted dormant V14 asset contract.
//!
//! The authoritative integrated policy is [`v1`] and the runtime SDK adapter. ReferenceLedger
//! retains historical WS45 fixture behavior only (including different thaw/metadata rules and
//! reversible collection freezes). It is excluded from normal builds and is not SDK conformance.

#[cfg(any(test, feature = "reference-fixtures"))]
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec::Vec;

/// Adopted integrated contract. Historical reference-ledger behavior below is fixture-only.
pub mod v1;

/// All module-local quantities use checked unsigned base-unit arithmetic.
pub type Balance = u128;

/// A fungible identifier cannot alias native ETKN with a registered application asset.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum FungibleAsset<AssetId> {
    /// Native ETKN. This interface exposes transfers only; it exposes no native mint or burn path.
    NativeEtkn,
    /// A non-native application asset managed by the asset backend.
    Registered(AssetId),
}

/// Explicit, integration-supplied metadata bounds. There is deliberately no default policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InterfaceLimits {
    pub max_asset_name_bytes: u32,
    pub max_asset_symbol_bytes: u32,
    pub max_collection_metadata_bytes: u32,
    pub max_item_metadata_bytes: u32,
}

/// A caller-supplied namespace. This helper adopts no range, reserved ID, or storage key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IdRange {
    first: u32,
    last: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AllocationError {
    InvalidRange,
    OutOfRange,
    Exhausted,
    Collision,
}

/// A pure plan, not an allocation. A future caller must atomically commit both backend creation
/// and `next`, after checking authorization and the unchanged cursor. `None` records exhaustion;
/// it must never mean "initialize again". Failed creation must leave the cursor untouched.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AllocationPlan {
    pub id: u32,
    pub next: Option<u32>,
}

impl IdRange {
    pub fn new(first: u32, last: u32) -> Result<Self, AllocationError> {
        if first > last {
            return Err(AllocationError::InvalidRange);
        }
        Ok(Self { first, last })
    }

    pub fn contains(&self, id: u32) -> bool {
        (self.first..=self.last).contains(&id)
    }

    pub fn overlaps(&self, other: &Self) -> bool {
        self.first <= other.last && other.first <= self.last
    }

    /// Exactly one collision lookup, with no scan, wraparound, reuse, or fallback selection.
    pub fn prepare(
        &self,
        next: Option<u32>,
        occupied: impl FnOnce(u32) -> bool,
    ) -> Result<AllocationPlan, AllocationError> {
        let id = next.ok_or(AllocationError::Exhausted)?;
        if !self.contains(id) {
            return Err(AllocationError::OutOfRange);
        }
        if occupied(id) {
            return Err(AllocationError::Collision);
        }
        Ok(AllocationPlan {
            id,
            next: if id == self.last {
                None
            } else {
                id.checked_add(1)
            },
        })
    }
}

/// Bounded fungible metadata.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetMetadata {
    pub name: Vec<u8>,
    pub symbol: Vec<u8>,
    pub decimals: u8,
}

/// Asset duties remain separately assignable instead of silently following ownership.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetRoles<Account> {
    pub owner: Account,
    pub issuer: Account,
    pub admin: Account,
    pub freezer: Account,
}

/// Collection duties mirror the permission boundaries needed by unique-item backends.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CollectionRoles<Account> {
    pub owner: Account,
    pub issuer: Account,
    pub admin: Account,
    pub freezer: Account,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetDetails<Account> {
    pub roles: AssetRoles<Account>,
    pub metadata: AssetMetadata,
    pub supply: Balance,
    pub frozen: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CollectionDetails<Account> {
    pub roles: CollectionRoles<Account>,
    pub metadata: Vec<u8>,
    pub frozen: bool,
    pub items: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UniqueDetails<Account> {
    pub owner: Account,
    pub metadata: Vec<u8>,
    pub frozen: bool,
}

/// Stable module-local events; runtime event encoding and pallet indices remain integration-owned.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event<Account, AssetId, CollectionId, ItemId> {
    AssetCreated {
        asset: AssetId,
        owner: Account,
    },
    AssetMetadataSet {
        asset: AssetId,
    },
    AssetOwnershipTransferred {
        asset: AssetId,
        old_owner: Account,
        new_owner: Account,
    },
    AssetRolesSet {
        asset: AssetId,
    },
    AssetMinted {
        asset: AssetId,
        beneficiary: Account,
        amount: Balance,
    },
    AssetBurned {
        asset: AssetId,
        account: Account,
        amount: Balance,
    },
    AssetTransferred {
        asset: AssetId,
        from: Account,
        to: Account,
        amount: Balance,
    },
    AssetFrozen {
        asset: AssetId,
    },
    AssetThawed {
        asset: AssetId,
    },
    AssetAccountFrozen {
        asset: AssetId,
        account: Account,
    },
    AssetAccountThawed {
        asset: AssetId,
        account: Account,
    },
    CollectionCreated {
        collection: CollectionId,
        owner: Account,
    },
    CollectionMetadataSet {
        collection: CollectionId,
    },
    CollectionOwnershipTransferred {
        collection: CollectionId,
        old_owner: Account,
        new_owner: Account,
    },
    CollectionRolesSet {
        collection: CollectionId,
    },
    CollectionFrozen {
        collection: CollectionId,
    },
    CollectionThawed {
        collection: CollectionId,
    },
    UniqueMinted {
        collection: CollectionId,
        item: ItemId,
        owner: Account,
    },
    UniqueBurned {
        collection: CollectionId,
        item: ItemId,
        owner: Account,
    },
    UniqueTransferred {
        collection: CollectionId,
        item: ItemId,
        from: Account,
        to: Account,
    },
    UniqueFrozen {
        collection: CollectionId,
        item: ItemId,
    },
    UniqueThawed {
        collection: CollectionId,
        item: ItemId,
    },
    UniqueMetadataSet {
        collection: CollectionId,
        item: ItemId,
    },
}

/// Stable errors used by callers to fail before any partial state is committed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    AlreadyExists,
    UnknownAsset,
    UnknownCollection,
    UnknownItem,
    NotOwner,
    NotIssuer,
    NotAdmin,
    NotFreezer,
    OwnershipBoundary,
    Frozen,
    AccountFrozen,
    MetadataTooLong,
    InsufficientBalance,
    ArithmeticOverflow,
    AccountingInvariant,
    NativeMutationProhibited,
    UnsupportedAsset,
    BelowMinimum,
    BackendRejected,
}

/// Read-only half of the interface consumed by application modules such as the V14 AMM.
pub trait FungibleInspect<Account> {
    type AssetId: Copy + Ord;

    fn balance(&self, asset: FungibleAsset<Self::AssetId>, account: &Account) -> Balance;
    fn total_issuance(&self, asset: FungibleAsset<Self::AssetId>) -> Result<Balance, Error>;
}

/// Transfer-only half of the interface. No native issuance operation is exposed.
pub trait FungibleTransfer<Account>: FungibleInspect<Account> {
    fn transfer(
        &mut self,
        asset: FungibleAsset<Self::AssetId>,
        from: &Account,
        to: &Account,
        amount: Balance,
    ) -> Result<(), Error>;
}

/// Historical module fixture, NOT the integrated SDK policy or runtime conformance oracle.
#[cfg(any(test, feature = "reference-fixtures"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReferenceLedger<Account, AssetId, CollectionId, ItemId> {
    limits: InterfaceLimits,
    assets: BTreeMap<AssetId, AssetDetails<Account>>,
    balances: BTreeMap<(AssetId, Account), Balance>,
    frozen_accounts: BTreeSet<(AssetId, Account)>,
    collections: BTreeMap<CollectionId, CollectionDetails<Account>>,
    uniques: BTreeMap<(CollectionId, ItemId), UniqueDetails<Account>>,
    native_balances: BTreeMap<Account, Balance>,
    native_issuance: Balance,
    events: Vec<Event<Account, AssetId, CollectionId, ItemId>>,
}

#[cfg(any(test, feature = "reference-fixtures"))]
impl<Account, AssetId, CollectionId, ItemId> ReferenceLedger<Account, AssetId, CollectionId, ItemId>
where
    Account: Clone + Ord,
    AssetId: Copy + Ord,
    CollectionId: Copy + Ord,
    ItemId: Copy + Ord,
{
    pub fn new(limits: InterfaceLimits) -> Self {
        Self {
            limits,
            assets: BTreeMap::new(),
            balances: BTreeMap::new(),
            frozen_accounts: BTreeSet::new(),
            collections: BTreeMap::new(),
            uniques: BTreeMap::new(),
            native_balances: BTreeMap::new(),
            native_issuance: 0,
            events: Vec::new(),
        }
    }

    pub fn limits(&self) -> InterfaceLimits {
        self.limits
    }

    pub fn events(&self) -> &[Event<Account, AssetId, CollectionId, ItemId>] {
        &self.events
    }

    pub fn asset(&self, asset: AssetId) -> Option<&AssetDetails<Account>> {
        self.assets.get(&asset)
    }

    pub fn collection(&self, collection: CollectionId) -> Option<&CollectionDetails<Account>> {
        self.collections.get(&collection)
    }

    pub fn unique(
        &self,
        collection: CollectionId,
        item: ItemId,
    ) -> Option<&UniqueDetails<Account>> {
        self.uniques.get(&(collection, item))
    }

    pub fn create_asset(
        &mut self,
        origin: &Account,
        asset: AssetId,
        roles: AssetRoles<Account>,
        metadata: AssetMetadata,
    ) -> Result<(), Error> {
        if self.assets.contains_key(&asset) {
            return Err(Error::AlreadyExists);
        }
        if roles.owner != *origin {
            return Err(Error::OwnershipBoundary);
        }
        self.ensure_asset_metadata(&metadata)?;
        let owner = roles.owner.clone();
        self.assets.insert(
            asset,
            AssetDetails {
                roles,
                metadata,
                supply: 0,
                frozen: false,
            },
        );
        self.events.push(Event::AssetCreated { asset, owner });
        Ok(())
    }

    pub fn set_asset_metadata(
        &mut self,
        origin: &Account,
        asset: AssetId,
        metadata: AssetMetadata,
    ) -> Result<(), Error> {
        self.ensure_asset_metadata(&metadata)?;
        let details = self.assets.get_mut(&asset).ok_or(Error::UnknownAsset)?;
        if details.roles.owner != *origin {
            return Err(Error::NotOwner);
        }
        details.metadata = metadata;
        self.events.push(Event::AssetMetadataSet { asset });
        Ok(())
    }

    pub fn transfer_asset_ownership(
        &mut self,
        origin: &Account,
        asset: AssetId,
        new_owner: Account,
    ) -> Result<(), Error> {
        let details = self.assets.get_mut(&asset).ok_or(Error::UnknownAsset)?;
        if details.roles.owner != *origin {
            return Err(Error::NotOwner);
        }
        let old_owner = details.roles.owner.clone();
        details.roles.owner = new_owner.clone();
        self.events.push(Event::AssetOwnershipTransferred {
            asset,
            old_owner,
            new_owner,
        });
        Ok(())
    }

    pub fn set_asset_roles(
        &mut self,
        origin: &Account,
        asset: AssetId,
        roles: AssetRoles<Account>,
    ) -> Result<(), Error> {
        let details = self.assets.get_mut(&asset).ok_or(Error::UnknownAsset)?;
        if details.roles.owner != *origin {
            return Err(Error::NotOwner);
        }
        if roles.owner != details.roles.owner {
            return Err(Error::OwnershipBoundary);
        }
        details.roles = roles;
        self.events.push(Event::AssetRolesSet { asset });
        Ok(())
    }

    pub fn mint_asset(
        &mut self,
        origin: &Account,
        asset: AssetId,
        beneficiary: &Account,
        amount: Balance,
    ) -> Result<(), Error> {
        let details = self.assets.get(&asset).ok_or(Error::UnknownAsset)?;
        if details.roles.issuer != *origin {
            return Err(Error::NotIssuer);
        }
        if details.frozen {
            return Err(Error::Frozen);
        }
        if self.frozen_accounts.contains(&(asset, beneficiary.clone())) {
            return Err(Error::AccountFrozen);
        }
        let new_supply = details
            .supply
            .checked_add(amount)
            .ok_or(Error::ArithmeticOverflow)?;
        let old_balance = self.registered_balance(asset, beneficiary);
        let new_balance = old_balance
            .checked_add(amount)
            .ok_or(Error::ArithmeticOverflow)?;
        self.assets
            .get_mut(&asset)
            .ok_or(Error::UnknownAsset)?
            .supply = new_supply;
        self.set_registered_balance(asset, beneficiary, new_balance);
        self.events.push(Event::AssetMinted {
            asset,
            beneficiary: beneficiary.clone(),
            amount,
        });
        Ok(())
    }

    pub fn burn_asset(
        &mut self,
        origin: &Account,
        asset: AssetId,
        account: &Account,
        amount: Balance,
    ) -> Result<(), Error> {
        let details = self.assets.get(&asset).ok_or(Error::UnknownAsset)?;
        if details.roles.admin != *origin {
            return Err(Error::NotAdmin);
        }
        if details.frozen {
            return Err(Error::Frozen);
        }
        if self.frozen_accounts.contains(&(asset, account.clone())) {
            return Err(Error::AccountFrozen);
        }
        let old_balance = self.registered_balance(asset, account);
        let new_balance = old_balance
            .checked_sub(amount)
            .ok_or(Error::InsufficientBalance)?;
        let new_supply = details
            .supply
            .checked_sub(amount)
            .ok_or(Error::AccountingInvariant)?;
        self.assets
            .get_mut(&asset)
            .ok_or(Error::UnknownAsset)?
            .supply = new_supply;
        self.set_registered_balance(asset, account, new_balance);
        self.events.push(Event::AssetBurned {
            asset,
            account: account.clone(),
            amount,
        });
        Ok(())
    }

    pub fn transfer_asset(
        &mut self,
        origin: &Account,
        asset: AssetId,
        to: &Account,
        amount: Balance,
    ) -> Result<(), Error> {
        let details = self.assets.get(&asset).ok_or(Error::UnknownAsset)?;
        if details.frozen {
            return Err(Error::Frozen);
        }
        if self.frozen_accounts.contains(&(asset, origin.clone()))
            || self.frozen_accounts.contains(&(asset, to.clone()))
        {
            return Err(Error::AccountFrozen);
        }
        self.transfer_registered_unchecked(asset, origin, to, amount)?;
        self.events.push(Event::AssetTransferred {
            asset,
            from: origin.clone(),
            to: to.clone(),
            amount,
        });
        Ok(())
    }

    pub fn freeze_asset(&mut self, origin: &Account, asset: AssetId) -> Result<(), Error> {
        let details = self.assets.get_mut(&asset).ok_or(Error::UnknownAsset)?;
        if details.roles.freezer != *origin {
            return Err(Error::NotFreezer);
        }
        details.frozen = true;
        self.events.push(Event::AssetFrozen { asset });
        Ok(())
    }

    pub fn thaw_asset(&mut self, origin: &Account, asset: AssetId) -> Result<(), Error> {
        let details = self.assets.get_mut(&asset).ok_or(Error::UnknownAsset)?;
        if details.roles.freezer != *origin {
            return Err(Error::NotFreezer);
        }
        details.frozen = false;
        self.events.push(Event::AssetThawed { asset });
        Ok(())
    }

    pub fn freeze_asset_account(
        &mut self,
        origin: &Account,
        asset: AssetId,
        account: &Account,
    ) -> Result<(), Error> {
        let details = self.assets.get(&asset).ok_or(Error::UnknownAsset)?;
        if details.roles.freezer != *origin {
            return Err(Error::NotFreezer);
        }
        self.frozen_accounts.insert((asset, account.clone()));
        self.events.push(Event::AssetAccountFrozen {
            asset,
            account: account.clone(),
        });
        Ok(())
    }

    pub fn thaw_asset_account(
        &mut self,
        origin: &Account,
        asset: AssetId,
        account: &Account,
    ) -> Result<(), Error> {
        let details = self.assets.get(&asset).ok_or(Error::UnknownAsset)?;
        if details.roles.freezer != *origin {
            return Err(Error::NotFreezer);
        }
        self.frozen_accounts.remove(&(asset, account.clone()));
        self.events.push(Event::AssetAccountThawed {
            asset,
            account: account.clone(),
        });
        Ok(())
    }

    pub fn create_collection(
        &mut self,
        origin: &Account,
        collection: CollectionId,
        roles: CollectionRoles<Account>,
        metadata: Vec<u8>,
    ) -> Result<(), Error> {
        if self.collections.contains_key(&collection) {
            return Err(Error::AlreadyExists);
        }
        if roles.owner != *origin {
            return Err(Error::OwnershipBoundary);
        }
        self.ensure_len(metadata.len(), self.limits.max_collection_metadata_bytes)?;
        let owner = roles.owner.clone();
        self.collections.insert(
            collection,
            CollectionDetails {
                roles,
                metadata,
                frozen: false,
                items: 0,
            },
        );
        self.events
            .push(Event::CollectionCreated { collection, owner });
        Ok(())
    }

    pub fn set_collection_metadata(
        &mut self,
        origin: &Account,
        collection: CollectionId,
        metadata: Vec<u8>,
    ) -> Result<(), Error> {
        self.ensure_len(metadata.len(), self.limits.max_collection_metadata_bytes)?;
        let details = self
            .collections
            .get_mut(&collection)
            .ok_or(Error::UnknownCollection)?;
        if details.roles.owner != *origin {
            return Err(Error::NotOwner);
        }
        details.metadata = metadata;
        self.events
            .push(Event::CollectionMetadataSet { collection });
        Ok(())
    }

    pub fn transfer_collection_ownership(
        &mut self,
        origin: &Account,
        collection: CollectionId,
        new_owner: Account,
    ) -> Result<(), Error> {
        let details = self
            .collections
            .get_mut(&collection)
            .ok_or(Error::UnknownCollection)?;
        if details.roles.owner != *origin {
            return Err(Error::NotOwner);
        }
        let old_owner = details.roles.owner.clone();
        details.roles.owner = new_owner.clone();
        self.events.push(Event::CollectionOwnershipTransferred {
            collection,
            old_owner,
            new_owner,
        });
        Ok(())
    }

    pub fn set_collection_roles(
        &mut self,
        origin: &Account,
        collection: CollectionId,
        roles: CollectionRoles<Account>,
    ) -> Result<(), Error> {
        let details = self
            .collections
            .get_mut(&collection)
            .ok_or(Error::UnknownCollection)?;
        if details.roles.owner != *origin {
            return Err(Error::NotOwner);
        }
        if roles.owner != details.roles.owner {
            return Err(Error::OwnershipBoundary);
        }
        details.roles = roles;
        self.events.push(Event::CollectionRolesSet { collection });
        Ok(())
    }

    pub fn freeze_collection(
        &mut self,
        origin: &Account,
        collection: CollectionId,
    ) -> Result<(), Error> {
        let details = self
            .collections
            .get_mut(&collection)
            .ok_or(Error::UnknownCollection)?;
        if details.roles.freezer != *origin {
            return Err(Error::NotFreezer);
        }
        details.frozen = true;
        self.events.push(Event::CollectionFrozen { collection });
        Ok(())
    }

    pub fn thaw_collection(
        &mut self,
        origin: &Account,
        collection: CollectionId,
    ) -> Result<(), Error> {
        let details = self
            .collections
            .get_mut(&collection)
            .ok_or(Error::UnknownCollection)?;
        if details.roles.freezer != *origin {
            return Err(Error::NotFreezer);
        }
        details.frozen = false;
        self.events.push(Event::CollectionThawed { collection });
        Ok(())
    }

    pub fn mint_unique(
        &mut self,
        origin: &Account,
        collection: CollectionId,
        item: ItemId,
        owner: &Account,
        metadata: Vec<u8>,
    ) -> Result<(), Error> {
        self.ensure_len(metadata.len(), self.limits.max_item_metadata_bytes)?;
        if self.uniques.contains_key(&(collection, item)) {
            return Err(Error::AlreadyExists);
        }
        let details = self
            .collections
            .get(&collection)
            .ok_or(Error::UnknownCollection)?;
        if details.roles.issuer != *origin {
            return Err(Error::NotIssuer);
        }
        if details.frozen {
            return Err(Error::Frozen);
        }
        let new_count = details
            .items
            .checked_add(1)
            .ok_or(Error::ArithmeticOverflow)?;
        self.collections
            .get_mut(&collection)
            .ok_or(Error::UnknownCollection)?
            .items = new_count;
        self.uniques.insert(
            (collection, item),
            UniqueDetails {
                owner: owner.clone(),
                metadata,
                frozen: false,
            },
        );
        self.events.push(Event::UniqueMinted {
            collection,
            item,
            owner: owner.clone(),
        });
        Ok(())
    }

    pub fn burn_unique(
        &mut self,
        origin: &Account,
        collection: CollectionId,
        item: ItemId,
    ) -> Result<(), Error> {
        let collection_details = self
            .collections
            .get(&collection)
            .ok_or(Error::UnknownCollection)?;
        if collection_details.frozen {
            return Err(Error::Frozen);
        }
        let unique = self
            .uniques
            .get(&(collection, item))
            .ok_or(Error::UnknownItem)?;
        if unique.owner != *origin {
            return Err(Error::NotOwner);
        }
        if unique.frozen {
            return Err(Error::Frozen);
        }
        let owner = unique.owner.clone();
        let new_count = collection_details
            .items
            .checked_sub(1)
            .ok_or(Error::AccountingInvariant)?;
        self.uniques.remove(&(collection, item));
        self.collections
            .get_mut(&collection)
            .ok_or(Error::UnknownCollection)?
            .items = new_count;
        self.events.push(Event::UniqueBurned {
            collection,
            item,
            owner,
        });
        Ok(())
    }

    pub fn transfer_unique(
        &mut self,
        origin: &Account,
        collection: CollectionId,
        item: ItemId,
        to: &Account,
    ) -> Result<(), Error> {
        if self
            .collections
            .get(&collection)
            .ok_or(Error::UnknownCollection)?
            .frozen
        {
            return Err(Error::Frozen);
        }
        let unique = self
            .uniques
            .get_mut(&(collection, item))
            .ok_or(Error::UnknownItem)?;
        if unique.owner != *origin {
            return Err(Error::NotOwner);
        }
        if unique.frozen {
            return Err(Error::Frozen);
        }
        let from = unique.owner.clone();
        unique.owner = to.clone();
        self.events.push(Event::UniqueTransferred {
            collection,
            item,
            from,
            to: to.clone(),
        });
        Ok(())
    }

    pub fn freeze_unique(
        &mut self,
        origin: &Account,
        collection: CollectionId,
        item: ItemId,
    ) -> Result<(), Error> {
        let collection_details = self
            .collections
            .get(&collection)
            .ok_or(Error::UnknownCollection)?;
        if collection_details.roles.freezer != *origin {
            return Err(Error::NotFreezer);
        }
        self.uniques
            .get_mut(&(collection, item))
            .ok_or(Error::UnknownItem)?
            .frozen = true;
        self.events.push(Event::UniqueFrozen { collection, item });
        Ok(())
    }

    pub fn thaw_unique(
        &mut self,
        origin: &Account,
        collection: CollectionId,
        item: ItemId,
    ) -> Result<(), Error> {
        let collection_details = self
            .collections
            .get(&collection)
            .ok_or(Error::UnknownCollection)?;
        if collection_details.roles.freezer != *origin {
            return Err(Error::NotFreezer);
        }
        self.uniques
            .get_mut(&(collection, item))
            .ok_or(Error::UnknownItem)?
            .frozen = false;
        self.events.push(Event::UniqueThawed { collection, item });
        Ok(())
    }

    pub fn set_unique_metadata(
        &mut self,
        origin: &Account,
        collection: CollectionId,
        item: ItemId,
        metadata: Vec<u8>,
    ) -> Result<(), Error> {
        self.ensure_len(metadata.len(), self.limits.max_item_metadata_bytes)?;
        let unique = self
            .uniques
            .get_mut(&(collection, item))
            .ok_or(Error::UnknownItem)?;
        if unique.owner != *origin {
            return Err(Error::NotOwner);
        }
        unique.metadata = metadata;
        self.events
            .push(Event::UniqueMetadataSet { collection, item });
        Ok(())
    }

    /// Reconciles every registered supply and the native-transfer conservation baseline.
    pub fn verify_accounting(&self) -> Result<(), Error> {
        for (asset, details) in &self.assets {
            let mut sum = 0u128;
            for ((balance_asset, _), balance) in &self.balances {
                if balance_asset == asset {
                    sum = sum.checked_add(*balance).ok_or(Error::ArithmeticOverflow)?;
                }
            }
            if sum != details.supply {
                return Err(Error::AccountingInvariant);
            }
        }
        let mut native_sum = 0u128;
        for balance in self.native_balances.values() {
            native_sum = native_sum
                .checked_add(*balance)
                .ok_or(Error::ArithmeticOverflow)?;
        }
        if native_sum != self.native_issuance {
            return Err(Error::AccountingInvariant);
        }
        for (collection, details) in &self.collections {
            let actual = self
                .uniques
                .keys()
                .filter(|(item_collection, _)| item_collection == collection)
                .count();
            if usize::try_from(details.items).map_err(|_| Error::AccountingInvariant)? != actual {
                return Err(Error::AccountingInvariant);
            }
        }
        Ok(())
    }

    #[cfg(any(test, feature = "reference-fixtures"))]
    pub fn seed_native_for_reference(
        &mut self,
        account: &Account,
        amount: Balance,
    ) -> Result<(), Error> {
        let old = self.native_balance(account);
        let next = old.checked_add(amount).ok_or(Error::ArithmeticOverflow)?;
        let issuance = self
            .native_issuance
            .checked_add(amount)
            .ok_or(Error::ArithmeticOverflow)?;
        self.native_balances.insert(account.clone(), next);
        self.native_issuance = issuance;
        Ok(())
    }

    fn ensure_asset_metadata(&self, metadata: &AssetMetadata) -> Result<(), Error> {
        self.ensure_len(metadata.name.len(), self.limits.max_asset_name_bytes)?;
        self.ensure_len(metadata.symbol.len(), self.limits.max_asset_symbol_bytes)
    }

    fn ensure_len(&self, actual: usize, maximum: u32) -> Result<(), Error> {
        let actual = u32::try_from(actual).map_err(|_| Error::MetadataTooLong)?;
        if actual > maximum {
            return Err(Error::MetadataTooLong);
        }
        Ok(())
    }

    fn registered_balance(&self, asset: AssetId, account: &Account) -> Balance {
        self.balances
            .get(&(asset, account.clone()))
            .copied()
            .unwrap_or(0)
    }

    fn native_balance(&self, account: &Account) -> Balance {
        self.native_balances.get(account).copied().unwrap_or(0)
    }

    fn set_registered_balance(&mut self, asset: AssetId, account: &Account, amount: Balance) {
        let key = (asset, account.clone());
        if amount == 0 {
            self.balances.remove(&key);
        } else {
            self.balances.insert(key, amount);
        }
    }

    fn set_native_balance(&mut self, account: &Account, amount: Balance) {
        if amount == 0 {
            self.native_balances.remove(account);
        } else {
            self.native_balances.insert(account.clone(), amount);
        }
    }

    fn transfer_registered_unchecked(
        &mut self,
        asset: AssetId,
        from: &Account,
        to: &Account,
        amount: Balance,
    ) -> Result<(), Error> {
        let from_balance = self.registered_balance(asset, from);
        if from == to {
            from_balance
                .checked_sub(amount)
                .ok_or(Error::InsufficientBalance)?;
            return Ok(());
        }
        let to_balance = self.registered_balance(asset, to);
        let new_from = from_balance
            .checked_sub(amount)
            .ok_or(Error::InsufficientBalance)?;
        let new_to = to_balance
            .checked_add(amount)
            .ok_or(Error::ArithmeticOverflow)?;
        self.set_registered_balance(asset, from, new_from);
        self.set_registered_balance(asset, to, new_to);
        Ok(())
    }

    fn transfer_native(
        &mut self,
        from: &Account,
        to: &Account,
        amount: Balance,
    ) -> Result<(), Error> {
        let from_balance = self.native_balance(from);
        if from == to {
            from_balance
                .checked_sub(amount)
                .ok_or(Error::InsufficientBalance)?;
            return Ok(());
        }
        let to_balance = self.native_balance(to);
        let new_from = from_balance
            .checked_sub(amount)
            .ok_or(Error::InsufficientBalance)?;
        let new_to = to_balance
            .checked_add(amount)
            .ok_or(Error::ArithmeticOverflow)?;
        self.set_native_balance(from, new_from);
        self.set_native_balance(to, new_to);
        Ok(())
    }
}

#[cfg(any(test, feature = "reference-fixtures"))]
impl<Account, AssetId, CollectionId, ItemId> FungibleInspect<Account>
    for ReferenceLedger<Account, AssetId, CollectionId, ItemId>
where
    Account: Clone + Ord,
    AssetId: Copy + Ord,
    CollectionId: Copy + Ord,
    ItemId: Copy + Ord,
{
    type AssetId = AssetId;

    fn balance(&self, asset: FungibleAsset<AssetId>, account: &Account) -> Balance {
        match asset {
            FungibleAsset::NativeEtkn => self.native_balance(account),
            FungibleAsset::Registered(asset) => self.registered_balance(asset, account),
        }
    }

    fn total_issuance(&self, asset: FungibleAsset<AssetId>) -> Result<Balance, Error> {
        match asset {
            FungibleAsset::NativeEtkn => Ok(self.native_issuance),
            FungibleAsset::Registered(asset) => self
                .assets
                .get(&asset)
                .map(|details| details.supply)
                .ok_or(Error::UnknownAsset),
        }
    }
}

#[cfg(any(test, feature = "reference-fixtures"))]
impl<Account, AssetId, CollectionId, ItemId> FungibleTransfer<Account>
    for ReferenceLedger<Account, AssetId, CollectionId, ItemId>
where
    Account: Clone + Ord,
    AssetId: Copy + Ord,
    CollectionId: Copy + Ord,
    ItemId: Copy + Ord,
{
    fn transfer(
        &mut self,
        asset: FungibleAsset<AssetId>,
        from: &Account,
        to: &Account,
        amount: Balance,
    ) -> Result<(), Error> {
        match asset {
            FungibleAsset::NativeEtkn => self.transfer_native(from, to, amount),
            FungibleAsset::Registered(asset) => self.transfer_asset(from, asset, to, amount),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    type Ledger = ReferenceLedger<u64, u32, u32, u32>;

    fn limits() -> InterfaceLimits {
        InterfaceLimits {
            max_asset_name_bytes: 16,
            max_asset_symbol_bytes: 8,
            max_collection_metadata_bytes: 24,
            max_item_metadata_bytes: 32,
        }
    }

    fn metadata(name: &[u8], symbol: &[u8]) -> AssetMetadata {
        AssetMetadata {
            name: name.to_vec(),
            symbol: symbol.to_vec(),
            decimals: 6,
        }
    }

    fn asset_roles(owner: u64) -> AssetRoles<u64> {
        AssetRoles {
            owner,
            issuer: owner + 1,
            admin: owner + 2,
            freezer: owner + 3,
        }
    }

    fn collection_roles(owner: u64) -> CollectionRoles<u64> {
        CollectionRoles {
            owner,
            issuer: owner + 1,
            admin: owner + 2,
            freezer: owner + 3,
        }
    }

    fn asset_ledger() -> Ledger {
        let mut ledger = Ledger::new(limits());
        ledger
            .create_asset(&10, 7, asset_roles(10), metadata(b"App token", b"APP"))
            .unwrap();
        ledger
    }

    #[test]
    fn create_mint_transfer_burn_conserves_registered_supply_and_native_etkn() {
        let mut ledger = asset_ledger();
        ledger.seed_native_for_reference(&10, 500).unwrap();
        let native_before = ledger.total_issuance(FungibleAsset::NativeEtkn).unwrap();

        ledger.mint_asset(&11, 7, &20, 1_000).unwrap();
        ledger.transfer_asset(&20, 7, &30, 250).unwrap();
        ledger.burn_asset(&12, 7, &20, 100).unwrap();

        assert_eq!(ledger.balance(FungibleAsset::Registered(7), &20), 650);
        assert_eq!(ledger.balance(FungibleAsset::Registered(7), &30), 250);
        assert_eq!(ledger.total_issuance(FungibleAsset::Registered(7)), Ok(900));
        assert_eq!(
            ledger.total_issuance(FungibleAsset::NativeEtkn),
            Ok(native_before)
        );
        assert_eq!(ledger.verify_accounting(), Ok(()));
    }

    #[test]
    fn authorization_and_freeze_boundaries_fail_without_state_or_event_changes() {
        let mut ledger = asset_ledger();
        ledger.mint_asset(&11, 7, &20, 100).unwrap();

        for result in [
            ledger.mint_asset(&99, 7, &20, 1),
            ledger.burn_asset(&99, 7, &20, 1),
            ledger.freeze_asset(&99, 7),
        ] {
            assert!(matches!(
                result,
                Err(Error::NotIssuer | Error::NotAdmin | Error::NotFreezer)
            ));
        }

        ledger.freeze_asset(&13, 7).unwrap();
        let frozen = ledger.clone();
        assert_eq!(ledger.transfer_asset(&20, 7, &30, 1), Err(Error::Frozen));
        assert_eq!(ledger, frozen);
        ledger.thaw_asset(&13, 7).unwrap();
        ledger.freeze_asset_account(&13, 7, &20).unwrap();
        let account_frozen = ledger.clone();
        assert_eq!(
            ledger.transfer_asset(&20, 7, &30, 1),
            Err(Error::AccountFrozen)
        );
        assert_eq!(ledger, account_frozen);
        ledger.thaw_asset_account(&13, 7, &20).unwrap();
        ledger.transfer_asset(&20, 7, &30, 1).unwrap();
    }

    #[test]
    fn metadata_and_ownership_are_bounded_and_separate_from_operational_roles() {
        let mut ledger = asset_ledger();
        let snapshot = ledger.clone();
        assert_eq!(
            ledger.set_asset_metadata(&10, 7, metadata(&[b'x'; 17], b"X")),
            Err(Error::MetadataTooLong)
        );
        assert_eq!(ledger, snapshot);
        assert_eq!(
            ledger.set_asset_metadata(&99, 7, metadata(b"Other", b"OTH")),
            Err(Error::NotOwner)
        );
        ledger.transfer_asset_ownership(&10, 7, 40).unwrap();
        assert_eq!(ledger.asset(7).unwrap().roles.owner, 40);
        assert_eq!(ledger.asset(7).unwrap().roles.issuer, 11);
        assert_eq!(
            ledger.set_asset_roles(&40, 7, asset_roles(50)),
            Err(Error::OwnershipBoundary)
        );
    }

    #[test]
    fn overflow_and_insufficient_balance_fail_atomically() {
        let mut ledger = asset_ledger();
        ledger.mint_asset(&11, 7, &20, Balance::MAX).unwrap();
        let maximum = ledger.clone();
        assert_eq!(
            ledger.mint_asset(&11, 7, &20, 1),
            Err(Error::ArithmeticOverflow)
        );
        assert_eq!(ledger, maximum);
        assert_eq!(
            ledger.burn_asset(&12, 7, &30, 1),
            Err(Error::InsufficientBalance)
        );
        assert_eq!(ledger, maximum);
    }

    #[test]
    fn unique_lifecycle_enforces_collection_item_metadata_and_ownership_boundaries() {
        let mut ledger = Ledger::new(limits());
        ledger
            .create_collection(&10, 3, collection_roles(10), b"collection".to_vec())
            .unwrap();
        ledger
            .mint_unique(&11, 3, 9, &20, b"content commitment".to_vec())
            .unwrap();
        assert_eq!(ledger.collection(3).unwrap().items, 1);
        assert_eq!(ledger.unique(3, 9).unwrap().owner, 20);

        let unauthorized = ledger.clone();
        assert_eq!(ledger.transfer_unique(&99, 3, 9, &30), Err(Error::NotOwner));
        assert_eq!(ledger, unauthorized);
        ledger.freeze_unique(&13, 3, 9).unwrap();
        let frozen = ledger.clone();
        assert_eq!(ledger.transfer_unique(&20, 3, 9, &30), Err(Error::Frozen));
        assert_eq!(ledger, frozen);
        ledger.thaw_unique(&13, 3, 9).unwrap();
        ledger.transfer_unique(&20, 3, 9, &30).unwrap();
        ledger
            .set_unique_metadata(&30, 3, 9, b"new commitment".to_vec())
            .unwrap();
        ledger.burn_unique(&30, 3, 9).unwrap();
        assert!(ledger.unique(3, 9).is_none());
        assert_eq!(ledger.collection(3).unwrap().items, 0);
        assert_eq!(ledger.verify_accounting(), Ok(()));
    }

    #[test]
    fn collection_freeze_and_duplicate_or_bound_failures_roll_back() {
        let mut ledger = Ledger::new(limits());
        ledger
            .create_collection(&10, 3, collection_roles(10), b"collection".to_vec())
            .unwrap();
        let created = ledger.clone();
        assert_eq!(
            ledger.create_collection(&10, 3, collection_roles(10), Vec::new()),
            Err(Error::AlreadyExists)
        );
        assert_eq!(ledger, created);
        assert_eq!(
            ledger.mint_unique(&11, 3, 1, &20, vec![0; 33]),
            Err(Error::MetadataTooLong)
        );
        assert_eq!(ledger, created);
        ledger.freeze_collection(&13, 3).unwrap();
        let frozen = ledger.clone();
        assert_eq!(
            ledger.mint_unique(&11, 3, 1, &20, Vec::new()),
            Err(Error::Frozen)
        );
        assert_eq!(ledger, frozen);
        ledger.thaw_collection(&13, 3).unwrap();
        ledger.mint_unique(&11, 3, 1, &20, Vec::new()).unwrap();
    }

    #[test]
    fn transfer_trait_conserves_native_etkn_and_rejects_shortfall() {
        let mut ledger = Ledger::new(limits());
        ledger.seed_native_for_reference(&1, 100).unwrap();
        let issuance = ledger.total_issuance(FungibleAsset::NativeEtkn).unwrap();
        FungibleTransfer::transfer(&mut ledger, FungibleAsset::NativeEtkn, &1, &2, 40).unwrap();
        assert_eq!(ledger.balance(FungibleAsset::NativeEtkn, &1), 60);
        assert_eq!(ledger.balance(FungibleAsset::NativeEtkn, &2), 40);
        let snapshot = ledger.clone();
        assert_eq!(
            FungibleTransfer::transfer(&mut ledger, FungibleAsset::NativeEtkn, &1, &2, 61),
            Err(Error::InsufficientBalance)
        );
        assert_eq!(ledger, snapshot);
        assert_eq!(
            ledger.total_issuance(FungibleAsset::NativeEtkn),
            Ok(issuance)
        );
        assert_eq!(ledger.verify_accounting(), Ok(()));
    }
}

#[cfg(test)]
mod allocation_tests {
    use super::*;

    #[test]
    fn explicit_ranges_detect_overlap_and_invalid_order() {
        assert_eq!(IdRange::new(9, 8), Err(AllocationError::InvalidRange));
        let range = IdRange::new(10, 19).unwrap();
        assert!(range.overlaps(&IdRange::new(19, 20).unwrap()));
        assert!(!range.overlaps(&IdRange::new(20, 30).unwrap()));
        assert!(!range.contains(9));
        assert!(!range.contains(20));
    }

    #[test]
    fn last_u32_is_allocatable_once_without_wrapping_or_resetting() {
        let range = IdRange::new(u32::MAX - 1, u32::MAX).unwrap();
        let first = range.prepare(Some(u32::MAX - 1), |_| false).unwrap();
        let last = range.prepare(first.next, |_| false).unwrap();
        assert_eq!(
            last,
            AllocationPlan {
                id: u32::MAX,
                next: None
            }
        );
        assert_eq!(
            range.prepare(last.next, |_| panic!("no backend lookup")),
            Err(AllocationError::Exhausted)
        );
    }

    #[test]
    fn collision_and_invalid_cursor_do_not_skip_or_mutate_input() {
        let range = IdRange::new(10, 19).unwrap();
        let cursor = Some(12);
        let mut lookups = 0;
        assert_eq!(
            range.prepare(cursor, |id| {
                lookups += 1;
                assert_eq!(id, 12);
                true
            }),
            Err(AllocationError::Collision)
        );
        assert_eq!(lookups, 1);
        assert_eq!(cursor, Some(12));
        assert_eq!(
            range.prepare(Some(9), |_| panic!("no lookup")),
            Err(AllocationError::OutOfRange)
        );
    }
}

/// Registered version-one runtime API. Frozen V1 response encodings stay unchanged.
pub mod runtime_api {
 use super::v1::*;
 sp_api::decl_runtime_apis! {
  #[api_version(1)]
  pub trait EraV14AssetsApiV1 {
   fn asset_v1(id:u32)->ApiResult<AssetV1>;
   fn assets_v1(cursor:Option<u32>,limit:u32)->ApiResult<Page<AssetV1>>;
   fn collection_v1(id:u32)->ApiResult<CollectionV1>;
   fn collections_v1(cursor:Option<u32>,limit:u32)->ApiResult<Page<CollectionV1>>;
   fn item_v1(collection:u32,id:u32)->ApiResult<ItemV1>;
   fn items_v1(collection:u32,cursor:Option<u32>,limit:u32)->ApiResult<Page<ItemV1>>;
  }
 }
}
