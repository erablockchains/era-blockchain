import {hexToBytes,bytesToHex} from './era-v14-native.mjs';
import {decodeEraAccount} from './era-account.mjs';
const join=(...xs)=>Uint8Array.from(xs.flatMap(x=>Array.from(x)));
function uint(n,size){if(typeof n==='boolean'||typeof n==='number'&&!Number.isSafeInteger(n))throw new TypeError('exact integer required');n=BigInt(n);if(n<0n||n>=1n<<BigInt(size*8))throw new RangeError('integer bound');return Uint8Array.from({length:size},()=>{const b=Number(n&255n);n>>=8n;return b;});}
function asset(a){if(a==='NativeEtkn')return [0];if(!a||Object.keys(a).length!==1||!Number.isInteger(a.Registered))throw new TypeError('explicit native or registered asset required');return join([1],uint(a.Registered,4));}
const rank=a=>a==='NativeEtkn'?-1:a.Registered;
export function canonicalPool(a,b){asset(a);asset(b);if(rank(a)===rank(b))throw new Error('identical assets');return rank(a)<rank(b)?{asset0:a,asset1:b}:{asset0:b,asset1:a};}
const pool=p=>{const c=canonicalPool(p.asset0,p.asset1);if(rank(c.asset0)!==rank(p.asset0))throw new Error('noncanonical pool');return join(asset(p.asset0),asset(p.asset1));};
const optionalPool=p=>p==null?[0]:join([1],pool(p));
export class AmmApiError extends Error{constructor(code){super(`AMM query rejected (V1 error ${code})`);this.code=code;this.name='AmmApiError';}}
export function decodeAmmResult(raw,type){
 const b=hexToBytes(raw);if(b.length>16384)throw new Error('AMM response bound');let offset=0;
 const take=n=>{if(offset+n>b.length)throw new Error('truncated AMM response');const v=b.slice(offset,offset+n);offset+=n;return v;};
 const u=n=>{const v=take(n);let x=0n;for(let i=n-1;i>=0;i--)x=(x<<8n)|BigInt(v[i]);return x;};
 const asset=()=>{const tag=Number(u(1));if(tag===0)return 'NativeEtkn';if(tag===1)return {Registered:Number(u(4))};throw new Error('invalid asset variant');};
 const p=()=>{const asset0=asset(),asset1=asset();if(rank(asset0)>=rank(asset1))throw new Error('noncanonical pool');return {asset0,asset1};};
 const optional=()=>{const tag=Number(u(1));if(tag===0)return null;if(tag===1)return p();throw new Error('invalid Option tag');};
 const record=()=>({custody:bytesToHex(take(32)),creator:bytesToHex(take(32)),deposit:u(16).toString(),reserve0:u(16).toString(),reserve1:u(16).toString(),totalLp:u(16).toString(),lockedLp:u(16).toString(),userLp:u(16).toString()});
 const entry=kind=>kind==='pool'?{pool:p(),record:record()}:kind==='position'?{pool:p(),account:bytesToHex(take(32)),lp:u(16).toString()}:null;
 const tag=Number(u(1));if(tag===1){const code=Number(u(1));if(offset!==b.length||code>29)throw new Error('invalid AMM error');throw new AmmApiError(code);}if(tag!==0)throw new Error('invalid Result tag');
 let value;
 if(type==='pool'||type==='position')value=entry(type);
 else if(type==='quote')value={pool:p(),assetIn:asset(),assetOut:asset(),amountIn:u(16).toString(),amountOut:u(16).toString(),totalFee:u(16).toString(),protocolFee:u(16).toString()};
 else if(type==='pools'||type==='positions'){
  const first=Number(u(1));let count;if((first&3)===0)count=first>>2;else if((first&3)===1){count=(first|Number(u(1))<<8)>>2;if(count<64)throw new Error('noncanonical compact count');}else throw new Error('page bound');
  if(count>64)throw new Error('page bound');value={entries:Array.from({length:count},()=>entry(type==='pools'?'pool':'position')),next:optional()};
 }else throw new Error('unsupported AMM result type');
 if(offset!==b.length)throw new Error('trailing AMM bytes');return value;
}
export class EraAmmClient {
 constructor(wallet){this.wallet=wallet;}
 async query(method,args,type){const c=await this.wallet.context();const raw=await this.wallet.rpc.request('state_call',['EraV14AmmRuntimeApi_'+method,bytesToHex(args),c.head]);return {at:c.head,height:c.height,value:decodeAmmResult(raw,type)};}
 pool(a,b){return this.query('pool_v1',pool(canonicalPool(a,b)),'pool');}
 pools(cursor=null,limit=64){if(!Number.isInteger(limit)||limit<1||limit>64)throw new RangeError('page limit');return this.query('pools_v1',join(optionalPool(cursor),uint(limit,4)),'pools');}
 position(a,b,address){return this.query('lp_position_v1',join(pool(canonicalPool(a,b)),hexToBytes(decodeEraAccount(address))),'position');}
 positions(address,cursor=null,limit=64){if(!Number.isInteger(limit)||limit<1||limit>64)throw new RangeError('page limit');return this.query('positions_v1',join(hexToBytes(decodeEraAccount(address)),optionalPool(cursor),uint(limit,4)),'positions');}
 async quote(assetIn,assetOut,amount,exact='input'){
  canonicalPool(assetIn,assetOut);if(!['input','output'].includes(exact))throw new Error('quote input/output required');const encoded=uint(amount,16);if(BigInt(amount)===0n)throw new RangeError('positive quote amount');
  const r=await this.query('quote_exact_'+exact+'_v1',join(asset(assetIn),asset(assetOut),encoded),'quote');
  if(rank(r.value.assetIn)!==rank(assetIn)||rank(r.value.assetOut)!==rank(assetOut)||BigInt(r.value[exact==='input'?'amountIn':'amountOut'])!==BigInt(amount))throw new Error('quote response does not match request');return r;
 }
}
export function slippageLimit(amount,basisPoints,exact='input'){
 if(!Number.isInteger(basisPoints)||basisPoints<0||basisPoints>10000)throw new RangeError('slippage basis points');uint(amount,16);const n=BigInt(amount);
 if(exact==='input')return n*BigInt(10000-basisPoints)/10000n;
 if(exact==='output'){const value=(n*BigInt(10000+basisPoints)+9999n)/10000n;uint(value,16);return value;}
 throw new Error('slippage input/output required');
}
