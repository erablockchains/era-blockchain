import {EraAssetsClient} from '../sdk/era-v14-assets-client.mjs';
import {hexToBytes} from '../sdk/era-v14-native.mjs';
import {resolveApprovedMetadata} from '../sdk/era-approved-metadata.mjs';

// This host-delivered setting selects retrieval only; it cannot alter chain identity or metadata URIs.
export function validateMetadataGateway(value, network) {
  if (!value || value.schema !== 1 || value.network !== network || !['development','production'].includes(network)) throw new Error('Metadata gateway profile mismatch');
  if (value.gatewayOrigin === null) return null;
  const url = new URL(value.gatewayOrigin);
  if (typeof value.gatewayOrigin !== 'string' || url.protocol !== 'https:' || url.username || url.password || url.pathname !== '/' || url.search || url.hash) throw new Error('A plain HTTPS metadata origin is required');
  if (network === 'production' && (url.hostname === 'localhost' || url.hostname.endsWith('.localhost') || url.hostname === '127.0.0.1' || url.hostname === '[::1]')) throw new Error('Local test origin is not a production metadata gateway');
  return url.origin;
}
export async function loadMetadataGateway(network, fetcher=globalThis.fetch) {
  const response=await fetcher('./metadata-gateway.json',{cache:'no-store',credentials:'same-origin',redirect:'error'});
  if (!response.ok) throw new Error('Metadata gateway configuration unavailable');
  return validateMetadataGateway(await response.json(),network);
}
export async function readNftMetadata(wallet,{collection,item='',gatewayOrigin},fetcher=globalThis.fetch) {
  if (!gatewayOrigin) throw new Error('Metadata retrieval is not configured for this deployment');
  if (!wallet) throw new Error('Connect to the verified network first');
  const client=await new EraAssetsClient(wallet).snapshot();
  const record=item===''?await client.collection(collection):await client.item(collection,item);
  if (record.value.metadataHex===null) throw new Error('No metadata reference at the finalized block');
  const uri=new TextDecoder('utf-8',{fatal:true}).decode(hexToBytes(record.value.metadataHex));
  const content=await resolveApprovedMetadata(uri,gatewayOrigin,fetcher);
  return {at:record.at,height:record.height,collection,item:item===''?null:item,owner:record.value.owner,...content};
}
export async function installNftMetadataPanel({document,network,getWallet,action,fetcher=globalThis.fetch}) {
  const button=document.getElementById('nft-metadata-read'),output=document.getElementById('nft-metadata-text'),status=document.getElementById('nft-metadata-status');
  button.disabled=true;button.dataset.unavailable='true';let origin;
  try {
    origin=await loadMetadataGateway(network,fetcher);
    if (!origin) {status.textContent='Metadata retrieval is awaiting deployment configuration.';return;}
    status.textContent='Metadata gateway: '+origin;
    button.dataset.unavailable='false';button.disabled=false;
  } catch(e) {status.textContent=e.message;return;}
  button.onclick=()=>action(async()=>{
    output.textContent='';status.textContent='Reading finalized metadata…';
    try {
      const result=await readNftMetadata(getWallet(),{collection:document.getElementById('nft-collection-id').value,item:document.getElementById('nft-item-id').value,gatewayOrigin:origin},fetcher);
      // Render data as text. Neither JSON nor a metadata field can inject markup/scripts.
      output.textContent=JSON.stringify(result,null,2);status.textContent='Metadata verified against the approved content hash.';
      return 'NFT metadata read at finalized block '+result.height+'. No transaction was requested.';
    } catch(e) {status.textContent='Metadata unavailable: '+e.message;throw e;}
  });
}
