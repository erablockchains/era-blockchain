import {hexToBytes} from './era-v14-native.mjs';
const EVENTS='0x26aa394eea5630e07c48ae0c9558cef780d41e5e16056765bc8461851072c9d7';
// Portable types are exported from, and bound to, the reviewed raw runtime metadata.
export function decodeFinalizedEvents(raw,schema,expectedMetadataSha256,rootType=schema.eventsType) {
 if(schema.metadataSha256!==expectedMetadataSha256)throw new Error('event schema metadata mismatch');
 const data=hexToBytes(raw);if(data.length>5*1024*1024)throw new Error('event byte limit');let offset=0,nodes=0;
 const take=n=>{if(!Number.isSafeInteger(n)||n<0||offset+n>data.length)throw new Error('truncated SCALE events');const v=data.slice(offset,offset+n);offset+=n;return v;};
 const unsigned=n=>{let v=0n;const b=take(n);for(let i=n-1;i>=0;i--)v=(v<<8n)|BigInt(b[i]);return v;};
 const number=v=>v<=BigInt(Number.MAX_SAFE_INTEGER)?Number(v):v.toString();
 const compact=()=>{const b=take(1)[0];if((b&3)===0)return BigInt(b>>2);const size=(b&3)===1?1:(b&3)===2?3:(b>>2)+4;const rest=unsigned(size);return (b&3)===3?rest:((rest<<8n)|BigInt(b))>>2n;};
 const count=()=>{const n=compact();if(n>100000n)throw new Error('SCALE sequence bound');return Number(n);};
 function fields(fs,depth){const values=fs.map(f=>decode(f.type,depth+1));return fs.every(f=>f.name!==null)?Object.fromEntries(fs.map((f,i)=>[f.name,values[i]])):values;}
 function decode(id,depth=0){if(depth>40||++nodes>200000)throw new Error('SCALE structural bound');const t=schema.types[id];if(!t)throw new Error('unknown portable type');const b=t.body;
  switch(t.kind){case 0:return fields(b,depth);case 1:{const index=Number(unsigned(1)),v=b.find(v=>v.index===index);if(!v)throw new Error('unknown SCALE variant');return {variant:v.name,fields:fields(v.fields,depth)};}
   case 2:return Array.from({length:count()},()=>decode(b,depth+1));case 3:{if(b.length>100000)throw new Error('array bound');return Array.from({length:b.length},()=>decode(b.type,depth+1));}case 4:return b.map(i=>decode(i,depth+1));
   case 5:if(b===0){const v=Number(unsigned(1));if(v>1)throw new Error('invalid bool');return !!v;}if(b===1)return String.fromCodePoint(Number(unsigned(4)));if(b===2)return new TextDecoder('utf-8',{fatal:true}).decode(take(count()));if(b>=3&&b<=8)return number(unsigned(2**(b-3)));if(b>=9&&b<=14){const bits=2**(b-9)*8;let v=unsigned(bits/8);if(v&(1n<<BigInt(bits-1)))v-=1n<<BigInt(bits);return v>=BigInt(Number.MIN_SAFE_INTEGER)&&v<=BigInt(Number.MAX_SAFE_INTEGER)?Number(v):v.toString();}break;
   case 6:return number(compact());
  }throw new Error('unsupported SCALE type');
 }
 const events=decode(rootType);if(offset!==data.length)throw new Error('trailing SCALE events');return events;
}
export class EraWsClient {
 constructor(endpoint,{WebSocketClass=globalThis.WebSocket,timeoutMs=15000}={}){const u=new URL(endpoint);if(u.username||u.password||!['ws:','wss:'].includes(u.protocol))throw new Error('credential-free WebSocket endpoint required');if(u.protocol==='ws:'&&!['localhost','127.0.0.1','[::1]'].includes(u.hostname))throw new Error('remote endpoints require WSS');this.socket=new WebSocketClass(endpoint);this.timeoutMs=timeoutMs;this.id=0;this.pending=new Map();this.subscriptions=new Map();this.early=new Map();this.ready=new Promise((resolve,reject)=>{const timer=setTimeout(()=>reject(new Error('WebSocket connection timed out')),timeoutMs);this.socket.addEventListener('open',()=>{clearTimeout(timer);resolve();},{once:true});this.socket.addEventListener('error',()=>{clearTimeout(timer);reject(new Error('WebSocket connection failed'));},{once:true});});this.socket.addEventListener('message',e=>this.message(e.data));this.socket.addEventListener('close',()=>{for(const p of this.pending.values())p.reject(new Error('connection closed; reconcile before retry'));this.pending.clear();});}
 message(raw){try{if(typeof raw!=='string'||raw.length>16_000_000)throw new Error('RPC message limit');const r=JSON.parse(raw);if(Object.hasOwn(r,'id')){const p=this.pending.get(r.id);if(!p)return;this.pending.delete(r.id);r.error?p.reject(new Error(r.error.message)):p.resolve(r.result);}else if(r.params?.subscription!==undefined){const id=r.params.subscription,f=this.subscriptions.get(id);if(f)f(r.params.result);else if(this.early.size<16)this.early.set(id,r.params.result);}}catch{this.close();}}
 async request(method,params=[]){await this.ready;const id=++this.id;return new Promise((resolve,reject)=>{const timer=setTimeout(()=>{this.pending.delete(id);reject(new Error(`${method}: RPC timeout; outcome unknown`));},this.timeoutMs);this.pending.set(id,{resolve:v=>{clearTimeout(timer);resolve(v);},reject:e=>{clearTimeout(timer);reject(e);}});this.socket.send(JSON.stringify({jsonrpc:'2.0',id,method,params}));});}
 async watch(extrinsicHex,{schema,metadataSha256,expectedGenesisHash,timeoutMs=120000}={}) {
  if(!/^0x(?:[0-9a-fA-F]{2})+$/.test(extrinsicHex))throw new Error('signed extrinsic bytes required');
  if(!expectedGenesisHash||await this.request('chain_getBlockHash',[0])!==expectedGenesisHash)throw new Error('wrong genesis; transaction not submitted');
  let id,timer;
  try {
   const finalized=await new Promise(async(resolve,reject)=>{
    timer=setTimeout(()=>reject(new Error('finality timeout; reconcile before retry')),timeoutMs);
    const event=status=>{if(status?.finalized)resolve(status.finalized);else if(status==='invalid'||status==='dropped'||status?.usurped||status?.finalityTimeout)reject(new Error('transaction no longer tracked; reconcile before retry'));};
    try{id=await this.request('author_submitAndWatchExtrinsic',[extrinsicHex]);this.subscriptions.set(id,event);if(this.early.has(id)){const value=this.early.get(id);this.early.delete(id);event(value);}}catch(e){reject(e);}
   });
   return await this.finalizedReceipt(extrinsicHex,finalized,{schema,metadataSha256,expectedGenesisHash});
  } finally {clearTimeout(timer);if(id!==undefined){this.subscriptions.delete(id);this.request('author_unwatchExtrinsic',[id]).catch(()=>{});}}
 }
 async finalizedReceipt(extrinsicHex,finalized,{schema,metadataSha256,expectedGenesisHash}) {
   if(await this.request('chain_getBlockHash',[0])!==expectedGenesisHash)throw new Error('wrong genesis');
   const block=await this.request('chain_getBlock',[finalized]);const index=block.block.extrinsics.findIndex(x=>x.toLowerCase()===extrinsicHex.toLowerCase());if(index<0)throw new Error('finalized block does not contain signed transaction');
   const head=await this.request('chain_getFinalizedHead');const header=await this.request('chain_getHeader',[head]);const height=Number.parseInt(block.block.header.number,16);
   if(Number.parseInt(header.number,16)<height||await this.request('chain_getBlockHash',[height])!==finalized)throw new Error('finality/canonical block mismatch');
   const metadata=hexToBytes(await this.request('state_getMetadata',[finalized]));const digest=Array.from(new Uint8Array(await globalThis.crypto.subtle.digest('SHA-256',metadata)),b=>b.toString(16).padStart(2,'0')).join('');if(digest!==metadataSha256)throw new Error('runtime metadata changed; finalized result requires new decoder');
   const raw=await this.request('state_getStorage',[EVENTS,finalized]);if(!raw)throw new Error('finalized events unavailable');
   const records=decodeFinalizedEvents(raw,schema,metadataSha256).filter(x=>x.phase.variant==='ApplyExtrinsic'&&x.phase.fields[0]===index);
   const system=records.filter(x=>x.event.variant==='System').map(x=>x.event.fields[0]);const ok=system.some(x=>x.variant==='ExtrinsicSuccess'),failed=system.find(x=>x.variant==='ExtrinsicFailed');
   if(ok===!!failed)throw new Error('ambiguous finalized dispatch result');
   return {finalizedBlock:finalized,index,dispatchSuccess:ok,dispatchError:failed?.fields,dispatchErrorName:describeDispatchError(failed?.fields?.dispatch_error,schema),events:records};
 }

 close(){this.socket.close();}
}

export function describeDispatchError(error,schema){
 if(!error)return undefined;
 if(error.variant!=='Module')return error.variant??'Unknown dispatch error';
 const fields=Array.isArray(error.fields)?error.fields[0]:error.fields,pallet=schema.moduleErrors?.[fields?.index],code=fields?.error?.[0];
 return pallet&&pallet.errors?.[code]?pallet.pallet+'.'+pallet.errors[code]:'Unknown module dispatch error';
}
