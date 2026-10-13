#!/usr/bin/env python3
"""Build and test an isolated reporting harness on an already-running disposable AVD.
No native emulator, firmware, adapter access or live uploads. Never a release APK.
"""
import argparse, os, shutil, subprocess, zipfile, xml.etree.ElementTree as ET
from pathlib import Path
from report_dependencies import ReportDependencies
import importlib.util
p=argparse.ArgumentParser(description=__doc__)
p.add_argument('--serial',required=True)
p.add_argument('--lifecycle',action='store_true',help='Add real report-screen rotation/recreation/Home tests; use a phone-sized AVD')
a=p.parse_args()
if not a.serial.startswith('emulator-'):raise SystemExit('Disposable Android emulator required')
r=Path(__file__).resolve().parents[2];sdk=Path.home()/'Library/Android/sdk';bt=sdk/'build-tools/36.0.0'
jdk=Path(os.environ.get('JAVA_HOME','/Applications/Android Studio.app/Contents/jbr/Contents/Home'));jar=sdk/'platforms/android-36/android.jar'
env=dict(os.environ,JAVA_HOME=str(jdk),PATH=str(jdk/'bin')+os.pathsep+os.environ['PATH'])
out=r/'target/beta-diagnostics-harness';out.mkdir(parents=True,exist_ok=True);classes=out/'classes';shutil.rmtree(classes,ignore_errors=True);classes.mkdir()
adb=[sdk/'platform-tools/adb','-s',a.serial];package='com.opensaab.beta.diagnostics'
def run(*args,capture=False):
 return subprocess.run([str(x) for x in args],check=True,env=env,timeout=90,capture_output=capture,text=True)
suites=['SupportReportInstrumentedTest','EmulatorHealthInstrumentedTest','SecurityAccessInstrumentedTest','SecuritySessionInstrumentedTest']
if a.lifecycle:suites.append('ReportLifecycleInstrumentedTest')
suites.extend(['ReportReviewInstrumentedTest','NativeLcdPumpInstrumentedTest','SessionEndInstrumentedTest'])
sources=[r/'android/tech2-app/com/opensaab/tech2/MainActivity.java',*sorted((r/'android/shared').rglob('*.java')),*sorted((r/'android/adapters').rglob('*.java')),*[r/'android/tests'/(n+'.java') for n in suites+['DiagnosticFailureTest','ChipsoftIdentityTest']]]
deps=ReportDependencies(r,out,env)
ns='http://schemas.android.com/apk/res/android';ET.register_namespace('android',ns)
tree=ET.parse(r/'android/tech2-app/AndroidManifest.xml');root=tree.getroot();root.set('package',package)
root.set('{'+ns+'}versionName','diagnostics-test-only');root.set('{'+ns+'}versionCode','1')
app=root.find('application');app.set('{'+ns+'}label','OpenSAAB diagnostics TEST ONLY')
for e in app.findall('activity'):
 if e.get('{'+ns+'}name')=='com.opensaab.usb.SupportReportActivity':e.set('{'+ns+'}exported','true') # Isolated harness only.
 if e.get('{'+ns+'}name')=='.MainActivity':e.set('{'+ns+'}name','com.opensaab.tech2.MainActivity')
for e in app.findall('provider'):e.set('{'+ns+'}authorities',e.get('{'+ns+'}authorities').replace('com.opensaab.tech2',package))
for t in suites:ET.SubElement(root,'instrumentation',{'{'+ns+'}name':'com.opensaab.usb.'+t,'{'+ns+'}targetPackage':package})
tree.write(out/'AndroidManifest.xml',encoding='utf-8',xml_declaration=True)
deps.package(bt,jar,out/'AndroidManifest.xml',r/'android/tech2-app/res',out/'unsigned.apk')
deps.compile(jdk,jar,classes,sources)
for test in ['DiagnosticFailureTest','ChipsoftIdentityTest']:run(jdk/'bin/java','-cp',classes,'com.opensaab.usb.'+test)
deps.dex(bt,jar,classes)
with zipfile.ZipFile(out/'unsigned.apk','a') as z:
 deps.add_runtime_resources(z)
 for dex in sorted(out.glob('classes*.dex')):z.write(dex,dex.name)
run(bt/'zipalign','-f','4',out/'unsigned.apk',out/'aligned.apk')
run(bt/'apksigner','sign','--ks',Path.home()/'.android/debug.keystore','--ks-pass','pass:android','--key-pass','pass:android','--out',out/'diagnostics-test-only.apk',out/'aligned.apk')
run(*adb,'install','-r',out/'diagnostics-test-only.apk')
try:
 for suite in suites:
  result=run(*adb,'shell','am','instrument','-w','-r',package+'/com.opensaab.usb.'+suite,capture=True)
  (out/(suite+'.txt')).write_text(result.stdout+'\n'+result.stderr)
  print(result.stdout)
  if 'PASS:' not in result.stdout or 'INSTRUMENTATION_CODE: -1' not in result.stdout:raise SystemExit(suite+' failed')
 if a.lifecycle:
  spec=importlib.util.spec_from_file_location('review_process',r/'scripts/android/test-review-process.py');module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module);module.exercise(adb,out,env)
finally:run(*adb,'uninstall',package)
