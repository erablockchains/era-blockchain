//! ERA AI Prediction Tokenization pallet.
//!
//! Phase 1 scope:
//! - Register off-chain AI models.
//! - Submit verifiable prediction proof records.
//! - Store prediction hashes and metadata URIs on-chain.
//! - Validate prediction outcomes through authorized validators/oracles/reviewers.
//! - Maintain basic model statistics.
//!
//! Out of scope for Phase 1:
//! - Public prediction markets.
//! - Reward pools.
//! - Slashing.
//! - NFTs.
//! - Outcome tokens.
//! - Liquidity or market making.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::vec::Vec;
use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use frame_support::pallet_prelude::*;
use scale_info::TypeInfo;

pub use pallet::*;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

pub mod weights;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

/// Stable broad prediction domains.
///
/// Detailed prediction category is intentionally stored separately as a flexible
/// bounded `category_code`, for example:
/// - `finance.debt_default_risk`
/// - `media.movie_box_office`
/// - `enterprise.accounts_payable.invoice_duplicate_risk`
#[derive(
    Encode,
    Decode,
    DecodeWithMemTracking,
    Clone,
    Copy,
    PartialEq,
    Eq,
    RuntimeDebug,
    TypeInfo,
    MaxEncodedLen,
)]
pub enum PredictionDomain {
    Enterprise,
    Finance,
    Risk,
    Market,
    SupplyChain,
    MediaEntertainment,
    GamingVirtualEconomy,
    AiBenchmark,
    WeatherClimate,
    CommunitySocial,
    SportsEventsPermitted,
    EraEcosystem,
    Other,
}

#[derive(
    Encode,
    Decode,
    DecodeWithMemTracking,
    Clone,
    Copy,
    PartialEq,
    Eq,
    RuntimeDebug,
    TypeInfo,
    MaxEncodedLen,
)]
pub enum PredictionStatus {
    Open,
    Validated,
    Closed,
    Expired,
}

#[derive(
    Encode,
    Decode,
    DecodeWithMemTracking,
    Clone,
    Copy,
    PartialEq,
    Eq,
    RuntimeDebug,
    TypeInfo,
    MaxEncodedLen,
)]
pub enum PredictionOutcome {
    Successful,
    Failed,
    Inconclusive,
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct AiModel<AccountId, BlockNumber, BoundedHash, BoundedUri> {
    pub owner: AccountId,
    pub model_hash: BoundedHash,
    pub metadata_uri: BoundedUri,
    pub registered_at: BlockNumber,
    pub updated_at: Option<BlockNumber>,
    pub active: bool,
}

#[derive(
    Encode,
    Decode,
    DecodeWithMemTracking,
    Clone,
    Copy,
    PartialEq,
    Eq,
    RuntimeDebug,
    TypeInfo,
    MaxEncodedLen,
)]
pub enum ModelStatus {
    Pending,
    Approved,
    Suspended,
    Rejected,
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct ModelGovernance<AccountId, BlockNumber> {
    pub status: ModelStatus,
    pub authorized_submitter: Option<AccountId>,
    pub updated_at: BlockNumber,
}

#[derive(
    Encode,
    Decode,
    DecodeWithMemTracking,
    Clone,
    PartialEq,
    Eq,
    RuntimeDebug,
    TypeInfo,
    MaxEncodedLen,
)]
pub struct ModelOnboardingSettings {
    pub model_registration_enabled: bool,
    pub prediction_submission_enabled: bool,
    pub market_creation_enabled: bool,
    pub public_model_registration_enabled: bool,
    pub require_model_approval: bool,
    pub max_active_predictions_per_model: u32,
}

impl Default for ModelOnboardingSettings {
    fn default() -> Self {
        Self {
            model_registration_enabled: true,
            prediction_submission_enabled: true,
            market_creation_enabled: true,
            public_model_registration_enabled: false,
            require_model_approval: true,
            max_active_predictions_per_model: 32,
        }
    }
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct Prediction<AccountId, BlockNumber, BoundedHash, BoundedUri, BoundedCategoryCode> {
    pub model_id: u64,
    pub submitter: AccountId,
    pub domain: PredictionDomain,
    pub category_code: BoundedCategoryCode,
    pub prediction_hash: BoundedHash,
    pub metadata_uri: BoundedUri,
    pub confidence: u8,
    pub created_at: BlockNumber,
    pub expires_at: BlockNumber,
    pub status: PredictionStatus,
    pub outcome: Option<PredictionOutcome>,
    pub validator: Option<AccountId>,
    pub validated_at: Option<BlockNumber>,
}

#[derive(Encode, Decode, Clone, Default, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct ModelStats {
    pub total_predictions: u64,
    pub validated_predictions: u64,
    pub successful_predictions: u64,
    pub failed_predictions: u64,
    pub inconclusive_predictions: u64,
}

#[derive(
    Encode,
    Decode,
    DecodeWithMemTracking,
    Clone,
    PartialEq,
    Eq,
    RuntimeDebug,
    TypeInfo,
    MaxEncodedLen,
)]
pub struct PredictionToken<AccountId, BlockNumber, BoundedUri> {
    pub prediction_id: u64,
    pub owner: AccountId,
    pub metadata_uri: BoundedUri,
    pub created_at: BlockNumber,
    pub updated_at: Option<BlockNumber>,
    pub transferable: bool,
    pub frozen: bool,
    pub burned: bool,
}

#[derive(
    Encode,
    Decode,
    DecodeWithMemTracking,
    Clone,
    Copy,
    PartialEq,
    Eq,
    RuntimeDebug,
    TypeInfo,
    MaxEncodedLen,
    Default,
)]
pub struct TokenizationSettings {
    pub tokenization_enabled: bool,
    pub transfers_enabled: bool,
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct StakingSettings<Balance> {
    pub staking_enabled: bool,
    pub min_stake: Balance,
}

impl<Balance: Default> Default for StakingSettings<Balance> {
    fn default() -> Self {
        Self {
            staking_enabled: false,
            min_stake: Balance::default(),
        }
    }
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct PredictionTokenStake<Balance, BlockNumber> {
    pub amount: Balance,
    pub created_at: BlockNumber,
    pub updated_at: Option<BlockNumber>,
}

#[derive(
    Encode,
    Decode,
    DecodeWithMemTracking,
    Clone,
    Copy,
    PartialEq,
    Eq,
    RuntimeDebug,
    TypeInfo,
    MaxEncodedLen,
)]
pub enum SettlementStatus {
    NotStarted,
    OutcomeProposed,
    DisputeWindowOpen,
    Disputed,
    Finalized,
    Cancelled,
}

impl Default for SettlementStatus {
    fn default() -> Self {
        Self::NotStarted
    }
}

#[derive(
    Encode,
    Decode,
    DecodeWithMemTracking,
    Clone,
    Copy,
    PartialEq,
    Eq,
    RuntimeDebug,
    TypeInfo,
    MaxEncodedLen,
)]
pub enum SettlementOutcome {
    Correct,
    Incorrect,
    Inconclusive,
    Cancelled,
    Fraudulent,
}

#[derive(
    Encode,
    Decode,
    DecodeWithMemTracking,
    Clone,
    Copy,
    PartialEq,
    Eq,
    RuntimeDebug,
    TypeInfo,
    MaxEncodedLen,
)]
pub enum DisputeReason {
    WrongOutcome,
    InvalidSource,
    SourceUnavailable,
    AmbiguousPrediction,
    MarketManipulation,
    DuplicateOrFraudulentPrediction,
    TechnicalError,
    Other,
}

#[derive(
    Encode,
    Decode,
    DecodeWithMemTracking,
    Clone,
    PartialEq,
    Eq,
    RuntimeDebug,
    TypeInfo,
    MaxEncodedLen,
)]
pub struct SettlementSettings<Balance, BlockNumber> {
    pub settlement_enabled: bool,
    pub dispute_window_blocks: BlockNumber,
    pub min_dispute_bond: Balance,
}

impl<Balance: Default, BlockNumber: Default> Default for SettlementSettings<Balance, BlockNumber> {
    fn default() -> Self {
        Self {
            settlement_enabled: false,
            dispute_window_blocks: BlockNumber::default(),
            min_dispute_bond: Balance::default(),
        }
    }
}

#[derive(
    Encode,
    Decode,
    DecodeWithMemTracking,
    Clone,
    PartialEq,
    Eq,
    RuntimeDebug,
    TypeInfo,
    MaxEncodedLen,
)]
pub struct PredictionTokenSettlement<AccountId, Balance, BlockNumber, BoundedUri> {
    pub token_id: u64,
    pub status: SettlementStatus,
    pub proposed_outcome: Option<SettlementOutcome>,
    pub final_outcome: Option<SettlementOutcome>,
    pub proposed_by: Option<AccountId>,
    pub finalized_by: Option<AccountId>,
    pub proposed_at: Option<BlockNumber>,
    pub dispute_until: Option<BlockNumber>,
    pub finalized_at: Option<BlockNumber>,
    pub evidence_uri: Option<BoundedUri>,
    pub total_stake_at_finalization: Balance,
    pub dispute_count: u32,
}

#[derive(
    Encode,
    Decode,
    DecodeWithMemTracking,
    Clone,
    PartialEq,
    Eq,
    RuntimeDebug,
    TypeInfo,
    MaxEncodedLen,
)]
pub struct PredictionTokenDispute<AccountId, Balance, BlockNumber, BoundedUri> {
    pub dispute_id: u64,
    pub token_id: u64,
    pub disputed_by: AccountId,
    pub reason: DisputeReason,
    pub evidence_uri: BoundedUri,
    pub bond: Balance,
    pub submitted_at: BlockNumber,
    pub resolved: bool,
    pub accepted: Option<bool>,
}

#[derive(
    Encode,
    Decode,
    DecodeWithMemTracking,
    Clone,
    Default,
    Eq,
    PartialEq,
    RuntimeDebug,
    MaxEncodedLen,
    TypeInfo,
)]
pub struct SettlementEconomicsSettings {
    pub economics_enabled: bool,
    pub slash_incorrect: bool,
    pub slash_fraudulent: bool,
}

#[derive(
    Encode,
    Decode,
    DecodeWithMemTracking,
    Clone,
    PartialEq,
    Eq,
    RuntimeDebug,
    MaxEncodedLen,
    TypeInfo,
)]
pub enum PredictionOutcomeSide {
    Yes,
    No,
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct PredictionSideStake<Balance, BlockNumber> {
    pub side: PredictionOutcomeSide,
    pub amount: Balance,
    pub staked_at: BlockNumber,
    pub updated_at: Option<BlockNumber>,
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct PredictionSidePayoutClaim<Balance, BlockNumber> {
    pub claimed_at: BlockNumber,
    pub stake_returned: Balance,
    pub reward_paid: Balance,
    pub slashed: Balance,
}

const STORAGE_VERSION: frame_support::traits::StorageVersion =
    frame_support::traits::StorageVersion::new(3);

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct PredictionMarketEconomicsSettings {
    pub market_enabled: bool,
    pub allow_unstake_before_settlement: bool,
    pub fees_enabled: bool,
    pub fee_bps: u16,
    pub treasury_enabled: bool,
}

impl Default for PredictionMarketEconomicsSettings {
    fn default() -> Self {
        Self {
            market_enabled: false,
            allow_unstake_before_settlement: true,
            fees_enabled: false,
            fee_bps: 0,
            treasury_enabled: true,
        }
    }
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct PredictionMarketEconomicsSettingsV1 {
    pub market_enabled: bool,
    pub allow_unstake_before_settlement: bool,
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen, Default)]
pub struct PredictionMarketAccounting<Balance> {
    pub fees_collected: Balance,
    pub remainders_collected: Balance,
    pub total_paid: Balance,
    pub total_refunded: Balance,
    pub total_slashed: Balance,
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct PredictionTokenSettlementClaim<Balance, BlockNumber> {
    pub claimed_at: BlockNumber,
    pub refunded: Balance,
    pub slashed: Balance,
}

pub trait WeightInfo {
    fn submit_evaluation() -> Weight { Weight::MAX }
    fn record_evaluation_evidence() -> Weight { Weight::MAX }
    fn register_model() -> Weight;
    fn update_model() -> Weight;
    fn approve_model() -> Weight;
    fn suspend_model() -> Weight;
    fn reject_model() -> Weight;
    fn set_model_submitter() -> Weight;
    fn submit_prediction() -> Weight;
    fn validate_prediction() -> Weight;
    fn close_prediction() -> Weight;
    fn authorize_validator() -> Weight;
    fn remove_validator() -> Weight;

    fn set_tokenization_config() -> Weight {
        Weight::zero()
    }
    fn tokenize_prediction() -> Weight {
        Weight::zero()
    }
    fn approve_prediction_token_transfer() -> Weight {
        Weight::zero()
    }
    fn revoke_prediction_token_approval() -> Weight {
        Weight::zero()
    }
    fn transfer_prediction_token() -> Weight {
        Weight::zero()
    }
    fn freeze_prediction_token() -> Weight {
        Weight::zero()
    }
    fn unfreeze_prediction_token() -> Weight {
        Weight::zero()
    }
    fn burn_prediction_token() -> Weight {
        Weight::zero()
    }
    fn update_prediction_token_metadata() -> Weight {
        Weight::zero()
    }
    fn set_staking_config() -> Weight;
    fn stake_on_prediction_token() -> Weight;
    fn unstake_prediction_token() -> Weight;
    fn lock_prediction_token_stake() -> Weight;
    fn release_prediction_token_stake() -> Weight;
    fn set_settlement_config() -> Weight;
    fn propose_prediction_outcome() -> Weight;
    fn dispute_prediction_outcome() -> Weight;
    fn admin_finalize_prediction_outcome() -> Weight;
    fn finalize_prediction_outcome_after_dispute_window() -> Weight;
    fn cancel_prediction_settlement() -> Weight;
    fn set_settlement_economics_config() -> Weight;
    fn claim_prediction_token_settlement() -> Weight;
    fn resolve_prediction_dispute_bond() -> Weight;
    fn set_market_economics_config() -> Weight;
    fn set_market_fee_config() -> Weight;
    fn stake_on_prediction_outcome_side() -> Weight;
    fn unstake_from_prediction_outcome_side() -> Weight;
    fn claim_prediction_market_payout() -> Weight;
}

impl WeightInfo for () {
    fn register_model() -> Weight {
        Weight::from_parts(20_000, 0)
    }
    fn update_model() -> Weight {
        Weight::from_parts(20_000, 0)
    }
    fn approve_model() -> Weight {
        Self::update_model()
    }
    fn suspend_model() -> Weight {
        Self::update_model()
    }
    fn reject_model() -> Weight {
        Self::update_model()
    }
    fn set_model_submitter() -> Weight {
        Self::update_model()
    }
    fn submit_prediction() -> Weight {
        Weight::from_parts(30_000, 0)
    }
    fn validate_prediction() -> Weight {
        Weight::from_parts(30_000, 0)
    }
    fn close_prediction() -> Weight {
        Weight::from_parts(20_000, 0)
    }
    fn authorize_validator() -> Weight {
        Weight::from_parts(10_000, 0)
    }
    fn remove_validator() -> Weight {
        Weight::from_parts(10_000, 0)
    }
    fn set_staking_config() -> Weight {
        Weight::from_parts(10_000, 0)
    }
    fn stake_on_prediction_token() -> Weight {
        Weight::from_parts(10_000, 0)
    }
    fn unstake_prediction_token() -> Weight {
        Weight::from_parts(10_000, 0)
    }
    fn lock_prediction_token_stake() -> Weight {
        Weight::from_parts(10_000, 0)
    }
    fn release_prediction_token_stake() -> Weight {
        Weight::from_parts(10_000, 0)
    }
    fn set_settlement_config() -> Weight {
        Weight::from_parts(10_000, 0)
    }
    fn propose_prediction_outcome() -> Weight {
        Weight::from_parts(10_000, 0)
    }
    fn dispute_prediction_outcome() -> Weight {
        Weight::from_parts(10_000, 0)
    }
    fn admin_finalize_prediction_outcome() -> Weight {
        Weight::from_parts(10_000, 0)
    }
    fn finalize_prediction_outcome_after_dispute_window() -> Weight {
        Weight::from_parts(10_000, 0)
    }
    fn cancel_prediction_settlement() -> Weight {
        Weight::from_parts(10_000, 0)
    }

    fn set_settlement_economics_config() -> Weight {
        Weight::from_parts(10_000, 0)
    }

    fn claim_prediction_token_settlement() -> Weight {
        Weight::from_parts(10_000, 0)
    }

    fn resolve_prediction_dispute_bond() -> Weight {
        Weight::from_parts(10_000, 0)
    }
    fn set_market_economics_config() -> Weight {
        Weight::zero()
    }
    fn set_market_fee_config() -> Weight {
        Weight::zero()
    }
    fn stake_on_prediction_outcome_side() -> Weight {
        Weight::zero()
    }
    fn unstake_from_prediction_outcome_side() -> Weight {
        Weight::zero()
    }
    fn claim_prediction_market_payout() -> Weight {
        Weight::zero()
    }
}

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_support::traits::{tokens::BalanceStatus, Currency, ReservableCurrency};
    use frame_system::pallet_prelude::*;
    use sp_runtime::traits::AccountIdConversion;
    use sp_runtime::traits::{SaturatedConversion, Saturating, Zero};
    use sp_runtime::Perquintill;

    pub type ModelId = u64;
    pub type PredictionId = u64;
    pub type PredictionTokenId = u64;
    pub type DisputeId = u64;

    pub type BalanceOf<T> =
        <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

    pub type BoundedHashOf<T> = BoundedVec<u8, <T as Config>::MaxHashLen>;
    pub type BoundedMetadataUriOf<T> = BoundedVec<u8, <T as Config>::MaxMetadataUriLen>;
    pub type BoundedCategoryCodeOf<T> = BoundedVec<u8, <T as Config>::MaxCategoryCodeLen>;

    pub type AiModelOf<T> = AiModel<
        <T as frame_system::Config>::AccountId,
        BlockNumberFor<T>,
        BoundedHashOf<T>,
        BoundedMetadataUriOf<T>,
    >;

    pub type PredictionOf<T> = Prediction<
        <T as frame_system::Config>::AccountId,
        BlockNumberFor<T>,
        BoundedHashOf<T>,
        BoundedMetadataUriOf<T>,
        BoundedCategoryCodeOf<T>,
    >;

    pub type PredictionTokenOf<T> = PredictionToken<
        <T as frame_system::Config>::AccountId,
        BlockNumberFor<T>,
        BoundedMetadataUriOf<T>,
    >;

    pub type PredictionTokenStakeOf<T> = PredictionTokenStake<BalanceOf<T>, BlockNumberFor<T>>;

    pub type SettlementSettingsOf<T> = SettlementSettings<BalanceOf<T>, BlockNumberFor<T>>;

    pub type PredictionTokenSettlementOf<T> = PredictionTokenSettlement<
        <T as frame_system::Config>::AccountId,
        BalanceOf<T>,
        BlockNumberFor<T>,
        BoundedMetadataUriOf<T>,
    >;

    pub type PredictionTokenDisputeOf<T> = PredictionTokenDispute<
        <T as frame_system::Config>::AccountId,
        BalanceOf<T>,
        BlockNumberFor<T>,
        BoundedMetadataUriOf<T>,
    >;

    pub type PredictionTokenSettlementClaimOf<T> =
        PredictionTokenSettlementClaim<BalanceOf<T>, BlockNumberFor<T>>;

    pub type PredictionSideStakeOf<T> = PredictionSideStake<BalanceOf<T>, BlockNumberFor<T>>;

    pub type PredictionSidePayoutClaimOf<T> =
        PredictionSidePayoutClaim<BalanceOf<T>, BlockNumberFor<T>>;
    pub type PredictionMarketAccountingOf<T> = PredictionMarketAccounting<BalanceOf<T>>;

    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    #[pallet::config]
    pub trait Config: frame_system::Config<RuntimeEvent: From<Event<Self>>> {
        #[pallet::constant]
        type MaxHashLen: Get<u32>;

        #[pallet::constant]
        type MaxMetadataUriLen: Get<u32>;

        #[pallet::constant]
        type MaxCategoryCodeLen: Get<u32>;

        #[pallet::constant]
        type MaxModelsPerOwner: Get<u32>;

        #[pallet::constant]
        type MaxPredictionsPerModel: Get<u32>;

        /// Compile-time policy. False rejects creation of financial obligations and tokens.
        #[pallet::constant]
        type FinancialModesAllowed: Get<bool>;
        type NowSeconds: Get<u64>;
        type Currency: ReservableCurrency<Self::AccountId>;
        type PalletId: frame_support::traits::Get<frame_support::PalletId>;
        /// Approved keyless, accumulation-only destination. No issuance is burned.
        type PenaltyDestination: Get<Self::AccountId>;

        type WeightInfo: WeightInfo;
    }

    /// Immutable artifact commitment for an existing prediction ID. Payload hash commits
    /// canonical input/version/horizon/terms off-chain. No token or financial position.
    #[pallet::storage]
    pub type EvaluationBindings<T: Config> = StorageMap<_, Blake2_128Concat, PredictionId, [u8;32], OptionQuery>;
    #[pallet::storage]
    pub type EvaluationTiming<T: Config> = StorageMap<_, Blake2_128Concat, PredictionId, (u64, u64), OptionQuery>;
    /// Stable business key survives process restart and delegation changes.
    #[pallet::storage]
    pub type EvaluationRequests<T: Config> = StorageDoubleMap<_, Blake2_128Concat, ModelId, Blake2_128Concat, [u8;32], PredictionId, OptionQuery>;
    #[pallet::storage]
    pub type EvaluationRevisionCount<T: Config> = StorageMap<_, Blake2_128Concat, PredictionId, u32, ValueQuery>;
    /// Append-only evidence revisions, bounded to 32 per prediction. Original validation
    /// statistics are historical; clients must derive corrected scores from this log.
    #[pallet::storage]
    pub type EvaluationEvidence<T: Config> = StorageDoubleMap<_, Blake2_128Concat, PredictionId, Twox64Concat, u32, (T::AccountId, [u8;32], PredictionOutcome, BlockNumberFor<T>), OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn next_model_id)]
    pub type NextModelId<T: Config> = StorageValue<_, ModelId, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn next_prediction_id)]
    pub type NextPredictionId<T: Config> = StorageValue<_, PredictionId, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn models)]
    pub type Models<T: Config> =
        StorageMap<_, Blake2_128Concat, ModelId, AiModelOf<T>, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn predictions)]
    pub type Predictions<T: Config> =
        StorageMap<_, Blake2_128Concat, PredictionId, PredictionOf<T>, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn model_stats)]
    pub type ModelStatsById<T: Config> =
        StorageMap<_, Blake2_128Concat, ModelId, ModelStats, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn model_governance)]
    pub type ModelGovernanceById<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        ModelId,
        ModelGovernance<T::AccountId, BlockNumberFor<T>>,
        OptionQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn model_onboarding_config)]
    pub type ModelOnboardingConfig<T: Config> =
        StorageValue<_, ModelOnboardingSettings, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn model_ids_by_owner)]
    pub type ModelIdsByOwner<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        BoundedVec<ModelId, T::MaxModelsPerOwner>,
        ValueQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn prediction_ids_by_model)]
    pub type PredictionIdsByModel<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        ModelId,
        BoundedVec<PredictionId, T::MaxPredictionsPerModel>,
        ValueQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn active_prediction_count_by_model)]
    pub type ActivePredictionCountByModel<T: Config> =
        StorageMap<_, Blake2_128Concat, ModelId, u32, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn authorized_validators)]
    pub type AuthorizedValidators<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, bool, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn next_prediction_token_id)]
    pub type NextPredictionTokenId<T: Config> = StorageValue<_, PredictionTokenId, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn prediction_tokens)]
    pub type PredictionTokens<T: Config> =
        StorageMap<_, Blake2_128Concat, PredictionTokenId, PredictionTokenOf<T>, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn prediction_token_by_prediction)]
    pub type PredictionTokenByPrediction<T: Config> =
        StorageMap<_, Blake2_128Concat, PredictionId, PredictionTokenId, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn prediction_token_approvals)]
    pub type PredictionTokenApprovals<T: Config> =
        StorageMap<_, Blake2_128Concat, PredictionTokenId, T::AccountId, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn tokenization_config)]
    pub type TokenizationConfig<T: Config> = StorageValue<_, TokenizationSettings, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn staking_config)]
    pub type StakingConfig<T: Config> = StorageValue<_, StakingSettings<BalanceOf<T>>, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn prediction_token_stakes)]
    pub type PredictionTokenStakes<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        PredictionTokenId,
        Blake2_128Concat,
        T::AccountId,
        PredictionTokenStakeOf<T>,
        OptionQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn prediction_token_total_stake)]
    pub type PredictionTokenTotalStake<T: Config> =
        StorageMap<_, Blake2_128Concat, PredictionTokenId, BalanceOf<T>, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn prediction_token_stake_locked)]
    pub type PredictionTokenStakeLocked<T: Config> =
        StorageMap<_, Blake2_128Concat, PredictionTokenId, bool, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn settlement_config)]
    pub type SettlementConfig<T: Config> = StorageValue<_, SettlementSettingsOf<T>, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn prediction_token_settlements)]
    pub type PredictionTokenSettlements<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        PredictionTokenId,
        PredictionTokenSettlementOf<T>,
        OptionQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn next_dispute_id)]
    pub type NextDisputeId<T: Config> = StorageValue<_, DisputeId, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn prediction_token_disputes)]
    pub type PredictionTokenDisputes<T: Config> =
        StorageMap<_, Blake2_128Concat, DisputeId, PredictionTokenDisputeOf<T>, OptionQuery>;

    #[pallet::storage]
    pub type SettlementEconomicsConfig<T: Config> =
        StorageValue<_, SettlementEconomicsSettings, ValueQuery>;

    #[pallet::storage]
    pub type PredictionTokenSettlementClaims<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        PredictionTokenId,
        Blake2_128Concat,
        T::AccountId,
        PredictionTokenSettlementClaimOf<T>,
        OptionQuery,
    >;

    #[pallet::storage]
    pub type PredictionMarketEconomicsConfig<T: Config> =
        StorageValue<_, PredictionMarketEconomicsSettings, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn prediction_token_side_stakes)]
    pub type PredictionTokenSideStakes<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        PredictionTokenId,
        Blake2_128Concat,
        T::AccountId,
        PredictionSideStakeOf<T>,
        OptionQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn prediction_token_side_totals)]
    pub type PredictionTokenSideTotals<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        PredictionTokenId,
        Blake2_128Concat,
        PredictionOutcomeSide,
        BalanceOf<T>,
        ValueQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn prediction_token_side_payout_claims)]
    pub type PredictionTokenSidePayoutClaims<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        PredictionTokenId,
        Blake2_128Concat,
        T::AccountId,
        PredictionSidePayoutClaimOf<T>,
        OptionQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn prediction_token_market_locked)]
    pub type PredictionTokenMarketLocked<T: Config> =
        StorageMap<_, Blake2_128Concat, PredictionTokenId, bool, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn prediction_token_market_accounting)]
    pub type PredictionTokenMarketAccounting<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        PredictionTokenId,
        PredictionMarketAccountingOf<T>,
        ValueQuery,
    >;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        ModelRegistered {
            model_id: ModelId,
            owner: T::AccountId,
        },
        ModelUpdated {
            model_id: ModelId,
            owner: T::AccountId,
        },
        ModelApproved {
            model_id: ModelId,
        },
        ModelSuspended {
            model_id: ModelId,
        },
        ModelRejected {
            model_id: ModelId,
        },
        ModelSubmitterUpdated {
            model_id: ModelId,
            submitter: Option<T::AccountId>,
        },
        ModelOnboardingConfigUpdated {
            config: ModelOnboardingSettings,
        },
        PredictionOpened {
            prediction_id: PredictionId,
            model_id: ModelId,
        },
        PredictionSubmitted {
            prediction_id: PredictionId,
            model_id: ModelId,
            submitter: T::AccountId,
            domain: PredictionDomain,
        },
        PredictionValidated {
            prediction_id: PredictionId,
            model_id: ModelId,
            validator: T::AccountId,
            outcome: PredictionOutcome,
        },
        PredictionClosed {
            prediction_id: PredictionId,
        },
        TokenizationConfigUpdated {
            tokenization_enabled: bool,
            transfers_enabled: bool,
        },
        PredictionTokenized {
            prediction_id: PredictionId,
            token_id: PredictionTokenId,
            owner: T::AccountId,
        },
        PredictionTokenTransferred {
            token_id: PredictionTokenId,
            from: T::AccountId,
            to: T::AccountId,
        },
        PredictionTokenTransferApproved {
            token_id: PredictionTokenId,
            owner: T::AccountId,
            approved: T::AccountId,
        },
        PredictionTokenApprovalRevoked {
            token_id: PredictionTokenId,
            owner: T::AccountId,
        },
        PredictionTokenFrozen {
            token_id: PredictionTokenId,
        },
        PredictionTokenUnfrozen {
            token_id: PredictionTokenId,
        },
        PredictionTokenBurned {
            token_id: PredictionTokenId,
        },
        PredictionTokenMetadataUpdated {
            token_id: PredictionTokenId,
            owner: T::AccountId,
        },
        StakingConfigUpdated {
            staking_enabled: bool,
            min_stake: BalanceOf<T>,
        },
        PredictionTokenStakeAdded {
            token_id: PredictionTokenId,
            staker: T::AccountId,
            amount: BalanceOf<T>,
            total_stake: BalanceOf<T>,
        },
        PredictionTokenStakeRemoved {
            token_id: PredictionTokenId,
            staker: T::AccountId,
            amount: BalanceOf<T>,
            remaining_stake: BalanceOf<T>,
        },
        PredictionTokenStakeLocked {
            token_id: PredictionTokenId,
        },
        PredictionTokenStakeReleased {
            token_id: PredictionTokenId,
        },
        SettlementConfigUpdated {
            settlement_enabled: bool,
            dispute_window_blocks: BlockNumberFor<T>,
            min_dispute_bond: BalanceOf<T>,
        },
        PredictionOutcomeProposed {
            token_id: PredictionTokenId,
            proposed_outcome: SettlementOutcome,
            proposed_by: Option<T::AccountId>,
            dispute_until: BlockNumberFor<T>,
        },
        PredictionOutcomeDisputed {
            token_id: PredictionTokenId,
            dispute_id: DisputeId,
            disputed_by: T::AccountId,
            reason: DisputeReason,
            bond: BalanceOf<T>,
        },
        PredictionOutcomeFinalized {
            token_id: PredictionTokenId,
            final_outcome: SettlementOutcome,
            finalized_by: Option<T::AccountId>,
        },
        PredictionSettlementCancelled {
            token_id: PredictionTokenId,
        },
        ValidatorAuthorized {
            account: T::AccountId,
        },
        ValidatorRemoved {
            account: T::AccountId,
        },

        SettlementEconomicsConfigUpdated {
            economics_enabled: bool,
            slash_incorrect: bool,
            slash_fraudulent: bool,
        },
        PredictionTokenSettlementClaimed {
            token_id: PredictionTokenId,
            staker: T::AccountId,
            refunded: BalanceOf<T>,
            slashed: BalanceOf<T>,
        },
        PredictionDisputeBondResolved {
            dispute_id: DisputeId,
            token_id: PredictionTokenId,
            disputed_by: T::AccountId,
            accepted: bool,
            bond: BalanceOf<T>,
        },

        PredictionMarketEconomicsConfigUpdated {
            market_enabled: bool,
            allow_unstake_before_settlement: bool,
        },
        PredictionOutcomeSideStaked {
            token_id: PredictionTokenId,
            staker: T::AccountId,
            side: PredictionOutcomeSide,
            amount: BalanceOf<T>,
            total_for_side: BalanceOf<T>,
        },
        PredictionOutcomeSideUnstaked {
            token_id: PredictionTokenId,
            staker: T::AccountId,
            side: PredictionOutcomeSide,
            amount: BalanceOf<T>,
            remaining_stake: BalanceOf<T>,
        },
        PredictionTokenMarketLocked {
            token_id: PredictionTokenId,
        },
        PredictionMarketPayoutClaimed {
            token_id: PredictionTokenId,
            staker: T::AccountId,
            side: PredictionOutcomeSide,
            stake_returned: BalanceOf<T>,
            reward_paid: BalanceOf<T>,
            slashed: BalanceOf<T>,
        },
        PredictionMarketFeeConfigUpdated {
            fees_enabled: bool,
            fee_bps: u16,
            treasury_enabled: bool,
        },
        PredictionMarketAccountingUpdated {
            token_id: PredictionTokenId,
            fees_collected: BalanceOf<T>,
            remainders_collected: BalanceOf<T>,
            total_paid: BalanceOf<T>,
            total_refunded: BalanceOf<T>,
            total_slashed: BalanceOf<T>,
        },
        ReservedPenaltyTransferred {
            payer: T::AccountId,
            destination: T::AccountId,
            amount: BalanceOf<T>,
            /// 0: incorrect prediction, 1: fraudulent prediction, 2: explicitly adjudicated fraudulent dispute bond.
            source: u8,
            reference: u64,
        },
        DisputeBondAdjudicated { dispute_id: DisputeId, accepted: bool, fraudulent: bool, evidence_hash: [u8;32] },
        PredictionMarketNoWinnerRefund { token_id: PredictionTokenId, staker: T::AccountId, amount: BalanceOf<T> },
        EvaluationRecorded { prediction_id: PredictionId, request_id: [u8;32], artifact: [u8;32] },
        EvaluationEvidenceRecorded { prediction_id: PredictionId, revision: u32, evidence: [u8;32], outcome: PredictionOutcome },
    }

    #[pallet::error]
    pub enum Error<T> {
        ModelNotFound,
        PredictionNotFound,
        NotModelOwner,
        InvalidConfidence,
        PredictionExpired,
        PredictionAlreadyValidated,
        PredictionAlreadyClosed,
        NotAuthorizedValidator,
        InvalidMetadata,
        InvalidModelHash,
        InvalidPredictionHash,
        InvalidCategoryCode,
        ModelInactive,
        Overflow,
        TokenizationDisabled,
        TransfersDisabled,
        PredictionAlreadyTokenized,
        PredictionTokenNotFound,
        NotPredictionOwner,
        NotPredictionTokenOwner,
        TokenFrozen,
        TokenBurned,
        TokenNotTransferable,
        TransferNotApproved,
        CannotTransferToSelf,
        StakingDisabled,
        StakeAmountZero,
        StakeBelowMinimum,
        StakeNotFound,
        InsufficientStake,
        StakeLocked,
        SettlementDisabled,
        SettlementAlreadyStarted,
        SettlementNotFound,
        SettlementAlreadyFinalized,
        SettlementNotFinalizable,
        SettlementNotDisputable,
        DisputeWindowClosed,
        DisputeWindowStillOpen,
        DisputeBondBelowMinimum,
        InvalidSettlementEvidence,
        OutcomeNotProposed,
        CannotDisputeFinalizedSettlement,
        NotAuthorizedSettlementProposer,
        SettlementEconomicsDisabled,
        SettlementNotFinalized,
        SettlementAlreadyClaimed,
        SettlementClaimNotAvailable,
        DisputeNotFound,
        DisputeAlreadyResolved,
        MarketEconomicsDisabled,
        MarketAlreadyLocked,
        MarketStakeAmountZero,
        MarketStakeNotFound,
        MarketStakeSideMismatch,
        MarketUnstakeDisabled,
        MarketPayoutAlreadyClaimed,
        MarketPayoutNotAvailable,
        MarketNoWinningStake,
        MarketLosingSideCannotClaim,
        MarketFeeBpsTooHigh,
        InvalidMarketOutcome,
        NotPermitted,
        ModelRegistrationDisabled,
        PublicModelRegistrationDisabled,
        PredictionSubmissionDisabled,
        MarketCreationDisabled,
        ModelNotApproved,
        ModelSuspended,
        ModelRejected,
        NotAuthorizedModelAdmin,
        NotAuthorizedModelSubmitter,
        ModelOwnerIndexFull,
        PredictionModelIndexFull,
        TooManyActivePredictions,
        PredictionNotOpen,
        ReserveInvariant,
        InvalidPenaltyDestination,
        InvalidBondAdjudication,
            FinancialModeDisabled,
        EvaluationSubmissionRequired,
        EvaluationEvidenceRequired,
        EvaluationRequestConflict,
        EvaluationArtifactChanged,
        EvaluationTooEarly,
        EvaluationReviewerConflict,
        EvaluationRevisionConflict,
}

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        fn on_runtime_upgrade() -> Weight {
            Self::migrate_market_economics_config_v1_to_v2()
        }

        #[cfg(feature = "try-runtime")]
        fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
            let version = frame_support::traits::StorageVersion::get::<Pallet<T>>();
            if version != frame_support::traits::StorageVersion::new(2)
                && version != STORAGE_VERSION
            {
                return Err("AI predictions Phase 6B expects storage version 2 or 3".into());
            }
            let snapshot = (
                PredictionMarketEconomicsConfig::<T>::get(),
                Models::<T>::iter_keys().count() as u64,
                Predictions::<T>::iter_keys().count() as u64,
                PredictionTokenMarketAccounting::<T>::iter_keys().count() as u64,
                PredictionTokenSideStakes::<T>::iter_keys().count() as u64,
                PredictionTokenSidePayoutClaims::<T>::iter_keys().count() as u64,
            );
            Ok((version, snapshot).encode())
        }

        #[cfg(feature = "try-runtime")]
        fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
            let (previous_version, (market_config, models, predictions, accounting, positions, claims)): (frame_support::traits::StorageVersion, (
                PredictionMarketEconomicsSettings,
                u64,
                u64,
                u64,
                u64,
                u64,
            )) = Decode::decode(&mut &state[..])
                .map_err(|_| "invalid Phase 6B pre-upgrade state")?;
            if frame_support::traits::StorageVersion::get::<Pallet<T>>() != STORAGE_VERSION {
                return Err("AI predictions storage version was not upgraded to 3".into());
            }
            if PredictionMarketEconomicsConfig::<T>::get() != market_config {
                return Err("Phase 6A market config changed during migration".into());
            }
            if Models::<T>::iter_keys().count() as u64 != models
                || Predictions::<T>::iter_keys().count() as u64 != predictions
            {
                return Err("canonical model or prediction records changed".into());
            }
            if PredictionTokenMarketAccounting::<T>::iter_keys().count() as u64 != accounting
                || PredictionTokenSideStakes::<T>::iter_keys().count() as u64 != positions
                || PredictionTokenSidePayoutClaims::<T>::iter_keys().count() as u64 != claims
            {
                return Err("Phase 6A accounting, positions, or claims changed".into());
            }
            for model_id in 0..NextModelId::<T>::get() {
                let Some(model) = Models::<T>::get(model_id) else {
                    continue;
                };
                let governance = ModelGovernanceById::<T>::get(model_id)
                    .ok_or("missing migrated model governance")?;
                let expected = if model.active {
                    ModelStatus::Approved
                } else {
                    ModelStatus::Suspended
                };
                if previous_version == frame_support::traits::StorageVersion::new(2) && governance.status != expected {
                    return Err("incorrect migrated model status".into());
                }
            }
            for (_, ids) in ModelIdsByOwner::<T>::iter() {
                if ids.as_slice().windows(2).any(|w| w[0] >= w[1]) {
                    return Err("owner model index is not deterministic".into());
                }
            }
            for (model_id, ids) in PredictionIdsByModel::<T>::iter() {
                if ids.as_slice().windows(2).any(|w| w[0] >= w[1]) {
                    return Err("model prediction index is not deterministic".into());
                }
                let expected = Predictions::<T>::iter()
                    .filter(|(_, p)| p.model_id == model_id && p.status == PredictionStatus::Open)
                    .count() as u32;
                if ActivePredictionCountByModel::<T>::get(model_id) != expected {
                    return Err("incorrect active prediction count".into());
                }
            }
            Ok(())
        }
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::call_index(0)]
        #[pallet::weight(T::WeightInfo::register_model())]
        pub fn register_model(
            origin: OriginFor<T>,
            model_hash: Vec<u8>,
            metadata_uri: Vec<u8>,
        ) -> DispatchResult {
            let owner = ensure_signed(origin)?;
            let onboarding = ModelOnboardingConfig::<T>::get();
            ensure!(
                onboarding.model_registration_enabled,
                Error::<T>::ModelRegistrationDisabled
            );
            ensure!(
                onboarding.public_model_registration_enabled
                    || AuthorizedValidators::<T>::get(&owner),
                Error::<T>::PublicModelRegistrationDisabled
            );
            let model_hash = Self::bounded_model_hash(model_hash)?;
            let metadata_uri = Self::bounded_metadata_uri(metadata_uri)?;
            let model_id = NextModelId::<T>::get();
            let next_model_id = model_id.checked_add(1).ok_or(Error::<T>::Overflow)?;
            let now = frame_system::Pallet::<T>::block_number();

            let model = AiModel {
                owner: owner.clone(),
                model_hash,
                metadata_uri,
                registered_at: now,
                updated_at: None,
                active: true,
            };

            Models::<T>::insert(model_id, model);
            ModelStatsById::<T>::insert(model_id, ModelStats::default());
            ModelGovernanceById::<T>::insert(
                model_id,
                ModelGovernance {
                    status: if onboarding.require_model_approval {
                        ModelStatus::Pending
                    } else {
                        ModelStatus::Approved
                    },
                    authorized_submitter: None,
                    updated_at: now,
                },
            );
            ModelIdsByOwner::<T>::try_mutate(&owner, |ids| ids.try_push(model_id))
                .map_err(|_| Error::<T>::ModelOwnerIndexFull)?;
            NextModelId::<T>::put(next_model_id);

            Self::deposit_event(Event::ModelRegistered { model_id, owner });
            Ok(())
        }

        #[pallet::call_index(1)]
        #[pallet::weight(T::WeightInfo::update_model())]
        pub fn update_model(
            origin: OriginFor<T>,
            model_id: ModelId,
            model_hash: Vec<u8>,
            metadata_uri: Vec<u8>,
        ) -> DispatchResult {
            let owner = ensure_signed(origin)?;
            let model_hash = Self::bounded_model_hash(model_hash)?;
            let metadata_uri = Self::bounded_metadata_uri(metadata_uri)?;
            let now = frame_system::Pallet::<T>::block_number();

            Models::<T>::try_mutate(model_id, |maybe_model| -> DispatchResult {
                let model = maybe_model.as_mut().ok_or(Error::<T>::ModelNotFound)?;
                ensure!(
                    model.owner == owner || AuthorizedValidators::<T>::get(&owner),
                    Error::<T>::NotModelOwner
                );
                let governance =
                    ModelGovernanceById::<T>::get(model_id).ok_or(Error::<T>::ModelNotFound)?;
                ensure!(
                    matches!(
                        governance.status,
                        ModelStatus::Pending | ModelStatus::Approved
                    ),
                    Error::<T>::NotPermitted
                );

                model.model_hash = model_hash;
                model.metadata_uri = metadata_uri;
                model.updated_at = Some(now);

                Ok(())
            })?;

            if !T::FinancialModesAllowed::get() {
                ModelGovernanceById::<T>::mutate(model_id, |value| {
                    if let Some(governance) = value { governance.status = ModelStatus::Pending; }
                });
            }
            Self::deposit_event(Event::ModelUpdated { model_id, owner });
            Ok(())
        }

        #[pallet::call_index(2)]
        #[pallet::weight(T::WeightInfo::submit_prediction())]
        #[allow(
            clippy::too_many_arguments,
            reason = "FRAME dispatchable ABI: preserve the existing SCALE call encoding and metadata"
        )]
        pub fn submit_prediction(
            origin: OriginFor<T>,
            model_id: ModelId,
            domain: PredictionDomain,
            category_code: Vec<u8>,
            prediction_hash: Vec<u8>,
            metadata_uri: Vec<u8>,
            confidence: u8,
            expires_at: BlockNumberFor<T>,
        ) -> DispatchResult {
            ensure!(T::FinancialModesAllowed::get(), Error::<T>::EvaluationSubmissionRequired);
            Self::do_submit_prediction(origin, model_id, domain, category_code, prediction_hash, metadata_uri, confidence, expires_at)
        }

        #[pallet::call_index(3)]
        #[pallet::weight(T::WeightInfo::validate_prediction())]
        pub fn validate_prediction(
            origin: OriginFor<T>,
            prediction_id: PredictionId,
            outcome: PredictionOutcome,
        ) -> DispatchResult {
            ensure!(!EvaluationBindings::<T>::contains_key(prediction_id), Error::<T>::EvaluationEvidenceRequired);
            Self::do_validate_prediction(origin, prediction_id, outcome)
        }

        #[pallet::call_index(4)]
        #[pallet::weight(T::WeightInfo::close_prediction())]
        pub fn close_prediction(
            origin: OriginFor<T>,
            prediction_id: PredictionId,
        ) -> DispatchResult {
            // Evaluation recovery is an evidence-bearing Inconclusive result, not silent closure.
            ensure!(!EvaluationBindings::<T>::contains_key(prediction_id), Error::<T>::EvaluationEvidenceRequired);
            let maybe_who = frame_system::ensure_signed_or_root(origin)?;

            Predictions::<T>::try_mutate(prediction_id, |maybe_prediction| -> DispatchResult {
                let prediction = maybe_prediction
                    .as_mut()
                    .ok_or(Error::<T>::PredictionNotFound)?;

                ensure!(
                    prediction.status != PredictionStatus::Closed,
                    Error::<T>::PredictionAlreadyClosed
                );

                let model =
                    Models::<T>::get(prediction.model_id).ok_or(Error::<T>::ModelNotFound)?;

                let permitted = match maybe_who {
                    None => true,
                    Some(ref who) => {
                        *who == prediction.submitter
                            || *who == model.owner
                            || AuthorizedValidators::<T>::get(who)
                    }
                };

                ensure!(permitted, Error::<T>::NotPermitted);

                if prediction.status == PredictionStatus::Open {
                    ActivePredictionCountByModel::<T>::mutate(prediction.model_id, |count| {
                        *count = count.saturating_sub(1)
                    });
                }
                prediction.status = PredictionStatus::Closed;

                Self::deposit_event(Event::PredictionClosed { prediction_id });
                Ok(())
            })
        }

        #[pallet::call_index(5)]
        #[pallet::weight(T::WeightInfo::authorize_validator())]
        pub fn authorize_validator(origin: OriginFor<T>, account: T::AccountId) -> DispatchResult {
            ensure_root(origin)?;

            AuthorizedValidators::<T>::insert(&account, true);

            Self::deposit_event(Event::ValidatorAuthorized { account });
            Ok(())
        }

        #[pallet::call_index(6)]
        #[pallet::weight(T::WeightInfo::remove_validator())]
        pub fn remove_validator(origin: OriginFor<T>, account: T::AccountId) -> DispatchResult {
            ensure_root(origin)?;

            AuthorizedValidators::<T>::remove(&account);

            Self::deposit_event(Event::ValidatorRemoved { account });
            Ok(())
        }

        #[pallet::call_index(7)]
        #[pallet::weight(T::WeightInfo::set_tokenization_config())]
        pub fn set_tokenization_config(
            origin: OriginFor<T>,
            tokenization_enabled: bool,
            transfers_enabled: bool,
        ) -> DispatchResult {
            ensure!(T::FinancialModesAllowed::get() || !(tokenization_enabled || transfers_enabled), Error::<T>::FinancialModeDisabled);
            ensure_root(origin)?;

            TokenizationConfig::<T>::put(TokenizationSettings {
                tokenization_enabled,
                transfers_enabled,
            });

            Self::deposit_event(Event::TokenizationConfigUpdated {
                tokenization_enabled,
                transfers_enabled,
            });

            Ok(())
        }

        #[pallet::call_index(8)]
        #[pallet::weight(T::WeightInfo::tokenize_prediction())]
        pub fn tokenize_prediction(
            origin: OriginFor<T>,
            prediction_id: PredictionId,
            metadata_uri: Vec<u8>,
            transferable: bool,
        ) -> DispatchResult {
            ensure!(T::FinancialModesAllowed::get(), Error::<T>::FinancialModeDisabled);
            let owner = ensure_signed(origin)?;
            let settings = TokenizationConfig::<T>::get();

            ensure!(
                settings.tokenization_enabled,
                Error::<T>::TokenizationDisabled
            );

            let prediction =
                Predictions::<T>::get(prediction_id).ok_or(Error::<T>::PredictionNotFound)?;

            ensure!(
                prediction.submitter == owner,
                Error::<T>::NotPredictionOwner
            );

            ensure!(
                !PredictionTokenByPrediction::<T>::contains_key(prediction_id),
                Error::<T>::PredictionAlreadyTokenized
            );

            let metadata_uri = Self::bounded_metadata_uri(metadata_uri)?;
            let token_id = NextPredictionTokenId::<T>::get();
            let next_token_id = token_id.checked_add(1).ok_or(Error::<T>::Overflow)?;
            let now = frame_system::Pallet::<T>::block_number();

            let token = PredictionToken {
                prediction_id,
                owner: owner.clone(),
                metadata_uri,
                created_at: now,
                updated_at: None,
                transferable,
                frozen: false,
                burned: false,
            };

            PredictionTokens::<T>::insert(token_id, token);
            PredictionTokenByPrediction::<T>::insert(prediction_id, token_id);
            NextPredictionTokenId::<T>::put(next_token_id);

            Self::deposit_event(Event::PredictionTokenized {
                prediction_id,
                token_id,
                owner,
            });

            Ok(())
        }

        #[pallet::call_index(9)]
        #[pallet::weight(T::WeightInfo::approve_prediction_token_transfer())]
        pub fn approve_prediction_token_transfer(
            origin: OriginFor<T>,
            token_id: PredictionTokenId,
            approved: T::AccountId,
        ) -> DispatchResult {
            ensure!(T::FinancialModesAllowed::get(), Error::<T>::FinancialModeDisabled);
            let owner = ensure_signed(origin)?;
            let token =
                PredictionTokens::<T>::get(token_id).ok_or(Error::<T>::PredictionTokenNotFound)?;

            ensure!(!token.burned, Error::<T>::TokenBurned);
            ensure!(!token.frozen, Error::<T>::TokenFrozen);
            ensure!(token.owner == owner, Error::<T>::NotPredictionTokenOwner);

            PredictionTokenApprovals::<T>::insert(token_id, approved.clone());

            Self::deposit_event(Event::PredictionTokenTransferApproved {
                token_id,
                owner,
                approved,
            });

            Ok(())
        }

        #[pallet::call_index(10)]
        #[pallet::weight(T::WeightInfo::revoke_prediction_token_approval())]
        pub fn revoke_prediction_token_approval(
            origin: OriginFor<T>,
            token_id: PredictionTokenId,
        ) -> DispatchResult {
            let owner = ensure_signed(origin)?;
            let token =
                PredictionTokens::<T>::get(token_id).ok_or(Error::<T>::PredictionTokenNotFound)?;

            ensure!(token.owner == owner, Error::<T>::NotPredictionTokenOwner);

            PredictionTokenApprovals::<T>::remove(token_id);

            Self::deposit_event(Event::PredictionTokenApprovalRevoked { token_id, owner });

            Ok(())
        }

        #[pallet::call_index(11)]
        #[pallet::weight(T::WeightInfo::transfer_prediction_token())]
        pub fn transfer_prediction_token(
            origin: OriginFor<T>,
            token_id: PredictionTokenId,
            to: T::AccountId,
        ) -> DispatchResult {
            ensure!(T::FinancialModesAllowed::get(), Error::<T>::FinancialModeDisabled);
            let sender = ensure_signed(origin)?;
            let settings = TokenizationConfig::<T>::get();
            let now = frame_system::Pallet::<T>::block_number();

            ensure!(settings.transfers_enabled, Error::<T>::TransfersDisabled);
            ensure!(sender != to, Error::<T>::CannotTransferToSelf);

            PredictionTokens::<T>::try_mutate(token_id, |maybe_token| -> DispatchResult {
                let token = maybe_token
                    .as_mut()
                    .ok_or(Error::<T>::PredictionTokenNotFound)?;

                ensure!(!token.burned, Error::<T>::TokenBurned);
                ensure!(!token.frozen, Error::<T>::TokenFrozen);
                ensure!(token.transferable, Error::<T>::TokenNotTransferable);

                let from = token.owner.clone();

                if sender != from {
                    let approved = PredictionTokenApprovals::<T>::get(token_id)
                        .ok_or(Error::<T>::TransferNotApproved)?;
                    ensure!(approved == sender, Error::<T>::TransferNotApproved);
                }

                PredictionTokenApprovals::<T>::remove(token_id);
                token.owner = to.clone();
                token.updated_at = Some(now);

                Self::deposit_event(Event::PredictionTokenTransferred { token_id, from, to });

                Ok(())
            })
        }

        #[pallet::call_index(12)]
        #[pallet::weight(T::WeightInfo::freeze_prediction_token())]
        pub fn freeze_prediction_token(
            origin: OriginFor<T>,
            token_id: PredictionTokenId,
        ) -> DispatchResult {
            ensure_root(origin)?;

            PredictionTokens::<T>::try_mutate(token_id, |maybe_token| -> DispatchResult {
                let token = maybe_token
                    .as_mut()
                    .ok_or(Error::<T>::PredictionTokenNotFound)?;
                ensure!(!token.burned, Error::<T>::TokenBurned);
                token.frozen = true;
                Ok(())
            })?;

            Self::deposit_event(Event::PredictionTokenFrozen { token_id });

            Ok(())
        }

        #[pallet::call_index(13)]
        #[pallet::weight(T::WeightInfo::unfreeze_prediction_token())]
        pub fn unfreeze_prediction_token(
            origin: OriginFor<T>,
            token_id: PredictionTokenId,
        ) -> DispatchResult {
            ensure_root(origin)?;

            PredictionTokens::<T>::try_mutate(token_id, |maybe_token| -> DispatchResult {
                let token = maybe_token
                    .as_mut()
                    .ok_or(Error::<T>::PredictionTokenNotFound)?;
                ensure!(!token.burned, Error::<T>::TokenBurned);
                token.frozen = false;
                Ok(())
            })?;

            Self::deposit_event(Event::PredictionTokenUnfrozen { token_id });

            Ok(())
        }

        #[pallet::call_index(14)]
        #[pallet::weight(T::WeightInfo::burn_prediction_token())]
        pub fn burn_prediction_token(
            origin: OriginFor<T>,
            token_id: PredictionTokenId,
        ) -> DispatchResult {
            let owner = ensure_signed(origin)?;
            let now = frame_system::Pallet::<T>::block_number();

            PredictionTokens::<T>::try_mutate(token_id, |maybe_token| -> DispatchResult {
                let token = maybe_token
                    .as_mut()
                    .ok_or(Error::<T>::PredictionTokenNotFound)?;

                ensure!(token.owner == owner, Error::<T>::NotPredictionTokenOwner);
                ensure!(!token.burned, Error::<T>::TokenBurned);

                token.burned = true;
                token.frozen = true;
                token.updated_at = Some(now);

                Ok(())
            })?;

            PredictionTokenApprovals::<T>::remove(token_id);

            Self::deposit_event(Event::PredictionTokenBurned { token_id });

            Ok(())
        }

        #[pallet::call_index(15)]
        #[pallet::weight(T::WeightInfo::update_prediction_token_metadata())]
        pub fn update_prediction_token_metadata(
            origin: OriginFor<T>,
            token_id: PredictionTokenId,
            metadata_uri: Vec<u8>,
        ) -> DispatchResult {
            let owner = ensure_signed(origin)?;
            let metadata_uri = Self::bounded_metadata_uri(metadata_uri)?;
            let now = frame_system::Pallet::<T>::block_number();

            PredictionTokens::<T>::try_mutate(token_id, |maybe_token| -> DispatchResult {
                let token = maybe_token
                    .as_mut()
                    .ok_or(Error::<T>::PredictionTokenNotFound)?;

                ensure!(token.owner == owner, Error::<T>::NotPredictionTokenOwner);
                ensure!(!token.burned, Error::<T>::TokenBurned);

                token.metadata_uri = metadata_uri;
                token.updated_at = Some(now);

                Ok(())
            })?;

            Self::deposit_event(Event::PredictionTokenMetadataUpdated { token_id, owner });

            Ok(())
        }
        #[pallet::call_index(16)]
        #[pallet::weight(T::WeightInfo::set_staking_config())]
        pub fn set_staking_config(
            origin: OriginFor<T>,
            staking_enabled: bool,
            min_stake: BalanceOf<T>,
        ) -> DispatchResult {
            ensure!(T::FinancialModesAllowed::get() || !(staking_enabled), Error::<T>::FinancialModeDisabled);
            ensure_root(origin)?;

            StakingConfig::<T>::put(StakingSettings {
                staking_enabled,
                min_stake,
            });

            Self::deposit_event(Event::StakingConfigUpdated {
                staking_enabled,
                min_stake,
            });

            Ok(())
        }

        #[pallet::call_index(17)]
        #[pallet::weight(T::WeightInfo::stake_on_prediction_token())]
        pub fn stake_on_prediction_token(
            origin: OriginFor<T>,
            token_id: PredictionTokenId,
            amount: BalanceOf<T>,
        ) -> DispatchResult {
            ensure!(T::FinancialModesAllowed::get(), Error::<T>::FinancialModeDisabled);
            let staker = ensure_signed(origin)?;
            let settings = StakingConfig::<T>::get();
            let now = frame_system::Pallet::<T>::block_number();

            ensure!(settings.staking_enabled, Error::<T>::StakingDisabled);
            ensure!(!amount.is_zero(), Error::<T>::StakeAmountZero);
            ensure!(amount >= settings.min_stake, Error::<T>::StakeBelowMinimum);
            ensure!(
                !PredictionTokenStakeLocked::<T>::get(token_id),
                Error::<T>::StakeLocked
            );

            let token =
                PredictionTokens::<T>::get(token_id).ok_or(Error::<T>::PredictionTokenNotFound)?;
            ensure!(!token.burned, Error::<T>::TokenBurned);
            ensure!(!token.frozen, Error::<T>::TokenFrozen);
            // Admission ends when resolution starts. Existing financial claims stay intact.
            ensure!(!PredictionTokenSettlements::<T>::contains_key(token_id), Error::<T>::SettlementAlreadyStarted);
            let prediction = Predictions::<T>::get(token.prediction_id)
                .ok_or(Error::<T>::PredictionNotFound)?;
            ensure!(prediction.status == PredictionStatus::Open && prediction.expires_at > now,
                Error::<T>::PredictionNotOpen);

            T::Currency::reserve(&staker, amount)?;

            PredictionTokenStakes::<T>::mutate(token_id, &staker, |maybe_stake| {
                if let Some(stake) = maybe_stake {
                    stake.amount = stake.amount.saturating_add(amount);
                    stake.updated_at = Some(now);
                } else {
                    *maybe_stake = Some(PredictionTokenStake {
                        amount,
                        created_at: now,
                        updated_at: None,
                    });
                }
            });

            let total_stake = PredictionTokenTotalStake::<T>::get(token_id).saturating_add(amount);
            PredictionTokenTotalStake::<T>::insert(token_id, total_stake);

            Self::deposit_event(Event::PredictionTokenStakeAdded {
                token_id,
                staker,
                amount,
                total_stake,
            });

            Ok(())
        }

        #[pallet::call_index(18)]
        #[pallet::weight(T::WeightInfo::unstake_prediction_token())]
        pub fn unstake_prediction_token(
            origin: OriginFor<T>,
            token_id: PredictionTokenId,
            amount: BalanceOf<T>,
        ) -> DispatchResult {
            let staker = ensure_signed(origin)?;
            let now = frame_system::Pallet::<T>::block_number();
            let mut remaining_stake = BalanceOf::<T>::zero();

            ensure!(!amount.is_zero(), Error::<T>::StakeAmountZero);
            ensure!(
                !PredictionTokenStakeLocked::<T>::get(token_id),
                Error::<T>::StakeLocked
            );
            ensure!(
                PredictionTokens::<T>::contains_key(token_id),
                Error::<T>::PredictionTokenNotFound
            );
            let token =
                PredictionTokens::<T>::get(token_id).ok_or(Error::<T>::PredictionTokenNotFound)?;
            let prediction =
                Predictions::<T>::get(token.prediction_id).ok_or(Error::<T>::PredictionNotFound)?;
            ensure!(
                prediction.status == PredictionStatus::Open
                    && prediction.expires_at > frame_system::Pallet::<T>::block_number(),
                Error::<T>::PredictionNotOpen
            );

            PredictionTokenStakes::<T>::try_mutate_exists(
                token_id,
                &staker,
                |maybe_stake| -> DispatchResult {
                    let stake = maybe_stake.as_mut().ok_or(Error::<T>::StakeNotFound)?;
                    ensure!(stake.amount >= amount, Error::<T>::InsufficientStake);

                    remaining_stake = stake.amount.saturating_sub(amount);

                    if remaining_stake.is_zero() {
                        *maybe_stake = None;
                    } else {
                        stake.amount = remaining_stake;
                        stake.updated_at = Some(now);
                    }

                    Ok(())
                },
            )?;

            PredictionTokenTotalStake::<T>::mutate(token_id, |total| {
                *total = total.saturating_sub(amount);
            });

            Self::refund_reserved_exact(&staker, amount)?;

            Self::deposit_event(Event::PredictionTokenStakeRemoved {
                token_id,
                staker,
                amount,
                remaining_stake,
            });

            Ok(())
        }

        #[pallet::call_index(19)]
        #[pallet::weight(T::WeightInfo::lock_prediction_token_stake())]
        pub fn lock_prediction_token_stake(
            origin: OriginFor<T>,
            token_id: PredictionTokenId,
        ) -> DispatchResult {
            ensure_root(origin)?;

            ensure!(
                PredictionTokens::<T>::contains_key(token_id),
                Error::<T>::PredictionTokenNotFound
            );

            PredictionTokenStakeLocked::<T>::insert(token_id, true);
            PredictionTokenMarketLocked::<T>::insert(token_id, true);
            Self::deposit_event(Event::PredictionTokenMarketLocked { token_id });

            Self::deposit_event(Event::PredictionTokenStakeLocked { token_id });

            Ok(())
        }

        #[pallet::call_index(20)]
        #[pallet::weight(T::WeightInfo::release_prediction_token_stake())]
        pub fn release_prediction_token_stake(
            origin: OriginFor<T>,
            token_id: PredictionTokenId,
        ) -> DispatchResult {
            ensure_root(origin)?;

            ensure!(
                PredictionTokens::<T>::contains_key(token_id),
                Error::<T>::PredictionTokenNotFound
            );

            PredictionTokenStakeLocked::<T>::insert(token_id, false);

            Self::deposit_event(Event::PredictionTokenStakeReleased { token_id });

            Ok(())
        }
        #[pallet::call_index(21)]
        #[pallet::weight(T::WeightInfo::set_settlement_config())]
        pub fn set_settlement_config(
            origin: OriginFor<T>,
            settlement_enabled: bool,
            dispute_window_blocks: BlockNumberFor<T>,
            min_dispute_bond: BalanceOf<T>,
        ) -> DispatchResult {
            ensure!(T::FinancialModesAllowed::get() || !(settlement_enabled), Error::<T>::FinancialModeDisabled);
            ensure_root(origin)?;

            SettlementConfig::<T>::put(SettlementSettings {
                settlement_enabled,
                dispute_window_blocks,
                min_dispute_bond,
            });

            Self::deposit_event(Event::SettlementConfigUpdated {
                settlement_enabled,
                dispute_window_blocks,
                min_dispute_bond,
            });

            Ok(())
        }

        #[pallet::call_index(22)]
        #[pallet::weight(T::WeightInfo::propose_prediction_outcome())]
        pub fn propose_prediction_outcome(
            origin: OriginFor<T>,
            token_id: PredictionTokenId,
            proposed_outcome: SettlementOutcome,
            evidence_uri: Vec<u8>,
        ) -> DispatchResult {
            ensure!(T::FinancialModesAllowed::get(), Error::<T>::FinancialModeDisabled);
            let maybe_proposer = frame_system::ensure_signed_or_root(origin)?;
            let settings = SettlementConfig::<T>::get();
            let now = frame_system::Pallet::<T>::block_number();

            ensure!(settings.settlement_enabled, Error::<T>::SettlementDisabled);

            if let Some(ref proposer) = maybe_proposer {
                ensure!(
                    AuthorizedValidators::<T>::get(proposer),
                    Error::<T>::NotAuthorizedSettlementProposer
                );
            }

            let token =
                PredictionTokens::<T>::get(token_id).ok_or(Error::<T>::PredictionTokenNotFound)?;
            ensure!(!token.burned, Error::<T>::TokenBurned);

            ensure!(
                !PredictionTokenSettlements::<T>::contains_key(token_id),
                Error::<T>::SettlementAlreadyStarted
            );

            let evidence_uri = Self::bounded_metadata_uri(evidence_uri)?;
            let dispute_until = now.saturating_add(settings.dispute_window_blocks);

            let settlement = PredictionTokenSettlement {
                token_id,
                status: SettlementStatus::DisputeWindowOpen,
                proposed_outcome: Some(proposed_outcome),
                final_outcome: None,
                proposed_by: maybe_proposer.clone(),
                finalized_by: None,
                proposed_at: Some(now),
                dispute_until: Some(dispute_until),
                finalized_at: None,
                evidence_uri: Some(evidence_uri),
                total_stake_at_finalization: BalanceOf::<T>::zero(),
                dispute_count: 0,
            };

            PredictionTokenSettlements::<T>::insert(token_id, settlement);
            PredictionTokenStakeLocked::<T>::insert(token_id, true);
            PredictionTokenMarketLocked::<T>::insert(token_id, true);
            Self::deposit_event(Event::PredictionTokenMarketLocked { token_id });

            Self::deposit_event(Event::PredictionOutcomeProposed {
                token_id,
                proposed_outcome,
                proposed_by: maybe_proposer,
                dispute_until,
            });

            Ok(())
        }

        #[pallet::call_index(23)]
        #[pallet::weight(T::WeightInfo::dispute_prediction_outcome())]
        pub fn dispute_prediction_outcome(
            origin: OriginFor<T>,
            token_id: PredictionTokenId,
            reason: DisputeReason,
            evidence_uri: Vec<u8>,
            bond: BalanceOf<T>,
        ) -> DispatchResult {
            ensure!(T::FinancialModesAllowed::get(), Error::<T>::FinancialModeDisabled);
            let disputed_by = ensure_signed(origin)?;
            let settings = SettlementConfig::<T>::get();
            let now = frame_system::Pallet::<T>::block_number();

            ensure!(settings.settlement_enabled, Error::<T>::SettlementDisabled);
            ensure!(
                bond >= settings.min_dispute_bond,
                Error::<T>::DisputeBondBelowMinimum
            );

            let evidence_uri = Self::bounded_metadata_uri(evidence_uri)?;

            PredictionTokenSettlements::<T>::try_mutate(
                token_id,
                |maybe_settlement| -> DispatchResult {
                    let settlement = maybe_settlement
                        .as_mut()
                        .ok_or(Error::<T>::SettlementNotFound)?;

                    ensure!(
                        settlement.status != SettlementStatus::Finalized,
                        Error::<T>::CannotDisputeFinalizedSettlement
                    );
                    ensure!(
                        settlement.status == SettlementStatus::DisputeWindowOpen,
                        Error::<T>::SettlementNotDisputable
                    );

                    let dispute_until = settlement
                        .dispute_until
                        .ok_or(Error::<T>::SettlementNotDisputable)?;
                    ensure!(now <= dispute_until, Error::<T>::DisputeWindowClosed);

                    settlement.status = SettlementStatus::Disputed;
                    settlement.dispute_count = settlement.dispute_count.saturating_add(1);

                    Ok(())
                },
            )?;

            if !bond.is_zero() {
                T::Currency::reserve(&disputed_by, bond)?;
            }

            let dispute_id = NextDisputeId::<T>::get();
            let next_dispute_id = dispute_id.checked_add(1).ok_or(Error::<T>::Overflow)?;

            let dispute = PredictionTokenDispute {
                dispute_id,
                token_id,
                disputed_by: disputed_by.clone(),
                reason,
                evidence_uri,
                bond,
                submitted_at: now,
                resolved: false,
                accepted: None,
            };

            PredictionTokenDisputes::<T>::insert(dispute_id, dispute);
            NextDisputeId::<T>::put(next_dispute_id);

            Self::deposit_event(Event::PredictionOutcomeDisputed {
                token_id,
                dispute_id,
                disputed_by,
                reason,
                bond,
            });

            Ok(())
        }

        #[pallet::call_index(24)]
        #[pallet::weight(T::WeightInfo::admin_finalize_prediction_outcome())]
        pub fn admin_finalize_prediction_outcome(
            origin: OriginFor<T>,
            token_id: PredictionTokenId,
            final_outcome: SettlementOutcome,
            evidence_uri: Vec<u8>,
        ) -> DispatchResult {
            ensure_root(origin)?;

            ensure!(
                PredictionTokens::<T>::contains_key(token_id),
                Error::<T>::PredictionTokenNotFound
            );

            let evidence_uri = Self::bounded_metadata_uri(evidence_uri)?;
            let now = frame_system::Pallet::<T>::block_number();

            PredictionTokenSettlements::<T>::try_mutate_exists(
                token_id,
                |maybe_settlement| -> DispatchResult {
                    let mut settlement = maybe_settlement
                        .take()
                        .ok_or(Error::<T>::SettlementNotFound)?;

                    ensure!(
                        settlement.status != SettlementStatus::Finalized,
                        Error::<T>::SettlementAlreadyFinalized
                    );

                    // Cancellation promises refunds and may already have paid them.
                    ensure!(settlement.status != SettlementStatus::Cancelled, Error::<T>::SettlementNotFinalizable);
                    settlement.status = SettlementStatus::Finalized;
                    settlement.final_outcome = Some(final_outcome);
                    settlement.finalized_by = None;
                    settlement.finalized_at = Some(now);
                    settlement.evidence_uri = Some(evidence_uri);
                    settlement.total_stake_at_finalization =
                        PredictionTokenTotalStake::<T>::get(token_id);

                    *maybe_settlement = Some(settlement);
                    Ok(())
                },
            )?;

            Self::deposit_event(Event::PredictionOutcomeFinalized {
                token_id,
                final_outcome,
                finalized_by: None,
            });

            Ok(())
        }

        #[pallet::call_index(25)]
        #[pallet::weight(T::WeightInfo::finalize_prediction_outcome_after_dispute_window())]
        pub fn finalize_prediction_outcome_after_dispute_window(
            origin: OriginFor<T>,
            token_id: PredictionTokenId,
        ) -> DispatchResult {
            let maybe_finalizer = frame_system::ensure_signed_or_root(origin)?;
            let now = frame_system::Pallet::<T>::block_number();
            let mut final_outcome = None;

            PredictionTokenSettlements::<T>::try_mutate(
                token_id,
                |maybe_settlement| -> DispatchResult {
                    let settlement = maybe_settlement
                        .as_mut()
                        .ok_or(Error::<T>::SettlementNotFound)?;

                    ensure!(
                        settlement.status == SettlementStatus::DisputeWindowOpen,
                        Error::<T>::SettlementNotFinalizable
                    );

                    let dispute_until = settlement
                        .dispute_until
                        .ok_or(Error::<T>::SettlementNotFinalizable)?;
                    ensure!(now > dispute_until, Error::<T>::DisputeWindowStillOpen);

                    let proposed_outcome = settlement
                        .proposed_outcome
                        .ok_or(Error::<T>::OutcomeNotProposed)?;

                    settlement.status = SettlementStatus::Finalized;
                    settlement.final_outcome = Some(proposed_outcome);
                    settlement.finalized_by = maybe_finalizer.clone();
                    settlement.finalized_at = Some(now);
                    settlement.total_stake_at_finalization =
                        PredictionTokenTotalStake::<T>::get(token_id);

                    final_outcome = Some(proposed_outcome);

                    Ok(())
                },
            )?;

            let final_outcome = final_outcome.ok_or(Error::<T>::OutcomeNotProposed)?;

            Self::deposit_event(Event::PredictionOutcomeFinalized {
                token_id,
                final_outcome,
                finalized_by: maybe_finalizer,
            });

            Ok(())
        }

        #[pallet::call_index(26)]
        #[pallet::weight(T::WeightInfo::cancel_prediction_settlement())]
        pub fn cancel_prediction_settlement(
            origin: OriginFor<T>,
            token_id: PredictionTokenId,
            evidence_uri: Vec<u8>,
        ) -> DispatchResult {
            ensure_root(origin)?;

            ensure!(
                PredictionTokens::<T>::contains_key(token_id),
                Error::<T>::PredictionTokenNotFound
            );

            let evidence_uri = Self::bounded_metadata_uri(evidence_uri)?;
            let now = frame_system::Pallet::<T>::block_number();

            PredictionTokenSettlements::<T>::try_mutate_exists(
                token_id,
                |maybe_settlement| -> DispatchResult {
                    if let Some(existing) = maybe_settlement.as_ref() {
                        ensure!(
                            existing.status != SettlementStatus::Finalized,
                            Error::<T>::SettlementAlreadyFinalized
                        );
                    }

                    let settlement = PredictionTokenSettlement {
                        token_id,
                        status: SettlementStatus::Cancelled,
                        proposed_outcome: None,
                        final_outcome: Some(SettlementOutcome::Cancelled),
                        proposed_by: None,
                        finalized_by: None,
                        proposed_at: None,
                        dispute_until: None,
                        finalized_at: Some(now),
                        evidence_uri: Some(evidence_uri),
                        total_stake_at_finalization: PredictionTokenTotalStake::<T>::get(token_id),
                        dispute_count: maybe_settlement
                            .as_ref()
                            .map(|existing| existing.dispute_count)
                            .unwrap_or(0),
                    };

                    *maybe_settlement = Some(settlement);

                    Ok(())
                },
            )?;

            PredictionTokenStakeLocked::<T>::insert(token_id, true);
            PredictionTokenMarketLocked::<T>::insert(token_id, true);

            Self::deposit_event(Event::PredictionSettlementCancelled { token_id });

            Ok(())
        }

        #[pallet::call_index(27)]
        #[pallet::weight(T::WeightInfo::set_settlement_economics_config())]
        pub fn set_settlement_economics_config(
            origin: OriginFor<T>,
            economics_enabled: bool,
            slash_incorrect: bool,
            slash_fraudulent: bool,
        ) -> DispatchResult {
            ensure!(T::FinancialModesAllowed::get() || !(economics_enabled || slash_incorrect || slash_fraudulent), Error::<T>::FinancialModeDisabled);
            ensure_root(origin)?;

            let config = SettlementEconomicsSettings {
                economics_enabled,
                slash_incorrect,
                slash_fraudulent,
            };

            SettlementEconomicsConfig::<T>::put(config);

            Self::deposit_event(Event::SettlementEconomicsConfigUpdated {
                economics_enabled,
                slash_incorrect,
                slash_fraudulent,
            });

            Ok(())
        }

        #[pallet::call_index(28)]
        #[pallet::weight(T::WeightInfo::claim_prediction_token_settlement())]
        pub fn claim_prediction_token_settlement(
            origin: OriginFor<T>,
            token_id: PredictionTokenId,
        ) -> DispatchResult {
            let claimer = ensure_signed(origin)?;

            let economics_config = SettlementEconomicsConfig::<T>::get();
            ensure!(
                economics_config.economics_enabled || !T::FinancialModesAllowed::get(),
                Error::<T>::SettlementEconomicsDisabled
            );

            ensure!(
                !PredictionTokenSettlementClaims::<T>::contains_key(token_id, &claimer),
                Error::<T>::SettlementAlreadyClaimed
            );

            let settlement = PredictionTokenSettlements::<T>::get(token_id)
                .ok_or(Error::<T>::SettlementNotFound)?;

            ensure!(
                matches!(
                    settlement.status,
                    SettlementStatus::Finalized | SettlementStatus::Cancelled
                ),
                Error::<T>::SettlementNotFinalized
            );

            let final_outcome = if settlement.status == SettlementStatus::Cancelled {
                SettlementOutcome::Cancelled
            } else {
                settlement
                    .final_outcome
                    .ok_or(Error::<T>::SettlementClaimNotAvailable)?
            };

            let stake = PredictionTokenStakes::<T>::get(token_id, &claimer)
                .ok_or(Error::<T>::StakeNotFound)?;

            let now = frame_system::Pallet::<T>::block_number();
            let amount = stake.amount;

            let mut refunded = BalanceOf::<T>::zero();
            let mut slashed = BalanceOf::<T>::zero();

            match final_outcome {
                SettlementOutcome::Correct
                | SettlementOutcome::Inconclusive
                | SettlementOutcome::Cancelled => {
                    Self::refund_reserved_exact(&claimer, amount)?;
                    refunded = amount;
                }
                SettlementOutcome::Incorrect => {
                    if economics_config.slash_incorrect {
                        Self::transfer_reserved_penalty(&claimer, amount, 0, token_id)?;
                        slashed = amount;
                    } else {
                        Self::refund_reserved_exact(&claimer, amount)?;
                        refunded = amount;
                    }
                }
                SettlementOutcome::Fraudulent => {
                    if economics_config.slash_fraudulent {
                        Self::transfer_reserved_penalty(&claimer, amount, 1, token_id)?;
                        slashed = amount;
                    } else {
                        Self::refund_reserved_exact(&claimer, amount)?;
                        refunded = amount;
                    }
                }
            }

            PredictionTokenStakes::<T>::remove(token_id, &claimer);

            let total = PredictionTokenTotalStake::<T>::get(token_id);
            ensure!(total >= amount, Error::<T>::ReserveInvariant);
            let new_total = total - amount;
            PredictionTokenTotalStake::<T>::insert(token_id, new_total);

            if new_total.is_zero() {
                PredictionTokenStakeLocked::<T>::insert(token_id, false);
            }

            let claim = PredictionTokenSettlementClaim {
                claimed_at: now,
                refunded,
                slashed,
            };

            PredictionTokenSettlementClaims::<T>::insert(token_id, &claimer, claim);

            Self::deposit_event(Event::PredictionTokenSettlementClaimed {
                token_id,
                staker: claimer,
                refunded,
                slashed,
            });

            Ok(())
        }

        #[pallet::call_index(29)]
        #[pallet::weight(T::WeightInfo::resolve_prediction_dispute_bond())]
        pub fn resolve_prediction_dispute_bond(
            origin: OriginFor<T>,
            dispute_id: DisputeId,
            accepted: bool,
        ) -> DispatchResult {
            ensure_root(origin)?;

            Self::resolve_bond(dispute_id, accepted, false, [0;32])
        }

        #[pallet::call_index(30)]
        #[pallet::weight(T::WeightInfo::set_market_economics_config())]
        pub fn set_market_economics_config(
            origin: OriginFor<T>,
            market_enabled: bool,
            allow_unstake_before_settlement: bool,
        ) -> DispatchResult {
            ensure!(T::FinancialModesAllowed::get() || !(market_enabled), Error::<T>::FinancialModeDisabled);
            ensure_root(origin)?;

            PredictionMarketEconomicsConfig::<T>::mutate(|config| {
                config.market_enabled = market_enabled;
                config.allow_unstake_before_settlement = allow_unstake_before_settlement;
            });

            Self::deposit_event(Event::PredictionMarketEconomicsConfigUpdated {
                market_enabled,
                allow_unstake_before_settlement,
            });

            Ok(())
        }

        #[pallet::call_index(31)]
        #[pallet::weight(T::WeightInfo::stake_on_prediction_outcome_side())]
        pub fn stake_on_prediction_outcome_side(
            origin: OriginFor<T>,
            token_id: PredictionTokenId,
            side: PredictionOutcomeSide,
            amount: BalanceOf<T>,
        ) -> DispatchResult {
            ensure!(T::FinancialModesAllowed::get(), Error::<T>::FinancialModeDisabled);
            let staker = ensure_signed(origin)?;

            let market_config = PredictionMarketEconomicsConfig::<T>::get();
            ensure!(
                market_config.market_enabled,
                Error::<T>::MarketEconomicsDisabled
            );

            let staking_config = StakingConfig::<T>::get();
            ensure!(staking_config.staking_enabled, Error::<T>::StakingDisabled);
            ensure!(!amount.is_zero(), Error::<T>::MarketStakeAmountZero);
            ensure!(
                amount >= staking_config.min_stake,
                Error::<T>::StakeBelowMinimum
            );

            ensure!(
                PredictionTokens::<T>::contains_key(token_id),
                Error::<T>::PredictionTokenNotFound
            );

            let token =
                PredictionTokens::<T>::get(token_id).ok_or(Error::<T>::PredictionTokenNotFound)?;
            ensure!(!token.burned, Error::<T>::TokenBurned);
            ensure!(!token.frozen, Error::<T>::TokenFrozen);
            ensure!(!PredictionTokenSettlements::<T>::contains_key(token_id), Error::<T>::SettlementAlreadyStarted);
            let prediction =
                Predictions::<T>::get(token.prediction_id).ok_or(Error::<T>::PredictionNotFound)?;
            ensure!(
                prediction.status == PredictionStatus::Open
                    && prediction.expires_at > frame_system::Pallet::<T>::block_number(),
                Error::<T>::PredictionNotOpen
            );

            ensure!(
                !PredictionTokenMarketLocked::<T>::get(token_id),
                Error::<T>::MarketAlreadyLocked
            );

            if let Some(existing_stake) = PredictionTokenSideStakes::<T>::get(token_id, &staker) {
                ensure!(
                    existing_stake.side == side,
                    Error::<T>::MarketStakeSideMismatch
                );
            }

            T::Currency::transfer(
                &staker,
                &Self::market_account_id(),
                amount,
                frame_support::traits::ExistenceRequirement::KeepAlive,
            )?;

            let now = frame_system::Pallet::<T>::block_number();

            PredictionTokenSideStakes::<T>::mutate(token_id, &staker, |maybe_stake| {
                if let Some(existing_stake) = maybe_stake.as_mut() {
                    existing_stake.amount = existing_stake.amount.saturating_add(amount);
                    existing_stake.updated_at = Some(now);
                } else {
                    *maybe_stake = Some(PredictionSideStake {
                        side: side.clone(),
                        amount,
                        staked_at: now,
                        updated_at: None,
                    });
                }
            });

            let total_for_side =
                PredictionTokenSideTotals::<T>::get(token_id, side.clone()).saturating_add(amount);
            PredictionTokenSideTotals::<T>::insert(token_id, side.clone(), total_for_side);

            Self::deposit_event(Event::PredictionOutcomeSideStaked {
                token_id,
                staker,
                side,
                amount,
                total_for_side,
            });

            Ok(())
        }

        #[pallet::call_index(32)]
        #[pallet::weight(T::WeightInfo::unstake_from_prediction_outcome_side())]
        pub fn unstake_from_prediction_outcome_side(
            origin: OriginFor<T>,
            token_id: PredictionTokenId,
            side: PredictionOutcomeSide,
            amount: BalanceOf<T>,
        ) -> DispatchResult {
            let staker = ensure_signed(origin)?;

            let market_config = PredictionMarketEconomicsConfig::<T>::get();
            ensure!(
                market_config.market_enabled || !T::FinancialModesAllowed::get(),
                Error::<T>::MarketEconomicsDisabled
            );
            ensure!(
                market_config.allow_unstake_before_settlement,
                Error::<T>::MarketUnstakeDisabled
            );
            ensure!(!amount.is_zero(), Error::<T>::MarketStakeAmountZero);

            ensure!(
                !PredictionTokenMarketLocked::<T>::get(token_id),
                Error::<T>::MarketAlreadyLocked
            );

            let existing_stake = PredictionTokenSideStakes::<T>::get(token_id, &staker)
                .ok_or(Error::<T>::MarketStakeNotFound)?;

            ensure!(
                existing_stake.side == side,
                Error::<T>::MarketStakeSideMismatch
            );
            ensure!(
                existing_stake.amount >= amount,
                Error::<T>::InsufficientStake
            );

            let remaining_stake = existing_stake.amount.saturating_sub(amount);

            T::Currency::transfer(
                &Self::market_account_id(),
                &staker,
                amount,
                frame_support::traits::ExistenceRequirement::AllowDeath,
            )?;

            if remaining_stake.is_zero() {
                PredictionTokenSideStakes::<T>::remove(token_id, &staker);
            } else {
                let updated_stake = PredictionSideStake {
                    side: side.clone(),
                    amount: remaining_stake,
                    staked_at: existing_stake.staked_at,
                    updated_at: Some(frame_system::Pallet::<T>::block_number()),
                };

                PredictionTokenSideStakes::<T>::insert(token_id, &staker, updated_stake);
            }

            let updated_total =
                PredictionTokenSideTotals::<T>::get(token_id, side.clone()).saturating_sub(amount);
            PredictionTokenSideTotals::<T>::insert(token_id, side.clone(), updated_total);

            Self::deposit_event(Event::PredictionOutcomeSideUnstaked {
                token_id,
                staker,
                side,
                amount,
                remaining_stake,
            });

            Ok(())
        }

        #[pallet::call_index(33)]
        #[pallet::weight(T::WeightInfo::claim_prediction_market_payout())]
        pub fn claim_prediction_market_payout(
            origin: OriginFor<T>,
            token_id: PredictionTokenId,
        ) -> DispatchResult {
            let claimer = ensure_signed(origin)?;

            let market_config = PredictionMarketEconomicsConfig::<T>::get();
            ensure!(
                market_config.market_enabled || !T::FinancialModesAllowed::get(),
                Error::<T>::MarketEconomicsDisabled
            );

            ensure!(
                !PredictionTokenSidePayoutClaims::<T>::contains_key(token_id, &claimer),
                Error::<T>::MarketPayoutAlreadyClaimed
            );

            let settlement = PredictionTokenSettlements::<T>::get(token_id)
                .ok_or(Error::<T>::SettlementNotFound)?;

            ensure!(
                matches!(
                    settlement.status,
                    SettlementStatus::Finalized | SettlementStatus::Cancelled
                ),
                Error::<T>::SettlementNotFinalized
            );

            let stake = PredictionTokenSideStakes::<T>::get(token_id, &claimer)
                .ok_or(Error::<T>::MarketStakeNotFound)?;

            let final_outcome = if settlement.status == SettlementStatus::Cancelled {
                SettlementOutcome::Cancelled
            } else {
                settlement
                    .final_outcome
                    .ok_or(Error::<T>::MarketPayoutNotAvailable)?
            };

            let no_winner = Self::winning_market_side(&final_outcome)
                .map(|side| PredictionTokenSideTotals::<T>::get(token_id, side).is_zero())
                .unwrap_or(false);
            let final_outcome = if no_winner {
                Self::deposit_event(Event::PredictionMarketNoWinnerRefund { token_id, staker: claimer.clone(), amount: stake.amount });
                SettlementOutcome::Inconclusive
            } else { final_outcome };

            let is_refund_outcome = matches!(
                &final_outcome,
                SettlementOutcome::Cancelled
                    | SettlementOutcome::Inconclusive
                    | SettlementOutcome::Fraudulent
            );

            let now = frame_system::Pallet::<T>::block_number();

            let mut stake_returned = BalanceOf::<T>::zero();
            let mut reward_paid = BalanceOf::<T>::zero();
            let mut slashed = BalanceOf::<T>::zero();
            let mut fee_collected = BalanceOf::<T>::zero();
            let mut remainder_collected = BalanceOf::<T>::zero();

            match final_outcome {
                SettlementOutcome::Cancelled
                | SettlementOutcome::Inconclusive
                | SettlementOutcome::Fraudulent => {
                    stake_returned = stake.amount;

                    T::Currency::transfer(
                        &Self::market_account_id(),
                        &claimer,
                        stake_returned,
                        frame_support::traits::ExistenceRequirement::AllowDeath,
                    )?;
                }
                SettlementOutcome::Correct | SettlementOutcome::Incorrect => {
                    let winning_side = Self::winning_market_side(&final_outcome)
                        .ok_or(Error::<T>::InvalidMarketOutcome)?;

                    let winning_total =
                        PredictionTokenSideTotals::<T>::get(token_id, winning_side.clone());

                    ensure!(!winning_total.is_zero(), Error::<T>::MarketNoWinningStake);

                    if stake.side == winning_side {
                        let losing_side = Self::opposite_market_side(&winning_side);
                        let losing_total =
                            PredictionTokenSideTotals::<T>::get(token_id, losing_side);

                        stake_returned = stake.amount;
                        let stake_amount_u128: u128 = stake.amount.saturated_into();
                        let winning_total_u128: u128 = winning_total.saturated_into();
                        let losing_total_u128: u128 = losing_total.saturated_into();

                        let fee_total_u128 = if market_config.fees_enabled
                            && market_config.treasury_enabled
                            && market_config.fee_bps > 0
                        {
                            losing_total_u128.saturating_mul(market_config.fee_bps as u128)
                                / 10_000u128
                        } else {
                            0u128
                        };

                        let distributable_losing_total_u128 =
                            losing_total_u128.saturating_sub(fee_total_u128);

                        let winner_share =
                            Perquintill::from_rational(stake_amount_u128, winning_total_u128);

                        let gross_reward_paid_u128 = winner_share * losing_total_u128;
                        let reward_paid_u128 = winner_share * distributable_losing_total_u128;
                        let fee_collected_u128 = winner_share * fee_total_u128;
                        let remainder_collected_u128 = gross_reward_paid_u128
                            .saturating_sub(reward_paid_u128)
                            .saturating_sub(fee_collected_u128);

                        reward_paid = reward_paid_u128.saturated_into();
                        fee_collected = fee_collected_u128.saturated_into();
                        remainder_collected = remainder_collected_u128.saturated_into();

                        let is_last_winning_claim =
                            PredictionTokenSideStakes::<T>::iter_prefix(token_id)
                                .filter(|item| {
                                    let (account, existing_stake) = item;
                                    existing_stake.side == winning_side && account != &claimer
                                })
                                .count()
                                == 0;

                        let treasury_transfer = fee_collected.saturating_add(remainder_collected);

                        if !treasury_transfer.is_zero() {
                            T::Currency::transfer(
                                &Self::market_account_id(),
                                &Self::market_treasury_account_id(),
                                treasury_transfer,
                                frame_support::traits::ExistenceRequirement::AllowDeath,
                            )?;
                        }

                        let payout = stake_returned.saturating_add(reward_paid);

                        T::Currency::transfer(
                            &Self::market_account_id(),
                            &claimer,
                            payout,
                            frame_support::traits::ExistenceRequirement::AllowDeath,
                        )?;

                        if is_last_winning_claim && market_config.treasury_enabled {
                            let accounting = PredictionTokenMarketAccounting::<T>::get(token_id);

                            let cumulative_paid_after_claim =
                                accounting.total_paid.saturating_add(payout);

                            let cumulative_reward_paid =
                                cumulative_paid_after_claim.saturating_sub(winning_total);

                            let expected_treasury_total =
                                losing_total.saturating_sub(cumulative_reward_paid);

                            let already_accounted_treasury = accounting
                                .fees_collected
                                .saturating_add(accounting.remainders_collected)
                                .saturating_add(fee_collected)
                                .saturating_add(remainder_collected);

                            if expected_treasury_total > already_accounted_treasury {
                                let final_remainder_transfer = expected_treasury_total
                                    .saturating_sub(already_accounted_treasury);

                                T::Currency::transfer(
                                    &Self::market_account_id(),
                                    &Self::market_treasury_account_id(),
                                    final_remainder_transfer,
                                    frame_support::traits::ExistenceRequirement::AllowDeath,
                                )?;

                                remainder_collected =
                                    remainder_collected.saturating_add(final_remainder_transfer);
                            }
                        }
                    } else {
                        slashed = stake.amount;
                    }
                }
            }

            let total_paid = if is_refund_outcome {
                BalanceOf::<T>::zero()
            } else {
                stake_returned.saturating_add(reward_paid)
            };

            let total_refunded = if is_refund_outcome {
                stake_returned
            } else {
                BalanceOf::<T>::zero()
            };

            PredictionTokenMarketAccounting::<T>::mutate(token_id, |accounting| {
                accounting.fees_collected = accounting.fees_collected.saturating_add(fee_collected);
                accounting.remainders_collected = accounting
                    .remainders_collected
                    .saturating_add(remainder_collected);
                accounting.total_paid = accounting.total_paid.saturating_add(total_paid);
                accounting.total_refunded =
                    accounting.total_refunded.saturating_add(total_refunded);
                accounting.total_slashed = accounting.total_slashed.saturating_add(slashed);
            });

            let accounting = PredictionTokenMarketAccounting::<T>::get(token_id);

            Self::deposit_event(Event::PredictionMarketAccountingUpdated {
                token_id,
                fees_collected: accounting.fees_collected,
                remainders_collected: accounting.remainders_collected,
                total_paid: accounting.total_paid,
                total_refunded: accounting.total_refunded,
                total_slashed: accounting.total_slashed,
            });

            PredictionTokenSideStakes::<T>::remove(token_id, &claimer);

            let claim = PredictionSidePayoutClaim {
                claimed_at: now,
                stake_returned,
                reward_paid,
                slashed,
            };

            PredictionTokenSidePayoutClaims::<T>::insert(token_id, &claimer, claim);

            Self::deposit_event(Event::PredictionMarketPayoutClaimed {
                token_id,
                staker: claimer,
                side: stake.side,
                stake_returned,
                reward_paid,
                slashed,
            });

            Ok(())
        }
        #[pallet::call_index(34)]
        #[pallet::weight(T::WeightInfo::set_market_fee_config())]
        pub fn set_market_fee_config(
            origin: OriginFor<T>,
            fees_enabled: bool,
            fee_bps: u16,
            treasury_enabled: bool,
        ) -> DispatchResult {
            ensure!(T::FinancialModesAllowed::get() || !(fees_enabled), Error::<T>::FinancialModeDisabled);
            ensure_root(origin)?;
            ensure!(fee_bps <= 10_000, Error::<T>::MarketFeeBpsTooHigh);

            PredictionMarketEconomicsConfig::<T>::mutate(|config| {
                config.fees_enabled = fees_enabled;
                config.fee_bps = fee_bps;
                config.treasury_enabled = treasury_enabled;
            });

            Self::deposit_event(Event::PredictionMarketFeeConfigUpdated {
                fees_enabled,
                fee_bps,
                treasury_enabled,
            });

            Ok(())
        }
        #[pallet::call_index(35)]
        #[pallet::weight(T::WeightInfo::update_model())]
        pub fn set_model_onboarding_config(
            origin: OriginFor<T>,
            config: ModelOnboardingSettings,
        ) -> DispatchResult {
            ensure_root(origin)?;
            ensure!(
                config.max_active_predictions_per_model > 0,
                Error::<T>::TooManyActivePredictions
            );
            ModelOnboardingConfig::<T>::put(config.clone());
            Self::deposit_event(Event::ModelOnboardingConfigUpdated { config });
            Ok(())
        }

        #[pallet::call_index(36)]
        #[pallet::weight(T::WeightInfo::approve_model())]
        pub fn approve_model(origin: OriginFor<T>, model_id: ModelId) -> DispatchResult {
            Self::ensure_model_admin(origin)?;
            Self::set_model_status(model_id, ModelStatus::Approved)?;
            Self::deposit_event(Event::ModelApproved { model_id });
            Ok(())
        }

        #[pallet::call_index(37)]
        #[pallet::weight(T::WeightInfo::suspend_model())]
        pub fn suspend_model(origin: OriginFor<T>, model_id: ModelId) -> DispatchResult {
            Self::ensure_model_admin(origin)?;
            Self::set_model_status(model_id, ModelStatus::Suspended)?;
            Self::deposit_event(Event::ModelSuspended { model_id });
            Ok(())
        }

        #[pallet::call_index(38)]
        #[pallet::weight(T::WeightInfo::reject_model())]
        pub fn reject_model(origin: OriginFor<T>, model_id: ModelId) -> DispatchResult {
            Self::ensure_model_admin(origin)?;
            Self::set_model_status(model_id, ModelStatus::Rejected)?;
            Self::deposit_event(Event::ModelRejected { model_id });
            Ok(())
        }

        #[pallet::call_index(39)]
        #[pallet::weight(T::WeightInfo::set_model_submitter())]
        pub fn set_model_submitter(
            origin: OriginFor<T>,
            model_id: ModelId,
            submitter: Option<T::AccountId>,
        ) -> DispatchResult {
            let maybe_who = frame_system::ensure_signed_or_root(origin)?;
            let model = Models::<T>::get(model_id).ok_or(Error::<T>::ModelNotFound)?;
            if let Some(who) = maybe_who {
                ensure!(
                    who == model.owner || AuthorizedValidators::<T>::get(&who),
                    Error::<T>::NotPermitted
                );
            }
            ModelGovernanceById::<T>::try_mutate(model_id, |value| -> DispatchResult {
                let governance = value.as_mut().ok_or(Error::<T>::ModelNotFound)?;
                governance.authorized_submitter = submitter.clone();
                governance.updated_at = frame_system::Pallet::<T>::block_number();
                Ok(())
            })?;
            Self::deposit_event(Event::ModelSubmitterUpdated {
                model_id,
                submitter,
            });
            Ok(())
        }
        /// Explicit fraud finding; ordinary unsuccessful disputes are refunded by call29.
        #[pallet::call_index(40)]
        #[pallet::weight(Weight::MAX)]
        pub fn adjudicate_prediction_dispute_bond(
            origin: OriginFor<T>, dispute_id: DisputeId, accepted: bool,
            fraudulent: bool, evidence_hash: [u8;32],
        ) -> DispatchResult {
            ensure!(T::FinancialModesAllowed::get() || !fraudulent, Error::<T>::FinancialModeDisabled);
            ensure_root(origin)?;
            ensure!(!(accepted && fraudulent) && evidence_hash != [0;32], Error::<T>::InvalidBondAdjudication);
            Self::resolve_bond(dispute_id, accepted, fraudulent, evidence_hash)
        }
        /// The existing prediction hash commits canonical model/version/input/horizon/terms.
        /// A duplicate exact business request succeeds without a second prediction.
        #[pallet::call_index(41)]
        #[pallet::weight(T::WeightInfo::submit_evaluation())]
        #[frame_support::transactional]
        pub fn submit_evaluation(
            origin: OriginFor<T>, model_id: ModelId, request_id: [u8;32],
            artifact: [u8;32], prediction_hash: [u8;32], metadata_uri: Vec<u8>,
            confidence: u8, expires_at: BlockNumberFor<T>, cutoff_utc: u64, evidence_after_utc: u64,
        ) -> DispatchResult {
            let who = ensure_signed(origin.clone())?;
            ensure!(request_id != [0;32] && prediction_hash != [0;32] && artifact != [0;32], Error::<T>::InvalidPredictionHash);
            if let Some(id) = EvaluationRequests::<T>::get(model_id, request_id) {
                let old = Predictions::<T>::get(id).ok_or(Error::<T>::PredictionNotFound)?;
                ensure!(old.submitter == who && old.prediction_hash.as_slice() == prediction_hash && old.metadata_uri.as_slice() == metadata_uri && old.confidence == confidence && old.expires_at == expires_at && EvaluationBindings::<T>::get(id) == Some(artifact) && EvaluationTiming::<T>::get(id) == Some((cutoff_utc, evidence_after_utc)), Error::<T>::EvaluationRequestConflict);
                return Ok(());
            }
            ensure!(T::NowSeconds::get() < cutoff_utc && cutoff_utc < evidence_after_utc, Error::<T>::EvaluationTooEarly);
            let model = Models::<T>::get(model_id).ok_or(Error::<T>::ModelNotFound)?;
            ensure!(model.model_hash.as_slice() == artifact, Error::<T>::EvaluationArtifactChanged);
            ensure!(ModelGovernanceById::<T>::get(model_id).map(|g| g.status == ModelStatus::Approved).unwrap_or(false), Error::<T>::ModelNotApproved);
            let id = NextPredictionId::<T>::get();
            Self::do_submit_prediction(origin, model_id, PredictionDomain::WeatherClimate, b"station-temperature-v1".to_vec(), prediction_hash.to_vec(), metadata_uri, confidence, expires_at)?;
            EvaluationBindings::<T>::insert(id, artifact);
            EvaluationTiming::<T>::insert(id, (cutoff_utc, evidence_after_utc));
            EvaluationRequests::<T>::insert(model_id, request_id, id);
            Self::deposit_event(Event::EvaluationRecorded { prediction_id: id, request_id, artifact });
            Ok(())
        }

        /// Authorized independent account publishes evidence; corrections append, never erase.
        #[pallet::call_index(42)]
        #[pallet::weight(T::WeightInfo::record_evaluation_evidence())]
        #[frame_support::transactional]
        pub fn record_evaluation_evidence(origin: OriginFor<T>, prediction_id: PredictionId, revision: u32, evidence: [u8;32], outcome: PredictionOutcome) -> DispatchResult {
            let who = ensure_signed(origin.clone())?;
            ensure!(AuthorizedValidators::<T>::get(&who), Error::<T>::NotAuthorizedValidator);
            ensure!(EvaluationBindings::<T>::contains_key(prediction_id) && evidence != [0;32], Error::<T>::EvaluationEvidenceRequired);
            let prediction = Predictions::<T>::get(prediction_id).ok_or(Error::<T>::PredictionNotFound)?;
            let model = Models::<T>::get(prediction.model_id).ok_or(Error::<T>::ModelNotFound)?;
            ensure!(who != prediction.submitter && who != model.owner, Error::<T>::EvaluationReviewerConflict);
            let (_, evidence_after) = EvaluationTiming::<T>::get(prediction_id).ok_or(Error::<T>::EvaluationEvidenceRequired)?;
            ensure!(T::NowSeconds::get() >= evidence_after, Error::<T>::EvaluationTooEarly);
            let count = EvaluationRevisionCount::<T>::get(prediction_id);
            ensure!(revision == count && count < 32, Error::<T>::EvaluationRevisionConflict);
            if count == 0 { Self::do_validate_prediction(origin, prediction_id, outcome)?; }
            EvaluationEvidence::<T>::insert(prediction_id, revision, (who, evidence, outcome, frame_system::Pallet::<T>::block_number()));
            EvaluationRevisionCount::<T>::insert(prediction_id, count + 1);
            Self::deposit_event(Event::EvaluationEvidenceRecorded { prediction_id, revision, evidence, outcome });
            Ok(())
        }
    }

    impl<T: Config> Pallet<T> {
        fn do_submit_prediction(
            origin: OriginFor<T>,
            model_id: ModelId,
            domain: PredictionDomain,
            category_code: Vec<u8>,
            prediction_hash: Vec<u8>,
            metadata_uri: Vec<u8>,
            confidence: u8,
            expires_at: BlockNumberFor<T>,
        ) -> DispatchResult {
            let submitter = ensure_signed(origin)?;
            let onboarding = ModelOnboardingConfig::<T>::get();
            ensure!(
                onboarding.prediction_submission_enabled,
                Error::<T>::PredictionSubmissionDisabled
            );
            ensure!(
                onboarding.market_creation_enabled || !T::FinancialModesAllowed::get(),
                Error::<T>::MarketCreationDisabled
            );
            ensure!(confidence <= 100, Error::<T>::InvalidConfidence);

            let model = Models::<T>::get(model_id).ok_or(Error::<T>::ModelNotFound)?;
            ensure!(model.active, Error::<T>::ModelInactive);
            let governance =
                ModelGovernanceById::<T>::get(model_id).ok_or(Error::<T>::ModelNotApproved)?;
            match governance.status {
                ModelStatus::Pending => ensure!(
                    !onboarding.require_model_approval,
                    Error::<T>::ModelNotApproved
                ),
                ModelStatus::Approved => {}
                ModelStatus::Suspended => return Err(Error::<T>::ModelSuspended.into()),
                ModelStatus::Rejected => return Err(Error::<T>::ModelRejected.into()),
            }
            ensure!(
                submitter == model.owner
                    || governance.authorized_submitter.as_ref() == Some(&submitter),
                Error::<T>::NotAuthorizedModelSubmitter
            );
            ensure!(
                ActivePredictionCountByModel::<T>::get(model_id)
                    < onboarding.max_active_predictions_per_model,
                Error::<T>::TooManyActivePredictions
            );

            let now = frame_system::Pallet::<T>::block_number();
            ensure!(expires_at > now, Error::<T>::PredictionExpired);

            let category_code = Self::bounded_category_code(category_code)?;
            let prediction_hash = Self::bounded_prediction_hash(prediction_hash)?;
            let metadata_uri = Self::bounded_metadata_uri(metadata_uri)?;

            let prediction_id = NextPredictionId::<T>::get();
            let next_prediction_id = prediction_id.checked_add(1).ok_or(Error::<T>::Overflow)?;

            ModelStatsById::<T>::try_mutate(model_id, |maybe_stats| -> DispatchResult {
                let stats = maybe_stats.as_mut().ok_or(Error::<T>::ModelNotFound)?;
                stats.total_predictions = stats
                    .total_predictions
                    .checked_add(1)
                    .ok_or(Error::<T>::Overflow)?;
                Ok(())
            })?;

            let prediction = Prediction {
                model_id,
                submitter: submitter.clone(),
                domain,
                category_code,
                prediction_hash,
                metadata_uri,
                confidence,
                created_at: now,
                expires_at,
                status: PredictionStatus::Open,
                outcome: None,
                validator: None,
                validated_at: None,
            };

            Predictions::<T>::insert(prediction_id, prediction);
            PredictionIdsByModel::<T>::try_mutate(model_id, |ids| ids.try_push(prediction_id))
                .map_err(|_| Error::<T>::PredictionModelIndexFull)?;
            ActivePredictionCountByModel::<T>::mutate(model_id, |count| {
                *count = count.saturating_add(1)
            });
            NextPredictionId::<T>::put(next_prediction_id);

            Self::deposit_event(Event::PredictionSubmitted {
                prediction_id,
                model_id,
                submitter,
                domain,
            });
            Self::deposit_event(Event::PredictionOpened {
                prediction_id,
                model_id,
            });

            Ok(())
        }
        fn do_validate_prediction(
            origin: OriginFor<T>,
            prediction_id: PredictionId,
            outcome: PredictionOutcome,
        ) -> DispatchResult {
            let validator = ensure_signed(origin)?;
            ensure!(
                AuthorizedValidators::<T>::get(&validator),
                Error::<T>::NotAuthorizedValidator
            );

            let now = frame_system::Pallet::<T>::block_number();

            Predictions::<T>::try_mutate(prediction_id, |maybe_prediction| -> DispatchResult {
                let prediction = maybe_prediction
                    .as_mut()
                    .ok_or(Error::<T>::PredictionNotFound)?;

                ensure!(
                    prediction.status == PredictionStatus::Open,
                    Error::<T>::PredictionAlreadyValidated
                );
                ensure!(
                    prediction.outcome.is_none(),
                    Error::<T>::PredictionAlreadyValidated
                );

                ModelStatsById::<T>::try_mutate(
                    prediction.model_id,
                    |maybe_stats| -> DispatchResult {
                        let stats = maybe_stats.as_mut().ok_or(Error::<T>::ModelNotFound)?;

                        stats.validated_predictions = stats
                            .validated_predictions
                            .checked_add(1)
                            .ok_or(Error::<T>::Overflow)?;

                        match outcome {
                            PredictionOutcome::Successful => {
                                stats.successful_predictions = stats
                                    .successful_predictions
                                    .checked_add(1)
                                    .ok_or(Error::<T>::Overflow)?;
                            }
                            PredictionOutcome::Failed => {
                                stats.failed_predictions = stats
                                    .failed_predictions
                                    .checked_add(1)
                                    .ok_or(Error::<T>::Overflow)?;
                            }
                            PredictionOutcome::Inconclusive => {
                                stats.inconclusive_predictions = stats
                                    .inconclusive_predictions
                                    .checked_add(1)
                                    .ok_or(Error::<T>::Overflow)?;
                            }
                        }

                        Ok(())
                    },
                )?;

                ActivePredictionCountByModel::<T>::mutate(prediction.model_id, |count| {
                    *count = count.saturating_sub(1)
                });
                prediction.status = PredictionStatus::Validated;
                prediction.outcome = Some(outcome);
                prediction.validator = Some(validator.clone());
                prediction.validated_at = Some(now);

                Self::deposit_event(Event::PredictionValidated {
                    prediction_id,
                    model_id: prediction.model_id,
                    validator: validator.clone(),
                    outcome,
                });

                Ok(())
            })
        }
        fn ensure_model_admin(origin: OriginFor<T>) -> DispatchResult {
            match frame_system::ensure_signed_or_root(origin)? {
                None => Ok(()),
                Some(who) => {
                    ensure!(
                        AuthorizedValidators::<T>::get(who),
                        Error::<T>::NotAuthorizedModelAdmin
                    );
                    Ok(())
                }
            }
        }

        fn set_model_status(model_id: ModelId, status: ModelStatus) -> DispatchResult {
            ensure!(
                Models::<T>::contains_key(model_id),
                Error::<T>::ModelNotFound
            );
            ModelGovernanceById::<T>::try_mutate(model_id, |value| -> DispatchResult {
                let governance = value.as_mut().ok_or(Error::<T>::ModelNotFound)?;
                governance.status = status;
                governance.updated_at = frame_system::Pallet::<T>::block_number();
                Ok(())
            })
        }
    }

    impl<T: Config> Pallet<T> {
        fn resolve_bond(dispute_id: DisputeId, accepted: bool, fraudulent: bool, evidence_hash: [u8;32]) -> DispatchResult {
            let economics_config = SettlementEconomicsConfig::<T>::get();
            ensure!(
                economics_config.economics_enabled || !T::FinancialModesAllowed::get(),
                Error::<T>::SettlementEconomicsDisabled
            );

            let (token_id, disputed_by, bond) = PredictionTokenDisputes::<T>::try_mutate(
                dispute_id,
                |maybe_dispute| -> Result<_, DispatchError> {
                    let dispute = maybe_dispute.as_mut().ok_or(Error::<T>::DisputeNotFound)?;

                    ensure!(!dispute.resolved, Error::<T>::DisputeAlreadyResolved);

                    let settlement = PredictionTokenSettlements::<T>::get(dispute.token_id)
                        .ok_or(Error::<T>::SettlementNotFound)?;

                    ensure!(
                        matches!(
                            settlement.status,
                            SettlementStatus::Finalized | SettlementStatus::Cancelled
                        ),
                        Error::<T>::SettlementNotFinalized
                    );

                    if !fraudulent {
                        Self::refund_reserved_exact(&dispute.disputed_by, dispute.bond)?;
                    } else {
                        Self::transfer_reserved_penalty(&dispute.disputed_by, dispute.bond, 2, dispute_id)?;
                    }

                    dispute.resolved = true;
                    dispute.accepted = Some(accepted);

                    Ok((dispute.token_id, dispute.disputed_by.clone(), dispute.bond))
                },
            )?;

            Self::deposit_event(Event::PredictionDisputeBondResolved {
                dispute_id,
                token_id,
                disputed_by,
                accepted,
                bond,
            });

            Self::deposit_event(Event::DisputeBondAdjudicated { dispute_id, accepted, fraudulent, evidence_hash });
            Ok(())
        }

        fn refund_reserved_exact(who: &T::AccountId, amount: BalanceOf<T>) -> DispatchResult {
            ensure!(T::Currency::unreserve(who, amount).is_zero(), Error::<T>::ReserveInvariant);
            Ok(())
        }

        fn transfer_reserved_penalty(who: &T::AccountId, amount: BalanceOf<T>, source: u8, reference: u64) -> DispatchResult {
            let destination = T::PenaltyDestination::get();
            ensure!(destination != *who, Error::<T>::InvalidPenaltyDestination);
            let issuance = T::Currency::total_issuance();
            let remainder = T::Currency::repatriate_reserved(who, &destination, amount, BalanceStatus::Free)?;
            ensure!(remainder.is_zero() && T::Currency::total_issuance() == issuance, Error::<T>::ReserveInvariant);
            Self::deposit_event(Event::ReservedPenaltyTransferred { payer: who.clone(), destination, amount, source, reference });
            Ok(())
        }

        pub fn market_account_id() -> T::AccountId {
            T::PalletId::get().into_account_truncating()
        }

        pub fn market_treasury_account_id() -> T::AccountId {
            T::PalletId::get().into_sub_account_truncating(b"trsy")
        }

        pub fn migrate_market_economics_config_v1_to_v2() -> Weight {
            let on_chain_version = frame_support::traits::StorageVersion::get::<Pallet<T>>();
            let mut reads = 1u64;
            let mut writes = 0u64;

            if on_chain_version < frame_support::traits::StorageVersion::new(2) {
                let _ = PredictionMarketEconomicsConfig::<T>::translate::<
                    PredictionMarketEconomicsSettingsV1,
                    _,
                >(|maybe_old| {
                    maybe_old.map(|old| PredictionMarketEconomicsSettings {
                        market_enabled: old.market_enabled,
                        allow_unstake_before_settlement: old.allow_unstake_before_settlement,
                        fees_enabled: false,
                        fee_bps: 0,
                        treasury_enabled: true,
                    })
                });
                reads = reads.saturating_add(1);
                writes = writes.saturating_add(1);
            }

            if on_chain_version < STORAGE_VERSION {
                let next_model_id = NextModelId::<T>::get();
                reads = reads.saturating_add(1);
                for model_id in 0..next_model_id {
                    let Some(model) = Models::<T>::get(model_id) else {
                        reads = reads.saturating_add(1);
                        continue;
                    };
                    reads = reads.saturating_add(1);
                    ModelGovernanceById::<T>::insert(
                        model_id,
                        ModelGovernance {
                            status: if model.active {
                                ModelStatus::Approved
                            } else {
                                ModelStatus::Suspended
                            },
                            authorized_submitter: None,
                            updated_at: model.updated_at.unwrap_or(model.registered_at),
                        },
                    );
                    ModelIdsByOwner::<T>::mutate(&model.owner, |ids| {
                        let _ = ids.try_push(model_id);
                    });
                    reads = reads.saturating_add(1);
                    writes = writes.saturating_add(2);
                }
                let next_prediction_id = NextPredictionId::<T>::get();
                reads = reads.saturating_add(1);
                for prediction_id in 0..next_prediction_id {
                    let Some(prediction) = Predictions::<T>::get(prediction_id) else {
                        reads = reads.saturating_add(1);
                        continue;
                    };
                    reads = reads.saturating_add(1);
                    PredictionIdsByModel::<T>::mutate(prediction.model_id, |ids| {
                        let _ = ids.try_push(prediction_id);
                    });
                    reads = reads.saturating_add(1);
                    writes = writes.saturating_add(1);
                    if prediction.status == PredictionStatus::Open {
                        ActivePredictionCountByModel::<T>::mutate(prediction.model_id, |count| {
                            *count = count.saturating_add(1)
                        });
                        reads = reads.saturating_add(1);
                        writes = writes.saturating_add(1);
                    }
                }
                STORAGE_VERSION.put::<Pallet<T>>();
                writes = writes.saturating_add(1);
            }

            T::DbWeight::get().reads_writes(reads, writes)
        }

        fn winning_market_side(outcome: &SettlementOutcome) -> Option<PredictionOutcomeSide> {
            match outcome {
                SettlementOutcome::Correct => Some(PredictionOutcomeSide::Yes),
                SettlementOutcome::Incorrect => Some(PredictionOutcomeSide::No),
                _ => None,
            }
        }

        fn opposite_market_side(side: &PredictionOutcomeSide) -> PredictionOutcomeSide {
            match side {
                PredictionOutcomeSide::Yes => PredictionOutcomeSide::No,
                PredictionOutcomeSide::No => PredictionOutcomeSide::Yes,
            }
        }

        fn bounded_model_hash(input: Vec<u8>) -> Result<BoundedHashOf<T>, DispatchError> {
            ensure!(!input.is_empty(), Error::<T>::InvalidModelHash);
            input
                .try_into()
                .map_err(|_| Error::<T>::InvalidModelHash.into())
        }

        fn bounded_prediction_hash(input: Vec<u8>) -> Result<BoundedHashOf<T>, DispatchError> {
            ensure!(!input.is_empty(), Error::<T>::InvalidPredictionHash);
            input
                .try_into()
                .map_err(|_| Error::<T>::InvalidPredictionHash.into())
        }

        fn bounded_metadata_uri(input: Vec<u8>) -> Result<BoundedMetadataUriOf<T>, DispatchError> {
            ensure!(!input.is_empty(), Error::<T>::InvalidMetadata);
            input
                .try_into()
                .map_err(|_| Error::<T>::InvalidMetadata.into())
        }

        fn bounded_category_code(
            input: Vec<u8>,
        ) -> Result<BoundedCategoryCodeOf<T>, DispatchError> {
            ensure!(!input.is_empty(), Error::<T>::InvalidCategoryCode);
            input
                .try_into()
                .map_err(|_| Error::<T>::InvalidCategoryCode.into())
        }
    }
}
