import {EVALUATION_BINDINGS} from '../sdk/era-v14-evaluation.mjs';
const networks=Object.freeze({
 development:{genesisHash:'0xcd415d07bdfae8f8b22635963157d25a5be9dfa8ec99a7db336d19705bcba096',rpc:'ws://127.0.0.1:21944',label:'ERA V14 isolated development'},
 production:{genesisHash:'0x0abc2c3d8db5815541050b73da4d81267ebf14d90dbee8d7258155b667ea112e',rpc:'wss://eraprojects.org/',label:'ERA V14'},
});
// Profiles select only reviewed network identities. They cannot redefine codecs or raw metadata.
export function validateWalletProfile(value){
 if(!value||!Object.hasOwn(networks,value.network))throw new Error('reviewed network profile required');
 const expected=networks[value.network];
 for(const field of ['genesisHash','rpc'])if(value[field]!==expected[field])throw new Error('network profile '+field+' mismatch');
 if(value.specVersion!==EVALUATION_BINDINGS.specVersion||value.transactionVersion!==EVALUATION_BINDINGS.transactionVersion||value.metadataSha256!==EVALUATION_BINDINGS.metadataSha256)throw new Error('profile does not match the reviewed application metadata');
 return Object.freeze({...expected,network:value.network});
}
export async function loadWalletProfile(fetchProfile=globalThis.fetch){
 const response=await fetchProfile('./network-profile.json',{cache:'no-store'});if(!response.ok)throw new Error('reviewed network profile unavailable');return validateWalletProfile(await response.json());
}
