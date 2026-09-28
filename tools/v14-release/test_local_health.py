import unittest
from local_health import check
class Transition(unittest.TestCase):
 def rpc(self,spec=14,height=10,genesis='g',txn=1,name='era'):
  return lambda m,p:{'state_getRuntimeVersion':{'specVersion':spec,'transactionVersion':txn,'specName':name},'chain_getBlockHash':genesis,'chain_getFinalizedHead':'h','chain_getHeader':{'number':hex(height)}}[m]
 def test_transition_and_no_downgrade(self):
  a=check('g',{},1000,self.rpc());b=check('g',a,1031,self.rpc(15,11));self.assertTrue(b['rpc_ok']);self.assertEqual(b['runtime_spec'],15)
  with self.assertRaises(RuntimeError):check('g',b,1062,self.rpc(14,12))
 def test_identity_refusal(self):
  for kwargs in [dict(spec=16),dict(spec=13),dict(genesis='wrong'),dict(txn=2),dict(name='other')]:
   with self.subTest(kwargs=kwargs),self.assertRaises(RuntimeError):check('g',{},1000,self.rpc(**kwargs))
 def test_stall_and_rpc_failure(self):
  a=check('g',{},1000,self.rpc());b=check('g',a,1200,self.rpc());self.assertTrue(b['finality_stalled'])
  def fail(*_):raise OSError('test disconnected')
  self.assertFalse(check('g',b,1240,fail)['rpc_ok'])
 def test_finality_regression(self):
  a=check('g',{},1000,self.rpc())
  with self.assertRaises(RuntimeError):check('g',a,1031,self.rpc(height=9))
if __name__=='__main__':unittest.main()
