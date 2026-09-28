"""Bounded, local-only public RPC checks; no signing or node mutation."""
import json,time,urllib.request

def request(method,params):
 req=urllib.request.Request('http://127.0.0.1:19944',data=json.dumps(dict(jsonrpc='2.0',id=1,method=method,params=params)).encode(),headers={'Content-Type':'application/json'})
 with urllib.request.urlopen(req,timeout=2) as response:
  raw=response.read(65537)
  if len(raw)>65536:raise ValueError('Local RPC response exceeds bound')
  data=json.loads(raw)
 if 'error' in data:raise ValueError('Local RPC error')
 return data['result']
def check(genesis,previous,now,rpc=request):
 state=dict(previous)
 if 0<=now-state.get('checked_epoch',0)<30:return state
 if now<state.get('last_advance_epoch',0):state['last_advance_epoch']=now
 state['checked_epoch']=now
 try:
  version=rpc('state_getRuntimeVersion',[]);spec=version.get('specVersion')
  if rpc('chain_getBlockHash',[0])!=genesis or spec not in (14,15) or version.get('transactionVersion')!=1 or version.get('specName')!='era':raise RuntimeError('Local ERA genesis/runtime differs from reviewed transition')
  if spec<state.get('runtime_spec',14):raise RuntimeError('Reviewed runtime version regressed')
  state['runtime_spec']=spec
  n=int(rpc('chain_getHeader',[rpc('chain_getFinalizedHead',[])])['number'],16)
  if state.get('finalized_number') is None or n>state['finalized_number']:state['last_advance_epoch']=now
  if state.get('finalized_number') is not None and n<state['finalized_number']:raise RuntimeError('Finalized height regressed')
  state.update(rpc_ok=True,finalized_number=n,reason=None,finality_stalled=now-state.get('last_advance_epoch',now)>=180)
 except RuntimeError:raise
 except Exception as error:state.update(rpc_ok=False,reason=type(error).__name__,finality_stalled=False)
 return state
