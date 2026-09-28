# Offline parser reused from the retained V14 runtime review; never performs RPC.
def compact(n):
    if n < 64:
        return bytes([n << 2])
    if n < 16384:
        return (n << 2 | 1).to_bytes(2, 'little')
    if n < 1 << 30:
        return (n << 2 | 2).to_bytes(4, 'little')
    raise ValueError('fixture compact bound')

def take_compact(b, i=0):
    mode = b[i] & 3
    if mode == 0:
        return (b[i] >> 2, i + 1)
    if mode == 1:
        return (int.from_bytes(b[i:i + 2], 'little') >> 2, i + 2)
    if mode == 2:
        return (int.from_bytes(b[i:i + 4], 'little') >> 2, i + 4)
    raise ValueError('unexpected long compact')

class Reader:

    def __init__(self, b):
        self.b = b
        self.p = 0

    def take(self, n):
        assert 0 <= n <= len(self.b) - self.p
        b = self.b[self.p:self.p + n]
        self.p += n
        return b

    def u8(self):
        return self.take(1)[0]

    def ci(self):
        n, p = take_compact(self.b, self.p)
        self.p = p
        return n

    def vec(self, f):
        n = self.ci()
        assert n <= 100000
        return [f() for _ in range(n)]

    def blob(self):
        return self.take(self.ci())

    def text(self):
        return self.blob().decode()

    def opt(self, f):
        tag = self.u8()
        assert tag in (0, 1)
        return f() if tag else None

    def docs(self):
        return self.vec(self.text)

    def field(self):
        return dict(name=self.opt(self.text), type=self.ci(), type_name=self.opt(self.text), docs=self.docs())

    def variant(self):
        return dict(name=self.text(), fields=self.vec(self.field), index=self.u8(), docs=self.docs())

    def typ(self):
        i = self.ci()
        path = self.vec(self.text)
        params = self.vec(lambda: dict(name=self.text(), type=self.opt(self.ci)))
        kind = self.u8()
        if kind == 0:
            body = self.vec(self.field)
        elif kind == 1:
            body = self.vec(self.variant)
        elif kind in (2, 6):
            body = self.ci()
        elif kind == 3:
            body = dict(length=int.from_bytes(self.take(4), 'little'), type=self.ci())
        elif kind == 4:
            body = self.vec(self.ci)
        elif kind == 5:
            body = self.u8()
        elif kind == 7:
            body = dict(store=self.ci(), order=self.ci())
        else:
            raise ValueError(kind)
        return dict(id=i, path=path, params=params, kind=kind, body=body, docs=self.docs())

    def entry(self):
        name = self.text()
        modifier = self.u8()
        kind = self.u8()
        if kind == 0:
            typ = dict(kind='plain', value=self.ci())
        elif kind == 1:
            typ = dict(kind='map', hashers=self.vec(self.u8), key=self.ci(), value=self.ci())
        else:
            raise ValueError(kind)
        return dict(name=name, modifier=modifier, type=typ, default='0x' + self.blob().hex(), docs=self.docs())

    def storage(self):
        return dict(prefix=self.text(), entries=self.vec(self.entry))

    def constant(self):
        return dict(name=self.text(), type=self.ci(), value='0x' + self.blob().hex(), docs=self.docs())

    def pallet(self):
        p = dict(name=self.text(), storage=self.opt(self.storage), calls=self.opt(self.ci), event=self.opt(self.ci), constants=self.vec(self.constant), error=self.opt(self.ci), index=self.u8())
        if self.version >= 15:
            p['docs'] = self.docs()
        return p

    def decode(self, tid, types, depth=0):
        assert depth < 40
        t = types[tid]
        k = t['kind']
        b = t['body']
        d = lambda i: self.decode(i, types, depth + 1)
        if k == 0:
            vals = [d(f['type']) for f in b]
            return dict(zip([f['name'] for f in b], vals)) if all((f['name'] is not None for f in b)) else vals
        if k == 1:
            index = self.u8()
            v = next((v for v in b if v['index'] == index))
            vals = [d(f['type']) for f in v['fields']]
            return dict(variant=v['name'], fields=dict(zip([f['name'] for f in v['fields']], vals)) if all((f['name'] is not None for f in v['fields'])) else vals)
        if k == 2:
            return self.vec(lambda: d(b))
        if k == 3:
            if types[b['type']]['kind'] == 5 and types[b['type']]['body'] == 3:
                return '0x' + self.take(b['length']).hex()
            return [d(b['type']) for _ in range(b['length'])]
        if k == 4:
            return [d(i) for i in b]
        if k == 5:
            if b == 0:
                v = self.u8()
                assert v in (0, 1)
                return bool(v)
            if b == 1:
                return chr(int.from_bytes(self.take(4), 'little'))
            if b == 2:
                return self.text()
            if 3 <= b <= 8:
                return int.from_bytes(self.take(2 ** (b - 3)), 'little')
            if 9 <= b <= 14:
                return int.from_bytes(self.take(2 ** (b - 9)), 'little', signed=True)
        if k == 6:
            return self.ci()
        raise ValueError(('unsupported value type', tid, k, b))
