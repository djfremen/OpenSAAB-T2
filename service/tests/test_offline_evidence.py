# SPDX-License-Identifier: MPL-2.0
import copy
import importlib.util
import json
import os
from pathlib import Path
import unittest

# Run the same privacy/schema corpus against Android, desktop and the iOS-compatible overlay.
source = Path(os.environ.get('OPENSAAB_TEST_VALIDATOR', Path(__file__).parents[1] / 'support_reports_api.py'))
spec = importlib.util.spec_from_file_location('evidence_validator', source)
validator = importlib.util.module_from_spec(spec)
spec.loader.exec_module(validator)

class OfflineEvidence(unittest.TestCase):
    def setUp(self):
        self.session = {
            'adapter': 'offline', 'session_id': 'offline-01234567-89ab-cdef-0123-456789abcdef',
            'emulation_evidence': {'schema': 1, 'reason': 'candi_stopped', 'stage': 'vehicle_link_wait',
                'candi': {'reason': 'unsupported_access', 'pc': 20, 'completed': 50, 'external_tx': False}},
            'emulation_context': {'schema': 1, 'profile': 'offline_research',
                'engine_sha256': 'a'*64, 'firmware_sha256': {'card.bin': 'b'*64},
                'instruction_limit': 50000000000, 'deadline_ms': 1800000,
                'started_utc': '2026-10-03T06:00:00Z'},
            'session_end': {'reason': 'process_exit', 'exit_code': 3, 'elapsed_ms': 41000,
                'input_count': 4, 'frame_count': 10, 'forced_stop': False}, 'guest_crash': False,
            'native_performance': {'performance_schema': 1, 'sample_count': 42,
                'retained_sample_count': 12, 'samples_truncated': True, 'max_frame_age_ms': 9000,
                'min_ram_available_kib': 10, 'samples': []}}
        self.report = {'format': 1, 'app': 'com.opensaab.tech2', 'android_api': 31,
                       'description': 'Synthetic offline test', 'submitter_notes_provided': False,
                       'recent_sessions': [self.session]}
    def test_new_evidence_round_trip(self):
        actual=json.loads(validator.parse_report(json.dumps(self.report).encode()))
        self.assertEqual(actual,self.report)
    def test_legacy_and_missing_records_remain_accepted(self):
        self.session['emulation_context']={'unavailable':True}
        self.session['session_end']={'unavailable':True}
        self.session['emulation_evidence']={'unavailable':True}
        validator.parse_report(json.dumps(self.report).encode())
        validator.parse_report(json.dumps({'format':1,'app':'com.opensaab.tech2','android_api':26,'description':'Legacy','recent_sessions':[]}).encode())
    def test_reject_raw_payloads_unknown_labels_and_malformed_fingerprints(self):
        mutations = [
            ('emulation_evidence','reason','PRIVATE RAW REASON'),
            ('emulation_evidence','stage',{'reason':'unknown'}),
            ('emulation_evidence','screen','PRIVATE SCREEN'),
            ('emulation_context','engine_sha256','/private/file'),
            ('emulation_context','firmware_sha256',{'card.bin':'PRIVATE DATA'}),
            ('emulation_context','deadline_ms',True),
            ('session_end','reason','PRIVATE EXIT'),
            ('session_end','input_count',-1),
            ('session_end','exit_code','3')]
        for parent,key,value in mutations:
            with self.subTest(parent=parent,key=key):
                report=copy.deepcopy(self.report);report['recent_sessions'][0][parent][key]=value
                with self.assertRaises(ValueError):validator.parse_report(json.dumps(report).encode())
    def test_reject_private_candi_detail_and_invalid_notes_flag(self):
        self.session['emulation_evidence']['candi']['payload']='PRIVATE BUS DATA'
        with self.assertRaises(ValueError):validator.parse_report(json.dumps(self.report).encode())
        del self.session['emulation_evidence']['candi']['payload']
        self.report['submitter_notes_provided']='no'
        with self.assertRaises(ValueError):validator.parse_report(json.dumps(self.report).encode())

if __name__ == '__main__': unittest.main()
