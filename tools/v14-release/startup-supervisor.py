#!/usr/bin/env python3
"""Bounded commissioning followed by one-time accepted, non-expiring continuous mode."""
import argparse,hashlib,json,os,pathlib,signal,socket,subprocess,sys,tempfile,time,re
from capacity_guard import evaluate as startup_evaluate,sample
from continuous_policy import evaluate,validate_acceptance
from operation_io import atomic_json,read_json,verify_release,canonical
from local_health import check as probe_local_health
if not __debug__:raise RuntimeError('Optimized Python forbidden')

def context_for(h,receipt,digest,receipt_sha):
 return dict(host=h['host'],provider_paths_reviewed=h['provider_controls_confirmed'],production_genesis_hash=h['production_genesis_hash'],release_manifest_sha256=digest,startup_receipt_sha256=receipt_sha)
def advance(h,receipt,profile,acceptance,state,rows,now,context):
 if state['startup_receipt_sha256']!=context['startup_receipt_sha256'] or state['host']!=h['host']:raise ValueError('State binding differs')
 if state['mode'] not in ['STARTUP','CONTINUOUS']:raise ValueError('Invalid mode')
 if receipt['deadline_utc_epoch']-receipt['started_utc_epoch']!=7200:raise ValueError('Commissioning receipt altered')
 state=dict(state);record=dict(mode=state['mode'],warnings=[])
 if acceptance is not None:
  validate_acceptance(profile,acceptance,context)
  if state['mode']=='CONTINUOUS' and state['acceptance_sha256']!=canonical(acceptance):raise ValueError('One-time acceptance changed')
  state['mode']='CONTINUOUS';state['acceptance_sha256']=canonical(acceptance)
 if state['mode']=='CONTINUOUS':
  if acceptance is None:raise ValueError('Retained continuous acceptance missing')
  result=evaluate(profile,rows);record.update(result,mode='CONTINUOUS')
  if result['storage_warning']:record['warnings'].append('Usable storage headroom below warning threshold')
  if result['storage_stop']:state['storage_paused']=True;state['resume_since_epoch']=None
  if state.get('storage_paused',False):
   if result['can_resume']:
    since=state.get('resume_since_epoch')
    if since is None or now<since:since=now
    state['resume_since_epoch']=since
    if now-since>=profile['resume_stable_seconds']:state['storage_paused']=False;state['resume_since_epoch']=None
   else:state['resume_since_epoch']=None
  record['action']='PAUSE' if state.get('storage_paused',False) else 'RUN'
 else:
  if now<state['last_checked_epoch']:raise ValueError('Clock rollback during commissioning')
  if now>=receipt['deadline_utc_epoch']:raise ValueError('Commissioning ended without acceptance; no deadline reset')
  if len(rows)!=3:raise ValueError('Missing filesystem samples')
  for key,initial,available in [('high_water_bytes','initial_available_bytes','available_bytes'),('high_water_inodes','initial_free_inodes','free_inodes')]:
   if type(state[key]) is not int or state[key]<0:raise ValueError('Invalid commissioning state')
   state[key]=max(state[key],receipt[initial]-min(r[available] for r in rows),0)
  result=startup_evaluate(h,rows,state['high_water_bytes'],state['high_water_inodes'])
  record.update(action='RUN',commissioning_deadline_epoch=receipt['deadline_utc_epoch'],remaining_commissioning_bytes=result['remaining_startup_bytes'],remaining_commissioning_inodes=result['remaining_startup_inodes'])
  if receipt['deadline_utc_epoch']-now<=1800:record['warnings'].append('Commissioning expires within 30 minutes; complete one-time acceptance or allow stop')
 state['last_checked_epoch']=now
 return state,record

def notify(message):
 address=os.environ.get('NOTIFY_SOCKET')
 if not address:return
 if address.startswith('@'):address='\0'+address[1:]
 with socket.socket(socket.AF_UNIX,socket.SOCK_DGRAM) as sock:sock.connect(address);sock.sendall(message.encode())
def stop_child(child,killpg=os.killpg):
 if child is not None and child.poll() is None:
  killpg(child.pid,signal.SIGTERM)
  try:child.wait(timeout=20)
  except subprocess.TimeoutExpired:killpg(child.pid,signal.SIGKILL);child.wait(timeout=5)
def metrics(record,path):
 # Fixed public gauges only. No labels derived from user strings or private data.
 values=dict(era_v14_monitor_checked_epoch=record['checked_epoch'],era_v14_node_running=int(record.get('status')=='RUNNING'),era_v14_continuous=int(record.get('mode')=='CONTINUOUS'),era_v14_storage_paused=int(record.get('status')=='STORAGE_PAUSED'),era_v14_warning=int(bool(record.get('warnings'))))
 values['era_v14_rpc_ok']=int(record.get('rpc_health',{}).get('rpc_ok',False))
 values['era_v14_warning_history_ok']=int(record.get('local_warning_recording_verified',False))
 if record.get('rpc_health',{}).get('last_advance_epoch') is not None:values['era_v14_finality_last_advance_epoch']=record['rpc_health']['last_advance_epoch']
 for key in ['usable_headroom_bytes','usable_headroom_inodes','warning_headroom_bytes','warning_headroom_inodes','stop_headroom_bytes','stop_headroom_inodes']:
  if key in record:values['era_v14_'+key]=record[key]
 data=''.join(f'{name} {value}\n' for name,value in values.items())
 if record.get('manifest_sha256'):
  if not re.fullmatch(r'[0-9a-f]{64}',record['manifest_sha256']) or not re.fullmatch(r'0x[0-9a-f]{64}',record['production_genesis_hash']) or not re.fullmatch(r'era-(val|sentry|rpc)-0[1-4]',record['host']):raise ValueError('Invalid public metric identity')
  data+='era_v14_identity_info{host="'+record['host']+'",manifest="'+record['manifest_sha256']+'",genesis="'+record['production_genesis_hash']+'"} 1\n'
 path=pathlib.Path(path)
 if path.is_symlink():raise ValueError('Metrics path symlink')
 fd,temp=tempfile.mkstemp(prefix='.era-v14-',dir=path.parent)
 try:
  with os.fdopen(fd,'w') as f:os.fchmod(f.fileno(),0o644);f.write(data);f.flush();os.fsync(f.fileno())
  os.replace(temp,path)
 finally:
  if os.path.exists(temp):os.unlink(temp)
def supervise(command,h,receipt,profile,state,context,read_acceptance,persist,health,sampler=sample,clock=time.time,sleep=time.sleep,popen=subprocess.Popen,notifier=notify,killpg=os.killpg,probe=probe_local_health):
 child=None;last_notice=None;ready=False;probe_state={};child_started=0
 try:
  while True:
   try:acceptance=read_acceptance()
   except FileNotFoundError:acceptance=None
   state,record=advance(h,receipt,profile,acceptance,state,sampler(h),int(clock()),context);persist(state)
   if record['action']=='PAUSE':stop_child(child,killpg);child=None
   elif child is None:child=popen(command,start_new_session=True);child_started=int(clock());probe_state={}
   elif child.poll() is not None:raise RuntimeError('ERA child exited; systemd bounded restart applies')
   record.update(host=h['host'],status='STORAGE_PAUSED' if record['action']=='PAUSE' else 'RUNNING',checked_epoch=state['last_checked_epoch'],production_genesis_hash=context['production_genesis_hash'],manifest_sha256=context['release_manifest_sha256'],startup_receipt_sha256=context['startup_receipt_sha256'],acceptance_sha256=state.get('acceptance_sha256'))
   if child is not None:
    try:probe_state=probe(context['production_genesis_hash'],probe_state,int(clock()))
    except RuntimeError as error:raise ValueError(str(error)) from error
    record['rpc_health']=probe_state
    if int(clock())-child_started>=60 and not probe_state.get('rpc_ok'):record['warnings'].append('Local safe RPC check failing')
    if probe_state.get('finality_stalled'):record['warnings'].append('Local finalized height has not advanced for 180 seconds')
   health(record)
   notice=(record['status'],record['mode'],tuple(record['warnings']))
   if notice!=last_notice:print(json.dumps(record),flush=True);last_notice=notice
   if not ready:notifier('READY=1');ready=True
   notifier('WATCHDOG=1\nSTATUS='+record['status']+' '+record['mode']);sleep(5)
 except BaseException as error:
  try:health(dict(host=h['host'],status='STOPPED',reason=str(error),checked_epoch=int(clock())))
  except BaseException:pass
  print('ERA supervisor stopped: '+str(error),flush=True)
  if isinstance(error,KeyboardInterrupt):return 0
  return 1 if isinstance(error,RuntimeError) else 78
 finally:stop_child(child,killpg)
def main():
 parser=argparse.ArgumentParser();parser.add_argument('--host',required=True);parser.add_argument('--release',type=pathlib.Path,required=True);parser.add_argument('command',nargs=argparse.REMAINDER);args=parser.parse_args()
 if args.release!=pathlib.Path('/opt/era/releases/v14-fresh-20260914') or socket.gethostname()!=args.host:raise ValueError('Host/release mismatch')
 manifest,digest=verify_release(args.release,operation="rolling_reviewed_host_restart")
 h=next(x for x in read_json(args.release/'fleet/fleet-inputs.confirmed.json')['hosts'] if x['host']==args.host);h['production_genesis_hash']=manifest['genesis_hash']
 parent=pathlib.Path(h['new_base']).parent;receipt_path=parent/'STARTUP-BUDGET.json';receipt=read_json(receipt_path)
 if receipt['host']!=args.host or receipt['manifest_sha256']!=digest:raise ValueError('Receipt mismatch')
 context=context_for(h,receipt,digest,hashlib.sha256(receipt_path.read_bytes()).hexdigest());state=read_json(parent/'OPERATING-STATE.json')
 # Recheck recovery headroom after every service/process restart, without resetting authority or usage.
 if state['mode']=='CONTINUOUS':state['storage_paused']=True;state['resume_since_epoch']=None
 profile=next(x for x in read_json(args.release/'operating/continuous-policy.REVIEWED.json') if x['host']==args.host)
 command=args.command[1:] if args.command[:1]==['--'] else args.command
 if not command or command[0]!=str(args.release/'era-node'):raise ValueError('Node command mismatch')
 for sig in [signal.SIGTERM,signal.SIGINT]:signal.signal(sig,lambda *_:(_ for _ in ()).throw(KeyboardInterrupt('Requested stop')))
 warning_path=parent/'WARNINGS.json'
 warning_history=read_json(warning_path) if warning_path.exists() else []
 if not isinstance(warning_history,list) or len(warning_history)>100:raise ValueError('Invalid local warning history; preserve it')
 last_record_key=None
 def publish(record):
  nonlocal last_record_key
  key=(record['status'],record.get('mode'),tuple(record.get('warnings',[])),record.get('reason'))
  if key!=last_record_key:
   warning_history.append({k:record[k] for k in ['checked_epoch','status','mode','warnings','reason'] if k in record})
   del warning_history[:-100]
   atomic_json(warning_path,warning_history,0o644);last_record_key=key
  if not warning_path.is_file() or warning_path.is_symlink():raise ValueError('Local warning record missing/unsafe')
  record['local_warning_recording_verified']=True
  atomic_json(parent/'OPERATING-HEALTH.json',record,0o644);metrics(record,'/var/lib/era-v14-monitor/era-v14.prom')
 return supervise(command,h,receipt,profile,state,context,lambda:read_json(pathlib.Path('/etc/era-v14/continuous')/args.host/'acceptance.json',root_owned=True),lambda x:atomic_json(parent/'OPERATING-STATE.json',x),publish)
if __name__=='__main__':
 try:sys.exit(main())
 except Exception as error:print('ERA preflight refused: '+str(error),flush=True);sys.exit(78)
