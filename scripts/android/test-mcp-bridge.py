#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""Protocol regressions and an optional synthetic round-trip benchmark; no ADB/car."""
import copy, importlib.util, json, pathlib, socketserver, tempfile, threading, time, unittest
from unittest import mock

ROOT=pathlib.Path(__file__).resolve().parent
def load(path,name):
    spec=importlib.util.spec_from_file_location(name,path);module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module);return module
bridge_module=load(ROOT/'opensaab-android-mcp.py','mcp_bridge')

class Endpoint:
    def __init__(self,features=True,delay=0):
        self.condition=threading.Condition();self.features=features;self.delay=delay
        self.phase=0;self.sequence=0;self.manual=0;self.session='synthetic';self.consume=True;self.keys=0;self.calls=[];self.drop_key_reply=False
        fixture=self
        class Handler(socketserver.StreamRequestHandler):
            def handle(self):
                request=json.loads(self.rfile.readline(16385));value=fixture.dispatch(request)
                if request['op']=='key' and fixture.drop_key_reply:return
                if fixture.delay:time.sleep(fixture.delay)
                self.wfile.write((json.dumps(value)+'\n').encode())
        self.server=socketserver.ThreadingTCPServer(('127.0.0.1',0),Handler);self.server.daemon_threads=True
        self.thread=threading.Thread(target=self.server.serve_forever,daemon=True);self.thread.start()
    def state(self):
        return {'session':self.session,'manual_generation':self.manual,'input_idle':True,
                'mcp_features':['wait_state_v1'] if self.features else [],
                'menu':{'input_sequence':self.sequence,'input_idle':True,'revision':str(self.phase),
                        'key_encoders':[['ENTER',16]],'entries':[{'label':'Next '+str(self.phase),'key':'ENTER'}]}}
    def advance(self):
        with self.condition:self.phase+=1;self.sequence+=1;self.condition.notify_all()
    def dispatch(self,r):
        with self.condition:
            self.calls.append(r['op'])
            if r['op']=='state':return self.state()
            if r['op']=='key':
                if r['session']!=self.session or r['manual_generation']!=self.manual or r['revision']!=str(self.phase):return {'error':'Stale menu or manual takeover'}
                self.keys+=1
                if self.consume:threading.Timer(.02,self.advance).start()
                return {'accepted':True}
            if r['op']=='wait_state':
                deadline=time.monotonic()+r['wait_ms']/1000
                while True:
                    if r['session']!=self.session or r['manual_generation']!=self.manual:return {'error':'Session replacement or manual takeover'}
                    transitioned=self.sequence>r['after_sequence'] and str(self.phase)!=r['after_revision']
                    remaining=deadline-time.monotonic()
                    if transitioned or remaining<=0:return {'state':self.state(),'observed_transition':transitioned}
                    self.condition.wait(remaining)
            return {'error':'Unknown operation'}
    def bridge(self,module=bridge_module):
        b=module.Bridge.__new__(module.Bridge);b.port=self.server.server_address[1];b.token='0'*64
        b.request_timings=[];b.request_count=0;b.request_elapsed_ms=0
        return b
    def close(self):self.server.shutdown();self.server.server_close();self.thread.join()
    def __enter__(self):return self
    def __exit__(self,*args):self.close()

class BridgeTests(unittest.TestCase):
    def test_navigation_reuses_observed_states(self):
        with Endpoint() as e:
            result=bridge_module.execute(e.bridge(),'opensaab_android_navigate',{'labels':['Next 0','Next 1','Next 2']})
            self.assertEqual(e.calls,['state','key','wait_state','key','wait_state','key','wait_state'])
            self.assertEqual(result['state']['menu']['input_sequence'],3)
            self.assertFalse(result['workflow_completion_verified'])
            self.assertEqual(result['connector_timing']['requests'],7)
    def test_old_endpoint_polling_remains_supported(self):
        with Endpoint(features=False) as e:
            result=e.bridge().select('Next 0')
            self.assertEqual(result['state']['menu']['input_sequence'],1)
            self.assertNotIn('wait_state',e.calls)
    def test_lost_or_unconsumed_key_is_not_repeated(self):
        with Endpoint() as e:
            e.consume=False;b=e.bridge();before=b.state();b.key('ENTER',before)
            with self.assertRaisesRegex(RuntimeError,'no repeated key'):b.wait_transition(before,.06)
            self.assertEqual(e.keys,1)
    def test_lost_key_reply_is_not_replayed(self):
        with Endpoint() as e:
            e.drop_key_reply=True
            with self.assertRaisesRegex(RuntimeError,'unavailable'):e.bridge().select('Next 0')
            self.assertEqual(e.keys,1)
    def test_manual_takeover_stops_a_route(self):
        with Endpoint() as e:
            e.consume=False
            def takeover():
                with e.condition:e.manual+=1;e.condition.notify_all()
            threading.Timer(.03,takeover).start()
            with self.assertRaisesRegex(RuntimeError,'manual takeover'):e.bridge().select('Next 0')
            self.assertEqual(e.keys,1)
    def test_replaced_session_stops_a_route(self):
        with Endpoint() as e:
            e.consume=False
            def replace():
                with e.condition:e.session='replacement';e.condition.notify_all()
            threading.Timer(.03,replace).start()
            with self.assertRaisesRegex(RuntimeError,'Session replacement'):e.bridge().select('Next 0')
            self.assertEqual(e.keys,1)
    def test_rejected_key_never_retried(self):
        with Endpoint() as e:
            b=e.bridge();state=b.state();e.phase+=1
            with self.assertRaisesRegex(RuntimeError,'Stale'):b.select('Next 0',state)
            self.assertEqual(e.keys,0)
    def test_timeout_response_is_not_sufficient_without_transition(self):
        with Endpoint() as e:
            b=e.bridge();before=b.state();before['menu']['input_sequence']=100
            with self.assertRaises(RuntimeError):b.wait_transition(before,.02)
            self.assertEqual(e.keys,0)
    def test_bridge_lifetime_retains_one_forward(self):
        with Endpoint() as e,tempfile.TemporaryDirectory() as t:
            token=pathlib.Path(t)/'token';token.write_text('0'*64);token.chmod(0o600)
            with mock.patch.object(bridge_module.subprocess,'check_output',return_value=str(e.server.server_address[1])) as create, mock.patch.object(bridge_module.subprocess,'run') as remove:
                b=bridge_module.Bridge('synthetic',str(token),'adb');b.state();b.select('Next 0');b.close()
                self.assertEqual(create.call_count,1);self.assertEqual(remove.call_count,1)
    def test_route_rejects_invalid_labels_before_reading(self):
        with Endpoint() as e:
            for labels in ([],[''],['Next 0',7],['x']*13):
                with self.assertRaises(ValueError):bridge_module.execute(e.bridge(),'opensaab_android_navigate',{'labels':labels})
            self.assertEqual(e.calls,[])

def benchmark(baseline):
    old=load(baseline,'baseline_mcp_bridge');rows=[]
    for name,module in [('baseline',old),('candidate',bridge_module)]:
        samples=[];counts=[]
        for _ in range(5):
            with Endpoint(delay=.015) as e:
                began=time.monotonic();module.execute(e.bridge(module),'opensaab_android_navigate',{'labels':['Next '+str(n) for n in range(4)]})
                samples.append(round((time.monotonic()-began)*1000,2));counts.append(len(e.calls))
        rows.append({'implementation':name,'elapsed_ms':samples,'requests':counts})
    return {'scope':'Synthetic TCP fixture; 15 ms response delay, 20 ms key consumption, four exact menu labels. No Android/ADB/vehicle qualification.','results':rows}

if __name__=='__main__':
    import sys
    if len(sys.argv)==3 and sys.argv[1]=='--benchmark-baseline':print(json.dumps(benchmark(sys.argv[2]),indent=2))
    else:unittest.main()
