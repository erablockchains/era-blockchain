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
    build_spec(true)
}

pub fn era_prod_config() -> ChainSpec {
    build_spec(false)
}

fn build_spec(is_dev: bool) -> ChainSpec {
    // 1) Build typed genesis config in-Rust.
    let full: GenesisConfig = runtime_genesis(is_dev);

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

    // 4) Convert storage maps into hex-keyed JSON for a RAW chainspec:
    let top_hex = map_to_hex_json(storage.top);
    let children_default_json = children_to_hex_json(storage.children_default);

    let name = if is_dev { "era-dev" } else { "era-prod" };
    let id = name;

    let spec = json!({
        "name": name,
        "id": id,
        "chainType": if is_dev { "Development" } else { "Live" },
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

fn runtime_genesis(is_dev: bool) -> GenesisConfig {
    let (val1_acc, val1_babe, val1_grandpa) = if is_dev {
        authority("Alice")
    } else {
        authority("EraOne")
    };
    let maybe_val2 = if is_dev {
        None
    } else {
        Some(authority("EraTwo"))
    };

    let sudo_acc = if is_dev {
        account_id("Alice")
    } else {
        account_id("Sudo")
    };

    let presale = account_id("Presale");
    let ecosystem = account_id("Ecosystem");
    let airdrop = account_id("Airdrop");
    let liquidity = account_id("Liquidity");

    let v1 = account_id("Vester1");
    let v2 = account_id("Vester2");
    let v3 = account_id("Vester3");
    let v4 = account_id("Vester4");
    let v5 = account_id("Vester5");

    // --- balances (deduped so no panics) ---
    let mut balances_map: BTreeMap<AccountId, Balance> = BTreeMap::new();

    if is_dev {
        add_balance(&mut balances_map, account_id("Alice"), 1_000_000 * DECIMALS);
        add_balance(&mut balances_map, account_id("Bob"), 1_000_000 * DECIMALS);
    } else {
        add_balance(&mut balances_map, presale.clone(), 200_000_000 * DECIMALS);
        add_balance(&mut balances_map, ecosystem.clone(), 150_000_000 * DECIMALS);
        add_balance(&mut balances_map, airdrop.clone(), 300_000_000 * DECIMALS);
        add_balance(&mut balances_map, liquidity.clone(), 150_000_000 * DECIMALS);

        add_balance(&mut balances_map, v1.clone(), 80_000_000 * DECIMALS);
        add_balance(&mut balances_map, v2.clone(), 30_000_000 * DECIMALS);
        add_balance(&mut balances_map, v3.clone(), 30_000_000 * DECIMALS);
        add_balance(&mut balances_map, v4.clone(), 30_000_000 * DECIMALS);
        add_balance(&mut balances_map, v5.clone(), 30_000_000 * DECIMALS);
    }

    // Always fund sudo + validators (merges if present already)
    add_balance(&mut balances_map, sudo_acc.clone(), 1_000_000 * DECIMALS);
    add_balance(&mut balances_map, val1_acc.clone(), 1_000_000 * DECIMALS);
    if let Some((val2_acc, _, _)) = maybe_val2.as_ref() {
        add_balance(&mut balances_map, val2_acc.clone(), 1_000_000 * DECIMALS);
    }

    let balances: Vec<(AccountId, Balance)> = balances_map.into_iter().collect();

    // --- session keys ---
    let mut session = vec![(
        val1_acc.clone(),
        val1_acc.clone(),
        session_keys(val1_babe, val1_grandpa),
    )];

    if let Some((val2_acc, val2_babe, val2_grandpa)) = maybe_val2.as_ref() {
        session.push((
            val2_acc.clone(),
            val2_acc.clone(),
            session_keys(*val2_babe, *val2_grandpa),
        ));
    }

    // --- staking genesis ---
    // Make Alice validator for dev. Staking is SessionManager, so this seeds session 0.
    let staking_cfg: StakingConfig = if is_dev {
        let stash = val1_acc.clone();
        let controller = val1_acc.clone();
        let bond: Balance = 100_000 * DECIMALS;

        StakingConfig {
            validator_count: 1,
            minimum_validator_count: 1,
            invulnerables: vec![stash.clone()],
            stakers: vec![(
                stash.clone(),
                controller.clone(),
                bond,
                StakerStatus::Validator,
            )],
            ..Default::default()
        }
    } else {
        StakingConfig {
            validator_count: 2,
            minimum_validator_count: 1,
            ..Default::default()
        }
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

        sudo: SudoConfig {
            key: Some(sudo_acc),
        },

        staking: staking_cfg,
        transaction_payment: TransactionPaymentConfig::default(),
        vesting: VestingConfig { vesting: vec![] },

        // GRANDPA: keep default genesis (authorities come via Session)
        grandpa: GrandpaConfig::default(),

        // Any other pallets get their defaults:
        ..Default::default()
    }
}
