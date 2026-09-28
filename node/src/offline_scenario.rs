//! Bounded signed Wasm scenario using only fixed public development identities and synthetic genesis.
use era_runtime::{self as rt,Header,RuntimeCall,UncheckedExtrinsic};
use parity_scale_codec::{Encode,Decode,Compact};
use sp_core::{Pair,H256,traits::ReadRuntimeVersionExt};
use sp_runtime::{BuildStorage,Digest,DigestItem,StateVersion,traits::{Header as _,Hash as _,BlakeTwo256}};
use sp_consensus_babe::digests::{CompatibleDigestItem,PreDigest,SecondaryPlainPreDigest};
use sc_executor::WasmExecutor;
use std::{path::Path,collections::BTreeMap};
use crate::fresh_check::{call,decode};
pub fn run(input:&Path,output:&Path)->Result<serde_json::Value,String>{
 if output.exists(){return Err("output must not exist; preserve prior evidence".into())}let d:serde_json::Value=serde_json::from_slice(&std::fs::read(input).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
 let blocks=d["blocks"].as_u64().ok_or("blocks")?;let start=d["start_unix_seconds"].as_u64().ok_or("start")?;let actions=d["actions"].as_array().ok_or("actions")?;
 if blocks==0||blocks>16000||start%6!=0||actions.len()>64{return Err("scenario bound".into())}for a in actions{if !(1..=blocks).contains(&a["block"].as_u64().ok_or("action block")?){return Err("action outside run".into())}}
 let times=d.get("block_unix_seconds").map(|v|v.as_array().ok_or("block times")).transpose()?;
 if let Some(ts)=times{if ts.len()!=blocks as usize{return Err("block times count".into())}let mut prev=None;for v in ts{let t=v.as_u64().ok_or("block time")?;if t%6!=0||t<start||t>start+172800||prev.map_or(false,|p|t<=p||t-p>600){return Err("bounded monotonic block time".into())}prev=Some(t);}}
 let inputs=rt::fresh_genesis_inputs::Inputs::synthetic();let code=rt::WASM_BINARY.ok_or("Wasm missing")?;let storage=inputs.genesis()?.build_storage()?;let mut ext=sp_state_machine::TestExternalities::<BlakeTwo256>::new_with_code(code,storage);ext.register_extension(ReadRuntimeVersionExt::new(WasmExecutor::<sp_io::SubstrateHostFunctions>::builder().build()));let executor=WasmExecutor::<sp_io::SubstrateHostFunctions>::builder().build();
 let root=ext.execute_with(||H256::from_slice(&sp_io::storage::root(StateVersion::V1)));let genesis=Header::new(0,BlakeTwo256::ordered_trie_root(vec![],StateVersion::V0),root,H256::zero(),Digest::default()).hash();let mut parent=genesis;let mut receipts=Vec::new();let mut last=serde_json::Value::Null;
 std::fs::create_dir(output).map_err(|e|e.to_string())?;
 for n in 1..=blocks {
  let slot=times.map(|ts|ts[(n-1) as usize].as_u64().unwrap()/6).unwrap_or(start/6+n-1);let h=Header::new(n as u32,H256::zero(),H256::zero(),parent,Digest{logs:vec![DigestItem::babe_pre_digest(PreDigest::SecondaryPlain(SecondaryPlainPreDigest{authority_index:((n-1)%4) as u32,slot:slot.into()}))]});call(&executor,&mut ext,code,"Core_initialize_block",&h.encode())?;
  let timestamp=UncheckedExtrinsic::new_bare(RuntimeCall::Timestamp(pallet_timestamp::Call::set{now:slot*6000}));let r:sp_runtime::ApplyExtrinsicResult=decode(call(&executor,&mut ext,code,"BlockBuilder_apply_extrinsic",&timestamp.encode())?)?;r.map_err(|e|format!("timestamp validity {n}: {e:?}"))?.map_err(|e|format!("timestamp dispatch {n}: {e:?}"))?;
  let mut touched=false;
  for action in actions.iter().filter(|a|a["block"].as_u64()==Some(n)) {
   let signer=match action["signer"].as_str(){Some("community")=>"//RelaunchCommunity",Some("sudo")=>"//RelaunchSudo",_=>return Err("only fixed public synthetic signers permitted".into())};let pair=sp_core::sr25519::Pair::from_string(signer,None).map_err(|e|format!("fixture: {e:?}"))?;let account:rt::AccountId=pair.public().into();let nonce=ext.execute_with(||rt::System::account_nonce(&account));let extra=rt::SignedExtra::decode(&mut &([0u8].to_vec().into_iter().chain(Compact(nonce).encode()).chain([0]).collect::<Vec<_>>())[..]).map_err(|e|e.to_string())?;
   let bytes=hex::decode(action["call"].as_str().ok_or("call")?.strip_prefix("0x").ok_or("call prefix")?).map_err(|e|e.to_string())?;let mut raw=&bytes[..];let c=RuntimeCall::decode(&mut raw).map_err(|e|e.to_string())?;if !raw.is_empty(){return Err("trailing call bytes".into())}
   let payload=sp_runtime::generic::SignedPayload::from_raw(c.clone(),extra.clone(),((),rt::VERSION.spec_version,rt::VERSION.transaction_version,genesis,genesis,(),(),()));let signature=payload.using_encoded(|b|pair.sign(b));let tx=UncheckedExtrinsic::new_signed(c,sp_runtime::MultiAddress::Id(account),signature.into(),extra);
   let result:sp_runtime::ApplyExtrinsicResult=decode(call(&executor,&mut ext,code,"BlockBuilder_apply_extrinsic",&tx.encode())?)?;let dispatch=result.map_err(|e|format!("{} validity: {e:?}",action["label"]))?;let expected=action["success"].as_bool().unwrap_or(true);if dispatch.is_ok()!=expected{return Err(format!("{} unexpected dispatch: {dispatch:?}",action["label"]))}
   let events=ext.execute_with(||format!("{:?}",rt::System::events()));receipts.push(serde_json::json!({"label":action["label"],"block":n,"unix_seconds":slot*6,"signer":action["signer"],"nonce":nonce,"extrinsic":format!("0x{}",hex::encode(tx.encode())),"dispatch":format!("{dispatch:?}"),"events":events}));touched=true;
  }
  let finished:Header=decode(call(&executor,&mut ext,code,"BlockBuilder_finalize_block",&[])?)?;parent=finished.hash();last=serde_json::to_value(&finished).map_err(|e|e.to_string())?;
  if touched||n==blocks{let state=ext.execute_with(||{let mut m=BTreeMap::new();let mut last=Vec::new();while let Some(k)=sp_io::storage::next_key(&last){m.insert(format!("0x{}",hex::encode(&k)),format!("0x{}",hex::encode(sp_io::storage::get(&k).unwrap())));last=k;}m});let path=output.join(format!("state-{n}.json"));std::fs::write(path,serde_json::to_vec(&serde_json::json!({"genesis":format!("{genesis:?}"),"block":format!("{parent:?}"),"header":finished,"state":state,"limitation":"Offline synthetic execution; no GRANDPA finality/network and no real weather claims"})).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;}
  if n%600==0{eprintln!("offline signed Wasm scenario block {n}/{blocks}");}
 }
 let out=serde_json::json!({"genesis":format!("{genesis:?}"),"blocks":blocks,"last_header":last,"receipts":receipts,"synthetic_only":true,"runtime_wasm_blake2":hex::encode(sp_core::blake2_256(code)),"limitation":"Signed Wasm block execution with synthetic BABE headers; not independent consensus finality or production commissioning."});std::fs::write(output.join("PASS.json"),serde_json::to_vec_pretty(&out).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;Ok(out)
}
