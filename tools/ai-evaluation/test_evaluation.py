import tempfile, unittest, json
from pathlib import Path
from evaluation import *
def doc(points,quality='V'):
    return {'features':[{'properties':{'station':'https://api.weather.gov/stations/KJFK','timestamp':dt.datetime.fromtimestamp(t,UTC).isoformat(),'temperature':{'value':v,'unitCode':'wmoUnit:degC','qualityControl':quality}}} for t,v in points]}
class Chain:
    def __init__(self,j,key):self.j=j;self.key=key;self.ledger={};self.crash=True;self.calls=0
    def find_finalized(self,model,key):return self.ledger.get(key)
    def submit_call(self,call):
        self.calls+=1;e=json.loads(self.j.db.execute('SELECT envelope FROM jobs WHERE request=?',(self.key,)).fetchone()[0]);self.ledger[self.key]={'prediction_id':7,'prediction_hash':digest(e)}
        if self.crash:raise TimeoutError('accepted but response lost')
class EvaluationTests(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory();self.path=Path(self.tmp.name)/'journal.sqlite';self.j=Journal(self.path);self.t=target('2030-01-02');self.now=self.t-22000;self.key=self.j.schedule(0,'2030-01-02',self.now)
    def tearDown(self):self.j.db.close();self.tmp.cleanup()
    def prepared(self):return self.j.prepare(self.key,doc([(self.now-1800,20),(self.now-600,28)]),self.now,'fixture://envelope',100,True)
    def finalized(self):
        self.prepared();c=Chain(self.j,self.key);c.crash=False;self.j.submit(self.key,c,self.now);return c
    def test_restart_after_lost_response_reconciles_without_duplicate(self):
        e=self.prepared();c=Chain(self.j,self.key)
        with self.assertRaises(TimeoutError):self.j.submit(self.key,c,self.now)
        self.j.db.close();self.j=Journal(self.path)
        self.assertEqual(self.j.submit(self.key,c,self.t+100),7);self.assertEqual(c.calls,1)
        self.assertEqual(self.j.prepare(self.key,{},self.t+100,'wrong',200),e)
    def test_changed_finalized_commitment_is_rejected(self):
        self.prepared();c=Chain(self.j,self.key);c.ledger[self.key]={'prediction_id':7,'prediction_hash':'00'*32}
        with self.assertRaises(ValueError):self.j.reconcile(self.key,c)
    def test_failed_prediction_baseline_and_corrections_are_preserved(self):
        self.finalized();e=self.j.score(self.key,doc([(self.t-300,26),(self.t+300,20)]),self.t+7200,'reviewer','operator')
        self.assertFalse(e['correct']);self.assertTrue(e['baseline_correct']);self.assertEqual(e['actual_c'],26)
        with self.assertRaises(ValueError):self.j.score(self.key,doc([(self.t,20)]),self.t+7300,'reviewer','operator')
        e2=self.j.score(self.key,doc([(self.t,20)]),self.t+7300,'reviewer','operator','official correction')
        self.assertTrue(e2['correct']);self.assertEqual(len(self.j.history()['evidence_revisions']),2)
    def test_missing_evidence_waits_grace_then_unavailable(self):
        self.finalized()
        with self.assertRaises(ValueError):self.j.score(self.key,{},self.t+7200,'reviewer','operator')
        e=self.j.score(self.key,{},self.t+93600,'reviewer','operator');self.assertEqual(e['status'],'unavailable');self.assertNotIn('correct',e)
    def test_quality_units_conflicting_duplicates_and_wrong_station(self):
        self.assertEqual(valid_observations(doc([(self.t,20)],'S')),[])
        self.assertEqual(valid_observations(doc([(self.t,20),(self.t,21)])),[])
        d=doc([(self.t,20)]);d['features'][0]['properties']['station']='KXXX';self.assertEqual(valid_observations(d),[])
    def test_late_submissions_and_failures_remain_visible(self):
        with self.assertRaises(ValueError):self.j.prepare(self.key,{},self.now,'x',100)
        with self.assertRaises(ValueError):self.j.prepare(self.key,{},self.t,'x',100)
        self.assertEqual(self.j.history()['jobs'][0]['state'],'missed');self.assertEqual([e['kind'] for e in self.j.history()['audit']],['inference_failed','missed_submission'])
    def test_collector_independence_and_early_evidence(self):
        self.finalized()
        for when,reviewer in [(self.t+7200,'operator'),(self.t,'reviewer')]:
            with self.assertRaises(ValueError):self.j.score(self.key,doc([(self.t,20)]),when,reviewer,'operator')
    def test_calls_and_fixture_export(self):
        e=self.prepared();self.assertEqual(call_submit(e,'fixture://envelope',100)[:6],'0x0c29');self.assertIn('not universal',self.j.html());self.assertTrue(e['fixture'])
if __name__=='__main__':unittest.main()

class AdapterTests(unittest.TestCase):
    def test_profile_and_production_write_refusal(self):
        from rpc_adapter import RpcAdapter,PRODUCTION_GENESIS
        profile={'specVersion':15,'transactionVersion':1,'metadataSha256':hashlib.sha256(b'meta').hexdigest()}
        def transport(m,p):
            return {'chain_getBlockHash':PRODUCTION_GENESIS,'chain_getFinalizedHead':'0x'+'11'*32,'state_getRuntimeVersion':{'specVersion':15,'transactionVersion':1},'state_getMetadata':'0x6d657461'}[m]
        r=RpcAdapter('http://127.0.0.1:19944',PRODUCTION_GENESIS,profile,transport=transport)
        self.assertEqual(r.context(),'0x'+'11'*32)
        with self.assertRaises(ValueError):r.submit_call('0x0c29')
        profile['metadataSha256']='00'*32
        with self.assertRaises(ValueError):r.context()

class CoverageTests(unittest.TestCase):
    def test_never_submitted_schedule_is_counted(self):
        j=Journal(':memory:');t=target('2030-01-02');j.schedule(1,'2030-01-02',t-22000);j.expire(t)
        self.assertEqual(j.history()['coverage']['scheduled'],1);self.assertEqual(j.history()['coverage']['missed'],1)
        j.db.close()

class EvidenceReconciliationTests(unittest.TestCase):
    setUp=EvaluationTests.setUp
    tearDown=EvaluationTests.tearDown
    prepared=EvaluationTests.prepared
    finalized=EvaluationTests.finalized
    def test_only_matching_finalized_evidence_contributes_to_metrics(self):
        c=self.finalized();reviewer='0x'+'23'*32
        ev=self.j.score(self.key,doc([(self.t,26)]),self.t+7200,reviewer,'operator')
        c.find_evidence_finalized=lambda pid,rev:None
        self.assertEqual(self.j.reconcile_evidence(self.key,c),0)
        self.assertEqual(self.j.history()['finalized_original_metrics']['synthetic_fixture']['evaluated'],0)
        receipt={'evidence_hash':digest(ev),'reviewer':reviewer,'outcome':1,'recorded_block':10,'finalized_block':'0x'+'44'*32}
        c.find_evidence_finalized=lambda pid,rev:receipt
        self.assertEqual(self.j.reconcile_evidence(self.key,c),1)
        self.assertEqual(self.j.history()['finalized_original_metrics']['synthetic_fixture']['correct'],0)
        self.j.reconcile_evidence(self.key,c)
        self.assertEqual(len(self.j.history()['finalized_receipts']),1)
        receipt['outcome']=0
        with self.assertRaises(ValueError):self.j.reconcile_evidence(self.key,c)
    def test_incomplete_pagination_cannot_be_scored(self):
        self.finalized();d=doc([(self.t,26)]);d['pagination']={'next':'https://api.weather.gov/next'}
        with self.assertRaises(ValueError):self.j.score(self.key,d,self.t+7200,'reviewer','operator')

class ActualEvidenceStorageTests(unittest.TestCase):
    def test_pinned_twox_revision_keys_match_actual_wasm_storage(self):
        from rpc_adapter import revision_key,concat_key,RpcAdapter
        self.assertEqual(revision_key(0).hex(),'b4def25cfda6ef3a00000000')
        self.assertEqual(revision_key(1).hex(),'5153cb1f00942ff401000000')
        prefix='507d4f0bb08b9e2c877acf6d401c17176ab21c55f8a233793e11faa202815ed1'
        key='0x'+prefix+'c804ce198ec337e3dc762bdd1a09aece0000000000000000b4def25cfda6ef3a00000000'
        value='0x'+'23'*32+'42'*32+'01'+'37000000'
        r=RpcAdapter('http://127.0.0.1:21944','0x'+'11'*32,{'evidencePrefix':prefix},transport=lambda m,p:value if m=='state_getStorage' and p==[key,'0x'+'44'*32] else None)
        r.context=lambda:'0x'+'44'*32
        self.assertEqual(r.find_evidence_finalized(0,0),{'reviewer':'0x'+'23'*32,'evidence_hash':'42'*32,'outcome':1,'recorded_block':55,'finalized_block':'0x'+'44'*32})
