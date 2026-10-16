# SPDX-License-Identifier: MPL-2.0
import io
import json
import unittest
from fastapi import FastAPI
from fastapi.testclient import TestClient
from service.support_reports_api import make_router

class Store:
    def __init__(self):
        self.objects = {}
        self.fail = False
    def put_object(self, **kw):
        if self.fail:
            raise RuntimeError('private server detail')
        self.objects[kw['Key']] = (kw['Body'], kw['Metadata'])
    def head_object(self, **kw):
        body, meta = self.objects[kw['Key']]
        return {'ContentLength': len(body), 'Metadata': meta}
    def get_object(self, **kw):
        return {'Body': io.BytesIO(self.objects[kw['Key']][0])}

class Reports(unittest.TestCase):
    def setUp(self):
        self.store = Store()
        self.app = FastAPI()
        self.app.include_router(make_router(lambda:self.store,lambda:'private-test-bucket',lambda:'test-only-admin'))
        self.client = TestClient(self.app)
        self.report = {'format':1,'app':'com.opensaab.tech2.headunit32','android_api':27,
                       'description':'Synthetic support test','recent_sessions':[{'adapter':'offline','logs':[]}]}
        self.headers = {'X-OpenSAAB-Consent':'support-report-v1'}
    def send(self, report=None):
        return self.client.post('/api/support/reports',json=self.report if report is None else report,headers=self.headers)
    def test_round_trip_private_and_retry_id(self):
        first = self.send()
        self.assertEqual(201,first.status_code)
        self.assertTrue(first.json()['stored'])
        self.assertEqual(first.json(),self.send().json())
        self.assertEqual(1,len(self.store.objects))
        route='/api/admin/support/reports/'+first.json()['report_id']
        self.assertEqual(401,self.client.get(route).status_code)
        self.assertEqual(self.report,self.client.get(route,headers={'Authorization':'Bearer test-only-admin'}).json())
        self.assertEqual(404,self.client.get('/api/support/reports/'+first.json()['report_id']).status_code)
    def test_performance_context_and_legacy_reports(self):
        report=dict(self.report,app='com.opensaab.tech2',device_resources={
            'ram_total_bytes':1073741824,'ram_available_bytes':180000000,'system_low_memory':True,
            'display_width_px':1024,'display_height_px':600},recent_sessions=[{
            'adapter':'offline','native_performance':{'performance_schema':1,'complete':False,
            'sample_interval_ms':5000,'sample_count':1,'samples':[{'elapsed_ms':5000,
            'cpu_ms':4000,'rss_kib':150000,'ram_available_kib':170000}]}}])
        self.assertEqual(201,self.send(report).status_code)
        self.assertEqual(201,self.send().status_code)
        report['device_resources']['vin']='not-allowed'
        self.assertEqual(400,self.send(report).status_code)

    def test_panic_location_only_where_never_message(self):
        where={'file':'emulator/src/chipsoft.rs','line':412,'column':17,'thread':'main'}
        def with_panic(location):
            return dict(self.report,app='com.opensaab.tech2',recent_sessions=[{'adapter':'chipsoft','logs':[
                {'source':'rust.log','file_bytes':120,'tail_only':False,'event_counts':{'PANIC':1},'panic_location':location}]}])
        self.assertEqual(201,self.send(with_panic(where)).status_code)
        self.assertEqual(201,self.send(with_panic(dict(where,thread='<unnamed>',file='m68k-0.12.1/src/cpu.rs'))).status_code)
        self.setUp()  # Fresh rate-limit window: these cases test validation, not limits.
        for bad in (dict(where,message='called unwrap on VIN YS3F...'), dict(where,file='/Users/dev/x/y/z.rs'),
                    dict(where,file='a/b/c/d.rs'), dict(where,line='412'), dict(where,line=True), dict(where,line=-1),
                    {k:v for k,v in where.items() if k!='thread'}, dict(where,thread='x'*65), 'src/a.rs:1:1'):
            self.assertEqual(400,self.send(with_panic(bad)).status_code,bad)
        self.setUp()
        # The inner keys are not valid anywhere else in a report.
        self.assertEqual(400,self.send(dict(self.report,device_resources={'file':'x'})).status_code)
        self.assertEqual(400,self.send(dict(self.report,recent_sessions=[{'adapter':'offline','line':3}])).status_code)

    def test_panic_location_rejects_relative_traversal(self):
        for path in ('../src/a.rs', './src/a.rs', 'src/../a.rs', 'src/./a.rs'):
            where={'file':path,'line':412,'column':17,'thread':'main'}
            report=dict(self.report,recent_sessions=[{'adapter':'chipsoft','logs':[{'source':'rust.log','panic_location':where}]}])
            self.assertEqual(400,self.send(report).status_code)

    def test_desktop_reports(self):
        desktop={'format':1,'app':'com.opensaab.desktop','platform':'macos','arch':'arm64','version':'0.2.18',
                 'build':'abc1234','os_version':'Version 15.1','description':'Mac QC',
                 'device_resources':{'ram_total_bytes':17179869184,'runtime_cpu_count':10},
                 'recent_sessions':[{'adapter':'chipsoft','startup':{'native_boot_ms':900},'guest_crash':False,
                   'session_end':{'reason':'adapter_stopped','exit_code':3,'elapsed_ms':900},
                   'emulator_outcome':{'status':'incomplete','exit_code':3}}],
                 'connection_attempts':[],'last-app-error.json':{'utc':'2026-09-24T10:00:00Z','kind':'session_error','message':'x'}}
        first=self.send(desktop)
        self.assertEqual(201,first.status_code)
        route='/api/admin/support/reports/'+first.json()['report_id']
        self.assertEqual(desktop,self.client.get(route,headers={'Authorization':'Bearer test-only-admin'}).json())
        for bad in (dict(desktop,platform='ios'),dict(desktop,arch='ppc'),dict(desktop,android_api=27),
                    {k:v for k,v in desktop.items() if k!='platform'},dict(desktop,vin='x'),dict(desktop,app='com.other')):
            self.assertEqual(400,self.send(bad).status_code)
        self.assertEqual(400,self.send({k:v for k,v in self.report.items() if k!='android_api'}).status_code)

    def test_auth_status_snapshot(self):
        auth = {'auth_status': '[POST-AUTH]', 'freshness': 'Fresh',
                'observed_utc': '2026-09-30T12:01:00Z', 'status_utc': '2026-09-30T12:00:00Z',
                'vehicle_access_verified': False}
        report = dict(self.report, security_status=auth)
        result = self.send(report)
        self.assertEqual(201, result.status_code)
        route = '/api/admin/support/reports/' + result.json()['report_id']
        self.assertEqual(report, self.client.get(route, headers={'Authorization': 'Bearer test-only-admin'}).json())
        for bad in [dict(auth, auth_status='private payload'), dict(auth, vin='private'),
                    dict(auth, vehicle_access_verified=True), dict(auth, status_utc='not a timestamp'),
                    dict(auth, freshness='private'), dict(auth, observed_utc='2026-09-30T12:00:00')]:
            self.assertEqual(400, self.send(dict(self.report, security_status=bad)).status_code)

    def test_candidate_diagnostics_round_trip(self):
        session_id = 'chipsoft-00000000-0000-0000-0000-000000000001'
        report = dict(self.report, recent_sessions=[{
            'adapter': 'chipsoft', 'session_id': session_id, 'firmware_version': '1.5.2',
            'diagnostic_events': [{'event': 'START', 'utc': '2026-09-29T12:00:00Z'},
                                  {'event': 'EXPECTED_STOP', 'utc': '2026-09-29T12:01:00Z'}],
            'logs': [{'source': 'rust.log', 'failure_categories': [
                {'category': 'adapter_firmware_not_validated', 'tail_line': 3}]}]}],
            emulator_health={'session_id': session_id, 'orientation_start': 1,
                'orientation_end': 2, 'pause_count': 1, 'resume_count': 1},
            **{'last-app-error.json': {'session_id': session_id,
                'failure_category': 'usb_open_denied', 'incident_relation': 'same_session'}})
        result = self.send(report)
        self.assertEqual(201, result.status_code)
        route = '/api/admin/support/reports/' + result.json()['report_id']
        self.assertEqual(report, self.client.get(route, headers={
            'Authorization': 'Bearer test-only-admin'}).json())

    def test_candidate_diagnostics_reject_raw_and_unbounded_values(self):
        invalid = [
            {'session_id': '/private/owner/device'},
            {'firmware_version': 'raw private text'},
            {'failure_category': 'raw secret error'},
            {'incident_relation': 'unverified value'},
            {'diagnostic_events': [{'event': 'private text'}]},
            {'diagnostic_events': [{'event': 'START'}] * 33},
            {'failure_categories': [{'category': 'unclassified'}] * 17},
            {'failure_categories': [{'category': 'usb_open_denied', 'raw': 'private'}]},
        ]
        for extra in invalid:
            with self.subTest(extra=extra):
                self.assertEqual(400, self.send(dict(self.report, recent_sessions=[extra])).status_code)
        self.assertFalse(self.store.objects)

    def test_evidence_v2_round_trip_and_privacy(self):
        evidence={'schema':2,'adapter':'nano','artifacts':{'crash.json':'available','report.json':'missing','cleanup-evidence.json':'missing'},
                  'attempt_id':'00000000-0000-0000-0000-000000000001','session_mode':'read_only','last_operation':'vin',
                  'primary_reason':'PROTOCOL_OR_PROCESS_ERROR','terminal_fault':{'pc':94662,'recent_pcs':[94660,94662]},
                  'cleanup':{'child_exit_code':3,'forced_termination':False,'usb_release':'succeeded','protocol_cleanup':'unknown'},
                  'startup_result':{'startup_complete':False,'fresh_dh_exchanges':0}}
        import copy
        desktop={'format':1,'app':'com.opensaab.desktop','platform':'macos','arch':'x86_64','description':'Synthetic evidence v2',
                 'submitter_notes_provided':True,'recent_sessions':[{'adapter':'nano','session_id':'a'*64,'diagnostic_evidence':evidence}]}
        result=self.send(desktop);self.assertEqual(201,result.status_code,result.text)
        body=json.loads(next(iter(self.store.objects.values()))[0]);self.assertEqual(desktop,body)
        for session_id in ['mdi-00000000-0000-0000-0000-000000000001','offline-00000000-0000-0000-0000-000000000001']:
            report=copy.deepcopy(self.report);report['recent_sessions']=[{'session_id':session_id,'diagnostic_evidence':evidence}]
            self.assertEqual(201,self.send(report).status_code)
        for bad in [dict(evidence,vin='private'),dict(evidence,terminal_fault={'pc':-1}),
                    dict(evidence,terminal_fault={'recent_pcs':list(range(33))}),dict(evidence,terminal_fault={'pc':True}),
                    dict(evidence,artifacts={'crash.json':'raw contents'}),dict(evidence,cleanup={'child_exit_code':2**40}),
                    dict(evidence,startup_result={'vin':'private'}),dict(evidence,last_operation='raw command')]:
            self.setUp() # Each malformed fixture gets a fresh rate limiter.
            report=copy.deepcopy(desktop);report['recent_sessions'][0]['diagnostic_evidence']=bad
            self.assertEqual(400,self.send(report).status_code)
    def test_existing_ios_client_preserved(self):
        self.assertEqual(201,self.send({'format':1,'app':'com.opensaab.t2.ios.preview','platform':'ios','arch':'arm64',
            'description':'Synthetic existing client','model':'iPhone17,1','failure_stage':'unexpected_emulator_exit'}).status_code)

    def test_consent_and_media_type(self):
        self.assertEqual(400,self.client.post('/api/support/reports',json=self.report).status_code)
        self.assertEqual(415,self.client.post('/api/support/reports',content=b'raw',headers=self.headers).status_code)
    def test_size_unknown_fields_and_format(self):
        self.assertEqual(400,self.send(dict(self.report,vin='not-allowed')).status_code)
        self.assertEqual(400,self.send(dict(self.report,format=2)).status_code)
        self.assertEqual(400,self.send(dict(self.report,description='x'*2049)).status_code)
        self.assertEqual(413,self.client.post('/api/support/reports',content=b'x'*65537,headers={**self.headers,'Content-Type':'application/json'}).status_code)
    def test_failure_never_reports_success(self):
        self.store.fail=True
        result=self.send()
        self.assertEqual(503,result.status_code)
        self.assertNotIn('private server detail',result.text)
        self.assertFalse(self.store.objects)
    def test_no_ephemeral_fallback(self):
        app=FastAPI();app.include_router(make_router(lambda:None,lambda:'',lambda:'test'))
        self.assertEqual(503,TestClient(app).post('/api/support/reports',json=self.report,headers=self.headers).status_code)
    def test_rate_limit(self):
        for _ in range(10):self.assertEqual(201,self.send().status_code)
        self.assertEqual(429,self.send().status_code)
    def test_storage_verification(self):
        self.store.head_object=lambda **kw:{'ContentLength':0,'Metadata':{}}
        self.assertEqual(503,self.send().status_code)

if __name__=='__main__':unittest.main()
