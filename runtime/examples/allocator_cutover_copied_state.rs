//! Offline successor-upgrade rehearsal over the authenticated retained spec15 state copy.
//! No RPC, historical execution, signing or production mutation.
use codec::Encode;
use era_runtime::{Balances, Executive, System};
use sp_runtime::StateVersion;
use std::{collections::BTreeMap, env, fs};

fn from_hex(input: &str) -> Vec<u8> {
    let digits = input.strip_prefix("0x").expect("0x-prefixed hex").as_bytes();
    assert_eq!(digits.len() % 2, 0, "even hex length");
    digits.chunks_exact(2).map(|pair| {
        let hi = (pair[0] as char).to_digit(16).expect("hex digit");
        let lo = (pair[1] as char).to_digit(16).expect("hex digit");
        ((hi << 4) | lo) as u8
    }).collect()
}

fn top_state() -> BTreeMap<Vec<u8>, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut cursor = Vec::new();
    while let Some(key) = sp_io::storage::next_key(&cursor) {
        let value = sp_io::storage::get(&key).expect("enumerated key has value");
        out.insert(key.clone(), value.to_vec());
        cursor = key;
    }
    out
}

fn main() {
    let mut args = env::args().skip(1);
    let state_path = args.next().expect("retained production-state.json path");
    let header_path = args.next().expect("authenticated retained-state header path");
    assert!(args.next().is_none(), "exactly two paths");
    let state_json: serde_json::Value = serde_json::from_slice(&fs::read(state_path).expect("read retained state")).expect("parse retained state");
    let header_json: serde_json::Value = serde_json::from_slice(&fs::read(header_path).expect("read retained header")).expect("parse retained header");
    let summary = &state_json["summary"];
    assert_eq!(summary["finalized_head"], header_json["requestedHash"]);
    assert_eq!(summary["finalized_height"], header_json["height"]);
    assert_eq!(summary["runtime"]["specVersion"].as_u64(), Some(15));
    assert_eq!(summary["runtime"]["stateVersion"].as_u64(), Some(1));
    assert_eq!(summary["onchain_code_sha256"].as_str(), Some("122af167022227c46b2b74d99f5d3a73f41f8d65bb4a8a1de7b88b6e986de2af"));
    let pairs = state_json["state"].as_object().expect("state key-value map");
    assert!(pairs.len() > 0);
    if let Some(expected) = summary["state_keys"].as_u64() {
        assert_eq!(pairs.len() as u64, expected);
    }
    if let Some(expected) = summary["keys"].as_u64() {
        assert_eq!(pairs.len() as u64, expected);
    }
    let mut storage = sp_core::storage::Storage::default();
    for (key, value) in pairs {
        storage.top.insert(from_hex(key), from_hex(value.as_str().expect("storage value hex")));
    }
    let expected_root = from_hex(header_json["header"]["stateRoot"].as_str().expect("header state root"));
    // The default constructor overwrites :code with empty bytes and uses V0.
    // Reinsert the authenticated code and construct the backend with V1.
    let code = storage.top.remove(b":code".as_slice()).expect("export includes :code");
    let mut ext = sp_io::TestExternalities::new_with_code_and_state(&code, storage, StateVersion::V1);
    ext.execute_with(|| {
        let before_root = sp_io::storage::root(StateVersion::V1);
        if before_root != expected_root {
            eprintln!("state root diagnostic: imported_keys={} enumerated_keys={} v0={:02x?} v1={:02x?} expected={:02x?}",
                pairs.len(), top_state().len(),
                sp_io::storage::root(StateVersion::V0),
                before_root, expected_root);
        }
        assert_eq!(before_root, expected_root, "retained state must match finalized header root");
        let before = top_state();
        assert_eq!(before.len(), pairs.len());
        let cutover_key = frame_support::storage::storage_prefix(b"EraV14AssetAllocator", b"LegacyCreationClosedAt").to_vec();
        let schema_key = frame_support::storage::storage_prefix(b"EraV14AssetAllocator", b"SchemaVersion").to_vec();
        assert!(sp_io::storage::get(&cutover_key).is_none());
        let issuance_before = Balances::total_issuance();
        let marker_height = System::block_number().saturating_add(1);
        Executive::execute_on_runtime_upgrade();
        assert_eq!(sp_io::storage::get(&cutover_key).as_ref().map(|v| v.as_ref()), Some(marker_height.encode().as_slice()));
        assert!(sp_io::storage::get(&schema_key).is_none());
        assert_eq!(Balances::total_issuance(), issuance_before);
        let after = top_state();
        let changed: Vec<_> = before.iter().filter(|(key, value)| after.get(*key) != Some(*value)).map(|(key, _)| key.clone()).collect();
        let added: Vec<_> = after.keys().filter(|key| !before.contains_key(*key)).cloned().collect();
        assert!(changed.is_empty(), "pre-existing storage changed during cutover: {} keys", changed.len());
        assert_eq!(added, vec![cutover_key], "only the cutover marker may be added");
        assert_eq!(after.len(), before.len() + 1);
        println!("PASS copied_state_keys={} preserved_old_keys={} added_cutover_key=1 block_height={}", before.len(), before.len(), marker_height);
    });
}
