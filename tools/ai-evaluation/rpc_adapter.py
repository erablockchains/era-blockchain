"""Finalized-state reconciliation and explicit disposable-chain submission bridge."""
import hashlib,json,struct,urllib.request,urllib.parse
from evaluation import hex32
PRODUCTION_GENESIS='0x0abc2c3d8db5815541050b73da4d81267ebf14d90dbee8d7258155b667ea112e'
def concat_key(value):return hashlib.blake2b(value,digest_size=16).digest()+value
def revision_key(revision):
    """Pinned EvaluationEvidence second key: Twox64Concat over SCALE u32, seed zero."""
    value=struct.pack('<I',revision);mask=(1<<64)-1
    p1,p2,p3,p5=11400714785074694791,14029467366897019727,1609587929392839161,2870177450012600261
    h=(p5+4)^((revision*p1)&mask)
    h=((((h<<23)|(h>>41))&mask)*p2+p3)&mask
    h^=h>>33;h=(h*p2)&mask;h^=h>>29;h=(h*p3)&mask;h^=h>>32
    return struct.pack('<Q',h)+value
def read_vec(data,pos,limit):
    b=data[pos];mode=b&3
    size=[1,2,4][mode] if mode<3 else 0
    if not size:raise ValueError('unsupported oversized SCALE vector')
    n=int.from_bytes(data[pos:pos+size],'little')>>2;pos+=size
    if n>limit or pos+n>len(data):raise ValueError('invalid SCALE vector')
    return data[pos:pos+n],pos+n
class RpcAdapter:
    def __init__(self,endpoint,expected_genesis,profile,signer=None,transport=None):
        u=urllib.parse.urlsplit(endpoint)
        if u.username or u.password or u.scheme not in ('http','https'):raise ValueError('credential-free HTTP(S) endpoint required')
        self.loopback=u.hostname in ('127.0.0.1','localhost','::1')
        if not self.loopback and u.scheme!='https':raise ValueError('remote read access requires HTTPS')
        hex32(expected_genesis);self.endpoint=endpoint;self.genesis=expected_genesis;self.profile=profile;self.signer=signer;self.transport=transport;self.seq=0
    def rpc(self,method,params):
        self.seq+=1
        if self.transport:return self.transport(method,params)
        body=json.dumps({'jsonrpc':'2.0','id':self.seq,'method':method,'params':params}).encode()
        req=urllib.request.Request(self.endpoint,data=body,headers={'Content-Type':'application/json'})
        with urllib.request.urlopen(req,timeout=10) as r:out=json.loads(r.read(8_000_001))
        if out.get('id')!=self.seq or 'error' in out or 'result' not in out:raise ValueError('RPC response mismatch/error')
        return out['result']
    def context(self):
        if self.rpc('chain_getBlockHash',[0])!=self.genesis:raise ValueError('wrong genesis')
        head=self.rpc('chain_getFinalizedHead',[]);hex32(head)
        v=self.rpc('state_getRuntimeVersion',[head]);m=self.rpc('state_getMetadata',[head])
        if (v['specVersion'],v['transactionVersion'])!=(self.profile['specVersion'],self.profile['transactionVersion']):raise ValueError('unsupported runtime')
        if hashlib.sha256(bytes.fromhex(m.removeprefix('0x'))).hexdigest()!=self.profile['metadataSha256']:raise ValueError('metadata mismatch')
        return head
    def find_finalized(self,model_id,request):
        head=self.context();key=bytes.fromhex(self.profile['requestsPrefix'])+concat_key(struct.pack('<Q',model_id))+concat_key(hex32(request))
        raw=self.rpc('state_getStorage',['0x'+key.hex(),head])
        if raw is None:return None
        b=bytes.fromhex(raw.removeprefix('0x'))
        if len(b)!=8:raise ValueError('invalid prediction ID encoding')
        pid=struct.unpack('<Q',b)[0];key=bytes.fromhex(self.profile['predictionsPrefix'])+concat_key(b)
        raw=self.rpc('state_getStorage',['0x'+key.hex(),head])
        if raw is None:raise ValueError('request points to absent prediction')
        b=bytes.fromhex(raw.removeprefix('0x'))
        if int.from_bytes(b[:8],'little')!=model_id:raise ValueError('model identity mismatch')
        _,pos=read_vec(b,41,96);commitment,_=read_vec(b,pos,128)
        if len(commitment)!=32:raise ValueError('not an evaluation SHA256 commitment')
        return {'prediction_id':pid,'prediction_hash':commitment.hex(),'finalized_block':head}
    def find_evidence_finalized(self,prediction_id,revision):
        head=self.context()
        key=bytes.fromhex(self.profile['evidencePrefix'])+concat_key(struct.pack('<Q',prediction_id))+revision_key(revision)
        raw=self.rpc('state_getStorage',['0x'+key.hex(),head])
        if raw is None:return None
        b=bytes.fromhex(raw.removeprefix('0x'))
        if len(b)!=69 or b[64]>2:raise ValueError('invalid finalized evaluation evidence encoding')
        return {'reviewer':'0x'+b[:32].hex(),'evidence_hash':b[32:64].hex(),'outcome':b[64],'recorded_block':int.from_bytes(b[65:],'little'),'finalized_block':head}
    def submit_call(self,call):
        if not self.loopback or self.genesis==PRODUCTION_GENESIS:raise ValueError('this reference bridge submits only to a disposable local chain')
        if self.signer is None:raise ValueError('explicit disposable signer adapter required; no embedded key')
        head=self.context();signed=self.signer(call,head)
        # Wallet/signer integration owns nonce/mortality/signature verification; this bridge
        # never records key material and does not infer finality from an extrinsic hash.
        return self.rpc('author_submitExtrinsic',[signed])
