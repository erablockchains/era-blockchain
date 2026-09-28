"""Final deployment authorization is external to the sealed preparation package."""
import json, os, pathlib, re, socket, stat

HOSTS = ['era-val-01', 'era-val-02', 'era-val-03', 'era-val-04',
         'era-sentry-01', 'era-sentry-02', 'era-rpc-01', 'era-rpc-02', 'era-rpc-03']
SCOPE = ['stage_reviewed_upgrade_release', 'replace_reviewed_node_preserve_chain',
         'rebind_existing_continuous_records', 'rolling_reviewed_host_restart']

def validate(record, host, manifest_sha256, archive_sha256=None):
    if host not in HOSTS or record.get('host') != host:
        raise ValueError('Deployment authorization host mismatch')
    if record.get('schema') != 'ERA_UPGRADE_HOST_AUTHORIZATION_V1' or record.get('deployment_authorized') is not True:
        raise ValueError('Final deployment authorization absent')
    if not re.fullmatch('[0-9a-f]{64}', manifest_sha256) or record.get('manifest_sha256') != manifest_sha256:
        raise ValueError('Deployment authorization manifest mismatch')
    if not re.fullmatch('[0-9a-f]{64}', record.get('archive_sha256', '')):
        raise ValueError('Deployment authorization archive missing')
    if archive_sha256 is not None and record['archive_sha256'] != archive_sha256:
        raise ValueError('Deployment authorization archive mismatch')
    if record.get('scope') != SCOPE or not record.get('owner_authorization_reference'):
        raise ValueError('Deployment authorization scope/reference mismatch')
    for k in ['public_endpoint_authorized', 'deletion_authorized', 'external_notifications_authorized', 'fresh_chain_authorized', 'network_change_authorized', 'production_transactions_authorized']:
        if record.get(k) is not False:
            raise ValueError('Authorization exceeds deployment scope: ' + k)
    if 'expires_at' in record or 'expires_epoch' in record:
        raise ValueError('No recurring deployment grant')
    return record

def verify(host, manifest_sha256, archive_sha256=None, *, operation=None):
    if operation not in SCOPE:
        raise ValueError("Explicit reviewed upgrade operation required; legacy fresh/network/endpoint entry points refused")
    if socket.gethostname() != host:
        raise ValueError('Deployment authorization local host mismatch')
    path = pathlib.Path('/etc/era-v14/deployment') / (host + '.json')
    for q in [path, *path.parents]:
        s = q.lstat()
        if stat.S_ISLNK(s.st_mode) or s.st_uid != 0 or s.st_mode & 0o022:
            raise ValueError('Untrusted deployment authorization path: ' + str(q))
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW)
    with os.fdopen(fd) as f:
        if os.fstat(f.fileno()).st_size > 16384:
            raise ValueError('Deployment authorization record too large')
        record = json.load(f)
    return validate(record, host, manifest_sha256, archive_sha256)
