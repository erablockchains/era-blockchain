#!/usr/bin/env python3
"""Nonfinancial evaluation reference operator. No embedded signer or production credentials."""
import argparse, datetime as dt, hashlib, html, json, math, sqlite3, struct
from pathlib import Path
from reference_model import predict
UTC = dt.timezone.utc
TERMS = {
    'schema':'era-station-temperature-v1', 'station':'KJFK', 'unit':'degC',
    'target':'12:00 UTC', 'cutoff':'06:00 UTC on target day, exclusive',
    'window_seconds':1800, 'threshold_c':25,
    'quality':'temperature unit wmoUnit:degC, finite numeric value, qualityControl V only',
    'selection':'closest timestamp to target; earlier timestamp breaks ties; conflicting duplicate timestamps excluded',
    'snapshot':'collector capture at or after target+7200 seconds; arrival by target+93600 seconds',
    'revision':'first accepted capture is original; later corrections append and never overwrite',
    'missing':'no valid sample in window or no capture by grace -> unavailable, never a successful forecast',
    'baseline':'latest valid input observation before cutoff (persistence)',
    'candidate':'untrained arithmetic mean of valid observations during the six hours before cutoff',
    'metrics':['binary accuracy at >=25C','absolute temperature error','baseline accuracy','baseline absolute error','scheduled/submitted/evaluated/unavailable/missed/failed counts'],
    'claim':'evaluated under stated conditions; not universal certification or proof of execution',
}
MODEL = {'schema':'era-reference-model-v1','name':'six-hour-temperature-mean','version':'1','trained':False,'rule':TERMS['candidate'],'runtime':'Python standard library; no paid compute','implementation_sha256':hashlib.sha256(Path(__file__).with_name('reference_model.py').read_bytes()).hexdigest()}
def canonical(v): return json.dumps(v,sort_keys=True,separators=(',',':'),ensure_ascii=True,allow_nan=False)
def digest(v): return hashlib.sha256(canonical(v).encode()).hexdigest()
def instant(v):
    d=dt.datetime.fromisoformat(v.replace('Z','+00:00'))
    if d.tzinfo is None: raise ValueError('timezone required')
    return int(d.timestamp())
def target(day): return instant(day+'T12:00:00Z')
def hex32(v):
    b=bytes.fromhex(v.removeprefix('0x'))
    if len(b)!=32: raise ValueError('32-byte commitment required')
    return b

def valid_observations(document):
    """Strict pinned NWS subset. Other quality states are unavailable, not silently accepted."""
    if document.get('pagination',{}).get('next'):raise ValueError('incomplete observation pagination')
    by_time={}
    for f in document.get('features',[]):
        p=f.get('properties',{}); t=p.get('temperature',{})
        station=p.get('station','').rstrip('/').split('/')[-1]
        value=t.get('value')
        if station!=TERMS['station'] or t.get('unitCode')!='wmoUnit:degC' or t.get('qualityControl')!='V': continue
        if isinstance(value,bool) or not isinstance(value,(int,float)) or not math.isfinite(value): continue
        try: stamp=instant(p['timestamp'])
        except (KeyError,TypeError,ValueError): continue
        by_time.setdefault(stamp,set()).add(float(value))
    return sorted((stamp,next(iter(values))) for stamp,values in by_time.items() if len(values)==1)

def compact(n):
    if n<64:return bytes([n<<2])
    if n<16384:return struct.pack('<H',(n<<2)|1)
    raise ValueError('bounded vector too large')
def vector(b):return compact(len(b))+b
def call_submit(envelope,uri,expires_at):
    b=uri.encode()
    if not 0<len(b)<=256:raise ValueError('metadata URI length')
    return '0x'+(bytes([12,41])+struct.pack('<Q',envelope['model_id'])+hex32(envelope['request_id'])+hex32(envelope['artifact_sha256'])+hex32(digest(envelope))+vector(b)+bytes([0])+struct.pack('<IQQ',expires_at,envelope['cutoff_utc'],envelope['evidence_after_utc'])).hex()
def call_evidence(prediction_id,revision,evidence):
    outcome=2 if evidence['status']!='evaluated' else (0 if evidence['correct'] else 1)
    return '0x'+(bytes([12,42])+struct.pack('<QI',prediction_id,revision)+hex32(digest(evidence))+bytes([outcome])).hex()

class Journal:
    def __init__(self,path):
        self.db=sqlite3.connect(path,timeout=10)
        self.db.execute('PRAGMA journal_mode=WAL');self.db.execute('PRAGMA synchronous=FULL')
        self.db.executescript('''CREATE TABLE IF NOT EXISTS jobs(request TEXT PRIMARY KEY,model INTEGER,day TEXT,envelope TEXT,call TEXT,prediction INTEGER,state TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS audit(seq INTEGER PRIMARY KEY AUTOINCREMENT,request TEXT NOT NULL,kind TEXT NOT NULL,payload TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS evaluations(request TEXT,revision INTEGER,evidence TEXT NOT NULL,PRIMARY KEY(request,revision));
        CREATE TABLE IF NOT EXISTS confirmations(request TEXT,revision INTEGER,receipt TEXT NOT NULL,PRIMARY KEY(request,revision));''')
    def schedule(self,model,day,now):
        if now>=target(day)-21600:raise ValueError('schedule must precede cutoff; do not backfill performance schedules')
        key=digest({'service':TERMS['schema'],'model_id':model,'day':day})
        with self.db:
            self.db.execute('INSERT OR IGNORE INTO jobs VALUES(?,?,?,NULL,NULL,NULL,?)',(key,model,day,'scheduled'))
        return key
    def event(self,key,kind,payload):self.db.execute('INSERT INTO audit(request,kind,payload) VALUES(?,?,?)',(key,kind,canonical(payload)))
    def prepare(self,key,document,now,uri,expires_at,fixture=False):
        row=self.db.execute('SELECT model,day,envelope FROM jobs WHERE request=?',(key,)).fetchone()
        if row is None:raise ValueError('schedule not found')
        if row[2]:return json.loads(row[2])
        model,day,_=row; t=target(day);cutoff=t-21600
        if now>=cutoff:
            with self.db:self.event(key,'missed_submission',{'at':now});self.db.execute("UPDATE jobs SET state='missed' WHERE request=?",(key,))
            raise ValueError('submission cutoff passed')
        values=[(stamp,value) for stamp,value in valid_observations(document) if cutoff-21600<=stamp<min(cutoff,now+1)]
        if not values:
            with self.db:self.event(key,'inference_failed',{'reason':'no valid pre-cutoff observations','at':now})
            raise ValueError('no valid model input')
        forecast=predict(values); baseline=values[-1][1]
        envelope={'schema':TERMS['schema'],'request_id':key,'model_id':model,'artifact_sha256':digest(MODEL),'model_version':MODEL['version'],'input_sha256':digest(document),'terms_sha256':digest(TERMS),'target_utc':t,'cutoff_utc':cutoff,'evidence_after_utc':t+7200,'forecast_c':forecast,'baseline_c':baseline,'threshold_c':25,'generated_utc':now,'fixture':fixture,'trained_model':False}
        call=call_submit(envelope,uri,expires_at)
        with self.db:
            # Claim the job atomically; a second process cannot overwrite a prepared commitment.
            changed=self.db.execute("UPDATE jobs SET envelope=?,call=?,state='prepared' WHERE request=? AND envelope IS NULL",(canonical(envelope),call,key)).rowcount
            if changed:self.event(key,'prepared',{'envelope_sha256':digest(envelope),'at':now})
        return json.loads(self.db.execute('SELECT envelope FROM jobs WHERE request=?',(key,)).fetchone()[0])
    def reconcile(self,key,chain):
        row=self.db.execute('SELECT model,envelope,call,prediction,state FROM jobs WHERE request=?',(key,)).fetchone()
        if not row or not row[1]:raise ValueError('request not prepared')
        # Adapter must query the FINALIZED request map and validate genesis/metadata first.
        found=chain.find_finalized(row[0],key)
        if found is not None:
            if found['prediction_hash'] != digest(json.loads(row[1])):
                raise ValueError('finalized request conflicts with local immutable commitment')
            found=found['prediction_id']
            with self.db:
                self.db.execute("UPDATE jobs SET prediction=?,state='finalized' WHERE request=?",(found,key))
                self.event(key,'finalized',{'prediction_id':found})
            return found
        return None
    def submit(self,key,chain,now):
        found=self.reconcile(key,chain)
        if found is not None:return found
        envelope,call=self.db.execute('SELECT envelope,call FROM jobs WHERE request=?',(key,)).fetchone();e=json.loads(envelope)
        if now>=e['cutoff_utc']:
            with self.db:self.event(key,'missed_submission',{'at':now});self.db.execute("UPDATE jobs SET state='missed' WHERE request=?",(key,))
            raise ValueError('cutoff passed without finalized evidence; reconcile in-flight request later')
        # Persist intent before any external side effect. Crash/unknown outcome is reconciled,
        # never treated as failure. Runtime request map is the final duplicate guard.
        with self.db:self.event(key,'submission_attempt',{'at':now});self.db.execute("UPDATE jobs SET state='pending' WHERE request=?",(key,))
        chain.submit_call(call)
        return self.reconcile(key,chain)
    def score(self,key,document,captured_utc,reviewer,submitter,correction_reason=None):
        if reviewer==submitter or not reviewer:raise ValueError('independent reviewer identity required')
        row=self.db.execute('SELECT envelope,prediction FROM jobs WHERE request=?',(key,)).fetchone()
        if not row or row[1] is None:raise ValueError('only finalized predictions may be scored')
        e=json.loads(row[0]);t=e['target_utc']
        if captured_utc<t+7200:raise ValueError('outcome collection before snapshot time')
        candidates=[(stamp,v) for stamp,v in valid_observations(document) if abs(stamp-t)<=1800]
        available=bool(candidates) and captured_utc<=t+93600
        if not available and captured_utc<t+93600:raise ValueError('wait until evidence grace before recording unavailable')
        rev=self.db.execute('SELECT COUNT(*) FROM evaluations WHERE request=?',(key,)).fetchone()[0]
        if rev and not correction_reason:raise ValueError('correction must give reason; original remains preserved')
        if rev>=32:raise ValueError('runtime evidence revision limit reached; retain further evidence off-chain')
        evidence={'schema':'era-evaluation-result-v1','request_id':key,'prediction_id':row[1],'revision':rev,'envelope_sha256':digest(e),'source_document_sha256':digest(document),'captured_utc':captured_utc,'reviewer':reviewer,'status':'evaluated' if available else 'unavailable','correction_reason':correction_reason,'fixture':e['fixture'],'claim':TERMS['claim']}
        if available:
            stamp,value=min(candidates,key=lambda p:(abs(p[0]-t),p[0]))
            evidence.update(observation_utc=stamp,actual_c=value,correct=(e['forecast_c']>=25)==(value>=25),absolute_error_c=abs(e['forecast_c']-value),baseline_correct=(e['baseline_c']>=25)==(value>=25),baseline_absolute_error_c=abs(e['baseline_c']-value))
        with self.db:
            self.db.execute('INSERT INTO evaluations VALUES(?,?,?)',(key,rev,canonical(evidence)))
            self.event(key,'evidence_prepared',{'revision':rev,'sha256':digest(evidence),'call':call_evidence(row[1],rev,evidence)})
        return evidence
    def reconcile_evidence(self,key,chain):
        self.reconcile(key,chain)
        confirmed=0
        for revision,raw in self.db.execute('SELECT revision,evidence FROM evaluations WHERE request=? ORDER BY revision',(key,)).fetchall():
            evidence=json.loads(raw)
            found=chain.find_evidence_finalized(evidence['prediction_id'],revision)
            if found is None:continue
            expected=2 if evidence['status']!='evaluated' else (0 if evidence['correct'] else 1)
            if found['evidence_hash']!=digest(evidence) or found['outcome']!=expected or found['reviewer'].lower()!=evidence['reviewer'].lower():
                raise ValueError('finalized evidence differs from the prepared hash, reviewer or outcome')
            with self.db:
                changed=self.db.execute('INSERT OR IGNORE INTO confirmations VALUES(?,?,?)',(key,revision,canonical(found))).rowcount
                if changed:self.event(key,'evidence_finalized',{'revision':revision,**found})
            confirmed+=1
        return confirmed
    def export(self,key):
        row=self.db.execute('SELECT envelope,call FROM jobs WHERE request=?',(key,)).fetchone()
        if not row or not row[0]:raise ValueError('request not prepared')
        return {'request':key,'committed_envelope_utf8':row[0],'envelope_sha256':hashlib.sha256(row[0].encode()).hexdigest(),'submission_call':row[1],'evidence':[{'revision':rev,'committed_evidence_utf8':raw,'evidence_sha256':hashlib.sha256(raw.encode()).hexdigest(),'call':call_evidence(json.loads(raw)['prediction_id'],rev,json.loads(raw))} for rev,raw in self.db.execute('SELECT revision,evidence FROM evaluations WHERE request=? ORDER BY revision',(key,))]}

    def expire(self,now):
        with self.db:
            for key,day,state in self.db.execute("SELECT request,day,state FROM jobs WHERE prediction IS NULL").fetchall():
                if now<target(day)-21600 or state in ('missed','past_cutoff_unconfirmed'):continue
                kind='past_cutoff_unconfirmed' if state=='pending' else 'missed'
                self.db.execute('UPDATE jobs SET state=? WHERE request=?',(kind,key))
                self.event(key,kind,{'at':now,'note':'reconcile in-flight requests; do not infer absence from a timeout'})

    def history(self):
        jobs=[dict(zip(['request','model','day','state','prediction'],r)) for r in self.db.execute('SELECT request,model,day,state,prediction FROM jobs ORDER BY day,request')]
        results=[json.loads(r[0]) for r in self.db.execute('SELECT evidence FROM evaluations ORDER BY request,revision')]
        first={}
        for result in results:first.setdefault(result['request_id'],result)
        summary={}
        for label,fixture in [('synthetic_fixture',True),('reference_model_real_data',False)]:
            initial=[r for r in first.values() if r['fixture']==fixture];scored=[r for r in initial if r['status']=='evaluated']
            summary[label]={'original_evaluated':len(scored),'original_unavailable':len(initial)-len(scored),'correct':sum(r['correct'] for r in scored),'baseline_correct':sum(r['baseline_correct'] for r in scored),'mean_absolute_error_c':sum(r['absolute_error_c'] for r in scored)/len(scored) if scored else None,'baseline_mean_absolute_error_c':sum(r['baseline_absolute_error_c'] for r in scored)/len(scored) if scored else None}
        receipts={(key,rev):json.loads(raw) for key,rev,raw in self.db.execute('SELECT request,revision,receipt FROM confirmations')}
        validated=[r for r in results if (r['request_id'],r['revision']) in receipts]
        original=[r for r in validated if r['revision']==0]
        latest={}
        for r in validated:latest[r['request_id']]=r
        def metrics(rows):
            out={}
            for label,fixture in [('synthetic_fixture',True),('reference_model_real_data',False)]:
                all_rows=[r for r in rows if r['fixture']==fixture];scored=[r for r in all_rows if r['status']=='evaluated']
                out[label]={'evaluated':len(scored),'unavailable':len(all_rows)-len(scored),'correct':sum(r['correct'] for r in scored),'baseline_correct':sum(r['baseline_correct'] for r in scored),'mean_absolute_error_c':sum(r['absolute_error_c'] for r in scored)/len(scored) if scored else None,'baseline_mean_absolute_error_c':sum(r['baseline_absolute_error_c'] for r in scored)/len(scored) if scored else None}
            return out
        return {'claim':TERMS['claim'],'jobs':jobs,'finalized_original_metrics':metrics(original),'finalized_latest_revision_metrics':metrics(latest.values()),'finalized_receipts':[{'request':key,'revision':rev,**receipt} for (key,rev),receipt in receipts.items()],'local_prepared_metrics_not_finalized_evidence':summary,'coverage':{'scheduled':len(jobs),'finalized_submissions':sum(j['prediction'] is not None for j in jobs),'missed':sum(j['state']=='missed' for j in jobs),'past_cutoff_unconfirmed':sum(j['state']=='past_cutoff_unconfirmed' for j in jobs)},'evidence_revisions':results,'audit':[dict(zip(['seq','request','kind','payload'],r)) for r in self.db.execute('SELECT * FROM audit ORDER BY seq')],'warning':'Evidence prepared locally is not on-chain validation. Finalized metrics include only hash/reviewer/outcome-matched receipts; prepared evidence remains separate. Synthetic results are not model-performance claims.'}
    def html(self):
        data=self.history()
        rows=''.join('<tr>'+''.join('<td>'+html.escape(str(v))+'</td>' for v in r.values())+'</tr>' for r in data['jobs'])
        return '<!doctype html><meta charset="utf-8"><title>ERA model evaluation</title><h1>ERA model evaluation</h1><p>'+html.escape(data['claim'])+'</p><p>'+html.escape(data['warning'])+'</p><table><tr><th>Request</th><th>Model</th><th>Day</th><th>Status</th><th>Prediction</th></tr>'+rows+'</table><h2>Evidence and correction history</h2><pre>'+html.escape(json.dumps(data,indent=2))+'</pre>'

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--journal',required=True)
    sub=p.add_subparsers(dest='command',required=True)
    q=sub.add_parser('schedule');q.add_argument('--model-id',type=int,required=True);q.add_argument('--day',required=True)
    q=sub.add_parser('prepare');q.add_argument('--request',required=True);q.add_argument('--input',required=True);q.add_argument('--uri',required=True);q.add_argument('--expires-block',type=int,required=True);q.add_argument('--fixture',action='store_true')
    q=sub.add_parser('score');q.add_argument('--request',required=True);q.add_argument('--input',required=True);q.add_argument('--captured-utc',required=True);q.add_argument('--reviewer',required=True);q.add_argument('--submitter',required=True);q.add_argument('--correction-reason')
    q=sub.add_parser('export');q.add_argument('--request',required=True)
    q=sub.add_parser('reconcile');q.add_argument('--request',required=True);q.add_argument('--rpc',required=True);q.add_argument('--genesis',required=True);q.add_argument('--profile',required=True)
    sub.add_parser('history');sub.add_parser('html');sub.add_parser('terms');sub.add_parser('expire')
    a=p.parse_args();j=Journal(a.journal);now=int(dt.datetime.now(UTC).timestamp())
    if a.command=='schedule':value=j.schedule(a.model_id,a.day,now)
    elif a.command=='prepare':value=j.prepare(a.request,json.loads(Path(a.input).read_text()),now,a.uri,a.expires_block,a.fixture)
    elif a.command=='score':value=j.score(a.request,json.loads(Path(a.input).read_text()),instant(a.captured_utc),a.reviewer,a.submitter,a.correction_reason)
    elif a.command=='export':value=j.export(a.request)
    elif a.command=='reconcile':
        from rpc_adapter import RpcAdapter
        chain=RpcAdapter(a.rpc,a.genesis,json.loads(Path(a.profile).read_text()));value={'finalized_prediction':j.reconcile(a.request,chain),'confirmed_evidence_revisions':j.reconcile_evidence(a.request,chain)}
    elif a.command=='html':print(j.html());return
    elif a.command=='terms':value={'terms':TERMS,'model':MODEL,'artifact_sha256':digest(MODEL)}
    elif a.command=='expire':j.expire(now);value=j.history()
    else:value=j.history()
    print(json.dumps(value,indent=2))
if __name__=='__main__':main()
