//! Coordinated dormant V13-to-V14 staking-economics migration.

use crate::v14_migration_lifecycle::{pristine_security_budget, required_version, version};
#[cfg(feature = "try-runtime")]
use alloc::vec::Vec;
#[cfg(feature = "try-runtime")]
use codec::{Decode, Encode};
use frame_support::{
    ensure,
    storage::{with_transaction, TransactionOutcome},
    traits::{fungible::Inspect, OnRuntimeUpgrade, StorageVersion},
    weights::Weight,
};
use sp_runtime::{DispatchError, DispatchResult, Perbill};

use crate::{
    issuance_cap, Balances, BlockNumber, IssuanceCap, RewardReserve, SecurityBudget,
    SecurityBudgetActivationValidatorCount, SecurityBudgetExistingEarnedRewardLiability,
    SecurityBudgetMaxValidatorCommission, SecurityBudgetMinimumValidatorBond,
    SecurityBudgetProtectedCommunityOnboarding, SecurityBudgetRetiredLegacyRewardReserveTarget,
    SecurityBudgetTotalStakingRewards, Session, Staking, System, VERSION,
};

#[cfg(feature = "try-runtime")]
use crate::{AccountId, Balance};

const REWARD_RESERVE_V13_VERSION: StorageVersion = StorageVersion::new(2);
const REWARD_RESERVE_V14_VERSION: StorageVersion = StorageVersion::new(3);
const SECURITY_BUDGET_V14_VERSION: StorageVersion = StorageVersion::new(1);
const ISSUANCE_CAP_VERSION: StorageVersion = StorageVersion::new(1);
const STAKING_VERSION: StorageVersion = StorageVersion::new(16);

pub fn declared_weight() -> Weight {
    use pallet_security_budget::weights::WeightInfo;
    // The generated initialization model accounts for its exact two reads and four writes,
    // including the SecurityBudget storage-version write. Remove those operations from the
    // retained whole-migration envelope before composing it, so no work is counted twice.
    <crate::Runtime as frame_system::Config>::DbWeight::get()
        .reads_writes(54, 10)
        .saturating_add(pallet_security_budget::weights::SubstrateWeight::<
            crate::Runtime,
        >::migration_initialize())
        .saturating_add(crate::v14_migration_lifecycle::pristine_weight())
        .saturating_add(crate::ws3_vesting::declared_weight())
}

fn validate_protocol_accounts() -> DispatchResult {
    ensure!(
        SecurityBudget::accounts_are_distinct(),
        DispatchError::Other("V14 protocol account collision")
    );
    Ok(())
}

fn validate_pristine_staking_pot() -> DispatchResult {
    let staking = SecurityBudget::staking_pot_account();
    ensure!(
        Balances::balance(&staking) == 0,
        DispatchError::Other("V14 staking pot is not empty")
    );
    let info = System::account(staking);
    ensure!(
        info.nonce == 0 && info.consumers == 0 && info.providers == 0 && info.sufficients == 0,
        DispatchError::Other("V14 staking pot system account is not pristine")
    );
    Ok(())
}

fn validate_staking_configuration() -> DispatchResult {
    ensure!(
        pallet_staking::MinValidatorBond::<crate::Runtime>::get()
            <= SecurityBudgetMinimumValidatorBond::get(),
        DispatchError::Other("V14 migration would lower the validator minimum")
    );
    ensure!(
        pallet_staking::MinCommission::<crate::Runtime>::get()
            <= SecurityBudgetMaxValidatorCommission::get(),
        DispatchError::Other("V14 staking minimum commission exceeds 20 percent")
    );
    Ok(())
}

fn validate_active_validators() -> Result<u32, DispatchError> {
    let validators = Session::validators();
    let expected = SecurityBudgetActivationValidatorCount::get();
    ensure!(
        validators.len() as u32 == expected,
        DispatchError::Other("V14 active validator count is not four")
    );
    ensure!(
        pallet_staking::ValidatorCount::<crate::Runtime>::get() == expected,
        DispatchError::Other("V14 configured validator count drift")
    );
    ensure!(
        pallet_staking::MinimumValidatorCount::<crate::Runtime>::get() == expected,
        DispatchError::Other("V14 minimum validator count drift")
    );
    let active = pallet_staking::ActiveEra::<crate::Runtime>::get()
        .ok_or(DispatchError::Other("V14 active era missing"))?;
    ensure!(
        active.start.is_some_and(|start| start > 0),
        DispatchError::Other("V14 active era start missing or zero")
    );
    for (index, validator) in validators.iter().enumerate() {
        ensure!(
            !validators
                .iter()
                .take(index)
                .any(|prior| prior == validator),
            DispatchError::Other("V14 duplicate active validator")
        );
        ensure!(
            pallet_session::NextKeys::<crate::Runtime>::contains_key(validator),
            DispatchError::Other("V14 active validator session keys missing")
        );
        let ledger = pallet_staking::Ledger::<crate::Runtime>::get(validator)
            .ok_or(DispatchError::Other("V14 active validator ledger missing"))?;
        ensure!(
            ledger.active >= SecurityBudgetMinimumValidatorBond::get(),
            DispatchError::Other("V14 active validator ledger below 10000 ETKN")
        );
        let own = if let Some(overview) =
            pallet_staking::ErasStakersOverview::<crate::Runtime>::get(active.index, validator)
        {
            overview.own
        } else {
            ensure!(
                pallet_staking::ErasStakersClipped::<crate::Runtime>::contains_key(
                    active.index,
                    validator
                ),
                DispatchError::Other("V14 active exposure missing")
            );
            pallet_staking::ErasStakersClipped::<crate::Runtime>::get(active.index, validator).own
        };
        ensure!(
            own >= SecurityBudgetMinimumValidatorBond::get(),
            DispatchError::Other("V14 active validator below 10000 ETKN")
        );
        ensure!(
            pallet_staking::ErasValidatorPrefs::<crate::Runtime>::get(active.index, validator)
                .commission
                <= SecurityBudgetMaxValidatorCommission::get(),
            DispatchError::Other("V14 active commission exceeds 20 percent")
        );
        ensure!(
            pallet_staking::Validators::<crate::Runtime>::get(validator).commission
                <= SecurityBudgetMaxValidatorCommission::get(),
            DispatchError::Other("V14 current commission exceeds 20 percent")
        );
    }
    Ok(expected)
}

fn validate_monetary_state() -> DispatchResult {
    ensure!(
        matches!(VERSION.spec_version, 14 | 15),
        DispatchError::Other("V14 migration requires supported runtime spec14 or completion spec15")
    );
    ensure!(
        VERSION.transaction_version == 1,
        DispatchError::Other("V14 transaction version changed")
    );
    ensure!(
        SecurityBudgetTotalStakingRewards::get() == 20_000_000 * crate::DECIMALS,
        DispatchError::Other("V14 total staking reward budget is not 20M ETKN")
    );
    ensure!(
        SecurityBudgetExistingEarnedRewardLiability::get() == 47_913_372_622_298_883_618_387,
        DispatchError::Other("V14 existing earned liability mismatch")
    );
    ensure!(
        SecurityBudgetProtectedCommunityOnboarding::get() == 10_000_000 * crate::DECIMALS,
        DispatchError::Other("V14 protected community allocation is not 10M ETKN")
    );
    ensure!(
        SecurityBudgetRetiredLegacyRewardReserveTarget::get() == 0,
        DispatchError::Other("V14 legacy reward reserve target is not zero")
    );
    ensure!(
        SecurityBudgetTotalStakingRewards::get()
            .checked_sub(SecurityBudgetExistingEarnedRewardLiability::get())
            == Some(19_952_086_627_377_701_116_381_613),
        DispatchError::Other("V14 remaining reward capacity mismatch")
    );
    ensure!(
        issuance_cap::V13MigrationCompleted::<crate::Runtime>::get().is_some(),
        DispatchError::Other("V13 completion marker missing")
    );
    let issuance = Balances::total_issuance();
    let remaining =
        IssuanceCap::remaining_allowance().ok_or(DispatchError::Other("V14 allowance missing"))?;
    ensure!(
        issuance <= crate::upgrade13_policy::ABSOLUTE_LIFETIME_CAP,
        DispatchError::Other("V14 issuance above lifetime cap")
    );
    ensure!(
        remaining <= crate::upgrade13_policy::MAXIMUM_POST_CORRECTION_NEW_ISSUANCE,
        DispatchError::Other("V14 allowance above maximum")
    );
    ensure!(
        issuance
            .checked_add(remaining)
            .ok_or(DispatchError::Other("V14 cap arithmetic overflow"))?
            <= crate::upgrade13_policy::ABSOLUTE_LIFETIME_CAP,
        DispatchError::Other("V14 issuance plus allowance exceeds cap")
    );
    ensure!(
        RewardReserve::pending_collection_obligations() == 0,
        DispatchError::Other("V14 legacy fee or tip obligation pending")
    );
    let legacy_required = RewardReserve::committed_liabilities()
        .checked_add(RewardReserve::reward_pot_floor())
        .ok_or(DispatchError::Other(
            "V14 legacy solvency arithmetic overflow",
        ))?;
    ensure!(
        RewardReserve::pot_balance() >= legacy_required,
        DispatchError::Other("V14 legacy reward pot is insolvent")
    );
    ensure!(
        RewardReserve::ecosystem_treasury_balance() >= RewardReserve::treasury_pot_floor(),
        DispatchError::Other("V14 ecosystem treasury is below its safety floor")
    );
    ensure!(
        RewardReserve::fee_collection_balance() >= RewardReserve::fee_collection_floor(),
        DispatchError::Other("V14 fee collection pot is below its safety floor")
    );
    Ok(())
}

fn validate_pending_versions() -> DispatchResult {
    ensure!(
        required_version::<IssuanceCap>()? == ISSUANCE_CAP_VERSION,
        DispatchError::Other("V14 issuance-cap storage version mismatch")
    );
    ensure!(
        required_version::<RewardReserve>()? == REWARD_RESERVE_V13_VERSION,
        DispatchError::Other("V14 reward-reserve storage version mismatch")
    );
    pristine_security_budget()?;
    ensure!(
        required_version::<Staking>()? == STAKING_VERSION,
        DispatchError::Other("V14 staking storage version mismatch")
    );
    ensure!(
        !RewardReserve::legacy_claim_only(),
        DispatchError::Other("V14 legacy cutoff unexpectedly active before migration")
    );
    ensure!(
        RewardReserve::committed_liabilities()
            == SecurityBudgetExistingEarnedRewardLiability::get(),
        DispatchError::Other("V14 pending legacy liability mismatch")
    );
    ensure!(
        SecurityBudget::legacy_reward_liability() == 0,
        DispatchError::Other("V14 pending security liability is not pristine")
    );
    ensure!(
        !SecurityBudget::active(),
        DispatchError::Other("V14 security budget unexpectedly active before migration")
    );
    Ok(())
}

fn validate_completed() -> DispatchResult {
    ensure!(
        required_version::<IssuanceCap>()? == ISSUANCE_CAP_VERSION,
        DispatchError::Other("V14 completed issuance-cap version mismatch")
    );
    ensure!(
        required_version::<RewardReserve>()? == REWARD_RESERVE_V14_VERSION,
        DispatchError::Other("V14 completed reward-reserve version mismatch")
    );
    ensure!(
        version::<SecurityBudget>()? == Some(SECURITY_BUDGET_V14_VERSION),
        DispatchError::Other("V14 completed security-budget version mismatch")
    );
    ensure!(
        required_version::<Staking>()? == STAKING_VERSION,
        DispatchError::Other("V14 completed staking version mismatch")
    );
    ensure!(
        pallet_staking::MinValidatorBond::<crate::Runtime>::get()
            == SecurityBudgetMinimumValidatorBond::get(),
        DispatchError::Other("V14 minimum validator bond mismatch")
    );
    ensure!(
        SecurityBudget::migration_completed_at().is_some(),
        DispatchError::Other("V14 migration marker missing")
    );
    ensure!(
        SecurityBudget::legacy_reward_liability()
            == SecurityBudgetExistingEarnedRewardLiability::get(),
        DispatchError::Other("V14 migrated legacy liability mismatch")
    );
    ensure!(
        RewardReserve::legacy_claim_only()
            && RewardReserve::v14_legacy_cutoff_era().is_some()
            && !RewardReserve::fee_routing_active(),
        DispatchError::Other("V14 legacy reward allocation is not retired")
    );
    ensure!(
        RewardReserve::committed_liabilities() <= SecurityBudget::legacy_reward_liability(),
        DispatchError::Other("V14 legacy liability increased after reconciliation")
    );
    ensure!(
        SecurityBudget::accounted_reward_total()? <= SecurityBudgetTotalStakingRewards::get(),
        DispatchError::Other("V14 staking reward cap exceeded")
    );
    if !SecurityBudget::active() {
        ensure!(
            SecurityBudget::remaining_reward_budget()?
                == SecurityBudgetTotalStakingRewards::get()
                    - SecurityBudgetExistingEarnedRewardLiability::get(),
            DispatchError::Other("V14 legacy liability was not deducted exactly once")
        );
    }
    let staking_pot_info = System::account(SecurityBudget::staking_pot_account());
    ensure!(
        staking_pot_info.providers > 0,
        DispatchError::Other("V14 staking pot provider missing")
    );
    if !SecurityBudget::active() {
        ensure!(
            staking_pot_info.providers == 1
                && staking_pot_info.consumers == 0
                && staking_pot_info.sufficients == 0,
            DispatchError::Other("V14 staking pot provider state is not exact")
        );
    }
    validate_monetary_state()?;
    validate_protocol_accounts()?;
    validate_staking_configuration()?;
    validate_active_validators()?;
    Ok(())
}

fn execute() -> Result<Weight, DispatchError> {
    let completed = version::<SecurityBudget>()? == Some(SECURITY_BUDGET_V14_VERSION)
        && required_version::<RewardReserve>()? == REWARD_RESERVE_V14_VERSION;
    if completed {
        validate_completed()?;
        crate::ws3_vesting::restore_locks()?;
        return Ok(<crate::Runtime as frame_system::Config>::DbWeight::get()
            .reads(32)
            .saturating_add(crate::ws3_vesting::declared_weight()));
    }

    validate_pending_versions()?;
    crate::ws3_vesting::restore_locks()?;
    validate_monetary_state()?;
    validate_protocol_accounts()?;
    validate_pristine_staking_pot()?;
    validate_staking_configuration()?;
    let active_count = validate_active_validators()?;
    let legacy_cutoff = pallet_staking::ActiveEra::<crate::Runtime>::get()
        .ok_or(DispatchError::Other("V14 active era missing"))?
        .index;

    let staking_pot = SecurityBudget::staking_pot_account();
    if System::account(&staking_pot).providers == 0 {
        System::inc_providers(&staking_pot);
    }
    pallet_staking::MinValidatorBond::<crate::Runtime>::put(
        SecurityBudgetMinimumValidatorBond::get(),
    );
    REWARD_RESERVE_V14_VERSION.put::<RewardReserve>();
    SECURITY_BUDGET_V14_VERSION.put::<SecurityBudget>();
    RewardReserve::enter_v14_claim_only(legacy_cutoff)?;
    SecurityBudget::record_migration_completed(
        active_count,
        SecurityBudgetExistingEarnedRewardLiability::get(),
    )?;

    validate_completed()?;
    Ok(declared_weight())
}

pub struct V14Migration;

impl OnRuntimeUpgrade for V14Migration {
    fn on_runtime_upgrade() -> Weight {
        if crate::fresh_genesis::Enabled::<crate::Runtime>::get() {
            crate::fresh_genesis::validate_lifecycle();
            return <crate::Runtime as frame_system::Config>::DbWeight::get().reads(12);
        }
        with_transaction(|| {
            let result = execute();
            if result.is_ok() {
                TransactionOutcome::Commit(result)
            } else {
                TransactionOutcome::Rollback(result)
            }
        })
        .unwrap_or_else(|error| panic!("V14 migration failed and rolled back: {error:?}"))
    }

    #[cfg(feature = "try-runtime")]
    fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
        if crate::fresh_genesis::Enabled::<crate::Runtime>::get() {
            crate::fresh_genesis::validate_lifecycle();
            return Ok(b"ERA_FRESH_V14_GENESIS".to_vec());
        }
        if version::<SecurityBudget>()? == Some(SECURITY_BUDGET_V14_VERSION)
            && required_version::<RewardReserve>()? == REWARD_RESERVE_V14_VERSION
        {
            validate_completed().map_err(|_| "V14 completed pre-upgrade validation failed")?;
        } else {
            validate_pending_versions().map_err(|_| "V14 pre-upgrade version validation failed")?;
            validate_monetary_state().map_err(|_| "V14 pre-upgrade monetary validation failed")?;
            validate_protocol_accounts()
                .map_err(|_| "V14 pre-upgrade account validation failed")?;
            validate_pristine_staking_pot()
                .map_err(|_| "V14 pre-upgrade staking-pot state validation failed")?;
            validate_staking_configuration()
                .map_err(|_| "V14 pre-upgrade staking configuration validation failed")?;
            validate_active_validators()
                .map_err(|_| "V14 pre-upgrade validator validation failed")?;
        }
        let state = TryState {
            vesting: crate::ws3_vesting::preserved_state()?,
            issuance: Balances::total_issuance(),
            allowance: IssuanceCap::remaining_allowance(),
            legacy_balance: RewardReserve::pot_balance(),
            legacy_liabilities: RewardReserve::committed_liabilities(),
            treasury_balance: RewardReserve::ecosystem_treasury_balance(),
            collection_balance: RewardReserve::fee_collection_balance(),
            active_era: pallet_staking::ActiveEra::<crate::Runtime>::get(),
            validators: Session::validators(),
            validator_count: pallet_staking::ValidatorCount::<crate::Runtime>::get(),
            minimum_validator_count: pallet_staking::MinimumValidatorCount::<crate::Runtime>::get(),
            min_validator_bond: pallet_staking::MinValidatorBond::<crate::Runtime>::get(),
        };
        Ok(state.encode())
    }

    #[cfg(feature = "try-runtime")]
    fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
        if crate::fresh_genesis::Enabled::<crate::Runtime>::get() {
            if state != b"ERA_FRESH_V14_GENESIS" {
                return Err("fresh-chain upgrade pre-state mismatch".into());
            }
            crate::fresh_genesis::validate_lifecycle();
            return Ok(());
        }
        let before =
            TryState::decode(&mut &state[..]).map_err(|_| "invalid V14 pre-upgrade state")?;
        validate_completed().map_err(|_| "V14 post-upgrade validation failed")?;
        crate::ws3_vesting::validate_locks()?;
        if crate::ws3_vesting::preserved_state()? != before.vesting {
            return Err(
                "WS3 migration changed schedules or preserved balances/restrictions".into(),
            );
        }
        if Balances::total_issuance() != before.issuance
            || IssuanceCap::remaining_allowance() != before.allowance
            || RewardReserve::pot_balance() != before.legacy_balance
            || RewardReserve::committed_liabilities() != before.legacy_liabilities
            || RewardReserve::ecosystem_treasury_balance() != before.treasury_balance
            || RewardReserve::fee_collection_balance() != before.collection_balance
            || pallet_staking::ActiveEra::<crate::Runtime>::get() != before.active_era
            || Session::validators() != before.validators
            || pallet_staking::ValidatorCount::<crate::Runtime>::get() != before.validator_count
            || pallet_staking::MinimumValidatorCount::<crate::Runtime>::get()
                != before.minimum_validator_count
        {
            return Err(
                "V14 migration changed preserved monetary, staking, session, or validator state"
                    .into(),
            );
        }
        if before.min_validator_bond > SecurityBudgetMinimumValidatorBond::get() {
            return Err("V14 migration lowered the validator minimum".into());
        }
        Ok(())
    }
}

#[cfg(feature = "try-runtime")]
#[derive(Encode, Decode)]
struct TryState {
    vesting: Vec<u8>,
    issuance: Balance,
    allowance: Option<Balance>,
    legacy_balance: Balance,
    legacy_liabilities: Balance,
    treasury_balance: Balance,
    collection_balance: Balance,
    active_era: Option<pallet_staking::ActiveEraInfo>,
    validators: Vec<AccountId>,
    validator_count: u32,
    minimum_validator_count: u32,
    min_validator_bond: Balance,
}

#[allow(dead_code)]
fn _approved_parameters_are_exact() {
    let _ = (BlockNumber::default(), Perbill::from_percent(20));
}
