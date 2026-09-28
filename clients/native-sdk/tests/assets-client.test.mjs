import test from 'node:test';import assert from 'node:assert/strict';
import {encodeAllocatedAssetCall,decodeAssetsResult,AssetsApiError,EraAssetsClient} from '../sdk/era-v14-assets-client.mjs';
const a='0x'+'11'.repeat(32),b='0x'+'22'.repeat(32),c='0x'+'33'.repeat(32);
test('allocator calls use index27, plain IDs and explicit operational roles',()=>{
 assert.equal(encodeAllocatedAssetCall('createAsset',{issuer:a,admin:b,freezer:c}),'0x1b03'+a.slice(2)+b.slice(2)+c.slice(2));
 assert.equal(encodeAllocatedAssetCall('createCollection',{issuer:a,admin:b,freezer:c}),'0x1b04'+a.slice(2)+b.slice(2)+c.slice(2));
 assert.equal(encodeAllocatedAssetCall('mintItem',{collection:0x12345678,recipient:a}),'0x1b0578563412'+a.slice(2));
 for(const collection of [-1,2**32,1.5,true,'',null])assert.throws(()=>encodeAllocatedAssetCall('mintItem',{collection,recipient:a}));
 assert.throws(()=>encodeAllocatedAssetCall('toString',{}));
});
test('asset API decoder preserves frozen enums, errors and bounded canonical responses',()=>{
 assert.deepEqual(decodeAssetsResult('0x000000','assets'),{entries:[],next:null});
 for(let code=0;code<7;code++)assert.throws(()=>decodeAssetsResult('0x01'+code.toString(16).padStart(2,'0'),'asset'),e=>e instanceof AssetsApiError&&e.code===code);
 const asset='0x0001000000'+a.slice(2)+b.slice(2)+c.slice(2)+a.slice(2)+'00'.repeat(16)+'01'+'00'.repeat(15)+'000000';
 const v=decodeAssetsResult(asset,'asset');assert.equal(v.id,1);assert.equal(v.minimumBalance,'1');assert.equal(v.metadata,null);assert.equal(v.status,'Live');
 for(const raw of ['0x','0x0000','0x00000000','0x0110','0x000501','0x000100','0x000002',asset+'00'])assert.throws(()=>decodeAssetsResult(raw,'assets'));
});
test('all cursor pages stay pinned to a verified finalized snapshot',async()=>{
 let n=0;const calls=[];const wallet={async context(){return {head:'finalized-'+n++,height:n};},rpc:{async request(method,args){calls.push([method,args]);return '0x000000';}}};
 const client=new EraAssetsClient(wallet);assert.throws(()=>client.assets(1),/snapshot/);const snap=await client.snapshot();await snap.assets();await snap.assets(7,2);await snap.collections();await snap.items(3);
 assert.ok(calls.every(([,args])=>args[2]==='finalized-0'));assert.equal(calls[1][1][1],'0x010700000002000000');assert.equal(calls[3][1][1],'0x030000000040000000');assert.throws(()=>snap.assets(null,65));
 wallet.context=async()=>{throw new Error('wrong identity')};await assert.rejects(()=>client.assets(),/identity/);
});
