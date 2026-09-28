//! Bounded restoration of strict SDK vesting locks before ordinary V14 extrinsics.
//!
//! Read at most 128 accounts plus one overflow key. Oversized/corrupt state rejects installation
//! atomically. No identities are embedded, schedules rewritten, or funds moved. This technical
//! installation bound does not limit subsequent standard-pallet schedule creation.

use crate::{AccountId, Balance, Balances, BlockNumber, Runtime, System};
use alloc::vec::Vec;
#[cfg(feature = "try-runtime")]
use codec::Encode;
use codec::{DecodeAll, MaxEncodedLen};
use frame_support::{
    ensure,
    storage::{with_transaction, TransactionOutcome},
    traits::{Get, LockableCurrency, WithdrawReasons},
    weights::Weight,
    BoundedVec,
};
use pallet_vesting::WeightInfo;
use sp_runtime::{traits::ConvertInto, DispatchError, DispatchResult};

pub const MAX_RESTORED_ACCOUNTS: u32 = 128;
const VESTING_LOCK: [u8; 8] = *b"vesting ";
type Schedules = BoundedVec<
    pallet_vesting::VestingInfo<Balance, BlockNumber>,
    pallet_vesting::MaxVestingSchedulesGet<Runtime>,
>;

/// Charge three pinned SDK refreshes per account, plus traversal/verification I/O. Explicit
/// proof allowance covers 128 schedules, 50 locks, account data and trie overhead instead of
/// relying on the SDK's 28-schedule benchmark proof size. No new dispatchable is introduced.
pub fn declared_weight() -> Weight {
    let max_locks = <Runtime as pallet_balances::Config>::MaxLocks::get();
    let schedules = <Runtime as pallet_vesting::Config>::MAX_VESTING_SCHEDULES;
    pallet_vesting::weights::SubstrateWeight::<Runtime>::vest_other_locked(max_locks, schedules)
        .saturating_mul(3)
        .saturating_add(<Runtime as frame_system::Config>::DbWeight::get().reads_writes(12, 6))
        .saturating_add(Weight::from_parts(0, 32_768))
        .saturating_mul(u64::from(MAX_RESTORED_ACCOUNTS))
        .saturating_add(<Runtime as frame_system::Config>::DbWeight::get().reads(2))
}

// The SDK intentionally uses WeakBoundedVec for locks. Validate its raw record against the
// actual runtime limit before any SDK getter can allocate/iterate it during installation.
fn validate_lock_record(who: &AccountId) -> DispatchResult {
    type Locks = BoundedVec<
        pallet_balances::BalanceLock<Balance>,
        <Runtime as pallet_balances::Config>::MaxLocks,
    >;
    let key = pallet_balances::Locks::<Runtime>::hashed_key_for(who);
    let mut bytes = alloc::vec![0; Locks::max_encoded_len()];
    if let Some(length) = sp_io::storage::read(&key, &mut bytes, 0) {
        ensure!(
            (length as usize) <= bytes.len(),
            DispatchError::Other("WS3 oversized balance lock record")
        );
        Locks::decode_all(&mut &bytes[..length as usize])
            .map_err(|_| DispatchError::Other("WS3 malformed balance lock record"))?;
    }
    Ok(())
}

fn records() -> Result<Vec<(AccountId, Schedules)>, DispatchError> {
    let prefix = frame_support::storage::storage_prefix(b"Vesting", b"Vesting");
    let mut cursor = prefix.to_vec();
    let mut result = Vec::new();
    ensure!(
        !sp_io::storage::exists(&prefix),
        DispatchError::Other("WS3 malformed vesting prefix")
    );
    for index in 0..=MAX_RESTORED_ACCOUNTS {
        let Some(key) = sp_io::storage::next_key(&cursor) else {
            return Ok(result);
        };
        if !key.starts_with(&prefix) {
            return Ok(result);
        }
        ensure!(
            index < MAX_RESTORED_ACCOUNTS,
            DispatchError::Other("WS3 vesting account restoration bound exceeded")
        );
        // Pinned map: Blake2_128Concat<AccountId32>. Do not skip malformed entries.
        ensure!(
            key.len() == 80,
            DispatchError::Other("WS3 malformed vesting key")
        );
        let who = AccountId::decode_all(&mut &key[48..])
            .map_err(|_| DispatchError::Other("WS3 invalid vesting account encoding"))?;
        ensure!(
            key == pallet_vesting::Vesting::<Runtime>::hashed_key_for(&who),
            DispatchError::Other("WS3 vesting key hash mismatch")
        );
        let mut bytes = alloc::vec![0; Schedules::max_encoded_len()];
        let length = sp_io::storage::read(&key, &mut bytes, 0)
            .ok_or(DispatchError::Other("WS3 vesting record disappeared"))?
            as usize;
        ensure!(
            length <= bytes.len(),
            DispatchError::Other("WS3 oversized vesting record")
        );
        let schedules = Schedules::decode_all(&mut &bytes[..length])
            .map_err(|_| DispatchError::Other("WS3 malformed vesting schedules"))?;
        ensure!(
            !schedules.is_empty(),
            DispatchError::Other("WS3 empty vesting record")
        );
        validate_lock_record(&who)?;
        result.push((who, schedules));
        cursor = key;
    }
    Err(DispatchError::Other("WS3 vesting traversal exhausted"))
}

fn locked_now(schedules: &Schedules) -> Result<Balance, DispatchError> {
    schedules.iter().try_fold(0u128, |total, schedule| {
        ensure!(
            schedule.is_valid(),
            DispatchError::Other("WS3 invalid vesting terms")
        );
        // SDK checked multiplication overflow means fully matured: mathematically the released
        // amount then exceeds u128 principal. Aggregate addition must reject, never saturate.
        total
            .checked_add(schedule.locked_at::<ConvertInto>(System::block_number()))
            .ok_or(DispatchError::Other("WS3 aggregate vesting overflow"))
    })
}

fn lock_matches(who: &AccountId, locked: Balance) -> bool {
    let locks = Balances::locks(who);
    let mut vesting = locks.iter().filter(|lock| lock.id == VESTING_LOCK);
    let first = vesting.next();
    let expected = if locked == 0 {
        first.is_none()
    } else {
        first.is_some_and(|lock| {
            lock.amount == locked
                && lock.reasons == pallet_balances::Reasons::All
                && System::account(who).data.frozen >= locked
        })
    };
    expected && vesting.next().is_none()
}

fn restore() -> DispatchResult {
    for (who, schedules) in records()? {
        let locked = locked_now(&schedules)?;
        if lock_matches(&who, locked) {
            continue;
        }
        let locks = Balances::locks(&who);
        ensure!(
            locked == 0
                || locks.iter().any(|lock| lock.id == VESTING_LOCK)
                || locks.len()
                    < <<Runtime as pallet_balances::Config>::MaxLocks as Get<u32>>::get() as usize,
            DispatchError::Other("WS3 vesting lock capacity exceeded")
        );
        if locked == 0 {
            Balances::remove_lock(VESTING_LOCK, &who);
        } else {
            Balances::set_lock(VESTING_LOCK, &who, locked, WithdrawReasons::all());
        }
        ensure!(
            lock_matches(&who, locked),
            DispatchError::Other("WS3 lock restoration failed")
        );
    }
    Ok(())
}

/// The outer V14 transaction also rolls back economic migration writes. Replaying correct locks
/// at the same block performs no writes and emits no events. No beneficiary signature is needed.
pub(crate) fn restore_locks() -> DispatchResult {
    with_transaction(|| match restore() {
        Ok(()) => TransactionOutcome::Commit(Ok(())),
        Err(error) => TransactionOutcome::Rollback(Err(error)),
    })
}

#[cfg(feature = "try-runtime")]
pub(crate) fn validate_locks() -> DispatchResult {
    for (who, schedules) in records()? {
        ensure!(
            lock_matches(&who, locked_now(&schedules)?),
            DispatchError::Other("WS3 vesting protection missing")
        );
    }
    Ok(())
}

/// Bounded try-runtime snapshot: schedule bytes, balances, other locks, holds and freezes must
/// remain exact; vesting-lock/provider changes are expected.
#[cfg(feature = "try-runtime")]
pub(crate) fn preserved_state() -> Result<Vec<u8>, DispatchError> {
    let mut state = Vec::new();
    for (who, schedules) in records()? {
        let other_locks = Balances::locks(&who)
            .into_iter()
            .filter(|lock| lock.id != VESTING_LOCK)
            .collect::<Vec<_>>();
        (
            &who,
            schedules,
            Balances::free_balance(&who),
            Balances::reserved_balance(&who),
            other_locks,
            pallet_balances::Holds::<Runtime>::get(&who),
            pallet_balances::Freezes::<Runtime>::get(&who),
        )
            .encode_to(&mut state);
    }
    Ok(state)
}
