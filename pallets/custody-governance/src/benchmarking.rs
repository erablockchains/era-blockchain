//! Benchmarks for strict three-of-three founding custody.

use super::*;
use crate::pallet::{Config, NextRequestId, Pallet, PendingWithdrawal};
use frame_benchmarking::v2::*;
use frame_support::traits::{
    fungible::{Inspect, Mutate},
    Get,
};
use frame_system::RawOrigin;
use sp_runtime::traits::{SaturatedConversion, Saturating};

#[benchmarks]
mod benchmarks {
    use super::*;

    #[benchmark]
    fn approve_withdrawal() {
        let category = CustodyCategory::Presale;
        let signers = T::Signers::get();
        let signer = signers[2].clone();
        let destination: T::AccountId = account("custody-destination", 0, 0);
        let source = Pallet::<T>::custody_account(category);
        let amount = T::Currency::minimum_balance().saturating_mul(100u32.saturated_into());

        T::Currency::mint_into(&source, amount).expect("benchmark custody funding");
        PendingWithdrawal::<T>::insert(
            category,
            WithdrawalRequest::<T> {
                request_id: 0,
                destination: destination.clone(),
                amount,
                approvals: 0b011,
            },
        );

        #[extrinsic_call]
        approve_withdrawal(
            RawOrigin::Signed(signer),
            category,
            0,
            destination.clone(),
            amount,
        );

        assert!(PendingWithdrawal::<T>::get(category).is_none());
        assert_eq!(NextRequestId::<T>::get(category), 1);
        assert_eq!(T::Currency::balance(&destination), amount);
    }
}
