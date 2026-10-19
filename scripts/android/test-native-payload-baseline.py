#!/usr/bin/env python3
"""Regression tests for accidentally substituted native workers; no device access."""
import copy, hashlib, tempfile, unittest
from pathlib import Path
from native_payload_baseline import verify_baseline,verify_files

class NativeBaselineTests(unittest.TestCase):
    def setUp(self):
        self.digests={'mdi-module':'a'*64,'mdi-engine':'b'*64,'nano':'c'*64,'recovered':'d'*64}
        self.spec={'schema':1,'baseline_version':'0.1.1-preview.1','baseline_apk_sha256':'e'*64,'unchanged':{k:v for k,v in self.digests.items() if k!='recovered'},'allowed_changed':['recovered']}
    def test_exact_approved_workers_pass(self):self.assertEqual(verify_baseline(self.spec,self.digests),3)
    def test_either_mdi_worker_substitution_fails(self):
        for name in ('mdi-module','mdi-engine'):
            d=dict(self.digests);d[name]='f'*64
            with self.assertRaisesRegex(ValueError,name):verify_baseline(self.spec,d)
    def test_new_recovered_identity_is_explicitly_allowed(self):
        d=dict(self.digests);d['recovered']='f'*64;self.assertEqual(verify_baseline(self.spec,d),3)
    def test_pin_cannot_be_accidentally_omitted(self):
        s=copy.deepcopy(self.spec);del s['unchanged']['mdi-module']
        with self.assertRaisesRegex(ValueError,'every packaged'):verify_baseline(s,self.digests)
    def test_extra_missing_or_overlapping_workers_fail(self):
        for d in ({k:v for k,v in self.digests.items() if k!='mdi-engine'},dict(self.digests,unknown='f'*64)):
            with self.assertRaises(ValueError):verify_baseline(self.spec,d)
        s=copy.deepcopy(self.spec);s['allowed_changed'].append('nano')
        with self.assertRaises(ValueError):verify_baseline(s,self.digests)
    def test_file_bytes_are_hashed(self):
        with tempfile.TemporaryDirectory() as folder:
            p=Path(folder)/'worker';p.write_bytes(b'approved bytes')
            s={'schema':1,'baseline_version':'fixture','baseline_apk_sha256':'e'*64,'unchanged':{'worker':hashlib.sha256(p.read_bytes()).hexdigest()},'allowed_changed':[]}
            self.assertEqual(verify_files(s,{'worker':p}),1);p.write_bytes(b'older retained bytes')
            with self.assertRaises(ValueError):verify_files(s,{'worker':p})

if __name__=='__main__':unittest.main()
