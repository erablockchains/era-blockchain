import test from 'node:test';import assert from 'node:assert/strict';import {readFileSync} from 'node:fs';
import {encodeRewardPageClaim} from '../sdk/era-staking-rewards.mjs';
import {encodeEraAccount} from '../sdk/era-account.mjs';
import {workflows} from '../wallet/workflows.mjs';
const {vectors}=JSON.parse(readFileSync(new URL('../fixtures/reward-call-vectors.json',import.meta.url)));
test('custom reward claim matches independently generated spec14/spec15 SCALE vectors',()=>{for(const v of vectors)assert.equal(encodeRewardPageClaim(v),v.callHex);});
test('SS58 account maps to historical validator, with no caller-chosen payout recipient',()=>{const v=vectors[1];assert.equal(encodeRewardPageClaim({...v,validator:encodeEraAccount(v.validator)}),v.callHex);const w=workflows.find(x=>x.label==='Claim consensus staking reward page');assert.deepEqual(w.fields.map(x=>x.key),['era','validator','page']);assert.equal(w.encode(v),v.callHex);});
test('malformed and inexact eras/pages or account fail before a fee preview or signing',()=>{for(const key of ['era','page'])for(const value of [true,null,'',' 1','1e3','01',-1,1.2,NaN,Infinity,4294967296,{},[]])assert.throws(()=>encodeRewardPageClaim({...vectors[0],[key]:value}));assert.throws(()=>encodeRewardPageClaim({...vectors[0],validator:'invalid'}));});
