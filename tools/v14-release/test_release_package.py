import unittest,tempfile,json,hashlib,tarfile
from pathlib import Path
from prepare_upgrade_release import build,sha
class ReleasePackage(unittest.TestCase):
 def fixture(self,root):
  base=root/'base';base.mkdir();names=['era-node','era_runtime.wasm','era_runtime.compact.compressed.wasm','fleet/local_health.py','fleet/deployment_authority.py','fleet/operation_io.py','fleet/startup-supervisor.py','era-v14.raw.json'];files={}
  for n in names:
   p=base/n;p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes(('public fixture '+n).encode());files[n]=sha(p)
  (base/'artifact-hashes.json').write_text(json.dumps({'files':files,'genesis_hash':'synthetic','wasm_sha256':files['era_runtime.compact.compressed.wasm']}));return base
 def test_bound_archive_preserves_genesis_and_candidate(self):
  with tempfile.TemporaryDirectory() as d:
   r=Path(d);base=self.fixture(r);dest=r/'proposed';new=r/'new';new.write_bytes(b'reviewed replacement');result=build(base,dest,new,new,new,'1'*40)
   self.assertEqual((base/'era-v14.raw.json').read_bytes(),(dest/'era-v14.raw.json').read_bytes());self.assertFalse(json.loads((dest/'artifact-hashes.json').read_text())['final_deployment_execution_approved_in_package'])
   manifest=json.loads((dest/'artifact-hashes.json').read_text());self.assertEqual(manifest['wasm_sha256'],sha(new));self.assertEqual(manifest['genesis_wasm_sha256'],sha(base/'era_runtime.compact.compressed.wasm'))
   with tarfile.open(r/'proposed.tar.gz') as t:
    m=json.load(t.extractfile('release/artifact-hashes.json'))
    for n,h in m['files'].items():self.assertEqual(hashlib.sha256(t.extractfile('release/'+n).read()).hexdigest(),h)
   with self.assertRaises(ValueError):build(base,dest,new,new,new,'1'*40)
 def test_baseline_drift_refused_before_output(self):
  with tempfile.TemporaryDirectory() as d:
   r=Path(d);base=self.fixture(r);(base/'era-v14.raw.json').write_text('changed genesis');dest=r/'proposed'
   with self.assertRaises(ValueError):build(base,dest,base/'era-node',base/'era_runtime.wasm',base/'era_runtime.compact.compressed.wasm','1'*40)
   self.assertFalse(dest.exists())
if __name__=='__main__':unittest.main()
