use super::*;
use core::{any::TypeId, marker::PhantomData};
use frame_support::{
    dispatch::{DispatchClass, GetDispatchInfo},
    traits::Get,
    weights::Weight,
};
use pallet_security_budget::weights::WeightInfo as SecurityWeightInfo;
use pallet_transaction_payment::WeightInfo as TransactionPaymentWeightInfo;
use sp_runtime::{
    traits::{TransactionExtension, TxBaseImplication},
    transaction_validity::TransactionSource,
};

type SecurityWeights = pallet_security_budget::weights::SubstrateWeight<Runtime>;

fn runtime_db() -> frame_support::weights::RuntimeDbWeight {
    <Runtime as frame_system::Config>::DbWeight::get()
}

#[test]
fn security_budget_selects_generated_runtime_weights() {
    let _: PhantomData<<Runtime as pallet_security_budget::Config>::SecurityWeightInfo> =
        PhantomData::<SecurityWeights>;
    assert_ne!(TypeId::of::<SecurityWeights>(), TypeId::of::<()>());
}

#[test]
fn generated_models_match_the_accepted_component_wise_selection() {
    let db = runtime_db();
    assert_eq!(
        SecurityWeights::activate(),
        Weight::from_parts(112_842_000, 13_825).saturating_add(db.reads_writes(21, 12))
    );
    assert_eq!(
        SecurityWeights::claim_reward_page(64),
        Weight::from_parts(239_194_758 + 63_334_452 * 64, 6_196 + 2_603 * 64,)
            .saturating_add(db.reads_writes(22 + 64, 9 + 64))
    );
    assert_eq!(
        SecurityWeights::route_normal_fee(100),
        Weight::from_parts(160_938_493 + 276_575 * 100, 8_799)
            .saturating_add(db.reads_writes(15, 9))
    );
    assert_eq!(
        SecurityWeights::route_tip_author(100),
        Weight::from_parts(160_112_643 + 231_620 * 100, 8_799)
            .saturating_add(db.reads_writes(11, 7))
    );
    assert_eq!(
        SecurityWeights::route_tip_fallback(100),
        Weight::from_parts(162_574_285 + 194_501 * 100, 8_799)
            .saturating_add(db.reads_writes(16, 9))
    );
    assert_eq!(
        SecurityWeights::on_initialize(16),
        Weight::from_parts(179_871_777 + 18_061_951 * 16, 6_196 + 2_567 * 16,)
            .saturating_add(db.reads_writes(24 + 2 * 16, 17 + 16))
    );
    assert_eq!(
        SecurityWeights::migration_initialize(),
        Weight::from_parts(9_458_000, 1_501).saturating_add(db.reads_writes(2, 4))
    );
}

#[test]
fn transaction_payment_declares_one_normal_and_one_worst_case_tip_route() {
    let normal = SecurityWeights::route_normal_fee(100);
    let author = SecurityWeights::route_tip_author(100);
    let fallback = SecurityWeights::route_tip_fallback(100);
    let measured = normal.saturating_add(author.max(fallback));
    let legacy = FeeRoutingTransactionPaymentWeight::legacy_routing_envelope();
    let sdk =
        pallet_transaction_payment::weights::SubstrateWeight::<Runtime>::charge_transaction_payment(
        );
    let declared = FeeRoutingTransactionPaymentWeight::charge_transaction_payment();

    assert_eq!(
        FeeRoutingTransactionPaymentWeight::v14_routing_envelope(),
        measured
    );
    assert_eq!(declared, sdk.saturating_add(legacy.max(measured)));
    assert!(declared.all_lte(sdk.saturating_add(legacy).saturating_add(measured)));
    assert_eq!(declared.ref_time(), sdk.saturating_add(legacy).ref_time());
    assert!(declared.proof_size() >= sdk.saturating_add(legacy).proof_size());
}

#[test]
fn maximum_security_budget_signed_path_fits_check_weight_and_normal_block_capacity() {
    use codec::Encode;

    let call = RuntimeCall::SecurityBudget(pallet_security_budget::Call::claim_reward_page {
        era: u32::MAX,
        validator: AccountId::new([7; 32]),
        page: u32::MAX,
    });
    let mut info = call.get_dispatch_info();
    let extension: SignedExtra = (
        frame_system::CheckNonZeroSender::new(),
        frame_system::CheckSpecVersion::new(),
        frame_system::CheckTxVersion::new(),
        frame_system::CheckGenesis::new(),
        frame_system::CheckMortality::from(sp_runtime::generic::Era::mortal(64, 1)),
        frame_system::CheckNonce::from(0),
        frame_system::CheckWeight::new(),
        pallet_transaction_payment::ChargeTransactionPayment::from(0),
    );
    let extension_weight = extension.weight(&call);
    info.extension_weight = extension_weight;

    let limits: frame_system::limits::BlockWeights =
        <Runtime as frame_system::Config>::BlockWeights::get();
    let normal = limits.get(DispatchClass::Normal);
    let max_extrinsic = normal.max_extrinsic.expect("Normal extrinsic limit");
    let max_total = normal.max_total.expect("Normal block limit");
    let charged = info.total_weight().saturating_add(normal.base_extrinsic);
    assert!(charged.all_lte(max_extrinsic));
    assert!(charged.all_lte(max_total));

    let length = call.encoded_size().saturating_add(128);
    sp_io::TestExternalities::default().execute_with(|| {
        frame_system::CheckWeight::<Runtime>::do_validate(&info, length)
            .expect("maximum SecurityBudget signed call is admitted");
    });

    let by_ref = max_total.ref_time() / charged.ref_time();
    let by_proof = max_total.proof_size() / charged.proof_size();
    let full_count = by_ref.min(by_proof);
    assert!(full_count > 0);
    assert!(charged.saturating_mul(full_count).all_lte(max_total));
    assert!(!charged
        .saturating_mul(full_count.saturating_add(1))
        .all_lte(max_total));

    let sdk =
        pallet_transaction_payment::weights::SubstrateWeight::<Runtime>::charge_transaction_payment(
        );
    let previous =
        sdk.saturating_add(FeeRoutingTransactionPaymentWeight::legacy_routing_envelope());
    let current = FeeRoutingTransactionPaymentWeight::charge_transaction_payment();
    let increase = current.saturating_sub(previous);
    assert_eq!(increase, Weight::from_parts(0, 1_598));
    // IdentityFee prices reference time only, so this proof-only declaration does not alter fees.
    assert_eq!(<<Runtime as pallet_transaction_payment::Config>::WeightToFee as frame_support::weights::WeightToFee>::weight_to_fee(&increase), 0);

    println!(
        "SECURITY_BUDGET_ADMISSION max_extrinsic={max_extrinsic:?} max_total={max_total:?} call={:?} extension={extension_weight:?} charged={charged:?} full_count={full_count} declared_increase={increase:?} fee_increase={}",
        info.call_weight,
        <<Runtime as pallet_transaction_payment::Config>::WeightToFee as frame_support::weights::WeightToFee>::weight_to_fee(&increase),
    );
}

#[test]
fn unsigned_and_inherent_like_extrinsics_refund_the_declared_payment_extension_weight() {
    fn assert_no_charge(call: RuntimeCall) {
        let extension = pallet_transaction_payment::ChargeTransactionPayment::<Runtime>::from(0);
        let info = call.get_dispatch_info();
        let len = codec::Encode::encoded_size(&call);
        let origin = RuntimeOrigin::none();
        let (_, val, returned_origin) = extension
            .validate(
                origin,
                &call,
                &info,
                len,
                (),
                &TxBaseImplication(()),
                TransactionSource::External,
            )
            .expect("non-signed payment extension validation");
        assert!(matches!(val, pallet_transaction_payment::Val::NoCharge));
        let pre = extension
            .prepare(val, &returned_origin, &call, &info, len)
            .expect("non-signed payment extension preparation");
        match pre {
            pallet_transaction_payment::Pre::NoCharge { refund } => {
                assert_eq!(
                    refund,
                    FeeRoutingTransactionPaymentWeight::charge_transaction_payment()
                )
            }
            pallet_transaction_payment::Pre::Charge { .. } => panic!("non-signed route charged"),
        }
    }

    assert_no_charge(RuntimeCall::System(frame_system::Call::remark {
        remark: Vec::new(),
    }));
    assert_no_charge(RuntimeCall::Timestamp(pallet_timestamp::Call::set {
        now: 1,
    }));
}

#[test]
fn security_budget_migration_weight_is_returned_once_without_db_double_counting() {
    let expected = runtime_db()
        .reads_writes(54, 10)
        .saturating_add(SecurityWeights::migration_initialize())
        .saturating_add(crate::v14_migration_lifecycle::pristine_weight())
        .saturating_add(crate::ws3_vesting::declared_weight());
    assert_eq!(crate::v14_migration::declared_weight(), expected);
    let limits: frame_system::limits::BlockWeights =
        <Runtime as frame_system::Config>::BlockWeights::get();
    assert!(expected.all_lte(limits.max_block));
}

#[test]
fn every_generated_weight_has_one_benchmark_and_a_real_accounting_consumer() {
    let benchmarks = include_str!("../../pallets/security-budget/src/benchmarking.rs");
    let weights = include_str!("../../pallets/security-budget/src/weights.rs");
    let runtime = include_str!("lib.rs");
    let migration = include_str!("v14_migration.rs");
    for method in [
        "activate",
        "claim_reward_page",
        "retry_normal_fee",
        "retry_tip",
        "route_normal_fee",
        "route_tip_author",
        "route_tip_fallback",
        "on_initialize",
        "migration_initialize",
    ] {
        assert!(
            benchmarks.contains(&format!("fn {method}(")),
            "{method} benchmark absent"
        );
        assert!(
            weights.contains(&format!("fn {method}(")),
            "{method} weight absent"
        );
    }
    for dispatch_or_hook in [
        "SecurityWeightInfo::activate()",
        "SecurityWeightInfo::claim_reward_page(",
        "SecurityWeightInfo::retry_normal_fee()",
        "SecurityWeightInfo::retry_tip()",
        "SecurityWeightInfo::on_initialize(",
    ] {
        assert!(include_str!("../../pallets/security-budget/src/lib.rs").contains(dispatch_or_hook));
    }
    assert!(runtime.contains("W::route_normal_fee(100)"));
    assert!(runtime.contains("W::route_tip_author(100).max(W::route_tip_fallback(100))"));
    assert!(migration.contains("pallet_security_budget::weights::SubstrateWeight::<"));
    assert!(migration.contains(">::migration_initialize()"));
    for prohibited in [
        "Weight::MAX",
        "WeightInfo for ()",
        "Weight::from_parts(0, 0)",
    ] {
        assert!(
            !weights.contains(prohibited),
            "generated weights contain {prohibited}"
        );
    }
}
