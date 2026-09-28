//! Owner-adopted A–D contract, 2026-09-05. Source-only: no runtime API registration or storage.
use super::{AllocationError, AllocationPlan, IdRange};
use bounded_collections::{BoundedVec, ConstU32};
use codec::{Decode, DecodeAll, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;

pub type AccountId = [u8; 32];
pub const API_NAME: &str = "EraV14AssetsApiV1";
pub const API_VERSION: u32 = 1;
pub const RETAIN_SPEC_TRANSITIONS: u32 = 2;
pub const ASSETS_INDEX: u8 = 16;
pub const NFTS_INDEX: u8 = 17;
pub const SDK_STORAGE_VERSION: u16 = 1;
pub const ETKN: u128 = 1_000_000_000_000_000_000;
pub const NATIVE_MINIMUM: u128 = 100_000_000_000_000;
pub const ASSET_DEPOSIT: u128 = 10_000_000_000_000_000_000;
pub const ASSET_ACCOUNT_DEPOSIT: u128 = 100_000_000_000_000_000;
pub const ASSET_METADATA_BASE: u128 = 1_000_000_000_000_000_000;
pub const ASSET_METADATA_BYTE: u128 = 1_000_000_000_000_000;
pub const ASSET_APPROVAL_DEPOSIT: u128 = 100_000_000_000_000_000;
pub const NFT_COLLECTION_DEPOSIT: u128 = 10_000_000_000_000_000_000;
pub const NFT_ITEM_DEPOSIT: u128 = 1_000_000_000_000_000_000;
pub const NFT_METADATA_BASE: u128 = 1_000_000_000_000_000_000;
pub const NFT_ATTRIBUTE_BASE: u128 = 1_000_000_000_000_000_000;
pub const NFT_DEPOSIT_BYTE: u128 = 1_000_000_000_000_000;
pub const ADDED_DEPOSIT_SLASH: u32 = 0;
pub const ROLE_DELAY: u32 = 0;
pub const ASSET_SDK_STRING_LIMIT: u32 = 64;
pub const NAME_LIMIT: u32 = 64;
pub const SYMBOL_LIMIT: u32 = 16;
pub const METADATA_LIMIT: u32 = 128;
pub const KEY_LIMIT: u32 = 64;
pub const VALUE_LIMIT: u32 = 128;
pub const REMOVE_LIMIT: u32 = 1_000;
pub const APPROVAL_LIMIT: u32 = 20;
pub const ATTRIBUTE_APPROVAL_LIMIT: u32 = 10;
pub const TIP_LIMIT: u32 = 10;
pub const DEADLINE_LIMIT: u32 = 14_400;
pub const ATTRIBUTES_PER_CALL: u32 = 10;
pub const PAGE_LIMIT: u32 = 64;
pub const PAGE_KEY_VISITS: u32 = 65;
pub const MAX_NORMAL_BLOCK_PERCENT: u32 = 25;
pub const REGISTERED_FIRST: u32 = 1;
pub const REGISTERED_LAST: u32 = 2_147_483_647;
pub const LP_FIRST: u32 = 2_147_483_648;
pub const LP_LAST: u32 = 3_221_225_471;
pub const SYSTEM_FIRST: u32 = 3_221_225_472;
pub const SYSTEM_LAST: u32 = u32::MAX;
pub const COLLECTION_FIRST: u32 = 1;
pub const COLLECTION_LAST: u32 = 2_147_483_647;
pub const ITEM_FIRST: u32 = 1;
pub const ITEM_LAST: u32 = u32::MAX;
pub const ALLOCATOR_PREFIX: &[u8] = b"EraV14AssetAllocator";
pub const SCHEMA_KEY: &[u8] = b"SchemaVersion";
pub const ASSET_CURSOR_KEY: &[u8] = b"NextAssetId";
pub const COLLECTION_CURSOR_KEY: &[u8] = b"NextCollectionId";
pub const ITEM_CURSOR_KEY: &[u8] = b"NextItemId";
pub const ALLOCATOR_VERSION: u16 = 1;

pub type Name = BoundedVec<u8, ConstU32<NAME_LIMIT>>;
pub type Symbol = BoundedVec<u8, ConstU32<SYMBOL_LIMIT>>;
pub type Metadata = BoundedVec<u8, ConstU32<METADATA_LIMIT>>;

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
pub enum AssetApiErrorV1 {
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
pub enum AssetStatusV1 {
    #[codec(index = 0)]
    Live,
    #[codec(index = 1)]
    Frozen,
    #[codec(index = 2)]
    Destroying,
}
#[derive(
    Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo,
)]
pub struct AssetMetadataV1 {
    pub name: Name,
    pub symbol: Symbol,
    pub decimals: u8,
    pub frozen: bool,
}
#[derive(
    Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo,
)]
pub struct AssetV1 {
    pub id: u32,
    pub owner: AccountId,
    pub issuer: AccountId,
    pub admin: AccountId,
    pub freezer: AccountId,
    pub supply: u128,
    pub minimum_balance: u128,
    pub is_sufficient: bool,
    pub status: AssetStatusV1,
    pub metadata: Option<AssetMetadataV1>,
}
#[derive(
    Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo,
)]
pub struct CollectionV1 {
    pub id: u32,
    pub owner: AccountId,
    pub issuer: Option<AccountId>,
    pub admin: Option<AccountId>,
    pub freezer: Option<AccountId>,
    pub item_count: u32,
    pub metadata: Option<Metadata>,
}
#[derive(
    Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo,
)]
pub struct ItemV1 {
    pub collection: u32,
    pub id: u32,
    pub owner: AccountId,
    pub metadata: Option<Metadata>,
}
#[derive(
    Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo,
)]
pub struct Page<T> {
    pub entries: BoundedVec<T, ConstU32<PAGE_LIMIT>>,
    pub next: Option<u32>,
}
pub type ApiResult<T> = Result<T, AssetApiErrorV1>;

/// Frozen source trait only. No sp_api declaration, generated API ID, registration or exposure.
/// A cursor is checked only in the requested map at the selected state, not for provenance.
pub trait EraV14AssetsApiV1 {
    fn asset_v1(id: u32) -> ApiResult<AssetV1>;
    fn assets_v1(cursor: Option<u32>, limit: u32) -> ApiResult<Page<AssetV1>>;
    fn collection_v1(id: u32) -> ApiResult<CollectionV1>;
    fn collections_v1(cursor: Option<u32>, limit: u32) -> ApiResult<Page<CollectionV1>>;
    fn item_v1(collection: u32, id: u32) -> ApiResult<ItemV1>;
    fn items_v1(collection: u32, cursor: Option<u32>, limit: u32) -> ApiResult<Page<ItemV1>>;
}

/// Bound before decode, consume all bytes, and require canonical SCALE. No UTF-8 normalization.
pub fn decode_exact<T: DecodeAll + Encode + MaxEncodedLen>(
    input: &[u8],
) -> Result<T, codec::Error> {
    if input.len() > T::max_encoded_len() {
        return Err("encoded bound exceeded".into());
    }
    let value = T::decode_all(&mut &input[..])?;
    if value.encode() != input {
        return Err("noncanonical SCALE".into());
    }
    Ok(value)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AssetAdmission {
    pub id: u32,
    pub minimum_balance: u128,
}
pub const ADMITTED_ASSETS: &[AssetAdmission] = &[];
pub const ADMITTED_COLLECTIONS: &[u32] = &[];
pub const ADMITTED_CATEGORIES: &[u32] = &[];

pub fn validate_asset_admission(entry: &AssetAdmission) -> ApiResult<()> {
    if !(REGISTERED_FIRST..=REGISTERED_LAST).contains(&entry.id) || entry.minimum_balance == 0 {
        return Err(AssetApiErrorV1::UnsupportedAsset);
    }
    Ok(())
}
pub fn admitted_asset(id: u32) -> ApiResult<AssetAdmission> {
    ADMITTED_ASSETS
        .iter()
        .copied()
        .find(|e| e.id == id)
        .ok_or(AssetApiErrorV1::UnsupportedAsset)
}
pub fn admitted_collection(id: u32) -> ApiResult<()> {
    if !(COLLECTION_FIRST..=COLLECTION_LAST).contains(&id) || !ADMITTED_COLLECTIONS.contains(&id) {
        return Err(AssetApiErrorV1::UnsupportedAsset);
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorError {
    Missing,
    Malformed,
    Version,
    Allocation(AllocationError),
}
/// A present None is exhausted; absence is a separate error. No default or reset exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AllocatorCursor(Option<u32>);
impl AllocatorCursor {
    pub fn decode(schema: Option<&[u8]>, state: Option<&[u8]>) -> Result<Self, CursorError> {
        let version = decode_exact::<u16>(schema.ok_or(CursorError::Missing)?)
            .map_err(|_| CursorError::Malformed)?;
        if version != ALLOCATOR_VERSION {
            return Err(CursorError::Version);
        }
        let cursor = decode_exact::<Option<u32>>(state.ok_or(CursorError::Missing)?)
            .map_err(|_| CursorError::Malformed)?;
        Ok(Self(cursor))
    }
    pub fn prepare(
        self,
        range: IdRange,
        occupied: impl FnOnce(u32) -> bool,
    ) -> Result<AllocationPlan, CursorError> {
        range
            .prepare(self.0, occupied)
            .map_err(CursorError::Allocation)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContractError {
    Unsupported,
    Authority,
    Bound,
    Arithmetic,
    Unsettled,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Authority {
    Creator,
    Owner,
    Issuer,
    Admin,
    Freezer,
    Holder,
    Delegate,
    SignedMaintenance,
    Depositor,
    ItemOwner,
    AttributeDelegate,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssetOperation {
    Create,
    Mint,
    Burn,
    Thaw,
    ThawAsset,
    Freeze,
    FreezeAsset,
    Block,
    Transfer,
    Approve,
    CancelApproval,
    TransferApproved,
    SetMetadata,
    ClearMetadata,
    SetTeam,
    TransferOwnership,
    StartDestroy,
    FinishDestroy,
    DestroyAccounts,
    DestroyApprovals,
    Touch,
    TouchOther,
    Refund,
    Unsupported,
}
impl AssetOperation {
    pub fn authority(self) -> Result<Authority, ContractError> {
        use AssetOperation::*;
        Ok(match self {
            Create => Authority::Creator,
            Mint => Authority::Issuer,
            Burn | Thaw | ThawAsset => Authority::Admin,
            Freeze | FreezeAsset | Block => Authority::Freezer,
            Transfer | Approve | CancelApproval => Authority::Holder,
            TransferApproved => Authority::Delegate,
            SetMetadata | ClearMetadata | SetTeam | TransferOwnership | StartDestroy
            | FinishDestroy => Authority::Owner,
            DestroyAccounts | DestroyApprovals => Authority::SignedMaintenance,
            Touch | TouchOther | Refund => Authority::Depositor,
            Unsupported => return Err(ContractError::Unsupported),
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NftOperation {
    Create,
    Mint,
    Transfer,
    Burn,
    TransferApproved,
    LockItemTransfer,
    UnlockItemTransfer,
    SetMetadata,
    ClearMetadata,
    SetCollectionMetadata,
    ClearCollectionMetadata,
    LockItemProperties,
    LockCollection,
    SetTeam,
    TransferOwnership,
    Destroy,
    CollectionAttribute,
    ItemAttribute,
    DelegatedAttribute,
    ApproveAttributes,
    CancelAttributes,
    ApproveTransfer,
    CancelApproval,
    ClearApprovals,
    AcceptOwnership,
    Unsupported,
}
impl NftOperation {
    pub fn authority(self) -> Result<Authority, ContractError> {
        use NftOperation::*;
        Ok(match self {
            Create => Authority::Creator,
            Mint => Authority::Issuer,
            Transfer | Burn | ItemAttribute | ApproveAttributes | CancelAttributes
            | ApproveTransfer | CancelApproval | ClearApprovals => Authority::ItemOwner,
            TransferApproved => Authority::Delegate,
            LockItemTransfer | UnlockItemTransfer => Authority::Freezer,
            SetMetadata
            | ClearMetadata
            | SetCollectionMetadata
            | ClearCollectionMetadata
            | LockItemProperties
            | CollectionAttribute => Authority::Admin,
            LockCollection | SetTeam | TransferOwnership | Destroy => Authority::Owner,
            DelegatedAttribute => Authority::AttributeDelegate,
            AcceptOwnership => Authority::Holder,
            Unsupported => return Err(ContractError::Unsupported),
        })
    }
}
/// A fixed set, never an unbounded role list. Operational duties do not follow ownership.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Roles<A> {
    pub owner: A,
    pub issuer: Option<A>,
    pub admin: Option<A>,
    pub freezer: Option<A>,
}
impl<A: PartialEq> Roles<A> {
    pub fn authorize(&self, caller: &A, authority: Authority) -> Result<(), ContractError> {
        let allowed = match authority {
            Authority::Owner => caller == &self.owner,
            Authority::Issuer => self.issuer.as_ref() == Some(caller),
            Authority::Admin => self.admin.as_ref() == Some(caller),
            Authority::Freezer => self.freezer.as_ref() == Some(caller),
            // Holder/delegate/payer/admission checks require SDK object-specific evidence.
            _ => return Err(ContractError::Authority),
        };
        if allowed {
            Ok(())
        } else {
            Err(ContractError::Authority)
        }
    }
    pub fn replacement(&self, caller: &A, next: &Self) -> Result<(), ContractError> {
        self.authorize(caller, Authority::Owner)?;
        if next.owner != self.owner
            || next.issuer.is_none()
            || next.admin.is_none()
            || next.freezer.is_none()
            || self.issuer.is_none()
            || self.admin.is_none()
            || self.freezer.is_none()
        {
            return Err(ContractError::Authority);
        }
        Ok(())
    }
}

pub fn deposit(base: u128, per_byte: u128, lengths: [u32; 2]) -> Result<u128, ContractError> {
    let bytes = lengths[0]
        .checked_add(lengths[1])
        .ok_or(ContractError::Arithmetic)?;
    base.checked_add(
        per_byte
            .checked_mul(bytes.into())
            .ok_or(ContractError::Arithmetic)?,
    )
    .ok_or(ContractError::Arithmetic)
}
pub fn asset_metadata_deposit(name: u32, symbol: u32) -> Result<u128, ContractError> {
    if name > NAME_LIMIT || symbol > SYMBOL_LIMIT {
        return Err(ContractError::Bound);
    }
    deposit(ASSET_METADATA_BASE, ASSET_METADATA_BYTE, [name, symbol])
}
pub fn nft_metadata_deposit(bytes: u32) -> Result<u128, ContractError> {
    if bytes > METADATA_LIMIT {
        return Err(ContractError::Bound);
    }
    deposit(NFT_METADATA_BASE, NFT_DEPOSIT_BYTE, [bytes, 0])
}
pub fn attribute_deposit(key: u32, value: u32) -> Result<u128, ContractError> {
    if key > KEY_LIMIT || value > VALUE_LIMIT {
        return Err(ContractError::Bound);
    }
    deposit(NFT_ATTRIBUTE_BASE, NFT_DEPOSIT_BYTE, [key, value])
}
pub fn validate_destroy(
    items: u32,
    actual: [u32; 3],
    witness: [u32; 3],
) -> Result<(), ContractError> {
    if items != 0 {
        return Err(ContractError::Unsettled);
    }
    if actual != witness || actual.iter().any(|n| *n > REMOVE_LIMIT) {
        return Err(ContractError::Bound);
    }
    Ok(())
}
pub fn validate_refund(balance: u128, allow_burn: bool) -> Result<(), ContractError> {
    if balance != 0 || allow_burn {
        return Err(ContractError::Unsettled);
    }
    Ok(())
}
pub fn bounded_count(count: u32, bound: u32) -> Result<(), ContractError> {
    if count > bound {
        Err(ContractError::Bound)
    } else {
        Ok(())
    }
}
pub fn next_item_count(current: u32) -> Result<u32, ContractError> {
    current.checked_add(1).ok_or(ContractError::Arithmetic)
}

#[cfg(test)]
#[path = "v1_tests.rs"]
mod tests;
