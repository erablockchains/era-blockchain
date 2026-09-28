import test from 'node:test';import assert from 'node:assert/strict';
import {encodeAmmCall} from '../sdk/era-v14-amm.mjs';import {encodeNftMarketCall} from '../sdk/era-v14-nft-market.mjs';import {encodeEvaluationEvidence} from '../sdk/era-v14-evaluation.mjs';
test('inherited JavaScript property names never select an AMM/NFT call or Successful AI outcome',()=>{
 for(const name of ['constructor','toString','__proto__','valueOf','hasOwnProperty']){
  assert.throws(()=>encodeAmmCall(name,{assetA:'NativeEtkn',assetB:{Registered:7},deadline:100}),/unsupported/);
  assert.throws(()=>encodeNftMarketCall(name,{}),/unsupported/);
  assert.throws(()=>encodeEvaluationEvidence({predictionId:0,revision:0,evidenceHash:'0x'+'11'.repeat(32),outcome:name}),/outcome/);
 }
});
