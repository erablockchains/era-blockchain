//! Offline adoption rehearsal from a complete, public, finalized top-state snapshot.
//! No sockets, database, keys or signed transactions. Reject unsupported child tries.
use parity_scale_codec::{Encode,Decode};
use sp_core::{H256,traits::{CallContext,CodeExecutor,ReadRuntimeVersionExt,RuntimeCode,WrappedRuntimeCode}};
use sp_runtime::{traits::Header as _,Digest,DigestItem,StateVersion};
use sp_consensus_babe::digests::{CompatibleDigestItem,PreDigest,SecondaryPlainPreDigest};
use sc_executor::WasmExecutor;
use std::{collections::BTreeMap,path::Path};
fn raw(s:&str)->Result<Vec<u8>,String>{hex::decode(s.strip_prefix("0x").ok_or("hex prefix")?).map_err(|e|e.to_string())}
fn state(ext:&mut sp_io::TestExternalities)->BTreeMap<Vec<u8>,Vec<u8>>{ext.execute_with(||{let mut m=BTreeMap::new();let mut last=Vec::new();while let Some(k)=sp_io::storage::next_key(&last){m.insert(k.clone(),sp_io::storage::get(&k).unwrap().to_vec());last=k;}m})}
pub fn run(snapshot:&Path,wasm:&Path,genesis:H256)->Result<serde_json::Value,String>{
 let doc:serde_json::Value=serde_json::from_slice(&std::fs::read(snapshot).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
 if doc["genesis"].as_str()!=Some(format!("{genesis:?}").as_str()){return Err("snapshot genesis mismatch".into())}
 let header:era_runtime::Header=serde_json::from_value(doc["header"].clone()).map_err(|e|e.to_string())?;
 if doc["finalized"].as_str()!=Some(format!("{:?}",header.hash()).as_str()){return Err("header hash mismatch".into())}
 if doc["runtime"]["specVersion"]!=14{return Err("requires deployed spec14 snapshot".into())}
 let mut top=BTreeMap::new();for(k,v)in doc["state"].as_object().ok_or("state map")?{let k=raw(k)?;if k.starts_with(b":child_storage:"){return Err("child trie capture unsupported; cannot claim complete state".into())}top.insert(k,raw(v.as_str().ok_or("storage value")?)?);}
 let old_code=top.get(b":code".as_slice()).ok_or("missing code")?.clone();let version=match doc["runtime"]["stateVersion"].as_u64(){Some(0)=>StateVersion::V0,Some(1)=>StateVersion::V1,_=>return Err("state version".into())};
 let mut ext=sp_io::TestExternalities::new_with_code_and_state(&old_code,sp_core::storage::Storage{top,children_default:Default::default()},version);
 let root=ext.execute_with(||sp_io::storage::root(version));if root.as_slice()!=header.state_root().as_bytes(){return Err("incomplete snapshot: state root mismatch".into())}
 if ext.execute_with(||era_runtime::System::block_hash(0))!=genesis{return Err("snapshot System::BlockHash(0) does not bind the approved genesis".into())}
 let before=state(&mut ext);let code=std::fs::read(wasm).map_err(|e|e.to_string())?;if code.len()>4*1024*1024{return Err("upgrade proposal exceeds reviewed 4MiB artifact bound".into())}
 ext.register_extension(ReadRuntimeVersionExt::new(WasmExecutor::<sp_io::SubstrateHostFunctions>::builder().build()));
 ext.execute_with(||sp_io::storage::set(b":code",&code));
 let slot=ext.execute_with(||era_runtime::Babe::current_slot());
 let next=era_runtime::Header::new(header.number().checked_add(1).ok_or("block overflow")?,H256::zero(),H256::zero(),header.hash(),Digest{logs:vec![DigestItem::babe_pre_digest(PreDigest::SecondaryPlain(SecondaryPlainPreDigest{authority_index:0,slot:(u64::from(slot)+1).into()}))]});
 let executor=WasmExecutor::<sp_io::SubstrateHostFunctions>::builder().build();let wrapped=WrappedRuntimeCode(code.as_slice().into());let runtime=RuntimeCode{code_fetcher:&wrapped,heap_pages:None,hash:sp_core::blake2_256(&code).to_vec()};
 let (ver,_)=executor.call(&mut ext.ext(),&runtime,"Core_version",&[],CallContext::Onchain);let ver=ver.map_err(|e|format!("candidate version: {e:?}"))?;let ver=sc_executor::RuntimeVersion::decode(&mut &ver[..]).map_err(|e|e.to_string())?;
 if ver.spec_version!=15 || ver.transaction_version!=1 || ver.spec_name!=era_runtime::VERSION.spec_name{return Err("candidate runtime identity/version mismatch".into())}
 let mut control=sp_io::TestExternalities::new_with_code_and_state(&old_code,sp_core::storage::Storage{top:before.clone(),children_default:Default::default()},version);
 control.register_extension(ReadRuntimeVersionExt::new(WasmExecutor::<sp_io::SubstrateHostFunctions>::builder().build()));
 let old_wrapped=WrappedRuntimeCode(old_code.as_slice().into());let old_runtime=RuntimeCode{code_fetcher:&old_wrapped,heap_pages:None,hash:sp_core::blake2_256(&old_code).to_vec()};
 executor.call(&mut control.ext(),&old_runtime,"Core_initialize_block",&next.encode(),CallContext::Onchain).0.map_err(|e|format!("old runtime control: {e:?}"))?;let control_state=state(&mut control);
 let (result,native)=executor.call(&mut ext.ext(),&runtime,"Core_initialize_block",&next.encode(),CallContext::Onchain);result.map_err(|e|format!("Wasm initialize: {e:?}"))?;if native{return Err("unexpected native execution".into())}
 let after=state(&mut ext);let mut protected=Vec::new();
 for pallet in ["Balances","Assets","Nfts","FounderCustody","FreshGenesis","Vesting","IssuanceCap","SecurityBudget","RewardReserve","EraWorlds","AiPredictions","Session","Staking","Sudo","Grandpa"]{
 let prefix=sp_core::twox_128(pallet.as_bytes());let a:BTreeMap<_,_>=before.iter().filter(|(k,_)|k.starts_with(&prefix)).collect();let b:BTreeMap<_,_>=after.iter().filter(|(k,_)|k.starts_with(&prefix)).collect();let c:BTreeMap<_,_>=control_state.iter().filter(|(k,_)|k.starts_with(&prefix)).collect();if b!=c{return Err(format!("candidate changes protected {pallet} relative to old-runtime control"))}if pallet=="Staking" {let allowed=[prefix,sp_core::twox_128(b"ErasRewardPoints")].concat();for k in a.keys().chain(b.keys()){if a.get(k)!=b.get(k)&&!k.starts_with(&allowed){return Err("unexpected staking block-init mutation".into())}}}if a!=b && pallet!="Staking"{return Err(format!("protected {pallet} changed during block initialization"))}protected.push(serde_json::json!({"pallet":pallet,"keys":a.len(),"identical_to_snapshot":a==b,"identical_to_old_runtime_same_block":true}));
 }
 let accounts=[sp_core::twox_128(b"System"),sp_core::twox_128(b"Account")].concat();let a:BTreeMap<_,_>=before.iter().filter(|(k,_)|k.starts_with(&accounts)).collect();let b:BTreeMap<_,_>=after.iter().filter(|(k,_)|k.starts_with(&accounts)).collect();if a!=b{return Err("account/nonce/balance/reference changed".into())}
 let mut changed=Vec::new();for k in before.keys().chain(after.keys()).collect::<std::collections::BTreeSet<_>>(){if before.get(k)!=after.get(k){changed.push(serde_json::json!({"key":format!("0x{}",hex::encode(k)),"before_blake2":before.get(k).map(|v|hex::encode(sp_core::blake2_256(v))),"after_blake2":after.get(k).map(|v|hex::encode(sp_core::blake2_256(v)))}));}}
 Ok(serde_json::json!({"origin":"offline Wasm execution of copied public finalized state; no production mutation","genesis":format!("{genesis:?}"),"snapshot_finalized":doc["finalized"],"snapshot_height":header.number(),"complete_state_root_verified":true,"stored_genesis_verified":true,"snapshot_keys":before.len(),"account_count":a.len(),"accounts_exact":true,"protected_storage":protected,"candidate_blake2":hex::encode(sp_core::blake2_256(&code)),"changed_keys":changed,"old_runtime_control_changed_keys":before.keys().chain(control_state.keys()).collect::<std::collections::BTreeSet<_>>().into_iter().filter(|k|before.get(*k)!=control_state.get(*k)).map(|k|format!("0x{}",hex::encode(k))).collect::<Vec<_>>(),"limitation":"Candidate first-block initialization only; not signed production upgrade, activation, full block execution or proof of future epoch-boundary behavior."}))
}
