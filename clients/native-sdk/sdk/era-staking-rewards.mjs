// Custom SecurityBudget rewards. Not Staking.payout_stakers or AI financial staking.
// Call indices/types are pinned to retained spec14/spec15 metadata. Network and metadata
// identity, permissions, nonce, fees and finality remain enforced by EraWalletController.
import {decodeEraAccount} from './era-account.mjs';
import {bytesToHex,hexToBytes} from './era-v14-native.mjs';
function u32(value,label) {
  if (!['string','number','bigint'].includes(typeof value) ||
      (typeof value==='number' && !Number.isSafeInteger(value)) ||
      !/^(0|[1-9][0-9]*)$/.test(String(value))) throw new TypeError(label+' must be an exact unsigned decimal integer');
  const n=BigInt(value);if(n>0xffffffffn)throw new RangeError(label+' exceeds u32');
  return Uint8Array.from([0n,8n,16n,24n],shift=>Number((n>>shift)&255n));
}
export function encodeRewardPageClaim({era,validator,page=0}) {
  const account=hexToBytes(decodeEraAccount(validator));
  return bytesToHex(Uint8Array.from([20,1,...u32(era,'era'),...account,...u32(page,'page')]));
}
