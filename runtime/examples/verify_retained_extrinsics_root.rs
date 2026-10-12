//! Verify retained block bodies against header extrinsics roots without RPC or execution.
use sp_runtime::{traits::{BlakeTwo256, Hash}, StateVersion};
use std::{env, fs};

fn bytes(value: &str) -> Result<Vec<u8>, String> {
    let raw = value.strip_prefix("0x").ok_or("hex lacks 0x")?.as_bytes();
    if raw.len() % 2 != 0 { return Err("odd hex length".into()); }
    raw.chunks_exact(2).map(|pair| {
        let digit = |c: u8| (c as char).to_digit(16).ok_or_else(|| "bad hex digit".to_string());
        Ok(((digit(pair[0])? << 4) | digit(pair[1])?) as u8)
    }).collect()
}

fn check(header: &serde_json::Value, extrinsics: &serde_json::Value, height: u64) -> Result<(), String> {
    let expected = header["extrinsicsRoot"].as_str().ok_or("missing extrinsicsRoot")?;
    let items = extrinsics.as_array().ok_or("missing extrinsic array")?;
    let mut encoded = Vec::with_capacity(items.len());
    for item in items {
        let (hex, expected_len) = if let Some(hex) = item.as_str() {
            (hex, None)
        } else {
            (item["prefix"].as_str().ok_or("missing retained bytes")?,
             Some(item["bytes"].as_u64().ok_or("missing retained byte count")?))
        };
        let raw = bytes(hex)?;
        if expected_len.is_some_and(|n| n != raw.len() as u64) {
            return Err(format!("height {height}: sample stores only a prefix"));
        }
        encoded.push(raw);
    }
    let actual = format!("{:#x}", BlakeTwo256::ordered_trie_root(encoded, StateVersion::V0));
    if !actual.eq_ignore_ascii_case(expected) {
        return Err(format!("height {height}: extrinsics root mismatch: expected {expected}, got {actual}"));
    }
    println!("PASS height={height} extrinsics={} root={actual}", items.len());
    Ok(())
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let path = args.next().ok_or("one retained JSON path required")?;
    if args.next().is_some() { return Err("exactly one retained JSON path required".into()); }
    let data: serde_json::Value = serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    if let Some(blocks) = data["blocks"].as_array() {
        if blocks.is_empty() { return Err("empty sample".into()); }
        for block in blocks {
            check(&block["header"], &block["extrinsics"], block["height"].as_u64().ok_or("missing height")?)?;
        }
    } else {
        let block = &data["block"];
        let height = u64::from_str_radix(block["header"]["number"].as_str().ok_or("missing number")?
            .trim_start_matches("0x"), 16).map_err(|e| e.to_string())?;
        check(&block["header"], &block["extrinsics"], height)?;
    }
    Ok(())
}

fn main() {
    if let Err(e) = run() { eprintln!("FAIL_CLOSED: {e}"); std::process::exit(1); }
}
