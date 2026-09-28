# Client interface contract

The review SDK consists of two dependency-free ES modules. `era-rpc-client.mjs` exposes named,
read-only JSON-RPC 2.0 operations supported by the integrated node. `era-v14-native.mjs` encodes
and decodes a bounded set of metadata-bound native calls and opaque extrinsics without managing
keys, signing, submitting, or mutating chain state itself.

| SDK operation | RPC method(s) | Source basis | Mutates chain state |
| --- | --- | --- | --- |
| `getNodeSnapshot()` | `system_chain`, `system_name`, `system_version`, `system_health`, `chain_getHeader`, `state_getRuntimeVersion` | Standard service RPC plus the wired System RPC | No |
| `getAccountNextIndex(account)` | `system_accountNextIndex` | Wired System RPC | No |
| `getGenesisHash()` | `chain_getBlockHash(0)` | Standard chain RPC | No |
| `getMetadataHex(at?)` | `state_getMetadata` | Standard state RPC | No |
| `getStorage(key, at?)` | `state_getStorage` | One caller-supplied key; standard state RPC | No |
| `getBlock(at?)` | `chain_getBlock` | One configured block, bounded by runtime block limits | No |
| `queryFeeInfo(extrinsicHex, at?)` | `payment_queryInfo` | Wired Transaction Payment RPC | No |

The SDK validates the endpoint protocol, JSON-RPC envelope, requested input shape, and response
shape needed by the sample. By default it accepts only loopback HTTP(S) endpoints. A non-loopback
endpoint requires the caller to set `allowRemote: true`, and remote plaintext HTTP is rejected.
This override is an API safety gate, not approval to contact any particular service.

The RPC module has no generic raw-request export. Neither module signs, manages keys, writes storage,
submits extrinsics, subscribes, runs contracts/EVM, or accesses proprietary engines. Fee queries
inspect encoded extrinsic bytes; they do not submit them.

## V14 native transaction codec

The codec is fixed to runtime baseline commit `cc37097ca93211a4f9c042569f610ad735d0d30a`,
metadata SHA-256 `320b4b7284808a55fa4b480e83fd920ef4a5a7b6b13f88ef4770f0cf008068d4`,
specification 14, transaction version 1 and extrinsic version 4. It covers selected reachable calls
at Balances index 2, Assets 16, Nfts 17 and EraWorlds 18. The exact call list, authorization,
deposit/weight boundaries, and dormant AMM/API status are in
[`DAPP_ERC20_SCOPE_RECONCILIATION.md`](../../../../DAPP_ERC20_SCOPE_RECONCILIATION.md).

All six configured EraWorlds calls now have benchmark-generated production weights. The codec
exposes the three reachable owner calls; calls 3–5 remain fail-closed by their runtime origins.
Owner-adopted Option C defers true ERC-20 to V15 and does not turn these FRAME SCALE interfaces into
Ethereum RPC, wallet or ERC-20 compatibility.

Native ETKN uses Balances and pays fees. A registered fungible uses an Assets `u32` ID and separate
supply; its mint/burn never changes ETKN issuance. The similarly shaped asset approval and delegated
transfer calls are FRAME SCALE interfaces, not ERC-20.

The codec supports immortal and caller-selected mortal signing payloads. Mortal payloads bind the
checkpoint block hash and quantized era; immortal payloads bind the genesis hash twice. It reports
when the encoded payload exceeds 256 bytes so an external Substrate signer can apply the standard
Blake2-256 rule. The companion injected-wallet adapter requests a signature through the standard
external signer contract. Key access, signature production, submission and status tracking remain
outside this package.

## Example

```js
import { EraReadOnlyClient } from "./sdk/era-rpc-client.mjs";

const client = new EraReadOnlyClient("http://127.0.0.1:9944");
const snapshot = await client.getNodeSnapshot();

if (snapshot.runtime.specVersion !== 14) {
  throw new Error("review node is not the expected V14 candidate");
}
```

A transaction-construction flow remains offline:

```js
import {
  assertEraV14Metadata,
  assertEraV14RuntimeVersion,
  buildEraV14SigningPayload,
  encodeEraV14Call,
} from "./sdk/era-v14-native.mjs";

assertEraV14RuntimeVersion(snapshot.runtime);
await assertEraV14Metadata(await client.getMetadataHex());
const genesisHash = await client.getGenesisHash();
const callHex = encodeEraV14Call("Assets", "transfer", {
  id: 7,
  target: "0x" + "22".repeat(32),
  amount: 1_000,
});
const signingRequest = buildEraV14SigningPayload({
  callHex,
  nonce: await client.getAccountNextIndex("review-account"),
  genesisHash,
});
```

The synthetic account value is an encoding example, not a usable address. A reviewer must still pin
the final integrated source commit/tree, runtime Wasm and genesis identity. This source gate supplies
only the baseline metadata binding and does not authorize publication or submission.

## Offline-tested injected-wallet adapter

`era-v14-injected-wallet.mjs` uses the established injected Substrate interface:
`injectedWeb3[source].enable(applicationName)`, `accounts.get()` and
`signer.signPayload(payload)`. It never requests or stores a seed phrase or private key, and it
contains no custom cryptography or submission method. The caller supplies the wallet-visible
address and its independently verified AccountId32 when those representations differ.

Before requesting a signature, the adapter verifies the captured V14 metadata hash, specification
14, transaction version 1, caller-pinned genesis, and the exact signed-extension order. It supports
caller-selected immortal or mortal eras. Mortal payloads require the caller's current block number
and checkpoint block hash; nonce and tip are explicit. The returned opaque extrinsic is decoded by
local tests before it is exposed to the caller.

Metadata-derived fixtures cover both captured spec-13 and spec-14 metadata. They prove stable pallet
indices 0–19, compatible established calls, unchanged signed extensions and runtime API version
identities, rejection of V14-only calls under spec-13 metadata, and the required metadata refresh
for the single 13-to-14 transition. The adapter rejects dormant runtime APIs, AMM 22 and every EVM,
ERC-20, Frontier, Contracts or PSP22 path.

`sample-dapp/offline-wallet-example.mjs` demonstrates the adapter only with an injected provider
and signer supplied by the caller. Its automated browser-facing example uses a synthetic mocked
provider, account and signature. No live wallet, extension, chain submission or public endpoint was
tested or authorized by V14-7.
