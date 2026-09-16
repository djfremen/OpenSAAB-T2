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
