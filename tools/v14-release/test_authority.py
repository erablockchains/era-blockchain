import unittest
from deployment_authority import validate,verify,SCOPE
class Authority(unittest.TestCase):
 def record(self):return dict(schema='ERA_UPGRADE_HOST_AUTHORIZATION_V1',host='era-rpc-01',deployment_authorized=True,manifest_sha256='a'*64,archive_sha256='b'*64,scope=SCOPE,owner_authorization_reference='synthetic approval only',**{k:False for k in ['public_endpoint_authorized','deletion_authorized','external_notifications_authorized','fresh_chain_authorized','network_change_authorized','production_transactions_authorized']})
 def test_exact_host_upgrade_scope(self):validate(self.record(),'era-rpc-01','a'*64,'b'*64)
 def test_no_implicit_approval_or_wider_scope(self):
  for field,value in [('deployment_authorized',False),('production_transactions_authorized',True),('network_change_authorized',True),('fresh_chain_authorized',True),('scope',[]),('schema','ERA_DEPLOYMENT_AUTHORIZATION_V1')]:
   r=self.record();r[field]=value
   with self.subTest(field=field),self.assertRaises(ValueError):validate(r,'era-rpc-01','a'*64)
 def test_old_entrypoint_without_operation_is_refused_before_host_or_file_access(self):
  with self.assertRaisesRegex(ValueError,'Explicit reviewed upgrade operation'):verify('era-rpc-01','a'*64)
 def test_fresh_network_and_endpoint_operations_refused(self):
  for operation in ['start_fresh_production','apply_reviewed_linux_network','switch_public_endpoint']:
   with self.subTest(operation=operation),self.assertRaisesRegex(ValueError,'Explicit reviewed upgrade operation'):verify('era-rpc-01','a'*64,operation=operation)
if __name__=='__main__':unittest.main()
