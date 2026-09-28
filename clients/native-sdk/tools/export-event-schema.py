"""Export only the pinned portable type graph needed for finalized event decoding."""
import sys,json,hashlib
from pathlib import Path
from scale_metadata import Reader
b=Path(sys.argv[1]).read_bytes();r=Reader(b);assert r.take(4)==b'meta';r.version=r.u8();assert r.version==14
T={t['id']:t for t in r.vec(r.typ)};P={p['name']:p for p in r.vec(r.pallet)}
entry=next(e for e in P['System']['storage']['entries'] if e['name']=='Events');assert entry['type']['kind']=='plain';root=entry['type']['value'];seen=set()
def walk(i):
 if i in seen:return
 seen.add(i);t=T[i];k=t['kind'];b=t['body'];next=[]
 if k==0:next=[f['type'] for f in b]
 elif k==1:next=[f['type'] for v in b for f in v['fields']]
 elif k in (2,6):next=[b]
 elif k==3:next=[b['type']]
 elif k==4:next=b
 elif k==7:next=[b['store'],b['order']]
 for j in next:walk(j)
account=next(e for e in P['System']['storage']['entries'] if e['name']=='Account');assert account['type']['kind']=='map';account_root=account['type']['value']
walk(root);walk(account_root)
print(json.dumps({'metadataSha256':hashlib.sha256(b).hexdigest(),'eventsType':root,'accountType':account_root,'moduleErrors':{str(p['index']):{'pallet':p['name'],'errors':{str(v['index']):v['name'] for v in T[p['error']]['body']}} for p in P.values() if p['error'] is not None},'types':{str(i):{'kind':T[i]['kind'],'body':T[i]['body']} for i in sorted(seen)}},separators=(',',':')))
