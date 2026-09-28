"""Public records only; no key, keystore, or database content access."""
import hashlib,json,os,pathlib,stat,tempfile

def canonical(value):return hashlib.sha256(json.dumps(value,sort_keys=True,separators=(',',':')).encode()).hexdigest()
def read_json(path,root_owned=False):
 path=pathlib.Path(path)
 if root_owned:
  for q in [path,*path.parents]:
   s=q.lstat()
   if stat.S_ISLNK(s.st_mode) or s.st_uid!=0 or s.st_mode&0o022:raise ValueError('Untrusted public authority path: '+str(q))
 fd=os.open(path,os.O_RDONLY|os.O_NOFOLLOW)
 with os.fdopen(fd) as f:
  if os.fstat(f.fileno()).st_size>1024*1024:raise ValueError('Public record exceeds bound')
  return json.load(f)
def atomic_json(path,value,mode=0o600):
 path=pathlib.Path(path)
 if path.is_symlink():raise ValueError('Record symlink')
 fd,name=tempfile.mkstemp(prefix='.'+path.name+'-',dir=path.parent)
 try:
  with os.fdopen(fd,'w') as f:
   os.fchmod(f.fileno(),mode);json.dump(value,f,sort_keys=True);f.write('\n');f.flush();os.fsync(f.fileno())
  os.replace(name,path)
  fd=os.open(path.parent,os.O_RDONLY|os.O_DIRECTORY)
  try:os.fsync(fd)
  finally:os.close(fd)
 finally:
  if os.path.exists(name):os.unlink(name)
def verify_release(release,digest=None,*,operation=None):
 release=pathlib.Path(release);raw=(release/'artifact-hashes.json').read_bytes();actual=hashlib.sha256(raw).hexdigest()
 if digest is not None and digest!=actual:raise ValueError('External release manifest mismatch')
 m=json.loads(raw)
 for k in ['execution_preparation_complete','owner_policy_risk_acceptance_recorded','four_validator_rehearsal_passed','continuous_operation_controls_reviewed']:
  if m.get(k) is not True:raise ValueError('Unresolved final gate: '+k)
 for name,want in m['files'].items():
  q=release/name
  if not q.resolve().is_relative_to(release.resolve()) or q.is_symlink() or hashlib.sha256(q.read_bytes()).hexdigest()!=want:raise ValueError('Release drift: '+name)
 from deployment_authority import verify
 import socket
 verify(socket.gethostname(),actual,operation=operation)
 return m,actual
