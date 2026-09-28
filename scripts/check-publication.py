#!/usr/bin/env python3
"""Offline source/publication bindings; no network, signing, or production operations."""
import hashlib,json,pathlib,sys,tomllib
root=pathlib.Path(__file__).resolve().parents[1]
def check_manifest(path,base):
 count=0
 for line in path.read_text().splitlines():
  digest,name=line.split('  ',1);p=pathlib.PurePosixPath(name)
  assert not p.is_absolute() and '..' not in p.parts
  f=base/p;assert f.is_file() and not f.is_symlink(),name
  assert hashlib.sha256(f.read_bytes()).hexdigest()==digest,name;count+=1
 return count
source=check_manifest(root/'provenance/SOURCE-SHA256SUMS',root)
licenses=check_manifest(root/'licenses/SHA256SUMS',root/'licenses')
prov=json.loads((root/'provenance/release.json').read_text())
assert hashlib.sha256((root/'network/era-v14.raw.json').read_bytes()).hexdigest()==prov['raw_spec_sha256']
assert hashlib.sha256((root/'network/metadata-spec14.scale').read_bytes()).hexdigest()=='eed011f659bd492aedb643d775fce7cab26adb89c46db8598f4b2c079897e5d2'
raw=json.loads((root/'network/era-v14.raw.json').read_text());assert raw['name']=='ERA' and raw['properties']['tokenSymbol']=='ETKN'
assert hashlib.sha256(bytes.fromhex(raw['genesis']['raw']['top']['0x3a636f6465'][2:])).hexdigest()==prov['compressed_wasm_sha256']
ws=tomllib.loads((root/'Cargo.toml').read_text())
for m in ws['workspace']['members']:assert (root/m/'Cargo.toml').is_file(),m
for p in root.rglob('Cargo.toml'):
 if 'target' in p.parts or '.git' in p.parts:continue
 tomllib.loads(p.read_text())
assert all(not p.is_symlink() for p in root.rglob('*') if '.git' not in p.parts)
print(json.dumps(dict(status='PUBLICATION_BINDINGS_PASS',source_files=source,license_texts=licenses,workspace_members=len(ws['workspace']['members']),new_build_performed=False)))
