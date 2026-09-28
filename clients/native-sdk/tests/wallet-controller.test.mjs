import test from 'node:test';import assert from 'node:assert/strict';import {readFileSync} from 'node:fs';
import {EraWalletController,accountStorageKey} from '../sdk/era-wallet-controller.mjs';
import {ERA_V14_NATIVE_BINDINGS as bindings} from '../sdk/era-v14-native.mjs';
import {encodeEraAccount} from '../sdk/era-account.mjs';
const schema=JSON.parse(readFileSync(new URL('../fixtures/deployed-event-schema.json',import.meta.url)));
const metadata='0x'+readFileSync(new URL('../fixtures/deployed-metadata.scale',import.meta.url)).toString('hex');
const hash=n=>'0x'+n.toString(16).padStart(64,'0'),id='0x'+'11'.repeat(32),address=encodeEraAccount(id),genesis=hash(9999),call='0x020000'+'22'.repeat(32)+'04';
function fixture(){
 const f={height:42,nonce:4,free:100n*10n**18n,fee:'1000000000000000',version:14,genesis,signs:0,submits:0,enabled:0,blocks:new Map()};
 const memory=new Map();f.storage={getItem:k=>memory.get(k)??null,setItem:(k,v)=>memory.set(k,v),removeItem:k=>memory.delete(k)};
 const le=(v,n)=>{v=BigInt(v);return Array.from({length:n},()=>{const b=Number(v&255n);v>>=8n;return b.toString(16).padStart(2,'0');}).join('');};
 f.rpc={async request(method,args=[]){switch(method){
  case 'chain_getBlockHash':return args[0]===0?f.genesis:hash(args[0]);case 'chain_getFinalizedHead':return hash(f.height);
  case 'chain_getHeader':return {number:'0x'+f.height.toString(16)};
  case 'state_getRuntimeVersion':return {specVersion:f.version,transactionVersion:1};case 'state_getMetadata':return metadata;
  case 'system_accountNextIndex':return f.nonce;
  case 'state_getStorage':assert.equal(args[0],accountStorageKey(address));return '0x'+le(f.nonce,4)+le(0,4)+le(1,4)+le(0,4)+le(f.free,16)+le(2n*10n**18n,16)+le(0,16)+le(1n<<127n,16);
  case 'payment_queryInfo':return {partialFee:f.fee};
  case 'chain_getBlock':if(f.missing) return null;return {block:{header:{number:'0x'+Number(BigInt(args[0])).toString(16)},extrinsics:f.blocks.get(args[0])??[]}};
  default:throw new Error('unexpected '+method);
 }},async watch(extrinsic,options){f.submits++;f.extrinsic=extrinsic;assert.ok(f.controller.pending());if(f.unknown)throw new Error('connection lost');return {finalizedBlock:hash(43),dispatchSuccess:!f.dispatchFailed};},async finalizedReceipt(extrinsic,at){assert.equal(extrinsic,f.extrinsic);return {finalizedBlock:at,dispatchSuccess:true};}};
 const injected={metadata:{async provide(definition){f.metadataDefinition=definition;await f.onMetadata?.();return !f.metadataDenied;}},accounts:{async get(){return [{address}];},async subscribe(cb){f.change=cb;cb([{address}]);return ()=>{};}},signer:{async signPayload(p){f.signs++;f.payload=p;await f.onSign?.();if(f.cancel)throw new Error('user rejected');return {signature:'0x01'+'33'.repeat(64)};}}};
 const registry={test:{async enable(){f.enabled++;return injected;}}};
 let held=false;const locks={async request(_name,_options,callback){if(held)return callback(null);held=true;try{return await callback({name:_name});}finally{held=false;}}};
 f.options={locks,rpc:f.rpc,bindings,schema,expectedGenesisHash:genesis,source:'test',registry,storage:f.storage};f.controller=new EraWalletController(f.options);return f;
}
test('wallet uses finalized metadata/balances, displays precise fee, and signs the preview nonce/checkpoint',async()=>{
 const f=fixture();await f.controller.connect();const p=await f.controller.prepare(address,call);
 assert.equal(p.feeEtkn,'0.001');assert.equal(p.balance.freeEtkn,'100');assert.equal(p.balance.reservedEtkn,'2');
 const r=await f.controller.signAndSubmit(p);assert.ok(r.dispatchSuccess);assert.equal(f.payload.blockHash,hash(42));assert.equal(f.payload.nonce,'0x00000004');assert.equal(f.enabled,1);assert.equal(f.submits,1);assert.equal(f.controller.pending(),null);
 await assert.rejects(()=>f.controller.signAndSubmit(p),/invalidated/);
});
test('wrong chain and metadata version fail before requesting wallet accounts',async()=>{for(const prop of ['genesis','version']){const f=fixture();f[prop]=prop==='genesis'?hash(123):15;await assert.rejects(()=>f.controller.connect());assert.equal(f.enabled,0);}});
test('permission cancellation, account changes, nonce drift, expiry and fee increases do not broadcast',async()=>{
 for(const kind of ['cancel','account','nonce','expired','fee']){
  const f=fixture();await f.controller.connect();const p=await f.controller.prepare(address,call);
  f.onSign=()=>{if(kind==='cancel')f.cancel=true;if(kind==='account')f.change([]);if(kind==='nonce')f.nonce++;if(kind==='expired')f.height=106;if(kind==='fee')f.fee='2000000000000000';};
  await assert.rejects(()=>f.controller.signAndSubmit(p));assert.equal(f.submits,0);assert.equal(f.controller.pending(),null);
 }
});
test('insufficient fee balance and durable-store failure stop before broadcast',async()=>{
 const f=fixture();await f.controller.connect();f.free=0n;await assert.rejects(()=>f.controller.prepare(address,call),/insufficient/);assert.equal(f.signs,0);
 f.free=10n**18n;const p=await f.controller.prepare(address,call);f.storage.setItem=()=>{throw new Error('storage full');};await assert.rejects(()=>f.controller.signAndSubmit(p),/storage full/);assert.equal(f.submits,0);
});
test('lost finality persists across reconnect; reconciliation finds exact bytes without resubmission',async()=>{
 const f=fixture();await f.controller.connect();f.unknown=true;const p=await f.controller.prepare(address,call);await assert.rejects(()=>f.controller.signAndSubmit(p),/connection/);
 f.controller.disconnect();f.controller=new EraWalletController(f.options);await f.controller.connect();await assert.rejects(()=>f.controller.prepare(address,call),/reconcile/);
 assert.equal((await f.controller.reconcile()).status,'pending-or-unknown');assert.equal(f.submits,1);
 f.height=44;f.blocks.set(hash(43),[f.extrinsic]);const result=await f.controller.reconcile();assert.equal(result.status,'finalized');assert.equal(f.controller.pending(),null);assert.equal(f.submits,1);
});
test('expiry only clears after complete finalized non-inclusion scan; missing history remains unknown',async()=>{
 const f=fixture();await f.controller.connect();f.unknown=true;await assert.rejects(async()=>f.controller.signAndSubmit(await f.controller.prepare(address,call)));
 f.height=106;f.missing=true;await assert.rejects(()=>f.controller.reconcile(),/unavailable/);assert.ok(f.controller.pending());f.missing=false;
 assert.equal((await f.controller.reconcile()).status,'expired-not-included');assert.equal(f.controller.pending(),null);assert.equal(f.submits,1);
});
test('finalized dispatch failure is returned and does not leave a falsely pending operation',async()=>{
 const f=fixture();await f.controller.connect();f.dispatchFailed=true;const receipt=await f.controller.signAndSubmit(await f.controller.prepare(address,call));assert.equal(receipt.dispatchSuccess,false);assert.equal(f.controller.pending(),null);
});

test('concurrent tabs cannot submit the same nonce before a pending record exists',async()=>{
 const f=fixture();await f.controller.connect();const second=new EraWalletController(f.options);await second.connect();
 const p1=await f.controller.prepare(address,call),p2=await second.prepare(address,call);
 let release,started;const ready=new Promise(resolve=>started=resolve);f.onSign=()=>{started();return new Promise(resolve=>release=resolve);};
 const first=f.controller.signAndSubmit(p1);await ready;await assert.rejects(()=>second.signAndSubmit(p2),/another wallet tab/);release();await first;assert.equal(f.submits,1);
});

test('extension metadata registration is bound to reviewed identity and preserves rejection',async()=>{
 const f=fixture();await f.controller.connect();const r=await f.controller.provideMetadata('ERA test fixture');assert.equal(r.accepted,true);
 assert.equal(f.metadataDefinition.rawMetadata,metadata);assert.equal(f.metadataDefinition.tokenDecimals,18);assert.equal(f.metadataDefinition.tokenSymbol,'ETKN');assert.equal(f.metadataDefinition.ss58Format,42);assert.equal(f.metadataDefinition.genesisHash,genesis);
 f.metadataDenied=true;await assert.rejects(()=>f.controller.provideMetadata('ERA test fixture'),/not accepted/);assert.equal(f.submits,0);
 f.metadataDenied=false;f.onMetadata=()=>{f.version=15;};await assert.rejects(()=>f.controller.provideMetadata('ERA test fixture'),/version/);
});

test('custom reward page enters the same guarded signing and finality path',async()=>{
 const {encodeRewardPageClaim}=await import('../sdk/era-staking-rewards.mjs');const f=fixture();await f.controller.connect();
 const encoded=encodeRewardPageClaim({era:'42',validator:id,page:0});const preview=await f.controller.prepare(address,encoded);
 assert.equal(f.signs,0);assert.equal(f.submits,0);const result=await f.controller.signAndSubmit(preview);
 assert.equal(f.payload.method,encoded);assert.equal(f.signs,1);assert.equal(f.submits,1);assert.ok(result.dispatchSuccess);assert.equal(result.finalizedBlock,hash(43));
});
