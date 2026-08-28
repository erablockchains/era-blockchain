//! Upgrade 13 policy constants and validation helpers.
//!
//! Real-account candidate data is confined to `cfg(test)` fixtures for reversible validation. The
//! module contains only generic monetary constants and pure helpers. Persistent lifetime-allowance
//! storage, migration gating and controlled issuance live in `crate::issuance_cap`; custody creation
//! and the account-specific reconciliation vector remain outside this module.

use crate::{Balance, BlockNumber};

pub const TOKEN_DECIMALS: u8 = 18;
pub const TARGET_RETAINED_ISSUANCE: Balance = 100_000_000_000_000_000_000_000_000;
pub const ABSOLUTE_LIFETIME_CAP: Balance = 1_000_000_000_000_000_000_000_000_000;
pub const MAXIMUM_POST_CORRECTION_NEW_ISSUANCE: Balance = 900_000_000_000_000_000_000_000_000;

pub const FOUNDING_MEMBERS_POOL: Balance = 20_000_000_000_000_000_000_000_000;
pub const ACTIVE_PRESALE_POOL: Balance = 20_000_000_000_000_000_000_000_000;
pub const AIRDROP_VALIDATOR_POOL: Balance = 30_000_000_000_000_000_000_000_000;
pub const ECOSYSTEM_POOL: Balance = 20_000_000_000_000_000_000_000_000;
pub const LIQUIDITY_RESERVE_POOL: Balance = 10_000_000_000_000_000_000_000_000;
pub const SUDO_OPERATIONAL_RESERVE: Balance = crate::DECIMALS;
pub const SUDO_TARGET_FREE: Balance = crate::EXISTENTIAL_DEPOSIT + SUDO_OPERATIONAL_RESERVE;
pub const VALIDATOR_NOMINATOR_REWARD_BUDGET: Balance = 20_000_000_000_000_000_000_000_000;
pub const COMMUNITY_ONBOARDING_BUDGET: Balance = 10_000_000_000_000_000_000_000_000;
pub const REWARD_RESERVE_GROSS_TARGET: Balance = VALIDATOR_NOMINATOR_REWARD_BUDGET;
pub const REWARD_RESERVE_SPENDABLE_TARGET: Balance =
    REWARD_RESERVE_GROSS_TARGET - crate::EXISTENTIAL_DEPOSIT;

pub const RETAINED_SUPPLY_POOLS: [Balance; 5] = [
    FOUNDING_MEMBERS_POOL,
    ACTIVE_PRESALE_POOL,
    AIRDROP_VALIDATOR_POOL,
    ECOSYSTEM_POOL,
    LIQUIDITY_RESERVE_POOL,
];

pub const FOUNDING_ALLOCATION_TARGETS: [Balance; 5] = [
    8_000_000_000_000_000_000_000_000,
    5_000_000_000_000_000_000_000_000,
    3_000_000_000_000_000_000_000_000,
    2_000_000_000_000_000_000_000_000,
    2_000_000_000_000_000_000_000_000,
];
pub const FOUNDING_VESTING_RELEASE_INTERVALS: BlockNumber = 5_256_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProvisionalLinearVestingTerms {
    pub release_intervals: BlockNumber,
    pub floor_release_per_interval: Balance,
    pub final_residual: Balance,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VestingScheduleTerms {
    pub locked: Balance,
    pub per_block: Balance,
    pub starting_block: BlockNumber,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExactC2VestingScheduleTerms {
    pub schedule_a: VestingScheduleTerms,
    pub schedule_b: VestingScheduleTerms,
    pub final_interval_release: Balance,
}

pub const VALIDATOR_MINIMUM_CANDIDACY_BOND: Balance = 10_000_000_000_000_000_000_000;
pub const VALIDATOR_MINIMUM_OPERATOR_FUNDED: Balance = 5_000_000_000_000_000_000_000;
pub const VALIDATOR_MAXIMUM_ONBOARDING_SUPPORT: Balance = 5_000_000_000_000_000_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PolicyError {
    ArithmeticOverflow,
    ArithmeticUnderflow,
    AllocationSumMismatch,
    FoundingAllocationSumMismatch,
    TargetIssuanceMismatch,
    CurrentIssuanceAboveCap,
    LifetimeCapExceeded,
    PostCorrectionIssuanceAllowanceExceeded,
    BlockCountConversionFailed,
    BlockNumberOverflow,
    InvalidTwoScheduleVestingTerms,
    InternalBudgetSumMismatch,
    MigrationAuthorizationMissing,
    EmissionsSemanticsUnresolved,
    RewardReserveAccountMigrationRejected,
    RewardReserveTargetMismatch,
    RewardLiabilityMismatch,
    RewardReserveInsolvent,
}

pub fn checked_sum(values: &[Balance]) -> Result<Balance, PolicyError> {
    values.iter().try_fold(0 as Balance, |sum, value| {
        sum.checked_add(*value)
            .ok_or(PolicyError::ArithmeticOverflow)
    })
}

pub fn validate_policy_constants() -> Result<(), PolicyError> {
    if checked_sum(&RETAINED_SUPPLY_POOLS)? != TARGET_RETAINED_ISSUANCE {
        return Err(PolicyError::AllocationSumMismatch);
    }

    if checked_sum(&FOUNDING_ALLOCATION_TARGETS)? != FOUNDING_MEMBERS_POOL {
        return Err(PolicyError::FoundingAllocationSumMismatch);
    }

    if checked_sum(&[
        VALIDATOR_NOMINATOR_REWARD_BUDGET,
        COMMUNITY_ONBOARDING_BUDGET,
    ])? != AIRDROP_VALIDATOR_POOL
    {
        return Err(PolicyError::InternalBudgetSumMismatch);
    }

    let capacity = ABSOLUTE_LIFETIME_CAP
        .checked_sub(TARGET_RETAINED_ISSUANCE)
        .ok_or(PolicyError::ArithmeticUnderflow)?;
    if capacity != MAXIMUM_POST_CORRECTION_NEW_ISSUANCE {
        return Err(PolicyError::TargetIssuanceMismatch);
    }

    Ok(())
}

pub fn validate_target_issuance(issuance: Balance) -> Result<(), PolicyError> {
    if issuance == TARGET_RETAINED_ISSUANCE {
        Ok(())
    } else {
        Err(PolicyError::TargetIssuanceMismatch)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RewardReserveCorrectionPlan {
    pub gross_target: Balance,
    pub spendable_target: Balance,
    pub candidate_in_place_destruction: Balance,
    pub candidate_transfer: Balance,
}

/// Validates only the approved in-place Reward Reserve arithmetic.
///
/// It deliberately rejects an account migration and performs no storage operation.
pub fn reward_reserve_in_place_correction(
    retain_existing_pallet_account: bool,
    current_gross_balance: Balance,
    requested_gross_target: Balance,
    safety_floor: Balance,
    committed_liabilities: Balance,
    summed_era_liabilities: Balance,
) -> Result<RewardReserveCorrectionPlan, PolicyError> {
    if !retain_existing_pallet_account {
        return Err(PolicyError::RewardReserveAccountMigrationRejected);
    }
    if requested_gross_target != REWARD_RESERVE_GROSS_TARGET {
        return Err(PolicyError::RewardReserveTargetMismatch);
    }
    let spendable_target = requested_gross_target
        .checked_sub(safety_floor)
        .ok_or(PolicyError::ArithmeticUnderflow)?;
    if spendable_target != REWARD_RESERVE_SPENDABLE_TARGET {
        return Err(PolicyError::RewardReserveTargetMismatch);
    }
    if committed_liabilities != summed_era_liabilities {
        return Err(PolicyError::RewardLiabilityMismatch);
    }
    if committed_liabilities > spendable_target {
        return Err(PolicyError::RewardReserveInsolvent);
    }
    let candidate_in_place_destruction = current_gross_balance
        .checked_sub(requested_gross_target)
        .ok_or(PolicyError::ArithmeticUnderflow)?;
    Ok(RewardReserveCorrectionPlan {
        gross_target: requested_gross_target,
        spendable_target,
        candidate_in_place_destruction,
        candidate_transfer: 0,
    })
}

pub fn remaining_post_correction_issuance_allowance(
    cumulative_new_issuance: Balance,
) -> Result<Balance, PolicyError> {
    MAXIMUM_POST_CORRECTION_NEW_ISSUANCE
        .checked_sub(cumulative_new_issuance)
        .ok_or(PolicyError::PostCorrectionIssuanceAllowanceExceeded)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IssuanceGuardState {
    pub current_issuance: Balance,
    pub cumulative_post_correction_new_issuance: Balance,
}

/// Checks both supply limits. Burns may lower current issuance but never this cumulative counter.
pub fn checked_positive_issuance(
    state: IssuanceGuardState,
    requested_issuance: Balance,
) -> Result<IssuanceGuardState, PolicyError> {
    if state.current_issuance > ABSOLUTE_LIFETIME_CAP {
        return Err(PolicyError::CurrentIssuanceAboveCap);
    }
    let next_issuance = state
        .current_issuance
        .checked_add(requested_issuance)
        .ok_or(PolicyError::ArithmeticOverflow)?;
    if next_issuance > ABSOLUTE_LIFETIME_CAP {
        return Err(PolicyError::LifetimeCapExceeded);
    }
    let next_cumulative = state
        .cumulative_post_correction_new_issuance
        .checked_add(requested_issuance)
        .ok_or(PolicyError::ArithmeticOverflow)?;
    if next_cumulative > MAXIMUM_POST_CORRECTION_NEW_ISSUANCE {
        return Err(PolicyError::PostCorrectionIssuanceAllowanceExceeded);
    }
    Ok(IssuanceGuardState {
        current_issuance: next_issuance,
        cumulative_post_correction_new_issuance: next_cumulative,
    })
}

pub fn checked_block_count_as_balance(block_count: BlockNumber) -> Result<Balance, PolicyError> {
    Ok(Balance::from(block_count))
}

pub fn provisional_linear_vesting_terms(
    target: Balance,
) -> Result<ProvisionalLinearVestingTerms, PolicyError> {
    let release_intervals = checked_block_count_as_balance(FOUNDING_VESTING_RELEASE_INTERVALS)?;
    let floor_release_per_interval = target / release_intervals;
    let final_residual = target % release_intervals;
    if floor_release_per_interval == 0 || final_residual == 0 {
        return Err(PolicyError::InvalidTwoScheduleVestingTerms);
    }
    Ok(ProvisionalLinearVestingTerms {
        release_intervals: FOUNDING_VESTING_RELEASE_INTERVALS,
        floor_release_per_interval,
        final_residual,
    })
}

/// Pure candidate construction for the approved C2 rule. It performs no storage operation.
pub fn provisional_c2_two_schedule_terms(
    target: Balance,
    starting_block: BlockNumber,
) -> Result<ExactC2VestingScheduleTerms, PolicyError> {
    let terms = provisional_linear_vesting_terms(target)?;
    let release_intervals = checked_block_count_as_balance(terms.release_intervals)?;
    let linear_locked = terms
        .floor_release_per_interval
        .checked_mul(release_intervals)
        .ok_or(PolicyError::ArithmeticOverflow)?;
    let final_start_offset = terms
        .release_intervals
        .checked_sub(1)
        .ok_or(PolicyError::BlockNumberOverflow)?;
    let final_start = starting_block
        .checked_add(final_start_offset)
        .ok_or(PolicyError::BlockNumberOverflow)?;
    let final_interval_release = terms
        .floor_release_per_interval
        .checked_add(terms.final_residual)
        .ok_or(PolicyError::ArithmeticOverflow)?;
    if linear_locked
        .checked_add(terms.final_residual)
        .ok_or(PolicyError::ArithmeticOverflow)?
        != target
    {
        return Err(PolicyError::InvalidTwoScheduleVestingTerms);
    }
    Ok(ExactC2VestingScheduleTerms {
        schedule_a: VestingScheduleTerms {
            locked: linear_locked,
            per_block: terms.floor_release_per_interval,
            starting_block,
        },
        schedule_b: VestingScheduleTerms {
            locked: terms.final_residual,
            per_block: terms.final_residual,
            starting_block: final_start,
        },
        final_interval_release,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RewardFundingBoundary {
    pub fee_funded: Balance,
    pub issuance_shortfall: Balance,
    pub post_issuance: Balance,
    pub post_cumulative_issuance: Balance,
}

/// Computes only the fee-first amount and dual-cap boundary.
///
/// The returned shortfall is not authorization to mint. Annual/per-era timing and rounding must be
/// supplied by a future independently approved emissions policy.
pub fn cap_checked_reward_shortfall(
    issuance_state: IssuanceGuardState,
    reward_due: Balance,
    fee_credit_available: Balance,
) -> Result<RewardFundingBoundary, PolicyError> {
    let fee_funded = core::cmp::min(reward_due, fee_credit_available);
    let issuance_shortfall = reward_due
        .checked_sub(fee_funded)
        .ok_or(PolicyError::ArithmeticUnderflow)?;
    let post = checked_positive_issuance(issuance_state, issuance_shortfall)?;
    Ok(RewardFundingBoundary {
        fee_funded,
        issuance_shortfall,
        post_issuance: post.current_issuance,
        post_cumulative_issuance: post.cumulative_post_correction_new_issuance,
    })
}

pub trait EmissionAuthorization {
    fn authorize_shortfall(
        &self,
        _current_issuance: Balance,
        _requested_shortfall: Balance,
    ) -> Result<Balance, PolicyError>;
}

/// Fail-closed placeholder until rounding, timing, carry and residual semantics are approved.
pub struct EmissionsUnresolved;

impl EmissionAuthorization for EmissionsUnresolved {
    fn authorize_shortfall(
        &self,
        _current_issuance: Balance,
        _requested_shortfall: Balance,
    ) -> Result<Balance, PolicyError> {
        Err(PolicyError::EmissionsSemanticsUnresolved)
    }
}

pub fn require_migration_authorization<T>(authorization: Option<T>) -> Result<T, PolicyError> {
    authorization.ok_or(PolicyError::MigrationAuthorizationMissing)
}

pub fn has_minimum_validator_bond(active_bond: Balance) -> bool {
    active_bond >= VALIDATOR_MINIMUM_CANDIDACY_BOND
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ValidatorCandidacyReadiness {
    pub active_bond: Balance,
    pub session_keys_registered: bool,
}

impl ValidatorCandidacyReadiness {
    /// Technical and bond qualification only; this never implies election or an active seat.
    pub fn candidate_requirements_met(&self) -> bool {
        self.session_keys_registered && has_minimum_validator_bond(self.active_bond)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pallet_vesting::VestingInfo;
    use sp_runtime::traits::ConvertInto;
    use std::{vec, vec::Vec};

    const MOCK_STORAGE_VERSION_BEFORE: u16 = 0;
    const MOCK_STORAGE_VERSION_AFTER: u16 = 1;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct ProvisionalFoundingFixture {
        account_id_hex: &'static str,
        beneficiary: &'static str,
        current_balance: Balance,
        target_balance: Balance,
        candidate_destruction: Balance,
    }

    // Real provisional values are deliberately confined to the native test build.
    const PROVISIONAL_FOUNDING_ALLOCATIONS: [ProvisionalFoundingFixture; 5] = [
        ProvisionalFoundingFixture {
            account_id_hex: "ba29ecf97ed52e7fcc0fdd2cc364a646e140bd8c79862a57db06db685d03567c",
            beneficiary: "King Aibangbee",
            current_balance: 80_000_000_000_000_000_000_000_000,
            target_balance: 8_000_000_000_000_000_000_000_000,
            candidate_destruction: 72_000_000_000_000_000_000_000_000,
        },
        ProvisionalFoundingFixture {
            account_id_hex: "aa0254cda2a14ab6c8ef9db2df74d48a175a7504f8a16775c0296975149e3d4a",
            beneficiary: "Richard Legemah",
            current_balance: 30_000_000_000_000_000_000_000_000,
            target_balance: 5_000_000_000_000_000_000_000_000,
            candidate_destruction: 25_000_000_000_000_000_000_000_000,
        },
        ProvisionalFoundingFixture {
            account_id_hex: "a4520fa060b013a5e4430f18ce8ea5568db96d000594a5857b212a5f3f631d6d",
            beneficiary: "Joachim Yaduat",
            current_balance: 30_000_000_000_000_000_000_000_000,
            target_balance: 3_000_000_000_000_000_000_000_000,
            candidate_destruction: 27_000_000_000_000_000_000_000_000,
        },
        ProvisionalFoundingFixture {
            account_id_hex: "981bb1c87263351dded88df780557dc8ae5f96aa7f7bcbccda8ea9ab8424dd3b",
            beneficiary: "King Aibangbee",
            current_balance: 30_000_000_000_000_000_000_000_000,
            target_balance: 2_000_000_000_000_000_000_000_000,
            candidate_destruction: 28_000_000_000_000_000_000_000_000,
        },
        ProvisionalFoundingFixture {
            account_id_hex: "b43d1c7b0f4e2d3a343c49699d3ba5060e06299a66e91f3542ef441ce2dc4306",
            beneficiary: "King Aibangbee",
            current_balance: 30_000_000_000_000_000_000_000_000,
            target_balance: 2_000_000_000_000_000_000_000_000,
            candidate_destruction: 28_000_000_000_000_000_000_000_000,
        },
    ];

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Pool {
        Founding,
        Presale,
        ContributorValidator,
        Foundation,
        Liquidity,
    }

    impl Pool {
        fn index(self) -> usize {
            match self {
                Self::Founding => 0,
                Self::Presale => 1,
                Self::ContributorValidator => 2,
                Self::Foundation => 3,
                Self::Liquidity => 4,
            }
        }
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct MockAccount {
        id: u8,
        balance: Balance,
        pool: Pool,
        preserved_third_party: bool,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct MockReduction {
        account: u8,
        amount: Balance,
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct MockAuthorization {
        expected_pre_issuance: Balance,
        reductions: Vec<MockReduction>,
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct MockPreState {
        issuance: Balance,
        preserved_account: u8,
        preserved_balance: Balance,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum MockMigrationError {
        Policy(PolicyError),
        AlreadyExecuted,
        PreIssuanceMismatch,
        IncompleteDebitVector,
        DuplicateAccount,
        UnknownAccount,
        ProtectedThirdParty,
        ReductionExceedsReducibleBalance,
        ResultNotTarget,
        PoolTotalMismatch,
        PreservedBalanceChanged,
        StorageVersionMismatch,
    }

    impl From<PolicyError> for MockMigrationError {
        fn from(error: PolicyError) -> Self {
            Self::Policy(error)
        }
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct MockMigration {
        storage_version: u16,
        accounts: Vec<MockAccount>,
    }

    impl MockMigration {
        fn issuance(&self) -> Result<Balance, MockMigrationError> {
            checked_sum(
                &self
                    .accounts
                    .iter()
                    .map(|account| account.balance)
                    .collect::<Vec<_>>(),
            )
            .map_err(Into::into)
        }

        fn pre_upgrade(
            &self,
            authorization: Option<&MockAuthorization>,
        ) -> Result<MockPreState, MockMigrationError> {
            let authorization = require_migration_authorization(authorization)?;
            if self.storage_version != MOCK_STORAGE_VERSION_BEFORE {
                return Err(MockMigrationError::AlreadyExecuted);
            }
            validate_policy_constants()?;

            let issuance = self.issuance()?;
            if issuance != authorization.expected_pre_issuance {
                return Err(MockMigrationError::PreIssuanceMismatch);
            }

            for (index, reduction) in authorization.reductions.iter().enumerate() {
                if authorization.reductions[..index]
                    .iter()
                    .any(|seen| seen.account == reduction.account)
                {
                    return Err(MockMigrationError::DuplicateAccount);
                }
            }

            let reducible_count = self
                .accounts
                .iter()
                .filter(|account| !account.preserved_third_party)
                .count();
            if authorization.reductions.len() != reducible_count {
                return Err(MockMigrationError::IncompleteDebitVector);
            }

            let mut simulated = self.clone();
            for reduction in &authorization.reductions {
                let account = simulated
                    .accounts
                    .iter_mut()
                    .find(|account| account.id == reduction.account)
                    .ok_or(MockMigrationError::UnknownAccount)?;
                if account.preserved_third_party {
                    return Err(MockMigrationError::ProtectedThirdParty);
                }
                account.balance = account
                    .balance
                    .checked_sub(reduction.amount)
                    .ok_or(MockMigrationError::ReductionExceedsReducibleBalance)?;
            }

            if simulated.issuance()? != TARGET_RETAINED_ISSUANCE {
                return Err(MockMigrationError::ResultNotTarget);
            }
            simulated.validate_pool_totals()?;

            let preserved = self
                .accounts
                .iter()
                .find(|account| account.preserved_third_party)
                .expect("fixture contains a synthetic third party");
            Ok(MockPreState {
                issuance,
                preserved_account: preserved.id,
                preserved_balance: preserved.balance,
            })
        }

        fn execute(
            &mut self,
            authorization: Option<&MockAuthorization>,
        ) -> Result<MockPreState, MockMigrationError> {
            let pre = self.pre_upgrade(authorization)?;
            let authorization = require_migration_authorization(authorization)?;
            let mut next = self.clone();
            for reduction in &authorization.reductions {
                let account = next
                    .accounts
                    .iter_mut()
                    .find(|account| account.id == reduction.account)
                    .ok_or(MockMigrationError::UnknownAccount)?;
                account.balance = account
                    .balance
                    .checked_sub(reduction.amount)
                    .ok_or(MockMigrationError::ReductionExceedsReducibleBalance)?;
            }
            next.storage_version = MOCK_STORAGE_VERSION_AFTER;
            next.post_upgrade(&pre)?;
            *self = next;
            Ok(pre)
        }

        fn post_upgrade(&self, pre: &MockPreState) -> Result<(), MockMigrationError> {
            if self.storage_version != MOCK_STORAGE_VERSION_AFTER {
                return Err(MockMigrationError::StorageVersionMismatch);
            }
            validate_target_issuance(self.issuance()?)?;
            self.validate_pool_totals()?;
            let preserved = self
                .accounts
                .iter()
                .find(|account| account.id == pre.preserved_account)
                .ok_or(MockMigrationError::UnknownAccount)?;
            if preserved.balance != pre.preserved_balance {
                return Err(MockMigrationError::PreservedBalanceChanged);
            }
            Ok(())
        }

        fn validate_pool_totals(&self) -> Result<(), MockMigrationError> {
            let mut totals = [0 as Balance; 5];
            for account in &self.accounts {
                totals[account.pool.index()] = totals[account.pool.index()]
                    .checked_add(account.balance)
                    .ok_or(MockMigrationError::Policy(PolicyError::ArithmeticOverflow))?;
            }
            if totals != RETAINED_SUPPLY_POOLS {
                return Err(MockMigrationError::PoolTotalMismatch);
            }
            Ok(())
        }
    }

    fn units(etkn: Balance) -> Balance {
        etkn.checked_mul(crate::DECIMALS).expect("mock amount fits")
    }

    fn mock_fixture() -> (MockMigration, MockAuthorization) {
        let migration = MockMigration {
            storage_version: MOCK_STORAGE_VERSION_BEFORE,
            accounts: vec![
                MockAccount {
                    id: 1,
                    balance: units(25_000_000),
                    pool: Pool::Founding,
                    preserved_third_party: false,
                },
                MockAccount {
                    id: 2,
                    balance: units(30_000_000),
                    pool: Pool::Presale,
                    preserved_third_party: false,
                },
                MockAccount {
                    id: 3,
                    balance: units(35_000_000),
                    pool: Pool::ContributorValidator,
                    preserved_third_party: false,
                },
                MockAccount {
                    id: 4,
                    balance: units(30_000_000),
                    pool: Pool::Foundation,
                    preserved_third_party: false,
                },
                MockAccount {
                    id: 5,
                    balance: units(14_000_000),
                    pool: Pool::Liquidity,
                    preserved_third_party: false,
                },
                MockAccount {
                    id: 6,
                    balance: units(1_000_000),
                    pool: Pool::Liquidity,
                    preserved_third_party: true,
                },
            ],
        };
        let authorization = MockAuthorization {
            expected_pre_issuance: units(135_000_000),
            reductions: vec![
                MockReduction {
                    account: 1,
                    amount: units(5_000_000),
                },
                MockReduction {
                    account: 2,
                    amount: units(10_000_000),
                },
                MockReduction {
                    account: 3,
                    amount: units(5_000_000),
                },
                MockReduction {
                    account: 4,
                    amount: units(10_000_000),
                },
                MockReduction {
                    account: 5,
                    amount: units(5_000_000),
                },
            ],
        };
        (migration, authorization)
    }

    #[test]
    fn policy_constants_are_exact_and_internally_consistent() {
        assert_eq!(validate_policy_constants(), Ok(()));
        assert_eq!(TOKEN_DECIMALS, 18);
        assert_eq!(
            remaining_post_correction_issuance_allowance(0),
            Ok(MAXIMUM_POST_CORRECTION_NEW_ISSUANCE)
        );
    }

    #[test]
    fn reward_reserve_correction_is_in_place_exact_and_liability_safe() {
        let committed = 47_913_372_780_742_576_152_444;
        let plan = reward_reserve_in_place_correction(
            true,
            30_000_000_000_100_000_000_000_000,
            REWARD_RESERVE_GROSS_TARGET,
            crate::EXISTENTIAL_DEPOSIT,
            committed,
            committed,
        )
        .expect("fresh fixed-anchor reward correction is solvent");
        assert_eq!(plan.gross_target, units(20_000_000));
        assert_eq!(
            plan.spendable_target,
            units(20_000_000) - crate::EXISTENTIAL_DEPOSIT
        );
        assert_eq!(
            plan.candidate_in_place_destruction,
            10_000_000_000_100_000_000_000_000
        );
        assert_eq!(plan.candidate_transfer, 0);
    }

    #[test]
    fn reward_reserve_alternate_account_and_continuity_fail_closed() {
        let committed = 47_913_372_780_742_576_152_444;
        assert_eq!(
            reward_reserve_in_place_correction(
                false,
                units(30_000_000),
                REWARD_RESERVE_GROSS_TARGET,
                crate::EXISTENTIAL_DEPOSIT,
                committed,
                committed,
            ),
            Err(PolicyError::RewardReserveAccountMigrationRejected)
        );
        assert_eq!(
            reward_reserve_in_place_correction(
                true,
                units(30_000_000),
                REWARD_RESERVE_GROSS_TARGET,
                crate::EXISTENTIAL_DEPOSIT,
                committed,
                committed - 1,
            ),
            Err(PolicyError::RewardLiabilityMismatch)
        );
        assert_eq!(
            reward_reserve_in_place_correction(
                true,
                units(30_000_000),
                REWARD_RESERVE_GROSS_TARGET,
                crate::EXISTENTIAL_DEPOSIT,
                REWARD_RESERVE_SPENDABLE_TARGET + 1,
                REWARD_RESERVE_SPENDABLE_TARGET + 1,
            ),
            Err(PolicyError::RewardReserveInsolvent)
        );
    }

    #[test]
    fn provisional_founding_map_and_candidate_reductions_are_exact() {
        assert_eq!(
            checked_sum(
                &PROVISIONAL_FOUNDING_ALLOCATIONS
                    .iter()
                    .map(|allocation| allocation.current_balance)
                    .collect::<Vec<_>>()
            ),
            Ok(units(200_000_000))
        );
        assert_eq!(
            checked_sum(
                &PROVISIONAL_FOUNDING_ALLOCATIONS
                    .iter()
                    .map(|allocation| allocation.target_balance)
                    .collect::<Vec<_>>()
            ),
            Ok(FOUNDING_MEMBERS_POOL)
        );
        assert_eq!(
            checked_sum(
                &PROVISIONAL_FOUNDING_ALLOCATIONS
                    .iter()
                    .map(|allocation| allocation.candidate_destruction)
                    .collect::<Vec<_>>()
            ),
            Ok(units(180_000_000))
        );
        for allocation in PROVISIONAL_FOUNDING_ALLOCATIONS {
            assert_eq!(allocation.account_id_hex.len(), 64);
            assert!(!allocation.beneficiary.is_empty());
            assert_eq!(
                allocation.current_balance - allocation.candidate_destruction,
                allocation.target_balance
            );
        }
    }

    #[test]
    fn checked_sum_rejects_overflow() {
        assert_eq!(
            checked_sum(&[Balance::MAX, 1]),
            Err(PolicyError::ArithmeticOverflow)
        );
    }

    #[test]
    fn absolute_and_monotonic_issuance_caps_fail_closed() {
        let at_checkpoint = IssuanceGuardState {
            current_issuance: TARGET_RETAINED_ISSUANCE,
            cumulative_post_correction_new_issuance: 0,
        };
        let fully_issued =
            checked_positive_issuance(at_checkpoint, MAXIMUM_POST_CORRECTION_NEW_ISSUANCE)
                .expect("exact shared allowance");
        assert_eq!(fully_issued.current_issuance, ABSOLUTE_LIFETIME_CAP);
        assert_eq!(
            fully_issued.cumulative_post_correction_new_issuance,
            MAXIMUM_POST_CORRECTION_NEW_ISSUANCE
        );
        assert_eq!(
            checked_positive_issuance(fully_issued, 1),
            Err(PolicyError::LifetimeCapExceeded)
        );

        let after_burn = IssuanceGuardState {
            current_issuance: TARGET_RETAINED_ISSUANCE,
            ..fully_issued
        };
        assert_eq!(
            checked_positive_issuance(after_burn, 1),
            Err(PolicyError::PostCorrectionIssuanceAllowanceExceeded)
        );
        assert_eq!(
            remaining_post_correction_issuance_allowance(MAXIMUM_POST_CORRECTION_NEW_ISSUANCE + 1),
            Err(PolicyError::PostCorrectionIssuanceAllowanceExceeded)
        );
    }

    #[test]
    fn pinned_vesting_info_reproduces_exact_c2_release_boundaries() {
        let expected = [
            (1_522_070_015_220_700_152, 1_088_000),
            (951_293_759_512_937_595, 680_000),
            (570_776_255_707_762_557, 408_000),
            (380_517_503_805_175_038, 272_000),
            (380_517_503_805_175_038, 272_000),
        ];
        let starting_block: BlockNumber = 100;
        let completion_block = starting_block
            .checked_add(FOUNDING_VESTING_RELEASE_INTERVALS)
            .expect("bounded completion block");
        let last_release_block = completion_block - 1;

        for (target, (floor, residual)) in FOUNDING_ALLOCATION_TARGETS.into_iter().zip(expected) {
            let terms = provisional_linear_vesting_terms(target).expect("approved terms");
            assert_eq!(terms.release_intervals, FOUNDING_VESTING_RELEASE_INTERVALS);
            assert_eq!(terms.floor_release_per_interval, floor);
            assert_eq!(terms.final_residual, residual);

            let pair =
                provisional_c2_two_schedule_terms(target, starting_block).expect("approved pair");
            let schedule_a = VestingInfo::<Balance, BlockNumber>::new(
                pair.schedule_a.locked,
                pair.schedule_a.per_block,
                pair.schedule_a.starting_block,
            );
            let schedule_b = VestingInfo::<Balance, BlockNumber>::new(
                pair.schedule_b.locked,
                pair.schedule_b.per_block,
                pair.schedule_b.starting_block,
            );
            let locked_at = |block| {
                schedule_a
                    .locked_at::<ConvertInto>(block)
                    .checked_add(schedule_b.locked_at::<ConvertInto>(block))
                    .expect("sum of schedules is bounded by target")
            };

            assert_eq!(pair.schedule_b.starting_block, last_release_block);
            assert_eq!(pair.final_interval_release, floor + residual);
            assert_eq!(locked_at(starting_block - 1), target);
            assert_eq!(locked_at(starting_block), target);
            assert_eq!(locked_at(starting_block + 1), target - floor);
            assert_eq!(locked_at(last_release_block), floor + residual);
            assert_eq!(locked_at(completion_block), 0);
            assert_eq!(locked_at(completion_block + 1), 0);
        }
    }

    #[test]
    fn c2_construction_checks_conversion_invalid_terms_and_block_overflow() {
        assert_eq!(
            checked_block_count_as_balance(FOUNDING_VESTING_RELEASE_INTERVALS),
            Ok(5_256_000)
        );
        assert_eq!(
            checked_block_count_as_balance(BlockNumber::MAX),
            Ok(BlockNumber::MAX as Balance)
        );

        let final_start_offset = FOUNDING_VESTING_RELEASE_INTERVALS - 1;
        let last_safe_start = BlockNumber::MAX - final_start_offset;
        let safe =
            provisional_c2_two_schedule_terms(FOUNDING_ALLOCATION_TARGETS[0], last_safe_start)
                .expect("last safe start");
        assert_eq!(safe.schedule_b.starting_block, BlockNumber::MAX);
        assert_eq!(
            provisional_c2_two_schedule_terms(FOUNDING_ALLOCATION_TARGETS[0], last_safe_start + 1),
            Err(PolicyError::BlockNumberOverflow)
        );

        let intervals =
            checked_block_count_as_balance(FOUNDING_VESTING_RELEASE_INTERVALS).expect("conversion");
        assert_eq!(
            provisional_c2_two_schedule_terms(intervals, 1),
            Err(PolicyError::InvalidTwoScheduleVestingTerms)
        );
        assert_eq!(
            provisional_c2_two_schedule_terms(intervals - 1, 1),
            Err(PolicyError::InvalidTwoScheduleVestingTerms)
        );
    }

    #[test]
    fn fee_first_reward_boundary_handles_zero_and_shortfall() {
        let checkpoint = IssuanceGuardState {
            current_issuance: TARGET_RETAINED_ISSUANCE,
            cumulative_post_correction_new_issuance: 0,
        };
        assert_eq!(
            cap_checked_reward_shortfall(checkpoint, units(10), units(10)),
            Ok(RewardFundingBoundary {
                fee_funded: units(10),
                issuance_shortfall: 0,
                post_issuance: TARGET_RETAINED_ISSUANCE,
                post_cumulative_issuance: 0,
            })
        );
        assert_eq!(
            cap_checked_reward_shortfall(checkpoint, units(10), units(4)),
            Ok(RewardFundingBoundary {
                fee_funded: units(4),
                issuance_shortfall: units(6),
                post_issuance: TARGET_RETAINED_ISSUANCE + units(6),
                post_cumulative_issuance: units(6),
            })
        );
        assert_eq!(
            cap_checked_reward_shortfall(
                IssuanceGuardState {
                    current_issuance: ABSOLUTE_LIFETIME_CAP - units(5),
                    cumulative_post_correction_new_issuance: MAXIMUM_POST_CORRECTION_NEW_ISSUANCE
                        - units(5),
                },
                units(10),
                units(4)
            ),
            Err(PolicyError::LifetimeCapExceeded)
        );
    }

    #[test]
    fn unresolved_emissions_authorization_fails_closed() {
        assert_eq!(
            EmissionsUnresolved.authorize_shortfall(TARGET_RETAINED_ISSUANCE, 1),
            Err(PolicyError::EmissionsSemanticsUnresolved)
        );
    }

    #[test]
    fn validator_threshold_boundaries_are_exact() {
        assert!(!has_minimum_validator_bond(
            VALIDATOR_MINIMUM_CANDIDACY_BOND - 1
        ));
        assert!(!has_minimum_validator_bond(units(9_999)));
        assert!(has_minimum_validator_bond(VALIDATOR_MINIMUM_CANDIDACY_BOND));
        assert!(has_minimum_validator_bond(
            VALIDATOR_MINIMUM_CANDIDACY_BOND + 1
        ));
    }

    #[test]
    fn validator_eligibility_reacts_to_slash_or_unbond_and_still_requires_keys() {
        let ready = ValidatorCandidacyReadiness {
            active_bond: VALIDATOR_MINIMUM_CANDIDACY_BOND,
            session_keys_registered: true,
        };
        assert!(ready.candidate_requirements_met());
        assert!(!ValidatorCandidacyReadiness {
            active_bond: VALIDATOR_MINIMUM_CANDIDACY_BOND - 1,
            ..ready
        }
        .candidate_requirements_met());
        assert!(!ValidatorCandidacyReadiness {
            active_bond: units(9_999),
            ..ready
        }
        .candidate_requirements_met());
        assert!(!ValidatorCandidacyReadiness {
            session_keys_registered: false,
            ..ready
        }
        .candidate_requirements_met());
        // Candidacy readiness intentionally has no active-seat field or election side effect.
    }

    #[test]
    fn synthetic_migration_reaches_exact_target_and_preserves_third_party_inside_pool() {
        let (mut migration, authorization) = mock_fixture();
        let pre = migration
            .execute(Some(&authorization))
            .expect("authorized synthetic migration");
        assert_eq!(pre.issuance, units(135_000_000));
        assert_eq!(migration.issuance(), Ok(TARGET_RETAINED_ISSUANCE));
        assert_eq!(
            migration
                .accounts
                .iter()
                .find(|account| account.id == 6)
                .unwrap()
                .balance,
            units(1_000_000)
        );
        assert_eq!(
            migration
                .accounts
                .iter()
                .filter(|account| account.pool == Pool::Liquidity)
                .map(|account| account.balance)
                .sum::<Balance>(),
            LIQUIDITY_RESERVE_POOL
        );
    }

    #[test]
    fn synthetic_migration_requires_authorization() {
        let (mut migration, _) = mock_fixture();
        assert_eq!(
            migration.execute(None),
            Err(MockMigrationError::Policy(
                PolicyError::MigrationAuthorizationMissing
            ))
        );
    }

    #[test]
    fn synthetic_migration_rejects_incomplete_vector() {
        let (migration, mut authorization) = mock_fixture();
        authorization.reductions.pop();
        assert_eq!(
            migration.pre_upgrade(Some(&authorization)),
            Err(MockMigrationError::IncompleteDebitVector)
        );
    }

    #[test]
    fn synthetic_migration_rejects_duplicate_accounts() {
        let (migration, mut authorization) = mock_fixture();
        authorization.reductions[4].account = authorization.reductions[0].account;
        assert_eq!(
            migration.pre_upgrade(Some(&authorization)),
            Err(MockMigrationError::DuplicateAccount)
        );
    }

    #[test]
    fn synthetic_migration_rejects_reduction_larger_than_balance() {
        let (migration, mut authorization) = mock_fixture();
        authorization.reductions[0].amount = units(26_000_000);
        assert_eq!(
            migration.pre_upgrade(Some(&authorization)),
            Err(MockMigrationError::ReductionExceedsReducibleBalance)
        );
    }

    #[test]
    fn synthetic_migration_rejects_result_below_target() {
        let (migration, mut authorization) = mock_fixture();
        authorization.reductions[0].amount = units(6_000_000);
        assert_eq!(
            migration.pre_upgrade(Some(&authorization)),
            Err(MockMigrationError::ResultNotTarget)
        );
    }

    #[test]
    fn synthetic_migration_rejects_result_above_target() {
        let (migration, mut authorization) = mock_fixture();
        authorization.reductions[0].amount = units(4_000_000);
        assert_eq!(
            migration.pre_upgrade(Some(&authorization)),
            Err(MockMigrationError::ResultNotTarget)
        );
    }

    #[test]
    fn synthetic_migration_rejects_arithmetic_overflow() {
        let migration = MockMigration {
            storage_version: MOCK_STORAGE_VERSION_BEFORE,
            accounts: vec![
                MockAccount {
                    id: 1,
                    balance: Balance::MAX,
                    pool: Pool::Founding,
                    preserved_third_party: false,
                },
                MockAccount {
                    id: 2,
                    balance: 1,
                    pool: Pool::Founding,
                    preserved_third_party: false,
                },
            ],
        };
        let authorization = MockAuthorization {
            expected_pre_issuance: 0,
            reductions: vec![],
        };
        assert_eq!(
            migration.pre_upgrade(Some(&authorization)),
            Err(MockMigrationError::Policy(PolicyError::ArithmeticOverflow))
        );
    }

    #[test]
    fn synthetic_migration_is_one_time_only() {
        let (mut migration, authorization) = mock_fixture();
        let pre = migration
            .execute(Some(&authorization))
            .expect("first execution");
        assert_eq!(migration.post_upgrade(&pre), Ok(()));
        assert_eq!(
            migration.execute(Some(&authorization)),
            Err(MockMigrationError::AlreadyExecuted)
        );
    }
}
