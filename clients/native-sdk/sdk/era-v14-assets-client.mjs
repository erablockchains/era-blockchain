import {hexToBytes,bytesToHex} from './era-v14-native.mjs';
import {decodeEraAccount} from './era-account.mjs';
const join=(...xs)=>Uint8Array.from(xs.flatMap(x=>Array.from(x)));
function u32(value){if(typeof value==='boolean'||!['string','number','bigint'].includes(typeof value)||typeof value==='string'&&!/^(0|[1-9][0-9]*)$/.test(value)||typeof value==='number'&&!Number.isSafeInteger(value))throw new TypeError('exact u32 required');let n=BigInt(value);if(n<0n||n>0xffffffffn)throw new RangeError('u32 bound');return Uint8Array.from({length:4},()=>{const b=Number(n&255n);n>>=8n;return b;});}
const address=a=>hexToBytes(decodeEraAccount(a));
const cursor=id=>id===null?[0]:join([1],u32(id));
export function encodeAllocatedAssetCall(method,fields){
 if(method==='createAsset'||method==='createCollection')return bytesToHex(join([27,method==='createAsset'?3:4],address(fields.issuer),address(fields.admin),address(fields.freezer)));
 if(method==='mintItem')return bytesToHex(join([27,5],u32(fields.collection),address(fields.recipient)));
 throw new Error('unsupported allocator call');
}
const errors=['NotFound','InvalidLimit','InvalidCursor','UnsupportedAsset','Unconfigured','Arithmetic','BackendInvariant'];
export class AssetsApiError extends Error{constructor(code){super(`Asset API V1: ${errors[code]}`);this.code=code;this.name='AssetsApiError';}}
export function decodeAssetsResult(raw,type){
 const b=hexToBytes(raw);if(b.length>32768)throw new Error('asset API response bound');let offset=0;
 const take=n=>{if(offset+n>b.length)throw new Error('truncated asset API response');const out=b.slice(offset,offset+n);offset+=n;return out;};
 const uint=n=>{const bytes=take(n);let out=0n;for(let i=n-1;i>=0;i--)out=out<<8n|BigInt(bytes[i]);return out;};
 const tag=()=>Number(uint(1));const integer=()=>Number(uint(4));
 const option=read=>{const t=tag();if(t===0)return null;if(t===1)return read();throw new Error('invalid Option tag');};
 const bool=()=>{const t=tag();if(t>1)throw new Error('invalid bool');return t===1;};
 const count=max=>{const first=tag();let n;if((first&3)===0)n=first>>2;else if((first&3)===1){n=(first|tag()<<8)>>2;if(n<64)throw new Error('noncanonical compact length');}else throw new Error('vector bound');if(n>max)throw new Error('vector bound');return n;};
 const bytes=max=>bytesToHex(take(count(max)));const account=()=>bytesToHex(take(32));
 const id=(max=0x7fffffff)=>{const n=integer();if(n===0||n>max)throw new Error('reserved response ID');return n;};
 const asset=()=>{const out={id:id(),owner:account(),issuer:account(),admin:account(),freezer:account(),supply:uint(16).toString(),minimumBalance:uint(16).toString(),isSufficient:bool()};const status=tag();if(status>2)throw new Error('invalid asset status');out.status=['Live','Frozen','Destroying'][status];out.metadata=option(()=>({nameHex:bytes(64),symbolHex:bytes(16),decimals:tag(),frozen:bool()}));return out;};
 const collection=()=>({id:id(),owner:account(),issuer:option(account),admin:option(account),freezer:option(account),itemCount:integer(),metadataHex:option(()=>bytes(128))});
 const item=()=>({collection:id(),id:id(0xffffffff),owner:account(),metadataHex:option(()=>bytes(128))});
 const result=tag();if(result===1){const code=tag();if(code>=errors.length||offset!==b.length)throw new Error('invalid API error');throw new AssetsApiError(code);}if(result!==0)throw new Error('invalid Result tag');
 const readers={asset,collection,item};let value;
 if(Object.hasOwn(readers,type))value=readers[type]();
 else if(['assets','collections','items'].includes(type)){const n=count(64),read=readers[type.slice(0,-1)];value={entries:Array.from({length:n},read),next:option(integer)};if(value.next===0)throw new Error('reserved cursor');}
 else throw new Error('unsupported asset API result');
 if(offset!==b.length)throw new Error('trailing asset API bytes');return value;
}
export class EraAssetsClient{
 #snapshot;
 constructor(wallet,snapshot=undefined){this.wallet=wallet;this.#snapshot=snapshot;}
 async snapshot(){return new EraAssetsClient(this.wallet,await this.wallet.context());}
 async query(method,args,type){const c=this.#snapshot??await this.wallet.context();const raw=await this.wallet.rpc.request('state_call',['EraV14AssetsApiV1_'+method,bytesToHex(args),c.head]);return {at:c.head,height:c.height,value:decodeAssetsResult(raw,type)};}
 page(method,type,cursorId,limit,prefix=[]){if(!Number.isInteger(limit)||limit<1||limit>64)throw new RangeError('page limit');if(cursorId!==null&&!this.#snapshot)throw new Error('use snapshot() for cursor pagination');return this.query(method,join(prefix,cursor(cursorId),u32(limit)),type);}
 asset(id){return this.query('asset_v1',u32(id),'asset');}
 assets(cursorId=null,limit=64){return this.page('assets_v1','assets',cursorId,limit);}
 collection(id){return this.query('collection_v1',u32(id),'collection');}
 collections(cursorId=null,limit=64){return this.page('collections_v1','collections',cursorId,limit);}
 item(collection,id){return this.query('item_v1',join(u32(collection),u32(id)),'item');}
 items(collection,cursorId=null,limit=64){return this.page('items_v1','items',cursorId,limit,u32(collection));}
}
