import test from 'node:test';import assert from 'node:assert/strict';import {readFile} from 'node:fs/promises';import {validateWalletProfile,loadWalletProfile} from '../wallet/network-profile.mjs';
const profile=JSON.parse(await readFile(new URL('../wallet/network-profile.json',import.meta.url),'utf8'));
test('reviewed development and prepared production profiles bind exact genesis endpoint and metadata',()=>{
 assert.equal(validateWalletProfile(profile).network,'development');
 const production={...profile,network:'production',genesisHash:'0x0abc2c3d8db5815541050b73da4d81267ebf14d90dbee8d7258155b667ea112e',rpc:'wss://eraprojects.org/'};assert.equal(validateWalletProfile(production).network,'production');
 for(const override of [{network:'unknown'},{genesisHash:'0x'+'11'.repeat(32)},{rpc:'ws://eraprojects.org/'},{rpc:'wss://user:password@eraprojects.org/'},{metadataSha256:'00'.repeat(32)},{specVersion:14}])assert.throws(()=>validateWalletProfile({...production,...override}));
});
test('missing or replaced profile fails before account permissions or signing',async()=>{
 await assert.rejects(()=>loadWalletProfile(async()=>({ok:false})),/unavailable/);
 await assert.rejects(()=>loadWalletProfile(async()=>({ok:true,json:async()=>({...profile,metadataSha256:'ff'.repeat(32)})})),/metadata/);
 assert.equal((await loadWalletProfile(async()=>({ok:true,json:async()=>profile}))).genesisHash,profile.genesisHash);
});
