import unittest,copy,hashlib
from prepare_continuous_rebind import *
class RebindTests(unittest.TestCase):
 def fixture(self):
  r={'host':'fixture-host','manifest_sha256':'11'*32,'started_utc_epoch':100,'deadline_utc_epoch':7300,'initial_available_bytes':100000,'initial_free_inodes':999};raw=record_bytes(r);h=hashlib.sha256(raw).hexdigest()
  a={'host':r['host'],'mode':'CONTINUOUS','periodic_renewal_required':False,'commissioning_accepted':True,'accepted_epoch':7200,'production_genesis_hash':'0x'+'22'*32,'policy_sha256':'33'*32,'release_manifest_sha256':r['manifest_sha256'],'startup_receipt_sha256':h};s={'host':r['host'],'mode':'CONTINUOUS','startup_receipt_sha256':h,'acceptance_sha256':canonical(a),'high_water_bytes':123456,'high_water_inodes':432,'storage_paused':True,'resume_since_epoch':None,'last_checked_epoch':10000};return raw,s,a
 def test_exact_continuous_usage_policy_and_pause_preservation(self):
  raw,s,a=self.fixture();p=propose(raw,s,a,'44'*32);r=p['records'];self.assertEqual({k:v for k,v in s.items() if k not in ['startup_receipt_sha256','acceptance_sha256']},{k:v for k,v in r['OPERATING-STATE.json'].items() if k not in ['startup_receipt_sha256','acceptance_sha256']});self.assertEqual(r['acceptance.json']['accepted_epoch'],7200);self.assertEqual(r['STARTUP-BUDGET.json']['started_utc_epoch'],100);self.assertEqual(r['STARTUP-BUDGET.json']['deadline_utc_epoch'],7300);self.assertEqual(r['acceptance.json']['policy_sha256'],a['policy_sha256']);self.assertEqual(r['OPERATING-STATE.json']['acceptance_sha256'],canonical(r['acceptance.json']));self.assertEqual(r['OPERATING-STATE.json']['startup_receipt_sha256'],hashlib.sha256(record_bytes(r['STARTUP-BUDGET.json'])).hexdigest());self.assertEqual(propose(raw,s,a,'44'*32),p)
 def test_wrong_host_hash_startup_or_renewal_reject(self):
  for kind in ['host','receipt','acceptance','startup','renewal']:
   raw,s,a=self.fixture()
   if kind=='host':s['host']='wrong'
   if kind=='receipt':s['startup_receipt_sha256']='ff'*32
   if kind=='acceptance':a['accepted_epoch']+=1
   if kind=='startup':s['mode']='STARTUP'
   if kind=='renewal':a['periodic_renewal_required']=True
   with self.assertRaises(ValueError):propose(raw,s,a,'44'*32)
if __name__=='__main__':unittest.main()
