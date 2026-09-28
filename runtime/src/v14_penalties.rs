//! Policy-parametric final V14 penalties, called only after pinned consensus proof validation.
//! Policy is absent by default. No downtime, connectivity or synchronization input exists.
use crate::{AccountId, Balances, Runtime, Session, Staking};
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{
    traits::{fungible::Balanced, Imbalance, OnUnbalanced},
    weights::Weight,
};
use scale_info::TypeInfo;
use sp_runtime::Perbill;

#[derive(
    Encode,
    Decode,
    codec::DecodeWithMemTracking,
    MaxEncodedLen,
    TypeInfo,
    Clone,
    Copy,
    PartialEq,
    Eq,
    sp_runtime::RuntimeDebug,
)]
pub struct PolicyV1 {
    pub maximum_fraction_ppb: u32,
    pub activation_session: u32,
}

impl pallet::Config for Runtime {}

/// Static worst-case charge: no storage reads while determining dispatch weight.
/// Includes the absent-policy read and every measured Apply path, plus 25% headroom.
pub(crate) fn measured_apply_weight() -> Weight {
    use crate::v14_penalty_weights::WeightInfo as M;
    let maximum = crate::component_wise_weight_max([
        M::<Runtime>::apply_new_max(), M::<Runtime>::apply_combined_cap(),
        M::<Runtime>::apply_queue_rollback(), M::<Runtime>::apply_duplicate_identity(),
        M::<Runtime>::apply_oldest_span(), M::<Runtime>::apply_prune_spans(),
    ]);
    maximum.saturating_add(Weight::from_parts(maximum.ref_time()/4,maximum.proof_size()/4))
}
pub(crate) fn measured_prepare_weight() -> Weight {
    let w=crate::v14_penalty_weights::WeightInfo::<Runtime>::prepare_policy();
    w.saturating_add(Weight::from_parts(w.ref_time()/4,w.proof_size()/4))
}

pub(crate) fn after_verified_report(
    offender: AccountId,
    session: u32,
    kind: u8,
    set_count: u32,
    new_record: bool,
) -> Result<(), sp_staking::offence::OffenceError> {
    use sp_staking::offence::{OffenceDetails, OffenceError};
    let Some(policy) = pallet::Policy::<Runtime>::get() else {
        return Ok(());
    };
    if !new_record || session < policy.activation_session {
        return Ok(());
    }
    let bad = || OffenceError::Other(40);
    if policy.maximum_fraction_ppb == 0
        || policy.maximum_fraction_ppb > 1_000_000_000
        || kind > 1
        || set_count == 0
        || set_count > 16
        || session > Session::current_index()
    {
        return Err(bad());
    }
    let active = pallet_staking::ActiveEra::<Runtime>::get()
        .ok_or_else(bad)?
        .index;
    let start = pallet_staking::ErasStartSessionIndex::<Runtime>::get(active).ok_or_else(bad)?;
    let era = if session >= start {
        active
    } else {
        pallet_staking::BondedEras::<Runtime>::get()
            .iter()
            .rev()
            .find(|(_, s)| *s <= session)
            .map(|(e, _)| *e)
            .ok_or_else(bad)?
    };
    let enactment = era
        .checked_add(crate::SlashDeferDuration::get())
        .and_then(|e| e.checked_add(1))
        .ok_or_else(bad)?;
    // The SDK takes only the queue for the exact active era. Never enqueue into a past/current slot.
    if enactment <= active
        || pallet_staking::Invulnerables::<Runtime>::get().contains(&offender)
        || pallet_staking::SlashRewardFraction::<Runtime>::get() != Perbill::zero()
    {
        return Err(bad());
    }
    // Bound reads before decoding the paged exposure and the deferred queue. This
    // fresh-chain adapter deliberately rejects legacy/unpaged exposure; migration
    // support would need separate bounded validation rather than an unbounded read.
    let overview =
        pallet_staking::ErasStakersOverview::<Runtime>::get(era, &offender).ok_or_else(bad)?;
    if overview.nominator_count > crate::MaxElectionBackersPerWinner::get()
        || overview.page_count > crate::MaxElectionBackersPerWinner::get()
    {
        return Err(bad());
    }
    if pallet_staking::UnappliedSlashes::<Runtime>::decode_len(enactment).unwrap_or(0)
        > crate::MaxValidatorSet::get() as usize
    {
        return Err(bad());
    }
    let exposure = Staking::eras_stakers(era, &offender);
    if exposure.total == 0 || exposure.others.len() > 64 {
        return Err(bad());
    }
    let sum = exposure
        .others
        .iter()
        .try_fold(exposure.own, |sum, n| sum.checked_add(n.value))
        .ok_or_else(bad)?;
    if sum != exposure.total {
        return Err(bad());
    }
    let destination = crate::AiPenaltyDestination::get();
    if Balances::free_balance(&destination) < crate::EXISTENTIAL_DEPOSIT {
        return Err(bad());
    }
    let fraction = Perbill::from_rational(3u32, set_count)
        .square()
        .min(Perbill::from_parts(policy.maximum_fraction_ppb));
    let prior = pallet_staking::ValidatorSlashInEra::<Runtime>::get(era, &offender)
        .map(|(p, _)| p)
        .unwrap_or_default();
    let before = pallet_staking::UnappliedSlashes::<Runtime>::get(enactment).len();
    let validators_before = Session::validators();
    let _weight = Staking::on_offence(
        core::iter::once(OffenceDetails {
            offender: offender.clone(),
            reporters: alloc::vec![],
        }),
        &[fraction],
        session,
    );
    // Shared staking maxima enforce one combined BABE/GRANDPA per-validator/era cap.
    let after = pallet_staking::UnappliedSlashes::<Runtime>::get(enactment).len();
    if after > crate::MaxValidatorSet::get() as usize
        || after < before
        || Session::validators() != validators_before
    {
        return Err(bad());
    }
    pallet::Pallet::<Runtime>::note_submission(
        offender,
        kind,
        era,
        enactment,
        fraction.deconstruct(),
        prior.deconstruct(),
        (after - before) as u32,
    );
    Ok(())
}

/// Both automatic and existing privileged staking slashes use the approved keyless destination.
pub struct TreasurySlash;
impl OnUnbalanced<pallet_staking::NegativeImbalanceOf<Runtime>> for TreasurySlash {
    fn on_nonzero_unbalanced(credit: pallet_staking::NegativeImbalanceOf<Runtime>) {
        let amount = credit.peek();
        let destination = crate::AiPenaltyDestination::get();
        // Activation requires a funded keyless treasury; V14 exposes no grants/spend path.
        // An invariant violation traps the block rather than accepting a burn. This must be
        // included in upgrade review; Root state corruption is not silently recovered by minting.
        <Balances as Balanced<AccountId>>::resolve(&destination, credit)
            .expect("funded accumulation-only V14 slash treasury must accept the complete credit");
        pallet::Pallet::<Runtime>::note_transfer(destination, amount);
    }
}

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_support::pallet_prelude::*;
    use frame_system::pallet_prelude::*;
    #[pallet::config]
    pub trait Config:
        frame_system::Config<AccountId = AccountId, RuntimeEvent: From<Event<Self>>>
    {
    }
    #[pallet::pallet]
    pub struct Pallet<T>(_);
    #[pallet::storage]
    pub type Policy<T> = StorageValue<_, PolicyV1, OptionQuery>;
    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        PolicyPrepared {
            maximum_fraction_ppb: u32,
            activation_session: u32,
            at: BlockNumberFor<T>,
        },
        PenaltySubmitted {
            offender: AccountId,
            kind: u8,
            offence_era: u32,
            enactment_era: u32,
            requested_ppb: u32,
            prior_maximum_ppb: u32,
            new_queue_entries: u32,
        },
        SlashTransferred {
            destination: AccountId,
            amount: u128,
        },
    }
    #[pallet::error]
    pub enum Error<T> {
        AlreadyConfigured,
        InvalidPolicy,
        TreasuryNotFunded,
    }
    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::call_index(0)]
        #[pallet::weight(super::measured_prepare_weight())]
        pub fn prepare_policy(
            origin: OriginFor<T>,
            maximum_fraction_ppb: u32,
            activation_session: u32,
        ) -> DispatchResult {
            ensure_root(origin)?;
            ensure!(!Policy::<T>::exists(), Error::<T>::AlreadyConfigured);
            ensure!(
                maximum_fraction_ppb > 0
                    && maximum_fraction_ppb <= 1_000_000_000
                    && activation_session > Session::current_index(),
                Error::<T>::InvalidPolicy
            );
            ensure!(
                Balances::free_balance(crate::AiPenaltyDestination::get())
                    >= crate::EXISTENTIAL_DEPOSIT,
                Error::<T>::TreasuryNotFunded
            );
            Policy::<T>::put(PolicyV1 {
                maximum_fraction_ppb,
                activation_session,
            });
            Self::deposit_event(Event::PolicyPrepared {
                maximum_fraction_ppb,
                activation_session,
                at: frame_system::Pallet::<T>::block_number(),
            });
            Ok(())
        }
    }
    impl<T: Config> Pallet<T> {
        pub fn note_submission(
            offender: AccountId,
            kind: u8,
            offence_era: u32,
            enactment_era: u32,
            requested_ppb: u32,
            prior_maximum_ppb: u32,
            new_queue_entries: u32,
        ) {
            Self::deposit_event(Event::PenaltySubmitted {
                offender,
                kind,
                offence_era,
                enactment_era,
                requested_ppb,
                prior_maximum_ppb,
                new_queue_entries,
            });
        }
        pub fn note_transfer(destination: AccountId, amount: u128) {
            Self::deposit_event(Event::SlashTransferred {
                destination,
                amount,
            });
        }
    }
}
pub use pallet::*;
