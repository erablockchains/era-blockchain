"""Offline metadata-directed verification independent of the JavaScript SCALE codec.
Reads only retained public metadata and locally exported synthetic vectors; no RPC.
"""
from pathlib import Path
import json, hashlib, sys
from scale_metadata import Reader
C=Path(__file__).resolve().parents[1]
b=(C/'fixtures/deployed-metadata.scale').read_bytes()
assert hashlib.sha256(b).hexdigest()=='eed011f659bd492aedb643d775fce7cab26adb89c46db8598f4b2c079897e5d2'
r=Reader(b); assert r.take(4)==b'meta'; r.version=r.u8();assert r.version==14
T={t['id']:t for t in r.vec(r.typ)}; P={p['name']:p for p in r.vec(r.pallet)}
et=r.ci(); ev=r.u8(); extensions=r.vec(lambda:dict(identifier=r.text(),type=r.ci(),additional_signed=r.ci()));runtime_type=r.ci(); assert r.p==len(b)
x=json.loads(Path(sys.argv[1]).read_text()); B=x['bindings'];assert B['extrinsicVersion']==ev==4
assert B['signedExtensions']==[e['identifier'] for e in extensions]
assert P['FounderCustody']['index']==22 and P['FreshGenesis']['index']==23
params={p['name']:p['type'] for p in T[et]['params']}
assert T[params['Address']]['path'][-1]=='MultiAddress'
assert T[params['Signature']]['path'][-1]=='MultiSignature'
assert T[params['Extra']]['kind']==4 and T[params['Extra']]['body']==[e['type'] for e in extensions]
assert T[params['Call']]['path'][-1]=='RuntimeCall'
# All advertised event/error discriminants must agree with raw deployed metadata.
for category,key in [('events','event'),('errors','error')]:
 for name,entries in B[category].items():
  variants={v['index']:v for v in T[P[name][key]]['body']}
  for index,label in entries.items(): assert variants[int(index)]['name']==label,(category,name,index,label)
checked=[]
for v in x['vectors']:
 p=P[v['pallet']]; rr=Reader(bytes.fromhex(v['hex'][2:]));assert rr.u8()==p['index'];ci=rr.u8();call=next(z for z in T[p['calls']]['body'] if z['index']==ci)
 fields={f['name']:rr.decode(f['type'],T) for f in call['fields']};assert rr.p==len(rr.b),(v['pallet'],v['method'],fields)
 # Validate semantic values, beyond merely consuming bytes.
 for k in ['id','collection','item','limit','min_balance','decimals']:
  if k in fields: assert fields[k]=={'id':7,'collection':3,'item':9,'limit':685,'min_balance':1,'decimals':6}[k]
 for k in ['amount','value']:
  if k in fields: assert fields[k]==1000
 checked.append(dict(pallet=v['pallet'],method=v['method'],runtime_call=call['name'],index=ci,fields=fields,bytes=len(rr.b)))
# Decode signed extrinsic address/signature/extra/call from the runtime's portable types.
rr=Reader(bytes.fromhex(x['extrinsic'][2:])); n=rr.ci(); assert len(rr.b)-rr.p==n; assert rr.u8()==132
address=rr.decode(params['Address'],T); signature=rr.decode(params['Signature'],T); extra=rr.decode(params['Extra'],T); call=rr.decode(params['Call'],T);assert rr.p==len(rr.b)
# Signing payload: runtime call + extra + all additional-signed values, in metadata order.
rr=Reader(bytes.fromhex(x['signing']['payloadHex'][2:])); rr.decode(params['Call'],T);rr.decode(params['Extra'],T)
additional=[rr.decode(e['additional_signed'],T) for e in extensions];assert rr.p==len(rr.b)
assert additional[1:3]==[14,1]
result=dict(metadata_sha256=hashlib.sha256(b).hexdigest(),pallet_count=len(P),extrinsic_version=ev,signed_extensions=extensions,checked_calls=checked,address=address,signature=signature,extra=extra,additional_signed=additional,limits='Offline metadata-directed decoder; no runtime execution, browser wallet or production signing claimed.')
print(json.dumps(result,indent=2))
