//! ERA V14 founding-custody, vesting, and future-governance controls.
//!
//! The active pallet holds presale, ecosystem, and liquidity allocations in three deterministic,
//! keyless, domain-separated accounts. A withdrawal executes only after the same three configured
//! founder accounts approve identical request bytes. Legacy pure preflight helpers below remain for
//! historical evidence and vesting arithmetic; they are not the active custody authority.

#![cfg_attr(not(feature = "std"), no_std)]
#![deny(unsafe_code)]

extern crate alloc;

mod active;
pub use active::*;
pub mod weights;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_runtime::RuntimeDebug;

pub const FOUNDER_COUNT: usize = 3;
pub const REQUIRED_CUSTODY_THRESHOLD: u16 = 3;
pub const FOUNDING_ALLOCATION_COUNT: usize = 5;
pub const TOKEN_BASE_UNITS: u128 = 1_000_000_000_000_000_000;
pub const FOUNDING_VESTING_RELEASE_INTERVALS: u32 = 5_256_000;
pub const FOUNDING_ALLOCATION_TARGETS: [u128; FOUNDING_ALLOCATION_COUNT] = [
    8_000_000 * TOKEN_BASE_UNITS,
    5_000_000 * TOKEN_BASE_UNITS,
    3_000_000 * TOKEN_BASE_UNITS,
    2_000_000 * TOKEN_BASE_UNITS,
    2_000_000 * TOKEN_BASE_UNITS,
];
pub const FOUNDING_VESTING_TOTAL: u128 = 20_000_000 * TOKEN_BASE_UNITS;

#[derive(
    Encode,
    Decode,
    DecodeWithMemTracking,
    Clone,
    Copy,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    RuntimeDebug,
    TypeInfo,
    MaxEncodedLen,
)]
pub enum CustodyCategory {
    Presale,
    Ecosystem,
    Liquidity,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MigrationDisposition {
    Apply,
    ResumePrepared,
    AlreadyAppliedNoop,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SudoDisposition {
    PreserveOperational,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MigrationMarker<PlanId> {
    Dormant,
    Prepared(PlanId),
    Applied(PlanId),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GateError {
    MissingFounderSet,
    WrongThreshold,
    DuplicateFounder,
    NonCanonicalFounderOrder,
    MissingFounderEvidence,
    FounderEvidenceAccountMismatch,
    InvalidFounderEvidence,
    MissingCustodyAttestation,
    InvalidCustodyAttestation,
    MissingCustody(CustodyCategory),
    WrongCustodyCategory,
    WrongMultisigController,
    WrongPureProxyAccount,
    CustodyAccountsNotSeparate,
    CustodyAccountConflictsWithController,
    CustodyAccountConflictsWithFounder,
    SingleSignerBypass,
    AlternateDelegatePresent,
    MissingVestingBeneficiaries,
    WrongVestingAllocation,
    DuplicateVestingBeneficiary,
    MissingVestingAttestation,
    InvalidVestingAttestation,
    VestingArithmeticOverflow,
    VestingInvariantFailure,
    MissingExecutionArtifacts,
    SudoNotOperational,
    WrongExecutionDestination,
    WrongExecutionVestingAccount,
    SourceAccountsNotSeparate,
    SourceDestinationConflict,
    InvalidExecutionArtifacts,
    ReplayConflict,
    MissingFoundationIdentity,
    FoundationIdentityConflict,
    MissingGovernanceDesignEvidence,
    MissingGovernanceAuditEvidence,
    MissingGovernanceTestEvidence,
    MissingGovernanceRecoveryEvidence,
    MissingGovernanceOwnerAttestation,
    InvalidGovernanceEvidence,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FounderSet<AccountId> {
    pub signatories: [AccountId; FOUNDER_COUNT],
    pub threshold: u16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FounderEvidence<AccountId, Evidence> {
    pub account: AccountId,
    pub identity_attestation: Evidence,
    pub control_attestation: Evidence,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CustodyDerivation<AccountId, ProxyEvidence> {
    pub category: CustodyCategory,
    pub multisig_controller: AccountId,
    pub pure_proxy_account: AccountId,
    /// Exact standard-pallet creation coordinates and proxy type are supplied by integration.
    pub proxy_evidence: ProxyEvidence,
    /// A standard pure proxy must have no alternate delegate capable of category calls.
    pub alternate_delegate_count: u32,
    /// Must remain false. This makes a direct founder/delegate path fail closed.
    pub single_signer_path: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VestingBeneficiary<AccountId> {
    pub account: AccountId,
    pub allocation: u128,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FoundationInputs<AccountId, ProxyEvidence, Evidence> {
    pub founders: Option<FounderSet<AccountId>>,
    pub founder_evidence: Option<[FounderEvidence<AccountId, Evidence>; FOUNDER_COUNT]>,
    pub unanimous_custody_attestation: Option<Evidence>,
    pub presale: Option<CustodyDerivation<AccountId, ProxyEvidence>>,
    pub ecosystem: Option<CustodyDerivation<AccountId, ProxyEvidence>>,
    pub vesting_beneficiaries: Option<[VestingBeneficiary<AccountId>; FOUNDING_ALLOCATION_COUNT]>,
    pub unanimous_vesting_attestation: Option<Evidence>,
}

/// Adapter boundary for the audited SDK account derivations.
///
/// Integration must implement this with `pallet_multisig::Pallet::multi_account_id` and
/// `pallet_proxy::Pallet::pure_account`; it must not substitute proprietary cryptography.
pub trait StandardPalletDerivation<AccountId, ProxyEvidence> {
    fn derive_multisig(
        &self,
        sorted_signatories: &[AccountId; FOUNDER_COUNT],
        threshold: u16,
    ) -> Option<AccountId>;

    fn derive_pure_proxy(
        &self,
        multisig_controller: &AccountId,
        category: CustodyCategory,
        evidence: &ProxyEvidence,
    ) -> Option<AccountId>;
}

/// Evidence verification remains an integration concern and must bind real owner-supplied data.
pub trait FoundationEvidenceVerifier<AccountId, Evidence> {
    fn founder_evidence_is_valid(
        &self,
        account: &AccountId,
        identity_attestation: &Evidence,
        control_attestation: &Evidence,
    ) -> bool;

    fn unanimous_custody_attestation_is_valid(
        &self,
        sorted_signatories: &[AccountId; FOUNDER_COUNT],
        presale: &AccountId,
        ecosystem: &AccountId,
        attestation: &Evidence,
    ) -> bool;

    fn unanimous_vesting_attestation_is_valid(
        &self,
        sorted_signatories: &[AccountId; FOUNDER_COUNT],
        beneficiaries: &[VestingBeneficiary<AccountId>; FOUNDING_ALLOCATION_COUNT],
        attestation: &Evidence,
    ) -> bool;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedFoundations<AccountId> {
    pub founders: [AccountId; FOUNDER_COUNT],
    pub multisig_controller: AccountId,
    pub presale_custody: AccountId,
    pub ecosystem_custody: AccountId,
    pub vesting_beneficiaries: [VestingBeneficiary<AccountId>; FOUNDING_ALLOCATION_COUNT],
    /// True only for the scoped standard-pallet custody paths. Root/Sudo is reported separately.
    pub no_single_founder_custody_path: bool,
    pub sudo_disposition: SudoDisposition,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LinearVestingTerms {
    pub locked: u128,
    pub release_intervals: u32,
    pub floor_release_per_interval: u128,
    pub final_residual: u128,
    pub final_interval_release: u128,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VestingScheduleTerms {
    pub locked: u128,
    pub per_block: u128,
    pub starting_block: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExactVestingSchedules {
    pub schedule_a: VestingScheduleTerms,
    pub schedule_b: VestingScheduleTerms,
    pub completion_block: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CustodyExecutionArtifacts<AccountId, Hash, Balance, Evidence> {
    pub plan_id: Hash,
    pub presale_source: AccountId,
    pub ecosystem_source: AccountId,
    pub presale_destination: AccountId,
    pub ecosystem_destination: AccountId,
    pub vesting_accounts: [AccountId; FOUNDING_ALLOCATION_COUNT],
    pub vesting_start_block: u32,
    pub presale_call_hash: Hash,
    pub ecosystem_call_hash: Hash,
    pub runtime_code_hash: Hash,
    pub metadata_hash: Hash,
    pub balance_snapshot_hash: Hash,
    pub deposit_schedule_hash: Hash,
    pub presale_balance: Balance,
    pub ecosystem_balance: Balance,
    pub multisig_deposit: Balance,
    pub presale_proxy_deposit: Balance,
    pub ecosystem_proxy_deposit: Balance,
    pub unanimous_final_authorization: Evidence,
}

pub trait ExecutionEvidenceVerifier<AccountId, Hash, Balance, Evidence> {
    fn execution_artifacts_are_valid(
        &self,
        founders: &[AccountId; FOUNDER_COUNT],
        artifacts: &CustodyExecutionArtifacts<AccountId, Hash, Balance, Evidence>,
    ) -> bool;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedMigrationPlan<AccountId, Hash, Balance> {
    pub disposition: MigrationDisposition,
    pub plan_id: Hash,
    pub presale_source: AccountId,
    pub ecosystem_source: AccountId,
    pub presale_destination: AccountId,
    pub ecosystem_destination: AccountId,
    pub presale_call_hash: Hash,
    pub ecosystem_call_hash: Hash,
    pub presale_balance: Balance,
    pub ecosystem_balance: Balance,
    pub multisig_deposit: Balance,
    pub presale_proxy_deposit: Balance,
    pub ecosystem_proxy_deposit: Balance,
    pub vesting_schedules: [ExactVestingSchedules; FOUNDING_ALLOCATION_COUNT],
    pub sudo_disposition: SudoDisposition,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FutureGovernanceInputs<AccountId, Evidence> {
    pub foundation_account: Option<AccountId>,
    pub governance_design_evidence: Option<Evidence>,
    pub independent_audit_evidence: Option<Evidence>,
    pub disposable_test_evidence: Option<Evidence>,
    pub recovery_rehearsal_evidence: Option<Evidence>,
    pub unanimous_owner_attestation: Option<Evidence>,
}

pub trait GovernanceEvidenceVerifier<AccountId, Evidence> {
    fn future_governance_evidence_is_valid(
        &self,
        founders: &[AccountId; FOUNDER_COUNT],
        inputs: &FutureGovernanceInputs<AccountId, Evidence>,
    ) -> bool;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparedFutureHandover<AccountId> {
    pub foundation_account: AccountId,
    /// WS3 only prepares evidence. It does not remove, transfer, or disable Sudo.
    pub sudo_disposition: SudoDisposition,
    /// Always false in this bounded workstream; activation needs separate authorization/wiring.
    pub activation_authorized_by_ws3: bool,
}

fn contains_duplicate<T: Eq, const N: usize>(values: &[T; N]) -> bool {
    for index in 0..N {
        for other in (index + 1)..N {
            if values[index] == values[other] {
                return true;
            }
        }
    }
    false
}

fn contains<T: Eq, const N: usize>(values: &[T; N], needle: &T) -> bool {
    values.iter().any(|value| value == needle)
}

pub fn linear_vesting_terms(locked: u128) -> Result<LinearVestingTerms, GateError> {
    let intervals = u128::from(FOUNDING_VESTING_RELEASE_INTERVALS);
    let floor_release_per_interval = locked / intervals;
    let final_residual = locked % intervals;
    let final_interval_release = floor_release_per_interval
        .checked_add(final_residual)
        .ok_or(GateError::VestingArithmeticOverflow)?;
    let reconstructed = floor_release_per_interval
        .checked_mul(intervals)
        .and_then(|value| value.checked_add(final_residual))
        .ok_or(GateError::VestingArithmeticOverflow)?;
    if locked == 0 || floor_release_per_interval == 0 || reconstructed != locked {
        return Err(GateError::VestingInvariantFailure);
    }
    Ok(LinearVestingTerms {
        locked,
        release_intervals: FOUNDING_VESTING_RELEASE_INTERVALS,
        floor_release_per_interval,
        final_residual,
        final_interval_release,
    })
}

/// Build the two standard `pallet_vesting` schedules needed to preserve the exact final residual.
pub fn exact_vesting_schedules(
    locked: u128,
    starting_block: u32,
) -> Result<ExactVestingSchedules, GateError> {
    let terms = linear_vesting_terms(locked)?;
    let normal_locked = terms
        .floor_release_per_interval
        .checked_mul(u128::from(terms.release_intervals))
        .ok_or(GateError::VestingArithmeticOverflow)?;
    let final_start = starting_block
        .checked_add(
            terms
                .release_intervals
                .checked_sub(1)
                .ok_or(GateError::VestingArithmeticOverflow)?,
        )
        .ok_or(GateError::VestingArithmeticOverflow)?;
    let completion_block = starting_block
        .checked_add(terms.release_intervals)
        .ok_or(GateError::VestingArithmeticOverflow)?;
    if normal_locked
        .checked_add(terms.final_residual)
        .ok_or(GateError::VestingArithmeticOverflow)?
        != locked
    {
        return Err(GateError::VestingInvariantFailure);
    }
    Ok(ExactVestingSchedules {
        schedule_a: VestingScheduleTerms {
            locked: normal_locked,
            per_block: terms.floor_release_per_interval,
            starting_block,
        },
        schedule_b: VestingScheduleTerms {
            locked: terms.final_residual,
            per_block: terms.final_residual,
            starting_block: final_start,
        },
        completion_block,
    })
}

pub fn verify_foundations<AccountId, ProxyEvidence, Evidence, Derivation, Verifier>(
    inputs: FoundationInputs<AccountId, ProxyEvidence, Evidence>,
    derivation: &Derivation,
    verifier: &Verifier,
) -> Result<VerifiedFoundations<AccountId>, GateError>
where
    AccountId: Clone + Eq + Ord,
    Derivation: StandardPalletDerivation<AccountId, ProxyEvidence>,
    Verifier: FoundationEvidenceVerifier<AccountId, Evidence>,
{
    let founders = inputs.founders.ok_or(GateError::MissingFounderSet)?;
    if founders.threshold != REQUIRED_CUSTODY_THRESHOLD {
        return Err(GateError::WrongThreshold);
    }
    if contains_duplicate(&founders.signatories) {
        return Err(GateError::DuplicateFounder);
    }
    if !(founders.signatories[0] < founders.signatories[1]
        && founders.signatories[1] < founders.signatories[2])
    {
        return Err(GateError::NonCanonicalFounderOrder);
    }
    let sorted_founders = founders.signatories;

    let founder_evidence = inputs
        .founder_evidence
        .ok_or(GateError::MissingFounderEvidence)?;
    for (expected, evidence) in sorted_founders.iter().zip(founder_evidence.iter()) {
        if expected != &evidence.account {
            return Err(GateError::FounderEvidenceAccountMismatch);
        }
        if !verifier.founder_evidence_is_valid(
            &evidence.account,
            &evidence.identity_attestation,
            &evidence.control_attestation,
        ) {
            return Err(GateError::InvalidFounderEvidence);
        }
    }

    let presale = inputs
        .presale
        .ok_or(GateError::MissingCustody(CustodyCategory::Presale))?;
    let ecosystem = inputs
        .ecosystem
        .ok_or(GateError::MissingCustody(CustodyCategory::Ecosystem))?;
    if presale.category != CustodyCategory::Presale
        || ecosystem.category != CustodyCategory::Ecosystem
    {
        return Err(GateError::WrongCustodyCategory);
    }
    if presale.single_signer_path || ecosystem.single_signer_path {
        return Err(GateError::SingleSignerBypass);
    }
    if presale.alternate_delegate_count != 0 || ecosystem.alternate_delegate_count != 0 {
        return Err(GateError::AlternateDelegatePresent);
    }

    let expected_controller = derivation
        .derive_multisig(&sorted_founders, REQUIRED_CUSTODY_THRESHOLD)
        .ok_or(GateError::WrongMultisigController)?;
    if presale.multisig_controller != expected_controller
        || ecosystem.multisig_controller != expected_controller
    {
        return Err(GateError::WrongMultisigController);
    }
    let expected_presale = derivation
        .derive_pure_proxy(
            &expected_controller,
            CustodyCategory::Presale,
            &presale.proxy_evidence,
        )
        .ok_or(GateError::WrongPureProxyAccount)?;
    let expected_ecosystem = derivation
        .derive_pure_proxy(
            &expected_controller,
            CustodyCategory::Ecosystem,
            &ecosystem.proxy_evidence,
        )
        .ok_or(GateError::WrongPureProxyAccount)?;
    if presale.pure_proxy_account != expected_presale
        || ecosystem.pure_proxy_account != expected_ecosystem
    {
        return Err(GateError::WrongPureProxyAccount);
    }
    if expected_presale == expected_ecosystem {
        return Err(GateError::CustodyAccountsNotSeparate);
    }
    if expected_presale == expected_controller || expected_ecosystem == expected_controller {
        return Err(GateError::CustodyAccountConflictsWithController);
    }
    if contains(&sorted_founders, &expected_presale)
        || contains(&sorted_founders, &expected_ecosystem)
    {
        return Err(GateError::CustodyAccountConflictsWithFounder);
    }

    let custody_attestation = inputs
        .unanimous_custody_attestation
        .ok_or(GateError::MissingCustodyAttestation)?;
    if !verifier.unanimous_custody_attestation_is_valid(
        &sorted_founders,
        &expected_presale,
        &expected_ecosystem,
        &custody_attestation,
    ) {
        return Err(GateError::InvalidCustodyAttestation);
    }

    let vesting_beneficiaries = inputs
        .vesting_beneficiaries
        .ok_or(GateError::MissingVestingBeneficiaries)?;
    for (beneficiary, expected_allocation) in vesting_beneficiaries
        .iter()
        .zip(FOUNDING_ALLOCATION_TARGETS.iter())
    {
        if beneficiary.allocation != *expected_allocation {
            return Err(GateError::WrongVestingAllocation);
        }
        linear_vesting_terms(beneficiary.allocation)?;
    }
    let vesting_accounts = vesting_beneficiaries
        .each_ref()
        .map(|beneficiary| &beneficiary.account);
    if contains_duplicate(&vesting_accounts) {
        return Err(GateError::DuplicateVestingBeneficiary);
    }
    let vesting_total = vesting_beneficiaries
        .iter()
        .try_fold(0_u128, |total, beneficiary| {
            total
                .checked_add(beneficiary.allocation)
                .ok_or(GateError::VestingArithmeticOverflow)
        })?;
    if vesting_total != FOUNDING_VESTING_TOTAL {
        return Err(GateError::VestingInvariantFailure);
    }
    let vesting_attestation = inputs
        .unanimous_vesting_attestation
        .ok_or(GateError::MissingVestingAttestation)?;
    if !verifier.unanimous_vesting_attestation_is_valid(
        &sorted_founders,
        &vesting_beneficiaries,
        &vesting_attestation,
    ) {
        return Err(GateError::InvalidVestingAttestation);
    }

    Ok(VerifiedFoundations {
        founders: sorted_founders,
        multisig_controller: expected_controller,
        presale_custody: expected_presale,
        ecosystem_custody: expected_ecosystem,
        vesting_beneficiaries,
        no_single_founder_custody_path: true,
        sudo_disposition: SudoDisposition::PreserveOperational,
    })
}

fn migration_disposition<PlanId: Eq>(
    marker: &MigrationMarker<PlanId>,
    plan_id: &PlanId,
) -> Result<MigrationDisposition, GateError> {
    match marker {
        MigrationMarker::Dormant => Ok(MigrationDisposition::Apply),
        MigrationMarker::Prepared(existing) if existing == plan_id => {
            Ok(MigrationDisposition::ResumePrepared)
        }
        MigrationMarker::Applied(existing) if existing == plan_id => {
            Ok(MigrationDisposition::AlreadyAppliedNoop)
        }
        MigrationMarker::Prepared(_) | MigrationMarker::Applied(_) => {
            Err(GateError::ReplayConflict)
        }
    }
}

pub fn verify_migration_ready<AccountId, Hash, Balance, Evidence, Verifier>(
    foundations: &VerifiedFoundations<AccountId>,
    marker: &MigrationMarker<Hash>,
    sudo_operational: bool,
    artifacts: Option<CustodyExecutionArtifacts<AccountId, Hash, Balance, Evidence>>,
    verifier: &Verifier,
) -> Result<VerifiedMigrationPlan<AccountId, Hash, Balance>, GateError>
where
    AccountId: Clone + Eq,
    Hash: Clone + Eq,
    Balance: Clone,
    Verifier: ExecutionEvidenceVerifier<AccountId, Hash, Balance, Evidence>,
{
    if !sudo_operational {
        return Err(GateError::SudoNotOperational);
    }
    let artifacts = artifacts.ok_or(GateError::MissingExecutionArtifacts)?;
    if artifacts.presale_destination != foundations.presale_custody
        || artifacts.ecosystem_destination != foundations.ecosystem_custody
    {
        return Err(GateError::WrongExecutionDestination);
    }
    if artifacts.presale_source == artifacts.ecosystem_source {
        return Err(GateError::SourceAccountsNotSeparate);
    }
    if artifacts.presale_source == artifacts.presale_destination
        || artifacts.presale_source == artifacts.ecosystem_destination
        || artifacts.ecosystem_source == artifacts.presale_destination
        || artifacts.ecosystem_source == artifacts.ecosystem_destination
    {
        return Err(GateError::SourceDestinationConflict);
    }
    for (artifact_account, beneficiary) in artifacts
        .vesting_accounts
        .iter()
        .zip(foundations.vesting_beneficiaries.iter())
    {
        if artifact_account != &beneficiary.account {
            return Err(GateError::WrongExecutionVestingAccount);
        }
    }
    if !verifier.execution_artifacts_are_valid(&foundations.founders, &artifacts) {
        return Err(GateError::InvalidExecutionArtifacts);
    }
    let disposition = migration_disposition(marker, &artifacts.plan_id)?;
    let mut vesting_schedules = [exact_vesting_schedules(
        FOUNDING_ALLOCATION_TARGETS[0],
        artifacts.vesting_start_block,
    )?; FOUNDING_ALLOCATION_COUNT];
    for (slot, beneficiary) in vesting_schedules
        .iter_mut()
        .zip(foundations.vesting_beneficiaries.iter())
    {
        *slot = exact_vesting_schedules(beneficiary.allocation, artifacts.vesting_start_block)?;
    }

    Ok(VerifiedMigrationPlan {
        disposition,
        plan_id: artifacts.plan_id,
        presale_source: artifacts.presale_source,
        ecosystem_source: artifacts.ecosystem_source,
        presale_destination: artifacts.presale_destination,
        ecosystem_destination: artifacts.ecosystem_destination,
        presale_call_hash: artifacts.presale_call_hash,
        ecosystem_call_hash: artifacts.ecosystem_call_hash,
        presale_balance: artifacts.presale_balance,
        ecosystem_balance: artifacts.ecosystem_balance,
        multisig_deposit: artifacts.multisig_deposit,
        presale_proxy_deposit: artifacts.presale_proxy_deposit,
        ecosystem_proxy_deposit: artifacts.ecosystem_proxy_deposit,
        vesting_schedules,
        sudo_disposition: SudoDisposition::PreserveOperational,
    })
}

/// Apply a caller-supplied state transition to a clone and commit only after post-validation.
///
/// Runtime integration must additionally use FRAME transactional storage semantics. This helper
/// exists to make rollback behavior explicit and directly testable without touching chain state.
pub fn transactional_apply<State, Error, Apply, PostValidate>(
    state: &mut State,
    apply: Apply,
    post_validate: PostValidate,
) -> Result<(), Error>
where
    State: Clone,
    Apply: FnOnce(&mut State) -> Result<(), Error>,
    PostValidate: FnOnce(&State) -> Result<(), Error>,
{
    let mut candidate = state.clone();
    apply(&mut candidate)?;
    post_validate(&candidate)?;
    *state = candidate;
    Ok(())
}

pub fn prepare_future_handover<AccountId, Evidence, Verifier>(
    foundations: &VerifiedFoundations<AccountId>,
    sudo_operational: bool,
    inputs: FutureGovernanceInputs<AccountId, Evidence>,
    verifier: &Verifier,
) -> Result<PreparedFutureHandover<AccountId>, GateError>
where
    AccountId: Clone + Eq,
    Verifier: GovernanceEvidenceVerifier<AccountId, Evidence>,
{
    if !sudo_operational {
        return Err(GateError::SudoNotOperational);
    }
    let foundation_account = inputs
        .foundation_account
        .as_ref()
        .ok_or(GateError::MissingFoundationIdentity)?;
    if contains(&foundations.founders, foundation_account)
        || foundation_account == &foundations.multisig_controller
        || foundation_account == &foundations.presale_custody
        || foundation_account == &foundations.ecosystem_custody
    {
        return Err(GateError::FoundationIdentityConflict);
    }
    inputs
        .governance_design_evidence
        .as_ref()
        .ok_or(GateError::MissingGovernanceDesignEvidence)?;
    inputs
        .independent_audit_evidence
        .as_ref()
        .ok_or(GateError::MissingGovernanceAuditEvidence)?;
    inputs
        .disposable_test_evidence
        .as_ref()
        .ok_or(GateError::MissingGovernanceTestEvidence)?;
    inputs
        .recovery_rehearsal_evidence
        .as_ref()
        .ok_or(GateError::MissingGovernanceRecoveryEvidence)?;
    inputs
        .unanimous_owner_attestation
        .as_ref()
        .ok_or(GateError::MissingGovernanceOwnerAttestation)?;
    if !verifier.future_governance_evidence_is_valid(&foundations.founders, &inputs) {
        return Err(GateError::InvalidGovernanceEvidence);
    }
    Ok(PreparedFutureHandover {
        foundation_account: foundation_account.clone(),
        sudo_disposition: SudoDisposition::PreserveOperational,
        activation_authorized_by_ws3: false,
    })
}

#[cfg(test)]
mod tests;
