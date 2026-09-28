// Development binding only. Preserved deployed-spec14 bindings remain the default.
import { ERA_V14_NATIVE_BINDINGS, hexToBytes, bytesToHex } from './era-v14-native.mjs';
export const EVALUATION_BINDINGS = Object.freeze({...ERA_V14_NATIVE_BINDINGS,
  specVersion:15, transactionVersion:1,
  metadataSha256:'c188b00f3589677fe7ecfa833d92d884858ad642a6d231d52696298edbe91d40',
  status:'development; not deployed or release accepted',
});
const concat=(...parts)=>Uint8Array.from(parts.flatMap(p=>Array.from(p)));
function uint(n,size) { if(typeof n==="boolean"||(typeof n==="number"&&!Number.isSafeInteger(n)))throw new TypeError("exact integer required; use decimal string or bigint");n=BigInt(n);if(n<0n||n>=(1n<<BigInt(size*8)))throw new RangeError('integer bounds');const b=new Uint8Array(size);for(let i=0;i<size;i++){b[i]=Number(n&255n);n>>=8n;}return b; }
function hash(v) {const b=hexToBytes(v.startsWith('0x')?v:'0x'+v);if(b.length!==32)throw new RangeError('32-byte commitment');return b;}
function vec(v) {const b=new TextEncoder().encode(v);if(b.length<1||b.length>256)throw new RangeError('URI bounds');const prefix=b.length<64?uint(b.length*4,1):uint(b.length*4+1,2);return concat(prefix,b);}
export function encodeEvaluationSubmission({modelId,requestId,artifactHash,predictionHash,uri,confidence=0,expiresAt,cutoffUtc,evidenceAfterUtc}) {
 if(!Number.isInteger(confidence)||confidence<0||confidence>100)throw new RangeError('confidence');
 if(BigInt(cutoffUtc)>=BigInt(evidenceAfterUtc))throw new RangeError('timing');
 return bytesToHex(concat([12,41],uint(modelId,8),hash(requestId),hash(artifactHash),hash(predictionHash),vec(uri),[confidence],uint(expiresAt,4),uint(cutoffUtc,8),uint(evidenceAfterUtc,8)));
}
export function encodeEvaluationEvidence({predictionId,revision,evidenceHash,outcome}) {
 const outcomes={Successful:0,Failed:1,Inconclusive:2};if(!Object.hasOwn(outcomes,outcome)||revision<0||revision>=32)throw new RangeError('evidence revision/outcome');
 return bytesToHex(concat([12,42],uint(predictionId,8),uint(revision,4),hash(evidenceHash),[outcomes[outcome]]));
}
export function encodeModelRegistration(artifactHash,uri) { return bytesToHex(concat([12,0],[128],hash(artifactHash),vec(uri))); }
export function encodeModelApproval(modelId) { return bytesToHex(concat([12,36],uint(modelId,8))); }
export function encodeModelDelegation(modelId,accountId=null) {return bytesToHex(concat([12,39],uint(modelId,8),accountId===null?[0]:concat([1],hash(accountId))));}

// Accounts/network changes invalidate a pending operation. Caller supplies a context reader,
// actual signer/submission transport and a finalized result decoder; no private key storage.
export class EvaluationWalletSession {
 #generation=0; #pending=false; #stop;
 constructor({provider,readContext,sign,submit,finalizedResult}) {Object.assign(this,{provider,readContext,sign,submit,finalizedResult});}
 async connect(applicationName) {
  this.injected=await this.provider.enable(applicationName);
  if(!this.injected?.accounts?.subscribe)throw new Error('account-change subscription required');
  this.#stop=await this.injected.accounts.subscribe(accounts=>{this.accounts=accounts;this.#generation++;});
  this.accounts=await this.injected.accounts.get();return this.accounts;
 }
 disconnect(){this.#generation++;this.#stop?.();this.accounts=[];}
 async transact(account,callHex) {
  if(this.#pending)throw new Error('operation already pending; reconcile before retry');
  if(!this.accounts?.some(a=>a.address===account))throw new Error('account not granted');
  this.#pending=true;const generation=this.#generation;
  try {
   const context=await this.readContext();
   const signed=await this.sign({account,callHex,context,bindings:EVALUATION_BINDINGS});
   const current=await this.readContext();
   if(generation!==this.#generation||current.genesisHash!==context.genesisHash||current.metadataHash!==context.metadataHash||current.specVersion!==context.specVersion)throw new Error('account or network changed; payload invalidated');
   const result=await this.submit(signed); // transport must track finalized block, not inclusion alone
   if(!result.finalizedBlock)throw new Error('unconfirmed; reconcile finalized request before retry');
   const receipt=await this.finalizedResult(result);
   if(!receipt.dispatchSuccess)throw new Error(receipt.dispatchError||'dispatch failed');
   return receipt;
  } finally {this.#pending=false;}
 }
}
