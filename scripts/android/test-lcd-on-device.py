#!/usr/bin/env python3
"""Real Android LCD observer test. Installs a temporary no-launcher test package;
opens no adapter and removes the package after collecting the result.
Run only with the production emulator stopped: instrumentation restarts its app.
"""
import argparse, os, pathlib, subprocess, tempfile, zipfile
p=argparse.ArgumentParser();p.add_argument('--serial',required=True);p.add_argument('--live-upload',action='store_true',help='Explicitly send one synthetic parity report to the project support endpoint');p.add_argument('--suite',choices=['lcd','keypad','security','dtc','firmware','firmware-download','support','vehicle','connection','updates','parity'],default='lcd');p.add_argument('--classes',type=pathlib.Path,help='Compiled application classes (defaults to the custom build)');p.add_argument('--package', choices=['com.opensaab.tech2','com.opensaab.tech2.headunit32'],default='com.opensaab.tech2');a=p.parse_args()
test_class={'lcd':'NativeLcdPumpInstrumentedTest','keypad':'Tech2ControlsInstrumentedTest','security':'SecurityAccessInstrumentedTest','dtc':'DtcReportInstrumentedTest','firmware':'FirmwareInstrumentedTest','firmware-download':'FirmwareDownloadInstrumentedTest','support':'SupportReportInstrumentedTest','vehicle':'VehicleSessionInstrumentedTest','connection':'ConnectionReportInstrumentedTest','updates':'AppUpdatesInstrumentedTest','parity':'ReleaseParityInstrumentedTest'}[a.suite]
if a.live_upload and a.suite!='parity':raise SystemExit('--live-upload is only supported for parity')
if a.suite in ('firmware-download','support','connection','parity') and not a.serial.startswith('emulator-'):raise SystemExit('This suite requires an Android emulator')
repo=pathlib.Path(__file__).resolve().parents[2]
sdk=pathlib.Path(os.environ.get('ANDROID_HOME',str(pathlib.Path.home()/'Library/Android/sdk')))
jdk=pathlib.Path(os.environ.get('JAVA_HOME','/Applications/Android Studio.app/Contents/jbr/Contents/Home'))
bt=sdk/'build-tools/36.0.0';jar=sdk/'platforms/android-36/android.jar';adb=['adb','-s',a.serial]
def run(*args,**kw):return subprocess.run(list(map(str,args)),check=True,timeout=240 if a.suite=='firmware-download' else 90,**kw)
active=subprocess.run(adb+['shell','pidof','libtech2_emu.so'],capture_output=True,text=True,check=False,timeout=10)
if active.stdout.strip():raise SystemExit('Stop the existing emulator before the LCD test')
with tempfile.TemporaryDirectory(prefix='opensaab-lcd-test-') as temp:
 d=pathlib.Path(temp);classes=d/'classes';classes.mkdir()
 (d/'AndroidManifest.xml').write_text(f'''<manifest xmlns:android="http://schemas.android.com/apk/res/android" package="com.opensaab.tech2.lcdtest"><uses-sdk android:minSdkVersion="26" android:targetSdkVersion="36"/><application android:label="OpenSAAB LCD test" android:debuggable="true"></application><instrumentation android:name="com.opensaab.usb.{test_class}" android:targetPackage="{a.package}"/></manifest>''')
 run(jdk/'bin/javac','-source','8','-target','8','-classpath',str(jar)+os.pathsep+str(a.classes or repo/'target/android-tech2-app/classes'),'-d',classes,repo/f'android/tests/{test_class}.java')
 env=dict(os.environ,JAVA_HOME=str(jdk),PATH=str(jdk/'bin')+os.pathsep+os.environ['PATH'])
 run(bt/'d8','--min-api','26','--lib',jar,'--output',d,*classes.rglob('*.class'),env=env)
 run(bt/'aapt','package','-f','-M',d/'AndroidManifest.xml','-I',jar,'-F',d/'unsigned.apk')
 with zipfile.ZipFile(d/'unsigned.apk','a') as z:z.write(d/'classes.dex','classes.dex')
 run(bt/'zipalign','-f','4',d/'unsigned.apk',d/'aligned.apk')
 release_key=os.environ.get('OPENSAAB_TEST_RELEASE_KEYSTORE')
 if release_key:
  if not a.serial.startswith('emulator-'):raise SystemExit('Release instrumentation is limited to a disposable Android emulator')
  password=os.environ.get('OPENSAAB_TEST_RELEASE_PASSWORD_FILE')
  if not password:raise SystemExit('Set OPENSAAB_TEST_RELEASE_PASSWORD_FILE')
  run(bt/'apksigner','sign','--ks',release_key,'--ks-key-alias','opensaab-release','--ks-pass','file:'+password,'--out',d/'test.apk',d/'aligned.apk',env=env)
 else:
  run(bt/'apksigner','sign','--ks',pathlib.Path.home()/'.android/debug.keystore','--ks-pass','pass:android','--key-pass','pass:android','--out',d/'test.apk',d/'aligned.apk',env=env)
 run(*adb,'install','-r',d/'test.apk')
 try:
  r=run(*adb,'shell','am','instrument','-w','-r',*(['-e','live_upload','true'] if a.live_upload else []),f'com.opensaab.tech2.lcdtest/com.opensaab.usb.{test_class}',capture_output=True,text=True)
  print(r.stdout)
  if 'PASS:' not in r.stdout or 'INSTRUMENTATION_CODE: -1' not in r.stdout:raise SystemExit(f'{a.suite} instrumentation failed')
 finally:run(*adb,'uninstall','com.opensaab.tech2.lcdtest')
