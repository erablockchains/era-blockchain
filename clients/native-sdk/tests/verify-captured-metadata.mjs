import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";

import {
  assertEraV14Metadata,
  bytesToHex,
  detectEraV14Transition,
} from "../sdk/era-v14-native.mjs";

const [spec13Path, spec14Path] = process.argv.slice(2);
if (!spec13Path || !spec14Path) throw new Error("usage: node verify-captured-metadata.mjs SPEC13 SPEC14");
const [spec13, spec14] = await Promise.all([readFile(spec13Path), readFile(spec14Path)]);
const sha = (bytes) => createHash("sha256").update(bytes).digest("hex");
assert.equal(sha(spec13), "5c3715502b0f4cd3c3bc124f13f7f153cd3486e0c3877dc1d685335e57ed4c3f");
assert.equal(sha(spec14), "eed011f659bd492aedb643d775fce7cab26adb89c46db8598f4b2c079897e5d2");
await assert.rejects(() => assertEraV14Metadata(bytesToHex(spec13)), /metadata hash/);
await assertEraV14Metadata(bytesToHex(spec14));
assert.deepEqual(await detectEraV14Transition({
  previousRuntimeVersion: { specVersion: 13, transactionVersion: 1 },
  runtimeVersion: { specVersion: 14, transactionVersion: 1 },
  metadataHex: bytesToHex(spec14),
}), {
  transition: "ERA_SPEC_13_TO_14",
  metadataRefreshed: true,
  previousSpecVersion: 13,
  specVersion: 14,
  transactionVersion: 1,
});
console.log("ERA_V14_CAPTURED_TWO_SPEC_METADATA_TEST=PASS");
