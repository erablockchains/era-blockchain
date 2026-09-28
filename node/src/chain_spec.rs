//! ERA node chain specs (Polkadot SDK v1.19.1)
//! Raw-genesis approach, staking + session at genesis (no explicit BABE
//! GenesisConfig needed on this SDK line).

use std::collections::{BTreeMap, HashMap};

use sc_chain_spec::Properties;
use serde_json::{json, Value};

use sp_core::{ed25519, sr25519, Pair};
use sp_runtime::traits::{IdentifyAccount, Verify};
use sp_runtime::BuildStorage; // for .build_storage()

// v1.19.x: `GenericChainSpec` has *no* runtime generic; we feed a JSON object.
pub type ChainSpec = sc_chain_spec::GenericChainSpec;

use era_runtime::{
    AccountId,
    Balance,
    BalancesConfig,
    GrandpaConfig, // <-- we still keep GRANDPA in genesis, via Default
    RuntimeGenesisConfig as GenesisConfig,
    SessionConfig,
    SessionKeys,
    StakingConfig,
    SudoConfig,
    SystemConfig,
    TransactionPaymentConfig,
    VestingConfig,
    DECIMALS,
};

use pallet_staking::StakerStatus;

// ---------- helpers ----------

type Signature = sp_runtime::MultiSignature;
type AccountPublic = <Signature as Verify>::Signer;

fn sr25519_pub(seed: &str) -> sr25519::Public {
    sr25519::Pair::from_string(&format!("//{seed}"), None)
        .expect("valid dev seed")
        .public()
}

fn ed25519_pub(seed: &str) -> ed25519::Public {
    ed25519::Pair::from_string(&format!("//{seed}"), None)
        .expect("valid dev seed")
        .public()
}

fn account_id(seed: &str) -> AccountId {
    AccountPublic::from(sr25519_pub(seed)).into_account()
}

fn session_keys(babe: sr25519::Public, grandpa: ed25519::Public) -> SessionKeys {
    SessionKeys {
        babe: babe.into(),
        grandpa: grandpa.into(),
    }
}

fn props() -> Properties {
    let mut p = Properties::new();
    p.insert("tokenSymbol".into(), "ERA".into());
    p.insert("tokenDecimals".into(), 18.into());
    p.insert("ss58Format".into(), 42.into());
    p.insert("tokenUnit".into(), "nakamishi".into());
    p
}

// Dedup/merge helper for balances
fn add_balance(map: &mut BTreeMap<AccountId, Balance>, who: AccountId, amount: Balance) {
    *map.entry(who).or_insert(0) += amount;
}

// ---------- public constructors ----------

pub fn development_config() -> ChainSpec {
    build_development_spec()
}

fn build_development_spec() -> ChainSpec {
    // 1) Build the development-only typed genesis config in Rust.
    let full: GenesisConfig = runtime_genesis();

    // 2) Turn it into raw storage (top + children_default).
    let mut storage = full
        .build_storage()
        .expect("RuntimeGenesisConfig -> storage must succeed");

    // 3) Inject the Wasm code at the canonical `:code` key.
    #[cfg(feature = "runtime-benchmarks")]
    let wasm: &'static [u8] = era_runtime::WASM_BINARY.unwrap_or_default();
    #[cfg(not(feature = "runtime-benchmarks"))]
    let wasm: &'static [u8] = era_runtime::WASM_BINARY.expect(
        "runtime wasm not generated; ensure runtime/build.rs and \
         #[cfg(feature = \"std\")] include!(.../wasm_binary.rs)",
    );
    storage.top.insert(b":code".to_vec(), wasm.to_vec());

    // 4) Convert storage maps into hex-keyed JSON for a RAW chainspec.
    let top_hex = map_to_hex_json(storage.top);
    let children_default_json = children_to_hex_json(storage.children_default);

    let spec = json!({
        "name": "era-dev",
        "id": "era-dev",
        "chainType": "Development",
        "bootNodes": [],
        "telemetryEndpoints": null,
        "protocolId": null,
        "properties": props(),
        "forkBlocks": null,
        "badBlocks": null,
        "codeSubstitutes": {},
        "genesis": {
            "raw": {
                "top": top_hex,
                "childrenDefault": children_default_json
            }
        }
    });

    sc_chain_spec::GenericChainSpec::from_json_bytes(
        serde_json::to_vec(&spec).expect("serialize spec"),
    )
    .expect("valid spec json")
}

fn map_to_hex_json(map: BTreeMap<Vec<u8>, Vec<u8>>) -> Value {
    use serde_json::Map;
    let mut out = Map::new();
    for (k, v) in map.into_iter() {
        out.insert(
            format!("0x{}", hex::encode(k)),
            Value::String(format!("0x{}", hex::encode(v))),
        );
    }
    Value::Object(out)
}

fn children_to_hex_json(children: HashMap<Vec<u8>, sp_runtime::StorageChild>) -> Value {
    use serde_json::Map;
    let mut outer = Map::new();
    for (child_key, child_store) in children.into_iter() {
        let mut child_top = BTreeMap::<String, String>::new();
        for (k, v) in child_store.data.into_iter() {
            child_top.insert(
                format!("0x{}", hex::encode(k)),
                format!("0x{}", hex::encode(v)),
            );
        }
        outer.insert(
            format!("0x{}", hex::encode(child_key)),
            json!({ "top": child_top }),
        );
    }
    Value::Object(outer)
}

// ---------- genesis ----------

fn authority(seed: &str) -> (AccountId, sr25519::Public, ed25519::Public) {
    (account_id(seed), sr25519_pub(seed), ed25519_pub(seed))
}

fn runtime_genesis() -> GenesisConfig {
    let (validator, babe, grandpa) = authority("Alice");
    let sudo = account_id("Alice");

    // Preserve the existing development genesis balances exactly. Duplicate roles are merged.
    let mut balances_map: BTreeMap<AccountId, Balance> = BTreeMap::new();
    add_balance(&mut balances_map, account_id("Alice"), 1_000_000 * DECIMALS);
    add_balance(&mut balances_map, account_id("Bob"), 1_000_000 * DECIMALS);
    add_balance(&mut balances_map, sudo.clone(), 1_000_000 * DECIMALS);
    add_balance(&mut balances_map, validator.clone(), 1_000_000 * DECIMALS);
    let balances: Vec<(AccountId, Balance)> = balances_map.into_iter().collect();

    let session = vec![(
        validator.clone(),
        validator.clone(),
        session_keys(babe, grandpa),
    )];

    // Staking is SessionManager, so this seeds development session 0.
    let bond: Balance = 100_000 * DECIMALS;
    let staking = StakingConfig {
        validator_count: 1,
        minimum_validator_count: 1,
        invulnerables: vec![validator.clone()],
        stakers: vec![(validator.clone(), validator, bond, StakerStatus::Validator)],
        ..Default::default()
    };

    GenesisConfig {
        // Wasm is injected into raw storage, not here.
        system: SystemConfig::default(),
        balances: BalancesConfig {
            balances,
            dev_accounts: None,
        },
        session: SessionConfig {
            keys: session,
            non_authority_keys: vec![],
        },
        sudo: SudoConfig { key: Some(sudo) },
        staking,
        transaction_payment: TransactionPaymentConfig::default(),
        vesting: VestingConfig { vesting: vec![] },
        // GRANDPA authorities come via Session.
        grandpa: GrandpaConfig::default(),
        ..Default::default()
    }
}

/// Generate the candidate from public identities only. No database or keystore is opened.
pub fn fresh_spec(inputs: &era_runtime::fresh_genesis_inputs::Inputs) -> Result<ChainSpec, String> {
    let genesis=inputs.genesis()?;
    let mut storage=genesis.build_storage()?;
    storage.top.insert(b":code".to_vec(),era_runtime::WASM_BINARY.ok_or("fresh runtime Wasm missing")?.to_vec());
    let id=if inputs.development {"era-v14-relaunch-dev-20260914"} else {"era-v14-20260914"};
    let spec=json!({
        "name":"ERA", "id":id,"chainType":if inputs.development {"Development"} else {"Live"},
        "bootNodes":[],"telemetryEndpoints":null,"protocolId":id,
        "properties":{"tokenSymbol":"ETKN","tokenDecimals":18,"ss58Format":42},
        "codeSubstitutes":{},"genesis":{"raw":{"top":map_to_hex_json(storage.top),"childrenDefault":children_to_hex_json(storage.children_default)}}
    });
    ChainSpec::from_json_bytes(serde_json::to_vec(&spec).map_err(|e|e.to_string())?)
}
