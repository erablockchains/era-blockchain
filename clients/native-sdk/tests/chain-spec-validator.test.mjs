import assert from "node:assert/strict";
import test from "node:test";

import { validateChainSpec } from "../tools/validate-chain-spec.mjs";

function validSpec() {
  return {
    name: "era-v14-review",
    id: "era-v14-review",
    chainType: "Development",
    bootNodes: [],
    telemetryEndpoints: null,
    protocolId: null,
    properties: { tokenSymbol: "ERA", tokenDecimals: 18 },
    genesis: { raw: { top: {}, childrenDefault: {} } },
  };
}

test("accepts a topology-free disposable spec", () => {
  assert.deepEqual(validateChainSpec(validSpec()), []);
});

test("accepts the approved ETKN policy term pending integration resolution", () => {
  const spec = validSpec();
  spec.properties.tokenSymbol = "ETKN";
  spec.chainType = "Local";
  assert.deepEqual(validateChainSpec(spec), []);
});

test("rejects production naming and non-disposable topology", () => {
  const spec = validSpec();
  spec.name = "production-mainnet";
  spec.chainType = "Live";
  spec.bootNodes = ["placeholder-peer"];
  spec.telemetryEndpoints = [["https://example.invalid/telemetry", 0]];
  const errors = validateChainSpec(spec).join(" | ");
  assert.match(errors, /disposable review network/);
  assert.match(errors, /Development or Local/);
  assert.match(errors, /bootNodes/);
  assert.match(errors, /telemetryEndpoints/);
});

test("rejects wrong decimals, symbol, or missing genesis", () => {
  const spec = validSpec();
  spec.properties = { tokenSymbol: "OTHER", tokenDecimals: 12 };
  delete spec.genesis;
  const errors = validateChainSpec(spec).join(" | ");
  assert.match(errors, /tokenDecimals/);
  assert.match(errors, /tokenSymbol/);
  assert.match(errors, /genesis/);
});
