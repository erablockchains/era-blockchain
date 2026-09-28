import test from 'node:test';import assert from 'node:assert/strict';import {encodeNftMarketCall} from '../sdk/era-v14-nft-market.mjs';
test('NFT offer cancellation and price withdrawal use existing call indices',()=>{assert.equal(encodeNftMarketCall('cancelSwap',{offeredCollection:1,offeredItem:2}),'0x11230100000002000000');assert.equal(encodeNftMarketCall('setPrice',{collection:1,item:2,price:null,buyer:null}),'0x111f01000000020000000000');});
test('NFT monetary terms must specify bounded amounts and direction',()=>{assert.throws(()=>encodeNftMarketCall('createSwap',{offeredCollection:0,offeredItem:1,desiredCollection:0,desiredItem:2,price:{amount:1,direction:'Either'},duration:10}),/direction/);assert.throws(()=>encodeNftMarketCall('buyItem',{collection:0,item:1,bidPrice:-1}),/bound/);});

test('NFT prices reject already-rounded JavaScript numbers',()=>{assert.throws(()=>encodeNftMarketCall('buyItem',{collection:0,item:1,bidPrice:1e18}),/exact integer/);});
