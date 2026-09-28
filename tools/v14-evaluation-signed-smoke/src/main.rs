//! Disposable-loopback signed evaluation acceptance. Public development signers only.
use std::{error::Error,fs::{self,OpenOptions},io::Write,path::PathBuf,time::{SystemTime,UNIX_EPOCH,Duration}};
use subxt::{dynamic::{tx,storage,Value},OnlineClient,PolkadotConfig,tx::DynamicPayload};
use subxt_signer::sr25519::{dev,Keypair};
use std::str::FromStr;
const RPC:&str="ws://127.0.0.1:21944";
const PRODUCTION:&str="0x0abc2c3d8db5815541050b73da4d81267ebf14d90dbee8d7258155b667ea112e";
fn bytes(v:impl AsRef<[u8]>)->Value{Value::from_bytes(v)}
fn variant(v:&str)->Value{Value::unnamed_variant(v,Vec::<Value>::new())}
fn root(call:&str,args:Vec<Value>)->DynamicPayload{tx("Sudo","sudo",vec![Value::unnamed_variant("AiPredictions",vec![Value::unnamed_variant(call,args)])])}
fn log(dir:&PathBuf,label:&str,value:impl std::fmt::Debug)->Result<(),Box<dyn Error>>{let mut f=OpenOptions::new().create(true).append(true).open(dir.join("signed-events.txt"))?;writeln!(f,"{label} {value:?}")?;Ok(())}
async fn submit(api:&OnlineClient<PolkadotConfig>,signer:&Keypair,dir:&PathBuf,label:&str,payload:DynamicPayload,expected_dispatch_error:bool,expected_sudo_error:bool)->Result<(),Box<dyn Error>> {
 let progress=api.tx().sign_and_submit_then_watch_default(&payload,signer).await?;
 let hash=progress.extrinsic_hash();log(dir,label,hash)?;
 let result=tokio::time::timeout(Duration::from_secs(60),progress.wait_for_finalized_success()).await?;
 if expected_dispatch_error {match result {Err(subxt::Error::Runtime(error))=>{log(dir,label,format!("expected runtime dispatch failure: {error:?}"))?;return Ok(());},other=>return Err(format!("{label}: expected finalized runtime dispatch error, received {other:?}").into())}}
 let events=result?;let mut sudo_error=false;
 for e in events.iter(){let e=e?;let fields=e.field_values()?;log(dir,label,format!("{}::{} {:?}",e.pallet_name(),e.variant_name(),fields))?;if e.pallet_name()=="Sudo"&&e.variant_name()=="Sudid"{sudo_error=format!("{fields:?}").contains("name: \"Err\"");}}
 if sudo_error!=expected_sudo_error{return Err(format!("{label}: unexpected nested Sudo result").into());}Ok(())
}
async fn snapshot(api:&OnlineClient<PolkadotConfig>,dir:&PathBuf,pallet:&str,item:&str,keys:Vec<Value>)->Result<(),Box<dyn Error>>{let value=api.storage().at_latest().await?.fetch(&storage(pallet,item,keys)).await?.map(|v|v.to_value()).transpose()?;log(dir,&format!("{pallet}::{item}"),value)}
#[tokio::main]
async fn main()->Result<(),Box<dyn Error>> {
 let args:Vec<_>=std::env::args().collect();if args.len()<3{return Err("usage: era-v14-evaluation-signed-smoke EXPECTED_DISPOSABLE_GENESIS EVIDENCE_DIRECTORY [--prepare-upgrade | --upgrade WASM | --amm | --active-ai]".into());}
 if args[1]==PRODUCTION{return Err("production genesis refused".into());}
 let dir=PathBuf::from(&args[2]);fs::create_dir_all(&dir)?;
 let api=OnlineClient::<PolkadotConfig>::from_insecure_url(RPC).await?;
 if format!("{:?}",api.genesis_hash())!=args[1]{return Err("disposable genesis/spec mismatch".into());}
 log(&dir,"context",(RPC,api.genesis_hash(),api.runtime_version()))?;
 let alice=dev::alice();let bob=dev::bob();let reviewer=dev::charlie();

 let mode=args.get(3).map(String::as_str).unwrap_or("--ai");
 let fixture_sudo=Keypair::from_uri(&subxt_signer::SecretUri::from_str("//RelaunchSudo")?)?;
 if mode=="--prepare-upgrade" {
  if api.runtime_version().spec_version!=14 {return Err("prepare requires retained spec14".into());}
  let community=Keypair::from_uri(&subxt_signer::SecretUri::from_str("//RelaunchCommunity")?)?;
  for signer in [&alice,&bob,&reviewer] {submit(&api,&community,&dir,"synthetic-community-funding",tx("Balances","transfer_keep_alive",vec![Value::unnamed_variant("Id",vec![bytes(signer.public_key())]),Value::u128(1000_000_000_000_000_000_000)]),false,false).await?;}
  for id in [7u128,8] {
   submit(&api,&alice,&dir,"create-synthetic-asset",tx("Assets","create",vec![Value::u128(id),Value::unnamed_variant("Id",vec![bytes(alice.public_key())]),Value::u128(1)]),false,false).await?;
   for signer in [&alice,&bob] {submit(&api,&alice,&dir,"mint-synthetic-asset",tx("Assets","mint",vec![Value::u128(id),Value::unnamed_variant("Id",vec![bytes(signer.public_key())]),Value::u128(10000_000_000_000_000_000_000)]),false,false).await?;}
  }
  println!("PASS: signed pre-upgrade fixtures on synthetic spec14");return Ok(());
 }
 if mode=="--upgrade" {
  if args.len()!=5||api.runtime_version().spec_version!=14 {return Err("upgrade requires spec14 and explicit Wasm path".into());}
  let wasm=fs::read(&args[4])?;
  submit(&api,&fixture_sudo,&dir,"upgrade-code",tx("Sudo","sudo",vec![Value::unnamed_variant("System",vec![Value::unnamed_variant("set_code",vec![bytes(wasm)])])]),false,false).await?;
  println!("PASS: finalized signed isolated upgrade; verify subsequent runtime/state independently");return Ok(());
 }
 if api.runtime_version().spec_version!=15 {return Err("workflow requires spec15".into());}
 if mode=="--amm" || mode=="--amm-resume" {
  let native=||variant("Native");let registered=|id:u128|Value::unnamed_variant("Registered",vec![Value::u128(id)]);
  if mode=="--amm" {
  for id in [7u128,8] {submit(&api,&fixture_sudo,&dir,"approve-synthetic-pair",tx("Sudo","sudo",vec![Value::unnamed_variant("Amm",vec![Value::unnamed_variant("approve_pair",vec![native(),registered(id)])])]),false,false).await?;}
  submit(&api,&fixture_sudo,&dir,"activate-isolated-amm",tx("Sudo","sudo",vec![Value::unnamed_variant("Amm",vec![Value::unnamed_variant("activate",vec![])])]),false,false).await?;
  } else {
   let state=api.storage().at_latest().await?;let enabled=state.fetch(&storage("Amm","Enabled",vec![])).await?.ok_or("missing commissioning state")?.to_value()?;if enabled.as_bool()!=Some(true){return Err("AMM resume requires existing isolated activation".into());}
  }
  let unit=1_000_000_000_000_000_000u128;let deadline=u128::from(api.blocks().at_latest().await?.number())+1000;
  submit(&api,&alice,&dir,"create-pool",tx("Amm","create_pool",vec![native(),registered(7),Value::u128(deadline)]),false,false).await?;
  for signer in [&alice,&bob] {submit(&api,signer,&dir,"provide-liquidity",tx("Amm","add_liquidity",vec![native(),registered(7),Value::u128(100*unit),Value::u128(100*unit),Value::u128(99*unit),Value::u128(99*unit),Value::u128(deadline)]),false,false).await?;}
  submit(&api,&bob,&dir,"swap-exact-input",tx("Amm","swap_exact_input",vec![native(),registered(7),Value::u128(unit),Value::u128(unit/2),bytes(bob.public_key()),Value::u128(deadline)]),false,false).await?;
  submit(&api,&bob,&dir,"swap-exact-output",tx("Amm","swap_exact_output",vec![registered(7),native(),Value::u128(unit/2),Value::u128(unit),bytes(bob.public_key()),Value::u128(deadline)]),false,false).await?;
  submit(&api,&alice,&dir,"remove-liquidity",tx("Amm","remove_liquidity",vec![native(),registered(7),Value::u128(10*unit),Value::u128(9*unit),Value::u128(9*unit),bytes(alice.public_key()),Value::u128(deadline)]),false,false).await?;
  submit(&api,&bob,&dir,"refuse-slippage",tx("Amm","swap_exact_input",vec![native(),registered(7),Value::u128(unit),Value::u128(100*unit),bytes(bob.public_key()),Value::u128(deadline)]),true,false).await?;
  submit(&api,&bob,&dir,"refuse-expired",tx("Amm","create_pool",vec![native(),registered(8),Value::u128(1)]),true,false).await?;
  for item in ["Enabled","PairCount"]{snapshot(&api,&dir,"Amm",item,vec![]).await?;}
  snapshot(&api,&dir,"EraV14Amm","PoolCount",vec![]).await?;
  fs::write(dir.join("PASS.txt"),"PASS signed AMM lifecycle on isolated chain; synthetic assets/funding do not approve production commissioning.\n")?;println!("PASS signed AMM lifecycle");return Ok(());
 }

 if mode=="--validators-target7" {
  let state=api.storage().at_latest().await?;let count=state.fetch(&storage("Staking","ValidatorCount",vec![])).await?.ok_or("validator count absent")?.to_value()?.as_u128().ok_or("validator count type")?;if !(4..=7).contains(&count){return Err("requires synthetic four-to-seven bound".into())}
  for target in count+1..=7 {submit(&api,&fixture_sudo,&dir,"one-at-a-time-validator-target",tx("Sudo","sudo",vec![Value::unnamed_variant("Staking",vec![Value::unnamed_variant("set_validator_count",vec![Value::u128(target)])])]),false,false).await?;}
  submit(&api,&fixture_sudo,&dir,"synthetic-next-era-election",tx("Sudo","sudo",vec![Value::unnamed_variant("Staking",vec![Value::unnamed_variant("force_new_era",Vec::<Value>::new())])]),false,false).await?;println!("PASS synthetic target changes; verify actual session activation");return Ok(());
 }
 if mode=="--validators-bootstrap7" {
  let file=fs::read_to_string(args.get(4).ok_or("public synthetic session-key file required")?)?;let rows:Vec<_>=file.lines().collect();if rows.len()!=3{return Err("exactly three synthetic candidates".into());}
  let community=Keypair::from_uri(&subxt_signer::SecretUri::from_str("//RelaunchCommunity")?)?;
  for (row,expected) in rows.iter().zip(["Eve","Ferdie","V14Seventh"]) {let f:Vec<_>=row.split_whitespace().collect();if f.len()!=3||f[0]!=expected{return Err("fixed synthetic candidate identities only".into());}
   let signer=Keypair::from_uri(&subxt_signer::SecretUri::from_str(&format!("//{expected}"))?)?;
   let parse=|s:&str|->Result<Vec<u8>,Box<dyn Error>>{let s=s.strip_prefix("0x").ok_or("public key hex prefix")?;if s.len()!=64{return Err("public key size".into())}Ok((0..64).step_by(2).map(|i|u8::from_str_radix(&s[i..i+2],16)).collect::<Result<Vec<_>,_>>()?)};
   let babe=parse(f[1])?;let grandpa=parse(f[2])?;if babe!=signer.public_key().0.to_vec(){return Err("synthetic account/key mismatch".into())}
   submit(&api,&community,&dir,"synthetic-onboarding-funding",tx("Balances","transfer_keep_alive",vec![Value::unnamed_variant("Id",vec![bytes(signer.public_key())]),Value::u128(11000_000_000_000_000_000_000)]),false,false).await?;
   submit(&api,&signer,&dir,"candidate-minimum-bond",tx("Staking","bond",vec![Value::u128(10000_000_000_000_000_000_000),variant("Stash")]),false,false).await?;
   submit(&api,&signer,&dir,"candidate-session-key-ownership",tx("Session","set_keys",vec![Value::named_composite(vec![("babe",bytes(babe)),("grandpa",bytes(grandpa))]),bytes([])]),false,false).await?;
   submit(&api,&signer,&dir,"candidate-validate",tx("Staking","validate",vec![Value::named_composite(vec![("commission",Value::u128(200_000_000)),("blocked",Value::bool(false))])]),false,false).await?;
  }
  for (name,values) in [("set_validator_count",vec![Value::u128(5)]),("set_validator_count",vec![Value::u128(6)]),("set_validator_count",vec![Value::u128(7)]),("force_new_era",vec![])] {submit(&api,&fixture_sudo,&dir,name,tx("Sudo","sudo",vec![Value::unnamed_variant("Staking",vec![Value::unnamed_variant(name,values)])]),false,false).await?;}
  println!("PASS signed synthetic candidate admission; actual session election/participation still requires verification");return Ok(());
 }
 if mode=="--allocator-browser-permit" {
  for name in ["permit_asset","permit_collection"] {let mut args=vec![bytes(alice.public_key())];if name=="permit_asset"{args.push(Value::u128(1));}submit(&api,&fixture_sudo,&dir,name,tx("Sudo","sudo",vec![Value::unnamed_variant("EraV14AssetAllocator",vec![Value::unnamed_variant(name,args)])]),false,false).await?;}println!("PASS public synthetic Alice next-object permits only");return Ok(());
 }
 if mode=="--allocator-setup" {
  if api.storage().at_latest().await?.fetch(&storage("EraV14AssetAllocator","SchemaVersion",vec![])).await?.is_some(){return Err("allocator already initialized; do not repeat".into());}
  let sudo=|name:&str,args:Vec<Value>|tx("Sudo","sudo",vec![Value::unnamed_variant("EraV14AssetAllocator",vec![Value::unnamed_variant(name,args)])]);
  let some=|id:u128|Value::unnamed_variant("Some",vec![Value::u128(id)]);
  // Exact retained synthetic high-water: asset7/8, no existing NFT collections.
  submit(&api,&fixture_sudo,&dir,"initialize-synthetic-cursors",sudo("initialize",vec![some(9),some(1),bytes([1u8;32])]),false,false).await?;
  for id in [7u128,8] {submit(&api,&fixture_sudo,&dir,"admit-retained-synthetic-asset",sudo("admit_existing_asset",vec![Value::u128(id),Value::u128(1)]),false,false).await?;}
  submit(&api,&fixture_sudo,&dir,"permit-synthetic-asset-creator",sudo("permit_asset",vec![bytes(alice.public_key()),Value::u128(1)]),false,false).await?;
  submit(&api,&fixture_sudo,&dir,"permit-synthetic-collection-creator",sudo("permit_collection",vec![bytes(alice.public_key())]),false,false).await?;
  println!("PASS: isolated allocator prerequisites only; no production IDs or permissions selected");return Ok(());
 }
 if mode=="--penalty-policy" {
  let state=api.storage().at_latest().await?;
  if state.fetch(&storage("EquivocationPenalties","Policy",vec![])).await?.is_some(){return Err("isolated policy already configured; do not repeat".into());}
  let session=state.fetch(&storage("Session","CurrentIndex",vec![])).await?.ok_or("session missing")?.to_value()?.as_u128().ok_or("session is not an integer")?;
  let future=session+2;
  submit(&api,&alice,&dir,"reject-unprivileged-policy",tx("EquivocationPenalties","prepare_policy",vec![Value::u128(10_000_000),Value::u128(future)]),true,false).await?;
  submit(&api,&fixture_sudo,&dir,"prepare-synthetic-one-percent-policy",tx("Sudo","sudo",vec![Value::unnamed_variant("EquivocationPenalties",vec![Value::unnamed_variant("prepare_policy",vec![Value::u128(10_000_000),Value::u128(future)])])]),false,false).await?;
  snapshot(&api,&dir,"EquivocationPenalties","Policy",vec![]).await?;
  println!("PASS: measured policy call finalized in Wasm; one-percent synthetic fixture, not owner policy or production activation");return Ok(());
 }
 if mode=="--wallet-setup" {
  for signer in [&alice,&reviewer] {submit(&api,&fixture_sudo,&dir,"authorize-wallet-test-model-operator-reviewer",root("authorize_validator",vec![bytes(signer.public_key())]),false,false).await?;}
  let registered=|id|Value::unnamed_variant("Registered",vec![Value::u128(id)]);
  for (a,b) in [(variant("Native"),registered(7)),(registered(7),registered(8))] {submit(&api,&fixture_sudo,&dir,"approve-wallet-test-pair",tx("Sudo","sudo",vec![Value::unnamed_variant("Amm",vec![Value::unnamed_variant("approve_pair",vec![a,b])])]),false,false).await?;}
  submit(&api,&fixture_sudo,&dir,"activate-wallet-test-amm",tx("Sudo","sudo",vec![Value::unnamed_variant("Amm",vec![Value::unnamed_variant("activate",vec![])])]),false,false).await?;
  println!("PASS: isolated wallet prerequisites; public development accounts and synthetic assets only");return Ok(());
 }
 if mode=="--sign-payload" {
  // Loopback/genesis/spec checks above still apply. Only public development identities.
  use subxt::config::{Hasher,substrate::BlakeTwo256};
  let encoded=fs::read_to_string(args.get(4).ok_or("payload file required")?)?;
  let encoded=encoded.trim().strip_prefix("0x").ok_or("hex payload required")?;
  if encoded.len()>32768 || encoded.len()%2!=0 {return Err("bounded even payload required".into());}
  let raw=(0..encoded.len()).step_by(2).map(|i|u8::from_str_radix(&encoded[i..i+2],16)).collect::<Result<Vec<_>,_>>()?;
  let signer=match args.get(5).map(String::as_str).unwrap_or("charlie") {"alice"=>&alice,"bob"=>&bob,"charlie"=>&reviewer,_=>return Err("only public development Alice/Bob/Charlie permitted".into())};
  let hash=BlakeTwo256::hash(&raw);let message=if raw.len()>256 {hash.as_ref()} else {raw.as_slice()};
  let sig=signer.sign(message);let result="0x01".to_owned()+&sig.0.iter().map(|b|format!("{b:02x}")).collect::<String>();
  fs::write(dir.join("public-development-signature.hex"),result)?;
  println!("Signed bounded payload with public development identity; no submission");return Ok(());
 }
 if mode=="--export-signed" {
  let payload=tx("Balances","transfer_keep_alive",vec![Value::unnamed_variant("Id",vec![bytes(dev::dave().public_key())]),Value::u128(1)]);
  let signed=api.tx().create_signed(&payload,&reviewer,Default::default()).await?;
  let encoded="0x".to_owned()+&signed.encoded().iter().map(|b|format!("{b:02x}")).collect::<String>();
  fs::write(dir.join("public-development-signed-extrinsic.hex"),encoded)?;
  println!("Prepared public development signed transaction; no submission in this mode");return Ok(());
 }
 if mode=="--nft" {
  let none=||variant("None");let some=|v|Value::unnamed_variant("Some",vec![v]);let addr=|s:&Keypair|Value::unnamed_variant("Id",vec![bytes(s.public_key())]);let unit=1_000_000_000_000_000_000u128;
  let config=Value::named_composite(vec![("settings",Value::u128(0)),("max_supply",none()),("mint_settings",Value::named_composite(vec![("mint_type",variant("Issuer")),("price",none()),("start_block",none()),("end_block",none()),("default_item_settings",Value::u128(0))]))]);
  submit(&api,&alice,&dir,"create-nft-collection",tx("Nfts","create",vec![addr(&alice),config]),false,false).await?;
  for (item,owner) in [(1u128,&alice),(2,&bob),(3,&alice)] {submit(&api,&alice,&dir,"mint-nft",tx("Nfts","mint",vec![Value::u128(0),Value::u128(item),addr(owner),none()]),false,false).await?;}
  submit(&api,&bob,&dir,"refuse-nft-nonowner-price",tx("Nfts","set_price",vec![Value::u128(0),Value::u128(1),some(Value::u128(5*unit)),none()]),true,false).await?;
  submit(&api,&alice,&dir,"set-nft-price",tx("Nfts","set_price",vec![Value::u128(0),Value::u128(1),some(Value::u128(5*unit)),some(addr(&bob))]),false,false).await?;
  submit(&api,&bob,&dir,"refuse-nft-underbid",tx("Nfts","buy_item",vec![Value::u128(0),Value::u128(1),Value::u128(4*unit)]),true,false).await?;
  submit(&api,&bob,&dir,"buy-nft",tx("Nfts","buy_item",vec![Value::u128(0),Value::u128(1),Value::u128(6*unit)]),false,false).await?;
  let price=|n|some(Value::named_composite(vec![("amount",Value::u128(n*unit)),("direction",variant("Receive"))]));
  submit(&api,&alice,&dir,"create-priced-nft-swap",tx("Nfts","create_swap",vec![Value::u128(0),Value::u128(3),Value::u128(0),some(Value::u128(2)),price(5),Value::u128(100)]),false,false).await?;
  submit(&api,&bob,&dir,"refuse-changed-swap-price",tx("Nfts","claim_swap",vec![Value::u128(0),Value::u128(2),Value::u128(0),Value::u128(3),price(4)]),true,false).await?;
  submit(&api,&bob,&dir,"claim-priced-nft-swap",tx("Nfts","claim_swap",vec![Value::u128(0),Value::u128(2),Value::u128(0),Value::u128(3),price(5)]),false,false).await?;
  submit(&api,&bob,&dir,"refuse-replayed-nft-swap",tx("Nfts","claim_swap",vec![Value::u128(0),Value::u128(2),Value::u128(0),Value::u128(3),price(5)]),true,false).await?;
  for item in [1u128,2,3]{snapshot(&api,&dir,"Nfts","Item",vec![Value::u128(0),Value::u128(item)]).await?;}
  fs::write(dir.join("PASS.txt"),"PASS signed native NFT purchase and priced atomic swap on isolated chain.\n")?;println!("PASS signed NFT market lifecycle");return Ok(());
 }
 let sudo=if mode=="--active-ai" {&fixture_sudo} else {&alice};

 submit(&api,&alice,&dir,"fund-disposable-reviewer",tx("Balances","transfer_keep_alive",vec![Value::unnamed_variant("Id",vec![bytes(reviewer.public_key())]),Value::u128(100_000_000_000_000_000_000)]),false,false).await?;
 submit(&api,sudo,&dir,"authorize-owner",root("authorize_validator",vec![bytes(alice.public_key())]),false,false).await?;
 submit(&api,sudo,&dir,"authorize-reviewer",root("authorize_validator",vec![bytes(reviewer.public_key())]),false,false).await?;
 submit(&api,&alice,&dir,"register",tx("AiPredictions","register_model",vec![bytes([7u8;32]),bytes(b"fixture://synthetic-model-v1")]),false,false).await?;
 submit(&api,&reviewer,&dir,"approve",tx("AiPredictions","approve_model",vec![Value::u128(0)]),false,false).await?;
 submit(&api,&alice,&dir,"delegate",tx("AiPredictions","set_model_submitter",vec![Value::u128(0),Value::unnamed_variant("Some",vec![bytes(bob.public_key())])]),false,false).await?;
 let now=SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();let cutoff=now+24;let evidence_after=now+30;
 let request=||tx("AiPredictions","submit_evaluation",vec![Value::u128(0),bytes([1u8;32]),bytes([7u8;32]),bytes([8u8;32]),bytes(b"fixture://synthetic-envelope"),Value::u128(0),Value::u128(100000),Value::u128(cutoff.into()),Value::u128(evidence_after.into())]);
 submit(&api,&bob,&dir,"submit",request(),false,false).await?;
 submit(&api,&bob,&dir,"idempotent-retry",request(),false,false).await?;
 submit(&api,sudo,&dir,"refuse-financial-root-enable",root("set_tokenization_config",vec![Value::bool(true),Value::bool(true)]),false,true).await?;
 submit(&api,&bob,&dir,"refuse-token",tx("AiPredictions","tokenize_prediction",vec![Value::u128(0),bytes(b"fixture://forbidden"),Value::bool(true)]),true,false).await?;
 let remaining=evidence_after.saturating_sub(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs());if remaining>0{tokio::time::sleep(Duration::from_secs(remaining+6)).await;}
 submit(&api,&reviewer,&dir,"record-failed-result",tx("AiPredictions","record_evaluation_evidence",vec![Value::u128(0),Value::u128(0),bytes([9u8;32]),variant("Failed")]),false,false).await?;
 submit(&api,&reviewer,&dir,"append-correction",tx("AiPredictions","record_evaluation_evidence",vec![Value::u128(0),Value::u128(1),bytes([10u8;32]),variant("Inconclusive")]),false,false).await?;
 for item in ["NextPredictionId","NextPredictionTokenId","NextDisputeId"]{snapshot(&api,&dir,"AiPredictions",item,vec![]).await?;}
 snapshot(&api,&dir,"AiPredictions","EvaluationRequests",vec![Value::u128(0),bytes([1u8;32])]).await?;
 snapshot(&api,&dir,"AiPredictions","EvaluationRevisionCount",vec![Value::u128(0)]).await?;
 for signer in [&alice,&bob,&reviewer]{snapshot(&api,&dir,"System","Account",vec![bytes(signer.public_key())]).await?;}
 fs::write(dir.join("PASS.txt"),"PASS: signed disposable-chain semantic lifecycle; synthetic hashes, not real model/weather evaluation. Transaction fees charged; no AI positions/tokens.\n")?;println!("PASS: signed disposable evaluation lifecycle");Ok(())
}
