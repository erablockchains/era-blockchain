//! Generate a synthetic, runtime-compatible Phase 6A (storage v2) snapshot for offline try-runtime.

use codec::{Compact, Encode};
use era_runtime::{AccountId, Header, Runtime, RuntimeGenesisConfig, System};
use frame_support::traits::StorageVersion;
use pallet_ai_predictions::{
    AiModel, ModelStats, ModelStatsById, Models, NextModelId, NextPredictionId, Prediction,
    PredictionDomain, PredictionMarketAccounting, PredictionMarketEconomicsConfig,
    PredictionMarketEconomicsSettings, PredictionOutcomeSide, PredictionSidePayoutClaim,
    PredictionSideStake, PredictionStatus, PredictionTokenMarketAccounting,
    PredictionTokenSidePayoutClaims, PredictionTokenSideStakes, Predictions,
};
use sp_runtime::{traits::Header as HeaderT, BuildStorage, Digest, StateVersion};

fn main() {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/era-phase6b-offline-try-runtime/phase6a-v2-synthetic.snap".into());
    let old_runtime_path = std::env::args()
        .nth(2)
        .expect("path to the archived Phase 6A spec-8 runtime WASM");
    let old_runtime = std::fs::read(old_runtime_path).expect("read old runtime WASM");
    let old_runtime_len = old_runtime.len();
    let storage = RuntimeGenesisConfig::default()
        .build_storage()
        .expect("runtime genesis storage");
    let mut ext = sp_io::TestExternalities::new(storage);
    ext.execute_with(|| {
        sp_io::storage::set(sp_core::storage::well_known_keys::CODE, &old_runtime);
        assert_eq!(
            sp_io::storage::get(sp_core::storage::well_known_keys::CODE)
                .unwrap()
                .len(),
            old_runtime_len,
            "snapshot old runtime code length",
        );
        System::set_block_number(1);
        let alice = AccountId::new([1u8; 32]);
        let bob = AccountId::new([2u8; 32]);
        for (model_id, owner, active) in [(0, alice.clone(), true), (1, bob, false)] {
            Models::<Runtime>::insert(
                model_id,
                AiModel {
                    owner,
                    model_hash: vec![model_id as u8 + 1; 32].try_into().unwrap(),
                    metadata_uri: format!("ipfs://phase6a-model-{model_id}")
                        .into_bytes()
                        .try_into()
                        .unwrap(),
                    registered_at: 1,
                    updated_at: None,
                    active,
                },
            );
            ModelStatsById::<Runtime>::insert(model_id, ModelStats::default());
        }
        NextModelId::<Runtime>::put(2);
        for (prediction_id, status) in [(0, PredictionStatus::Open), (1, PredictionStatus::Closed)]
        {
            Predictions::<Runtime>::insert(
                prediction_id,
                Prediction {
                    model_id: 0,
                    submitter: alice.clone(),
                    domain: PredictionDomain::Finance,
                    category_code: b"finance.synthetic-v2".to_vec().try_into().unwrap(),
                    prediction_hash: vec![prediction_id as u8 + 11; 32].try_into().unwrap(),
                    metadata_uri: format!("ipfs://phase6a-prediction-{prediction_id}")
                        .into_bytes()
                        .try_into()
                        .unwrap(),
                    confidence: 80,
                    created_at: 1,
                    expires_at: 100,
                    status,
                    outcome: None,
                    validator: None,
                    validated_at: None,
                },
            );
        }
        NextPredictionId::<Runtime>::put(2);
        PredictionMarketEconomicsConfig::<Runtime>::put(PredictionMarketEconomicsSettings {
            market_enabled: true,
            allow_unstake_before_settlement: false,
            fees_enabled: true,
            fee_bps: 125,
            treasury_enabled: true,
        });
        PredictionTokenMarketAccounting::<Runtime>::insert(
            7,
            PredictionMarketAccounting {
                fees_collected: 3,
                remainders_collected: 2,
                total_paid: 90,
                total_refunded: 4,
                total_slashed: 10,
            },
        );
        PredictionTokenSideStakes::<Runtime>::insert(
            7,
            &alice,
            PredictionSideStake {
                side: PredictionOutcomeSide::Yes,
                amount: 50,
                staked_at: 1,
                updated_at: None,
            },
        );
        PredictionTokenSidePayoutClaims::<Runtime>::insert(
            7,
            &alice,
            PredictionSidePayoutClaim {
                claimed_at: 2,
                stake_returned: 50,
                reward_paid: 10,
                slashed: 0,
            },
        );
        StorageVersion::new(2).put::<pallet_ai_predictions::Pallet<Runtime>>();
    });
    ext.commit_all().expect("commit synthetic state overlay");
    ext.execute_with(|| {
        assert_eq!(
            StorageVersion::get::<pallet_ai_predictions::Pallet<Runtime>>(),
            StorageVersion::new(2),
            "committed snapshot storage version",
        );
    });
    let (raw_storage, storage_root) = ext.into_raw_snapshot();
    let header = Header::new(
        1,
        Default::default(),
        storage_root,
        Default::default(),
        Digest::default(),
    );
    let snapshot = (
        Compact(4u16),
        StateVersion::V1,
        raw_storage,
        storage_root,
        header,
    )
        .encode();
    if let Some(parent) = std::path::Path::new(&path).parent() {
        std::fs::create_dir_all(parent).expect("snapshot directory");
    }
    std::fs::write(&path, snapshot).expect("write snapshot");
    println!("{path}");
}
