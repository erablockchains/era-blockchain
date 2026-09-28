import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const read = (path) => readFile(new URL(`../${path}`, import.meta.url), "utf8");

test("package is private, dependency-free, and Apache-2.0 for ERA-authored code", async () => {
  const packageJson = JSON.parse(await read("package.json"));
  assert.equal(packageJson.private, true);
  assert.equal(packageJson.license, "Apache-2.0");
  assert.deepEqual(packageJson.dependencies, {});
  assert.deepEqual(packageJson.devDependencies, {});
});

test("provenance pins the authorized checkpoint and V14 target", async () => {
  const provenance = JSON.parse(await read("provenance.json"));
  assert.deepEqual(provenance.sourceBase, {
    commit: "85d84c06b6298a3260bce3ff2d1cf0f60d6773df",
    tree: "9f68cb2fcb83502915d8f3764891ef20c0943a3a",
  });
  assert.deepEqual(provenance.targetRuntime, { specVersion: 14, transactionVersion: 1 });
  assert.deepEqual(provenance.finalCandidateBinding, {
    mode: "POSTCOMMIT_EXTERNAL_CHECKSUM_SEALED_EVIDENCE_REQUIRED",
    commit: null,
    tree: null,
    runtimeWasmSha256: null,
    metadataSha256: null,
  });
  assert.equal(provenance.eraAuthoredLicense, "Apache-2.0");
  assert.equal(provenance.mixedThirdPartyTermsRetained, true);
  assert.equal(provenance.publicationAuthorized, false);
  assert.equal(provenance.auditComplete, false);
});

test("sample DApp is user-triggered and imports only the local read-only SDK", async () => {
  const html = await read("sample-dapp/index.html");
  const app = await read("sample-dapp/app.js");
  assert.match(html, /id="connect-form"/);
  assert.match(html, /No request has been sent/);
  assert.match(app, /EraReadOnlyClient/);
  assert.doesNotMatch(app, /author_submitExtrinsic|signAndSend|web3|ethereum/i);
});

test("node launcher is non-authority, safe-RPC, and loopback-only", async () => {
  const script = await read("node/run-non-authority.sh");
  assert.match(script, /--rpc-methods safe/);
  assert.match(script, /\/ip4\/127\.0\.0\.1\/tcp/);
  assert.match(script, /--no-mdns/);
  assert.match(script, /--no-telemetry/);
  assert.doesNotMatch(script, /--validator|--rpc-external|--unsafe-rpc-external|--alice|node-key/);
});

test("litepaper preserves the approved bounded policy", async () => {
  const litepaper = await read("LITEPAPER.md");
  assert.match(litepaper, /1,000,000,000 ETKN/);
  assert.match(litepaper, /burns never restore/);
  assert.match(litepaper, /not a guaranteed yield/);
  assert.match(litepaper, /6,000,000 ETKN/);
  assert.match(litepaper, /10,000[\s\S]{0,80}self-bond/);
  assert.match(litepaper, /above 20% is[\s\S]{0,30}rejected/);
  assert.match(litepaper, /no\s+permanent policy cap/i);
  assert.match(litepaper, /GPU and digital-twin[\s\S]{0,80}deferred to V15/);
});
