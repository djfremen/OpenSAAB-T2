#!/usr/bin/env python3
"""Private-firmware integration check: cold boot, menu navigation, first CANdi use."""
import argparse,hashlib,json,os,re,subprocess,time
from pathlib import Path
p=argparse.ArgumentParser(description=__doc__)
p.add_argument('--native',type=Path,required=True);p.add_argument('--firmware',type=Path,required=True);p.add_argument('--output',type=Path,required=True)
a=p.parse_args();a.output.mkdir(parents=True,exist_ok=False)
def read(name):
 try:return (a.output/name).read_text()
 except FileNotFoundError:return ''
def wait(predicate,label,timeout=20):
 until=time.monotonic()+timeout
 while time.monotonic()<until:
  if predicate():return
  if child.poll() is not None:raise RuntimeError('Native process ended during '+label)
  time.sleep(.025)
 raise RuntimeError('Timeout '+label+'\n'+read('screen.txt')+'\n'+read('console.log')[-4000:])
def key(code,expected,label):
 wait(lambda:not (a.output/'interactive-key.txt').exists(),'mailbox')
 tmp=a.output/'interactive-key.tmp';tmp.write_text(code+'\n');tmp.replace(a.output/'interactive-key.txt')
 wait(lambda: expected in read('screen.txt'),label)
 (a.output/(label+'.txt')).write_text(read('screen.txt'))
 print(label,flush=True)
cmd=[str(a.native.resolve()),'--interactive-headless','--research-harness','--candi-native-link','--candi-on-demand','--candi-firmware',str(a.firmware/'candi.bin'),'--boot',str(a.firmware/'eprom.bin'),'--opsys',str(a.firmware/'opsys.dwn'),'--max-insns','50000000000','--output-dir',str(a.output),str(a.firmware/'card.bin')]
with (a.output/'console.log').open('w') as log:
 child=subprocess.Popen(cmd,env=dict(os.environ,LOAD_TEST_ACCELERATE='1'),stdout=log,stderr=log)
 try:
  wait(lambda:'verified_welcome' in read('startup.json'),'welcome')
  assert not json.loads(read('startup.json'))['candi_started']
  key('enter','Main Menu','01-main')
  key('0x18','Model Year(s)','02-year')
  key('0x01','Main Menu','02-back')
  key('0x18','Model Year(s)','02-year-again')
  for n in range(2,7):key('0x0c',str(n)+' / 15','03-year-'+str(n))
  key('enter','Vehicle Type(s)','04-model')
  key('0x0c','2 / 2','05-sport')
  key('enter','F0: Engine','06-system')
  key('0x18','Customer Functions','07-function')
  assert not (a.output/'candi-startup.json').exists(),'CANdi started before communication'
  key('enter','Checking Key Position','08-communication')
  wait(lambda:(a.output/'candi-startup.json').exists(),'first CANdi dependency',20)
  candi=json.loads(read('candi-startup.json'));assert candi['status']=='ready',candi
  print('CANdi first start',candi,flush=True)
  wait(lambda:child.poll() is not None,'offline CAN wake-up boundary',60)
  console=read('console.log'); trace=read('tech2.log')
  assert console.count('CANDI_STARTING:')==1, 'Initialization repeated'
  assert console.count('CANDI_GUEST_INITIALIZED:')==1, 'Guest handshake incomplete or repeated'
  assert 'No CANDI Communication Established' not in console
  assert 'CAN TX requested; backend absent' in trace, 'Did not reach the real absent-adapter boundary'
  frames=[re.search(r'\| ([0-9A-F ]+) \| virtual',line).group(1) for line in trace.splitlines() if 'Tech2 -> CANdi' in line]
  frames=frames[frames.index('8A 0B 00 6B'):]
  # Reference: eager initialization, same private Saab 9.250/MSI firmware.
  # All 79 commands through the first real CAN wake-up must match byte for byte.
  assert len(frames)==79, len(frames)
  digest=hashlib.sha256('\n'.join(frames).encode()).hexdigest()
  assert digest=='7f1355f548ec296c42884f9ead0bfff9134ab204c7f5b24b29f2ed4bc5d00c7d',digest
  assert 'Internal request: arg=0x00000007 key=0x00000a10' in trace
  (a.output/'comparison.json').write_text(json.dumps({'commands':len(frames),'sha256':digest,'matched_eager_reference':True,'external_adapter_connected':False},indent=2))
  print('PASS: all 79 communication commands match eager startup; no adapter attached',flush=True)
  (a.output/'09-after-connection.txt').write_text(read('screen.txt'))
 finally:
  (a.output/'interactive-key.txt').write_text('stop\n')
  try:child.wait(timeout=20)
  except subprocess.TimeoutExpired:child.terminate();child.wait(timeout=5)
