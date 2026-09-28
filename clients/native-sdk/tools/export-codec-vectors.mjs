import {ERA_V14_NATIVE_BINDINGS as bindings,encodeEraV14Call, buildEraV14SigningPayload, encodeEraV14SignedExtrinsic} from "../sdk/era-v14-native.mjs";
const account=`0x${"22".repeat(32)}`;
const args={dest:account,id:7,admin:account,minBalance:1,beneficiary:account,who:account,target:account,amount:1000, name:"Example",symbol:"APP",decimals:6,delegate:account,owner:account,destination:account,collection:3,item:9,mintTo:account,data:"hello",limit:685,worldId:"world-1",commitment:`0x${"55".repeat(32)}`};
const vectors=[];
for(const [pallet,b] of Object.entries(bindings.pallets))for(const method of Object.keys(b.calls))vectors.push({pallet,method,args,hex:encodeEraV14Call(pallet,method,args)});
const genesisHash="0x0abc2c3d8db5815541050b73da4d81267ebf14d90dbee8d7258155b667ea112e";
const callHex=vectors[0].hex;
console.log(JSON.stringify({bindings,vectors,signing:buildEraV14SigningPayload({callHex,genesisHash,nonce:7,tip:11}),extrinsic:encodeEraV14SignedExtrinsic({callHex,signer:account,signatureType:"Sr25519",signature:`0x${"44".repeat(64)}`,nonce:7,tip:11})},null,2));
