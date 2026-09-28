import assert from "node:assert/strict";
import { chmod, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawnSync } from "node:child_process";
import test from "node:test";
import { fileURLToPath } from "node:url";

const launcher = fileURLToPath(new URL("../node/run-non-authority.sh", import.meta.url));

function reviewSpec(overrides = {}) {
  return {
    name: "era-v14-review",
    id: "era-v14-review",
    chainType: "Development",
    bootNodes: [],
    telemetryEndpoints: null,
    protocolId: null,
    properties: { tokenSymbol: "ERA", tokenDecimals: 18 },
    genesis: { raw: { top: {}, childrenDefault: {} } },
    ...overrides,
  };
}

async function fixture(t) {
  const directory = await mkdtemp(join(tmpdir(), "era-v14-ws8-launcher-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  const fakeNode = join(directory, "fake-era-node");
  const chainSpec = join(directory, "review-spec.json");
  const basePath = join(directory, "new-node-data");
  await writeFile(fakeNode, "#!/bin/sh\nprintf \"%s\\n\" \"$@\"\n", "utf8");
  await chmod(fakeNode, 0o755);
  return { directory, fakeNode, chainSpec, basePath };
}

function runLauncher(paths) {
  return spawnSync("bash", [launcher], {
    encoding: "utf8",
    env: {
      ...process.env,
      ERA_REVIEW_NODE_BINARY: paths.fakeNode,
      ERA_REVIEW_NODEJS_BINARY: process.execPath,
      ERA_REVIEW_CHAIN_SPEC: paths.chainSpec,
      ERA_REVIEW_BASE_PATH: paths.basePath,
      ERA_REVIEW_NODE_NAME: "era-v14-review-node",
      ERA_REVIEW_RPC_PORT: "19944",
      ERA_REVIEW_P2P_PORT: "20333",
      ERA_REVIEW_DAPP_ORIGIN: "http://127.0.0.1:18080",
    },
  });
}

test("launcher validates inputs and passes only non-authority loopback flags", async (t) => {
  const paths = await fixture(t);
  await writeFile(paths.chainSpec, JSON.stringify(reviewSpec()), "utf8");
  const result = runLauncher(paths);

  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /ERA_REVIEW_CHAIN_SPEC_STATUS=PASS/);
  assert.match(result.stdout, /--rpc-methods\nsafe/);
  assert.match(result.stdout, /--listen-addr\n\/ip4\/127\.0\.0\.1\/tcp\/20333/);
  assert.match(result.stdout, /--rpc-cors\nhttp:\/\/127\.0\.0\.1:18080/);
  assert.match(result.stdout, /--no-mdns/);
  assert.match(result.stdout, /--no-telemetry/);
  assert.match(result.stdout, /--no-prometheus/);
  assert.doesNotMatch(result.stdout, /--validator|--rpc-external|--unsafe-rpc-external/);
});

test("launcher rejects a non-disposable chain spec before node execution", async (t) => {
  const paths = await fixture(t);
  await writeFile(
    paths.chainSpec,
    JSON.stringify(reviewSpec({ name: "mainnet", chainType: "Live", bootNodes: ["placeholder-peer"] })),
    "utf8",
  );
  const result = runLauncher(paths);

  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /ERA_REVIEW_CHAIN_SPEC_STATUS=BLOCKED_/);
  assert.doesNotMatch(result.stdout, /--chain/);
});
