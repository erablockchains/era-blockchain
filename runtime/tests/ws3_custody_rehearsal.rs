//! Executable synthetic-only custody rehearsal. Evidence acceptance and transfer orchestration
//! exist ONLY in this test target; no production custody call or automatic transfer is added.
use codec::{Decode, Encode};
use era_runtime::ws3_custody::{PureProxyCoordinates, SdkCustodyDerivation};
use era_runtime::*;
use era_v14_custody_governance::*;
use frame_support::{
    assert_ok, ensure,
    storage::{with_transaction, TransactionOutcome},
    traits::{Currency, ExistenceRequirement, InstanceFilter, VestingSchedule},
    weights::Weight,
};
use sp_runtime::{BuildStorage, DispatchError, DispatchResult, MultiAddress, StateVersion};

type Digest = [u8; 32];
type Inputs = FoundationInputs<AccountId, PureProxyCoordinates, Digest>;
type Artifacts = CustodyExecutionArtifacts<AccountId, Digest, Balance, Digest>;
const MARKER: &[u8] = b"ws3/synthetic-rehearsal-only";
fn who(id: u8) -> AccountId {
    AccountId::new([id; 32])
}
fn digest(value: impl Encode) -> Digest {
    sp_io::hashing::blake2_256(&value.encode())
}

#[derive(Clone)]
struct Fixture {
    inputs: Inputs,
    artifacts: Artifacts,
}
impl Fixture {
    fn new() -> Self {
        let founders = [who(1), who(2), who(3)];
        let controller = Multisig::multi_account_id(&founders, 3);
        let custody = |category, proxy_type, index| {
            let coordinates = PureProxyCoordinates {
                proxy_type,
                disambiguation_index: index,
                creation_block: 100,
                creation_extrinsic_index: 0,
            };
            CustodyDerivation {
                category,
                multisig_controller: controller.clone(),
                pure_proxy_account: Proxy::pure_account(
                    &controller,
                    &proxy_type,
                    index,
                    Some((100, 0)),
                ),
                proxy_evidence: coordinates,
                alternate_delegate_count: 0,
                single_signer_path: false,
            }
        };
        let presale = custody(CustodyCategory::Presale, ProxyType::Presale, 0);
        let ecosystem = custody(CustodyCategory::Ecosystem, ProxyType::Ecosystem, 1);
        let beneficiaries = core::array::from_fn(|i| VestingBeneficiary {
            account: who(30 + i as u8),
            allocation: FOUNDING_ALLOCATION_TARGETS[i],
        });
        let mut artifacts = Artifacts {
            plan_id: [0; 32],
            presale_source: who(10),
            ecosystem_source: who(11),
            presale_destination: presale.pure_proxy_account.clone(),
            ecosystem_destination: ecosystem.pure_proxy_account.clone(),
            vesting_accounts: beneficiaries.each_ref().map(|b| b.account.clone()),
            vesting_start_block: 1_000,
            presale_call_hash: digest(Self::transfer_call(
                &presale.pure_proxy_account,
                20_000_000 * DECIMALS,
            )),
            ecosystem_call_hash: digest(Self::transfer_call(
                &ecosystem.pure_proxy_account,
                20_000_000 * DECIMALS - 40 * DECIMALS,
            )),
            runtime_code_hash: digest(b"synthetic WS3 runtime binding; not a Wasm attestation"),
            metadata_hash: digest(Runtime::metadata()),
            balance_snapshot_hash: digest(b"synthetic 100M allocation snapshot v1"),
            deposit_schedule_hash: digest((
                MultisigDepositBase::get(),
                MultisigDepositFactor::get(),
                ProxyDepositBase::get(),
                ProxyDepositFactor::get(),
            )),
            presale_balance: 20_000_000 * DECIMALS,
            ecosystem_balance: 20_000_000 * DECIMALS - 40 * DECIMALS,
            multisig_deposit: MultisigDepositBase::get() + 3 * MultisigDepositFactor::get(),
            presale_proxy_deposit: ProxyDepositBase::get() + ProxyDepositFactor::get(),
            ecosystem_proxy_deposit: ProxyDepositBase::get() + ProxyDepositFactor::get(),
            unanimous_final_authorization: digest(b"synthetic unanimous execution evidence"),
        };
        artifacts.plan_id = digest((
            b"WS3 synthetic plan",
            &founders,
            &artifacts.vesting_accounts,
            artifacts.presale_call_hash,
            artifacts.ecosystem_call_hash,
            artifacts.metadata_hash,
            artifacts.deposit_schedule_hash,
            artifacts.vesting_start_block,
        ));
        let inputs = Inputs {
            founders: Some(FounderSet {
                signatories: founders.clone(),
                threshold: 3,
            }),
            founder_evidence: Some(core::array::from_fn(|i| FounderEvidence {
                account: founders[i].clone(),
                identity_attestation: digest((artifacts.plan_id, &founders[i], b"identity")),
                control_attestation: digest((artifacts.plan_id, &founders[i], b"control")),
            })),
            unanimous_custody_attestation: Some(digest((artifacts.plan_id, b"custody"))),
            presale: Some(presale),
            ecosystem: Some(ecosystem),
            vesting_beneficiaries: Some(beneficiaries),
            unanimous_vesting_attestation: Some(digest((artifacts.plan_id, b"vesting"))),
        };
        Self { inputs, artifacts }
    }
    fn transfer_call(destination: &AccountId, amount: Balance) -> RuntimeCall {
        RuntimeCall::Balances(pallet_balances::Call::transfer_allow_death {
            dest: MultiAddress::Id(destination.clone()),
            value: amount,
        })
    }
    fn ext(&self) -> sp_io::TestExternalities {
        let mut storage = frame_system::GenesisConfig::<Runtime>::default()
            .build_storage()
            .unwrap();
        let controller = self
            .inputs
            .presale
            .as_ref()
            .unwrap()
            .multisig_controller
            .clone();
        let mut balances = vec![
            (who(10), 20_000_000 * DECIMALS),
            (who(11), 20_000_000 * DECIMALS - 40 * DECIMALS),
            (who(12), 10_000_000 * DECIMALS),
            (who(13), 10_000_000 * DECIMALS),
            (who(14), 20_000_000 * DECIMALS),
            (who(1), 10 * DECIMALS),
            (who(2), 10 * DECIMALS),
            (who(3), 10 * DECIMALS),
            (controller, 10 * DECIMALS),
        ];
        balances.extend(
            self.inputs
                .vesting_beneficiaries
                .as_ref()
                .unwrap()
                .iter()
                .map(|b| (b.account.clone(), b.allocation)),
        );
        pallet_balances::GenesisConfig::<Runtime> {
            balances,
            dev_accounts: None,
        }
        .assimilate_storage(&mut storage)
        .unwrap();
        let mut ext = sp_io::TestExternalities::new(storage);
        ext.execute_with(|| {
            System::set_block_number(100);
            System::set_extrinsic_index(0);
            pallet_sudo::Key::<Runtime>::put(who(99));
            pallet_session::Validators::<Runtime>::put(vec![who(90), who(91), who(92), who(93)]);
            pallet_staking::ValidatorCount::<Runtime>::put(4);
            issuance_cap::RemainingAllowance::<Runtime>::put(900_000_000 * DECIMALS);
        });
        ext
    }
    fn marker(&self) -> MigrationMarker<Digest> {
        match sp_io::storage::get(MARKER) {
            None => MigrationMarker::Dormant,
            Some(bytes) => {
                let (applied, id) = <(bool, Digest)>::decode(&mut &bytes[..]).unwrap();
                if applied {
                    MigrationMarker::Applied(id)
                } else {
                    MigrationMarker::Prepared(id)
                }
            }
        }
    }
    fn multisig_create(
        &self,
        definition: &CustodyDerivation<AccountId, PureProxyCoordinates>,
    ) -> DispatchResult {
        let call = RuntimeCall::Proxy(pallet_proxy::Call::create_pure {
            proxy_type: definition.proxy_evidence.proxy_type,
            delay: PRESALE_PROXY_DELAY,
            index: definition.proxy_evidence.disambiguation_index,
        });
        let hash = digest(&call);
        let founders = &self.inputs.founders.as_ref().unwrap().signatories;
        let mut when = None;
        for (index, signer) in founders.iter().enumerate() {
            Multisig::as_multi(
                RuntimeOrigin::signed(signer.clone()),
                3,
                founders.iter().filter(|f| *f != signer).cloned().collect(),
                when,
                Box::new(call.clone()),
                Weight::MAX,
            )
            .map_err(|error| error.error)?;
            if index < 2 {
                ensure!(
                    Proxy::proxies(definition.pure_proxy_account.clone())
                        .0
                        .is_empty(),
                    DispatchError::Other("single-founder execution")
                );
                when = Some(
                    pallet_multisig::Multisigs::<Runtime>::get(
                        &definition.multisig_controller,
                        hash,
                    )
                    .unwrap()
                    .when,
                );
            }
        }
        let (delegates, deposit) = Proxy::proxies(definition.pure_proxy_account.clone());
        ensure!(
            delegates.len() == 1
                && delegates[0].delegate == definition.multisig_controller
                && delegates[0].proxy_type == definition.proxy_evidence.proxy_type
                && delegates[0].delay == PRESALE_PROXY_DELAY
                && deposit == self.artifacts.presale_proxy_deposit,
            DispatchError::Other("wrong proxy custody state")
        );
        Ok(())
    }
    fn execute(
        &self,
        inputs: Inputs,
        artifacts: Artifacts,
        fail_after: Option<u8>,
    ) -> DispatchResult {
        let foundations = verify_foundations(inputs.clone(), &SdkCustodyDerivation, self)
            .map_err(|_| DispatchError::Other("foundation evidence rejected"))?;
        let plan = verify_migration_ready(
            &foundations,
            &self.marker(),
            pallet_sudo::Key::<Runtime>::get() == Some(who(99)),
            Some(artifacts.clone()),
            self,
        )
        .map_err(|_| DispatchError::Other("execution evidence rejected"))?;
        // An expected fixture is the independent rehearsal verifier, never an acceptance rule for
        // real evidence. Full record equality binds coordinates and category-specific inputs too.
        ensure!(
            inputs == self.inputs && artifacts == self.artifacts,
            DispatchError::Other("unbound fixture")
        );
        if plan.disposition == MigrationDisposition::AlreadyAppliedNoop {
            // Ordinary vesting may have advanced since installation; never reapply the plan.
            return Ok(());
        }
        with_transaction(|| {
            let result = (|| {
                ensure!(
                    Balances::free_balance(&artifacts.presale_source) == artifacts.presale_balance
                        && Balances::free_balance(&artifacts.ecosystem_source)
                            == artifacts.ecosystem_balance,
                    DispatchError::Other("source snapshot mismatch")
                );
                ensure!(
                    Balances::free_balance(&artifacts.presale_destination) == 0
                        && Balances::free_balance(&artifacts.ecosystem_destination) == 0,
                    DispatchError::Other("custody contamination")
                );
                let fail = |step| {
                    if fail_after == Some(step) {
                        Err(DispatchError::Other("injected rehearsal failure"))
                    } else {
                        Ok(())
                    }
                };
                sp_io::storage::set(MARKER, &(false, plan.plan_id).encode());
                fail(0)?;
                self.multisig_create(inputs.presale.as_ref().unwrap())?;
                fail(1)?;
                self.multisig_create(inputs.ecosystem.as_ref().unwrap())?;
                fail(2)?;
                <Balances as Currency<AccountId>>::transfer(
                    &artifacts.presale_source,
                    &artifacts.presale_destination,
                    artifacts.presale_balance,
                    ExistenceRequirement::AllowDeath,
                )?;
                fail(3)?;
                <Balances as Currency<AccountId>>::transfer(
                    &artifacts.ecosystem_source,
                    &artifacts.ecosystem_destination,
                    artifacts.ecosystem_balance,
                    ExistenceRequirement::AllowDeath,
                )?;
                fail(4)?;
                for (index, beneficiary) in inputs
                    .vesting_beneficiaries
                    .as_ref()
                    .unwrap()
                    .iter()
                    .enumerate()
                {
                    let pair = plan.vesting_schedules[index];
                    // Initial installation only; already applied plans never re-add schedules.
                    for schedule in [pair.schedule_a, pair.schedule_b] {
                        <Vesting as VestingSchedule<AccountId>>::add_vesting_schedule(
                            &beneficiary.account,
                            schedule.locked,
                            schedule.per_block,
                            schedule.starting_block,
                        )?;
                    }
                    fail(5 + index as u8)?;
                }
                self.postconditions()?;
                fail(10)?;
                sp_io::storage::set(MARKER, &(true, plan.plan_id).encode());
                fail(11)?;
                Ok(())
            })();
            match result {
                Ok(()) => TransactionOutcome::Commit(Ok(())),
                Err(e) => TransactionOutcome::Rollback(Err(e)),
            }
        })
    }
    fn postconditions(&self) -> DispatchResult {
        ensure!(
            Balances::total_issuance() == 100_000_000 * DECIMALS,
            DispatchError::Other("allocation conservation failed")
        );
        ensure!(
            IssuanceCap::remaining_allowance() == Some(900_000_000 * DECIMALS),
            DispatchError::Other("allowance changed")
        );
        ensure!(
            Balances::free_balance(who(10)) == 0
                && Balances::free_balance(who(11)) == 0
                && Balances::free_balance(&self.artifacts.presale_destination)
                    == self.artifacts.presale_balance
                && Balances::free_balance(&self.artifacts.ecosystem_destination)
                    == self.artifacts.ecosystem_balance,
            DispatchError::Other("category delta mismatch")
        );
        ensure!(
            Balances::free_balance(who(12)) == 10_000_000 * DECIMALS
                && Balances::free_balance(who(13)) == 10_000_000 * DECIMALS
                && Balances::free_balance(who(14)) == 20_000_000 * DECIMALS,
            DispatchError::Other("allocation cross spending")
        );
        let controller = &self.inputs.presale.as_ref().unwrap().multisig_controller;
        let ecosystem_total = Balances::free_balance(&self.artifacts.ecosystem_destination)
            + Balances::total_balance(controller)
            + [who(1), who(2), who(3)]
                .iter()
                .map(Balances::total_balance)
                .sum::<Balance>();
        ensure!(
            ecosystem_total == 20_000_000 * DECIMALS,
            DispatchError::Other("deposit allocation mismatch")
        );
        for b in self.inputs.vesting_beneficiaries.as_ref().unwrap() {
            let schedules = pallet_vesting::Vesting::<Runtime>::get(&b.account)
                .ok_or(DispatchError::Other("missing beneficiary schedules"))?;
            ensure!(
                schedules.len() == 2 && Balances::free_balance(&b.account) == b.allocation,
                DispatchError::Other("beneficiary allocation mismatch")
            );
        }
        ensure!(
            pallet_sudo::Key::<Runtime>::get() == Some(who(99))
                && Session::validators().len() == 4
                && Staking::validator_count() == 4,
            DispatchError::Other("authority changed")
        );
        Ok(())
    }
}
impl FoundationEvidenceVerifier<AccountId, Digest> for Fixture {
    fn founder_evidence_is_valid(
        &self,
        account: &AccountId,
        identity: &Digest,
        control: &Digest,
    ) -> bool {
        self.inputs
            .founder_evidence
            .as_ref()
            .unwrap()
            .iter()
            .any(|e| {
                &e.account == account
                    && &e.identity_attestation == identity
                    && &e.control_attestation == control
            })
    }
    fn unanimous_custody_attestation_is_valid(
        &self,
        founders: &[AccountId; 3],
        presale: &AccountId,
        ecosystem: &AccountId,
        evidence: &Digest,
    ) -> bool {
        founders == &self.inputs.founders.as_ref().unwrap().signatories
            && presale == &self.artifacts.presale_destination
            && ecosystem == &self.artifacts.ecosystem_destination
            && Some(*evidence) == self.inputs.unanimous_custody_attestation
    }
    fn unanimous_vesting_attestation_is_valid(
        &self,
        founders: &[AccountId; 3],
        beneficiaries: &[VestingBeneficiary<AccountId>; 5],
        evidence: &Digest,
    ) -> bool {
        founders == &self.inputs.founders.as_ref().unwrap().signatories
            && Some(beneficiaries) == self.inputs.vesting_beneficiaries.as_ref()
            && Some(*evidence) == self.inputs.unanimous_vesting_attestation
    }
}
impl ExecutionEvidenceVerifier<AccountId, Digest, Balance, Digest> for Fixture {
    fn execution_artifacts_are_valid(
        &self,
        founders: &[AccountId; 3],
        artifacts: &Artifacts,
    ) -> bool {
        founders == &self.inputs.founders.as_ref().unwrap().signatories
            && artifacts == &self.artifacts
    }
}

#[test]
fn ws3_custody_rehearsal_uses_three_signers_preserves_allocations_and_replays() {
    let f = Fixture::new();
    for prepared in [false, true] {
        f.ext().execute_with(|| {
            if prepared {
                sp_io::storage::set(MARKER, &(false, f.artifacts.plan_id).encode());
            }
            assert_ok!(f.execute(f.inputs.clone(), f.artifacts.clone(), None));
            let root = sp_io::storage::root(StateVersion::V1);
            assert_ok!(f.execute(f.inputs.clone(), f.artifacts.clone(), None));
            assert_eq!(root, sp_io::storage::root(StateVersion::V1));
            for definition in [
                f.inputs.presale.as_ref().unwrap(),
                f.inputs.ecosystem.as_ref().unwrap(),
            ] {
                for signer in [who(1), who(2), who(3)] {
                    assert!(Proxy::proxy(
                        RuntimeOrigin::signed(signer),
                        MultiAddress::Id(definition.pure_proxy_account.clone()),
                        Some(definition.proxy_evidence.proxy_type),
                        Box::new(Fixture::transfer_call(&who(15), DECIMALS))
                    )
                    .is_err());
                }
                for proxy_type in [ProxyType::Presale, ProxyType::Ecosystem] {
                    for call in [
                        Fixture::transfer_call(&who(15), DECIMALS),
                        RuntimeCall::Proxy(pallet_proxy::Call::add_proxy {
                            delegate: MultiAddress::Id(who(1)),
                            proxy_type: ProxyType::AuditOnly,
                            delay: 0,
                        }),
                        RuntimeCall::Multisig(pallet_multisig::Call::as_multi_threshold_1 {
                            other_signatories: vec![who(1)],
                            call: Box::new(Fixture::transfer_call(&who(15), DECIMALS)),
                        }),
                    ] {
                        assert!(!proxy_type.filter(&call));
                    }
                }
            }
            assert_ok!(Sudo::sudo(
                RuntimeOrigin::signed(who(99)),
                Box::new(RuntimeCall::System(frame_system::Call::remark {
                    remark: b"sudo preserved".to_vec()
                }))
            ));
        });
    }
}

#[test]
fn ws3_custody_failures_after_each_write_roll_back_schedules_deposits_events_and_marker() {
    let f = Fixture::new();
    for step in 0..=11 {
        f.ext().execute_with(|| {
            let root = sp_io::storage::root(StateVersion::V1);
            assert!(f
                .execute(f.inputs.clone(), f.artifacts.clone(), Some(step))
                .is_err());
            assert_eq!(root, sp_io::storage::root(StateVersion::V1));
        });
    }
}

#[test]
fn ws3_custody_rejects_wrong_authority_evidence_destinations_and_replay_plan() {
    let f = Fixture::new();
    for scenario in 0..17 {
        f.ext().execute_with(|| {
            let mut inputs = f.inputs.clone();
            let mut artifacts = f.artifacts.clone();
            match scenario {
                0 => inputs.founders.as_mut().unwrap().threshold = 1,
                1 => inputs.founders.as_mut().unwrap().threshold = 2,
                2 => inputs.founders.as_mut().unwrap().threshold = 4,
                3 => inputs.founders.as_mut().unwrap().signatories.swap(0, 1),
                4 => inputs.founders.as_mut().unwrap().signatories[1] = who(1),
                5 => inputs.founder_evidence = None,
                6 => inputs.unanimous_custody_attestation = None,
                7 => inputs.unanimous_vesting_attestation = None,
                8 => inputs.presale.as_mut().unwrap().alternate_delegate_count = 1,
                9 => inputs.presale.as_mut().unwrap().single_signer_path = true,
                10 => artifacts.presale_destination = who(1),
                11 => artifacts.metadata_hash = [1; 32],
                12 => artifacts.presale_call_hash = [1; 32],
                13 => artifacts.presale_proxy_deposit += 1,
                14 => artifacts.balance_snapshot_hash = [1; 32],
                15 => sp_io::storage::set(MARKER, &(false, [1u8; 32]).encode()),
                _ => sp_io::storage::set(MARKER, &(true, [1u8; 32]).encode()),
            }
            let root = sp_io::storage::root(StateVersion::V1);
            assert!(f.execute(inputs, artifacts, None).is_err());
            assert_eq!(root, sp_io::storage::root(StateVersion::V1));
        });
    }
}

#[test]
fn ws3_custody_replay_after_partial_vesting_and_state_failures_preserve_exact_state() {
    use frame_support::traits::{LockableCurrency, WithdrawReasons};
    let f = Fixture::new();
    f.ext().execute_with(|| {
        assert_ok!(f.execute(f.inputs.clone(), f.artifacts.clone(), None));
        System::set_block_number(1_001);
        let beneficiary = &f.artifacts.vesting_accounts[0];
        assert_ok!(Vesting::vest_other(
            RuntimeOrigin::signed(who(1)),
            MultiAddress::Id(beneficiary.clone())
        ));
        let matured = linear_vesting_terms(FOUNDING_ALLOCATION_TARGETS[0])
            .unwrap()
            .floor_release_per_interval;
        assert_ok!(Balances::transfer_allow_death(
            RuntimeOrigin::signed(beneficiary.clone()),
            MultiAddress::Id(who(1)),
            matured
        ));
        let root = sp_io::storage::root(StateVersion::V1);
        assert_ok!(f.execute(f.inputs.clone(), f.artifacts.clone(), None));
        assert_eq!(sp_io::storage::root(StateVersion::V1), root);
    });
    for failure in 0..4 {
        f.ext().execute_with(|| {
            match failure {
                0 => {
                    // Fresh destination contamination is not silently absorbed.
                    assert_ok!(Balances::transfer_allow_death(
                        RuntimeOrigin::signed(who(1)),
                        MultiAddress::Id(f.artifacts.presale_destination.clone()),
                        DECIMALS
                    ));
                }
                1 => Balances::set_lock(
                    *b"ws3src00",
                    &who(10),
                    20_000_000 * DECIMALS,
                    WithdrawReasons::all(),
                ),
                2 => {
                    let schedules: frame_support::BoundedVec<
                        _,
                        pallet_vesting::MaxVestingSchedulesGet<Runtime>,
                    > = vec![pallet_vesting::VestingInfo::new(DECIMALS, 1, 1_000); 127]
                        .try_into()
                        .unwrap();
                    pallet_vesting::Vesting::<Runtime>::insert(
                        &f.artifacts.vesting_accounts[4],
                        schedules,
                    );
                }
                _ => pallet_sudo::Key::<Runtime>::kill(),
            }
            let root = sp_io::storage::root(StateVersion::V1);
            assert!(f
                .execute(f.inputs.clone(), f.artifacts.clone(), None)
                .is_err());
            assert_eq!(sp_io::storage::root(StateVersion::V1), root);
        });
    }
}
