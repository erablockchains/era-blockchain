#!/usr/bin/env python3
"""Bounded public-data collector. No chain writes, embedded signer or credentials."""
import argparse, datetime as dt, hashlib, json, time, urllib.parse, urllib.request
from pathlib import Path
from evaluation import instant
BASE = 'https://api.weather.gov/stations/KJFK/observations'
MAX_PAGE = 4_000_000
MAX_TOTAL = 16_000_000


def safe_url(url):
    p = urllib.parse.urlsplit(url)
    if p.scheme != 'https' or p.netloc != 'api.weather.gov' or p.path != '/stations/KJFK/observations' or p.fragment:
        raise ValueError('unexpected observation pagination origin/path')
    return url


class SameOriginRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        safe_url(newurl)
        return super().redirect_request(req, fp, code, msg, headers, newurl)


def collect(start, end, user_agent, fetch=None, max_pages=8):
    lower, upper = instant(start), instant(end)
    if not 0 < upper-lower <= 172800:
        raise ValueError('bounded interval must be <=48 hours')
    if not user_agent.strip() or not 1 <= max_pages <= 8:
        raise ValueError('user agent and bounded page count required')
    opener = urllib.request.build_opener(SameOriginRedirect())
    def network(url):
        req = urllib.request.Request(url, headers={'User-Agent':user_agent, 'Accept':'application/geo+json'})
        with opener.open(req, timeout=15) as r:
            safe_url(r.geturl())
            return r.read(MAX_PAGE+1)
    fetch = fetch or network
    url = BASE+'?'+urllib.parse.urlencode({'start':start,'end':end,'limit':500})
    seen=set(); pages=[]; features=[]; total=0; previous_oldest=None; deadline=time.monotonic()+90
    for _ in range(max_pages):
        safe_url(url)
        if url in seen or time.monotonic() >= deadline:
            raise ValueError('pagination cycle or time bound')
        seen.add(url); raw=fetch(url); total+=len(raw)
        if len(raw)>MAX_PAGE or total>MAX_TOTAL:
            raise ValueError('response exceeds byte bound')
        document=json.loads(raw)
        if document.get('type')!='FeatureCollection' or not isinstance(document.get('features'),list):
            raise ValueError('invalid observation collection')
        rows=document['features']; stamps=[instant(f['properties']['timestamp']) for f in rows]
        if stamps != sorted(stamps,reverse=True):
            raise ValueError('observations must be reverse chronological')
        if previous_oldest is not None and stamps and (max(stamps)>previous_oldest or min(stamps)>=previous_oldest):
            raise ValueError('pagination did not move backwards')
        pages.append({'url':url,'raw':raw,'sha256':hashlib.sha256(raw).hexdigest(),'observations':len(rows)})
        features.extend(f for f,t in zip(rows,stamps) if lower<=t<upper)
        next_url=document.get('pagination',{}).get('next')
        # NWS cursor links can omit original start/end. Follow once past the lower bound;
        # retain only the requested half-open interval, never silently accept an unfinished page.
        if not next_url:
            reason='source pagination exhausted';break
        safe_url(next_url)
        if stamps and min(stamps)<lower:
            reason='reverse chronological pagination crossed requested lower bound';break
        if not stamps:
            raise ValueError('empty nonterminal page')
        previous_oldest=min(stamps);url=next_url
    else:
        raise ValueError('incomplete pagination: page bound exceeded')
    # Keep conflicting observations for the existing quality/conflict filter to reject.
    unique={json.dumps(f,sort_keys=True,separators=(',',':')):f for f in features}
    result={'type':'FeatureCollection','features':list(unique.values()),'era_capture':{'complete_requested_interval':True,'start_inclusive':start,'end_exclusive':end,'terminal_reason':reason,'source_pages':len(pages)}}
    return result,pages


def main():
    p=argparse.ArgumentParser(description=__doc__)
    for name in ['start','end','user-agent','output']:p.add_argument('--'+name,required=True)
    a=p.parse_args();document,pages=collect(a.start,a.end,a.user_agent)
    dest=Path(a.output);paths=[dest,Path(str(dest)+'.capture.json')]+[Path(str(dest)+f'.page-{i:02}.json') for i in range(len(pages))]
    if any(f.exists() for f in paths):raise FileExistsError('capture output already exists')
    raw=(json.dumps(document,sort_keys=True,separators=(',',':'))+'\n').encode()
    manifest={'source':BASE,'captured_utc':dt.datetime.now(dt.timezone.utc).isoformat(),'sha256':hashlib.sha256(raw).hexdigest(),'bytes':len(raw),'complete_requested_interval':True,'origin':'collector execution host; independence not inferred from code','interval':document['era_capture'],'pages':[]}
    for i,page in enumerate(pages):
        f=paths[2+i]
        with f.open('xb') as stream:stream.write(page['raw'])
        manifest['pages'].append({k:v for k,v in page.items() if k!='raw'}|{'file':f.name})
    with dest.open('xb') as stream:stream.write(raw)
    with paths[1].open('x') as stream:json.dump(manifest,stream,indent=2)
    print(json.dumps(manifest))


if __name__=='__main__':main()
