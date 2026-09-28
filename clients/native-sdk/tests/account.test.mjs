import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {blake2b,decodeEraAccount,encodeEraAccount,formatEtkn,parseEtkn} from '../sdk/era-account.mjs';
const alice='0xd43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d';
const address='5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY';
test('public-account checksum hash matches independent Python reference at block boundaries',()=>{
 const {vectors}=JSON.parse(readFileSync(new URL('../fixtures/blake2b-reference.json',import.meta.url)));
 for(const v of vectors)for(const length of [32,64])assert.equal(Buffer.from(blake2b(Uint8Array.from(Buffer.from(v.input,'hex')),length)).toString('hex'),v['hash'+length]);
});
test('SS58 public Alice vector, exact account binding, malformed and wrong-network rejection',()=>{
 assert.equal(encodeEraAccount(alice),address);assert.equal(decodeEraAccount(address),alice);
 for(let i=0;i<address.length;i++){const changed=address.slice(0,i)+(address[i]==='1'?'2':'1')+address.slice(i+1);assert.throws(()=>decodeEraAccount(changed));}
 assert.throws(()=>decodeEraAccount('15oF4uVJwmo4TdGW7VvPzCRgDoD8Dp6J9AxvpzDdVhjuySbg'));
 assert.throws(()=>decodeEraAccount('0x'+'00'.repeat(31)));assert.throws(()=>decodeEraAccount(' '+address));
 for(let i=0;i<32;i++){const id='0x'+i.toString(16).padStart(2,'0').repeat(32);assert.equal(decodeEraAccount(encodeEraAccount(id)),id);}
});
test('ETKN decimal I/O preserves all 18 places without floating point',()=>{
 for(const text of ['0','1','1000000000','0.000000000000000001','123.123456789012345678'])assert.equal(formatEtkn(parseEtkn(text)),text);
 for(const text of ['1e18','1.0000000000000000001','-1','NaN','1.',' 1','01',''])assert.throws(()=>parseEtkn(text));
 assert.throws(()=>parseEtkn((1n<<128n).toString()));assert.throws(()=>formatEtkn(Number.MAX_SAFE_INTEGER+1));
});
