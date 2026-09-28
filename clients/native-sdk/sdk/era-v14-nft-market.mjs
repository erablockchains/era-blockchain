// Native NFT trading/swaps: no ERC-20, EVM or cross-chain representation.
const join=(...parts)=>Uint8Array.from(parts.flatMap(p=>Array.from(p)));
const hex=b=>'0x'+Array.from(b,x=>x.toString(16).padStart(2,'0')).join('');
function u(value,size){if(typeof value==="boolean"||(typeof value==="number"&&!Number.isSafeInteger(value)))throw new TypeError("exact integer required; use decimal string or bigint");value=BigInt(value);if(value<0n||value>=(1n<<BigInt(size*8)))throw new RangeError('unsigned integer bound');return Array.from({length:size},()=>{const b=Number(value&255n);value>>=8n;return b;});}
function account(value){if(!/^0x[0-9a-fA-F]{64}$/.test(value))throw new TypeError('AccountId32 required');return [0,...value.slice(2).match(/../g).map(v=>parseInt(v,16))];}
const opt=(v,encode)=>v===null||v===undefined?[0]:[1,...encode(v)];
function price(v){if(v.direction!=='Send'&&v.direction!=='Receive')throw new TypeError('price direction must be Send or Receive');return [...u(v.amount,16),v.direction==='Send'?0:1];}
export function encodeNftMarketCall(method,a){
 const ids={setPrice:31,buyItem:32,createSwap:34,cancelSwap:35,claimSwap:36};if(!Object.hasOwn(ids,method))throw new TypeError('unsupported NFT market call');
 let fields;
 if(method==='setPrice')fields=[u(a.collection,4),u(a.item,4),opt(a.price,x=>u(x,16)),opt(a.buyer,account)];
 if(method==='buyItem')fields=[u(a.collection,4),u(a.item,4),u(a.bidPrice,16)];
 if(method==='createSwap')fields=[u(a.offeredCollection,4),u(a.offeredItem,4),u(a.desiredCollection,4),opt(a.desiredItem,x=>u(x,4)),opt(a.price,price),u(a.duration,4)];
 if(method==='cancelSwap')fields=[u(a.offeredCollection,4),u(a.offeredItem,4)];
 if(method==='claimSwap')fields=[u(a.sendCollection,4),u(a.sendItem,4),u(a.receiveCollection,4),u(a.receiveItem,4),opt(a.witnessPrice,price)];
 return hex(join([17,ids[method]],...fields));
}
