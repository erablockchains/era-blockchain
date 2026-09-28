//! Private ERA V14 R2 compile-time integration boundaries.
//!
//! This module deliberately defines no FRAME pallet, storage, dispatchable, event, runtime API,
//! genesis value, migration, account, or active economic configuration. It keeps each unresolved
//! surface unconstructable or rejecting while compiling the imported policy crates with the
//! runtime.

#![allow(
    dead_code,
    reason = "R2 adapters remain deliberately dormant until a later checksum-bound owner gate"
)]

use era_v14_application_primitives::amm::{AmmConfig, Error as AmmError, Rate};
use era_v14_custody_governance::{
    verify_foundations, CustodyCategory, FoundationEvidenceVerifier, FoundationInputs, GateError,
    StandardPalletDerivation, VerifiedFoundations, FOUNDER_COUNT,
};
use era_validator_security::{
    prepare_offence_batch, validate_growth, CandidateSnapshot, GrowthApproval, GrowthError,
    GrowthEvidence, OffenceBatch, OffenceError, OffencePolicy, ValidatorPolicy,
};

/// Validate an offence batch against the deliberately unresolved R2 policy.
///
/// This always returns `PolicyUnresolved`; it cannot store a report or invoke Staking.
pub(crate) fn validate_dormant_offence<Id: PartialEq>(
    batch: OffenceBatch<'_, Id>,
) -> Result<(), OffenceError> {
    prepare_offence_batch(OffencePolicy::Unresolved, batch).map(|_| ())
}

/// Validate a growth candidate with every external authorization/evidence input absent.
///
/// Candidate eligibility is still checked first by the policy crate. An eligible candidate then
/// fails with `MissingSeparateAuthorization`, before any runtime count or session operation.
pub(crate) fn validate_unauthorized_growth<Id>(
    policy: &ValidatorPolicy,
    current: u32,
    requested: u32,
    candidate: &CandidateSnapshot<Id>,
) -> Result<GrowthApproval, GrowthError> {
    validate_growth(
        policy,
        current,
        requested,
        candidate,
        GrowthEvidence {
            separately_authorized: false,
            independent_operator_attested: false,
            disposable_finality_proven: false,
            monitoring_and_wallet_ready: false,
            stable_observation_complete: false,
            no_unresolved_security_incident: false,
        },
    )
}

type DormantAccountId = [u8; 32];
type DormantEvidence = [u8; 32];

struct NoCustodyDerivation;

impl StandardPalletDerivation<DormantAccountId, DormantEvidence> for NoCustodyDerivation {
    fn derive_multisig(
        &self,
        _sorted_signatories: &[DormantAccountId; FOUNDER_COUNT],
        _threshold: u16,
    ) -> Option<DormantAccountId> {
        None
    }

    fn derive_pure_proxy(
        &self,
        _multisig_controller: &DormantAccountId,
        _category: CustodyCategory,
        _evidence: &DormantEvidence,
    ) -> Option<DormantAccountId> {
        None
    }
}

struct RejectCustodyEvidence;

impl FoundationEvidenceVerifier<DormantAccountId, DormantEvidence> for RejectCustodyEvidence {
    fn founder_evidence_is_valid(
        &self,
        _account: &DormantAccountId,
        _identity_attestation: &DormantEvidence,
        _control_attestation: &DormantEvidence,
    ) -> bool {
        false
    }

    fn unanimous_custody_attestation_is_valid(
        &self,
        _sorted_signatories: &[DormantAccountId; FOUNDER_COUNT],
        _presale: &DormantAccountId,
        _ecosystem: &DormantAccountId,
        _attestation: &DormantEvidence,
    ) -> bool {
        false
    }

    fn unanimous_vesting_attestation_is_valid(
        &self,
        _sorted_signatories: &[DormantAccountId; FOUNDER_COUNT],
        _beneficiaries: &[era_v14_custody_governance::VestingBeneficiary<DormantAccountId>; 5],
        _attestation: &DormantEvidence,
    ) -> bool {
        false
    }
}

/// Run the custody preflight with no identities or evidence.
///
/// This always stops at `MissingFounderSet`; no account derivation or custody action occurs.
pub(crate) fn validate_dormant_custody() -> Result<VerifiedFoundations<DormantAccountId>, GateError>
{
    let inputs: FoundationInputs<DormantAccountId, DormantEvidence, DormantEvidence> =
        FoundationInputs {
            founders: None,
            founder_evidence: None,
            unanimous_custody_attestation: None,
            presale: None,
            ecosystem: None,
            vesting_beneficiaries: None,
            unanimous_vesting_attestation: None,
        };
    verify_foundations(inputs, &NoCustodyDerivation, &RejectCustodyEvidence)
}

/// Attempt to construct an AMM configuration without an approved LP fee.
///
/// The zero denominator is invalid, so no active `AmmConfig` can be returned.
pub(crate) fn construct_unapproved_amm_config() -> Result<AmmConfig, AmmError> {
    let unresolved_pool_fee = Rate::new(0, 0)?;
    let zero_protocol_share = Rate::new(0, 1)?;
    AmmConfig::new(unresolved_pool_fee, zero_protocol_share, 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use era_validator_security::{
        CandidateIneligibility, ERA_V14_MAX_COMMISSION_PPB, PARTS_PER_BILLION,
    };

    const ETKN: u128 = crate::DECIMALS;

    fn eligible_candidate() -> CandidateSnapshot<u64> {
        CandidateSnapshot {
            validator: 7,
            self_bond: 10_000 * ETKN,
            total_backing: 100_000 * ETKN,
            commission_ppb: ERA_V14_MAX_COMMISSION_PPB,
            session_keys_registered: true,
        }
    }

    #[test]
    fn offence_boundary_is_unresolved_and_cannot_dispatch() {
        let offenders = [7_u64];
        let fractions = [0_u32];
        let batch = OffenceBatch {
            session_index: 1,
            offenders: &offenders,
            slash_fractions_ppb: &fractions,
        };
        assert_eq!(
            validate_dormant_offence(batch),
            Err(OffenceError::PolicyUnresolved)
        );
    }

    #[test]
    fn growth_boundary_rejects_missing_authorization() {
        let policy = ValidatorPolicy::era_v14(ETKN).expect("V14 validator policy is valid");
        assert_eq!(
            validate_unauthorized_growth(&policy, 4, 5, &eligible_candidate()),
            Err(GrowthError::MissingSeparateAuthorization)
        );
    }

    #[test]
    fn growth_boundary_still_rejects_ineligible_candidates_first() {
        let policy = ValidatorPolicy::era_v14(ETKN).expect("V14 validator policy is valid");
        let mut candidate = eligible_candidate();
        candidate.commission_ppb = PARTS_PER_BILLION;
        assert_eq!(
            validate_unauthorized_growth(&policy, 4, 5, &candidate),
            Err(GrowthError::CandidateIneligible(
                CandidateIneligibility::CommissionAboveMaximum
            ))
        );
    }

    #[test]
    fn custody_boundary_rejects_before_derivation() {
        assert_eq!(
            validate_dormant_custody(),
            Err(GateError::MissingFounderSet)
        );
    }

    #[test]
    fn amm_boundary_cannot_construct_unapproved_configuration() {
        assert_eq!(
            construct_unapproved_amm_config(),
            Err(AmmError::InvalidConfiguration)
        );
    }
}
