import unittest,tempfile,json
from pathlib import Path
from unittest.mock import patch
from operation_io import verify_release
from deployment_authority import SCOPE
class OperationBinding(unittest.TestCase):
 def test_manifest_verification_carries_explicit_operation_and_refuses_legacy_default(self):
  with tempfile.TemporaryDirectory() as d:
   p=Path(d);m={'files':{},**{k:True for k in ['execution_preparation_complete','owner_policy_risk_acceptance_recorded','four_validator_rehearsal_passed','continuous_operation_controls_reviewed']}};(p/'artifact-hashes.json').write_text(json.dumps(m))
   def authority(_host,_digest,*,operation=None):
    if operation not in SCOPE:raise ValueError('operation absent')
   with patch('deployment_authority.verify',side_effect=authority) as called:
    with self.assertRaises(ValueError):verify_release(p)
    verify_release(p,operation='rolling_reviewed_host_restart');self.assertEqual(called.call_args.kwargs['operation'],'rolling_reviewed_host_restart')
if __name__=='__main__':unittest.main()
