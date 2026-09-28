#!/usr/bin/env python3
"""Prepare public-record proposals only. Never install records, restart a service or authorize deployment."""
import argparse,hashlib,json,re
from pathlib import Path

def canonical(value):return hashlib.sha256(json.dumps(value,sort_keys=True,separators=(',',':')).encode()).hexdigest()
def record_bytes(value):return (json.dumps(value,sort_keys=True)+'\n').encode()
def propose(receipt_bytes,state,acceptance,new_manifest):
    receipt=json.loads(receipt_bytes);old=receipt['manifest_sha256'];old_receipt=hashlib.sha256(receipt_bytes).hexdigest()
    if not all(re.fullmatch('[0-9a-f]{64}',h) for h in [old,new_manifest]) or old==new_manifest:raise ValueError('distinct old/new manifest hashes required')
    if state['mode']!='CONTINUOUS' or acceptance['mode']!='CONTINUOUS' or acceptance['periodic_renewal_required'] is not False or acceptance['commissioning_accepted'] is not True:raise ValueError('retained continuous acceptance required')
    if len({receipt['host'],state['host'],acceptance['host']})!=1:raise ValueError('host mismatch')
    if receipt['deadline_utc_epoch']-receipt['started_utc_epoch']!=7200:raise ValueError('original commissioning interval altered')
    if state['startup_receipt_sha256']!=old_receipt or acceptance['startup_receipt_sha256']!=old_receipt or acceptance['release_manifest_sha256']!=old or state['acceptance_sha256']!=canonical(acceptance):raise ValueError('old record binding mismatch')
    new_receipt={**receipt,'manifest_sha256':new_manifest};new_receipt_hash=hashlib.sha256(record_bytes(new_receipt)).hexdigest()
    new_acceptance={**acceptance,'release_manifest_sha256':new_manifest,'startup_receipt_sha256':new_receipt_hash}
    new_state={**state,'startup_receipt_sha256':new_receipt_hash,'acceptance_sha256':canonical(new_acceptance)}
    return {'status':'PROPOSED_NOT_INSTALLED_OR_AUTHORIZED','host':receipt['host'],'old_manifest_sha256':old,'new_manifest_sha256':new_manifest,'original_receipt_sha256':old_receipt,'original_state_sha256':canonical(state),'original_acceptance_sha256':canonical(acceptance),'records':{'STARTUP-BUDGET.json':new_receipt,'OPERATING-STATE.json':new_state,'acceptance.json':new_acceptance},'requirements':['Separate owner execution approval and deployment_authority binding for the new manifest.','Quiesce only the approved host supervisor before reading the operation-time public records; preserve originals and install the three records as one recoverable stopped-service step.','Keep original genesis, accepted epoch, commissioning start/deadline, policy hash, mode, warning history, high-water values, usage and storage pause state. No new commissioning or observation window.','The existing supervisor still checks recovery headroom on restart. A manifest update does not waive any floor, reservation, authority or acceptance check.']}

def main():
    a=argparse.ArgumentParser();a.add_argument('--receipt',type=Path,required=True);a.add_argument('--state',type=Path,required=True);a.add_argument('--acceptance',type=Path,required=True);a.add_argument('--new-manifest',required=True);a.add_argument('--output',type=Path,required=True);x=a.parse_args()
    for p in [x.receipt,x.state,x.acceptance]:
        if p.is_symlink() or p.stat().st_size>1024*1024:raise ValueError('bounded regular public record required')
    value=propose(x.receipt.read_bytes(),json.loads(x.state.read_text()),json.loads(x.acceptance.read_text()),x.new_manifest)
    with x.output.open('x') as f:json.dump(value,f,indent=2);f.write('\n')
if __name__=='__main__':main()
