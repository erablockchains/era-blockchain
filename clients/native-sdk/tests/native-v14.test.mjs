import assert from "node:assert/strict";
import test from "node:test";

import {
  ERA_V14_NATIVE_BINDINGS,
  assertEraV14Metadata,
  assertEraV14RuntimeVersion,
  buildEraV14SigningPayload,
  bytesToHex,
  decodeEraV14Call,
  decodeEraV14Extrinsic,
  encodeEraV14Call,
  encodeEraV14SignedExtrinsic,
  encodeScaleCompact,
} from "../sdk/era-v14-native.mjs";

const ACCOUNT_11 = `0x${"11".repeat(32)}`;
const ACCOUNT_22 = `0x${"22".repeat(32)}`;
const SIGNATURE_44 = `0x${"44".repeat(64)}`;
const GENESIS_AA = `0x${"aa".repeat(32)}`;

test("bindings pin the integrated metadata and exact reachable/dormant boundary", () => {
  assert.equal(ERA_V14_NATIVE_BINDINGS.specVersion, 14);
  assert.equal(ERA_V14_NATIVE_BINDINGS.transactionVersion, 1);
  assert.equal(ERA_V14_NATIVE_BINDINGS.pallets.Assets.index, 16);
  assert.equal(ERA_V14_NATIVE_BINDINGS.pallets.Nfts.index, 17);
  assert.equal(ERA_V14_NATIVE_BINDINGS.pallets.EraWorlds.index, 18);
  assert.equal(ERA_V14_NATIVE_BINDINGS.availability.amm, "DORMANT_SOURCE_ONLY");
  assert.equal(ERA_V14_NATIVE_BINDINGS.availability.erc20, "NOT_IMPLEMENTED");
  assert.equal(
    ERA_V14_NATIVE_BINDINGS.availability.eraWorlds,
    "REACHABLE",
  );
  assertEraV14RuntimeVersion({ specVersion: 14, transactionVersion: 1 });
  assert.throws(
    () => assertEraV14RuntimeVersion({ specVersion: 15, transactionVersion: 1 }),
    /does not match/,
  );
});

test("SCALE compact integer boundaries match the V14 runtime codec", () => {
  assert.equal(bytesToHex(encodeScaleCompact(0)), "0x00");
  assert.equal(bytesToHex(encodeScaleCompact(63)), "0xfc");
  assert.equal(bytesToHex(encodeScaleCompact(64)), "0x0101");
  assert.equal(bytesToHex(encodeScaleCompact(16_383)), "0xfdff");
  assert.equal(bytesToHex(encodeScaleCompact(16_384)), "0x02000100");
  assert.equal(bytesToHex(encodeScaleCompact(1n << 30n)), "0x0300000040");
});

test("native ETKN and registered-asset calls have disjoint golden SCALE prefixes", () => {
  const nativeTransfer = encodeEraV14Call("Balances", "transferKeepAlive", {
    dest: ACCOUNT_22,
    amount: 42,
  });
  const assetCreate = encodeEraV14Call("Assets", "create", {
    id: 7,
    admin: ACCOUNT_22,
    minBalance: 1,
  });
  const assetTransfer = encodeEraV14Call("Assets", "transfer", {
    id: 7,
    target: ACCOUNT_22,
    amount: 1_000,
  });
  const approval = encodeEraV14Call("Assets", "approveTransfer", {
    id: 7,
    delegate: ACCOUNT_22,
    amount: 250,
  });
  const metadata = encodeEraV14Call("Assets", "setMetadata", {
    id: 7,
    name: "Example",
    symbol: "APP",
    decimals: 6,
  });

  assert.equal(nativeTransfer, `0x020300${"22".repeat(32)}a8`);
  assert.equal(assetCreate, `0x10000700000000${"22".repeat(32)}01${"00".repeat(15)}`);
  assert.equal(assetTransfer, `0x10080700000000${"22".repeat(32)}a10f`);
  assert.equal(approval, `0x10160700000000${"22".repeat(32)}e903`);
  assert.equal(metadata, "0x1011070000001c4578616d706c650c41505006");
  assert.deepEqual(decodeEraV14Call(assetTransfer), {
    palletIndex: 16,
    callIndex: 8,
    pallet: "Assets",
    method: "transfer",
    args: { id: 7, target: ACCOUNT_22, amount: 1_000 },
  });
});

test("NFT lifecycle and World registry calls match the integrated call schema", () => {
  const nftTransfer = encodeEraV14Call("Nfts", "transfer", {
    collection: 3,
    item: 9,
    dest: ACCOUNT_22,
  });
  const nftRetirement = encodeEraV14Call("Nfts", "continueCollectionRetirement", {
    collection: 3,
    limit: 685,
  });
  const worldRegistration = encodeEraV14Call("EraWorlds", "registerWorld", {
    worldId: "world-1",
    commitment: `0x${"55".repeat(32)}`,
  });

  assert.equal(nftTransfer, `0x1106030000000900000000${"22".repeat(32)}`);
  assert.equal(nftRetirement, "0x112803000000ad020000");
  assert.equal(worldRegistration, `0x12001c776f726c642d31${"55".repeat(32)}`);
  assert.deepEqual(decodeEraV14Call(nftRetirement), {
    palletIndex: 17,
    callIndex: 40,
    pallet: "Nfts",
    method: "continueCollectionRetirement",
    args: { collection: 3, limit: 685 },
  });
  assert.throws(
    () => encodeEraV14Call("Nfts", "continueCollectionRetirement", { collection: 3, limit: 686 }),
    /cleanup ceiling/,
  );
});

test("default collection creation and no-witness mint are explicit bounded variants", () => {
  const create = encodeEraV14Call("Nfts", "createCollection", { admin: ACCOUNT_22 });
  const mint = encodeEraV14Call("Nfts", "mint", {
    collection: 3,
    item: 9,
    mintTo: ACCOUNT_22,
  });

  assert.equal(create, `0x110000${"22".repeat(32)}${"00".repeat(21)}`);
  assert.equal(mint, `0x1103030000000900000000${"22".repeat(32)}00`);
  assert.equal(decodeEraV14Call(create).args.configHex, `0x${"00".repeat(21)}`);
  assert.equal(decodeEraV14Call(mint).args.mintTo, ACCOUNT_22);
});

test("immortal signing payload and externally signed opaque extrinsic round-trip", () => {
  const callHex = encodeEraV14Call("Assets", "transfer", {
    id: 7,
    target: ACCOUNT_22,
    amount: 1_000,
  });
  const signing = buildEraV14SigningPayload({ callHex, nonce: 7, tip: 11, genesisHash: GENESIS_AA });
  assert.equal(
    signing.payloadHex,
    `${callHex}001c2c0e00000001000000${"aa".repeat(64)}`,
  );
  assert.equal(signing.payloadByteLength, 116);
  assert.equal(signing.requiresBlake2_256, false);

  const extrinsic = encodeEraV14SignedExtrinsic({
    callHex,
    signer: ACCOUNT_11,
    signatureType: "Sr25519",
    signature: SIGNATURE_44,
    nonce: 7,
    tip: 11,
  });
  assert.equal(
    extrinsic,
    `0x3d028400${"11".repeat(32)}01${"44".repeat(64)}001c2c${callHex.slice(2)}`,
  );
  assert.deepEqual(decodeEraV14Extrinsic(extrinsic), {
    signed: true,
    version: 4,
    signer: ACCOUNT_11,
    signatureType: "Sr25519",
    signature: SIGNATURE_44,
    era: { type: "Immortal" },
    nonce: 7,
    tip: 11,
    call: {
      palletIndex: 16,
      callIndex: 8,
      pallet: "Assets",
      method: "transfer",
      args: { id: 7, target: ACCOUNT_22, amount: 1_000 },
    },
  });
});

test("metadata verification rejects bytes outside the pinned runtime surface", async () => {
  await assert.rejects(() => assertEraV14Metadata("0x00"), /metadata hash does not match/);
});

test("codec rejects invalid accounts, unsupported calls, and oversized metadata", () => {
  assert.throws(
    () => encodeEraV14Call("Balances", "transferKeepAlive", { dest: "0x11", amount: 1 }),
    /exactly 32 bytes/,
  );
  assert.throws(() => encodeEraV14Call("Assets", "notACall", {}), /unsupported/);
  assert.throws(
    () => encodeEraV14Call("Assets", "setMetadata", {
      id: 1,
      name: "x".repeat(65),
      symbol: "X",
      decimals: 0,
    }),
    /64-byte V14 bound/,
  );
});
