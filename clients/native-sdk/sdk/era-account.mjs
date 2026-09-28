// Public-account encoding only; no signing or secret-key handling.
// BLAKE2b, unkeyed, RFC 7693. Used for SS58's BLAKE2b-512 checksum.
import {hexToBytes, bytesToHex} from './era-v14-native.mjs';
const MASK=(1n<<64n)-1n;
const IV=[0x6a09e667f3bcc908n,0xbb67ae8584caa73bn,0x3c6ef372fe94f82bn,0xa54ff53a5f1d36f1n,0x510e527fade682d1n,0x9b05688c2b3e6c1fn,0x1f83d9abfb41bd6bn,0x5be0cd19137e2179n];
const SIGMA=[[0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15],[14,10,4,8,9,15,13,6,1,12,0,2,11,7,5,3],[11,8,12,0,5,2,15,13,10,14,3,6,7,1,9,4],[7,9,3,1,13,12,11,14,2,6,5,10,4,0,15,8],[9,0,5,7,2,4,10,15,14,1,11,12,6,8,3,13],[2,12,6,10,0,11,8,3,4,13,7,5,15,14,1,9],[12,5,1,15,14,13,4,10,0,7,6,3,9,2,8,11],[13,11,7,14,12,1,3,9,5,0,15,4,8,6,2,10],[6,15,14,9,11,3,0,8,12,2,13,7,1,4,10,5],[10,2,8,4,7,6,1,5,15,11,9,14,3,12,13,0]];
const rotate=(v,n)=>((v>>n)|(v<<(64n-n)))&MASK;
export function blake2b(bytes,length=64) {
 if(!(bytes instanceof Uint8Array)||!Number.isInteger(length)||length<1||length>64)throw new TypeError('BLAKE2b bytes/output length');
 const h=[...IV];h[0]^=0x01010000n|BigInt(length);
 for(let offset=0;offset<bytes.length||offset===0;offset+=128){
  const size=Math.min(128,bytes.length-offset),block=new Uint8Array(128);block.set(bytes.subarray(offset,offset+size));
  const m=Array.from({length:16},(_,i)=>{let n=0n;for(let j=7;j>=0;j--)n=(n<<8n)|BigInt(block[i*8+j]);return n;});
  const v=[...h,...IV],total=BigInt(offset+size);v[12]^=total&MASK;v[13]^=total>>64n;if(offset+size===bytes.length)v[14]^=MASK;
  const g=(a,b,c,d,x,y)=>{v[a]=(v[a]+v[b]+x)&MASK;v[d]=rotate(v[d]^v[a],32n);v[c]=(v[c]+v[d])&MASK;v[b]=rotate(v[b]^v[c],24n);v[a]=(v[a]+v[b]+y)&MASK;v[d]=rotate(v[d]^v[a],16n);v[c]=(v[c]+v[d])&MASK;v[b]=rotate(v[b]^v[c],63n);};
  for(let r=0;r<12;r++){const s=SIGMA[r%10];g(0,4,8,12,m[s[0]],m[s[1]]);g(1,5,9,13,m[s[2]],m[s[3]]);g(2,6,10,14,m[s[4]],m[s[5]]);g(3,7,11,15,m[s[6]],m[s[7]]);g(0,5,10,15,m[s[8]],m[s[9]]);g(1,6,11,12,m[s[10]],m[s[11]]);g(2,7,8,13,m[s[12]],m[s[13]]);g(3,4,9,14,m[s[14]],m[s[15]]);}
  for(let i=0;i<8;i++)h[i]^=v[i]^v[i+8];
 }
 return Uint8Array.from({length},(_,i)=>Number((h[i>>3]>>BigInt((i%8)*8))&255n));
}
const ALPHABET='123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz';
const PREFIX=new TextEncoder().encode('SS58PRE');
function checksum(payload){return blake2b(Uint8Array.from([...PREFIX,...payload])).slice(0,2);}
function base58(bytes){let n=0n;for(const b of bytes)n=n*256n+BigInt(b);let out='';while(n){out=ALPHABET[Number(n%58n)]+out;n/=58n;}for(const b of bytes){if(b!==0)break;out='1'+out;}return out;}
export function decodeEraAccount(address) {
 if(typeof address!=='string')throw new TypeError('ERA account address required');
 if(/^0x[0-9a-fA-F]{64}$/.test(address))return address.toLowerCase();
 if(address.length<46||address.length>48)throw new Error('ERA SS58 AccountId32 required');
 let n=0n;for(const c of address){const i=ALPHABET.indexOf(c);if(i<0)throw new Error('invalid SS58 character');n=n*58n+BigInt(i);}
 const values=[];while(n){values.unshift(Number(n&255n));n>>=8n;}for(const c of address){if(c!=='1')break;values.unshift(0);}
 const bytes=Uint8Array.from(values);if(bytes.length!==35||bytes[0]!==42)throw new Error('ERA requires SS58 prefix 42 and AccountId32');
 const sum=checksum(bytes.slice(0,33));if(bytes[33]!==sum[0]||bytes[34]!==sum[1]||base58(bytes)!==address)throw new Error('invalid SS58 checksum');
 return bytesToHex(bytes.slice(1,33));
}
export function encodeEraAccount(accountId){const raw=hexToBytes(decodeEraAccount(accountId));const payload=Uint8Array.from([42,...raw]);return base58(Uint8Array.from([...payload,...checksum(payload)]));}
export function formatEtkn(baseUnits){if(typeof baseUnits==='number'&&!Number.isSafeInteger(baseUnits))throw new TypeError('exact base units required');const n=BigInt(baseUnits);if(n<0n)throw new RangeError('negative balance');const frac=(n%10n**18n).toString().padStart(18,'0').replace(/0+$/,'');return (n/10n**18n).toString()+(frac?'.'+frac:'');}
export function parseEtkn(text){if(typeof text!=='string'||!/^(0|[1-9][0-9]*)(\.[0-9]{1,18})?$/.test(text))throw new TypeError('decimal ETKN amount with at most 18 places required');const [whole,fraction='']=text.split('.');const value=BigInt(whole)*10n**18n+BigInt(fraction.padEnd(18,'0'));if(value>=1n<<128n)throw new RangeError('ETKN amount exceeds u128');return value;}
