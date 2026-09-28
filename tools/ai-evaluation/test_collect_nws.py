import json,unittest
from collect_nws import collect,BASE

def page(times,next_url=None):
    d={'type':'FeatureCollection','features':[{'properties':{'timestamp':t}} for t in times]}
    if next_url:d['pagination']={'next':next_url}
    return json.dumps(d).encode()
class CollectorTests(unittest.TestCase):
    def run_pages(self,pages,**kw):
        seq=iter(pages)
        return collect('2026-09-20T11:30:00Z','2026-09-20T12:31:00Z','test',fetch=lambda u:next(seq),**kw)
    def test_nws_lower_boundary_cursor_is_completed_and_old_rows_excluded(self):
        d,p=self.run_pages([page(['2026-09-20T12:00:00Z','2026-09-20T11:30:00Z'],BASE+'?cursor=1'),page(['2026-09-20T11:30:00Z','2026-09-20T11:25:00Z'],BASE+'?cursor=2')])
        self.assertEqual(len(d['features']),2);self.assertEqual(len(p),2);self.assertTrue(d['era_capture']['complete_requested_interval'])
    def test_unsafe_next_origin_refused(self):
        with self.assertRaisesRegex(ValueError,'origin/path'):self.run_pages([page(['2026-09-20T12:00:00Z'],'https://evil.example/')])
    def test_incomplete_page_bound_is_not_success(self):
        with self.assertRaisesRegex(ValueError,'page bound'):self.run_pages([page(['2026-09-20T12:00:00Z'],BASE+'?cursor=1')],max_pages=1)
    def test_nonprogressing_page_refused(self):
        with self.assertRaisesRegex(ValueError,'backwards'):self.run_pages([page(['2026-09-20T12:00:00Z'],BASE+'?cursor=1'),page(['2026-09-20T12:00:00Z'],BASE+'?cursor=2')])
    def test_bad_order_refused(self):
        with self.assertRaisesRegex(ValueError,'chronological'):self.run_pages([page(['2026-09-20T11:35:00Z','2026-09-20T12:00:00Z'])])
    def test_empty_terminal_is_valid_unavailable_input(self):
        d,_=self.run_pages([page([])]);self.assertEqual(d['features'],[])
    def test_oversized_page_refused(self):
        with self.assertRaisesRegex(ValueError,'byte bound'):self.run_pages([b'x'*4_000_001])
