//! Adopted dormant SDK contract; no public dispatch/API registration or allocator installation.
//! Only tests invoke the mutation and pagination helpers. Existing calls/filters are unchanged.

#![allow(
    dead_code,
    reason = "dormant asset contracts are compiled without an activation path"
)]

use crate::{AccountId, Assets, Balance, Balances, Runtime};
use alloc::{vec, vec::Vec};
use codec::{DecodeAll, Encode, MaxEncodedLen};
use core::marker::PhantomData;
use era_v14_application_primitives::assets::{
    v1, Error, FungibleAsset, FungibleInspect, FungibleTransfer,
};
use frame_support::{
    storage::{transactional::with_transaction_opaque_err, TransactionOutcome},
    traits::{fungible, fungibles, tokens::Preservation, ConstU32},
    BoundedVec,
};
use sp_runtime::DispatchError;
use v1::{ApiResult, AssetApiErrorV1 as ApiError, EraV14AssetsApiV1};

pub(crate) struct SdkFungibles;

impl FungibleInspect<AccountId> for SdkFungibles {
    type AssetId = u32;

    fn balance(&self, asset: FungibleAsset<u32>, who: &AccountId) -> Balance {
        match asset {
            FungibleAsset::NativeEtkn => <Balances as fungible::Inspect<AccountId>>::balance(who),
            FungibleAsset::Registered(id) => Assets::balance(id, who),
        }
    }

    fn total_issuance(&self, asset: FungibleAsset<u32>) -> Result<Balance, Error> {
        match asset {
            FungibleAsset::NativeEtkn => Ok(Balances::total_issuance()),
            FungibleAsset::Registered(id) => {
                Assets::maybe_total_supply(id).ok_or(Error::UnknownAsset)
            }
        }
    }
}

/// Internal SDK diagnostics; the transfer contract maps these to stable module errors.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TransferError {
    UnknownAsset,
    Arithmetic,
    Backend(DispatchError),
    Conservation,
    TransactionLimit,
}

impl SdkFungibles {
    /// Transfer-only primitive. The caller supplies preservation explicitly and must establish
    /// authority and application-level atomicity before any future wiring. No mint/burn methods.
    /// A backend dust/reaping effect or inexact amount rolls back balances, issuance and events.
    pub(crate) fn transfer_exact(
        &self,
        asset: FungibleAsset<u32>,
        from: &AccountId,
        to: &AccountId,
        amount: Balance,
        preservation: Preservation,
    ) -> Result<(), TransferError> {
        with_transaction_opaque_err(|| {
            let result =
                (|| {
                    let supply = self
                        .total_issuance(asset)
                        .map_err(|_| TransferError::UnknownAsset)?;
                    let native = Balances::total_issuance();
                    let before_from = self.balance(asset, from);
                    let before_to = self.balance(asset, to);
                    let (expected_from, expected_to) = if from == to {
                        (before_from, before_to)
                    } else {
                        (
                            before_from
                                .checked_sub(amount)
                                .ok_or(TransferError::Arithmetic)?,
                            before_to
                                .checked_add(amount)
                                .ok_or(TransferError::Arithmetic)?,
                        )
                    };
                    let actual =
                        match asset {
                            FungibleAsset::NativeEtkn => <Balances as fungible::Mutate<
                                AccountId,
                            >>::transfer(
                                from, to, amount, preservation
                            ),
                            FungibleAsset::Registered(id) => <Assets as fungibles::Mutate<
                                AccountId,
                            >>::transfer(
                                id, from, to, amount, preservation
                            ),
                        }
                        .map_err(TransferError::Backend)?;
                    if actual != amount
                        || self.balance(asset, from) != expected_from
                        || self.balance(asset, to) != expected_to
                        || self.total_issuance(asset) != Ok(supply)
                        || Balances::total_issuance() != native
                    {
                        return Err(TransferError::Conservation);
                    }
                    Ok(())
                })();
            match result {
                Ok(()) => TransactionOutcome::Commit(Ok(())),
                Err(error) => TransactionOutcome::Rollback(Err(error)),
            }
        })
        .map_err(|_| TransferError::TransactionLimit)?
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PageError {
    InvalidLimit,
    InvalidCursor,
    BackendInvariant,
}

/// Bounded physical-key traversal for a SDK map whose final key is Blake2_128Concat(u32).
/// `prefix` is trusted adapter input (a map or collection-specific prefix), never a public input.
/// Blake2_128Concat hash order is adopted for V1 (MAX=64 in the reader). Tests may use smaller
/// bounds. The Option<u32> cursor proves only existence in this map at the selected state.
/// At most limit+1 next-key reads, plus one cursor existence check. No total-map scan or sort.
pub(crate) fn page_ids<const MAX: u32>(
    prefix: &[u8],
    cursor: Option<u32>,
    limit: u32,
) -> Result<(BoundedVec<u32, ConstU32<MAX>>, Option<u32>), PageError> {
    if limit == 0 || limit > MAX {
        return Err(PageError::InvalidLimit);
    }
    let mut key = prefix.to_vec();
    if let Some(id) = cursor {
        let encoded = id.encode();
        key.extend_from_slice(&sp_io::hashing::blake2_128(&encoded));
        key.extend_from_slice(&encoded);
        if !sp_io::storage::exists(&key) {
            return Err(PageError::InvalidCursor);
        }
    }
    let mut ids = BoundedVec::<u32, ConstU32<MAX>>::default();
    while let Some(next) = sp_io::storage::next_key(&key) {
        if !next.starts_with(prefix) {
            break;
        }
        let suffix = &next[prefix.len()..];
        if suffix.len() != 20 || suffix[..16] != sp_io::hashing::blake2_128(&suffix[16..]) {
            return Err(PageError::BackendInvariant);
        }
        let id = u32::from_le_bytes(
            suffix[16..]
                .try_into()
                .map_err(|_| PageError::BackendInvariant)?,
        );
        if ids.len() == limit as usize {
            return Ok((ids, cursor_for(&key, prefix)?));
        }
        ids.try_push(id).map_err(|_| PageError::InvalidLimit)?;
        key = next;
    }
    Ok((ids, None))
}

fn cursor_for(key: &[u8], prefix: &[u8]) -> Result<Option<u32>, PageError> {
    let bytes = key
        .get(prefix.len() + 16..)
        .ok_or(PageError::BackendInvariant)?;
    Ok(Some(u32::from_le_bytes(
        bytes.try_into().map_err(|_| PageError::BackendInvariant)?,
    )))
}

#[cfg(test)]
#[path = "v14_assets_tests.rs"]
mod tests;

/// Frozen error mapping for the transfer-only V14-5 contract. Raw SDK module errors never escape.
fn transfer_error(error: TransferError) -> Error {
    use sp_runtime::{ArithmeticError, TokenError};
    match error {
        TransferError::UnknownAsset => Error::UnknownAsset,
        TransferError::Arithmetic => Error::ArithmeticOverflow,
        TransferError::Conservation | TransferError::TransactionLimit => Error::AccountingInvariant,
        TransferError::Backend(e) => match e {
            DispatchError::Token(TokenError::FundsUnavailable) => Error::InsufficientBalance,
            DispatchError::Token(TokenError::Frozen) => Error::Frozen,
            DispatchError::Token(
                TokenError::BelowMinimum | TokenError::CannotCreate | TokenError::NotExpendable,
            ) => Error::BelowMinimum,
            DispatchError::Token(TokenError::UnknownAsset) => Error::UnknownAsset,
            DispatchError::Arithmetic(
                ArithmeticError::Overflow
                | ArithmeticError::Underflow
                | ArithmeticError::DivisionByZero,
            ) => Error::ArithmeticOverflow,
            _ if e == pallet_assets::Error::<Runtime>::Frozen.into() => Error::Frozen,
            _ if e == pallet_assets::Error::<Runtime>::BalanceLow.into() => {
                Error::InsufficientBalance
            }
            _ if e == pallet_assets::Error::<Runtime>::MinBalanceZero.into() => Error::BelowMinimum,
            _ => Error::BackendRejected,
        },
    }
}

pub(crate) trait AdmissionLookup {
    const CONFIGURED: bool;
    fn configured() -> bool { Self::CONFIGURED }
    fn asset(id: u32) -> ApiResult<v1::AssetAdmission>;
    fn collection(id: u32) -> ApiResult<()>;
}
pub(crate) struct AdoptedAdmissions;
impl AdmissionLookup for AdoptedAdmissions {
    const CONFIGURED: bool = true;
    fn asset(id: u32) -> ApiResult<v1::AssetAdmission> {
        v1::admitted_asset(id)
    }
    fn collection(id: u32) -> ApiResult<()> {
        v1::admitted_collection(id)
    }
}

/// Transfer-only, Preserve/Polite contract. Registered IDs require an exact admitted minimum;
/// native ETKN uses Balances and its existing locks. Zero/self transfers still validate admission
/// and SDK preconditions. Distinct-account shortfall is InsufficientBalance before backend calls.
/// No dust/reaping or supply changes may commit. Application callers still need one enclosing
/// transaction for multiple transfers, and must establish ownership/delegation before invoking.
pub(crate) struct ContractFungibles<A>(PhantomData<A>);
pub(crate) type DormantFungibles = ContractFungibles<AdoptedAdmissions>;
impl<A: AdmissionLookup> ContractFungibles<A> {
    pub(crate) fn new() -> Self {
        Self(PhantomData)
    }
    fn check(asset: FungibleAsset<u32>) -> Result<(), Error> {
        if !A::CONFIGURED {
            return Err(Error::UnsupportedAsset);
        }
        if let FungibleAsset::Registered(id) = asset {
            SdkReader::<A>::asset_v1(id).map_err(|e| match e {
                ApiError::NotFound => Error::UnknownAsset,
                ApiError::UnsupportedAsset | ApiError::Unconfigured => Error::UnsupportedAsset,
                _ => Error::AccountingInvariant,
            })?;
        }
        Ok(())
    }
}
impl<A: AdmissionLookup> FungibleInspect<AccountId> for ContractFungibles<A> {
    type AssetId = u32;
    fn balance(&self, asset: FungibleAsset<u32>, who: &AccountId) -> Balance {
        if Self::check(asset).is_err() {
            return 0;
        }
        SdkFungibles.balance(asset, who)
    }
    fn total_issuance(&self, asset: FungibleAsset<u32>) -> Result<Balance, Error> {
        Self::check(asset)?;
        SdkFungibles.total_issuance(asset)
    }
}
impl<A: AdmissionLookup> FungibleTransfer<AccountId> for ContractFungibles<A> {
    fn transfer(
        &mut self,
        asset: FungibleAsset<u32>,
        from: &AccountId,
        to: &AccountId,
        amount: Balance,
    ) -> Result<(), Error> {
        Self::check(asset)?;
        if from != to && SdkFungibles.balance(asset, from) < amount {
            return Err(Error::InsufficientBalance);
        }
        SdkFungibles
            .transfer_exact(asset, from, to, amount, Preservation::Preserve)
            .map_err(transfer_error)
    }
}

/// Bound the host read before decoding; malformed/oversized SDK storage never becomes defaults.
fn record<T: DecodeAll + Encode + MaxEncodedLen>(key: &[u8]) -> ApiResult<Option<T>> {
    let mut bytes = vec![0u8; T::max_encoded_len()];
    let Some(length) = sp_io::storage::read(key, &mut bytes, 0) else {
        return Ok(None);
    };
    if length as usize > bytes.len() {
        return Err(ApiError::BackendInvariant);
    }
    bytes.truncate(length as usize);
    v1::decode_exact(&bytes)
        .map(Some)
        .map_err(|_| ApiError::BackendInvariant)
}
fn present<T: DecodeAll + Encode + MaxEncodedLen>(key: &[u8]) -> ApiResult<T> {
    record(key)?.ok_or(ApiError::NotFound)
}
fn map_page(error: PageError) -> ApiError {
    match error {
        PageError::InvalidLimit => ApiError::InvalidLimit,
        PageError::InvalidCursor => ApiError::InvalidCursor,
        PageError::BackendInvariant => ApiError::BackendInvariant,
    }
}
fn query_page<T>(
    prefix: &[u8],
    cursor: Option<u32>,
    limit: u32,
    mut lookup: impl FnMut(u32) -> ApiResult<T>,
) -> ApiResult<v1::Page<T>> {
    let (ids, next) = page_ids::<{ v1::PAGE_LIMIT }>(prefix, cursor, limit).map_err(map_page)?;
    let mut entries: BoundedVec<T, ConstU32<{ v1::PAGE_LIMIT }>> = BoundedVec::default();
    for id in ids {
        entries
            .try_push(lookup(id)?)
            .map_err(|_| ApiError::BackendInvariant)?;
    }
    Ok(v1::Page { entries, next })
}

pub(crate) struct SdkReader<A>(PhantomData<A>);
pub(crate) type DormantAssetsApi = SdkReader<AdoptedAdmissions>;
impl<A: AdmissionLookup> SdkReader<A> {
    fn configured() -> ApiResult<()> {
        if A::configured() {
            Ok(())
        } else {
            Err(ApiError::Unconfigured)
        }
    }
    fn admitted_asset(id: u32) -> ApiResult<v1::AssetAdmission> {
        Self::configured()?;
        let entry = A::asset(id)?;
        v1::validate_asset_admission(&entry)?;
        if entry.id != id {
            return Err(ApiError::BackendInvariant);
        }
        Ok(entry)
    }
    fn admitted_collection(id: u32) -> ApiResult<()> {
        Self::configured()?;
        if !(v1::COLLECTION_FIRST..=v1::COLLECTION_LAST).contains(&id) {
            return Err(ApiError::UnsupportedAsset);
        }
        A::collection(id)
    }
}
impl<A: AdmissionLookup> EraV14AssetsApiV1 for SdkReader<A> {
    fn asset_v1(id: u32) -> ApiResult<v1::AssetV1> {
        let admission = Self::admitted_asset(id)?;
        let d: pallet_assets::AssetDetails<Balance, AccountId, Balance> =
            present(&pallet_assets::Asset::<Runtime>::hashed_key_for(id))?;
        if d.is_sufficient {
            return Err(ApiError::UnsupportedAsset);
        }
        if d.min_balance != admission.minimum_balance {
            return Err(ApiError::BackendInvariant);
        }
        let metadata: Option<
            pallet_assets::AssetMetadata<Balance, BoundedVec<u8, crate::AssetStringLimit>>,
        > = record(&pallet_assets::Metadata::<Runtime>::hashed_key_for(id))?;
        let metadata = metadata
            .map(|m| {
                Ok(v1::AssetMetadataV1 {
                    name: m
                        .name
                        .into_inner()
                        .try_into()
                        .map_err(|_| ApiError::BackendInvariant)?,
                    symbol: m
                        .symbol
                        .into_inner()
                        .try_into()
                        .map_err(|_| ApiError::BackendInvariant)?,
                    decimals: m.decimals,
                    frozen: m.is_frozen,
                })
            })
            .transpose()?;
        Ok(v1::AssetV1 {
            id,
            owner: d.owner.into(),
            issuer: d.issuer.into(),
            admin: d.admin.into(),
            freezer: d.freezer.into(),
            supply: d.supply,
            minimum_balance: d.min_balance,
            is_sufficient: d.is_sufficient,
            status: match d.status {
                pallet_assets::AssetStatus::Live => v1::AssetStatusV1::Live,
                pallet_assets::AssetStatus::Frozen => v1::AssetStatusV1::Frozen,
                pallet_assets::AssetStatus::Destroying => v1::AssetStatusV1::Destroying,
            },
            metadata,
        })
    }
    fn assets_v1(cursor: Option<u32>, limit: u32) -> ApiResult<v1::Page<v1::AssetV1>> {
        Self::configured()?;
        query_page(
            &frame_support::storage::storage_prefix(b"Assets", b"Asset"),
            cursor,
            limit,
            Self::asset_v1,
        )
    }
    fn collection_v1(id: u32) -> ApiResult<v1::CollectionV1> {
        Self::admitted_collection(id)?;
        let d: pallet_nfts::CollectionDetails<AccountId, Balance> =
            present(&pallet_nfts::Collection::<Runtime>::hashed_key_for(id))?;
        let metadata: Option<pallet_nfts::CollectionMetadata<Balance, crate::NftStringLimit>> =
            record(&pallet_nfts::CollectionMetadataOf::<Runtime>::hashed_key_for(id))?;
        let roles = collection_roles(id)?;
        Ok(v1::CollectionV1 {
            id,
            owner: d.owner.into(),
            issuer: roles[0],
            admin: roles[1],
            freezer: roles[2],
            item_count: d.items,
            metadata: metadata
                .map(|m| {
                    m.data
                        .into_inner()
                        .try_into()
                        .map_err(|_| ApiError::BackendInvariant)
                })
                .transpose()?,
        })
    }
    fn collections_v1(cursor: Option<u32>, limit: u32) -> ApiResult<v1::Page<v1::CollectionV1>> {
        Self::configured()?;
        query_page(
            &frame_support::storage::storage_prefix(b"Nfts", b"Collection"),
            cursor,
            limit,
            Self::collection_v1,
        )
    }
    fn item_v1(collection: u32, id: u32) -> ApiResult<v1::ItemV1> {
        Self::admitted_collection(collection)?;
        if !(v1::ITEM_FIRST..=v1::ITEM_LAST).contains(&id) {
            return Err(ApiError::UnsupportedAsset);
        }
        let _: pallet_nfts::CollectionDetails<AccountId, Balance> = present(
            &pallet_nfts::Collection::<Runtime>::hashed_key_for(collection),
        )?;
        let d: pallet_nfts::ItemDetails<
            AccountId,
            pallet_nfts::ItemDepositOf<Runtime, ()>,
            pallet_nfts::ApprovalsOf<Runtime>,
        > = present(&pallet_nfts::Item::<Runtime>::hashed_key_for(
            collection, id,
        ))?;
        let metadata: Option<
            pallet_nfts::ItemMetadata<
                pallet_nfts::ItemMetadataDeposit<Balance, AccountId>,
                crate::NftStringLimit,
            >,
        > = record(&pallet_nfts::ItemMetadataOf::<Runtime>::hashed_key_for(
            collection, id,
        ))?;
        Ok(v1::ItemV1 {
            collection,
            id,
            owner: d.owner.into(),
            metadata: metadata
                .map(|m| {
                    m.data
                        .into_inner()
                        .try_into()
                        .map_err(|_| ApiError::BackendInvariant)
                })
                .transpose()?,
        })
    }
    fn items_v1(
        collection: u32,
        cursor: Option<u32>,
        limit: u32,
    ) -> ApiResult<v1::Page<v1::ItemV1>> {
        Self::admitted_collection(collection)?;
        let _: pallet_nfts::CollectionDetails<AccountId, Balance> = present(
            &pallet_nfts::Collection::<Runtime>::hashed_key_for(collection),
        )?;
        let key = pallet_nfts::Item::<Runtime>::hashed_key_for(collection, 0);
        query_page(&key[..key.len() - 20], cursor, limit, |id| {
            Self::item_v1(collection, id)
        })
    }
}

/// At most three role records plus one overflow probe; reject duplicates/corruption, never scan.
fn collection_roles(id: u32) -> ApiResult<[Option<[u8; 32]>; 3]> {
    let full =
        pallet_nfts::CollectionRoleOf::<Runtime>::hashed_key_for(id, AccountId::new([0; 32]));
    let prefix = &full[..full.len() - 48];
    let mut key = prefix.to_vec();
    let mut roles = [None; 3];
    for index in 0..4 {
        let Some(next) = sp_io::storage::next_key(&key) else {
            break;
        };
        if !next.starts_with(prefix) {
            break;
        }
        if index == 3 {
            return Err(ApiError::BackendInvariant);
        }
        let suffix = &next[prefix.len()..];
        if suffix.len() != 48 || suffix[..16] != sp_io::hashing::blake2_128(&suffix[16..]) {
            return Err(ApiError::BackendInvariant);
        }
        let account: [u8; 32] = suffix[16..]
            .try_into()
            .map_err(|_| ApiError::BackendInvariant)?;
        let assigned: pallet_nfts::CollectionRoles = present(&next)?;
        for (slot, role) in [
            pallet_nfts::CollectionRole::Issuer,
            pallet_nfts::CollectionRole::Admin,
            pallet_nfts::CollectionRole::Freezer,
        ]
        .into_iter()
        .enumerate()
        {
            if assigned.has_role(role) && roles[slot].replace(account).is_some() {
                return Err(ApiError::BackendInvariant);
            }
        }
        key = next;
    }
    Ok(roles)
}

/// Storage names are frozen, but no storage alias, genesis, migration or install hook is added.
fn allocator_key(name: &[u8], collection: Option<u32>) -> Vec<u8> {
    let mut key = frame_support::storage::storage_prefix(v1::ALLOCATOR_PREFIX, name).to_vec();
    if let Some(id) = collection {
        let bytes = id.encode();
        key.extend_from_slice(&sp_io::hashing::blake2_128(&bytes));
        key.extend_from_slice(&bytes);
    }
    key
}
fn read_allocator(
    name: &[u8],
    collection: Option<u32>,
) -> Result<v1::AllocatorCursor, v1::CursorError> {
    fn raw<const N: usize>(key: &[u8]) -> Result<Option<Vec<u8>>, v1::CursorError> {
        let mut bytes = [0u8; N];
        let Some(length) = sp_io::storage::read(key, &mut bytes, 0) else {
            return Ok(None);
        };
        if length as usize > N {
            return Err(v1::CursorError::Malformed);
        }
        Ok(Some(bytes[..length as usize].to_vec()))
    }
    let schema = raw::<2>(&allocator_key(v1::SCHEMA_KEY, None))?;
    let state = raw::<5>(&allocator_key(name, collection))?;
    v1::AllocatorCursor::decode(schema.as_deref(), state.as_deref())
}

/// Closed call-shape mapping. SDK execution must still check holder/delegate/payer/lock state;
/// these mappings are not permission grants and are not connected to any dispatch filter.
fn asset_operation(
    call: &pallet_assets::Call<Runtime>,
) -> Result<v1::AssetOperation, v1::ContractError> {
    use pallet_assets::Call as C;
    use v1::AssetOperation as O;
    Ok(match call {
        C::create { min_balance, .. } => {
            if *min_balance == 0 {
                return Err(v1::ContractError::Bound);
            }
            O::Create
        }
        C::mint { .. } => O::Mint,
        C::burn { .. } => O::Burn,
        C::thaw { .. } => O::Thaw,
        C::thaw_asset { .. } => O::ThawAsset,
        C::freeze { .. } => O::Freeze,
        C::freeze_asset { .. } => O::FreezeAsset,
        C::block { .. } => O::Block,
        C::transfer { .. } | C::transfer_keep_alive { .. } => O::Transfer,
        C::approve_transfer { .. } => O::Approve,
        C::cancel_approval { .. } => O::CancelApproval,
        C::transfer_approved { .. } => O::TransferApproved,
        C::set_metadata { name, symbol, .. } => {
            v1::asset_metadata_deposit(
                u32::try_from(name.len()).map_err(|_| v1::ContractError::Bound)?,
                u32::try_from(symbol.len()).map_err(|_| v1::ContractError::Bound)?,
            )?;
            O::SetMetadata
        }
        C::clear_metadata { .. } => O::ClearMetadata,
        C::set_team { .. } => O::SetTeam,
        C::transfer_ownership { .. } => O::TransferOwnership,
        C::start_destroy { .. } => O::StartDestroy,
        C::finish_destroy { .. } => O::FinishDestroy,
        C::destroy_accounts { .. } => O::DestroyAccounts,
        C::destroy_approvals { .. } => O::DestroyApprovals,
        C::touch { .. } => O::Touch,
        C::touch_other { .. } => O::TouchOther,
        C::refund { allow_burn, .. } => {
            v1::validate_refund(0, *allow_burn)?;
            O::Refund
        }
        C::refund_other { .. } => O::Refund,
        _ => return Err(v1::ContractError::Unsupported),
    })
}
fn nft_operation(call: &pallet_nfts::Call<Runtime>) -> Result<v1::NftOperation, v1::ContractError> {
    use pallet_nfts::Call as C;
    use v1::NftOperation as O;
    Ok(match call {
        C::create { config, .. } => {
            if config.mint_settings.mint_type != pallet_nfts::MintType::Issuer
                || config.mint_settings.price.is_some()
                || config.has_disabled_setting(pallet_nfts::CollectionSetting::DepositRequired)
            {
                return Err(v1::ContractError::Unsupported);
            }
            O::Create
        }
        C::mint { .. } => O::Mint,
        C::transfer { .. } => O::Transfer,
        C::burn { .. } => O::Burn,
        C::lock_item_transfer { .. } => O::LockItemTransfer,
        C::unlock_item_transfer { .. } => O::UnlockItemTransfer,
        C::set_metadata { .. } => O::SetMetadata,
        C::clear_metadata { .. } => O::ClearMetadata,
        C::set_collection_metadata { .. } => O::SetCollectionMetadata,
        C::clear_collection_metadata { .. } => O::ClearCollectionMetadata,
        C::lock_item_properties { .. } => O::LockItemProperties,
        C::lock_collection { .. } => O::LockCollection,
        C::set_team {
            issuer,
            admin,
            freezer,
            ..
        } => {
            if issuer.is_none() || admin.is_none() || freezer.is_none() {
                return Err(v1::ContractError::Authority);
            }
            O::SetTeam
        }
        C::transfer_ownership { .. } => O::TransferOwnership,
        C::destroy { witness, .. } => {
            v1::validate_destroy(
                0,
                [
                    witness.item_metadatas,
                    witness.item_configs,
                    witness.attributes,
                ],
                [
                    witness.item_metadatas,
                    witness.item_configs,
                    witness.attributes,
                ],
            )?;
            O::Destroy
        }
        C::set_attribute { namespace, .. } | C::clear_attribute { namespace, .. } => {
            match namespace {
                pallet_nfts::AttributeNamespace::CollectionOwner => O::CollectionAttribute,
                pallet_nfts::AttributeNamespace::ItemOwner => O::ItemAttribute,
                pallet_nfts::AttributeNamespace::Account(_) => O::DelegatedAttribute,
                _ => return Err(v1::ContractError::Unsupported),
            }
        }
        C::approve_item_attributes { .. } => O::ApproveAttributes,
        C::cancel_item_attributes_approval { witness, .. } => {
            v1::bounded_count(witness.account_attributes, v1::REMOVE_LIMIT)?;
            O::CancelAttributes
        }
        C::approve_transfer { .. } => O::ApproveTransfer,
        C::cancel_approval { .. } => O::CancelApproval,
        C::clear_all_transfer_approvals { .. } => O::ClearApprovals,
        C::set_accept_ownership { .. } => O::AcceptOwnership,
        _ => return Err(v1::ContractError::Unsupported),
    })
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum AllocationFailure {
    Cursor(v1::CursorError),
    Backend(DispatchError),
    StateChanged,
    TransactionLimit,
}
/// Trusted internal transaction kernel, invoked only by tests in this dormant gate. It never
/// initializes state: a future caller must supply admitted creator/issuer authorization first.
pub(crate) fn allocate_existing(
    name: &[u8],
    collection: Option<u32>,
    range: era_v14_application_primitives::assets::IdRange,
    occupied: impl FnOnce(u32) -> bool,
    create: impl FnOnce(u32) -> Result<(), DispatchError>,
) -> Result<u32, AllocationFailure> {
    with_transaction_opaque_err(|| {
        let result = (|| {
            let cursor = read_allocator(name, collection).map_err(AllocationFailure::Cursor)?;
            let plan = cursor
                .prepare(range, occupied)
                .map_err(AllocationFailure::Cursor)?;
            create(plan.id).map_err(AllocationFailure::Backend)?;
            if read_allocator(name, collection) != Ok(cursor) {
                return Err(AllocationFailure::StateChanged);
            }
            sp_io::storage::set(&allocator_key(name, collection), &plan.next.encode());
            Ok(plan.id)
        })();
        match result {
            Ok(id) => TransactionOutcome::Commit(Ok(id)),
            Err(error) => TransactionOutcome::Rollback(Err(error)),
        }
    })
    .map_err(|_| AllocationFailure::TransactionLimit)?
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum DepositError {
    Arithmetic,
    Shortfall,
    Backend(DispatchError),
    Conservation,
    TransactionLimit,
}
/// Exact refund/reservation delta against one recorded payer, including the full event rollback.
pub(crate) fn checked_deposit_change(
    payer: &AccountId,
    old: Balance,
    new: Balance,
    action: impl FnOnce() -> Result<(), DispatchError>,
) -> Result<(), DepositError> {
    let free = Balances::free_balance(payer);
    let reserved = Balances::reserved_balance(payer);
    if reserved < old {
        return Err(DepositError::Shortfall);
    }
    let expected_free = free
        .checked_add(old)
        .and_then(|n| n.checked_sub(new))
        .ok_or(DepositError::Arithmetic)?;
    let expected_reserved = reserved
        .checked_sub(old)
        .and_then(|n| n.checked_add(new))
        .ok_or(DepositError::Arithmetic)?;
    with_native_expectations(&[(payer.clone(), expected_free, expected_reserved)], action)
}
fn checked_reserve_move(
    from: &AccountId,
    to: &AccountId,
    amount: Balance,
    action: impl FnOnce() -> Result<(), DispatchError>,
) -> Result<(), DepositError> {
    let from_reserved = Balances::reserved_balance(from);
    if from_reserved < amount {
        return Err(DepositError::Shortfall);
    }
    if from == to {
        return with_native_expectations(
            &[(from.clone(), Balances::free_balance(from), from_reserved)],
            action,
        );
    }
    let to_reserved = Balances::reserved_balance(to)
        .checked_add(amount)
        .ok_or(DepositError::Arithmetic)?;
    with_native_expectations(
        &[
            (
                from.clone(),
                Balances::free_balance(from),
                from_reserved - amount,
            ),
            (to.clone(), Balances::free_balance(to), to_reserved),
        ],
        action,
    )
}
fn with_native_expectations(
    expected: &[(AccountId, Balance, Balance)],
    action: impl FnOnce() -> Result<(), DispatchError>,
) -> Result<(), DepositError> {
    // Only the one-payer and two-party ownership wrappers above call this function.
    if expected.len() > 2 {
        return Err(DepositError::Conservation);
    }
    let issuance = Balances::total_issuance();
    with_transaction_opaque_err(|| {
        let result = action().map_err(DepositError::Backend).and_then(|()| {
            if Balances::total_issuance() != issuance
                || expected.iter().any(|(who, free, reserved)| {
                    Balances::free_balance(who) != *free
                        || Balances::reserved_balance(who) != *reserved
                })
            {
                Err(DepositError::Conservation)
            } else {
                Ok(())
            }
        });
        match result {
            Ok(()) => TransactionOutcome::Commit(Ok(())),
            Err(error) => TransactionOutcome::Rollback(Err(error)),
        }
    })
    .map_err(|_| DepositError::TransactionLimit)?
}
