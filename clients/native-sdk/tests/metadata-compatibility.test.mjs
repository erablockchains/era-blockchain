import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import {
  ERA_V14_NATIVE_BINDINGS,
  assertEraCallSupportedByMetadata,
  decodeEraV14Call,
  encodeEraV14Call,
} from "../sdk/era-v14-native.mjs";

const readFixture = async (spec) => JSON.parse(await readFile(
  new URL(`../fixtures/era-spec${spec}-metadata-bindings.json`, import.meta.url),
));
const spec13 = await readFixture(13);
const spec14 = await readFixture(14);
const ACCOUNT_22 = `0x${"22".repeat(32)}`;

test("historical pre-launch fixtures preserve 0 through 19 and add 20 through 21; deployed topology is tested separately", () => {
  for (const [name, index] of Object.entries(spec13.palletIndexMap)) {
    assert.equal(spec14.palletIndexMap[name], index, `${name} index changed`);
  }
  assert.equal(spec14.palletIndexMap.SecurityBudget, 20);
  assert.equal(spec14.palletIndexMap.ValidatorSecurity, 21);
  assert.equal(Object.values(spec14.palletIndexMap).includes(22), false);
  assert.equal(spec14.palletIndexMap.Assets, 16);
  assert.equal(spec14.palletIndexMap.Nfts, 17);
  assert.equal(spec14.palletIndexMap.EraWorlds, 18);
});

test("signed extensions, transaction version, extrinsic version, and runtime API versions stay compatible", () => {
  assert.equal(spec13.source.transaction_version, 1);
  assert.equal(spec14.source.transaction_version, 1);
  assert.equal(spec13.extrinsic.version, 4);
  assert.equal(spec14.extrinsic.version, 4);
  assert.deepEqual(
    spec14.extrinsic.signed_extensions.map((entry) => entry.identifier),
    ERA_V14_NATIVE_BINDINGS.signedExtensions,
  );
  assert.deepEqual(
    spec13.extrinsic.signed_extensions.map((entry) => entry.identifier),
    spec14.extrinsic.signed_extensions.map((entry) => entry.identifier),
  );
  assert.deepEqual(spec13.runtimeApiVersions, spec14.runtimeApiVersions);
});

test("NFT calls zero through 38 retain names, indices, and structural parameter types", () => {
  const pallet = (fixture, name) => fixture.pallets.find((entry) => entry.name === name);
  const oldCalls = pallet(spec13, "Nfts").calls;
  assert.deepEqual(pallet(spec14, "Nfts").calls.filter((call) => call.index <= 38), oldCalls);
  assert.deepEqual(
    pallet(spec14, "Nfts").calls.filter((call) => call.index > 38).map(({ name, index }) => ({ name, index })),
    [
      { name: "start_collection_retirement", index: 39 },
      { name: "continue_collection_retirement", index: 40 },
      { name: "continue_delegate_cleanup", index: 41 },
    ],
  );
});

test("compatible spec-13 calls decode while V14-only calls fail against spec-13 metadata", () => {
  const compatible = encodeEraV14Call("Assets", "transfer", { id: 7, target: ACCOUNT_22, amount: 1_000 });
  assert.deepEqual(assertEraCallSupportedByMetadata(compatible, spec13), {
    pallet: "Assets", call: "transfer", palletIndex: 16, callIndex: 8,
  });
  assert.equal(decodeEraV14Call(compatible).method, "transfer");
  const v14Only = encodeEraV14Call("Nfts", "continueCollectionRetirement", { collection: 3, limit: 685 });
  assert.throws(() => assertEraCallSupportedByMetadata(v14Only, spec13), /absent from supplied metadata/);
  assert.deepEqual(assertEraCallSupportedByMetadata(v14Only, spec14), {
    pallet: "Nfts", call: "continue_collection_retirement", palletIndex: 17, callIndex: 40,
  });
});

test("malformed, wrong-index, dormant, AMM, and EVM paths fail closed", () => {
  assert.throws(() => decodeEraV14Call("0x10"), /truncated/);
  assert.throws(() => decodeEraV14Call("0x1600"), /unsupported.*pallet index/);
  assert.throws(() => decodeEraV14Call("0x1021"), /unsupported.*call index/);
  assert.throws(() => encodeEraV14Call("AMM", "swap", {}), /unsupported/);
  assert.throws(() => encodeEraV14Call("EVM", "call", {}), /unsupported/);
  assert.throws(() => encodeEraV14Call("Contracts", "call", {}), /unsupported/);
  assert.throws(() => encodeEraV14Call("ERC20", "transfer", {}), /unsupported/);
  assert.throws(() => encodeEraV14Call("AssetRuntimeApi", "balance", {}), /unsupported/);
});
