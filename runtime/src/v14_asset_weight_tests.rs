//! Actual-dispatch coverage for corrected metadata weights and the bounded NFT cleanup lifecycle.
use super::*;
use frame_support::dispatch::{DispatchClass, GetDispatchInfo};
use pallet_assets::WeightInfo;

type AssetWeights = <Runtime as pallet_assets::Config>::WeightInfo;
fn who(id: u8) -> AccountId {
    AccountId::new([id; 32])
}
fn signed(id: u8) -> RuntimeOrigin {
    RuntimeOrigin::signed(who(id))
}
fn address(id: u8) -> MultiAddress<AccountId, ()> {
    MultiAddress::Id(who(id))
}
fn ext() -> sp_io::TestExternalities {
    let mut storage = frame_system::GenesisConfig::<Runtime>::default()
        .build_storage()
        .unwrap();
    pallet_balances::GenesisConfig::<Runtime> {
        balances: vec![
            (who(1), 1_000_000 * DECIMALS),
            (who(2), 1_000_000 * DECIMALS),
        ],
        dev_accounts: None,
    }
    .assimilate_storage(&mut storage)
    .unwrap();
    let mut ext = sp_io::TestExternalities::new(storage);
    ext.execute_with(|| System::set_block_number(1));
    ext
}

#[test]
fn metadata_both_64_byte_bounds_and_deposit_lifecycle_are_preserved() {
    ext().execute_with(|| {
        assert_ok!(Assets::create(signed(1), 1, address(1), 1));
        for (n, s) in [
            (0, 0),
            (0, 64),
            (64, 0),
            (63, 63),
            (64, 64),
            (1, 1),
            (64, 64),
        ] {
            let call = RuntimeCall::Assets(pallet_assets::Call::set_metadata {
                id: 1,
                name: vec![1; n],
                symbol: vec![2; s],
                decimals: 12,
            });
            assert_eq!(
                call.get_dispatch_info().call_weight,
                AssetWeights::set_metadata(n as u32, s as u32)
            );
            assert_ok!(call.dispatch(signed(1)));
            let m = pallet_assets::Metadata::<Runtime>::get(1);
            assert_eq!((m.name.len(), m.symbol.len()), (n, s));
            assert_eq!(
                m.deposit,
                AssetMetadataDepositBase::get()
                    + ((n + s) as Balance) * AssetMetadataDepositPerByte::get()
            );
        }
        for (n, s) in [(65, 64), (64, 65)] {
            assert_noop!(
                Assets::set_metadata(signed(1), 1, vec![1; n], vec![2; s], 12),
                pallet_assets::Error::<Runtime>::BadMetadata
            );
        }
        let reserved = Balances::reserved_balance(who(1));
        assert_ok!(Assets::transfer_ownership(signed(1), 1, address(2)));
        assert_eq!(Balances::reserved_balance(who(1)), 0);
        assert_eq!(Balances::reserved_balance(who(2)), reserved);
        assert_ok!(Assets::clear_metadata(signed(2), 1));
        assert_eq!(Balances::reserved_balance(who(2)), AssetDeposit::get());
        assert_ok!(Assets::set_metadata(
            signed(2),
            1,
            vec![1; 64],
            vec![2; 64],
            12
        ));
        assert_ok!(Assets::start_destroy(signed(2), 1));
        assert_ok!(Assets::finish_destroy(signed(2), 1));
        assert_eq!(Balances::reserved_balance(who(2)), 0);
    });
}

#[test]
fn metadata_weight_wiring_and_actual_admission_limits() {
    type Generated = crate::v14_asset_weights::AssetsWeight<Runtime>;
    assert_eq!(
        AssetWeights::set_metadata(64, 64),
        Generated::set_metadata(64, 64)
    );
    assert_eq!(
        AssetWeights::set_metadata(65, 64),
        Generated::set_metadata(65, 64)
    );
    assert_eq!(AssetWeights::clear_metadata(), Generated::clear_metadata());
    assert_eq!(
        AssetWeights::transfer_ownership(),
        Generated::transfer_ownership()
    );
    assert_eq!(AssetWeights::finish_destroy(), Generated::finish_destroy());
    let limits: frame_system::limits::BlockWeights =
        <Runtime as frame_system::Config>::BlockWeights::get();
    let bytes: frame_system::limits::BlockLength =
        <Runtime as frame_system::Config>::BlockLength::get();
    assert_eq!(*bytes.max.get(DispatchClass::Operational), 5_242_880);
    let normal = limits.get(DispatchClass::Normal);
    let quarter = normal.max_total.unwrap().saturating_div(4);
    let maximum = normal.max_extrinsic.unwrap();
    for weight in [
        AssetWeights::set_metadata(64, 64),
        AssetWeights::set_metadata(5_242_880, 64),
        AssetWeights::set_metadata(64, 5_242_880),
        AssetWeights::clear_metadata(),
        AssetWeights::transfer_ownership(),
        AssetWeights::finish_destroy(),
    ] {
        assert!(weight.all_lte(maximum));
        assert!(weight.all_lte(quarter));
    }
    for call in [
        pallet_assets::Call::<Runtime>::clear_metadata { id: 1 },
        pallet_assets::Call::transfer_ownership {
            id: 1,
            owner: address(2),
        },
        pallet_assets::Call::finish_destroy { id: 1 },
    ] {
        let expected = match call {
            pallet_assets::Call::clear_metadata { .. } => AssetWeights::clear_metadata(),
            pallet_assets::Call::transfer_ownership { .. } => AssetWeights::transfer_ownership(),
            _ => AssetWeights::finish_destroy(),
        };
        assert_eq!(call.get_dispatch_info().call_weight, expected);
    }
    println!("METADATA_ADMISSION normal_max={:?} max_extrinsic={:?} normal_bytes={} operational_bytes={}", normal.max_total, maximum, bytes.max.get(DispatchClass::Normal), bytes.max.get(DispatchClass::Operational));
}

#[test]
fn force_metadata_still_rejects_even_with_filter_bypass() {
    use frame_support::traits::UnfilteredDispatchable;
    ext().execute_with(|| {
        assert_ok!(Assets::create(signed(1), 1, address(1), 1));
        for origin in [RuntimeOrigin::root(), signed(1)] {
            let before = sp_io::storage::root(StateVersion::V1);
            let call = RuntimeCall::Assets(pallet_assets::Call::force_set_metadata {
                id: 1,
                name: vec![1; 64],
                symbol: vec![2; 64],
                decimals: 12,
                is_frozen: false,
            });
            assert_eq!(
                call.dispatch_bypass_filter(origin.clone())
                    .unwrap_err()
                    .error,
                DispatchError::BadOrigin
            );
            let call = RuntimeCall::Assets(pallet_assets::Call::force_clear_metadata { id: 1 });
            assert_eq!(
                call.dispatch_bypass_filter(origin).unwrap_err().error,
                DispatchError::BadOrigin
            );
            assert_eq!(sp_io::storage::root(StateVersion::V1), before);
        }
    });
}

#[test]
fn nft_more_than_1000_locked_burned_records_retire_in_bounded_batches() {
    ext().execute_with(|| {
        assert_ok!(Nfts::create(signed(1), address(1), Default::default()));
        for item in 0..1001 {
            assert_ok!(Nfts::mint(signed(1), 0, item, address(1), None));
            assert_ok!(Nfts::set_metadata(
                signed(1),
                0,
                item,
                vec![1; 128].try_into().unwrap()
            ));
            assert_ok!(Nfts::lock_item_properties(signed(1), 0, item, true, true));
            assert_ok!(Nfts::burn(signed(1), 0, item));
        }
        let d = pallet_nfts::Collection::<Runtime>::get(0).unwrap();
        assert_eq!((d.items, d.item_metadatas, d.item_configs), (0, 1001, 1001));
        assert_noop!(
            Nfts::clear_metadata(signed(1), 0, 0),
            pallet_nfts::Error::<Runtime>::LockedItemMetadata
        );
        let call = RuntimeCall::Nfts(pallet_nfts::Call::destroy {
            collection: 0,
            witness: d.destroy_witness(),
        });
        assert!(IssuanceCallFilter::contains(&call));
        assert_noop!(
            call.dispatch(signed(1)),
            pallet_nfts::Error::<Runtime>::CleanupLimitExceeded
        );
        assert_ok!(Nfts::start_collection_retirement(signed(1), 0));
        let mut batches = 0;
        while pallet_nfts::Collection::<Runtime>::contains_key(0) {
            assert_ok!(Nfts::continue_collection_retirement(
                signed(1),
                0,
                pallet_nfts::CLEANUP_LIMIT
            ));
            batches += 1;
            assert!(batches <= 2005);
        }
        assert_eq!(
            pallet_nfts::ItemMetadataOf::<Runtime>::iter_prefix(0).count(),
            0
        );
        assert_eq!(
            pallet_nfts::ItemConfigOf::<Runtime>::iter_prefix(0).count(),
            0
        );
        assert_eq!(Balances::reserved_balance(who(1)), 0);
    });
}

#[test]
fn nft_delegated_cleanup_rejects_underwitness_before_removal() {
    ext().execute_with(|| {
        assert_ok!(Nfts::create(signed(1), address(1), Default::default()));
        assert_ok!(Nfts::mint(signed(1), 0, 1, address(1), None));
        assert_ok!(Nfts::approve_item_attributes(signed(1), 0, 1, address(2)));
        let namespace = pallet_nfts::AttributeNamespace::Account(who(2));
        for i in 0u32..1001 {
            assert_ok!(Nfts::set_attribute(
                signed(2),
                0,
                Some(1),
                namespace.clone(),
                i.encode().try_into().unwrap(),
                vec![1; 128].try_into().unwrap()
            ));
        }
        let before = sp_io::storage::root(StateVersion::V1);
        // A zero witness now stops at the first raw key without decoding or removing it.
        assert_noop!(
            Nfts::cancel_item_attributes_approval(
                signed(1),
                0,
                1,
                address(2),
                pallet_nfts::CancelAttributesApprovalWitness {
                    account_attributes: 0
                }
            ),
            pallet_nfts::Error::<Runtime>::BadWitness
        );
        assert_eq!(sp_io::storage::root(StateVersion::V1), before);
        assert_noop!(
            Nfts::cancel_item_attributes_approval(
                signed(1),
                0,
                1,
                address(2),
                pallet_nfts::CancelAttributesApprovalWitness {
                    account_attributes: 1001
                }
            ),
            pallet_nfts::Error::<Runtime>::CleanupLimitExceeded
        );
        for _ in 0..1001u32.div_ceil(pallet_nfts::CLEANUP_LIMIT) {
            assert_ok!(Nfts::continue_delegate_cleanup(
                signed(1),
                0,
                1,
                who(2),
                pallet_nfts::CLEANUP_LIMIT
            ));
        }
        assert!(!pallet_nfts::DelegateCleanup::<Runtime>::contains_key(
            0,
            (1, who(2))
        ));
        assert_eq!(
            pallet_nfts::Collection::<Runtime>::get(0)
                .unwrap()
                .attributes,
            0
        );
        assert_eq!(
            pallet_nfts::Attribute::<Runtime>::iter_prefix((0, Some(1), namespace)).count(),
            0
        );
        assert_eq!(Balances::reserved_balance(who(2)), 0);
    });
}

#[test]
fn nft_backend_limits_survive_multisig_and_privileged_weight_replacement() {
    ext().execute_with(|| {
        assert_ok!(Nfts::create(signed(1), address(1), Default::default()));
        let oversize = RuntimeCall::Nfts(pallet_nfts::Call::destroy {
            collection: 0,
            witness: pallet_nfts::DestroyWitness {
                item_metadatas: pallet_nfts::CLEANUP_LIMIT + 1,
                item_configs: 0,
                attributes: 0,
            },
        });
        let expected: DispatchError = pallet_nfts::Error::<Runtime>::CleanupLimitExceeded.into();
        assert_eq!(
            oversize.clone().dispatch(signed(1)).unwrap_err().error,
            expected
        );
        assert_eq!(
            Multisig::as_multi_threshold_1(signed(1), vec![who(2)], Box::new(oversize.clone()))
                .unwrap_err()
                .error,
            expected
        );
        pallet_sudo::Key::<Runtime>::put(who(1));
        let nested = RuntimeCall::Sudo(pallet_sudo::Call::sudo_as {
            who: address(1),
            call: Box::new(oversize),
        });
        assert_ok!(Sudo::sudo_unchecked_weight(
            signed(1),
            Box::new(nested),
            Weight::zero()
        ));
        System::assert_has_event(
            pallet_sudo::Event::<Runtime>::SudoAsDone {
                sudo_result: Err(expected),
            }
            .into(),
        );
        assert!(pallet_nfts::Collection::<Runtime>::contains_key(0));
        // The privilege can still replace outer weight. Backend limits only bound actual cleanup.
        let start =
            RuntimeCall::Nfts(pallet_nfts::Call::start_collection_retirement { collection: 0 });
        assert_eq!(&start.encode()[..2], &[17, 39]);
        assert_ok!(start.dispatch(signed(1)));
        let excessive_batch =
            RuntimeCall::Nfts(pallet_nfts::Call::continue_collection_retirement {
                collection: 0,
                limit: pallet_nfts::CLEANUP_LIMIT + 1,
            });
        let before = pallet_nfts::CollectionRetirement::<Runtime>::get(0);
        assert_eq!(
            excessive_batch.dispatch(signed(1)).unwrap_err().error,
            expected
        );
        assert_eq!(pallet_nfts::CollectionRetirement::<Runtime>::get(0), before);
    });
}

#[test]
fn nft_metadata_preserves_legacy_variants_and_appends_only_authorized_surface() {
    fn variants(registry: &scale_info::PortableRegistry, id: u32, expected: &[&str]) {
        let scale_info::TypeDef::Variant(v) = &registry.resolve(id).unwrap().type_def else {
            panic!("variant type");
        };
        assert_eq!(v.variants.len(), expected.len());
        for (i, (actual, name)) in v.variants.iter().zip(expected).enumerate() {
            assert_eq!(actual.index as usize, i);
            assert_eq!(&actual.name, name);
        }
    }
    let calls = [
        "create",
        "force_create",
        "destroy",
        "mint",
        "force_mint",
        "burn",
        "transfer",
        "redeposit",
        "lock_item_transfer",
        "unlock_item_transfer",
        "lock_collection",
        "transfer_ownership",
        "set_team",
        "force_collection_owner",
        "force_collection_config",
        "approve_transfer",
        "cancel_approval",
        "clear_all_transfer_approvals",
        "lock_item_properties",
        "set_attribute",
        "force_set_attribute",
        "clear_attribute",
        "approve_item_attributes",
        "cancel_item_attributes_approval",
        "set_metadata",
        "clear_metadata",
        "set_collection_metadata",
        "clear_collection_metadata",
        "set_accept_ownership",
        "set_collection_max_supply",
        "update_mint_settings",
        "set_price",
        "buy_item",
        "pay_tips",
        "create_swap",
        "cancel_swap",
        "claim_swap",
        "mint_pre_signed",
        "set_attributes_pre_signed",
        "start_collection_retirement",
        "continue_collection_retirement",
        "continue_delegate_cleanup",
    ];
    let events = [
        "Created",
        "ForceCreated",
        "Destroyed",
        "Issued",
        "Transferred",
        "Burned",
        "ItemTransferLocked",
        "ItemTransferUnlocked",
        "ItemPropertiesLocked",
        "CollectionLocked",
        "OwnerChanged",
        "TeamChanged",
        "TransferApproved",
        "ApprovalCancelled",
        "AllApprovalsCancelled",
        "CollectionConfigChanged",
        "CollectionMetadataSet",
        "CollectionMetadataCleared",
        "ItemMetadataSet",
        "ItemMetadataCleared",
        "Redeposited",
        "AttributeSet",
        "AttributeCleared",
        "ItemAttributesApprovalAdded",
        "ItemAttributesApprovalRemoved",
        "OwnershipAcceptanceChanged",
        "CollectionMaxSupplySet",
        "CollectionMintSettingsUpdated",
        "NextCollectionIdIncremented",
        "ItemPriceSet",
        "ItemPriceRemoved",
        "ItemBought",
        "TipSent",
        "SwapCreated",
        "SwapCancelled",
        "SwapClaimed",
        "PreSignedAttributesSet",
        "PalletAttributeSet",
        "CollectionRetirementStarted",
        "CollectionRetirementProgress",
        "AttributeCountReconciled",
        "DelegateCleanupProgress",
    ];
    let errors = [
        "NoPermission",
        "UnknownCollection",
        "AlreadyExists",
        "ApprovalExpired",
        "WrongOwner",
        "BadWitness",
        "CollectionIdInUse",
        "ItemsNonTransferable",
        "NotDelegate",
        "WrongDelegate",
        "Unapproved",
        "Unaccepted",
        "ItemLocked",
        "LockedItemAttributes",
        "LockedCollectionAttributes",
        "LockedItemMetadata",
        "LockedCollectionMetadata",
        "MaxSupplyReached",
        "MaxSupplyLocked",
        "MaxSupplyTooSmall",
        "UnknownItem",
        "UnknownSwap",
        "MetadataNotFound",
        "AttributeNotFound",
        "NotForSale",
        "BidTooLow",
        "ReachedApprovalLimit",
        "DeadlineExpired",
        "WrongDuration",
        "MethodDisabled",
        "WrongSetting",
        "InconsistentItemConfig",
        "NoConfig",
        "RolesNotCleared",
        "MintNotStarted",
        "MintEnded",
        "AlreadyClaimed",
        "IncorrectData",
        "WrongOrigin",
        "WrongSignature",
        "IncorrectMetadata",
        "MaxAttributesLimitReached",
        "WrongNamespace",
        "CollectionNotEmpty",
        "WitnessRequired",
        "CleanupLimitExceeded",
        "CollectionRetiring",
        "NotRetiring",
        "DelegateCleanupPending",
        "CleanupStateInvalid",
        "RefundFailed",
    ];
    macro_rules! check {
        ($m:expr) => {{
            let m = $m;
            let p = m.pallets.iter().find(|p| p.name == "Nfts").unwrap();
            assert_eq!(p.index, 17);
            variants(&m.types, p.calls.as_ref().unwrap().ty.id, &calls);
            variants(&m.types, p.event.as_ref().unwrap().ty.id, &events);
            variants(&m.types, p.error.as_ref().unwrap().ty.id, &errors);
            let entries = &p.storage.as_ref().unwrap().entries;
            assert_eq!(entries.len(), 17);
            for name in ["CollectionRetirement", "DelegateCleanup"] {
                assert!(entries.iter().any(|e| e.name == name));
            }
            assert!(p.constants.iter().any(
                |c| c.name == "CleanupLimit" && c.value == pallet_nfts::CLEANUP_LIMIT.encode()
            ));
        }};
    }
    match Runtime::metadata().1 {
        frame_metadata::RuntimeMetadata::V14(m) => check!(m),
        frame_metadata::RuntimeMetadata::V15(m) => check!(m),
        _ => panic!("supported metadata version"),
    }
}

#[test]
fn nft_zero_witness_is_bounded_through_the_actual_multisig_item_owner() {
    ext().execute_with(|| {
        let multi = Multisig::multi_account_id(&[who(1), who(2)], 1);
        assert_ok!(Nfts::create(signed(1), address(1), Default::default()));
        assert_ok!(Nfts::mint(
            signed(1),
            0,
            0,
            MultiAddress::Id(multi.clone()),
            None
        ));
        assert_ok!(Nfts::approve_item_attributes(
            RuntimeOrigin::signed(multi),
            0,
            0,
            address(2)
        ));
        let namespace = pallet_nfts::AttributeNamespace::Account(who(2));
        for i in 0u32..1001 {
            assert_ok!(Nfts::set_attribute(
                signed(2),
                0,
                Some(0),
                namespace.clone(),
                i.encode().try_into().unwrap(),
                vec![255; 128].try_into().unwrap()
            ));
        }
        let reserved = Balances::reserved_balance(who(2));
        let call = RuntimeCall::Nfts(pallet_nfts::Call::cancel_item_attributes_approval {
            collection: 0,
            item: 0,
            delegate: address(2),
            witness: pallet_nfts::CancelAttributesApprovalWitness {
                account_attributes: 0,
            },
        });
        assert_eq!(
            Multisig::as_multi_threshold_1(signed(1), vec![who(2)], Box::new(call))
                .unwrap_err()
                .error,
            pallet_nfts::Error::<Runtime>::BadWitness.into()
        );
        assert_eq!(Balances::reserved_balance(who(2)), reserved);
        assert_eq!(
            pallet_nfts::Attribute::<Runtime>::iter_prefix((0, Some(0), namespace)).count(),
            1001
        );
    });
}

#[test]
fn nft_measured_cleanup_weights_fit_actual_normal_extrinsic_budget() {
    use pallet_nfts::WeightInfo as NftWeightInfo;
    use sp_runtime::traits::TransactionExtension;
    type NftWeights = <Runtime as pallet_nfts::Config>::WeightInfo;
    let limit = pallet_nfts::CLEANUP_LIMIT;
    assert_eq!(limit, 685);
    let blocks: frame_system::limits::BlockWeights =
        <Runtime as frame_system::Config>::BlockWeights::get();
    let maximum = blocks.get(DispatchClass::Normal).max_extrinsic.unwrap();
    assert_eq!(maximum.ref_time(), 649_891_843_000);
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
    let extension = extra.weight(&RuntimeCall::Nfts(
        pallet_nfts::Call::start_collection_retirement { collection: 0 },
    ));
    assert_eq!(extension.ref_time(), 12_957_981_000);
    println!("NFT_ADMISSION maximum={maximum:?} signed_extension={extension:?}");
    let cases = [
        ("destroy", NftWeights::destroy(limit, limit, limit)),
        ("cancel", NftWeights::cancel_item_attributes_approval(limit)),
        ("start", NftWeights::start_collection_retirement()),
        (
            "retirement",
            NftWeights::continue_collection_retirement(limit),
        ),
        ("delegate", NftWeights::continue_delegate_cleanup(limit)),
        ("burn", NftWeights::burn()),
        ("team", NftWeights::set_team()),
    ];
    for (name, weight) in cases {
        assert!(weight.ref_time() > 0 && weight.proof_size() > 0, "{name}");
        assert!(
            weight.saturating_add(extension).all_lte(maximum),
            "{name}: call={weight:?}, extension={extension:?}, maximum={maximum:?}"
        );
    }
    // Fixed full-domain envelopes conservatively charge small and rejected oversized requests.
    for n in [0, 1, limit, limit + 1, u32::MAX] {
        assert_eq!(
            NftWeights::destroy(n, n, n),
            NftWeights::destroy(limit, limit, limit)
        );
        assert_eq!(
            NftWeights::cancel_item_attributes_approval(n),
            NftWeights::cancel_item_attributes_approval(limit)
        );
        assert_eq!(
            NftWeights::continue_collection_retirement(n),
            NftWeights::continue_collection_retirement(limit)
        );
        assert_eq!(
            NftWeights::continue_delegate_cleanup(n),
            NftWeights::continue_delegate_cleanup(limit)
        );
    }
    let calls = [
        (
            RuntimeCall::Nfts(pallet_nfts::Call::destroy {
                collection: 0,
                witness: pallet_nfts::DestroyWitness {
                    item_metadatas: limit,
                    item_configs: limit,
                    attributes: limit,
                },
            }),
            NftWeights::destroy(limit, limit, limit),
        ),
        (
            RuntimeCall::Nfts(pallet_nfts::Call::cancel_item_attributes_approval {
                collection: 0,
                item: 0,
                delegate: address(2),
                witness: pallet_nfts::CancelAttributesApprovalWitness {
                    account_attributes: limit,
                },
            }),
            NftWeights::cancel_item_attributes_approval(limit),
        ),
        (
            RuntimeCall::Nfts(pallet_nfts::Call::start_collection_retirement { collection: 0 }),
            NftWeights::start_collection_retirement(),
        ),
        (
            RuntimeCall::Nfts(pallet_nfts::Call::continue_collection_retirement {
                collection: 0,
                limit,
            }),
            NftWeights::continue_collection_retirement(limit),
        ),
        (
            RuntimeCall::Nfts(pallet_nfts::Call::continue_delegate_cleanup {
                collection: 0,
                item: 0,
                delegate: who(2),
                limit,
            }),
            NftWeights::continue_delegate_cleanup(limit),
        ),
    ];
    for (call, expected) in calls {
        assert_eq!(call.get_dispatch_info().class, DispatchClass::Normal);
        assert_eq!(call.get_dispatch_info().call_weight, expected);
        let mut info = call.get_dispatch_info();
        info.extension_weight = extra.weight(&call);
        ext().execute_with(|| {
            assert_ok!(frame_system::CheckWeight::<Runtime>::do_validate(
                &info,
                call.encoded_size() + 128
            ));
        });
    }
    let schema =
        pallet_nfts::weights::measured_schema(<Runtime as frame_system::Config>::DbWeight::get());
    assert!(schema.ref_time() > 0 && schema.all_lte(maximum));
}
