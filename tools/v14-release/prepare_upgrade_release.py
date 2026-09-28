#!/usr/bin/env python3
"""Materialize a reviewable public upgrade release locally. No network, service, key or production actions."""
import argparse,hashlib,json,shutil,tarfile,gzip,io
from pathlib import Path

def sha(path):return hashlib.sha256(path.read_bytes()).hexdigest()
def build(base,dest,node,wasm,compressed,commit):
    if dest.exists():raise ValueError('preserve existing candidate; new output required')
    if len(commit)!=40 or any(c not in '0123456789abcdef' for c in commit):raise ValueError('exact source commit required')
    original=json.loads((base/'artifact-hashes.json').read_text());files=original['files']
    for name,digest in files.items():
        p=base/name
        if p.is_symlink() or not p.resolve().is_relative_to(base.resolve()) or sha(p)!=digest:raise ValueError('baseline manifest drift: '+name)
    dest.mkdir(parents=True)
    for name in files:
        p=dest/name;p.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(base/name,p)
    replacements={'era-node':node,'era_runtime.wasm':wasm,'era_runtime.compact.compressed.wasm':compressed,'fleet/local_health.py':Path(__file__).with_name('local_health.py'),'fleet/deployment_authority.py':Path(__file__).with_name('deployment_authority.py'),'fleet/operation_io.py':Path(__file__).with_name('operation_io.py'),'fleet/startup-supervisor.py':Path(__file__).with_name('startup-supervisor.py')}
    for name,p in replacements.items():shutil.copy2(p,dest/name)
    assert set(replacements).issubset(files), 'replacement absent from baseline manifest'
    manifest={**original,'status':'V14_COMPLETION_UPGRADE_PROPOSAL_NOT_AUTHORIZED','source_commit':commit,'base_manifest_sha256':sha(base/'artifact-hashes.json'),'node_sha256':sha(node),'wasm_sha256':sha(compressed),'compressed_wasm_sha256':sha(compressed),'raw_wasm_sha256':sha(wasm),'genesis_wasm_sha256':original.get('genesis_wasm_sha256',original['wasm_sha256']),'node_profile':'release','reviewed_runtime_transition':[14,15],'existing_chain_preserved':True,'fresh_start_authorized':False,'final_deployment_execution_approved_in_package':False,'public_endpoint_execution_approved_in_package':False,'execution_preparation_complete':True,'scope_note':'Host upgrade preparation only. Old genesis approvals are historical. New upgrade authority and separate signed runtime/commissioning approval required. Never invoke old fresh-start, endpoint, network or accept-continuous commands for this upgrade.','files':{name:sha(dest/name) for name in files}}
    (dest/'artifact-hashes.json').write_text(json.dumps(manifest,sort_keys=True,indent=2)+'\n')
    names=sorted([*files,'artifact-hashes.json']);(dest/'SHA256SUMS').write_text(''.join(sha(dest/n)+'  '+n+'\n' for n in names))
    archive=dest.parent/(dest.name+'.tar.gz')
    with archive.open('xb') as f,gzip.GzipFile(filename='',mode='wb',fileobj=f,mtime=0) as gz,tarfile.open(fileobj=gz,mode='w') as tar:
        for n in sorted([*names,'SHA256SUMS']):
            p=dest/n;info=tarfile.TarInfo('release/'+n);info.size=p.stat().st_size;info.mode=0o755 if n in ['era-node','raw-genesis'] else 0o644;tar.addfile(info,io.BytesIO(p.read_bytes()))
    result={'status':'PROPOSED_NOT_INSTALLED_OR_AUTHORIZED','source_commit':commit,'manifest_sha256':sha(dest/'artifact-hashes.json'),'archive_sha256':sha(archive),'archive_bytes':archive.stat().st_size,'expanded_bytes':sum(p.stat().st_size for p in dest.rglob('*') if p.is_file()),'changed_files':{n:{'before':files[n],'after':sha(dest/n)} for n in replacements},'genesis':manifest['genesis_hash'],'baseline_verified_files':len(files)}
    (dest.parent/'host-release-identity.json').write_text(json.dumps(result,indent=2)+'\n');return result
if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--baseline',type=Path,required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--node',type=Path,required=True);p.add_argument('--wasm',type=Path,required=True);p.add_argument('--compressed',type=Path,required=True);p.add_argument('--commit',required=True);a=p.parse_args();print(json.dumps(build(a.baseline,a.output,a.node,a.wasm,a.compressed,a.commit),indent=2))
