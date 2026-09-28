use crate::{weights::WeightInfo, CustodyCategory, FOUNDER_COUNT};
use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use frame_support::{
    pallet_prelude::*,
    traits::{
        fungible::{Inspect, Mutate},
        tokens::{Fortitude, Preservation},
        Get,
    },
    transactional, PalletId,
};
use frame_system::pallet_prelude::*;
use scale_info::TypeInfo;
use sp_runtime::{
    traits::{AccountIdConversion, Zero},
    RuntimeDebug,
};

pub const CUSTODY_DESTINATION_VERSION: u8 = 2;
pub const CUSTODY_SUBDOMAIN: [u8; 8] = *b"founding";
pub const COMPLETE_APPROVAL_MASK: u8 = 0b111;

pub type BalanceOf<T> =
    <<T as pallet::Config>::Currency as Inspect<<T as frame_system::Config>::AccountId>>::Balance;

#[derive(
    Encode,
    Decode,
    DecodeWithMemTracking,
    Clone,
    Eq,
    PartialEq,
    RuntimeDebug,
    TypeInfo,
    MaxEncodedLen,
)]
#[scale_info(skip_type_params(T))]
pub struct WithdrawalRequest<T: pallet::Config> {
    pub request_id: u64,
    pub destination: T::AccountId,
    pub amount: BalanceOf<T>,
    pub approvals: u8,
}

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use alloc::vec::Vec;

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::config]
    pub trait Config: frame_system::Config<RuntimeEvent: From<Event<Self>>> {
        type Currency: Inspect<Self::AccountId> + Mutate<Self::AccountId>;

        #[pallet::constant]
        type CustodyPalletId: Get<PalletId>;

        /// Exact canonical signers, supplied by genesis storage on a fresh chain.
        /// State-dependent values must not be advertised as metadata constants.
        type Signers: Get<[Self::AccountId; FOUNDER_COUNT]>;

        type WeightInfo: WeightInfo;
    }

    #[pallet::storage]
    #[pallet::getter(fn pending_withdrawal)]
    pub type PendingWithdrawal<T: Config> =
        StorageMap<_, Twox64Concat, CustodyCategory, WithdrawalRequest<T>, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn next_request_id)]
    pub type NextRequestId<T: Config> =
        StorageMap<_, Twox64Concat, CustodyCategory, u64, ValueQuery>;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        ApprovalRecorded {
            category: CustodyCategory,
            request_id: u64,
            signer: T::AccountId,
            approval_count: u8,
        },
        WithdrawalExecuted {
            category: CustodyCategory,
            request_id: u64,
            destination: T::AccountId,
            amount: BalanceOf<T>,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        InvalidSignerConfiguration,
        UnauthorizedSigner,
        RequestIdMismatch,
        ZeroAmount,
        InvalidDestination,
        ProposalMismatch,
        DuplicateApproval,
        MalformedPendingRequest,
        InsufficientCustodyBalance,
        RequestIdOverflow,
        TransferFailed,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// Approve an exact category withdrawal.
        ///
        /// The first and second distinct approvals only record intent. The third approval from the
        /// remaining configured signer atomically transfers from the category's keyless subaccount.
        #[pallet::call_index(0)]
        // Fresh-chain signers are storage-backed. Retain the accepted measured envelope and
        // explicitly charge the added read and a conservative 4 KiB proof allowance.
        #[pallet::weight(T::WeightInfo::approve_withdrawal()
            .saturating_add(T::DbWeight::get().reads(1))
            .saturating_add(Weight::from_parts(0, 4096)))]
        #[transactional]
        pub fn approve_withdrawal(
            origin: OriginFor<T>,
            category: CustodyCategory,
            request_id: u64,
            destination: T::AccountId,
            #[pallet::compact] amount: BalanceOf<T>,
        ) -> DispatchResult {
            let signer = ensure_signed(origin)?;
            let signers = Self::validated_signers()?;
            let signer_index = signers
                .iter()
                .position(|candidate| candidate == &signer)
                .ok_or(Error::<T>::UnauthorizedSigner)?;
            ensure!(
                request_id == NextRequestId::<T>::get(category),
                Error::<T>::RequestIdMismatch
            );
            ensure!(!amount.is_zero(), Error::<T>::ZeroAmount);
            Self::ensure_valid_destination(&destination, &signers)?;

            let source = Self::custody_account(category);
            ensure!(
                T::Currency::reducible_balance(
                    &source,
                    Preservation::Expendable,
                    Fortitude::Polite,
                ) >= amount,
                Error::<T>::InsufficientCustodyBalance
            );

            let approval_bit = 1u8 << signer_index;
            let approvals = PendingWithdrawal::<T>::try_mutate(
                category,
                |pending| -> Result<u8, DispatchError> {
                    let request = pending.get_or_insert_with(|| WithdrawalRequest::<T> {
                        request_id,
                        destination: destination.clone(),
                        amount,
                        approvals: 0,
                    });
                    ensure!(
                        request.approvals & !COMPLETE_APPROVAL_MASK == 0,
                        Error::<T>::MalformedPendingRequest
                    );
                    ensure!(
                        request.request_id == request_id
                            && request.destination == destination
                            && request.amount == amount,
                        Error::<T>::ProposalMismatch
                    );
                    ensure!(
                        request.approvals & approval_bit == 0,
                        Error::<T>::DuplicateApproval
                    );
                    request.approvals |= approval_bit;
                    Ok(request.approvals)
                },
            )?;

            Self::deposit_event(Event::ApprovalRecorded {
                category,
                request_id,
                signer,
                approval_count: approvals.count_ones() as u8,
            });

            if approvals == COMPLETE_APPROVAL_MASK {
                T::Currency::transfer(&source, &destination, amount, Preservation::Expendable)
                    .map_err(|_| Error::<T>::TransferFailed)?;
                PendingWithdrawal::<T>::remove(category);
                NextRequestId::<T>::insert(
                    category,
                    request_id
                        .checked_add(1)
                        .ok_or(Error::<T>::RequestIdOverflow)?,
                );
                Self::deposit_event(Event::WithdrawalExecuted {
                    category,
                    request_id,
                    destination,
                    amount,
                });
            }

            Ok(())
        }
    }

    impl<T: Config> Pallet<T> {
        pub fn custody_account(category: CustodyCategory) -> T::AccountId {
            T::CustodyPalletId::get().into_sub_account_truncating((
                CUSTODY_DESTINATION_VERSION,
                CUSTODY_SUBDOMAIN,
                category,
            ))
        }

        pub fn derivation_preimage(category: CustodyCategory) -> Vec<u8> {
            (
                *b"modl",
                T::CustodyPalletId::get(),
                (CUSTODY_DESTINATION_VERSION, CUSTODY_SUBDOMAIN, category),
            )
                .encode()
        }

        pub fn custody_accounts() -> [T::AccountId; 3] {
            [
                Self::custody_account(CustodyCategory::Presale),
                Self::custody_account(CustodyCategory::Ecosystem),
                Self::custody_account(CustodyCategory::Liquidity),
            ]
        }

        fn validated_signers() -> Result<[T::AccountId; FOUNDER_COUNT], DispatchError> {
            let signers = T::Signers::get();
            ensure!(
                signers[0] < signers[1] && signers[1] < signers[2],
                Error::<T>::InvalidSignerConfiguration
            );
            let accounts = Self::custody_accounts();
            ensure!(
                accounts[0] != accounts[1]
                    && accounts[0] != accounts[2]
                    && accounts[1] != accounts[2],
                Error::<T>::InvalidSignerConfiguration
            );
            ensure!(
                accounts
                    .iter()
                    .all(|account| signers.iter().all(|signer| account != signer)),
                Error::<T>::InvalidSignerConfiguration
            );
            Ok(signers)
        }

        fn ensure_valid_destination(
            destination: &T::AccountId,
            signers: &[T::AccountId; FOUNDER_COUNT],
        ) -> DispatchResult {
            ensure!(
                !Self::custody_accounts()
                    .iter()
                    .any(|account| account == destination)
                    && !signers.iter().any(|signer| signer == destination),
                Error::<T>::InvalidDestination
            );
            Ok(())
        }
    }
}

pub use pallet::*;
