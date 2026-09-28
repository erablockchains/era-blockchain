// Local V14 AMM codec. No asset naming, allowance, funding or activation is inferred.
const join=(...xs)=>Uint8Array.from(xs.flatMap(x=>Array.from(x)));
const hex=b=>'0x'+Array.from(b,x=>x.toString(16).padStart(2,'0')).join('');
function u(n,size){if(typeof n==="boolean"||(typeof n==="number"&&!Number.isSafeInteger(n)))throw new TypeError("exact integer required; use decimal string or bigint");n=BigInt(n);if(n<0n||n>=(1n<<BigInt(size*8)))throw new RangeError('unsigned integer bound');const b=[];for(let i=0;i<size;i++){b.push(Number(n&255n));n>>=8n;}return b;}
function asset(a){if(a==='NativeEtkn')return [0];if(a&&Object.keys(a).length===1&&Number.isInteger(a.Registered))return join([1],u(a.Registered,4));throw new TypeError('asset must be NativeEtkn or an explicit Registered ID');}
function account(s){if(!/^0x[0-9a-fA-F]{64}$/.test(s))throw new TypeError('AccountId32 required');return s.slice(2).match(/../g).map(x=>parseInt(x,16));}
export function encodeAmmCall(method,a){
 const ids={createPool:0,addLiquidity:1,removeLiquidity:2,swapExactInput:3,swapExactOutput:4};if(!Object.hasOwn(ids,method))throw new TypeError('unsupported AMM call');
 const first=a.assetA??a.assetIn,last=a.assetB??a.assetOut;const x=asset(first),y=asset(last);if(hex(x)===hex(y))throw new TypeError('identical asset pair');
 let fields=[];
 if(method==='addLiquidity')fields=[u(a.desiredA,16),u(a.desiredB,16),u(a.minA,16),u(a.minB,16)];
 if(method==='removeLiquidity')fields=[u(a.lp,16),u(a.minA,16),u(a.minB,16),account(a.recipient)];
 if(method==='swapExactInput')fields=[u(a.amountIn,16),u(a.minOut,16),account(a.recipient)];
 if(method==='swapExactOutput')fields=[u(a.amountOut,16),u(a.maxIn,16),account(a.recipient)];
 return hex(join([24,ids[method]],x,y,...fields,u(a.deadline,4)));
}
