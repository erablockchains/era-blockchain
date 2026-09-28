import test from 'node:test';import assert from 'node:assert/strict';
import {workflows,quoteWorkflow} from '../wallet/workflows.mjs';
const a='0x'+'11'.repeat(32);
test('wallet managed creation never accepts caller-chosen object IDs',()=>{
 assert.equal(workflows[11].encode({issuer:a,admin:a,freezer:a}).slice(0,6),'0x1b04');
 assert.equal(workflows[12].encode({collection:1,recipient:a}).slice(0,14),'0x1b0501000000');
 assert.equal(workflows.find(w=>w.label==='Create admitted fungible asset').encode({issuer:a,admin:a,freezer:a}).slice(0,6),'0x1b03');
 assert.ok(!workflows[12].fields.some(f=>f.key==='item'));
});
test('wallet quote uses explicit tolerance, correct direction, exact bounds and fresh deadline',async()=>{
 const calls=[];const amm={async quote(...args){calls.push(args);return {height:200,value:{amountIn:'101',amountOut:'99'}};}};
 const inside=await quoteWorkflow(amm,4,{a:'ETKN',b:'7',amountIn:'101'},'50');assert.equal(inside.bound,'98');assert.equal(inside.field,'minOut');assert.equal(inside.deadline,'264');assert.equal(calls[0][0],'NativeEtkn');
 const outside=await quoteWorkflow(amm,5,{a:'7',b:'ETKN',amountOut:'99'},'50');assert.equal(outside.bound,'102');assert.equal(outside.field,'maxIn');assert.equal(calls[1][3],'output');
 for(const bps of ['',-1,10000,'1.5',' 50'])await assert.rejects(()=>quoteWorkflow(amm,4,{},bps));
 await assert.rejects(()=>quoteWorkflow(amm,3,{},50));
});

test('application role and lifecycle forms preserve exact base-unit/address and cleanup bounds',()=>{
 const form=label=>workflows.find(w=>w.label===label);
 assert.equal(form('Assets · mint').encode({id:'9',recipient:a,amount:'1000'}),'0x10060900000000'+'11'.repeat(32)+'a10f');
 assert.equal(form('Nfts · transfer').encode({collection:'2',item:'1',recipient:a}),'0x1106020000000100000000'+'11'.repeat(32));
 assert.throws(()=>form('Nfts · continueCollectionRetirement').encode({collection:2,limit:0}),/limit/);
 assert.throws(()=>form('Assets · mint').encode({id:9,recipient:'wrong chain address',amount:1000}));
 for(const label of ['Assets · burn','Assets · setMetadata','Assets · clearMetadata','Assets · approveTransfer','Assets · cancelApproval','Assets · transferApproved','Nfts · burn','Nfts · setMetadata','Nfts · startCollectionRetirement','Nfts · continueDelegateCleanup'])assert.ok(form(label),label);
});
