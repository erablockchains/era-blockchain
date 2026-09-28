// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0
//! Bounded raw traversal. Every successor returned is either decoded once or rejected;
//! malformed keys are never skipped. Transactions also cover callers of owning helpers.
use crate::*;
use codec::{DecodeAll, MaxEncodedLen};
use frame_support::{
    pallet_prelude::*, storage::storage_prefix, traits::PalletInfoAccess, Blake2_128Concat,
    StorageHasher,
};
use sp_runtime::traits::{CheckedAdd, CheckedSub};

type Details<T, I> = CollectionDetails<<T as SystemConfig>::AccountId, DepositBalanceOf<T, I>>;
type Refund<T, I> = (
    Option<<T as SystemConfig>::AccountId>,
    DepositBalanceOf<T, I>,
);
const PHASES: [&str; 8] = [
    "ItemMetadataOf",
    "Attribute",
    "ItemConfigOf",
    "ItemPriceOf",
    "PendingSwapOf",
    "ItemAttributesApprovalsOf",
    "CollectionRoleOf",
    "DelegateCleanup",
];

impl<T: Config<I>, I: 'static> Pallet<T, I> {
    /// An existence check deliberately also rejects malformed progress.
    pub(crate) fn ensure_collection_active(collection: T::CollectionId) -> DispatchResult {
        ensure!(
            !sp_io::storage::exists(&CollectionRetirement::<T, I>::hashed_key_for(collection)),
            Error::<T, I>::CollectionRetiring
        );
        Ok(())
    }
    pub(crate) fn ensure_namespace_active(
        collection: T::CollectionId,
        item: Option<T::ItemId>,
        namespace: &AttributeNamespace<T::AccountId>,
    ) -> DispatchResult {
        if let (Some(item), AttributeNamespace::Account(delegate)) = (item, namespace) {
            ensure!(
                !sp_io::storage::exists(&DelegateCleanup::<T, I>::hashed_key_for(
                    collection,
                    (item, delegate)
                )),
                Error::<T, I>::DelegateCleanupPending
            );
        }
        Ok(())
    }
    fn limit(limit: u32, allow_zero: bool) -> DispatchResult {
        ensure!(
            limit <= CLEANUP_LIMIT && (allow_zero || limit > 0),
            Error::<T, I>::CleanupLimitExceeded
        );
        Ok(())
    }
    pub(crate) fn cleanup_prefix(name: &str, collection: T::CollectionId) -> Vec<u8> {
        let mut p = storage_prefix(Self::name().as_bytes(), name.as_bytes()).to_vec();
        p.extend(Blake2_128Concat::hash(&collection.encode()));
        p
    }
    /// Bounded value read, including trailing-byte rejection. Never allocate from raw length.
    pub(crate) fn cleanup_read<V: Decode + MaxEncodedLen>(key: &[u8]) -> Result<V, DispatchError> {
        let mut buf = vec![0; V::max_encoded_len()];
        let n = sp_io::storage::read(key, &mut buf, 0).ok_or(Error::<T, I>::CleanupStateInvalid)?
            as usize;
        ensure!(n <= buf.len(), Error::<T, I>::CleanupStateInvalid);
        V::decode_all(&mut &buf[..n]).map_err(|_| Error::<T, I>::CleanupStateInvalid.into())
    }
    /// One exact-prefix existence check and one successor probe, including overlay entries.
    pub(crate) fn cleanup_first(prefix: &[u8]) -> Result<Option<Vec<u8>>, DispatchError> {
        ensure!(
            !sp_io::storage::exists(prefix),
            Error::<T, I>::CleanupStateInvalid
        );
        Ok(sp_io::storage::next_key(prefix).filter(|key| key.starts_with(prefix)))
    }
    fn empty(prefix: &[u8]) -> DispatchResult {
        ensure!(
            Self::cleanup_first(prefix)?.is_none(),
            Error::<T, I>::CleanupStateInvalid
        );
        Ok(())
    }
    fn concat<K: Decode + Encode + MaxEncodedLen>(bytes: &mut &[u8]) -> Result<K, DispatchError> {
        ensure!(bytes.len() >= 16, Error::<T, I>::CleanupStateInvalid);
        let hash = &bytes[..16];
        *bytes = &bytes[16..];
        let before = *bytes;
        let value = K::decode(bytes).map_err(|_| Error::<T, I>::CleanupStateInvalid)?;
        let consumed = before.len() - bytes.len();
        ensure!(
            consumed <= K::max_encoded_len()
                && value.encode() == before[..consumed]
                && sp_io::hashing::blake2_128(&before[..consumed]) == hash,
            Error::<T, I>::CleanupStateInvalid
        );
        Ok(value)
    }
    fn validate_record(
        collection: T::CollectionId,
        phase: u8,
        key: &[u8],
    ) -> Result<Option<Refund<T, I>>, DispatchError> {
        let name = PHASES
            .get(phase as usize)
            .ok_or(Error::<T, I>::CleanupStateInvalid)?;
        let prefix = Self::cleanup_prefix(name, collection);
        ensure!(key.starts_with(&prefix), Error::<T, I>::CleanupStateInvalid);
        let mut suffix = &key[prefix.len()..];
        // Reject oversized raw keys before any SCALE decode.
        let max_suffix = match phase {
            1 => {
                64 + Option::<T::ItemId>::max_encoded_len()
                    + AttributeNamespace::<T::AccountId>::max_encoded_len()
                    + BoundedVec::<u8, T::KeyLimit>::max_encoded_len()
            }
            6 => 16 + T::AccountId::max_encoded_len(),
            7 => 16 + <(T::ItemId, T::AccountId)>::max_encoded_len(),
            _ => 16 + T::ItemId::max_encoded_len(),
        };
        ensure!(
            suffix.len() <= max_suffix,
            Error::<T, I>::CleanupStateInvalid
        );
        let mut attribute_namespace = None;
        match phase {
            1 => {
                Self::concat::<Option<T::ItemId>>(&mut suffix)?;
                attribute_namespace = Some(Self::concat::<AttributeNamespace<T::AccountId>>(
                    &mut suffix,
                )?);
                Self::concat::<BoundedVec<u8, T::KeyLimit>>(&mut suffix)?;
            }
            6 => {
                Self::concat::<T::AccountId>(&mut suffix)?;
            }
            7 => {
                Self::concat::<(T::ItemId, T::AccountId)>(&mut suffix)?;
            }
            _ => {
                Self::concat::<T::ItemId>(&mut suffix)?;
            }
        }
        ensure!(suffix.is_empty(), Error::<T, I>::CleanupStateInvalid);
        let refund = match phase {
            0 => {
                let m: ItemMetadata<ItemMetadataDepositOf<T, I>, T::StringLimit> =
                    Self::cleanup_read(key)?;
                Some((m.deposit.account, m.deposit.amount))
            }
            1 => {
                let (_, d): (BoundedVec<u8, T::ValueLimit>, AttributeDepositOf<T, I>) =
                    Self::cleanup_read(key)?;
                ensure!(
                    d.account.is_some()
                        || d.amount.is_zero()
                        || matches!(
                            attribute_namespace,
                            Some(AttributeNamespace::CollectionOwner)
                        ),
                    Error::<T, I>::CleanupStateInvalid
                );
                Some((d.account, d.amount))
            }
            2 => {
                let _: ItemConfig = Self::cleanup_read(key)?;
                None
            }
            3 => {
                let _: (ItemPrice<T, I>, Option<T::AccountId>) = Self::cleanup_read(key)?;
                None
            }
            4 => {
                let _: PendingSwap<
                    T::CollectionId,
                    T::ItemId,
                    PriceWithDirection<ItemPrice<T, I>>,
                    BlockNumberFor<T, I>,
                > = Self::cleanup_read(key)?;
                None
            }
            5 => {
                let _: ItemAttributesApprovals<T, I> = Self::cleanup_read(key)?;
                None
            }
            6 => {
                let _: CollectionRoles = Self::cleanup_read(key)?;
                None
            }
            7 => {
                let p: CleanupProgress = Self::cleanup_read(key)?;
                ensure!(
                    p.version == 1 && p.phase == 0,
                    Error::<T, I>::CleanupStateInvalid
                );
                None
            }
            _ => return Err(Error::<T, I>::CleanupStateInvalid.into()),
        };
        Ok(refund)
    }
    /// Exactly `witness + 1` successor probes at most. The extra key is never decoded.
    pub(crate) fn cleanup_preflight(
        collection: T::CollectionId,
        phase: u8,
        prefix: &[u8],
        witness: u32,
    ) -> Result<Vec<Vec<u8>>, DispatchError> {
        let mut keys = Vec::new();
        let mut next = Self::cleanup_first(prefix)?;
        while let Some(key) = next {
            ensure!(keys.len() < witness as usize, Error::<T, I>::BadWitness);
            Self::validate_record(collection, phase, &key)?;
            next = sp_io::storage::next_key(&key).filter(|k| k.starts_with(prefix));
            keys.push(key);
        }
        Ok(keys)
    }
    pub(crate) fn refund_exact(
        account: &T::AccountId,
        amount: DepositBalanceOf<T, I>,
    ) -> DispatchResult {
        let free = T::Currency::free_balance(account);
        let reserved = T::Currency::reserved_balance(account);
        let expected_free = free
            .checked_add(&amount)
            .ok_or(Error::<T, I>::RefundFailed)?;
        let expected_reserved = reserved
            .checked_sub(&amount)
            .ok_or(Error::<T, I>::RefundFailed)?;
        ensure!(
            T::Currency::unreserve(account, amount).is_zero(),
            Error::<T, I>::RefundFailed
        );
        ensure!(
            T::Currency::free_balance(account) == expected_free
                && T::Currency::reserved_balance(account) == expected_reserved,
            Error::<T, I>::RefundFailed
        );
        Ok(())
    }
    pub(crate) fn refund_record(
        details: &mut Details<T, I>,
        payer: Option<T::AccountId>,
        amount: DepositBalanceOf<T, I>,
    ) -> DispatchResult {
        if let Some(payer) = payer {
            Self::refund_exact(&payer, amount)
        } else {
            details.owner_deposit = details
                .owner_deposit
                .checked_sub(&amount)
                .ok_or(Error::<T, I>::CleanupStateInvalid)?;
            Self::refund_exact(&details.owner, amount)
        }
    }
    fn remove_record(
        collection: T::CollectionId,
        phase: u8,
        key: &[u8],
        details: &mut Details<T, I>,
    ) -> DispatchResult {
        if let Some((payer, amount)) = Self::validate_record(collection, phase, key)? {
            Self::refund_record(details, payer, amount)?;
        }
        let counter = match phase {
            0 => Some(&mut details.item_metadatas),
            1 => Some(&mut details.attributes),
            2 => Some(&mut details.item_configs),
            _ => None,
        };
        if let Some(counter) = counter {
            *counter = counter
                .checked_sub(1)
                .ok_or(Error::<T, I>::CleanupStateInvalid)?;
        }
        sp_io::storage::clear(key);
        Ok(())
    }
    fn empty_collection(collection: T::CollectionId) -> Result<Details<T, I>, DispatchError> {
        ensure!(
            Collection::<T, I>::contains_key(collection),
            Error::<T, I>::UnknownCollection
        );
        let details: Details<T, I> =
            Self::cleanup_read(&Collection::<T, I>::hashed_key_for(collection))?;
        ensure!(details.items == 0, Error::<T, I>::CollectionNotEmpty);
        ensure!(
            Self::cleanup_first(&Self::cleanup_prefix("Item", collection))?.is_none(),
            Error::<T, I>::CollectionNotEmpty
        );
        Ok(details)
    }
    // Direct destroy rejects malformed fixed records before its first deletion or refund.
    fn preflight_fixed(collection: T::CollectionId, details: &Details<T, I>) -> DispatchResult {
        let key = CollectionMetadataOf::<T, I>::hashed_key_for(collection);
        if sp_io::storage::exists(&key) {
            let m: CollectionMetadata<DepositBalanceOf<T, I>, T::StringLimit> =
                Self::cleanup_read(&key)?;
            ensure!(
                details.owner_deposit >= m.deposit,
                Error::<T, I>::CleanupStateInvalid
            );
        }
        let _: CollectionConfigFor<T, I> =
            Self::cleanup_read(&CollectionConfigOf::<T, I>::hashed_key_for(collection))?;
        let _: () = Self::cleanup_read(&CollectionAccount::<T, I>::hashed_key_for(
            &details.owner,
            collection,
        ))?;
        Ok(())
    }
    /// Fixed finalization: eight empty phase proofs, one live-item proof, bounded scalars,
    /// collection ownership index and one remaining owner aggregate refund.
    fn finalize(
        collection: T::CollectionId,
        mut details: Details<T, I>,
        reconcile: bool,
    ) -> DispatchResult {
        Self::empty(&Self::cleanup_prefix("Item", collection))?;
        for name in PHASES {
            Self::empty(&Self::cleanup_prefix(name, collection))?;
        }
        ensure!(
            details.items == 0 && details.item_metadatas == 0 && details.item_configs == 0,
            Error::<T, I>::CleanupStateInvalid
        );
        if details.attributes != 0 {
            ensure!(reconcile, Error::<T, I>::CleanupStateInvalid);
            Self::deposit_event(Event::AttributeCountReconciled {
                collection,
                discrepancy: details.attributes,
            });
            details.attributes = 0;
        }
        let key = CollectionMetadataOf::<T, I>::hashed_key_for(collection);
        if sp_io::storage::exists(&key) {
            let m: CollectionMetadata<DepositBalanceOf<T, I>, T::StringLimit> =
                Self::cleanup_read(&key)?;
            Self::refund_record(&mut details, None, m.deposit)?;
            sp_io::storage::clear(&key);
        }
        let _: CollectionConfigFor<T, I> =
            Self::cleanup_read(&CollectionConfigOf::<T, I>::hashed_key_for(collection))?;
        let _: () = Self::cleanup_read(&CollectionAccount::<T, I>::hashed_key_for(
            &details.owner,
            collection,
        ))?;
        Self::refund_exact(&details.owner, details.owner_deposit)?;
        CollectionAccount::<T, I>::remove(&details.owner, collection);
        CollectionConfigOf::<T, I>::remove(collection);
        Collection::<T, I>::remove(collection);
        CollectionRetirement::<T, I>::remove(collection);
        Self::deposit_event(Event::Destroyed { collection });
        Ok(())
    }
    #[frame_support::transactional]
    pub(crate) fn bounded_destroy(
        collection: T::CollectionId,
        witness: DestroyWitness,
        owner: Option<T::AccountId>,
    ) -> Result<DestroyWitness, DispatchError> {
        for n in [
            witness.item_metadatas,
            witness.attributes,
            witness.item_configs,
        ] {
            Self::limit(n, true)?;
        }
        Self::ensure_collection_active(collection)?;
        let mut details: Details<T, I> =
            Self::cleanup_read(&Collection::<T, I>::hashed_key_for(collection))?;
        if let Some(owner) = owner {
            ensure!(owner == details.owner, Error::<T, I>::NoPermission);
        }
        Self::empty_collection(collection)?;
        ensure!(
            details.item_metadatas == witness.item_metadatas
                && details.attributes == witness.attributes
                && details.item_configs == witness.item_configs,
            Error::<T, I>::BadWitness
        );
        Self::preflight_fixed(collection, &details)?;
        let mut records = Vec::new();
        for (phase, count) in [
            (0, witness.item_metadatas),
            (1, witness.attributes),
            (2, witness.item_configs),
            (6, 3),
        ] {
            let keys = Self::cleanup_preflight(
                collection,
                phase,
                &Self::cleanup_prefix(PHASES[phase as usize], collection),
                count,
            )?;
            ensure!(
                phase == 6 || keys.len() == count as usize,
                Error::<T, I>::BadWitness
            );
            records.push((phase, keys));
        }
        for phase in [3, 4, 5, 7] {
            Self::empty(&Self::cleanup_prefix(PHASES[phase], collection))?;
        }
        for (phase, keys) in records {
            for key in keys {
                Self::remove_record(collection, phase, &key, &mut details)?;
            }
        }
        Self::finalize(collection, details, false)?;
        Ok(witness)
    }
    /// Starts only on the real empty live-item prefix; counter zero alone is insufficient.
    #[frame_support::transactional]
    pub fn do_start_collection_retirement(
        owner: T::AccountId,
        collection: T::CollectionId,
    ) -> DispatchResult {
        Self::ensure_collection_active(collection)?;
        let details = Self::empty_collection(collection)?;
        ensure!(details.owner == owner, Error::<T, I>::NoPermission);
        CollectionRetirement::<T, I>::insert(
            collection,
            CleanupProgress {
                version: 1,
                phase: 0,
            },
        );
        Self::deposit_event(Event::CollectionRetirementStarted { collection });
        Ok(())
    }
    /// At most limit raw removals, at most limit+8 successor probes for the processing loop.
    /// Phase validation and finalization each add at most eight fixed emptiness proofs.
    #[frame_support::transactional]
    pub fn do_continue_collection_retirement(
        owner: T::AccountId,
        collection: T::CollectionId,
        limit: u32,
    ) -> DispatchResult {
        Self::limit(limit, false)?;
        ensure!(
            CollectionRetirement::<T, I>::contains_key(collection),
            Error::<T, I>::NotRetiring
        );
        let mut p: CleanupProgress =
            Self::cleanup_read(&CollectionRetirement::<T, I>::hashed_key_for(collection))?;
        ensure!(
            p.version == 1 && (p.phase as usize) < PHASES.len(),
            Error::<T, I>::CleanupStateInvalid
        );
        let mut details = Self::empty_collection(collection)?;
        ensure!(details.owner == owner, Error::<T, I>::NoPermission);
        for name in &PHASES[..p.phase as usize] {
            Self::empty(&Self::cleanup_prefix(name, collection))?;
        }
        let mut removed = 0;
        'phases: while (p.phase as usize) < PHASES.len() {
            let prefix = Self::cleanup_prefix(PHASES[p.phase as usize], collection);
            let mut next = Self::cleanup_first(&prefix)?;
            while let Some(key) = next {
                if removed == limit {
                    break 'phases;
                }
                Self::remove_record(collection, p.phase, &key, &mut details)?;
                removed += 1;
                next = sp_io::storage::next_key(&key).filter(|k| k.starts_with(&prefix));
            }
            p.phase += 1;
        }
        Self::deposit_event(Event::CollectionRetirementProgress {
            collection,
            removed,
            phase: p.phase,
        });
        if p.phase as usize == PHASES.len() {
            Self::finalize(collection, details, true)
        } else {
            Collection::<T, I>::insert(collection, details);
            CollectionRetirement::<T, I>::insert(collection, p);
            Ok(())
        }
    }
    fn delegate_prefix(
        collection: T::CollectionId,
        item: T::ItemId,
        delegate: &T::AccountId,
    ) -> Vec<u8> {
        let mut p = Self::cleanup_prefix("Attribute", collection);
        p.extend(Blake2_128Concat::hash(&Some(item).encode()));
        p.extend(Blake2_128Concat::hash(
            &AttributeNamespace::Account(delegate.clone()).encode(),
        ));
        p
    }
    fn delegate_owner(
        owner: &T::AccountId,
        collection: T::CollectionId,
        item: T::ItemId,
    ) -> DispatchResult {
        let details: ItemDetails<T::AccountId, ItemDepositOf<T, I>, ApprovalsOf<T, I>> =
            Self::cleanup_read(&Item::<T, I>::hashed_key_for(collection, item))?;
        ensure!(&details.owner == owner, Error::<T, I>::NoPermission);
        Ok(())
    }
    fn revoke_delegate(
        collection: T::CollectionId,
        item: T::ItemId,
        delegate: &T::AccountId,
    ) -> DispatchResult {
        let key = ItemAttributesApprovalsOf::<T, I>::hashed_key_for(collection, item);
        if sp_io::storage::exists(&key) {
            let mut approvals: ItemAttributesApprovals<T, I> = Self::cleanup_read(&key)?;
            approvals.remove(delegate);
            if approvals.is_empty() {
                sp_io::storage::clear(&key);
            } else {
                ItemAttributesApprovalsOf::<T, I>::insert(collection, item, approvals);
            }
        }
        Ok(())
    }
    #[frame_support::transactional]
    pub(crate) fn bounded_cancel(
        owner: T::AccountId,
        collection: T::CollectionId,
        item: T::ItemId,
        delegate: T::AccountId,
        witness: CancelAttributesApprovalWitness,
    ) -> DispatchResult {
        Self::limit(witness.account_attributes, true)?;
        Self::ensure_collection_active(collection)?;
        ensure!(
            Self::is_pallet_feature_enabled(PalletFeature::Attributes),
            Error::<T, I>::MethodDisabled
        );
        Self::delegate_owner(&owner, collection, item)?;
        Self::ensure_namespace_active(
            collection,
            Some(item),
            &AttributeNamespace::Account(delegate.clone()),
        )?;
        let prefix = Self::delegate_prefix(collection, item, &delegate);
        let keys = Self::cleanup_preflight(collection, 1, &prefix, witness.account_attributes)?;
        let mut details: Details<T, I> =
            Self::cleanup_read(&Collection::<T, I>::hashed_key_for(collection))?;
        Self::revoke_delegate(collection, item, &delegate)?;
        for key in keys {
            Self::remove_record(collection, 1, &key, &mut details)?;
        }
        Self::empty(&prefix)?;
        Collection::<T, I>::insert(collection, details);
        Self::deposit_event(Event::ItemAttributesApprovalRemoved {
            collection,
            item,
            delegate,
        });
        Ok(())
    }
    /// Starts and continues cleanup. Burn leaves the marker for collection retirement;
    /// a burned item has no owner and therefore cannot authorize this live-item operation.
    #[frame_support::transactional]
    pub fn do_continue_delegate_cleanup(
        owner: T::AccountId,
        collection: T::CollectionId,
        item: T::ItemId,
        delegate: T::AccountId,
        limit: u32,
    ) -> DispatchResult {
        Self::limit(limit, false)?;
        Self::ensure_collection_active(collection)?;
        ensure!(
            Self::is_pallet_feature_enabled(PalletFeature::Attributes),
            Error::<T, I>::MethodDisabled
        );
        Self::delegate_owner(&owner, collection, item)?;
        let marker = DelegateCleanup::<T, I>::hashed_key_for(collection, (item, &delegate));
        if sp_io::storage::exists(&marker) {
            let p: CleanupProgress = Self::cleanup_read(&marker)?;
            ensure!(
                p.version == 1 && p.phase == 0,
                Error::<T, I>::CleanupStateInvalid
            );
        }
        let mut details: Details<T, I> =
            Self::cleanup_read(&Collection::<T, I>::hashed_key_for(collection))?;
        Self::revoke_delegate(collection, item, &delegate)?;
        let prefix = Self::delegate_prefix(collection, item, &delegate);
        let mut removed = 0;
        let mut next = Self::cleanup_first(&prefix)?;
        while let Some(ref key) = next {
            if removed == limit {
                break;
            }
            Self::remove_record(collection, 1, &key, &mut details)?;
            removed += 1;
            next = sp_io::storage::next_key(key).filter(|k| k.starts_with(&prefix));
        }
        let complete = next.is_none();
        if complete {
            sp_io::storage::clear(&marker);
        } else {
            DelegateCleanup::<T, I>::insert(
                collection,
                (item, &delegate),
                CleanupProgress {
                    version: 1,
                    phase: 0,
                },
            );
        }
        Collection::<T, I>::insert(collection, details);
        Self::deposit_event(Event::ItemAttributesApprovalRemoved {
            collection,
            item,
            delegate: delegate.clone(),
        });
        Self::deposit_event(Event::DelegateCleanupProgress {
            collection,
            item,
            delegate,
            removed,
            complete,
        });
        Ok(())
    }
}
