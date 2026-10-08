#!/usr/bin/env python3
"""Measure the isolated interactive APK on an explicitly selected ADB device.
Process-cold launches include Activity startup and the first verified LCD draw.
Optional navigation exercises the guest mailbox; touch controls need separate QA.
"""
import argparse, json, subprocess, time
from pathlib import Path

p=argparse.ArgumentParser(description=__doc__)
p.add_argument('--serial',required=True)
p.add_argument('--adb',default='adb')
p.add_argument('--output',type=Path,required=True)
p.add_argument('--launches',type=int,default=3)
p.add_argument('--navigate',action='store_true')
a=p.parse_args()
if a.launches<1: p.error('--launches must be positive')
a.output.mkdir(parents=True,exist_ok=False)
PKG='com.opensaab.tech2.headunit32.test'
adb=[a.adb,'-s',a.serial]
def call(*args,check=True):
 return subprocess.run(adb+list(args),capture_output=True,check=check,timeout=45)
def read(path):
 result=call('exec-out','run-as',PKG,'cat',path,check=False)
 text=result.stdout.decode(errors='replace')
 return '' if result.returncode or text.startswith('cat:') else text
def json_read(path):
 try:return json.loads(read(path))
 except (ValueError,TypeError):return {}
def wait(fn,label,timeout=75):
 until=time.monotonic()+timeout
 while time.monotonic()<until:
  result=fn()
  if result:return result
  time.sleep(.25)
 raise RuntimeError('Timed out: '+label)
results=[]
for i in range(a.launches):
 old=json_read('files/latest-startup.json').get('run_id')
 call('shell','am','force-stop',PKG)
 origin=int(float(call('shell','cat','/proc/uptime').stdout.split()[0])*1000)
 launch=call('shell','am','start','-W','-n',PKG+'/com.opensaab.tech2.MainActivity','--el','launch_origin_elapsed_ms',str(origin))
 (a.output/f'launch-{i+1}.txt').write_bytes(launch.stdout)
 def current():
  r=json_read('files/latest-startup.json')
  return r if r.get('run_id') and r['run_id']!=old else None
 r=wait(current,'verified welcome')
 assert r['verified_welcome'] and not r['candi_started'],r
 results.append(r);print(json.dumps(r),flush=True)
 run='files/sessions/'+r['run_id']
 (a.output/f'launch-{i+1}.json').write_text(json.dumps(r,indent=2))
 (a.output/f'launch-{i+1}-screen.png').write_bytes(call('exec-out','screencap','-p').stdout)
(a.output/'startup-summary.json').write_text(json.dumps(results,indent=2))
if a.navigate:
 def key(code,expected,label):
  wait(lambda:not read(run+'/interactive-key.txt'),'mailbox empty')
  # Fixed test commands only, passed as separate adb arguments except redirection.
  assert code=='enter' or code.startswith('0x')
  call('shell','run-as',PKG,'sh','-c',f"'echo {code} > {run}/interactive-key.tmp && mv {run}/interactive-key.tmp {run}/interactive-key.txt'")
  screen=wait(lambda:(s if expected in (s:=read(run+'/screen.txt')) else None),label)
  (a.output/(label+'.txt')).write_text(screen)
 key('enter','Main Menu','01-main')
 key('0x18','Model Year(s)','02-year')
 key('0x01','Main Menu','02-back')
 key('0x18','Model Year(s)','02-year-again')
 for n in range(2,7):key('0x0c',str(n)+' / 15','03-year-'+str(n))
 key('enter','Vehicle Type(s)','04-model')
 key('0x0c','2 / 2','05-sport')
 key('enter','F0: Engine','06-system')
 key('0x18','Customer Functions','07-function')
 assert not json_read(run+'/candi-startup.json'),'CANdi started before communication'
 key('enter','Checking Key Position','08-communication')
 wait(lambda:'CANDI_GUEST_INITIALIZED:' in read(run+'/console.log'),'guest CANdi initialization')
 wait(lambda:'backend absent' in read(run+'/tech2.log'),'first CAN wake-up with absent backend')
 for name in ['console.log','tech2.log','candi-startup.json','startup.json','report.json','screen.txt']:
  (a.output/name).write_text(read(run+'/'+name))
 (a.output/'after-connection.png').write_bytes(call('exec-out','screencap','-p').stdout)
 print('PASS: CANdi deferred through menus, guest handshake complete, absent-adapter boundary reached',flush=True)
assert all(r['goal_met'] for r in results),results
print('PASS: every measured launch within 10 seconds',flush=True)
