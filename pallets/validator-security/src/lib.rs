//! ERA V14 validator-security policy.
//!
//! This module is deliberately free of storage, dispatchables, account identities, and policy
//! defaults for offences. It gives the integration runtime deterministic, bounded validation
//! before it delegates elections and slashing to the pinned Polkadot SDK staking, session, and
//! offences pallets. Invalid input never produces an action, so callers can validate before any
//! runtime write.

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
pub mod observation;

pub mod weights;

/// Denominator used by `sp_runtime::Perbill` and by every fraction in this module.
pub const PARTS_PER_BILLION: u32 = 1_000_000_000;

/// Owner-approved maximum validator commission (20%).
pub const ERA_V14_MAX_COMMISSION_PPB: u32 = 200_000_000;

/// Validator count retained by migration and initial V14 activation.
pub const ERA_V14_ACTIVATION_VALIDATORS: u32 = 4;

/// Bootstrap milestone. It is a milestone, not a cap.
pub const ERA_V14_BOOTSTRAP_MILESTONE: u32 = 7;

/// Current technical runtime bound. It is not a permanent policy cap.
pub const ERA_V14_TECHNICAL_VALIDATOR_BOUND: u32 = 16;

/// Validator policy whose economic values come only from approved V14 records.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ValidatorPolicy {
    minimum_self_bond: u128,
    maximum_commission_ppb: u32,
    activation_validators: u32,
    bootstrap_milestone: u32,
    technical_validator_bound: u32,
}

impl ValidatorPolicy {
    /// Construct the approved ERA V14 validator policy for a native-token base unit.
    pub fn era_v14(etkn_base_unit: u128) -> Result<Self, PolicyError> {
        let minimum_self_bond = etkn_base_unit
            .checked_mul(10_000)
            .ok_or(PolicyError::ArithmeticOverflow)?;
        Self::new(
            minimum_self_bond,
            ERA_V14_MAX_COMMISSION_PPB,
            ERA_V14_ACTIVATION_VALIDATORS,
            ERA_V14_BOOTSTRAP_MILESTONE,
            ERA_V14_TECHNICAL_VALIDATOR_BOUND,
        )
    }

    /// Construct a policy. Integration tests may use a different technical bound to prove that
    /// sixteen is replaceable after separate benchmark and runtime review.
    pub fn new(
        minimum_self_bond: u128,
        maximum_commission_ppb: u32,
        activation_validators: u32,
        bootstrap_milestone: u32,
        technical_validator_bound: u32,
    ) -> Result<Self, PolicyError> {
        if minimum_self_bond == 0 {
            return Err(PolicyError::ZeroMinimumSelfBond);
        }
        if maximum_commission_ppb > PARTS_PER_BILLION {
            return Err(PolicyError::InvalidCommissionFraction);
        }
        if activation_validators == 0 {
            return Err(PolicyError::ZeroActivationValidators);
        }
        if bootstrap_milestone < activation_validators {
            return Err(PolicyError::BootstrapBelowActivation);
        }
        if technical_validator_bound < bootstrap_milestone {
            return Err(PolicyError::TechnicalBoundBelowBootstrap);
        }
        Ok(Self {
            minimum_self_bond,
            maximum_commission_ppb,
            activation_validators,
            bootstrap_milestone,
            technical_validator_bound,
        })
    }

    pub const fn minimum_self_bond(&self) -> u128 {
        self.minimum_self_bond
    }

    pub const fn maximum_commission_ppb(&self) -> u32 {
        self.maximum_commission_ppb
    }

    pub const fn activation_validators(&self) -> u32 {
        self.activation_validators
    }

    pub const fn bootstrap_milestone(&self) -> u32 {
        self.bootstrap_milestone
    }

    pub const fn technical_validator_bound(&self) -> u32 {
        self.technical_validator_bound
    }

    /// There is deliberately no policy-cap value. Capacity above the current technical bound is
    /// an integration/benchmark gate, not a permanently rejected policy state.
    pub const fn permanent_policy_cap(&self) -> Option<u32> {
        None
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PolicyError {
    ArithmeticOverflow,
    ZeroMinimumSelfBond,
    InvalidCommissionFraction,
    ZeroActivationValidators,
    BootstrapBelowActivation,
    TechnicalBoundBelowBootstrap,
}

/// Snapshot of only the fields relevant to validator eligibility.
///
/// `total_backing` is retained for election/exposure handoff, but it is never substituted for
/// `self_bond`. Nominations therefore remain part of staking elections without allowing them to
/// satisfy the minimum validator bond.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CandidateSnapshot<Id> {
    pub validator: Id,
    pub self_bond: u128,
    pub total_backing: u128,
    pub commission_ppb: u32,
    pub session_keys_registered: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CandidateIneligibility {
    SelfBondBelowMinimum,
    CommissionAboveMaximum,
    MissingSessionKeys,
}

/// Enforce candidacy using self-bond, never total nominated backing.
pub fn validate_candidate<Id>(
    policy: &ValidatorPolicy,
    candidate: &CandidateSnapshot<Id>,
) -> Result<(), CandidateIneligibility> {
    if candidate.self_bond < policy.minimum_self_bond {
        return Err(CandidateIneligibility::SelfBondBelowMinimum);
    }
    if candidate.commission_ppb > policy.maximum_commission_ppb {
        return Err(CandidateIneligibility::CommissionAboveMaximum);
    }
    if !candidate.session_keys_registered {
        return Err(CandidateIneligibility::MissingSessionKeys);
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ElectionSummary {
    pub candidate_count: u32,
    pub eligible_count: u32,
    pub required_winners: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ElectionError {
    ZeroRequiredWinners,
    RequiredWinnersAboveTechnicalBound,
    TooManyCandidates,
    DuplicateCandidate { first: u32, second: u32 },
    InsufficientEligibleCandidates { required: u32, eligible: u32 },
}

/// Validate the bounded eligibility surface before passing the unchanged candidates, backing,
/// and nominations to the existing staking election provider.
pub fn validate_election_candidates<Id: PartialEq>(
    policy: &ValidatorPolicy,
    required_winners: u32,
    candidates: &[CandidateSnapshot<Id>],
) -> Result<ElectionSummary, ElectionError> {
    if required_winners == 0 {
        return Err(ElectionError::ZeroRequiredWinners);
    }
    if required_winners > policy.technical_validator_bound {
        return Err(ElectionError::RequiredWinnersAboveTechnicalBound);
    }
    if candidates.len() > policy.technical_validator_bound as usize {
        return Err(ElectionError::TooManyCandidates);
    }
    reject_duplicate_candidates(candidates).map_err(|(first, second)| {
        ElectionError::DuplicateCandidate {
            first: first as u32,
            second: second as u32,
        }
    })?;
    let eligible_count = candidates
        .iter()
        .filter(|candidate| validate_candidate(policy, candidate).is_ok())
        .count() as u32;
    if eligible_count < required_winners {
        return Err(ElectionError::InsufficientEligibleCandidates {
            required: required_winners,
            eligible: eligible_count,
        });
    }
    Ok(ElectionSummary {
        candidate_count: candidates.len() as u32,
        eligible_count,
        required_winners,
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MigrationSetError {
    WrongValidatorCount {
        expected: u32,
        observed: u32,
    },
    DuplicateValidator {
        first: u32,
        second: u32,
    },
    IneligibleValidator {
        index: u32,
        reason: CandidateIneligibility,
    },
}

/// Validate, without reordering or replacing, the four-validator migration set.
pub fn validate_migration_set<Id: PartialEq>(
    policy: &ValidatorPolicy,
    validators: &[CandidateSnapshot<Id>],
) -> Result<(), MigrationSetError> {
    if validators.len() != policy.activation_validators as usize {
        return Err(MigrationSetError::WrongValidatorCount {
            expected: policy.activation_validators,
            observed: validators.len() as u32,
        });
    }
    reject_duplicate_candidates(validators).map_err(|(first, second)| {
        MigrationSetError::DuplicateValidator {
            first: first as u32,
            second: second as u32,
        }
    })?;
    for (index, validator) in validators.iter().enumerate() {
        validate_candidate(policy, validator).map_err(|reason| {
            MigrationSetError::IneligibleValidator {
                index: index as u32,
                reason,
            }
        })?;
    }
    Ok(())
}

fn reject_duplicate_candidates<Id: PartialEq>(
    candidates: &[CandidateSnapshot<Id>],
) -> Result<(), (usize, usize)> {
    for (second, candidate) in candidates.iter().enumerate() {
        if let Some(first) = candidates[..second]
            .iter()
            .position(|seen| seen.validator == candidate.validator)
        {
            return Err((first, second));
        }
    }
    Ok(())
}

/// Evidence supplied by the separately authorized validator-growth gate.
///
/// These booleans are inputs, not claims made by this module. The module never manufactures an
/// operator identity, authorization, testnet proof, observation window, or operational approval.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GrowthEvidence {
    pub separately_authorized: bool,
    pub independent_operator_attested: bool,
    pub disposable_finality_proven: bool,
    pub monitoring_and_wallet_ready: bool,
    pub stable_observation_complete: bool,
    pub no_unresolved_security_incident: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GrowthStage {
    BootstrapToSeven,
    IncrementalAfterSeven,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GrowthApproval {
    pub from: u32,
    pub to: u32,
    pub stage: GrowthStage,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GrowthError {
    BelowActivationSet,
    CurrentCountAboveTechnicalBound,
    NotOneAtATime,
    TechnicalRebenchmarkRequired { current_bound: u32, requested: u32 },
    CandidateIneligible(CandidateIneligibility),
    MissingSeparateAuthorization,
    MissingIndependentOperatorAttestation,
    MissingDisposableFinalityProof,
    MonitoringOrWalletNotReady,
    StableObservationIncomplete,
    UnresolvedSecurityIncident,
}

/// Validate one proposed validator-count increase and its new candidate.
pub fn validate_growth<Id>(
    policy: &ValidatorPolicy,
    current: u32,
    requested: u32,
    candidate: &CandidateSnapshot<Id>,
    evidence: GrowthEvidence,
) -> Result<GrowthApproval, GrowthError> {
    if current < policy.activation_validators {
        return Err(GrowthError::BelowActivationSet);
    }
    if current > policy.technical_validator_bound {
        return Err(GrowthError::CurrentCountAboveTechnicalBound);
    }
    let next = current.checked_add(1).ok_or(GrowthError::NotOneAtATime)?;
    if requested != next {
        return Err(GrowthError::NotOneAtATime);
    }
    if requested > policy.technical_validator_bound {
        return Err(GrowthError::TechnicalRebenchmarkRequired {
            current_bound: policy.technical_validator_bound,
            requested,
        });
    }
    validate_candidate(policy, candidate).map_err(GrowthError::CandidateIneligible)?;
    if !evidence.separately_authorized {
        return Err(GrowthError::MissingSeparateAuthorization);
    }
    if !evidence.independent_operator_attested {
        return Err(GrowthError::MissingIndependentOperatorAttestation);
    }
    if !evidence.disposable_finality_proven {
        return Err(GrowthError::MissingDisposableFinalityProof);
    }
    if !evidence.monitoring_and_wallet_ready {
        return Err(GrowthError::MonitoringOrWalletNotReady);
    }
    if !evidence.no_unresolved_security_incident {
        return Err(GrowthError::UnresolvedSecurityIncident);
    }
    let stage = if current < policy.bootstrap_milestone {
        GrowthStage::BootstrapToSeven
    } else {
        if !evidence.stable_observation_complete {
            return Err(GrowthError::StableObservationIncomplete);
        }
        GrowthStage::IncrementalAfterSeven
    };
    Ok(GrowthApproval {
        from: current,
        to: requested,
        stage,
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionAction {
    PreserveExisting,
    InstallElected,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SessionPlan {
    pub action: SessionAction,
    pub authority_count: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionError {
    EmptyExistingSession,
    ExistingSessionAboveTechnicalBound,
    Election(ElectionError),
}

/// Plan a session transition without mutating session, BABE, GRANDPA, or keys.
///
/// `None` means election failed: the existing non-empty session is retained. A successful
/// election is accepted only when the complete proposed set passes the same bounded eligibility
/// checks. This preserves a rollback-safe decision point for the integration runtime.
pub fn plan_session_transition<Id: PartialEq>(
    policy: &ValidatorPolicy,
    existing_authority_count: u32,
    elected: Option<&[CandidateSnapshot<Id>]>,
    required_winners: u32,
) -> Result<SessionPlan, SessionError> {
    if existing_authority_count == 0 {
        return Err(SessionError::EmptyExistingSession);
    }
    if existing_authority_count > policy.technical_validator_bound {
        return Err(SessionError::ExistingSessionAboveTechnicalBound);
    }
    let Some(elected) = elected else {
        return Ok(SessionPlan {
            action: SessionAction::PreserveExisting,
            authority_count: existing_authority_count,
        });
    };
    let summary = validate_election_candidates(policy, required_winners, elected)
        .map_err(SessionError::Election)?;
    if summary.candidate_count != required_winners {
        return Err(SessionError::Election(
            ElectionError::InsufficientEligibleCandidates {
                required: required_winners,
                eligible: summary.eligible_count.min(summary.candidate_count),
            },
        ));
    }
    Ok(SessionPlan {
        action: SessionAction::InstallElected,
        authority_count: required_winners,
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RewardIneligibility {
    Candidate(CandidateIneligibility),
    NotElected,
    NoRewardPoints,
}

/// Apply the same self-bond and commission rules at reward eligibility time.
pub fn validate_reward_eligibility<Id>(
    policy: &ValidatorPolicy,
    candidate: &CandidateSnapshot<Id>,
    elected_for_era: bool,
    reward_points: u32,
) -> Result<(), RewardIneligibility> {
    validate_candidate(policy, candidate).map_err(RewardIneligibility::Candidate)?;
    if !elected_for_era {
        return Err(RewardIneligibility::NotElected);
    }
    if reward_points == 0 {
        return Err(RewardIneligibility::NoRewardPoints);
    }
    Ok(())
}

/// Owner decision for any automatic validator ejection triggered by slashing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EjectionPolicyDecision {
    NoAutomaticEjectionApproved,
    SdkBehaviorApproved,
    SlashThresholdApproved(u32),
}

/// Owner decision for offence appeals. This module does not provide either origin.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AppealPolicyDecision {
    NoRuntimeAppealApproved,
    SeparatelyConfiguredOriginApproved,
}

/// Owner decision for the destination of any staking slash imbalance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SlashDestinationDecision {
    BurnApproved,
    AccumulationTreasuryApproved,
    SeparatelyConfiguredHandlerApproved,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OffenceMode {
    ObserveOnly,
    Apply,
}

/// Complete owner-supplied slashing inputs. There is intentionally no `Default` implementation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResolvedOffencePolicy {
    mode: OffenceMode,
    max_offenders_per_report: u32,
    maximum_slash_fraction_ppb: u32,
    reporter_reward_fraction_ppb: u32,
    ejection: EjectionPolicyDecision,
    appeal: AppealPolicyDecision,
    slash_destination: SlashDestinationDecision,
}

impl ResolvedOffencePolicy {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        validator_policy: &ValidatorPolicy,
        mode: OffenceMode,
        max_offenders_per_report: u32,
        maximum_slash_fraction_ppb: u32,
        reporter_reward_fraction_ppb: u32,
        ejection: EjectionPolicyDecision,
        appeal: AppealPolicyDecision,
        slash_destination: SlashDestinationDecision,
    ) -> Result<Self, OffencePolicyError> {
        if max_offenders_per_report == 0 {
            return Err(OffencePolicyError::ZeroOffenderBound);
        }
        if max_offenders_per_report > validator_policy.technical_validator_bound {
            return Err(OffencePolicyError::OffenderBoundAboveTechnicalValidatorBound);
        }
        if maximum_slash_fraction_ppb > PARTS_PER_BILLION {
            return Err(OffencePolicyError::InvalidSlashFraction);
        }
        if reporter_reward_fraction_ppb > PARTS_PER_BILLION {
            return Err(OffencePolicyError::InvalidReporterRewardFraction);
        }
        if let EjectionPolicyDecision::SlashThresholdApproved(threshold) = ejection {
            if threshold > PARTS_PER_BILLION {
                return Err(OffencePolicyError::InvalidEjectionThreshold);
            }
        }
        Ok(Self {
            mode,
            max_offenders_per_report,
            maximum_slash_fraction_ppb,
            reporter_reward_fraction_ppb,
            ejection,
            appeal,
            slash_destination,
        })
    }

    pub const fn mode(&self) -> OffenceMode {
        self.mode
    }

    pub const fn max_offenders_per_report(&self) -> u32 {
        self.max_offenders_per_report
    }

    pub const fn maximum_slash_fraction_ppb(&self) -> u32 {
        self.maximum_slash_fraction_ppb
    }

    pub const fn reporter_reward_fraction_ppb(&self) -> u32 {
        self.reporter_reward_fraction_ppb
    }

    pub const fn ejection(&self) -> EjectionPolicyDecision {
        self.ejection
    }

    pub const fn appeal(&self) -> AppealPolicyDecision {
        self.appeal
    }

    pub const fn slash_destination(&self) -> SlashDestinationDecision {
        self.slash_destination
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OffencePolicyError {
    ZeroOffenderBound,
    OffenderBoundAboveTechnicalValidatorBound,
    InvalidSlashFraction,
    InvalidReporterRewardFraction,
    InvalidEjectionThreshold,
}

/// Fail-closed decision boundary. Integration must use `Unresolved` until all listed slashing
/// inputs receive explicit approval; the module supplies no slash percentage or governance origin.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OffencePolicy {
    Unresolved,
    Resolved(ResolvedOffencePolicy),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OffenceBatch<'a, Id> {
    pub session_index: u32,
    pub offenders: &'a [Id],
    pub slash_fractions_ppb: &'a [u32],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OffenceError {
    PolicyUnresolved,
    EmptyBatch,
    TooManyOffenders { maximum: u32, observed: u32 },
    FractionCountMismatch,
    DuplicateOffender { first: u32, second: u32 },
    InvalidSlashFraction { index: u32 },
    SlashFractionAboveApprovedMaximum { index: u32 },
}

/// Fully validated offence input. Construct it only through `prepare_offence_batch`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PreparedOffenceBatch<'a, Id> {
    policy: ResolvedOffencePolicy,
    batch: OffenceBatch<'a, Id>,
}

impl<'a, Id> PreparedOffenceBatch<'a, Id> {
    pub const fn policy(&self) -> ResolvedOffencePolicy {
        self.policy
    }

    pub const fn batch(&self) -> &OffenceBatch<'a, Id> {
        &self.batch
    }
}

/// Validate a complete offence batch before any report or staking state is changed.
///
/// The standard SDK offence pallet remains responsible for duplicate report identifiers; this
/// bounded layer also rejects duplicate offender identities within one batch.
pub fn prepare_offence_batch<'a, Id: PartialEq>(
    policy: OffencePolicy,
    batch: OffenceBatch<'a, Id>,
) -> Result<PreparedOffenceBatch<'a, Id>, OffenceError> {
    let OffencePolicy::Resolved(policy) = policy else {
        return Err(OffenceError::PolicyUnresolved);
    };
    if batch.offenders.is_empty() {
        return Err(OffenceError::EmptyBatch);
    }
    if batch.offenders.len() > policy.max_offenders_per_report as usize {
        return Err(OffenceError::TooManyOffenders {
            maximum: policy.max_offenders_per_report,
            observed: batch.offenders.len() as u32,
        });
    }
    if batch.offenders.len() != batch.slash_fractions_ppb.len() {
        return Err(OffenceError::FractionCountMismatch);
    }
    for (second, offender) in batch.offenders.iter().enumerate() {
        if let Some(first) = batch.offenders[..second]
            .iter()
            .position(|seen| seen == offender)
        {
            return Err(OffenceError::DuplicateOffender {
                first: first as u32,
                second: second as u32,
            });
        }
    }
    for (index, fraction) in batch.slash_fractions_ppb.iter().enumerate() {
        if *fraction > PARTS_PER_BILLION {
            return Err(OffenceError::InvalidSlashFraction {
                index: index as u32,
            });
        }
        if *fraction > policy.maximum_slash_fraction_ppb {
            return Err(OffenceError::SlashFractionAboveApprovedMaximum {
                index: index as u32,
            });
        }
    }
    Ok(PreparedOffenceBatch { policy, batch })
}

/// Invoke an integration-owned standard SDK handler only after complete validation.
///
/// Observe-only mode intentionally produces no slashing-handler call. Integration must wrap any
/// report storage plus this dispatch in one runtime transaction so a downstream trap rolls back
/// the report and slash together.
pub fn dispatch_prepared_offence<Id, Result>(
    prepared: &PreparedOffenceBatch<'_, Id>,
    handler: impl FnOnce(&OffenceBatch<'_, Id>, &ResolvedOffencePolicy) -> Result,
) -> Option<Result> {
    match prepared.policy.mode {
        OffenceMode::ObserveOnly => None,
        OffenceMode::Apply => Some(handler(&prepared.batch, &prepared.policy)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ETKN: u128 = 1_000_000_000_000_000_000;

    fn policy() -> ValidatorPolicy {
        ValidatorPolicy::era_v14(ETKN).expect("valid V14 policy")
    }

    fn candidate(id: u64) -> CandidateSnapshot<u64> {
        CandidateSnapshot {
            validator: id,
            self_bond: 10_000 * ETKN,
            total_backing: 100_000 * ETKN,
            commission_ppb: ERA_V14_MAX_COMMISSION_PPB,
            session_keys_registered: true,
        }
    }

    fn complete_evidence() -> GrowthEvidence {
        GrowthEvidence {
            separately_authorized: true,
            independent_operator_attested: true,
            disposable_finality_proven: true,
            monitoring_and_wallet_ready: true,
            stable_observation_complete: true,
            no_unresolved_security_incident: true,
        }
    }

    fn resolved_offence_policy(mode: OffenceMode) -> ResolvedOffencePolicy {
        ResolvedOffencePolicy::new(
            &policy(),
            mode,
            16,
            100_000_000,
            10_000_000,
            EjectionPolicyDecision::NoAutomaticEjectionApproved,
            AppealPolicyDecision::NoRuntimeAppealApproved,
            SlashDestinationDecision::BurnApproved,
        )
        .expect("explicit test-only offence inputs")
    }

    #[test]
    fn approved_policy_constants_are_exact_and_have_no_permanent_cap() {
        let policy = policy();
        assert_eq!(policy.minimum_self_bond(), 10_000 * ETKN);
        assert_eq!(policy.maximum_commission_ppb(), 200_000_000);
        assert_eq!(policy.activation_validators(), 4);
        assert_eq!(policy.bootstrap_milestone(), 7);
        assert_eq!(policy.technical_validator_bound(), 16);
        assert_eq!(policy.permanent_policy_cap(), None);
    }

    #[test]
    fn etkn_conversion_overflow_fails_closed() {
        assert_eq!(
            ValidatorPolicy::era_v14(u128::MAX),
            Err(PolicyError::ArithmeticOverflow)
        );
    }

    #[test]
    fn under_bonded_candidate_cannot_substitute_nominations_for_self_bond() {
        let mut under = candidate(1);
        under.self_bond -= 1;
        under.total_backing = u128::MAX;
        assert_eq!(
            validate_candidate(&policy(), &under),
            Err(CandidateIneligibility::SelfBondBelowMinimum)
        );
        under.self_bond += 1;
        assert_eq!(validate_candidate(&policy(), &under), Ok(()));
    }

    #[test]
    fn commission_boundary_is_inclusive_and_never_clamped() {
        let at_limit = candidate(1);
        assert_eq!(validate_candidate(&policy(), &at_limit), Ok(()));
        let mut above = at_limit;
        above.commission_ppb += 1;
        assert_eq!(
            validate_candidate(&policy(), &above),
            Err(CandidateIneligibility::CommissionAboveMaximum)
        );
        assert_eq!(above.commission_ppb, ERA_V14_MAX_COMMISSION_PPB + 1);
    }

    #[test]
    fn election_eligibility_is_bounded_deduplicated_and_stake_neutral() {
        let candidates = [candidate(1), candidate(2), candidate(3), candidate(4)];
        let summary = validate_election_candidates(&policy(), 4, &candidates).unwrap();
        assert_eq!(summary.eligible_count, 4);

        let mut different_backing = candidates.clone();
        different_backing[0].total_backing = 1;
        different_backing[1].total_backing = u128::MAX;
        assert_eq!(
            validate_election_candidates(&policy(), 4, &different_backing),
            Ok(summary)
        );

        let duplicated = [candidate(1), candidate(2), candidate(1), candidate(4)];
        assert_eq!(
            validate_election_candidates(&policy(), 4, &duplicated),
            Err(ElectionError::DuplicateCandidate {
                first: 0,
                second: 2
            })
        );
    }

    #[test]
    fn election_rejects_under_bonded_or_keyless_shortfall() {
        let mut candidates = [candidate(1), candidate(2), candidate(3), candidate(4)];
        candidates[2].self_bond -= 1;
        assert_eq!(
            validate_election_candidates(&policy(), 4, &candidates),
            Err(ElectionError::InsufficientEligibleCandidates {
                required: 4,
                eligible: 3
            })
        );
        candidates[2] = candidate(3);
        candidates[3].session_keys_registered = false;
        assert_eq!(
            validate_election_candidates(&policy(), 4, &candidates),
            Err(ElectionError::InsufficientEligibleCandidates {
                required: 4,
                eligible: 3
            })
        );
    }

    #[test]
    fn migration_requires_the_same_four_eligible_unique_validators() {
        let validators = [candidate(1), candidate(2), candidate(3), candidate(4)];
        assert_eq!(validate_migration_set(&policy(), &validators), Ok(()));
        assert_eq!(
            validate_migration_set(&policy(), &validators[..3]),
            Err(MigrationSetError::WrongValidatorCount {
                expected: 4,
                observed: 3
            })
        );
        let duplicated = [candidate(1), candidate(2), candidate(3), candidate(1)];
        assert_eq!(
            validate_migration_set(&policy(), &duplicated),
            Err(MigrationSetError::DuplicateValidator {
                first: 0,
                second: 3
            })
        );
    }

    #[test]
    fn growth_is_one_qualified_validator_at_a_time_from_four_through_seven() {
        let evidence = complete_evidence();
        for current in 4..7 {
            assert_eq!(
                validate_growth(
                    &policy(),
                    current,
                    current + 1,
                    &candidate(100 + current as u64),
                    evidence
                ),
                Ok(GrowthApproval {
                    from: current,
                    to: current + 1,
                    stage: GrowthStage::BootstrapToSeven,
                })
            );
        }
        assert_eq!(
            validate_growth(&policy(), 4, 6, &candidate(9), evidence),
            Err(GrowthError::NotOneAtATime)
        );
    }

    #[test]
    fn growth_after_seven_requires_observation_and_no_incident() {
        let mut evidence = complete_evidence();
        evidence.stable_observation_complete = false;
        assert_eq!(
            validate_growth(&policy(), 7, 8, &candidate(8), evidence),
            Err(GrowthError::StableObservationIncomplete)
        );
        evidence.stable_observation_complete = true;
        evidence.no_unresolved_security_incident = false;
        assert_eq!(
            validate_growth(&policy(), 7, 8, &candidate(8), evidence),
            Err(GrowthError::UnresolvedSecurityIncident)
        );
    }

    #[test]
    fn every_growth_gate_is_required() {
        let candidate = candidate(5);
        let mut evidence = complete_evidence();
        evidence.separately_authorized = false;
        assert_eq!(
            validate_growth(&policy(), 4, 5, &candidate, evidence),
            Err(GrowthError::MissingSeparateAuthorization)
        );
        evidence = complete_evidence();
        evidence.independent_operator_attested = false;
        assert_eq!(
            validate_growth(&policy(), 4, 5, &candidate, evidence),
            Err(GrowthError::MissingIndependentOperatorAttestation)
        );
        evidence = complete_evidence();
        evidence.disposable_finality_proven = false;
        assert_eq!(
            validate_growth(&policy(), 4, 5, &candidate, evidence),
            Err(GrowthError::MissingDisposableFinalityProof)
        );
        evidence = complete_evidence();
        evidence.monitoring_and_wallet_ready = false;
        assert_eq!(
            validate_growth(&policy(), 4, 5, &candidate, evidence),
            Err(GrowthError::MonitoringOrWalletNotReady)
        );
    }

    #[test]
    fn sixteen_is_replaceable_technical_capacity_not_policy_cap() {
        assert_eq!(
            validate_growth(&policy(), 16, 17, &candidate(17), complete_evidence()),
            Err(GrowthError::TechnicalRebenchmarkRequired {
                current_bound: 16,
                requested: 17
            })
        );
        let expanded = ValidatorPolicy::new(10_000 * ETKN, 200_000_000, 4, 7, 17).unwrap();
        assert_eq!(expanded.permanent_policy_cap(), None);
        assert_eq!(
            validate_growth(&expanded, 16, 17, &candidate(17), complete_evidence()),
            Ok(GrowthApproval {
                from: 16,
                to: 17,
                stage: GrowthStage::IncrementalAfterSeven,
            })
        );
    }

    #[test]
    fn failed_election_preserves_nonempty_session() {
        assert_eq!(
            plan_session_transition::<u64>(&policy(), 4, None, 4),
            Ok(SessionPlan {
                action: SessionAction::PreserveExisting,
                authority_count: 4
            })
        );
        assert_eq!(
            plan_session_transition::<u64>(&policy(), 0, None, 4),
            Err(SessionError::EmptyExistingSession)
        );
    }

    #[test]
    fn successful_session_transition_requires_complete_eligible_set() {
        let elected = [candidate(1), candidate(2), candidate(3), candidate(4)];
        assert_eq!(
            plan_session_transition(&policy(), 4, Some(&elected), 4),
            Ok(SessionPlan {
                action: SessionAction::InstallElected,
                authority_count: 4
            })
        );
        assert!(matches!(
            plan_session_transition(&policy(), 4, Some(&elected[..3]), 4),
            Err(SessionError::Election(
                ElectionError::InsufficientEligibleCandidates { .. }
            ))
        ));
    }

    #[test]
    fn reward_eligibility_reuses_self_bond_commission_election_and_points() {
        let eligible = candidate(1);
        assert_eq!(
            validate_reward_eligibility(&policy(), &eligible, true, 10),
            Ok(())
        );
        assert_eq!(
            validate_reward_eligibility(&policy(), &eligible, false, 10),
            Err(RewardIneligibility::NotElected)
        );
        assert_eq!(
            validate_reward_eligibility(&policy(), &eligible, true, 0),
            Err(RewardIneligibility::NoRewardPoints)
        );
        let mut under = eligible;
        under.self_bond -= 1;
        assert_eq!(
            validate_reward_eligibility(&policy(), &under, true, 10),
            Err(RewardIneligibility::Candidate(
                CandidateIneligibility::SelfBondBelowMinimum
            ))
        );
    }

    #[test]
    fn unresolved_offence_policy_fails_without_invoking_a_handler() {
        let offenders = [1u64];
        let fractions = [1u32];
        let error = prepare_offence_batch(
            OffencePolicy::Unresolved,
            OffenceBatch {
                session_index: 10,
                offenders: &offenders,
                slash_fractions_ppb: &fractions,
            },
        );
        assert_eq!(error, Err(OffenceError::PolicyUnresolved));
    }

    #[test]
    fn offence_policy_requires_explicit_bounded_valid_inputs() {
        assert_eq!(
            ResolvedOffencePolicy::new(
                &policy(),
                OffenceMode::Apply,
                17,
                1,
                0,
                EjectionPolicyDecision::SdkBehaviorApproved,
                AppealPolicyDecision::SeparatelyConfiguredOriginApproved,
                SlashDestinationDecision::SeparatelyConfiguredHandlerApproved,
            ),
            Err(OffencePolicyError::OffenderBoundAboveTechnicalValidatorBound)
        );
        assert_eq!(
            ResolvedOffencePolicy::new(
                &policy(),
                OffenceMode::Apply,
                16,
                PARTS_PER_BILLION + 1,
                0,
                EjectionPolicyDecision::SdkBehaviorApproved,
                AppealPolicyDecision::SeparatelyConfiguredOriginApproved,
                SlashDestinationDecision::SeparatelyConfiguredHandlerApproved,
            ),
            Err(OffencePolicyError::InvalidSlashFraction)
        );
    }

    #[test]
    fn offence_batch_is_bounded_atomic_input_and_rejects_duplicates() {
        let policy = OffencePolicy::Resolved(resolved_offence_policy(OffenceMode::Apply));
        let offenders = [1u64, 2];
        let fractions = [10_000_000u32, 20_000_000];
        let prepared = prepare_offence_batch(
            policy,
            OffenceBatch {
                session_index: 42,
                offenders: &offenders,
                slash_fractions_ppb: &fractions,
            },
        )
        .unwrap();
        let mut calls = 0;
        let result = dispatch_prepared_offence(&prepared, |batch, _| {
            calls += 1;
            batch.session_index
        });
        assert_eq!(result, Some(42));
        assert_eq!(calls, 1);

        let duplicates = [1u64, 1];
        let invalid = prepare_offence_batch(
            policy,
            OffenceBatch {
                session_index: 42,
                offenders: &duplicates,
                slash_fractions_ppb: &fractions,
            },
        );
        assert_eq!(
            invalid,
            Err(OffenceError::DuplicateOffender {
                first: 0,
                second: 1
            })
        );
        assert_eq!(calls, 1, "invalid input produced no handler call");
    }

    #[test]
    fn offence_fraction_failure_produces_no_partial_handler_call() {
        let policy = OffencePolicy::Resolved(resolved_offence_policy(OffenceMode::Apply));
        let offenders = [1u64, 2];
        let fractions = [10_000_000u32, 100_000_001];
        assert_eq!(
            prepare_offence_batch(
                policy,
                OffenceBatch {
                    session_index: 1,
                    offenders: &offenders,
                    slash_fractions_ppb: &fractions,
                }
            ),
            Err(OffenceError::SlashFractionAboveApprovedMaximum { index: 1 })
        );
    }

    #[test]
    fn observe_only_offence_policy_never_calls_slashing_handler() {
        let policy = OffencePolicy::Resolved(resolved_offence_policy(OffenceMode::ObserveOnly));
        let offenders = [1u64];
        let fractions = [10_000_000u32];
        let prepared = prepare_offence_batch(
            policy,
            OffenceBatch {
                session_index: 1,
                offenders: &offenders,
                slash_fractions_ppb: &fractions,
            },
        )
        .unwrap();
        let mut called = false;
        let result = dispatch_prepared_offence(&prepared, |_, _| called = true);
        assert_eq!(result, None);
        assert!(!called);
    }
}
