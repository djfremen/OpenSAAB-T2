import json
from pathlib import Path
import tempfile
import threading
import time
import unittest
from agent import Desktop, Harness, safe_name, template_distance

class HarnessTests(unittest.TestCase):
    def test_native_key_identity_and_unicode(self):
        import ctypes as C
        class KI(C.Structure):
            _fields_=[('vk',C.c_uint16),('scan',C.c_uint16),('flags',C.c_uint32),('time',C.c_uint32),('extra',C.c_size_t)]
        class Input(C.Structure):
            _fields_=[('type',C.c_uint32),('ki',KI)]
        sent=[]
        class User32:
            def MapVirtualKeyW(self,vk,mode):
                return {13:0x1c,40:0x50,38:0xe048}.get(vk,0)
            def SendInput(self,count,events,size):
                sent.append([(e.ki.vk,e.ki.scan,e.ki.flags) for e in events])
                return count
        d=Desktop.__new__(Desktop);d.u=User32();d.Input=Input;d.KI=KI
        d.key(13);self.assertEqual(sent[-1],[(0,0x1c,8),(0,0x1c,10)])
        d.key(40);self.assertEqual(sent[-1],[(0,0x50,9),(0,0x50,11)])
        d.key(38);self.assertEqual(sent[-1],[(0,0x48,9),(0,0x48,11)])
        d.key(scan=0x00e9,unicode=True);self.assertEqual(sent[-1],[(0,0xe9,4),(0,0xe9,6)])
        with self.assertRaises(ValueError): d.key(0)
        self.assertEqual(len(sent),4)

    def test_reject_path_escape(self):
        for name in ('../token', '/etc/passwd', 'x.y', '', 'a'*81):
            with self.assertRaises(ValueError): safe_name(name)
        self.assertEqual(safe_name('ecm-2004'), 'ecm-2004')

    def test_no_unguarded_actions(self):
        h=Harness.__new__(Harness)
        with self.assertRaises(ValueError):
            h.validate_recipe({'steps':[{'label':'x','expect':{'title':'x'},'action':{'key':'ENTER'}}]})

    def test_cancel_interrupts_wait(self):
        h=Harness.__new__(Harness)
        h.desktop=type('D',(),{'target':lambda _,t:{'title':'wrong'}})()
        cancel=threading.Event(); threading.Timer(.05,cancel.set).start()
        start=time.monotonic()
        with self.assertRaisesRegex(RuntimeError,'Cancelled'):
            h.wait_guard({}, {'title':'expected','timeout':5}, cancel)
        self.assertLess(time.monotonic()-start,.5)

    def test_screen_mismatch_times_out(self):
        h=Harness.__new__(Harness)
        h.desktop=type('D',(),{'target':lambda _,t:{'title':'unexpected'}})()
        with self.assertRaises(TimeoutError):
            h.wait_guard({}, {'title':'expected','timeout':.1}, threading.Event())

    def test_failed_precondition_sends_no_input_and_unlocks(self):
        h=Harness.__new__(Harness); h.lock=threading.Lock();h.lock.acquire()
        h.cancel=threading.Event();h.jobs={'j':{}};h.event=lambda *a,**k:None
        called=[]
        h.desktop=type('D',(),{'focus':lambda _,t:None,'input':lambda *args:called.append(True),'target':lambda _,t:(_ for _ in ()).throw(RuntimeError('gone'))})()
        h.wait_guard=lambda *args:(_ for _ in ()).throw(TimeoutError('wrong screen'))
        h.workflow('j',{'steps':[{'label':'x','expect':{},'action':{'key':'ENTER'},'after':{}}]}, {})
        self.assertEqual(called,[]);self.assertEqual(h.jobs['j']['state'],'failed');self.assertFalse(h.lock.locked())

    def test_image_guard_does_not_resize(self):
        from PIL import Image
        a=Image.new('RGB',(8,8),'blue')
        self.assertEqual(template_distance(a,a),0)
        self.assertEqual(template_distance(a,Image.new('RGB',(9,8),'blue')),1)
        self.assertGreater(template_distance(a,Image.new('RGB',(8,8),'white')),.08)

if __name__=='__main__': unittest.main()
