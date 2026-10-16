"""Persistent, loopback-only Windows desktop harness. No adapter protocol code.

Requires Python 3.10+ and Pillow. Run in the interactive user's desktop.
All mutations are serialized; workflow cancellation stays available while busy.
"""
import argparse
import base64
import ctypes as C
from ctypes import wintypes as W
import hashlib
import hmac
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import io
import json
import logging
from logging.handlers import RotatingFileHandler
import os
from pathlib import Path
import re
import secrets
import subprocess
import threading
import time
import uuid

VERSION = '1.0.0'
MAX_BODY = 256 * 1024


def safe_name(value):
    if not isinstance(value, str) or not re.fullmatch(r'[a-zA-Z0-9_-]{1,80}', value):
        raise ValueError('Invalid artifact or recipe name')
    return value


def template_distance(a, b):
    """Normalized mean pixel error. Deliberately no resizing across window sizes."""
    from PIL import ImageChops, ImageStat
    if a.size != b.size:
        return 1.0
    return sum(ImageStat.Stat(ImageChops.difference(a.convert('RGB'), b.convert('RGB'))).mean) / 765


class Desktop:
    def __init__(self):
        from PIL import ImageGrab
        self.grab = ImageGrab.grab
        self.u = C.WinDLL('user32', use_last_error=True)
        self.k = C.WinDLL('kernel32', use_last_error=True)
        self.u.SetProcessDPIAware()
        self.callback = C.WINFUNCTYPE(W.BOOL, W.HWND, W.LPARAM)
        specs = {
            'EnumWindows': ([self.callback, W.LPARAM], W.BOOL),
            'EnumChildWindows': ([W.HWND, self.callback, W.LPARAM], W.BOOL),
            'GetWindowTextW': ([W.HWND, W.LPWSTR, C.c_int], C.c_int),
            'GetClassNameW': ([W.HWND, W.LPWSTR, C.c_int], C.c_int),
            'IsWindowVisible': ([W.HWND], W.BOOL),
            'IsWindow': ([W.HWND], W.BOOL),
            'GetWindowRect': ([W.HWND, C.POINTER(W.RECT)], W.BOOL),
            'GetClientRect': ([W.HWND, C.POINTER(W.RECT)], W.BOOL),
            'ClientToScreen': ([W.HWND, C.POINTER(W.POINT)], W.BOOL),
            'SetCursorPos': ([C.c_int, C.c_int], W.BOOL),
            'mouse_event': ([W.DWORD, W.DWORD, W.DWORD, W.DWORD, C.c_size_t], None),
            'GetWindowThreadProcessId': ([W.HWND, C.POINTER(W.DWORD)], W.DWORD),
            'GetForegroundWindow': ([], W.HWND),
            'MapVirtualKeyW': ([W.UINT, W.UINT], W.UINT),
            'SetForegroundWindow': ([W.HWND], W.BOOL),
            'ShowWindow': ([W.HWND, C.c_int], W.BOOL),
            'OpenInputDesktop': ([W.DWORD, W.BOOL, W.DWORD], W.HANDLE),
            'GetUserObjectInformationW': ([W.HANDLE, C.c_int, W.LPVOID, W.DWORD, C.POINTER(W.DWORD)], W.BOOL),
            'CloseDesktop': ([W.HANDLE], W.BOOL),
            'PostMessageW': ([W.HWND, W.UINT, W.WPARAM, W.LPARAM], W.BOOL),
            'SendMessageTimeoutW': ([W.HWND, W.UINT, W.WPARAM, W.LPARAM, W.UINT, W.UINT, C.POINTER(C.c_size_t)], W.LPARAM),
        }
        for name, (args, ret) in specs.items():
            f = getattr(self.u, name); f.argtypes = args; f.restype = ret
        # INPUT must include the full MOUSEINPUT union, including on 32-bit Python.
        class MI(C.Structure):
            _fields_ = [('dx', W.LONG), ('dy', W.LONG), ('data', W.DWORD), ('flags', W.DWORD), ('time', W.DWORD), ('extra', C.c_size_t)]
        class KI(C.Structure):
            _fields_ = [('vk', W.WORD), ('scan', W.WORD), ('flags', W.DWORD), ('time', W.DWORD), ('extra', C.c_size_t)]
        class U(C.Union):
            _fields_ = [('mi', MI), ('ki', KI)]
        class Input(C.Structure):
            _anonymous_ = ('u',)
            _fields_ = [('type', W.DWORD), ('u', U)]
        self.Input, self.KI = Input, KI
        self.u.SendInput.argtypes = [W.UINT, C.POINTER(Input), C.c_int]
        self.u.SendInput.restype = W.UINT

    def ready(self):
        d = self.u.OpenInputDesktop(0, False, 1)
        if not d:
            raise RuntimeError('Windows desktop locked or unavailable')
        try:
            name = C.create_unicode_buffer(256); used = W.DWORD()
            if not self.u.GetUserObjectInformationW(d, 2, name, C.sizeof(name), C.byref(used)):
                raise C.WinError(C.get_last_error())
            if name.value.lower() != 'default':
                raise RuntimeError('Unlock Windows / dismiss secure desktop prompt')
        finally:
            self.u.CloseDesktop(d)

    def title(self, hwnd):
        b = C.create_unicode_buffer(2048); self.u.GetWindowTextW(hwnd, b, len(b)); return b.value

    def info(self, hwnd):
        r = W.RECT(); p = W.DWORD(); b = C.create_unicode_buffer(256)
        if not self.u.GetWindowRect(hwnd, C.byref(r)):
            raise RuntimeError('Window no longer exists')
        self.u.GetWindowThreadProcessId(hwnd, C.byref(p))
        self.u.GetClassNameW(hwnd, b, len(b))
        return dict(hwnd=int(hwnd), pid=p.value, title=self.title(hwnd), class_name=b.value,
                    rect=[r.left, r.top, r.right, r.bottom])

    def windows(self, parent=None):
        result = []
        @self.callback
        def cb(h, _):
            if self.u.IsWindowVisible(h):
                result.append(self.info(h))
            return True
        if parent:
            self.u.EnumChildWindows(parent, cb, 0)
        else:
            self.u.EnumWindows(cb, 0)
        return result

    def target(self, target):
        # A PID guard prevents a stale HWND being used after process replacement.
        if not isinstance(target, dict) or not target.get('pid'):
            raise ValueError('target.pid required')
        matches = [w for w in self.windows() if w['pid'] == int(target['pid'])
                   and (not target.get('hwnd') or w['hwnd'] == int(target['hwnd']))
                   and (not target.get('title') or w['title'] == target['title'])]
        if len(matches) != 1:
            raise RuntimeError(f'Expected one target window, found {len(matches)}')
        return matches[0]

    def focus(self, target):
        self.ready(); w = self.target(target); h = w['hwnd']
        self.u.ShowWindow(h, 9)
        if self.u.GetForegroundWindow() != h:
            self.u.SetForegroundWindow(h)
            # Windows may initially reject foreground activation. Alt is the
            # standard user-input gesture, and focus is verified before typing.
            if self.u.GetForegroundWindow() != h:
                self.key(0x12); self.u.SetForegroundWindow(h)
            time.sleep(.08)
        if self.u.GetForegroundWindow() != h:
            raise RuntimeError('Could not acquire target focus; no input sent')
        return w

    def key(self, vk=0, scan=0, unicode=False):
        flags = 4 if unicode else 0
        if not unicode:
            # minifb reads the native scan code in WM_KEYDOWN's lParam.
            # Virtual-key-only SendInput leaves that field zero on this host.
            mapped = self.u.MapVirtualKeyW(vk, 4)
            if not mapped or mapped >> 8 not in (0, 0xE0):
                raise ValueError('Unsupported keyboard scan code')
            scan = mapped & 0xFF
            # Some installed layouts return an unprefixed keypad scan for
            # navigation VKs even with MAPVK_VK_TO_VSC_EX. Preserve their
            # dedicated (extended) identity rather than typing keypad digits.
            extended = mapped >> 8 == 0xE0 or vk in (33, 34, 35, 36, 37, 38, 39, 40, 45, 46)
            flags = 8 | (1 if extended else 0)
            vk = 0
        events = (self.Input * 2)()
        for i in range(2):
            events[i].type = 1
            events[i].ki = self.KI(vk, scan, flags | (2 if i else 0), 0, 0)
        if self.u.SendInput(2, events, C.sizeof(self.Input)) != 2:
            raise RuntimeError('Windows rejected keyboard input')

    def input(self, target, action, cancel):
        w = self.focus(target)
        if 'sequence' in action:
            if not 1 <= len(action['sequence']) <= 12: raise ValueError('Invalid sequence length')
            for part in action['sequence']:
                if 'sequence' in part: raise ValueError('Nested sequences prohibited')
                self.input(target, part, cancel)
            return
        if 'key' in action:
            keys = {'ENTER': 13, 'ESC': 27, 'UP': 38, 'DOWN': 40, 'LEFT': 37, 'RIGHT': 39,
                    'TAB': 9, 'SPACE': 32, 'HOME': 36, 'END': 35,
                    **{f'F{i}': 111+i for i in range(1, 13)}}
            vk = keys[action['key']]
            count = int(action.get('count', 1))
            if not 1 <= count <= 40: raise ValueError('Invalid key count')
            for _ in range(count):
                if cancel.is_set(): raise RuntimeError('Cancelled')
                if self.u.GetForegroundWindow() != w['hwnd']: raise RuntimeError('Focus changed')
                self.key(vk); time.sleep(.07)
        elif 'text' in action:
            text = action['text']
            if len(text) > 2000: raise ValueError('Text too long')
            for offset in range(0, len(raw := text.encode('utf-16le')), 2):
                if cancel.is_set(): raise RuntimeError('Cancelled')
                if self.u.GetForegroundWindow() != w['hwnd']: raise RuntimeError('Focus changed')
                self.key(scan=int.from_bytes(raw[offset:offset+2], 'little'), unicode=True)
        else:
            raise ValueError('Expected key or text')

    def capture(self, target=None, crop=None, client=False):
        self.ready()
        if target:
            w = self.target(target)
            # Capture only a visible, foreground target, avoiding occluded/stale evidence.
            if self.u.GetForegroundWindow() != w['hwnd']:
                self.focus(target)
            box=w['rect']
            if client:
                rc=W.RECT(); pt=W.POINT(0,0)
                if not self.u.GetClientRect(w['hwnd'], C.byref(rc)) or not self.u.ClientToScreen(w['hwnd'],C.byref(pt)):
                    raise RuntimeError('Cannot locate client area')
                box=[pt.x,pt.y,pt.x+rc.right,pt.y+rc.bottom]
            im = self.grab(bbox=tuple(box), all_screens=True)
        else:
            im = self.grab(all_screens=True)
        if crop:
            if len(crop) != 4 or not (0 <= crop[0] < crop[2] <= im.width and 0 <= crop[1] < crop[3] <= im.height):
                raise ValueError('Crop outside target window')
            im = im.crop(tuple(crop))
        return im.convert('RGB')

    def control_text(self, hwnd):
        b = C.create_unicode_buffer(16384); result = C.c_size_t()
        ok = self.u.SendMessageTimeoutW(hwnd, 13, len(b), C.addressof(b), 2, 500, C.byref(result))
        if not ok: raise RuntimeError('Control text query timed out')
        return b.value


class CaptureJob:
    """Kill only owned capture processes if this agent unexpectedly terminates."""
    def __init__(self):
        self.k=C.WinDLL('kernel32',use_last_error=True)
        class Basic(C.Structure):
            _fields_=[('process_time',C.c_int64),('job_time',C.c_int64),('flags',W.DWORD),
                ('min_ws',C.c_size_t),('max_ws',C.c_size_t),('active',W.DWORD),
                ('affinity',C.c_size_t),('priority',W.DWORD),('scheduling',W.DWORD)]
        class IO(C.Structure):
            _fields_=[(n,C.c_uint64) for n in ('read_ops','write_ops','other_ops','read_bytes','write_bytes','other_bytes')]
        class Extended(C.Structure):
            _fields_=[('basic',Basic),('io',IO),('process_mem',C.c_size_t),('job_mem',C.c_size_t),
                ('peak_process',C.c_size_t),('peak_job',C.c_size_t)]
        self.k.CreateJobObjectW.argtypes=[W.LPVOID,W.LPCWSTR];self.k.CreateJobObjectW.restype=W.HANDLE
        self.k.SetInformationJobObject.argtypes=[W.HANDLE,C.c_int,W.LPVOID,W.DWORD];self.k.SetInformationJobObject.restype=W.BOOL
        self.k.AssignProcessToJobObject.argtypes=[W.HANDLE,W.HANDLE];self.k.AssignProcessToJobObject.restype=W.BOOL
        self.handle=self.k.CreateJobObjectW(None,None)
        limits=Extended();limits.basic.flags=0x2000
        if not self.handle or not self.k.SetInformationJobObject(self.handle,9,C.byref(limits),C.sizeof(limits)):
            raise C.WinError(C.get_last_error())
    def add(self,process):
        if not self.k.AssignProcessToJobObject(self.handle,int(process._handle)):
            process.terminate();process.wait(timeout=3)
            raise RuntimeError('Cannot attach capture lifetime to harness')


class Harness:
    def __init__(self, root, desktop):
        self.root, self.desktop = root, desktop
        self.config = json.loads((root/'config.json').read_text())
        self.started = time.time()
        self.lock = threading.Lock()
        self.cancel = threading.Event()
        self.jobs = {}
        self.capture_process = None
        self.capture_lock = threading.RLock()
        self.capture_job = CaptureJob()
        self.capture_deadline = 0
        self.capture_file = None
        self.capture_log = None
        self.log = logging.getLogger('harness')
        self.log.setLevel(logging.INFO)
        handler = RotatingFileHandler(root/'events.jsonl', maxBytes=5_000_000, backupCount=5, encoding='utf-8')
        handler.setFormatter(logging.Formatter('%(message)s')); self.log.addHandler(handler)
        self.event('started', version=VERSION, pid=os.getpid())
        threading.Thread(target=self.watch_capture, daemon=True).start()

    def event(self, kind, **fields):
        self.log.info(json.dumps(dict(time=time.time(), event=kind, **fields)))

    def screenshot(self, request, name=None):
        im = self.desktop.capture(request.get('target'), request.get('crop'), request.get('client',False))
        b = io.BytesIO(); im.save(b, 'PNG'); data = b.getvalue()
        name = name or f'screen-{uuid.uuid4().hex}'
        (self.root/'artifacts'/f'{safe_name(name)}.png').write_bytes(data)
        return dict(file=f'{name}.png', width=im.width, height=im.height,
                    sha256=hashlib.sha256(data).hexdigest(), png=base64.b64encode(data).decode())

    def watch_capture(self):
        while True:
            time.sleep(1)
            with self.capture_lock:
                if self.capture_process and time.monotonic() >= self.capture_deadline:
                    self.stop_capture()

    def stop_capture(self):
        with self.capture_lock:
            return self._stop_capture()

    def _stop_capture(self):
        p = self.capture_process
        if not p: return dict(running=False)
        if p.poll() is None:
            try:
                p.stdin.write(b'q\n'); p.stdin.flush(); p.wait(timeout=3)
            except (OSError, subprocess.TimeoutExpired):
                p.terminate(); p.wait(timeout=3)
        result = dict(running=False, file=self.capture_file.name, bytes=self.capture_file.stat().st_size if self.capture_file.exists() else 0)
        if self.capture_log: self.capture_log.close()
        self.capture_process = None
        self.event('capture_stopped', **result)
        return result

    def wait_guard(self, target, guard, cancel):
        from PIL import Image
        deadline = time.monotonic() + min(max(float(guard.get('timeout', 15)), .1), 90)
        template = None
        if 'template' in guard:
            template = Image.open(self.root/'templates'/f"{safe_name(guard['template'])}.png").convert('RGB')
        if not template and 'control_contains' not in guard and 'title' not in guard:
            raise ValueError('Guard must specify template, title or control_contains')
        consecutive = 0
        while True:
            if cancel.is_set(): raise RuntimeError('Cancelled')
            w = self.desktop.target(target)
            if hasattr(self.desktop, 'u') and self.desktop.u.GetForegroundWindow() != w['hwnd']:
                raise RuntimeError('Focus changed; sequence stopped')
            good = not guard.get('title') or w['title'] == guard['title']
            if template:
                score = template_distance(self.desktop.capture(target, guard.get('crop'), guard.get('client',False)), template)
                good = good and score <= min(float(guard.get('tolerance', .015)), .08)
            if 'control_contains' in guard:
                controls = self.desktop.windows(w['hwnd'])
                good = good and any(guard['control_contains'] in self.desktop.control_text(c['hwnd']) for c in controls if c['class_name'] == guard.get('class_name', 'Edit'))
            consecutive = consecutive + 1 if good else 0
            if consecutive >= 2: return
            if time.monotonic() >= deadline: raise TimeoutError('Expected screen/control not found; sequence stopped')
            cancel.wait(.15)

    def workflow(self, job, recipe, target):
        result = self.jobs[job]
        try:
            self.desktop.focus(target)
            for i, step in enumerate(recipe['steps']):
                result['step'] = i; result['label'] = step['label']
                self.event('step_wait', job=job, step=i, label=step['label'])
                self.wait_guard(target, step['expect'], self.cancel)
                if self.cancel.is_set(): raise RuntimeError('Cancelled')
                if step.get('action'):
                    self.desktop.input(target, step['action'], self.cancel)
                    # Postcondition mandatory: a script never runs blind sequences.
                    self.wait_guard(target, step['after'], self.cancel)
                self.event('step_complete', job=job, step=i, label=step['label'])
            result['state'] = 'complete'
        except Exception as e:
            result.update(state='cancelled' if self.cancel.is_set() else 'failed', error=str(e))
        finally:
            try:
                # Focus changes are errors; do not wrest control back for final evidence.
                w = self.desktop.target(target)
                if self.desktop.u.GetForegroundWindow() == w['hwnd']:
                    result['screenshot'] = self.screenshot({'target':target}, job)
            except Exception as e:
                result['screenshot_error'] = str(e)
            result['finished'] = time.time()
            self.event('workflow_end', job=job, state=result['state'], error=result.get('error'))
            self.lock.release()

    def validate_recipe(self, r):
        if not isinstance(r.get('steps'), list) or not 1 <= len(r['steps']) <= 40:
            raise ValueError('Recipe needs 1–40 guarded steps')
        for step in r['steps']:
            if not step.get('label') or not step.get('expect') or (step.get('action') and not step.get('after')):
                raise ValueError('Every action needs precondition and postcondition')
        return r

    def dispatch(self, route, r):
        if route == '/shutdown':
            self.cancel.set(); self.event('shutdown_requested')
            threading.Thread(target=self.shutdown,daemon=True).start()
            return dict(requested=True)
        if route == '/events':
            with (self.root/'events.jsonl').open('rb') as f:
                f.seek(0,2); size=f.tell(); f.seek(max(0,size-65536)); lines=f.read().decode('utf-8',errors='replace').splitlines()
            if size>65536: lines=lines[1:]
            return dict(events=[json.loads(line) for line in lines[-min(max(int(r.get('limit',50)),1),200):]])
        if route == '/health':
            return dict(version=VERSION, pid=os.getpid(), uptime=round(time.time()-self.started, 2), busy=self.lock.locked(),
                        capture_running=bool(self.capture_process and self.capture_process.poll() is None))
        if route == '/jobs':
            if r.get('id'):
                job = self.jobs.get(r['id'])
                if job is None: raise ValueError('Unknown job')
                return dict(job)
            return {k:{x:v for x,v in j.items() if x!='screenshot'} for k,j in self.jobs.items()}
        if route == '/cancel':
            self.cancel.set(); self.event('cancel_requested'); return dict(requested=True)
        if route == '/recipes':
            return dict(recipes=[p.stem for p in (self.root/'recipes').glob('*.json')])
        if not self.lock.acquire(blocking=False): raise RuntimeError('Harness busy; use /jobs or /cancel')
        release = True
        try:
            self.cancel.clear()
            if route == '/windows': return dict(windows=self.desktop.windows(r.get('parent')))
            if route == '/snapshot': return self.screenshot(r)
            if route == '/input':
                self.desktop.input(r['target'], r, self.cancel)
                self.event('input', target=r['target'], key=r.get('key'), count=r.get('count',1), text_length=len(r.get('text','')))
                return dict(sent=True)
            if route == '/launch':
                name = safe_name(r['app']); app = self.config['apps'][name]
                self.desktop.ready()
                existing = [w for w in self.desktop.windows() if w['title'] == app.get('title')]
                if existing: return dict(existing=existing)
                env = os.environ.copy()
                if 'env' in app: env.update(app['env'])
                p = subprocess.Popen([app['exe'], *app.get('args',[])], cwd=app.get('cwd'), env=env)
                self.event('launch', app=name, pid=p.pid)
                return dict(pid=p.pid)
            if route == '/click':
                w=self.desktop.focus(r['target']); x=int(r['x']); y=int(r['y'])
                rect=w['rect']
                if not 0 <= x < rect[2]-rect[0] or not 0 <= y < rect[3]-rect[1]: raise ValueError('Click outside target window')
                self.desktop.u.SetCursorPos(rect[0]+x,rect[1]+y)
                self.desktop.u.mouse_event(2,0,0,0,0); self.desktop.u.mouse_event(4,0,0,0,0)
                self.event('click',pid=w['pid'],x=x,y=y); return dict(sent=True)
            if route == '/close':
                w = self.desktop.target(r['target'])
                self.desktop.u.PostMessageW(w['hwnd'], 0x10, 0, 0)
                self.event('close_requested', pid=w['pid']); return dict(requested=True)
            if route == '/capture/start':
                with self.capture_lock:
                    if self.capture_process and self.capture_process.poll() is None: raise RuntimeError('Capture already running')
                    device = int(r['device']); bus = int(r.get('bus',1))
                    if not 1 <= device <= 127 or not 1 <= bus <= 16: raise ValueError('Invalid USB bus/device')
                    name = f"capture-{uuid.uuid4().hex}"
                    self.capture_file = self.root/'artifacts'/f'{name}.pcap'
                    self.capture_log = (self.root/'artifacts'/f'{name}.txt').open('wb')
                    self.capture_process = subprocess.Popen([self.config['usbpcap'], '-d', f'\\\\.\\USBPcap{bus}', '--devices', str(device), '--inject-descriptors', '-o', str(self.capture_file)], stdin=subprocess.PIPE, stdout=self.capture_log, stderr=subprocess.STDOUT, creationflags=subprocess.CREATE_NO_WINDOW)
                    self.capture_job.add(self.capture_process)
                    self.capture_deadline = time.monotonic() + min(max(int(r.get('seconds',300)),1),1800)
                    time.sleep(.2)
                    if self.capture_process.poll() is not None:
                        self.stop_capture(); raise RuntimeError('USB capture failed to start; inspect capture log')
                    self.event('capture_started', pid=self.capture_process.pid, bus=bus, device=device)
                    return dict(pid=self.capture_process.pid,file=self.capture_file.name)
            if route == '/capture/stop': return self.stop_capture()
            if route == '/run':
                recipe = self.validate_recipe(json.loads((self.root/'recipes'/f"{safe_name(r['recipe'])}.json").read_text()))
                target = r['target']; self.desktop.target(target)
                job = uuid.uuid4().hex
                # Bound in-memory history; artifacts remain on disk.
                if len(self.jobs) >= 30: self.jobs.pop(next(iter(self.jobs)))
                self.jobs[job] = dict(state='running', recipe=r['recipe'], started=time.time())
                threading.Thread(target=self.workflow,args=(job,recipe,target),daemon=True).start()
                release = False
                return dict(id=job,state='running')
            raise ValueError('Unknown route')
        finally:
            if release: self.lock.release()


def main():
    parser=argparse.ArgumentParser(); parser.add_argument('--root',type=Path,required=True); parser.add_argument('--port',type=int,default=18765)
    args=parser.parse_args(); root=args.root
    for name in ('artifacts','recipes','templates'): (root/name).mkdir(parents=True,exist_ok=True)
    token_file=root/'token.txt'
    if not token_file.exists(): token_file.write_text(secrets.token_urlsafe(32),encoding='ascii')
    token=token_file.read_text().strip()
    if len(token)<32: raise ValueError('Invalid authentication token')
    harness=Harness(root,Desktop())
    class Handler(BaseHTTPRequestHandler):
        protocol_version='HTTP/1.1'
        def log_message(self,*a): pass
        def do_POST(self):
            if not hmac.compare_digest(self.headers.get('Authorization',''), 'Bearer '+token):
                self.close_connection=True; self.send_error(401); return
            try:
                length=int(self.headers.get('Content-Length','0'))
                if not 0 <= length <= MAX_BODY: raise ValueError('Request too large')
                self.connection.settimeout(5)
                r=json.loads(self.rfile.read(length) or b'{}')
                result=harness.dispatch(self.path,r); status=200
            except Exception as e:
                status=409 if isinstance(e,(RuntimeError,TimeoutError)) else 400
                result=dict(error=str(e))
            data=json.dumps(result).encode()
            self.send_response(status); self.send_header('Content-Type','application/json'); self.send_header('Content-Length',str(len(data))); self.end_headers(); self.wfile.write(data)
    server=ThreadingHTTPServer(('127.0.0.1',args.port),Handler)
    server.daemon_threads=True
    harness.shutdown=server.shutdown
    try: server.serve_forever(poll_interval=.2)
    finally: harness.cancel.set(); harness.stop_capture(); server.server_close()

if __name__=='__main__': main()
