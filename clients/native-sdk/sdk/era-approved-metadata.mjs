// Local preparation only; the deployed wallet does not yet import this module.
const approved = {
  "bafkreigneb4mdfbsrjf764whskvoo2pqsl5d2kjc6kyxsnyz5ikxcede6m": {
    "bytes": 594,
    "sha256": "cd2078c194328a4bff72c792aae769f092fa3d2922f2b1793719ea15711064f3"
  },
  "bafkreifnjhrm572sicfwfx6wurmavad4vkab3tdfk4btxob43shh6qhiey": {
    "bytes": 542,
    "sha256": "ad49e2ceff52408b62dfd6a4580a807caa801dcc6557033bb83cdc8e7f40e826"
  },
  "bafkreiby6jtpx5bdtosvclzjssu4rze37w7ehdlluilefjadhs737mwonu": {
    "bytes": 558,
    "sha256": "38f266fbf4239ba5512f2994a9c8e49bfdbe438d6ba21642a4033cbfbfb2ce6d"
  },
  "bafkreihxzvde4g7s2p6c3k7rkdtbumeobgtfe6fgp2dca7cugc3mns6noq": {
    "bytes": 558,
    "sha256": "f7cd464e1bf2d3fc2dabf150e61a308e09a65278a67e86207c5430b6c6cbcd74"
  }
};
export async function resolveApprovedMetadata(uri, gatewayOrigin, fetcher=globalThis.fetch) {
  if (typeof uri !== 'string' || !uri.startsWith('ipfs://') || !Object.hasOwn(approved, uri.slice(7))) throw new Error('Unapproved IPFS reference');
  const cid=uri.slice(7), expected=approved[cid], origin=new URL(gatewayOrigin);
  if(origin.protocol!=='https:' || origin.username || origin.password || origin.pathname!=='/' || origin.search || origin.hash) throw new Error('Bind a plain HTTPS gateway origin');
  const controller=new AbortController(), timer=setTimeout(()=>controller.abort(),8000);
  try {
    const response=await fetcher(new URL('/ipfs/'+cid,origin),{method:'GET',mode:'cors',credentials:'omit',redirect:'error',signal:controller.signal});
    if(!response.ok || !response.body) throw new Error('Metadata unavailable');
    const reader=response.body.getReader(),chunks=[];let size=0;
    try {
      while(true){const {value,done}=await reader.read();if(done)break;size+=value.byteLength;if(size>expected.bytes)throw new Error('Metadata too large');chunks.push(value);}
    } catch(e) {await reader.cancel();throw e;} finally {reader.releaseLock();}
    if(size!==expected.bytes)throw new Error('Metadata length mismatch');
    const bytes=new Uint8Array(size);let at=0;for(const v of chunks){bytes.set(v,at);at+=v.byteLength;}
    const hash=[...new Uint8Array(await crypto.subtle.digest('SHA-256',bytes))].map(x=>x.toString(16).padStart(2,'0')).join('');
    if(hash!==expected.sha256)throw new Error('Metadata hash mismatch');
    const metadata=JSON.parse(new TextDecoder('utf-8',{fatal:true}).decode(bytes));
    return {uri,cid,retrievedFrom:origin.origin,sha256:hash,metadata};
  } finally {clearTimeout(timer);}
}
