import assert from "node:assert/strict";
import test from "node:test";

import {
  EraReadOnlyClient,
  EraRpcError,
  READ_ONLY_RPC_METHODS,
} from "../sdk/era-rpc-client.mjs";

function mockRpc(results, seen) {
  return async (_url, options) => {
    const request = JSON.parse(options.body);
    seen.push(request);
    const value = results[request.method];
    return {
      ok: true,
      status: 200,
      async json() {
        if (value instanceof Error) {
          return { jsonrpc: "2.0", id: request.id, error: { code: -32000, message: value.message } };
        }
        return { jsonrpc: "2.0", id: request.id, result: value };
      },
    };
  };
}

test("node snapshot uses only the expected read-only methods", async () => {
  const seen = [];
  const results = {
    system_chain: "era-review",
    system_name: "era-node",
    system_version: "0.1.0",
    system_health: { peers: 0, isSyncing: false, shouldHavePeers: false },
    chain_getHeader: { number: "0x2a" },
    state_getRuntimeVersion: { specVersion: 14, transactionVersion: 1 },
  };
  const client = new EraReadOnlyClient("http://127.0.0.1:9944", {
    fetchImpl: mockRpc(results, seen),
  });

  const snapshot = await client.getNodeSnapshot();
  assert.equal(snapshot.runtime.specVersion, 14);
  assert.equal(snapshot.header.number, "0x2a");
  assert.deepEqual(new Set(seen.map(({ method }) => method)), new Set(Object.keys(results)));
  assert.ok(seen.every(({ method }) => READ_ONLY_RPC_METHODS.includes(method)));
});

test("nonce and fee helpers validate input without submission", async () => {
  const seen = [];
  const client = new EraReadOnlyClient("http://localhost:9944", {
    fetchImpl: mockRpc({ system_accountNextIndex: 7, payment_queryInfo: { partialFee: "12" } }, seen),
  });

  assert.equal(await client.getAccountNextIndex("synthetic-review-account"), 7);
  assert.deepEqual(await client.queryFeeInfo("0x0102"), { partialFee: "12" });
  assert.deepEqual(seen.map(({ method }) => method), ["system_accountNextIndex", "payment_queryInfo"]);
  await assert.rejects(() => client.queryFeeInfo("not-hex"), TypeError);
});

test("metadata, storage, genesis, and block reads remain bounded named operations", async () => {
  const seen = [];
  const hash = `0x${"aa".repeat(32)}`;
  const block = { block: { header: { number: "0x2a" }, extrinsics: ["0x0402"] } };
  const client = new EraReadOnlyClient("http://localhost:9944", {
    fetchImpl: mockRpc({
      chain_getBlockHash: hash,
      state_getMetadata: "0x6d657461",
      state_getStorage: "0x0102",
      chain_getBlock: block,
    }, seen),
  });

  assert.equal(await client.getGenesisHash(), hash);
  assert.equal(await client.getMetadataHex(), "0x6d657461");
  assert.equal(await client.getStorage("0x1234", hash), "0x0102");
  assert.deepEqual(await client.getBlock(hash), block);
  assert.deepEqual(seen.map(({ method }) => method), [
    "chain_getBlockHash",
    "state_getMetadata",
    "state_getStorage",
    "chain_getBlock",
  ]);
  await assert.rejects(() => client.getStorage("not-hex"), TypeError);
});

test("remote and credential-bearing endpoints fail closed", () => {
  assert.throws(() => new EraReadOnlyClient("https://example.invalid"), /allowRemote/);
  assert.throws(
    () => new EraReadOnlyClient("http://example.invalid", { allowRemote: true }),
    /must use HTTPS/,
  );
  const credentialUrl = ["http://", "user", ":", "credential", "@localhost:9944"].join("");
  assert.throws(() => new EraReadOnlyClient(credentialUrl), /credentials/);
});

test("JSON-RPC errors preserve method and code", async () => {
  const client = new EraReadOnlyClient("http://[::1]:9944", {
    fetchImpl: mockRpc({ system_accountNextIndex: new Error("rejected") }, []),
  });
  await assert.rejects(
    () => client.getAccountNextIndex("synthetic-review-account"),
    (error) => error instanceof EraRpcError && error.method === "system_accountNextIndex" && error.code === -32000,
  );
});
