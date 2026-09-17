#!/usr/bin/env python3
"""Build the private, offline 32-bit cold-load test APK (never publishes)."""
import argparse, hashlib, json, os, shutil, subprocess, zipfile, zlib
from pathlib import Path
p=argparse.ArgumentParser(description=__doc__)
p.add_argument('--native', type=Path, required=True)
p.add_argument('--firmware-dir', type=Path, required=True)
a=p.parse_args()
r=Path(__file__).resolve().parents[2]
s=r/'android/load-test'
b=r/'target/android-load-test'
b.mkdir(parents=True,exist_ok=True)
sdk=Path(os.environ.get('ANDROID_HOME',str(Path.home()/'Library/Android/sdk')))
jdk=Path(os.environ.get('JAVA_HOME','/Applications/Android Studio.app/Contents/jbr/Contents/Home'))
bt=sdk/'build-tools/36.0.0'
jar=sdk/'platforms/android-36/android.jar'
env=dict(os.environ,JAVA_HOME=str(jdk),PATH=str(jdk/'bin')+os.pathsep+os.environ['PATH'])
def run(*args): subprocess.run(list(map(str,args)),env=env,check=True)
elf=a.native.read_bytes()[:20]
if elf[:5]!=b'\x7fELF\x01' or int.from_bytes(elf[18:20],'little')!=40: raise SystemExit('Expected ARM32 ELF')
classes=b/'classes'
shutil.rmtree(classes,ignore_errors=True)
classes.mkdir()
run(jdk/'bin/javac','-source','8','-target','8','-classpath',jar,'-d',classes,s/'com/opensaab/loadtest/MainActivity.java')
run(bt/'d8','--min-api','26','--lib',jar,'--output',b,*classes.rglob('*.class'))
unsigned=b/'unsigned.apk'
run(bt/'aapt','package','-f','-M',s/'AndroidManifest.xml','-I',jar,'-F',unsigned)
manifest={}
with zipfile.ZipFile(unsigned,'a') as z:
 z.write(b/'classes.dex','classes.dex')
 z.write(a.native,'lib/armeabi-v7a/libloadtest.so')
 for name in ('eprom.bin','opsys.dwn','card.bin'):
  f=a.firmware_dir/name
  data=f.read_bytes()
  manifest[name]={'bytes':len(data),'sha256':hashlib.sha256(data).hexdigest(),'crc32':zlib.crc32(data)}
  z.write(f,'assets/firmware/'+name,compress_type=zipfile.ZIP_DEFLATED)
 z.writestr('assets/firmware.json',json.dumps(manifest))
 for f in ('LICENSE','LICENSING.md','THIRD_PARTY_NOTICES.md','licenses/ANDROID_CARGO_NOTICES.txt'):
  z.write(r/f,'assets/legal/'+Path(f).name,compress_type=zipfile.ZIP_DEFLATED)
 run_id={'git_commit':subprocess.check_output(['git','rev-parse','HEAD'],cwd=r,text=True).strip(),'native_sha256':hashlib.sha256(a.native.read_bytes()).hexdigest(),'private_load_test':True}
 z.writestr('assets/build.json',json.dumps(run_id))
aligned=b/'aligned.apk'
apk=b/'32-bit-load-test.apk'
run(bt/'zipalign','-f','4',unsigned,aligned)
run(bt/'apksigner','sign','--ks',Path.home()/'.android/debug.keystore','--ks-pass','pass:android','--key-pass','pass:android','--out',apk,aligned)
run(bt/'apksigner','verify',apk)
print(apk)
