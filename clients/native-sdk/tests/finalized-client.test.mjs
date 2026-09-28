import test from 'node:test';import assert from 'node:assert/strict';import {decodeFinalizedEvents,EraWsClient,describeDispatchError} from '../sdk/era-v14-finalized-client.mjs';
const schema={metadataSha256:'fixture',eventsType:0,types:{0:{kind:2,body:1},1:{kind:1,body:[{index:0,name:'Success',fields:[]},{index:1,name:'Failure',fields:[{name:'code',type:2}]}]},2:{kind:5,body:3}}};
test('portable events preserve failures and reject mismatched or truncated evidence',()=>{assert.deepEqual(decodeFinalizedEvents('0x08000107',schema,'fixture'),[{variant:'Success',fields:{}},{variant:'Failure',fields:{code:7}}]);assert.throws(()=>decodeFinalizedEvents('0x080001',schema,'fixture'),/truncated/);assert.throws(()=>decodeFinalizedEvents('0x0800010700',schema,'fixture'),/trailing/);assert.throws(()=>decodeFinalizedEvents('0x00',schema,'other'),/metadata/);});
test('remote unencrypted or credential-bearing wallet transports are rejected',()=>{assert.throws(()=>new EraWsClient('ws://example.org'),/WSS/);assert.throws(()=>new EraWsClient('wss://name:secret@example.org'),/credential/);});

test('actual metadata Module tuple decodes to the pallet error and preserves unknown codes',()=>{
 const types={moduleErrors:{24:{pallet:'Amm',errors:{20:'SlippageExceeded'}}}};
 assert.equal(describeDispatchError({variant:'Module',fields:[{index:24,error:[20,0,0,0]}]},types),'Amm.SlippageExceeded');
 assert.equal(describeDispatchError({variant:'Module',fields:[{index:99,error:[20,0,0,0]}]},types),'Unknown module dispatch error');
 assert.equal(describeDispatchError({variant:'BadOrigin',fields:{}},types),'BadOrigin');
});
