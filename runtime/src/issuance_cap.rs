//! Persistent, burn-independent control of post-V13 native-token issuance.
//!
//! The remaining allowance is initialized exactly once by the ordered V13 runtime migration, and
//! only after the native total issuance equals the approved retained-supply baseline. All
//! production code that genuinely creates ETKN must use [`Pallet::controlled_mint_into`].

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
    use frame_support::{
        pallet_prelude::*,
        storage::{with_transaction, TransactionOutcome},
        traits::fungible::{Inspect, Mutate},
    };
    use frame_system::pallet_prelude::*;
    use sp_runtime::{DispatchError, RuntimeDebug};

    use crate::{
        upgrade13_policy::{
            ABSOLUTE_LIFETIME_CAP, MAXIMUM_POST_CORRECTION_NEW_ISSUANCE, TARGET_RETAINED_ISSUANCE,
        },
        Balance,
    };

    const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

    /// Custody identities authorized immediately before the V13 runtime upgrade.
    ///
    /// The four destinations were deliberately left symbolic in the approved reconciliation
    /// plan. This record is the only deployment input: the runtime supplies every amount and
    /// source account and rejects an input for a different approved-vector hash.
    #[derive(
        Clone,
        Encode,
        Decode,
        DecodeWithMemTracking,
        PartialEq,
        Eq,
        RuntimeDebug,
        TypeInfo,
        MaxEncodedLen,
    )]
    pub struct V13MigrationInput<AccountId> {
        pub reconciliation_vector_hash: [u8; 32],
        pub active_presale_custody: AccountId,
        pub ecosystem_custody: AccountId,
        pub liquidity_custody: AccountId,
        pub community_onboarding_custody: AccountId,
    }

    /// Independent proof that the ordered V13 migration committed in full.
    #[derive(
        Clone,
        Encode,
        Decode,
        DecodeWithMemTracking,
        PartialEq,
        Eq,
        RuntimeDebug,
        TypeInfo,
        MaxEncodedLen,
    )]
    pub struct V13Completion<AccountId, BlockNumber> {
        pub migration_version: u16,
        pub completed_at: BlockNumber,
        pub input: V13MigrationInput<AccountId>,
    }

    /// Prefix used for the one-time raw deployment input.
    ///
    /// This deliberately is not under `IssuanceCap`: FRAME must first recognize that pallet
    /// as new and initialize its declared storage version before `SingleBlockMigrations` runs.
    pub const V13_MIGRATION_INPUT_PALLET_PREFIX: &[u8] = b"EraV13Migration";
    pub const V13_MIGRATION_INPUT_STORAGE_PREFIX: &[u8] = b"Input";

    pub fn v13_migration_input_key() -> [u8; 32] {
        frame_support::storage::storage_prefix(
            V13_MIGRATION_INPUT_PALLET_PREFIX,
            V13_MIGRATION_INPUT_STORAGE_PREFIX,
        )
    }

    pub fn v13_migration_input<T: Config>() -> Option<V13MigrationInput<T::AccountId>> {
        frame_support::storage::unhashed::get(&v13_migration_input_key())
    }

    pub(crate) fn clear_v13_migration_input() {
        frame_support::storage::unhashed::kill(&v13_migration_input_key());
    }

    #[pallet::config]
    pub trait Config: frame_system::Config<RuntimeEvent: From<Event<Self>>> {
        /// The native currency. Direct production minting through this type is forbidden; callers
        /// must use `controlled_mint_into` so the shared lifetime allowance is consumed atomically.
        type Currency: Inspect<Self::AccountId, Balance = Balance>
            + Mutate<Self::AccountId, Balance = Balance>;
    }

    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    /// Burn-independent ETKN issuance still authorized after the V13 retained-supply baseline.
    /// `None` is deliberately distinct from a fully consumed allowance of `Some(0)`.
    #[pallet::storage]
    #[pallet::getter(fn remaining_allowance)]
    pub type RemainingAllowance<T> = StorageValue<_, Balance, OptionQuery>;

    /// Explicit V13 completion marker, independent of FRAME pallet storage-version setup.
    #[pallet::storage]
    pub type V13MigrationCompleted<T: Config> =
        StorageValue<_, V13Completion<T::AccountId, BlockNumberFor<T>>, OptionQuery>;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// New ETKN was created through the single controlled interface.
        Issued {
            beneficiary: T::AccountId,
            amount: Balance,
            remaining_allowance: Balance,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        /// The V13 baseline gate has not initialized the shared allowance.
        AllowanceNotInitialized,
        /// A zero-value request is not an issuance operation.
        ZeroIssuance,
        /// The request exceeds the burn-independent remaining lifetime allowance.
        AllowanceExceeded,
        /// Stored allowance is outside the single approved initialization range.
        InvalidStoredAllowance,
        /// Native issuance is already above the approved lifetime ceiling.
        IssuanceAboveLifetimeCeiling,
        /// The requested issuance would exceed the approved lifetime ceiling.
        LifetimeCeilingExceeded,
        /// Base-unit arithmetic overflowed.
        ArithmeticOverflow,
        /// The native currency rejected the mint after allowance validation.
        DownstreamMintFailed,
        /// The currency reported an amount or total-issuance delta other than the exact request.
        UnexpectedIssuanceDelta,
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        fn integrity_test() {
            assert_eq!(
                TARGET_RETAINED_ISSUANCE.checked_add(MAXIMUM_POST_CORRECTION_NEW_ISSUANCE),
                Some(ABSOLUTE_LIFETIME_CAP),
                "retained issuance plus future allowance must equal the lifetime ceiling"
            );
        }
    }

    impl<T: Config> Pallet<T> {
        /// Mint native ETKN while consuming the single persistent lifetime allowance.
        ///
        /// Allowance, native balance, total issuance and the success event share one storage
        /// transaction. Any validation or downstream failure rolls all of them back.
        pub fn controlled_mint_into(
            beneficiary: &T::AccountId,
            amount: Balance,
        ) -> Result<Balance, DispatchError> {
            with_transaction(|| {
                let result = Self::do_controlled_mint_into(beneficiary, amount);
                if result.is_ok() {
                    TransactionOutcome::Commit(result)
                } else {
                    TransactionOutcome::Rollback(result)
                }
            })
        }

        fn do_controlled_mint_into(
            beneficiary: &T::AccountId,
            amount: Balance,
        ) -> Result<Balance, DispatchError> {
            ensure!(amount != 0, Error::<T>::ZeroIssuance);

            let remaining =
                RemainingAllowance::<T>::get().ok_or(Error::<T>::AllowanceNotInitialized)?;
            ensure!(
                remaining <= MAXIMUM_POST_CORRECTION_NEW_ISSUANCE,
                Error::<T>::InvalidStoredAllowance
            );
            let next_remaining = remaining
                .checked_sub(amount)
                .ok_or(Error::<T>::AllowanceExceeded)?;

            let issuance_before = T::Currency::total_issuance();
            let expected_issuance = issuance_before
                .checked_add(amount)
                .ok_or(Error::<T>::ArithmeticOverflow)?;
            ensure!(
                issuance_before <= ABSOLUTE_LIFETIME_CAP,
                Error::<T>::IssuanceAboveLifetimeCeiling
            );
            ensure!(
                expected_issuance <= ABSOLUTE_LIFETIME_CAP,
                Error::<T>::LifetimeCeilingExceeded
            );

            RemainingAllowance::<T>::put(next_remaining);
            let minted = T::Currency::mint_into(beneficiary, amount)
                .map_err(|_| Error::<T>::DownstreamMintFailed)?;
            ensure!(minted == amount, Error::<T>::UnexpectedIssuanceDelta);
            ensure!(
                T::Currency::total_issuance() == expected_issuance,
                Error::<T>::UnexpectedIssuanceDelta
            );

            Self::deposit_event(Event::Issued {
                beneficiary: beneficiary.clone(),
                amount,
                remaining_allowance: next_remaining,
            });
            Ok(minted)
        }
    }
}
