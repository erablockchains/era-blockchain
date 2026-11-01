import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {EraReadOnlyClient} from '../sdk/era-rpc-client.mjs';
import {ERA_SPEC16, verifyEraSpec16FinalizedContext} from '../sdk/era-spec16-context.mjs';

const metadata = '0x' + readFileSync(new URL('../fixtures/spec16-metadata.scale', import.meta.url)).toString('hex');
const at = '0x' + '11'.repeat(32);
const runtime = {specName:'era', specVersion:16, transactionVersion:1, stateVersion:1};

function fixture(overrides = {}) {
  const calls = [];
  const replies = {
    chain_getBlockHash: ERA_SPEC16.genesis,
    chain_getFinalizedHead: at,
    chain_getHeader: {number:'0x8cce7'},
    state_getRuntimeVersion: runtime,
    state_getMetadata: metadata,
    ...overrides,
  };
  const fetchImpl = async (_url, options) => {
    const request = JSON.parse(options.body);
    calls.push(request);
    return {ok:true, json:async()=>({jsonrpc:'2.0',id:request.id,result:replies[request.method]})};
  };
  return {client:new EraReadOnlyClient('http://127.0.0.1:9944',{fetchImpl}),calls};
}

test('published spec16 metadata fixture matches the finalized ERA identity', async()=>{
  const {client,calls} = fixture();
  const context = await verifyEraSpec16FinalizedContext(client);
  assert.equal(context.at, at);
  assert.equal(context.height, 576743);
  assert.equal(context.metadataSha256, ERA_SPEC16.metadataSha256);
  assert.deepEqual(calls.map(c=>c.method), [
    'chain_getBlockHash','chain_getFinalizedHead','chain_getHeader',
    'state_getRuntimeVersion','state_getMetadata',
  ]);
  assert.deepEqual(calls.slice(2).map(c=>c.params.at(-1)),[at,at,at]);
});

test('wrong genesis fails before any runtime or metadata read', async()=>{
  const {client,calls}=fixture({chain_getBlockHash:'0x'+'aa'.repeat(32)});
  await assert.rejects(()=>verifyEraSpec16FinalizedContext(client),/genesis mismatch/);
  assert.deepEqual(calls.map(c=>c.method),['chain_getBlockHash']);
});

test('wrong runtime, tampered metadata and malformed finalized hash fail closed', async()=>{
  await assert.rejects(()=>verifyEraSpec16FinalizedContext(fixture({state_getRuntimeVersion:{...runtime,specVersion:17}}).client),/runtime identity mismatch/);
  const last = metadata.slice(0,-2) + (metadata.endsWith('00')?'01':'00');
  await assert.rejects(()=>verifyEraSpec16FinalizedContext(fixture({state_getMetadata:last}).client),/metadata mismatch/);
  await assert.rejects(()=>verifyEraSpec16FinalizedContext(fixture({chain_getFinalizedHead:'0x1234'}).client),/finalized head/);
});
