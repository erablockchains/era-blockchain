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
    use sp_core::U256;
    use sp_runtime::{DispatchError, RuntimeDebug};

    use crate::{
        upgrade13_policy::{
            ABSOLUTE_LIFETIME_CAP, MAXIMUM_POST_CORRECTION_NEW_ISSUANCE, TARGET_RETAINED_ISSUANCE,
        },
        Balance,
    };

    const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

    /// The one remaining non-custody destination authorized before the V13 runtime upgrade.
    ///
    /// Founding custody destinations are compiled deterministic pallet subaccounts and therefore
    /// cannot be supplied at deployment. Community onboarding remains a separate allocation whose
    /// owner/control model is outside this correction.
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
        pub community_onboarding_destination: AccountId,
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
        /// One complete V14 gross issuance was atomically split between protocol accounts.
        GrossIssued {
            staking_beneficiary: T::AccountId,
            staking_amount: Balance,
            treasury_beneficiary: T::AccountId,
            treasury_amount: Balance,
            gross_amount: Balance,
            remaining_allowance: Balance,
            carry_before: u8,
            carry_after: u8,
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
        /// Both issuance shares must have distinct protocol destinations.
        DestinationCollision,
        /// The supplied split carry is outside the fixed 0..9 domain.
        InvalidSplitCarry,
        /// The two shares and carry transition do not encode the approved persistent 90/10 split.
        InvalidSplitEvidence,
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

        /// Atomically mint one complete gross issuance to the V14 staking pot and treasury.
        ///
        /// The allowance decrement, both balance credits, exact total-issuance delta, and event
        /// share one transaction. Either destination may receive zero, but their checked sum must
        /// be positive and the destinations must differ.
        pub fn controlled_mint_split(
            staking_beneficiary: &T::AccountId,
            staking_amount: Balance,
            treasury_beneficiary: &T::AccountId,
            treasury_amount: Balance,
            carry_before: u8,
            carry_after: u8,
        ) -> Result<Balance, DispatchError> {
            with_transaction(|| {
                let result = Self::do_controlled_mint_split(
                    staking_beneficiary,
                    staking_amount,
                    treasury_beneficiary,
                    treasury_amount,
                    carry_before,
                    carry_after,
                );
                if result.is_ok() {
                    TransactionOutcome::Commit(result)
                } else {
                    TransactionOutcome::Rollback(result)
                }
            })
        }

        fn do_controlled_mint_split(
            staking_beneficiary: &T::AccountId,
            staking_amount: Balance,
            treasury_beneficiary: &T::AccountId,
            treasury_amount: Balance,
            carry_before: u8,
            carry_after: u8,
        ) -> Result<Balance, DispatchError> {
            ensure!(
                staking_beneficiary != treasury_beneficiary,
                Error::<T>::DestinationCollision
            );
            ensure!(
                carry_before < 10 && carry_after < 10,
                Error::<T>::InvalidSplitCarry
            );
            let gross = staking_amount
                .checked_add(treasury_amount)
                .ok_or(Error::<T>::ArithmeticOverflow)?;
            ensure!(gross != 0, Error::<T>::ZeroIssuance);
            let split_numerator = U256::from(gross)
                .checked_mul(U256::from(9u8))
                .and_then(|value| value.checked_add(U256::from(carry_before)))
                .ok_or(Error::<T>::ArithmeticOverflow)?;
            let expected_staking = (split_numerator / U256::from(10u8)).low_u128();
            let expected_carry = (split_numerator % U256::from(10u8)).low_u32() as u8;
            let expected_treasury = gross
                .checked_sub(expected_staking)
                .ok_or(Error::<T>::ArithmeticOverflow)?;
            ensure!(
                staking_amount == expected_staking
                    && treasury_amount == expected_treasury
                    && carry_after == expected_carry,
                Error::<T>::InvalidSplitEvidence
            );

            let remaining =
                RemainingAllowance::<T>::get().ok_or(Error::<T>::AllowanceNotInitialized)?;
            ensure!(
                remaining <= MAXIMUM_POST_CORRECTION_NEW_ISSUANCE,
                Error::<T>::InvalidStoredAllowance
            );
            let next_remaining = remaining
                .checked_sub(gross)
                .ok_or(Error::<T>::AllowanceExceeded)?;
            let issuance_before = T::Currency::total_issuance();
            let expected_issuance = issuance_before
                .checked_add(gross)
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
            if staking_amount != 0 {
                let minted = T::Currency::mint_into(staking_beneficiary, staking_amount)
                    .map_err(|_| Error::<T>::DownstreamMintFailed)?;
                ensure!(
                    minted == staking_amount,
                    Error::<T>::UnexpectedIssuanceDelta
                );
            }
            if treasury_amount != 0 {
                let minted = T::Currency::mint_into(treasury_beneficiary, treasury_amount)
                    .map_err(|_| Error::<T>::DownstreamMintFailed)?;
                ensure!(
                    minted == treasury_amount,
                    Error::<T>::UnexpectedIssuanceDelta
                );
            }
            ensure!(
                T::Currency::total_issuance() == expected_issuance,
                Error::<T>::UnexpectedIssuanceDelta
            );
            Self::deposit_event(Event::GrossIssued {
                staking_beneficiary: staking_beneficiary.clone(),
                staking_amount,
                treasury_beneficiary: treasury_beneficiary.clone(),
                treasury_amount,
                gross_amount: gross,
                remaining_allowance: next_remaining,
                carry_before,
                carry_after,
            });
            Ok(gross)
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
