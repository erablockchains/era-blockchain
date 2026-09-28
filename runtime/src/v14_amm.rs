//! Required final V14 AMM runtime integration. Private development candidate only.
//! The retained R2 numerical proposals are candidate parameters, not production approvals.
use crate::v14_assets::{AdmissionLookup, ContractFungibles};
use crate::{AccountId, Assets, Balances, Runtime, System as EraSystem, EXISTENTIAL_DEPOSIT};
use era_v14_amm::{self as kernel, v1 as contract, AssetKey, Backend};
use era_v14_application_primitives::{
    amm,
    assets::{self, v1, FungibleAsset, FungibleInspect, FungibleTransfer},
};
use frame_support::{
    traits::{
        fungible, fungibles,
        tokens::{Fortitude, Preservation},
        Get, ReservableCurrency,
    },
    weights::Weight,
};
use sp_runtime::traits::AccountIdConversion;

pub struct Admissions;
impl AdmissionLookup for Admissions {
    const CONFIGURED: bool = true;
    fn asset(id: u32) -> v1::ApiResult<v1::AssetAdmission> {
        pallet::AdmittedAssets::<Runtime>::get(id)
            .map(|minimum_balance| v1::AssetAdmission {
                id,
                minimum_balance,
            })
            .ok_or(v1::AssetApiErrorV1::UnsupportedAsset)
    }
    fn collection(_: u32) -> v1::ApiResult<()> {
        Err(v1::AssetApiErrorV1::UnsupportedAsset)
    }
}
#[derive(Default)]
pub struct Actual;
impl FungibleInspect<AccountId> for Actual {
    type AssetId = u32;
    fn balance(&self, a: FungibleAsset<u32>, who: &AccountId) -> u128 {
        ContractFungibles::<Admissions>::new().balance(a, who)
    }
    fn total_issuance(&self, a: FungibleAsset<u32>) -> Result<u128, assets::Error> {
        ContractFungibles::<Admissions>::new().total_issuance(a)
    }
}
impl FungibleTransfer<AccountId> for Actual {
    fn transfer(
        &mut self,
        a: FungibleAsset<u32>,
        from: &AccountId,
        to: &AccountId,
        amount: u128,
    ) -> Result<(), assets::Error> {
        ContractFungibles::<Admissions>::new().transfer(a, from, to, amount)?;
        Ok(())
    }
}
impl Backend<AccountId> for Actual {
    fn minimum(&self, a: FungibleAsset<u32>) -> Result<u128, assets::Error> {
        match a {
            FungibleAsset::NativeEtkn => Ok(EXISTENTIAL_DEPOSIT),
            FungibleAsset::Registered(id) => {
                if Assets::maybe_total_supply(id).is_none() {
                    return Err(assets::Error::UnknownAsset);
                }
                let minimum = <Assets as fungibles::Inspect<AccountId>>::minimum_balance(id);
                if minimum == 0 {
                    return Err(assets::Error::AccountingInvariant);
                }
                Ok(minimum)
            }
        }
    }
    fn reserved_native(&self, who: &AccountId) -> u128 {
        Balances::reserved_balance(who)
    }
    fn reserve_native(&mut self, who: &AccountId, amount: u128) -> Result<(), assets::Error> {
        Balances::reserve(who, amount).map_err(|_| assets::Error::BackendRejected)
    }
    fn identity_conflict(&self, who: &AccountId) -> bool {
        // These are bounded protocol/authority lookups, not a scan or a balance/liveness ban.
        let protocol = [
            crate::AiPredictionsPalletId::get(),
            crate::RewardReservePalletId::get(),
            crate::EcosystemTreasuryPalletId::get(),
            crate::FeeCollectionPalletId::get(),
            crate::SecurityBudgetPalletId::get(),
        ];
        protocol.into_iter().any(|id| {
            let account: AccountId = id.into_account_truncating();
            account == *who
        }) || EraSystem::account(who).nonce != 0
            || pallet_sudo::Key::<Runtime>::get().as_ref() == Some(who)
            || pallet_staking::Bonded::<Runtime>::contains_key(who)
            || pallet_staking::Ledger::<Runtime>::contains_key(who)
            || pallet_session::NextKeys::<Runtime>::contains_key(who)
            || !pallet_proxy::Proxies::<Runtime>::get(who).0.is_empty()
    }
    fn providers(&self, who: &AccountId) -> u32 {
        EraSystem::providers(who)
    }
    fn reducible(&self, asset: FungibleAsset<u32>, who: &AccountId) -> u128 {
        match asset {
            FungibleAsset::NativeEtkn => {
                <Balances as fungible::Inspect<AccountId>>::reducible_balance(
                    who,
                    Preservation::Preserve,
                    Fortitude::Polite,
                )
            }
            FungibleAsset::Registered(id) => {
                <Assets as fungibles::Inspect<AccountId>>::reducible_balance(
                    id,
                    who,
                    Preservation::Preserve,
                    Fortitude::Polite,
                )
            }
        }
    }
    fn establish(&mut self, who: &AccountId) -> Result<(), assets::Error> {
        let before = EraSystem::account(who);
        let next = before
            .providers
            .checked_add(1)
            .ok_or(assets::Error::ArithmeticOverflow)?;
        EraSystem::inc_providers(who);
        let after = EraSystem::account(who);
        let mut expected = before;
        expected.providers = next;
        if after != expected {
            return Err(assets::Error::AccountingInvariant);
        }
        Ok(())
    }
}
pub struct PairRegistry;
impl kernel::Admission for PairRegistry {
    fn ensure(pair: amm::PoolId<u32>) -> kernel::Result<()> {
        if !pallet::Enabled::<Runtime>::get() {
            return Err(amm::Error::InvalidConfiguration.into());
        }
        let a = contract::Asset::from_fungible(pair.asset_0);
        let b = contract::Asset::from_fungible(pair.asset_1);
        if !pallet::ApprovedPairs::<Runtime>::contains_key((a, b)) {
            return kernel::EmptyPairs::ensure(pair);
        }
        Ok(())
    }
}
pub struct Notices;
impl kernel::EventSink<AccountId> for Notices {
    fn emit(event: amm::Event<AccountId, u32>) -> kernel::Result<()> {
        let event = contract::EventV1::try_from(event).map_err(|_| kernel::Fault::CorruptState)?;
        pallet::Pallet::<Runtime>::record_notice(event);
        Ok(())
    }
}
pub struct Clock;
impl Get<u64> for Clock {
    fn get() -> u64 {
        EraSystem::block_number() as u64
    }
}
impl kernel::Config for Runtime {
    type Asset = contract::Asset;
    type Record = contract::PoolRecord;
    type PoolHasher = frame_support::Blake2_128Concat;
    type AccountHasher = frame_support::Blake2_128Concat;
    type Assets = Actual;
    type Admission = PairRegistry;
    type Creator = contract::SignedCreator;
    type Custody = contract::CheckedCustody;
    type Events = Notices;
    type Math = kernel::WideArithmetic;
    type Clock = Clock;
    type Fee = contract::Fee;
    type FeeLimit = contract::FeeLimit;
    type LockedLiquidity = contract::LockedLiquidity;
    type MinimumPosition = contract::MinimumPosition;
    type MinimumTrade = contract::MinimumTrade;
    type CreationDeposit = contract::CreationDeposit;
    type Horizon = contract::Horizon;
    type MaxPools = contract::MaxPools;
    type MaxPositions = contract::MaxPositions;
    type MaxProviders = contract::MaxProviders;
    type MaxPage = contract::MaxPage;
}
impl pallet::Config for Runtime {}

// Component-wise maximum of actual runtime Wasm measurements, plus 25% execution/proof
// headroom. Max-provider/account fixtures are reconciled before measurement. Both pair classes,
// orientations, recipient creation and initial/subsequent/new-provider/removal branches measured.
// This is engineering admission evidence, not owner approval for production pairs/funding.
pub fn measured_weight(call: u8) -> Weight {
    use crate::v14_amm_weights::WeightInfo as M;
    let paths: alloc::vec::Vec<Weight> = match call {
        0 => alloc::vec![M::<Runtime>::create_pool(), M::<Runtime>::create_registered()],
        1 => alloc::vec![M::<Runtime>::add_initial(), M::<Runtime>::add_subsequent(), M::<Runtime>::add_registered_initial(), M::<Runtime>::add_native_bounded(), M::<Runtime>::add_registered_bounded(), M::<Runtime>::add_native_new_bounded(), M::<Runtime>::add_registered_new_bounded()],
        2 => alloc::vec![M::<Runtime>::remove_liquidity(), M::<Runtime>::remove_native_bounded(), M::<Runtime>::remove_registered_bounded()],
        3 => alloc::vec![M::<Runtime>::swap_exact_input(), M::<Runtime>::swap_native_input_0_0(), M::<Runtime>::swap_native_input_0_1(), M::<Runtime>::swap_native_input_1_0(), M::<Runtime>::swap_native_input_1_1(), M::<Runtime>::swap_registered_input_0_0(), M::<Runtime>::swap_registered_input_0_1(), M::<Runtime>::swap_registered_input_1_0(), M::<Runtime>::swap_registered_input_1_1()],
        4 => alloc::vec![M::<Runtime>::swap_exact_output(), M::<Runtime>::swap_native_output_0_0(), M::<Runtime>::swap_native_output_0_1(), M::<Runtime>::swap_native_output_1_0(), M::<Runtime>::swap_native_output_1_1(), M::<Runtime>::swap_registered_output_0_0(), M::<Runtime>::swap_registered_output_0_1(), M::<Runtime>::swap_registered_output_1_0(), M::<Runtime>::swap_registered_output_1_1()],
        10 => alloc::vec![M::<Runtime>::approve_registered()],
        11 => alloc::vec![M::<Runtime>::activate()],
        _ => return Weight::MAX,
    };
    let maximum=paths.into_iter().fold(Weight::zero(),|a,b|Weight::from_parts(a.ref_time().max(b.ref_time()),a.proof_size().max(b.proof_size())));
    maximum.saturating_add(Weight::from_parts(maximum.ref_time()/4, maximum.proof_size()/4))
}

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_support::{pallet_prelude::*, traits::GetDefault};
    use frame_system::pallet_prelude::*;
    #[pallet::config]
    pub trait Config:
        frame_system::Config<AccountId = AccountId, RuntimeEvent: From<Event<Self>>>
        + kernel::v1::ContractConfig
    {
    }
    #[pallet::pallet]
    pub struct Pallet<T>(_);
    #[pallet::storage]
    pub type Enabled<T> = StorageValue<_, bool, ValueQuery>;
    #[pallet::storage]
    pub type ApprovedPairs<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        (contract::Asset, contract::Asset),
        (),
        OptionQuery,
        GetDefault,
        kernel::MapBound<contract::MaxPools>,
    >;
    #[pallet::storage]
    pub type PairCount<T> = StorageValue<_, u32, ValueQuery>;
    #[pallet::storage]
    pub type AdmittedAssets<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        u32,
        u128,
        OptionQuery,
        GetDefault,
        kernel::MapBound<frame_support::traits::ConstU32<2048>>,
    >;
    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        Notice {
            event: contract::EventV1,
        },
        PairApproved {
            asset_a: contract::Asset,
            asset_b: contract::Asset,
            at: BlockNumberFor<T>,
        },
        Activated {
            at: BlockNumberFor<T>,
        },
    }
    #[pallet::error]
    pub enum Error<T> {
        NotFound,
        InvalidLimit,
        InvalidCursor,
        UnsupportedAsset,
        Unconfigured,
        Arithmetic,
        BackendInvariant,
        IdenticalAssets,
        Unauthorized,
        PoolAlreadyExists,
        CustodyCollision,
        CustodyNotEmpty,
        CustodyMismatch,
        CustodyNotLive,
        DeadlineExpired,
        BoundExceeded,
        ZeroAmount,
        InsufficientInitialLiquidity,
        InsufficientLiquidity,
        InsufficientLp,
        SlippageExceeded,
        InvalidConfiguration,
        BelowMinimum,
        InsufficientBalance,
        Frozen,
        DepositFailure,
        BackendRejected,
        Reentrant,
        TransactionLimit,
        CorruptState,
        BadPair,
        Bound,
        AlreadyActive,
    }
    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::call_index(0)]
        #[pallet::weight(super::measured_weight(0))]
        pub fn create_pool(
            origin: OriginFor<T>,
            asset_a: contract::Asset,
            asset_b: contract::Asset,
            deadline: u32,
        ) -> DispatchResult {
            contract::execute_signed::<T>(
                origin,
                contract::CallV1::CreatePool {
                    asset_a,
                    asset_b,
                    deadline,
                },
            )
            .map_err(Self::kernel_error)
        }
        #[pallet::call_index(1)]
        #[pallet::weight(super::measured_weight(1))]
        pub fn add_liquidity(
            origin: OriginFor<T>,
            asset_a: contract::Asset,
            asset_b: contract::Asset,
            desired_a: u128,
            desired_b: u128,
            min_a: u128,
            min_b: u128,
            deadline: u32,
        ) -> DispatchResult {
            contract::execute_signed::<T>(
                origin,
                contract::CallV1::AddLiquidity {
                    asset_a,
                    asset_b,
                    desired_a,
                    desired_b,
                    min_a,
                    min_b,
                    deadline,
                },
            )
            .map_err(Self::kernel_error)
        }
        #[pallet::call_index(2)]
        #[pallet::weight(super::measured_weight(2))]
        pub fn remove_liquidity(
            origin: OriginFor<T>,
            asset_a: contract::Asset,
            asset_b: contract::Asset,
            lp: u128,
            min_a: u128,
            min_b: u128,
            recipient: AccountId,
            deadline: u32,
        ) -> DispatchResult {
            contract::execute_signed::<T>(
                origin,
                contract::CallV1::RemoveLiquidity {
                    asset_a,
                    asset_b,
                    lp,
                    min_a,
                    min_b,
                    recipient: recipient.into(),
                    deadline,
                },
            )
            .map_err(Self::kernel_error)
        }
        #[pallet::call_index(3)]
        #[pallet::weight(super::measured_weight(3))]
        pub fn swap_exact_input(
            origin: OriginFor<T>,
            asset_in: contract::Asset,
            asset_out: contract::Asset,
            amount_in: u128,
            min_out: u128,
            recipient: AccountId,
            deadline: u32,
        ) -> DispatchResult {
            contract::execute_signed::<T>(
                origin,
                contract::CallV1::SwapExactInput {
                    asset_in,
                    asset_out,
                    amount_in,
                    min_out,
                    recipient: recipient.into(),
                    deadline,
                },
            )
            .map_err(Self::kernel_error)
        }
        #[pallet::call_index(4)]
        #[pallet::weight(super::measured_weight(4))]
        pub fn swap_exact_output(
            origin: OriginFor<T>,
            asset_in: contract::Asset,
            asset_out: contract::Asset,
            amount_out: u128,
            max_in: u128,
            recipient: AccountId,
            deadline: u32,
        ) -> DispatchResult {
            contract::execute_signed::<T>(
                origin,
                contract::CallV1::SwapExactOutput {
                    asset_in,
                    asset_out,
                    amount_out,
                    max_in,
                    recipient: recipient.into(),
                    deadline,
                },
            )
            .map_err(Self::kernel_error)
        }
        #[pallet::call_index(10)]
        #[pallet::weight(super::measured_weight(10))]
        #[frame_support::transactional]
        pub fn approve_pair(
            origin: OriginFor<T>,
            asset_a: contract::Asset,
            asset_b: contract::Asset,
        ) -> DispatchResult {
            ensure_root(origin)?;
            let pair = contract::PoolId::new(asset_a, asset_b).map_err(|_| Error::<T>::BadPair)?;
            ensure!(
                !ApprovedPairs::<T>::contains_key(pair.pair()),
                Error::<T>::BadPair
            );
            let count = PairCount::<T>::get();
            ensure!(
                count < <contract::MaxPools as Get<u32>>::get(),
                Error::<T>::Bound
            );
            for asset in [pair.asset_0, pair.asset_1] {
                if let contract::Asset::Registered(id) = asset {
                    let minimum = T::Assets::default()
                        .minimum(FungibleAsset::Registered(id))
                        .map_err(|_| Error::<T>::UnsupportedAsset)?;
                    ensure!(minimum > 0, Error::<T>::UnsupportedAsset);
                    AdmittedAssets::<T>::insert(id, minimum);
                }
            }
            ApprovedPairs::<T>::insert(pair.pair(), ());
            PairCount::<T>::put(count + 1);
            Self::deposit_event(Event::PairApproved {
                asset_a: pair.asset_0,
                asset_b: pair.asset_1,
                at: frame_system::Pallet::<T>::block_number(),
            });
            Ok(())
        }
        #[pallet::call_index(11)]
        #[pallet::weight(super::measured_weight(11))]
        pub fn activate(origin: OriginFor<T>) -> DispatchResult {
            ensure_root(origin)?;
            ensure!(!Enabled::<T>::get(), Error::<T>::AlreadyActive);
            ensure!(PairCount::<T>::get() > 0, Error::<T>::Unconfigured);
            Enabled::<T>::put(true);
            Self::deposit_event(Event::Activated {
                at: frame_system::Pallet::<T>::block_number(),
            });
            Ok(())
        }
    }
    impl<T: Config> Pallet<T> {
        pub fn record_notice(event: contract::EventV1) {
            Self::deposit_event(Event::Notice { event });
        }
        fn kernel_error(error: contract::AmmErrorV1) -> DispatchError {
            match error {
                contract::AmmErrorV1::NotFound => Error::<T>::NotFound.into(),
                contract::AmmErrorV1::InvalidLimit => Error::<T>::InvalidLimit.into(),
                contract::AmmErrorV1::InvalidCursor => Error::<T>::InvalidCursor.into(),
                contract::AmmErrorV1::UnsupportedAsset => Error::<T>::UnsupportedAsset.into(),
                contract::AmmErrorV1::Unconfigured => Error::<T>::Unconfigured.into(),
                contract::AmmErrorV1::Arithmetic => Error::<T>::Arithmetic.into(),
                contract::AmmErrorV1::BackendInvariant => Error::<T>::BackendInvariant.into(),
                contract::AmmErrorV1::IdenticalAssets => Error::<T>::IdenticalAssets.into(),
                contract::AmmErrorV1::Unauthorized => Error::<T>::Unauthorized.into(),
                contract::AmmErrorV1::PoolAlreadyExists => Error::<T>::PoolAlreadyExists.into(),
                contract::AmmErrorV1::CustodyCollision => Error::<T>::CustodyCollision.into(),
                contract::AmmErrorV1::CustodyNotEmpty => Error::<T>::CustodyNotEmpty.into(),
                contract::AmmErrorV1::CustodyMismatch => Error::<T>::CustodyMismatch.into(),
                contract::AmmErrorV1::CustodyNotLive => Error::<T>::CustodyNotLive.into(),
                contract::AmmErrorV1::DeadlineExpired => Error::<T>::DeadlineExpired.into(),
                contract::AmmErrorV1::BoundExceeded => Error::<T>::BoundExceeded.into(),
                contract::AmmErrorV1::ZeroAmount => Error::<T>::ZeroAmount.into(),
                contract::AmmErrorV1::InsufficientInitialLiquidity => {
                    Error::<T>::InsufficientInitialLiquidity.into()
                }
                contract::AmmErrorV1::InsufficientLiquidity => {
                    Error::<T>::InsufficientLiquidity.into()
                }
                contract::AmmErrorV1::InsufficientLp => Error::<T>::InsufficientLp.into(),
                contract::AmmErrorV1::SlippageExceeded => Error::<T>::SlippageExceeded.into(),
                contract::AmmErrorV1::InvalidConfiguration => {
                    Error::<T>::InvalidConfiguration.into()
                }
                contract::AmmErrorV1::BelowMinimum => Error::<T>::BelowMinimum.into(),
                contract::AmmErrorV1::InsufficientBalance => Error::<T>::InsufficientBalance.into(),
                contract::AmmErrorV1::Frozen => Error::<T>::Frozen.into(),
                contract::AmmErrorV1::DepositFailure => Error::<T>::DepositFailure.into(),
                contract::AmmErrorV1::BackendRejected => Error::<T>::BackendRejected.into(),
                contract::AmmErrorV1::Reentrant => Error::<T>::Reentrant.into(),
                contract::AmmErrorV1::TransactionLimit => Error::<T>::TransactionLimit.into(),
                contract::AmmErrorV1::CorruptState => Error::<T>::CorruptState.into(),
            }
        }
    }
}

pub use pallet::*;
