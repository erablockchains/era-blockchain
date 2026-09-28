use super::*;

type AccountId = u64;
type Evidence = u64;
type Hash = u64;
type Balance = u128;

#[derive(Clone, Copy)]
struct SyntheticDerivation;

impl StandardPalletDerivation<AccountId, u16> for SyntheticDerivation {
    fn derive_multisig(
        &self,
        sorted_signatories: &[AccountId; FOUNDER_COUNT],
        threshold: u16,
    ) -> Option<AccountId> {
        Some(sorted_signatories.iter().sum::<u64>() + u64::from(threshold).checked_mul(1_000)?)
    }

    fn derive_pure_proxy(
        &self,
        multisig_controller: &AccountId,
        category: CustodyCategory,
        evidence: &u16,
    ) -> Option<AccountId> {
        let domain = match category {
            CustodyCategory::Presale => 10_000,
            CustodyCategory::Ecosystem => 20_000,
            CustodyCategory::Liquidity => 30_000,
        };
        multisig_controller
            .checked_add(domain)?
            .checked_add(u64::from(*evidence))
    }
}

struct SyntheticFoundationVerifier;

impl FoundationEvidenceVerifier<AccountId, Evidence> for SyntheticFoundationVerifier {
    fn founder_evidence_is_valid(
        &self,
        account: &AccountId,
        identity_attestation: &Evidence,
        control_attestation: &Evidence,
    ) -> bool {
        *identity_attestation == account * 10 + 1 && *control_attestation == account * 10 + 2
    }

    fn unanimous_custody_attestation_is_valid(
        &self,
        sorted_signatories: &[AccountId; FOUNDER_COUNT],
        presale: &AccountId,
        ecosystem: &AccountId,
        attestation: &Evidence,
    ) -> bool {
        sorted_signatories == &[1, 2, 3]
            && presale == &13_017
            && ecosystem == &23_028
            && *attestation == 100
    }

    fn unanimous_vesting_attestation_is_valid(
        &self,
        sorted_signatories: &[AccountId; FOUNDER_COUNT],
        beneficiaries: &[VestingBeneficiary<AccountId>; FOUNDING_ALLOCATION_COUNT],
        attestation: &Evidence,
    ) -> bool {
        sorted_signatories == &[1, 2, 3]
            && beneficiaries.each_ref().map(|item| item.account) == [10, 11, 12, 13, 14]
            && *attestation == 200
    }
}

fn foundation_inputs() -> FoundationInputs<AccountId, u16, Evidence> {
    FoundationInputs {
        founders: Some(FounderSet {
            signatories: [1, 2, 3],
            threshold: 3,
        }),
        founder_evidence: Some([
            FounderEvidence {
                account: 1,
                identity_attestation: 11,
                control_attestation: 12,
            },
            FounderEvidence {
                account: 2,
                identity_attestation: 21,
                control_attestation: 22,
            },
            FounderEvidence {
                account: 3,
                identity_attestation: 31,
                control_attestation: 32,
            },
        ]),
        unanimous_custody_attestation: Some(100),
        presale: Some(CustodyDerivation {
            category: CustodyCategory::Presale,
            multisig_controller: 3_006,
            pure_proxy_account: 13_017,
            proxy_evidence: 11,
            alternate_delegate_count: 0,
            single_signer_path: false,
        }),
        ecosystem: Some(CustodyDerivation {
            category: CustodyCategory::Ecosystem,
            multisig_controller: 3_006,
            pure_proxy_account: 23_028,
            proxy_evidence: 22,
            alternate_delegate_count: 0,
            single_signer_path: false,
        }),
        vesting_beneficiaries: Some([
            VestingBeneficiary {
                account: 10,
                allocation: FOUNDING_ALLOCATION_TARGETS[0],
            },
            VestingBeneficiary {
                account: 11,
                allocation: FOUNDING_ALLOCATION_TARGETS[1],
            },
            VestingBeneficiary {
                account: 12,
                allocation: FOUNDING_ALLOCATION_TARGETS[2],
            },
            VestingBeneficiary {
                account: 13,
                allocation: FOUNDING_ALLOCATION_TARGETS[3],
            },
            VestingBeneficiary {
                account: 14,
                allocation: FOUNDING_ALLOCATION_TARGETS[4],
            },
        ]),
        unanimous_vesting_attestation: Some(200),
    }
}

fn verified_foundations() -> VerifiedFoundations<AccountId> {
    verify_foundations(
        foundation_inputs(),
        &SyntheticDerivation,
        &SyntheticFoundationVerifier,
    )
    .expect("complete synthetic inputs pass")
}

#[test]
fn complete_foundations_are_scoped_and_keep_sudo() {
    let verified = verified_foundations();
    assert_eq!(verified.founders, [1, 2, 3]);
    assert_eq!(verified.multisig_controller, 3_006);
    assert_ne!(verified.presale_custody, verified.ecosystem_custody);
    assert!(verified.no_single_founder_custody_path);
    assert_eq!(
        verified.sudo_disposition,
        SudoDisposition::PreserveOperational
    );
}

#[test]
fn threshold_duplicate_missing_and_order_fail_closed() {
    let mut missing = foundation_inputs();
    missing.founders = None;
    assert_eq!(
        verify_foundations(missing, &SyntheticDerivation, &SyntheticFoundationVerifier),
        Err(GateError::MissingFounderSet)
    );

    let mut wrong_threshold = foundation_inputs();
    wrong_threshold.founders.as_mut().unwrap().threshold = 2;
    assert_eq!(
        verify_foundations(
            wrong_threshold,
            &SyntheticDerivation,
            &SyntheticFoundationVerifier
        ),
        Err(GateError::WrongThreshold)
    );

    let mut duplicate = foundation_inputs();
    duplicate.founders.as_mut().unwrap().signatories = [1, 1, 3];
    assert_eq!(
        verify_foundations(
            duplicate,
            &SyntheticDerivation,
            &SyntheticFoundationVerifier
        ),
        Err(GateError::DuplicateFounder)
    );

    let mut unsorted = foundation_inputs();
    unsorted.founders.as_mut().unwrap().signatories = [2, 1, 3];
    assert_eq!(
        verify_foundations(unsorted, &SyntheticDerivation, &SyntheticFoundationVerifier),
        Err(GateError::NonCanonicalFounderOrder)
    );
}

#[test]
fn wrong_accounts_and_missing_attestations_are_rejected() {
    let mut wrong_proxy = foundation_inputs();
    wrong_proxy.presale.as_mut().unwrap().pure_proxy_account += 1;
    assert_eq!(
        verify_foundations(
            wrong_proxy,
            &SyntheticDerivation,
            &SyntheticFoundationVerifier
        ),
        Err(GateError::WrongPureProxyAccount)
    );

    let mut wrong_controller = foundation_inputs();
    wrong_controller
        .ecosystem
        .as_mut()
        .unwrap()
        .multisig_controller += 1;
    assert_eq!(
        verify_foundations(
            wrong_controller,
            &SyntheticDerivation,
            &SyntheticFoundationVerifier
        ),
        Err(GateError::WrongMultisigController)
    );

    let mut missing = foundation_inputs();
    missing.unanimous_custody_attestation = None;
    assert_eq!(
        verify_foundations(missing, &SyntheticDerivation, &SyntheticFoundationVerifier),
        Err(GateError::MissingCustodyAttestation)
    );

    let mut invalid = foundation_inputs();
    invalid.founder_evidence.as_mut().unwrap()[1].control_attestation = 0;
    assert_eq!(
        verify_foundations(invalid, &SyntheticDerivation, &SyntheticFoundationVerifier),
        Err(GateError::InvalidFounderEvidence)
    );
}

#[test]
fn vesting_allocations_and_exact_one_year_residuals_are_invariant() {
    assert_eq!(
        FOUNDING_ALLOCATION_TARGETS.iter().sum::<u128>(),
        FOUNDING_VESTING_TOTAL
    );
    for amount in FOUNDING_ALLOCATION_TARGETS {
        let terms = linear_vesting_terms(amount).expect("approved amount has exact terms");
        let schedules = exact_vesting_schedules(amount, 1_000).expect("block range fits");
        assert_eq!(terms.release_intervals, 5_256_000);
        assert_eq!(
            schedules.schedule_a.locked + schedules.schedule_b.locked,
            amount
        );
        assert_eq!(schedules.schedule_b.starting_block, 1_000 + 5_256_000 - 1);
        assert_eq!(schedules.completion_block, 1_000 + 5_256_000);
        assert_eq!(
            terms.final_interval_release,
            terms.floor_release_per_interval + terms.final_residual
        );
    }

    let mut wrong = foundation_inputs();
    wrong.vesting_beneficiaries.as_mut().unwrap()[0].allocation -= 1;
    assert_eq!(
        verify_foundations(wrong, &SyntheticDerivation, &SyntheticFoundationVerifier),
        Err(GateError::WrongVestingAllocation)
    );

    let mut duplicate = foundation_inputs();
    duplicate.vesting_beneficiaries.as_mut().unwrap()[4].account = 13;
    assert_eq!(
        verify_foundations(
            duplicate,
            &SyntheticDerivation,
            &SyntheticFoundationVerifier
        ),
        Err(GateError::DuplicateVestingBeneficiary)
    );
    assert_eq!(
        exact_vesting_schedules(FOUNDING_ALLOCATION_TARGETS[0], u32::MAX),
        Err(GateError::VestingArithmeticOverflow)
    );
}

#[test]
fn no_single_person_or_alternate_delegate_path_is_accepted() {
    let mut single = foundation_inputs();
    single.presale.as_mut().unwrap().single_signer_path = true;
    assert_eq!(
        verify_foundations(single, &SyntheticDerivation, &SyntheticFoundationVerifier),
        Err(GateError::SingleSignerBypass)
    );

    let mut alternate = foundation_inputs();
    alternate
        .ecosystem
        .as_mut()
        .unwrap()
        .alternate_delegate_count = 1;
    assert_eq!(
        verify_foundations(
            alternate,
            &SyntheticDerivation,
            &SyntheticFoundationVerifier
        ),
        Err(GateError::AlternateDelegatePresent)
    );
}

struct SyntheticExecutionVerifier;

impl ExecutionEvidenceVerifier<AccountId, Hash, Balance, Evidence> for SyntheticExecutionVerifier {
    fn execution_artifacts_are_valid(
        &self,
        founders: &[AccountId; FOUNDER_COUNT],
        artifacts: &CustodyExecutionArtifacts<AccountId, Hash, Balance, Evidence>,
    ) -> bool {
        founders == &[1, 2, 3]
            && artifacts.plan_id == 900
            && artifacts.presale_call_hash == 901
            && artifacts.ecosystem_call_hash == 902
            && artifacts.runtime_code_hash == 903
            && artifacts.metadata_hash == 904
            && artifacts.balance_snapshot_hash == 905
            && artifacts.deposit_schedule_hash == 906
            && artifacts.presale_balance == 501
            && artifacts.ecosystem_balance == 502
            && artifacts.multisig_deposit == 503
            && artifacts.presale_proxy_deposit == 504
            && artifacts.ecosystem_proxy_deposit == 505
            && artifacts.unanimous_final_authorization == 999
    }
}

fn execution_artifacts() -> CustodyExecutionArtifacts<AccountId, Hash, Balance, Evidence> {
    CustodyExecutionArtifacts {
        plan_id: 900,
        presale_source: 40,
        ecosystem_source: 41,
        presale_destination: 13_017,
        ecosystem_destination: 23_028,
        vesting_accounts: [10, 11, 12, 13, 14],
        vesting_start_block: 1_000,
        presale_call_hash: 901,
        ecosystem_call_hash: 902,
        runtime_code_hash: 903,
        metadata_hash: 904,
        balance_snapshot_hash: 905,
        deposit_schedule_hash: 906,
        presale_balance: 501,
        ecosystem_balance: 502,
        multisig_deposit: 503,
        presale_proxy_deposit: 504,
        ecosystem_proxy_deposit: 505,
        unanimous_final_authorization: 999,
    }
}

#[test]
fn migration_requires_exact_artifacts_accounts_and_operational_sudo() {
    let foundations = verified_foundations();
    assert_eq!(
        verify_migration_ready(
            &foundations,
            &MigrationMarker::<Hash>::Dormant,
            true,
            None,
            &SyntheticExecutionVerifier
        ),
        Err(GateError::MissingExecutionArtifacts)
    );
    assert_eq!(
        verify_migration_ready(
            &foundations,
            &MigrationMarker::Dormant,
            false,
            Some(execution_artifacts()),
            &SyntheticExecutionVerifier
        ),
        Err(GateError::SudoNotOperational)
    );

    let mut wrong_destination = execution_artifacts();
    wrong_destination.presale_destination += 1;
    assert_eq!(
        verify_migration_ready(
            &foundations,
            &MigrationMarker::Dormant,
            true,
            Some(wrong_destination),
            &SyntheticExecutionVerifier
        ),
        Err(GateError::WrongExecutionDestination)
    );

    let plan = verify_migration_ready(
        &foundations,
        &MigrationMarker::Dormant,
        true,
        Some(execution_artifacts()),
        &SyntheticExecutionVerifier,
    )
    .expect("complete exact artifacts pass");
    assert_eq!(plan.disposition, MigrationDisposition::Apply);
    assert_eq!(plan.sudo_disposition, SudoDisposition::PreserveOperational);
}

#[test]
fn replay_is_idempotent_for_same_plan_and_conflicting_for_another() {
    let foundations = verified_foundations();
    for (marker, expected) in [
        (MigrationMarker::Dormant, MigrationDisposition::Apply),
        (
            MigrationMarker::Prepared(900),
            MigrationDisposition::ResumePrepared,
        ),
        (
            MigrationMarker::Applied(900),
            MigrationDisposition::AlreadyAppliedNoop,
        ),
    ] {
        let plan = verify_migration_ready(
            &foundations,
            &marker,
            true,
            Some(execution_artifacts()),
            &SyntheticExecutionVerifier,
        )
        .expect("same plan is deterministic");
        assert_eq!(plan.disposition, expected);
    }
    assert_eq!(
        verify_migration_ready(
            &foundations,
            &MigrationMarker::Applied(899),
            true,
            Some(execution_artifacts()),
            &SyntheticExecutionVerifier
        ),
        Err(GateError::ReplayConflict)
    );
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SyntheticState {
    presale: u128,
    ecosystem: u128,
    marker: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SyntheticError {
    ApplyFailure,
    PostFailure,
}

#[test]
fn transactional_apply_rolls_back_apply_and_post_failures() {
    let original = SyntheticState {
        presale: 100,
        ecosystem: 200,
        marker: 0,
    };
    let mut apply_failure = original.clone();
    assert_eq!(
        transactional_apply(
            &mut apply_failure,
            |candidate| {
                candidate.presale = 0;
                Err(SyntheticError::ApplyFailure)
            },
            |_| Ok(())
        ),
        Err(SyntheticError::ApplyFailure)
    );
    assert_eq!(apply_failure, original);

    let mut post_failure = original.clone();
    assert_eq!(
        transactional_apply(
            &mut post_failure,
            |candidate| {
                candidate.presale = 0;
                candidate.marker = 900;
                Ok(())
            },
            |_| Err(SyntheticError::PostFailure)
        ),
        Err(SyntheticError::PostFailure)
    );
    assert_eq!(post_failure, original);

    let mut success = original;
    transactional_apply(
        &mut success,
        |candidate| {
            candidate.presale = 0;
            candidate.ecosystem = 0;
            candidate.marker = 900;
            Ok::<(), SyntheticError>(())
        },
        |candidate| {
            if candidate.presale == 0 && candidate.ecosystem == 0 {
                Ok(())
            } else {
                Err(SyntheticError::PostFailure)
            }
        },
    )
    .expect("valid candidate commits");
    assert_eq!(success.marker, 900);
}

struct SyntheticGovernanceVerifier;

impl GovernanceEvidenceVerifier<AccountId, Evidence> for SyntheticGovernanceVerifier {
    fn future_governance_evidence_is_valid(
        &self,
        founders: &[AccountId; FOUNDER_COUNT],
        inputs: &FutureGovernanceInputs<AccountId, Evidence>,
    ) -> bool {
        founders == &[1, 2, 3]
            && inputs.foundation_account == Some(50)
            && inputs.governance_design_evidence == Some(601)
            && inputs.independent_audit_evidence == Some(602)
            && inputs.disposable_test_evidence == Some(603)
            && inputs.recovery_rehearsal_evidence == Some(604)
            && inputs.unanimous_owner_attestation == Some(605)
    }
}

fn governance_inputs() -> FutureGovernanceInputs<AccountId, Evidence> {
    FutureGovernanceInputs {
        foundation_account: Some(50),
        governance_design_evidence: Some(601),
        independent_audit_evidence: Some(602),
        disposable_test_evidence: Some(603),
        recovery_rehearsal_evidence: Some(604),
        unanimous_owner_attestation: Some(605),
    }
}

#[test]
fn future_governance_handover_is_evidence_gated_and_never_activated() {
    let foundations = verified_foundations();
    let mut missing = governance_inputs();
    missing.independent_audit_evidence = None;
    assert_eq!(
        prepare_future_handover(&foundations, true, missing, &SyntheticGovernanceVerifier),
        Err(GateError::MissingGovernanceAuditEvidence)
    );

    let mut conflict = governance_inputs();
    conflict.foundation_account = Some(1);
    assert_eq!(
        prepare_future_handover(&foundations, true, conflict, &SyntheticGovernanceVerifier),
        Err(GateError::FoundationIdentityConflict)
    );

    let prepared = prepare_future_handover(
        &foundations,
        true,
        governance_inputs(),
        &SyntheticGovernanceVerifier,
    )
    .expect("complete synthetic future evidence prepares only");
    assert_eq!(prepared.foundation_account, 50);
    assert_eq!(
        prepared.sudo_disposition,
        SudoDisposition::PreserveOperational
    );
    assert!(!prepared.activation_authorized_by_ws3);
}
