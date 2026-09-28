import test from 'node:test';import assert from 'node:assert/strict';
import {canonicalPool,decodeAmmResult,AmmApiError,EraAmmClient,slippageLimit} from '../sdk/era-v14-amm-client.mjs';
test('AMM exact-in/out slippage uses integer floor/ceiling and bounds',()=>{
 assert.equal(slippageLimit('101',50),100n);assert.equal(slippageLimit('101',50,'output'),102n);
 assert.equal(slippageLimit('1000000000000000000',25),997500000000000000n);
 assert.throws(()=>slippageLimit(Number.MAX_SAFE_INTEGER+1,50));assert.throws(()=>slippageLimit('1',10001));assert.throws(()=>slippageLimit((2n**128n-1n).toString(),1,'output'));
 assert.deepEqual(canonicalPool({Registered:8},'NativeEtkn'),{asset0:'NativeEtkn',asset1:{Registered:8}});assert.throws(()=>canonicalPool({Registered:7},{Registered:7}));
});
test('AMM decoder preserves errors and refuses malformed pages and trailing bytes',()=>{
 assert.deepEqual(decodeAmmResult('0x000000','pools'),{entries:[],next:null});
 assert.throws(()=>decodeAmmResult('0x0114','pool'),e=>e instanceof AmmApiError&&e.code===20);
 for(const raw of ['0x','0x0000','0x00000000','0x00050000','0x000501','0x000003','0x011400','0x011e'])assert.throws(()=>decodeAmmResult(raw,'pools'));
});
test('AMM client queries only after verified context and pins reads to its finalized block',async()=>{
 const calls=[],wallet={async context(){return {head:'finalized-fixture',height:42};},rpc:{async request(method,args){calls.push([method,args]);return '0x000000';}}};
 const client=new EraAmmClient(wallet);const result=await client.pools(null,4);assert.equal(result.at,'finalized-fixture');assert.deepEqual(calls,[['state_call',['EraV14AmmRuntimeApi_pools_v1','0x0004000000','finalized-fixture']]]);
 assert.throws(()=>client.pools(null,65));wallet.context=async()=>{throw new Error('wrong genesis');};await assert.rejects(()=>client.pools(),/genesis/);assert.equal(calls.length,1);
});
