import test from 'node:test';import assert from 'node:assert/strict';import {readFileSync} from 'node:fs';
import {validateMetadataGateway,loadMetadataGateway,readNftMetadata} from '../wallet/nft-metadata.mjs';
const cid='bafkreigneb4mdfbsrjf764whskvoo2pqsl5d2kjc6kyxsnyz5ikxcede6m',uri='ipfs://'+cid;
const body=readFileSync(new URL('../fixtures/approved-nft-metadata/collection.json',import.meta.url));
const le=(n,size)=>{let v=BigInt(n);return Array.from({length:size},()=>{const b=Number(v&255n);v>>=8n;return b.toString(16).padStart(2,'0');}).join('');};
function fixture(reference=uri){let calls=[],fetches=0;const bytes=Buffer.from(reference??''),option=reference===null?'00':'01'+(bytes.length<64?le(bytes.length<<2,1):le((bytes.length<<2)|1,2))+bytes.toString('hex');return {calls,get fetches(){return fetches},wallet:{async context(){return {head:'0x'+'12'.repeat(32),height:123};},rpc:{async request(method,args){calls.push([method,args]);return '0x00'+le(1,4)+'11'.repeat(32)+'000000'+le(3,4)+option;}}},fetcher:async(url,options)=>{fetches++;assert.equal(String(url),'https://metadata.invalid/ipfs/'+cid);assert.equal(options.credentials,'omit');return new Response(body);}};}
test('gateway remains unbound and rejects cross-network, insecure and malformed settings',()=>{
 assert.equal(validateMetadataGateway({schema:1,network:'development',gatewayOrigin:null},'development'),null);
 assert.equal(validateMetadataGateway({schema:1,network:'development',gatewayOrigin:'https://localhost:1234'},'development'),'https://localhost:1234');
 for(const config of [{schema:1,network:'production',gatewayOrigin:'https://localhost:1234'},{schema:1,network:'development',gatewayOrigin:'https://metadata.invalid'},{schema:1,network:'production',gatewayOrigin:'http://metadata.invalid'},{schema:1,network:'production',gatewayOrigin:'https://metadata.invalid/child'},{schema:1,network:'production',gatewayOrigin:'https://user:password@metadata.invalid'},{schema:2,network:'production',gatewayOrigin:'https://metadata.invalid'}])assert.throws(()=>validateMetadataGateway(config,'production'));
});
test('configuration fetch is uncached, same-origin and refuses unavailable settings',async()=>{
 await assert.rejects(loadMetadataGateway('development',async()=>new Response('',{status:404})),/unavailable/);
 assert.equal(await loadMetadataGateway('development',async(path,options)=>{assert.equal(path,'./metadata-gateway.json');assert.equal(options.cache,'no-store');assert.equal(options.redirect,'error');return Response.json({schema:1,network:'development',gatewayOrigin:null});}),null);
});
test('NFT read uses a verified finalized snapshot and preserves the exact approved URI',async()=>{
 const f=fixture(),r=await readNftMetadata(f.wallet,{collection:'1',gatewayOrigin:'https://metadata.invalid'},f.fetcher);
 assert.equal(r.uri,uri);assert.equal(r.height,123);assert.equal(r.at,'0x'+'12'.repeat(32));assert.equal(f.calls[0][1][2],r.at);assert.equal(f.calls[0][1][0],'EraV14AssetsApiV1_collection_v1');assert.equal(f.fetches,1);assert.equal(r.metadata.name,JSON.parse(body).name);
});
test('unbound, disconnected and wrong chain states cannot issue content requests',async()=>{
 const f=fixture();await assert.rejects(readNftMetadata(f.wallet,{collection:1,gatewayOrigin:null},f.fetcher),/not configured/);
 await assert.rejects(readNftMetadata(null,{collection:1,gatewayOrigin:'https://metadata.invalid'},f.fetcher),/Connect/);
 f.wallet.context=async()=>{throw new Error('wrong genesis')};await assert.rejects(readNftMetadata(f.wallet,{collection:1,gatewayOrigin:'https://metadata.invalid'},f.fetcher),/genesis/);assert.equal(f.fetches,0);assert.equal(f.calls.length,0);
});
test('missing or unapproved finalized references fail without fetching a fallback',async()=>{
 for(const uri of [null,'ipfs://unapproved','https://metadata.invalid/object']){const f=fixture(uri);await assert.rejects(readNftMetadata(f.wallet,{collection:1,gatewayOrigin:'https://metadata.invalid'},f.fetcher));assert.equal(f.fetches,0);}
});
test('runtime API failures stop retrieval and never become synthetic metadata',async()=>{
 const f=fixture();f.wallet.rpc.request=async()=> '0x0100';await assert.rejects(readNftMetadata(f.wallet,{collection:1,gatewayOrigin:'https://metadata.invalid'},f.fetcher),/NotFound/);assert.equal(f.fetches,0);
});
