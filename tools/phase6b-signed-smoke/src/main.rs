use std::{error::Error, fs::{self, OpenOptions}, io::Write, path::Path};
use subxt::{dynamic::{storage, tx, Value}, blocks::ExtrinsicEvents, OnlineClient, PolkadotConfig};
use subxt_signer::sr25519::{dev, Keypair};

const RPC: &str = "ws://127.0.0.1:12944";
const EVIDENCE: &str = "/tmp/era-phase6b-full-signed-smoke-evidence";
const UNIT: u128 = 1_000_000_000_000_000_000;

fn bytes(value: impl AsRef<[u8]>) -> Value { Value::from_bytes(value) }
fn variant(name: &str) -> Value { Value::unnamed_variant(name, Vec::<Value>::new()) }
fn option_some(value: Value) -> Value { Value::unnamed_variant("Some", vec![value]) }

fn runtime_call(call: &str, args: Vec<Value>) -> Value {
    Value::unnamed_variant(
        "AiPredictions",
        vec![Value::unnamed_variant(call, args)],
    )
}
fn sudo(call: &str, args: Vec<Value>) -> subxt::tx::DynamicPayload {
    tx("Sudo", "sudo", vec![runtime_call(call, args)])
}

fn append(path: &str, line: impl AsRef<str>) -> Result<(), Box<dyn Error>> {
    let mut f = OpenOptions::new().create(true).append(true).open(Path::new(EVIDENCE).join(path))?;
    writeln!(f, "{}", line.as_ref())?;
    Ok(())
}

async fn submit(
    api: &OnlineClient<PolkadotConfig>,
    signer: &Keypair,
    label: &str,
    payload: subxt::tx::DynamicPayload,
) -> Result<ExtrinsicEvents<PolkadotConfig>, Box<dyn Error>> {
    let progress = api.tx().sign_and_submit_then_watch_default(&payload, signer).await?;
    let submitted_hash = progress.extrinsic_hash();
    let events = progress.wait_for_finalized_success().await?;
    append("extrinsics.txt", format!("{label} hash={submitted_hash:?}"))?;
    let mut event_text = String::new();
    for item in events.iter() {
        let event = item?;
        event_text.push_str(&format!(
            "{}::{} {:?}\n",
            event.pallet_name(), event.variant_name(), event.field_values()?
        ));
    }
    fs::write(Path::new(EVIDENCE).join(format!("events-{label}.txt")), event_text)?;
    Ok(events)
}

async fn snapshot(
    api: &OnlineClient<PolkadotConfig>,
    label: &str,
    pallet: &str,
    item: &str,
    keys: Vec<Value>,
) -> Result<(), Box<dyn Error>> {
    let address = storage(pallet, item, keys);
    let value = api.storage().at_latest().await?.fetch(&address).await?
        .map(|v| v.to_value())
        .transpose()?;
    append("storage-snapshots.txt", format!("{label} {pallet}::{item} = {value:?}"))?;
    Ok(())
}

async fn account_snapshot(
    api: &OnlineClient<PolkadotConfig>,
    label: &str,
    account: &Keypair,
) -> Result<(), Box<dyn Error>> {
    snapshot(api, label, "System", "Account", vec![bytes(account.public_key())]).await
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(EVIDENCE)?;
    fs::write(Path::new(EVIDENCE).join("rpc-url.txt"), format!("{RPC}\n"))?;
    let api = OnlineClient::<PolkadotConfig>::from_insecure_url(RPC).await?;
    let version = api.runtime_version();
    fs::write(
        Path::new(EVIDENCE).join("runtime-version-client.txt"),
        format!("spec_version={} transaction_version={}\n", version.spec_version, version.transaction_version),
    )?;
    if version.spec_version != 9 { return Err(format!("expected specVersion 9, got {}", version.spec_version).into()); }

    let alice = dev::alice();
    let bob = dev::bob();
    account_snapshot(&api, "before-alice", &alice).await?;
    account_snapshot(&api, "before-bob", &bob).await?;

    submit(&api, &alice, "01-authorize-alice", sudo("authorize_validator", vec![bytes(alice.public_key())])).await?;
    submit(&api, &alice, "02-register-model", tx("AiPredictions", "register_model", vec![
        bytes([0x11u8; 32]), bytes(b"ipfs://era-phase6b-signed-model")
    ])).await?;
    submit(&api, &alice, "03-approve-model", sudo("approve_model", vec![Value::u128(0)])).await?;
    submit(&api, &alice, "04-delegate-bob", tx("AiPredictions", "set_model_submitter", vec![
        Value::u128(0), option_some(bytes(bob.public_key()))
    ])).await?;
    submit(&api, &bob, "05-submit-prediction", tx("AiPredictions", "submit_prediction", vec![
        Value::u128(0), variant("Finance"), bytes(b"finance.phase6b.smoke"),
        bytes([0x22u8; 32]), bytes(b"ipfs://era-phase6b-signed-prediction"),
        Value::u128(90), Value::u128(10_000)
    ])).await?;

    snapshot(&api, "after-submit", "AiPredictions", "Models", vec![Value::u128(0)]).await?;
    snapshot(&api, "after-submit", "AiPredictions", "ModelGovernanceById", vec![Value::u128(0)]).await?;
    snapshot(&api, "after-submit", "AiPredictions", "PredictionIdsByModel", vec![Value::u128(0)]).await?;
    snapshot(&api, "after-submit", "AiPredictions", "Predictions", vec![Value::u128(0)]).await?;

    submit(&api, &alice, "06-enable-tokenization", sudo("set_tokenization_config", vec![Value::bool(true), Value::bool(false)])).await?;
    submit(&api, &bob, "07-tokenize", tx("AiPredictions", "tokenize_prediction", vec![
        Value::u128(0), bytes(b"ipfs://era-phase6b-signed-token"), Value::bool(false)
    ])).await?;
    submit(&api, &alice, "08-enable-staking", sudo("set_staking_config", vec![
        Value::bool(true), Value::u128(UNIT)
    ])).await?;
    submit(&api, &alice, "09-enable-market", sudo("set_market_economics_config", vec![
        Value::bool(true), Value::bool(true)
    ])).await?;
    submit(&api, &alice, "10-disable-market-fees", sudo("set_market_fee_config", vec![
        Value::bool(false), Value::u128(0), Value::bool(true)
    ])).await?;
    submit(&api, &alice, "11-enable-settlement", sudo("set_settlement_config", vec![
        Value::bool(true), Value::u128(10), Value::u128(UNIT)
    ])).await?;

    submit(&api, &alice, "12-stake-yes-alice", tx("AiPredictions", "stake_on_prediction_outcome_side", vec![
        Value::u128(0), variant("Yes"), Value::u128(10 * UNIT)
    ])).await?;
    submit(&api, &bob, "13-stake-no-bob", tx("AiPredictions", "stake_on_prediction_outcome_side", vec![
        Value::u128(0), variant("No"), Value::u128(5 * UNIT)
    ])).await?;

    snapshot(&api, "after-stakes", "AiPredictions", "PredictionTokenSideTotals", vec![Value::u128(0), variant("Yes")]).await?;
    snapshot(&api, "after-stakes", "AiPredictions", "PredictionTokenSideTotals", vec![Value::u128(0), variant("No")]).await?;
    snapshot(&api, "after-stakes-alice", "AiPredictions", "PredictionTokenSideStakes", vec![Value::u128(0), bytes(alice.public_key())]).await?;
    snapshot(&api, "after-stakes-bob", "AiPredictions", "PredictionTokenSideStakes", vec![Value::u128(0), bytes(bob.public_key())]).await?;

    submit(&api, &bob, "14-close-prediction", tx("AiPredictions", "close_prediction", vec![Value::u128(0)])).await?;
    submit(&api, &alice, "15-propose-correct", sudo("propose_prediction_outcome", vec![
        Value::u128(0), variant("Correct"), bytes(b"ipfs://era-phase6b-proposal")
    ])).await?;
    submit(&api, &alice, "16-finalize-correct", sudo("admin_finalize_prediction_outcome", vec![
        Value::u128(0), variant("Correct"), bytes(b"ipfs://era-phase6b-final")
    ])).await?;
    submit(&api, &alice, "17-claim-alice", tx("AiPredictions", "claim_prediction_market_payout", vec![Value::u128(0)])).await?;

    let duplicate = api.tx().sign_and_submit_then_watch_default(
        &tx("AiPredictions", "claim_prediction_market_payout", vec![Value::u128(0)]), &alice
    ).await;
    let duplicate_text = match duplicate {
        Err(e) => format!("submission rejected as expected: {e:?}"),
        Ok(progress) => match progress.wait_for_finalized_success().await {
            Err(e) => format!("finalization rejected as expected: {e:?}"),
            Ok(_) => return Err("duplicate claim unexpectedly succeeded".into()),
        }
    };
    fs::write(Path::new(EVIDENCE).join("duplicate-claim-failure.txt"), duplicate_text)?;

    snapshot(&api, "final", "AiPredictions", "PredictionTokenSettlements", vec![Value::u128(0)]).await?;
    snapshot(&api, "final", "AiPredictions", "PredictionTokenSidePayoutClaims", vec![Value::u128(0), bytes(alice.public_key())]).await?;
    snapshot(&api, "final", "AiPredictions", "PredictionTokenMarketAccounting", vec![Value::u128(0)]).await?;
    account_snapshot(&api, "after-alice", &alice).await?;
    account_snapshot(&api, "after-bob", &bob).await?;
    fs::write(Path::new(EVIDENCE).join("smoke-result.txt"), "PASS: complete signed local lifecycle, including duplicate-claim rejection\n")?;
    println!("PASS: evidence archived under {EVIDENCE}");
    Ok(())
}
