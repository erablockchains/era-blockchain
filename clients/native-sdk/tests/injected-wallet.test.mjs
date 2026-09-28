import {encodeEraAccount} from '../sdk/era-account.mjs';
import assert from "node:assert/strict";
import test from "node:test";

import { readFile } from "node:fs/promises";
import {
  ERA_V14_NATIVE_BINDINGS,
  assertEraV14ChainContext,
  buildEraV14SigningPayload,
  decodeEraV14Event,
  decodeEraV14ModuleError,
  decodeEraV14Extrinsic,
  encodeEraMortality,
  encodeEraV14Call,
} from "../sdk/era-v14-native.mjs";
import {
  EraV14InjectedWalletError,
  selectEraV14InjectedProvider,
  signEraV14CallWithInjectedProvider,
} from "../sdk/era-v14-injected-wallet.mjs";
import { createOfflineWalletExample } from "../sample-dapp/offline-wallet-example.mjs";

const ACCOUNT_11 = `0x${"11".repeat(32)}`;
const ACCOUNT_22 = `0x${"22".repeat(32)}`;
const GENESIS_AA = `0x${"aa".repeat(32)}`;
const BLOCK_BB = `0x${"bb".repeat(32)}`;
const SIGNATURE_44_TYPED = `0x01${"44".repeat(64)}`;
const fixture = JSON.parse(await readFile(new URL("../fixtures/era-spec14-metadata-bindings.json", import.meta.url)));
// Mocked adapter contract tests; real deployed bytes are covered separately.
const metadataHexForHash = ERA_V14_NATIVE_BINDINGS.metadataSha256;

function mockCryptoForPinnedMetadata() {
  return {
    subtle: {
      async digest() {
        return Uint8Array.from(metadataHexForHash.match(/../g), (byte) => Number.parseInt(byte, 16));
      },
    },
  };
}

function installCrypto(value) {
  const descriptor = Object.getOwnPropertyDescriptor(globalThis, "crypto");
  Object.defineProperty(globalThis, "crypto", { configurable: true, value });
  return () => {
    if (descriptor) Object.defineProperty(globalThis, "crypto", descriptor);
    else delete globalThis.crypto;
  };
}

function mockRegistry(calls, walletAddress = ACCOUNT_11) {
  return {
    "mock-era-wallet": {
      async enable(applicationName) {
        calls.applicationName = applicationName;
        return {
          accounts: { async get() { return [{ address: walletAddress, name: "Synthetic Alice" }]; } },
          signer: {
            async signPayload(payload) {
              calls.payload = payload;
              return { id: 7, signature: SIGNATURE_44_TYPED };
            },
          },
        };
      },
    },
  };
}

test("mortal era, signing payload, and extrinsic bytes are fixed golden vectors", () => {
  const era = encodeEraMortality({ period: 64, current: 42 });
  assert.deepEqual(era, { type: "Mortal", period: 64, phase: 42, encodedHex: "0xa502" });
  const callHex = encodeEraV14Call("Assets", "transfer", { id: 7, target: ACCOUNT_22, amount: 1_000 });
  const signing = buildEraV14SigningPayload({
    callHex,
    nonce: 7,
    tip: 11,
    genesisHash: GENESIS_AA,
    blockHash: BLOCK_BB,
    era,
  });
  assert.equal(signing.payloadHex, `${callHex}a5021c2c0e00000001000000${"aa".repeat(32)}${"bb".repeat(32)}`);
  assert.equal(signing.payloadByteLength, 117);
});

test("standard injected signer receives a fully bound payload without key custody", async () => {
  const restoreCrypto = installCrypto(mockCryptoForPinnedMetadata());
  try {
    const calls = {};
    const walletAddress = encodeEraAccount(ACCOUNT_11);
    const callHex = encodeEraV14Call("Assets", "transfer", { id: 7, target: ACCOUNT_22, amount: 1_000 });
    const result = await signEraV14CallWithInjectedProvider({
      source: "mock-era-wallet",
      applicationName: "ERA V14 offline test",
      address: walletAddress,
      accountId: ACCOUNT_11,
      callHex,
      runtimeVersion: { specVersion: 14, transactionVersion: 1 },
      metadataHex: "0x00",
      genesisHash: GENESIS_AA,
      expectedGenesisHash: GENESIS_AA,
      blockHash: BLOCK_BB,
      blockNumber: 42,
      nonce: 7,
      tip: 11,
      mortality: { period: 64 },
      registry: mockRegistry(calls, walletAddress),
    });
    assert.equal(calls.applicationName, "ERA V14 offline test");
    assert.deepEqual(calls.payload.signedExtensions, ERA_V14_NATIVE_BINDINGS.signedExtensions);
    assert.deepEqual(calls.payload, {
      address: walletAddress,
      blockHash: BLOCK_BB,
      blockNumber: "0x0000002a",
      era: "0xa502",
      genesisHash: GENESIS_AA,
      method: callHex,
      nonce: "0x00000007",
      signedExtensions: [...ERA_V14_NATIVE_BINDINGS.signedExtensions],
      specVersion: "0x0000000e",
      tip: "0x0000000000000000000000000000000b",
      transactionVersion: "0x00000001",
      version: 4,
    });
    assert.equal(result.signingPayloadHex, `${callHex}a5021c2c0e00000001000000${"aa".repeat(32)}${"bb".repeat(32)}`);
    assert.equal(result.extrinsicHex, `0x41028400${"11".repeat(32)}01${"44".repeat(64)}a5021c2c${callHex.slice(2)}`);
    assert.deepEqual(decodeEraV14Extrinsic(result.extrinsicHex).era, { type: "Mortal", encodedHex: "0xa502" });
    assert.equal(calls.payload.address, walletAddress);
    assert.equal("seed" in calls.payload, false);
    assert.equal("privateKey" in calls.payload, false);
  } finally {
    restoreCrypto();
  }
});

test("wallet adapter rejects wrong genesis, version, metadata, extensions, provider, and account", async () => {
  const context = {
    runtimeVersion: { specVersion: 14, transactionVersion: 1 },
    metadataHex: "0x00",
    genesisHash: GENESIS_AA,
    expectedGenesisHash: GENESIS_AA,
    signedExtensions: ERA_V14_NATIVE_BINDINGS.signedExtensions,
  };
  const restoreCrypto = installCrypto(mockCryptoForPinnedMetadata());
  try {
    await assert.rejects(() => assertEraV14ChainContext({ ...context, expectedGenesisHash: BLOCK_BB }), /genesis/);
    await assert.rejects(() => assertEraV14ChainContext({ ...context, runtimeVersion: { specVersion: 13, transactionVersion: 1 } }), /version/);
    await assert.rejects(() => assertEraV14ChainContext({ ...context, signedExtensions: ["CheckGenesis"] }), /signed extensions/);
    await assert.rejects(() => signEraV14CallWithInjectedProvider({
      source: "mock-era-wallet",
      applicationName: "ERA V14 offline test",
      address: ACCOUNT_22,
      accountId: ACCOUNT_22,
      callHex: encodeEraV14Call("Balances", "transferKeepAlive", { dest: ACCOUNT_11, amount: 1 }),
      runtimeVersion: context.runtimeVersion,
      metadataHex: context.metadataHex,
      genesisHash: GENESIS_AA,
      expectedGenesisHash: GENESIS_AA,
      blockHash: BLOCK_BB,
      blockNumber: 42,
      nonce: 0,
      mortality: { period: 64 },
      registry: mockRegistry({}),
    }), /not exposed/);
    Object.defineProperty(globalThis, "crypto", {
      configurable: true,
      value: { subtle: { async digest() { return new Uint8Array(32); } } },
    });
    await assert.rejects(() => assertEraV14ChainContext(context), /metadata hash/);
    assert.throws(() => selectEraV14InjectedProvider("missing", {}), EraV14InjectedWalletError);
  } finally {
    restoreCrypto();
  }
});

test("metadata-derived event and module-error decoding use fixed discriminants", () => {
  const transfer = `0x0202${"11".repeat(32)}${"22".repeat(32)}2a${"00".repeat(15)}`;
  assert.deepEqual(decodeEraV14Event(transfer), {
    palletIndex: 2,
    eventIndex: 2,
    pallet: "Balances",
    event: "Transfer",
    fields: { from: ACCOUNT_11, to: ACCOUNT_22, amount: 42 },
  });
  assert.deepEqual(decodeEraV14ModuleError({ palletIndex: 17, errorHex: "0x2d000000" }), {
    palletIndex: 17,
    errorIndex: 45,
    pallet: "Nfts",
    error: "CleanupLimitExceeded",
  });
  assert.throws(() => decodeEraV14Event("0x1616"), /unsupported/);
  assert.throws(() => decodeEraV14ModuleError({ palletIndex: 22, errorHex: "0x00000000" }), /unsupported/);
});

test("browser example executes only through a mocked injected provider", async () => {
  const restoreCrypto = installCrypto(mockCryptoForPinnedMetadata());
  try {
    const calls = {};
    const result = await createOfflineWalletExample({
      registry: mockRegistry(calls),
      metadataHex: "0x00",
      account: ACCOUNT_11,
      genesisHash: GENESIS_AA,
      blockHash: BLOCK_BB,
    });
    assert.equal(result.providerSource, "mock-era-wallet");
    assert.equal(result.signerPayload.method.slice(0, 6), "0x1200");
    assert.equal(calls.payload.address, ACCOUNT_11);
  } finally {
    restoreCrypto();
  }
});

// Regression: the encoded signing payload and extension JSON must use the same selected runtime.
test("selected development profile reaches the injected signer contract", async () => {
  const {webcrypto}=await import('node:crypto');
  const metadata=new Uint8Array([109,101,116,97,15]); // Explicit contract fixture, not deployed metadata.
  const digest=Buffer.from(await webcrypto.subtle.digest('SHA-256',metadata)).toString('hex');
  const bindings={...ERA_V14_NATIVE_BINDINGS,specVersion:15,metadataSha256:digest};
  const restore=installCrypto(webcrypto);
  try {
    const calls={};
    const result=await signEraV14CallWithInjectedProvider({bindings,source:'mock-era-wallet',applicationName:'isolated profile regression',address:ACCOUNT_11,callHex:'0x180000010700000064000000',runtimeVersion:{specVersion:15,transactionVersion:1},metadataHex:'0x'+Buffer.from(metadata).toString('hex'),genesisHash:GENESIS_AA,expectedGenesisHash:GENESIS_AA,blockHash:BLOCK_BB,blockNumber:42,nonce:0,registry:mockRegistry(calls)});
    assert.equal(calls.payload.specVersion,'0x0000000f');
    assert.equal(result.signingPayloadHex.slice(32,40),'0f000000');
    await assert.rejects(()=>signEraV14CallWithInjectedProvider({bindings,source:'mock-era-wallet',applicationName:'profile mismatch',address:ACCOUNT_11,callHex:'0x00',runtimeVersion:{specVersion:14,transactionVersion:1},metadataHex:'0x00',genesisHash:GENESIS_AA,expectedGenesisHash:GENESIS_AA,blockNumber:42,nonce:0,registry:mockRegistry({})}),/version/);
  } finally {restore();}
});

// Polkadot.js consumes byte-aligned u32/u128 hex fields, not JSON-RPC quantity hex.
test('injected signer numeric fields reject overflow before requesting a signature', async()=>{
 const restore=installCrypto(mockCryptoForPinnedMetadata());
 try {
  for(const override of [{blockNumber:2**32},{nonce:2**32},{tip:2n**128n},{blockNumber:Number.MAX_SAFE_INTEGER+1}]){
   const calls={};
   await assert.rejects(()=>signEraV14CallWithInjectedProvider({source:'mock-era-wallet',applicationName:'bounds regression',address:ACCOUNT_11,callHex:'0x0000',runtimeVersion:{specVersion:14,transactionVersion:1},metadataHex:'0x00',genesisHash:GENESIS_AA,expectedGenesisHash:GENESIS_AA,blockHash:BLOCK_BB,blockNumber:42,nonce:0,registry:mockRegistry(calls),...override}));
   assert.equal(calls.payload,undefined);
  }
 } finally {restore();}
});
