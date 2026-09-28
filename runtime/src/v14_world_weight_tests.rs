use super::*;
use codec::Encode;
use frame_support::{
    assert_ok,
    dispatch::{DispatchClass, GetDispatchInfo},
    traits::Get,
};
use pallet_era_worlds::weights::WeightInfo as WorldWeightInfo;
use sp_runtime::{traits::TransactionExtension, BuildStorage};

fn ext() -> sp_io::TestExternalities {
    let storage = frame_system::GenesisConfig::<Runtime>::default()
        .build_storage()
        .expect("system genesis storage");
    sp_io::TestExternalities::new(storage)
}

#[test]
fn world_calls_use_generated_weights_and_fit_runtime_admission_limits() {
    type WorldWeights = <Runtime as pallet_era_worlds::Config>::WeightInfo;

    assert_eq!(MaxWorldIdLength::get(), 64);
    let world_id: pallet_era_worlds::WorldIdOf<Runtime> = vec![b'w'; 64]
        .try_into()
        .expect("maximum runtime world identifier");
    let calls = [
        (
            "register_world",
            RuntimeCall::EraWorlds(pallet_era_worlds::Call::register_world {
                world_id: world_id.clone(),
                commitment: [1; 32],
            }),
            WorldWeights::register_world(),
        ),
        (
            "update_commitment",
            RuntimeCall::EraWorlds(pallet_era_worlds::Call::update_commitment {
                world_id: world_id.clone(),
                commitment: [2; 32],
            }),
            WorldWeights::update_commitment(),
        ),
        (
            "deregister_world",
            RuntimeCall::EraWorlds(pallet_era_worlds::Call::deregister_world {
                world_id: world_id.clone(),
            }),
            WorldWeights::deregister_world(),
        ),
        (
            "admin_remove_world",
            RuntimeCall::EraWorlds(pallet_era_worlds::Call::admin_remove_world {
                world_id: world_id.clone(),
            }),
            WorldWeights::admin_remove_world(),
        ),
        (
            "pause",
            RuntimeCall::EraWorlds(pallet_era_worlds::Call::pause {}),
            WorldWeights::pause(),
        ),
        (
            "unpause",
            RuntimeCall::EraWorlds(pallet_era_worlds::Call::unpause {}),
            WorldWeights::unpause(),
        ),
    ];

    let limits: frame_system::limits::BlockWeights =
        <Runtime as frame_system::Config>::BlockWeights::get();
    let lengths: frame_system::limits::BlockLength =
        <Runtime as frame_system::Config>::BlockLength::get();
    let normal = limits.get(DispatchClass::Normal);
    let max_extrinsic = normal
        .max_extrinsic
        .expect("Normal-class maximum extrinsic weight");
    let max_total = normal.max_total.expect("Normal-class block capacity");
    let max_length = *lengths.max.get(DispatchClass::Normal);
    let extra: SignedExtra = (
        frame_system::CheckNonZeroSender::new(),
        frame_system::CheckSpecVersion::new(),
        frame_system::CheckTxVersion::new(),
        frame_system::CheckGenesis::new(),
        frame_system::CheckMortality::from(sp_runtime::generic::Era::mortal(64, 1)),
        frame_system::CheckNonce::from(0),
        frame_system::CheckWeight::new(),
        pallet_transaction_payment::ChargeTransactionPayment::from(0),
    );

    for (index, (name, call, expected)) in calls.into_iter().enumerate() {
        let encoded = call.encode();
        assert_eq!(&encoded[..2], &[18, index as u8], "{name}");

        let mut info = call.get_dispatch_info();
        assert_eq!(info.class, DispatchClass::Normal, "{name}");
        assert_eq!(info.call_weight, expected, "{name}");
        assert!(expected.ref_time() > 0, "{name}");
        assert!(expected.all_lte(max_extrinsic), "{name}");
        assert!(expected.all_lte(max_total), "{name}");

        let extension = extra.weight(&call);
        let charged = expected
            .saturating_add(extension)
            .saturating_add(normal.base_extrinsic);
        assert!(charged.all_lte(max_extrinsic), "{name}: {charged:?}");
        assert!(charged.all_lte(max_total), "{name}: {charged:?}");

        // The 128-byte envelope covers signature, signer, era, nonce and fee fields around the
        // encoded call. CheckWeight uses the same encoded-length input during admission.
        let encoded_extrinsic_length = call.encoded_size() + 128;
        assert!(encoded_extrinsic_length <= max_length as usize, "{name}");
        info.extension_weight = extension;
        ext().execute_with(|| {
            assert!(frame_system::CheckWeight::<Runtime>::do_validate(
                &info,
                encoded_extrinsic_length,
            )
            .is_ok());
        });

        println!(
            "WORLD_ADMISSION {name} call={expected:?} extension={extension:?} base={:?} charged={charged:?} encoded_length={encoded_extrinsic_length} max_extrinsic={max_extrinsic:?} max_total={max_total:?} max_length={max_length}",
            normal.base_extrinsic,
        );
    }
}

fn account(id: u8) -> AccountId {
    AccountId::new([id; 32])
}

fn funded_ext(accounts: &[AccountId]) -> sp_io::TestExternalities {
    let mut storage = frame_system::GenesisConfig::<Runtime>::default()
        .build_storage()
        .expect("system genesis storage");
    pallet_balances::GenesisConfig::<Runtime> {
        balances: accounts
            .iter()
            .cloned()
            .map(|who| (who, 1_000_000 * DECIMALS))
            .collect(),
        dev_accounts: None,
    }
    .assimilate_storage(&mut storage)
    .expect("balances genesis storage");
    sp_io::TestExternalities::new(storage)
}

fn assert_signed_normal_admission(call: &RuntimeCall) {
    let limits: frame_system::limits::BlockWeights =
        <Runtime as frame_system::Config>::BlockWeights::get();
    let lengths: frame_system::limits::BlockLength =
        <Runtime as frame_system::Config>::BlockLength::get();
    let normal = limits.get(DispatchClass::Normal);
    let max_extrinsic = normal.max_extrinsic.expect("Normal extrinsic limit");
    let max_total = normal.max_total.expect("Normal block limit");
    let extra: SignedExtra = (
        frame_system::CheckNonZeroSender::new(),
        frame_system::CheckSpecVersion::new(),
        frame_system::CheckTxVersion::new(),
        frame_system::CheckGenesis::new(),
        frame_system::CheckMortality::from(sp_runtime::generic::Era::mortal(64, 1)),
        frame_system::CheckNonce::from(0),
        frame_system::CheckWeight::new(),
        pallet_transaction_payment::ChargeTransactionPayment::from(0),
    );
    let mut info = call.get_dispatch_info();
    assert_eq!(info.class, DispatchClass::Normal);
    let extension = extra.weight(call);
    let charged = info
        .call_weight
        .saturating_add(extension)
        .saturating_add(normal.base_extrinsic);
    assert!(charged.all_lte(max_extrinsic));
    assert!(charged.all_lte(max_total));
    let encoded_length = call.encoded_size() + 128;
    assert!(encoded_length <= *lengths.max.get(DispatchClass::Normal) as usize);
    info.extension_weight = extension;
    assert!(frame_system::CheckWeight::<Runtime>::do_validate(&info, encoded_length).is_ok());
}

#[test]
fn world_account_work_is_charged_and_admitted_through_multisig_and_sudo_as() {
    type WorldWeights = <Runtime as pallet_era_worlds::Config>::WeightInfo;

    let signer = account(1);
    let other = account(2);
    let sudo_target = account(3);
    let multisig = Multisig::multi_account_id(&[signer.clone(), other.clone()], 1);
    funded_ext(&[
        signer.clone(),
        other.clone(),
        sudo_target.clone(),
        multisig.clone(),
    ])
    .execute_with(|| {
        let multisig_world: pallet_era_worlds::WorldIdOf<Runtime> =
            b"multisig-world".to_vec().try_into().unwrap();
        let register = RuntimeCall::EraWorlds(pallet_era_worlds::Call::register_world {
            world_id: multisig_world.clone(),
            commitment: [7; 32],
        });
        let outer_register = RuntimeCall::Multisig(pallet_multisig::Call::as_multi_threshold_1 {
            other_signatories: vec![other.clone()],
            call: Box::new(register),
        });
        assert!(outer_register
            .get_dispatch_info()
            .call_weight
            .all_gte(WorldWeights::register_world()));
        assert_signed_normal_admission(&outer_register);
        assert_ok!(outer_register.dispatch(RuntimeOrigin::signed(signer.clone())));
        assert_eq!(
            pallet_era_worlds::Worlds::<Runtime>::get(&multisig_world)
                .unwrap()
                .owner,
            multisig
        );
        assert_eq!(
            Balances::reserved_balance(multisig.clone()),
            WorldRegistrationDeposit::get()
        );

        let outer_deregister = RuntimeCall::Multisig(pallet_multisig::Call::as_multi_threshold_1 {
            other_signatories: vec![other],
            call: Box::new(RuntimeCall::EraWorlds(
                pallet_era_worlds::Call::deregister_world {
                    world_id: multisig_world.clone(),
                },
            )),
        });
        assert!(outer_deregister
            .get_dispatch_info()
            .call_weight
            .all_gte(WorldWeights::deregister_world()));
        assert_signed_normal_admission(&outer_deregister);
        assert_ok!(outer_deregister.dispatch(RuntimeOrigin::signed(signer.clone())));
        assert!(!pallet_era_worlds::Worlds::<Runtime>::contains_key(
            &multisig_world
        ));
        assert_eq!(Balances::reserved_balance(multisig), 0);

        pallet_sudo::Key::<Runtime>::put(signer.clone());
        let sudo_world: pallet_era_worlds::WorldIdOf<Runtime> =
            b"sudo-as-world".to_vec().try_into().unwrap();
        let sudo_register = RuntimeCall::Sudo(pallet_sudo::Call::sudo_as {
            who: MultiAddress::Id(sudo_target.clone()),
            call: Box::new(RuntimeCall::EraWorlds(
                pallet_era_worlds::Call::register_world {
                    world_id: sudo_world.clone(),
                    commitment: [8; 32],
                },
            )),
        });
        assert!(sudo_register
            .get_dispatch_info()
            .call_weight
            .all_gte(WorldWeights::register_world()));
        assert_signed_normal_admission(&sudo_register);
        assert_ok!(sudo_register.dispatch(RuntimeOrigin::signed(signer.clone())));
        assert_eq!(
            pallet_era_worlds::Worlds::<Runtime>::get(&sudo_world)
                .unwrap()
                .owner,
            sudo_target
        );
        assert_eq!(
            Balances::reserved_balance(sudo_target.clone()),
            WorldRegistrationDeposit::get()
        );

        let sudo_deregister = RuntimeCall::Sudo(pallet_sudo::Call::sudo_as {
            who: MultiAddress::Id(sudo_target.clone()),
            call: Box::new(RuntimeCall::EraWorlds(
                pallet_era_worlds::Call::deregister_world {
                    world_id: sudo_world.clone(),
                },
            )),
        });
        assert!(sudo_deregister
            .get_dispatch_info()
            .call_weight
            .all_gte(WorldWeights::deregister_world()));
        assert_signed_normal_admission(&sudo_deregister);
        assert_ok!(sudo_deregister.dispatch(RuntimeOrigin::signed(signer)));
        assert!(!pallet_era_worlds::Worlds::<Runtime>::contains_key(
            &sudo_world
        ));
        assert_eq!(Balances::reserved_balance(sudo_target), 0);
    });
}
