import {decodeEraAccount,blake2b,formatEtkn} from './era-account.mjs';
import {hexToBytes,bytesToHex,assertEraV14ChainContext,encodeEraMortality,encodeEraV14SignedExtrinsic} from './era-v14-native.mjs';
import {decodeFinalizedEvents} from './era-v14-finalized-client.mjs';
import {signEraV14CallWithInjectedProvider,selectEraV14InjectedProvider} from './era-v14-injected-wallet.mjs';
const ACCOUNT='26aa394eea5630e07c48ae0c9558cef7b99d880ec681799c0cf30e8886371da9';
export function accountStorageKey(address){const id=hexToBytes(decodeEraAccount(address));return '0x'+ACCOUNT+bytesToHex(blake2b(id,16)).slice(2)+bytesToHex(id).slice(2);}
// A transaction is persisted before broadcast. A lost connection never causes an
// automatic second submission. Reload/reconnect retains the unresolved intent.
export class EraWalletController {
 #generation=0; #stop; #busy=false; #prepared;
 constructor({rpc,bindings,schema,expectedGenesisHash,source,applicationName='ERA V14',registry=globalThis.injectedWeb3,storage=globalThis.localStorage,locks=globalThis.navigator?.locks}) {
  if(!storage?.setItem||!storage?.getItem||!storage?.removeItem)throw new Error('durable public transaction storage required');
  if(schema.metadataSha256!==bindings.metadataSha256||schema.accountType===undefined)throw new Error('reviewed account/event schema required');
  Object.assign(this,{rpc,bindings,schema,expectedGenesisHash,source,applicationName,registry,storage,locks});
  this.storageKey='era.pending.'+expectedGenesisHash;
 }
 pending(){const text=this.storage.getItem(this.storageKey);if(!text)return null;const p=JSON.parse(text);if(p.genesisHash!==this.expectedGenesisHash||!/^0x[0-9a-f]+$/i.test(p.extrinsicHex)||!Number.isSafeInteger(p.birth)||p.death!==p.birth+64)throw new Error('invalid pending record; retain it for manual reconciliation');return p;}
 async connect(){
  await this.context(); // No account permission request on an unverified chain.
  this.injected=await selectEraV14InjectedProvider(this.source,this.registry).enable(this.applicationName);
  if(!this.injected?.accounts?.subscribe||!this.injected?.accounts?.get||!this.injected?.signer?.signPayload)throw new Error('wallet needs account subscription and signPayload');
  this.#stop?.();this.#stop=await this.injected.accounts.subscribe(accounts=>{this.accounts=accounts;this.#generation++;this.#prepared=undefined;});
  this.accounts=await this.injected.accounts.get();return this.accounts;
 }
 disconnect(){this.#generation++;this.#prepared=undefined;this.#stop?.();this.#stop=undefined;this.accounts=[];this.injected=undefined;}
 async provideMetadata(chainName){
  if(typeof chainName!=='string'||chainName.length<1||chainName.length>64)throw new Error('explicit network display name required');
  if(!this.injected?.metadata?.provide)throw new Error('this wallet does not expose metadata registration');
  const generation=this.#generation,c=await this.context();
  const accepted=await this.injected.metadata.provide({chain:chainName,genesisHash:c.genesisHash,icon:'substrate',ss58Format:42,chainType:'substrate',specVersion:this.bindings.specVersion,tokenDecimals:18,tokenSymbol:'ETKN',types:{},rawMetadata:c.metadataHex});
  if(generation!==this.#generation)throw new Error('account session changed during metadata approval');
  await this.context();
  if(accepted!==true)throw new Error('wallet metadata registration was not accepted');
  return {accepted:true,genesisHash:c.genesisHash,specVersion:this.bindings.specVersion,metadataSha256:this.bindings.metadataSha256};
 }
 async context(){
  const genesisHash=await this.rpc.request('chain_getBlockHash',[0]);
  if(genesisHash!==this.expectedGenesisHash)throw new Error('wrong genesis');
  const head=await this.rpc.request('chain_getFinalizedHead');
  const [header,runtimeVersion,metadataHex]=await Promise.all([this.rpc.request('chain_getHeader',[head]),this.rpc.request('state_getRuntimeVersion',[head]),this.rpc.request('state_getMetadata',[head])]);
  await assertEraV14ChainContext({bindings:this.bindings,signedExtensions:this.bindings.signedExtensions,runtimeVersion,metadataHex,genesisHash,expectedGenesisHash:this.expectedGenesisHash});
  const height=Number.parseInt(header.number,16);if(!Number.isSafeInteger(height)||height<0)throw new Error('invalid finalized height');
  return {head,height,genesisHash,runtimeVersion,metadataHex};
 }
 async balance(address,context=undefined){
  const c=context??await this.context();const raw=await this.rpc.request('state_getStorage',[accountStorageKey(address),c.head]);
  const account=raw===null?{nonce:0,data:{free:0,reserved:0,frozen:0}}:decodeFinalizedEvents(raw,this.schema,this.bindings.metadataSha256,this.schema.accountType);
  const free=BigInt(account.data.free),reserved=BigInt(account.data.reserved),frozen=BigInt(account.data.frozen);
  return {at:c.head,nonce:account.nonce,free:free.toString(),reserved:reserved.toString(),frozen:frozen.toString(),transferable:(free>frozen?free-frozen:0n).toString(),freeEtkn:formatEtkn(free),reservedEtkn:formatEtkn(reserved)};
 }
 async prepare(address,callHex){
  if(this.#busy||this.pending())throw new Error('reconcile the pending transaction before preparing another');
  if(!this.injected||!this.accounts?.some(a=>a.address===address))throw new Error('account not granted');
  const generation=this.#generation,c=await this.context(),accountId=decodeEraAccount(address);
  const [nonce,balance]=await Promise.all([this.rpc.request('system_accountNextIndex',[address]),this.balance(address,c)]);
  if(!Number.isSafeInteger(nonce)||nonce<0||nonce>0xffffffff)throw new Error('invalid next nonce');
  const era=encodeEraMortality({period:64,current:c.height});
  const dummy=encodeEraV14SignedExtrinsic({callHex,signer:accountId,signatureType:'Ecdsa',signature:'0x'+'00'.repeat(65),nonce,tip:0,era});
  const fee=await this.rpc.request('payment_queryInfo',[dummy,c.head]);
  const partialFee=BigInt(fee.partialFee);if(partialFee<0n)throw new Error('invalid fee response');
  if(BigInt(balance.transferable)<partialFee)throw new Error('insufficient transferable balance for estimated transaction fee');
  if(generation!==this.#generation)throw new Error('account changed during preparation');
  const p=Object.freeze({address,accountId,callHex,nonce,birth:c.height,death:c.height+64,blockHash:c.head,feeBaseUnits:partialFee.toString(),feeEtkn:formatEtkn(partialFee),balance:Object.freeze(balance),generation});
  this.#prepared=p;return p;
 }
 async signAndSubmit(prepared){
  if(!this.locks?.request)throw new Error('Web Locks or an explicit single-process lock adapter is required for submission');
  return this.locks.request(this.storageKey,{ifAvailable:true},async lock=>{
   if(!lock)throw new Error('another wallet tab is signing or tracking this chain; reconcile before retry');
   return this.#signUnderLock(prepared);
  });
 }
 async #signUnderLock(prepared){
  if(this.#busy||this.pending())throw new Error('reconcile the pending transaction before submission');
  if(prepared!==this.#prepared||prepared.generation!==this.#generation)throw new Error('fee preview invalidated; prepare again');
  this.#busy=true;
  try {
   const c=await this.context();await this.assertCurrent(prepared,c);
   const signed=await signEraV14CallWithInjectedProvider({bindings:this.bindings,source:this.source,applicationName:this.applicationName,address:prepared.address,accountId:prepared.accountId,callHex:prepared.callHex,runtimeVersion:c.runtimeVersion,metadataHex:c.metadataHex,genesisHash:c.genesisHash,expectedGenesisHash:this.expectedGenesisHash,blockHash:prepared.blockHash,blockNumber:prepared.birth,nonce:prepared.nonce,mortality:{period:64},registry:this.registry,injectedProvider:this.injected});
   const refreshed=await this.context();await this.assertCurrent(prepared,refreshed);
   const fee=await this.rpc.request('payment_queryInfo',[signed.extrinsicHex,refreshed.head]);
   if(BigInt(fee.partialFee)>BigInt(prepared.feeBaseUnits))throw new Error('fee increased; prepare a new preview');
   const pending={genesisHash:this.expectedGenesisHash,address:prepared.address,accountId:prepared.accountId,nonce:prepared.nonce,birth:prepared.birth,death:prepared.death,extrinsicHex:signed.extrinsicHex,metadataSha256:this.bindings.metadataSha256};
   this.storage.setItem(this.storageKey,JSON.stringify(pending)); // Must succeed before the network write.
   this.#prepared=undefined;
   const receipt=await this.rpc.watch(signed.extrinsicHex,{schema:this.schema,metadataSha256:this.bindings.metadataSha256,expectedGenesisHash:this.expectedGenesisHash});
   this.storage.removeItem(this.storageKey);return receipt; // Includes finalized dispatch failures.
  } finally {this.#busy=false;}
 }
 async assertCurrent(p,c){
  if(p.generation!==this.#generation||!this.injected||!this.accounts?.some(a=>a.address===p.address))throw new Error('account changed; payload invalidated');
  const best=await this.rpc.request('chain_getHeader');const height=Number.parseInt(best.number,16);
  if(!Number.isSafeInteger(height)||c.height>=p.death||height>=p.death||c.height<p.birth)throw new Error('transaction expired or chain moved backwards; prepare again');
  if(await this.rpc.request('chain_getBlockHash',[p.birth])!==p.blockHash)throw new Error('mortality checkpoint changed');
  if(await this.rpc.request('system_accountNextIndex',[p.address])!==p.nonce)throw new Error('nonce changed; prepare again');
 }
 async reconcile(){
  if(!this.locks?.request)throw new Error('Web Locks or an explicit single-process lock adapter is required for reconciliation');
  return this.locks.request(this.storageKey,{ifAvailable:true},async lock=>{
   if(!lock)throw new Error('another wallet tab is signing or tracking this chain; reconcile after it finishes');
   return this.#reconcileUnderLock();
  });
 }
 async #reconcileUnderLock(){
  if(this.#busy)throw new Error('transaction operation in progress');const p=this.pending();if(!p)return {status:'none'};
  this.#busy=true;
  try {
   const c=await this.context();
   for(let height=p.birth;height<=Math.min(c.height,p.death-1);height++){
    const hash=await this.rpc.request('chain_getBlockHash',[height]);if(!hash)throw new Error('historical block unavailable; pending outcome remains unknown');
    const block=await this.rpc.request('chain_getBlock',[hash]);if(!block?.block?.extrinsics)throw new Error('historical block unavailable; pending outcome remains unknown');
    if(block.block.extrinsics.some(x=>x.toLowerCase()===p.extrinsicHex.toLowerCase())){
     const receipt=await this.rpc.finalizedReceipt(p.extrinsicHex,hash,{schema:this.schema,metadataSha256:p.metadataSha256,expectedGenesisHash:p.genesisHash});
     this.storage.removeItem(this.storageKey);return {status:'finalized',receipt};
    }
   }
   if(c.height>=p.death){this.storage.removeItem(this.storageKey);return {status:'expired-not-included',through:c.head};}
   return {status:'pending-or-unknown',through:c.head};
  } finally {this.#busy=false;}
 }
}
