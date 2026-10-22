#!/usr/bin/env python3
"""Build and test an isolated reporting harness on an already-running disposable AVD.
No native emulator, firmware, adapter access or live uploads. Never a release APK.
"""
import argparse, os, re, shutil, subprocess, zipfile, xml.etree.ElementTree as ET
import hashlib, json
from pathlib import Path
from report_dependencies import ReportDependencies
import importlib.util
p=argparse.ArgumentParser(description=__doc__)
p.add_argument('--serial',required=True)
p.add_argument('--lifecycle',action='store_true',help='Add real report-screen rotation/recreation/Home tests; use a phone-sized AVD')
p.add_argument('--crash-details-only',action='store_true')
p.add_argument('--evidence-only',action='store_true')
p.add_argument('--review-only',action='store_true')
p.add_argument('--security-only',action='store_true')
p.add_argument('--nano-reboot',action='store_true')
p.add_argument('--guided-context',action='store_true')
p.add_argument('--android-rounds',action='store_true',help='October 13 security handoff and session-end regressions')
p.add_argument('--session-safety',action='store_true',help='Bounded Stop and security continuation Java regressions')
p.add_argument('--direct-bluetooth',action='store_true',help='Production SPP binding against synthetic endpoints only')
p.add_argument('--mx-security-handoff',action='store_true',help='Production loopback/security parked transport against synthetic SPP only')
p.add_argument('--connection-core','--workflow-worker',dest='connection_core',type=Path,help='Exact matching Android connection worker for direct/parked transport fixtures')
p.add_argument('--output-dir',type=Path,help='Retain fixture artifacts/logs outside the checkout')
p.add_argument('--test-version',help='Distinct full test-only version for a changed fixture attempt')
a=p.parse_args()
if not a.serial.startswith('emulator-'):raise SystemExit('Disposable Android emulator required')
if (a.direct_bluetooth or a.mx_security_handoff) and (a.connection_core is None or not a.connection_core.is_file()):raise SystemExit('Direct/parked fixtures require --workflow-worker')
if a.connection_core and not (a.direct_bluetooth or a.mx_security_handoff):raise SystemExit('--workflow-worker is only for direct/parked fixtures')
if a.test_version and (not re.fullmatch(r'\d+\.\d+\.\d+-[a-z0-9.-]+',a.test_version) or 'tests' not in a.test_version):raise SystemExit('Use a full test-only prerelease version')
r=Path(__file__).resolve().parents[2];sdk=Path.home()/'Library/Android/sdk';bt=sdk/'build-tools/36.0.0'
jdk=Path(os.environ.get('JAVA_HOME','/Applications/Android Studio.app/Contents/jbr/Contents/Home'));jar=sdk/'platforms/android-36/android.jar'
env=dict(os.environ,JAVA_HOME=str(jdk),PATH=str(jdk/'bin')+os.pathsep+os.environ['PATH'])
out=a.output_dir or r/'target/beta-diagnostics-harness'
if a.output_dir and out.exists() and any(out.iterdir()):raise SystemExit('Use a fresh output directory to preserve earlier fixture evidence')
out.mkdir(parents=True,exist_ok=True);classes=out/'classes';shutil.rmtree(classes,ignore_errors=True);classes.mkdir()
adb=[sdk/'platform-tools/adb','-s',a.serial];package='com.opensaab.beta.diagnostics'
def run(*args,capture=False):
 return subprocess.run([str(x) for x in args],check=True,env=env,timeout=90,capture_output=capture,text=True)
suites=['SupportReportInstrumentedTest','EmulatorHealthInstrumentedTest','SecurityAccessInstrumentedTest','SecuritySessionInstrumentedTest']
if a.lifecycle:suites.append('ReportLifecycleInstrumentedTest')
suites.extend(['ReportReviewInstrumentedTest','NativeLcdPumpInstrumentedTest','SessionEndInstrumentedTest'])
if a.crash_details_only:suites=['CrashDetailsInstrumentedTest']
if a.evidence_only:suites=['DiagnosticEvidenceInstrumentedTest']
if a.review_only:suites=['ReportReviewInstrumentedTest']
if a.security_only or a.nano_reboot or a.guided_context:suites=['SecurityAccessInstrumentedTest','SecuritySessionInstrumentedTest','SecurityCollectionInstrumentedTest']
if a.nano_reboot:suites.append('NanoRebootInstrumentedTest')
if a.guided_context:suites.append('SecurityGuidedContextInstrumentedTest')
if a.android_rounds:suites=['SecurityAccessInstrumentedTest','SecuritySessionInstrumentedTest','SecurityCollectionInstrumentedTest','SecurityGuidedContextInstrumentedTest','NanoSecurityResumeInstrumentedTest','NanoRebootInstrumentedTest','SessionEndInstrumentedTest','DiagnosticEvidenceInstrumentedTest']
if a.session_safety:suites=['SessionSafetyInstrumentedTest','SecurityAccessInstrumentedTest']
if a.direct_bluetooth:suites=['DirectBluetoothInstrumentedTest']
if a.mx_security_handoff:suites=(suites if a.direct_bluetooth else [])+['MxSecurityHandoffInstrumentedTest']
sources=[r/'android/tech2-app/com/opensaab/tech2/MainActivity.java',*sorted((r/'android/shared').rglob('*.java')),*sorted((r/'android/adapters').rglob('*.java')),*[r/'android/tests'/(n+'.java') for n in suites+['DiagnosticFailureTest','ChipsoftIdentityTest']]]
source_hashes={str(path.relative_to(r)):hashlib.sha256(path.read_bytes()).hexdigest() for path in sources}
for path in sources:
 target=out/'tested-source'/path.relative_to(r);target.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(path,target)
deps=ReportDependencies(r,out,env)
ns='http://schemas.android.com/apk/res/android';ET.register_namespace('android',ns)
tree=ET.parse(r/'android/tech2-app/AndroidManifest.xml');root=tree.getroot();root.set('package',package)
test_version='0.1.0-guided-context-tests.2' if a.guided_context else '0.1.0-nano-reboot-tests.1' if a.nano_reboot else '0.1.0-security-tests.4' if a.security_only else '0.1.0-chipsoft-guidance-tests.1' if a.crash_details_only else '0.1.0-evidence-tests.2' if a.evidence_only else '0.1.0-report-tests.2'
if a.android_rounds:test_version='0.1.0-android-rounds-tests.3'
if a.session_safety:test_version='0.1.0-session-safety-tests.6'
if a.direct_bluetooth:test_version='0.1.0-direct-bluetooth-tests.6'
if a.mx_security_handoff:test_version='0.1.0-mx-security-handoff-tests.3'
if a.test_version:test_version=a.test_version
root.set('{'+ns+'}versionName',test_version);root.set('{'+ns+'}versionCode','1')
app=root.find('application');
if a.nano_reboot or a.android_rounds:app.set('{'+ns+'}debuggable','false')
app.set('{'+ns+'}label','OpenSAAB '+test_version+' ARM64 TEST ONLY')
for e in app.findall('activity'):
 if e.get('{'+ns+'}name')=='com.opensaab.usb.SupportReportActivity':e.set('{'+ns+'}exported','true') # Isolated harness only.
 if e.get('{'+ns+'}name')=='.MainActivity':e.set('{'+ns+'}name','com.opensaab.tech2.MainActivity')
for e in app.findall('provider'):e.set('{'+ns+'}authorities',e.get('{'+ns+'}authorities').replace('com.opensaab.tech2',package))
for t in suites:ET.SubElement(root,'instrumentation',{'{'+ns+'}name':'com.opensaab.usb.'+t,'{'+ns+'}targetPackage':package})
tree.write(out/'AndroidManifest.xml',encoding='utf-8',xml_declaration=True)
deps.package(bt,jar,out/'AndroidManifest.xml',r/'android/tech2-app/res',out/'unsigned.apk')
deps.compile(jdk,jar,classes,sources)
if source_hashes!={str(path.relative_to(r)):hashlib.sha256(path.read_bytes()).hexdigest() for path in sources}:raise SystemExit('Source changed during fixture compilation; preserve this attempt and rebuild with a new version')
for test in ['DiagnosticFailureTest','ChipsoftIdentityTest']:run(jdk/'bin/java','-cp',classes,'com.opensaab.usb.'+test)
deps.dex(bt,jar,classes)
with zipfile.ZipFile(out/'unsigned.apk','a') as z:
 if a.direct_bluetooth or a.mx_security_handoff:z.write(a.connection_core,'lib/arm64-v8a/libopensaab_connection.so')
 if a.crash_details_only:
  import json
  z.write(r/'target/aarch64-linux-android/release/tech2-emu','lib/arm64-v8a/libtech2_emu.so')
  z.writestr('assets/build.json',json.dumps({'architecture':'arm64-v8a','source_commit':subprocess.check_output(['git','-C',r,'rev-parse','HEAD'],text=True).strip()}))
 deps.add_runtime_resources(z)
 for dex in sorted(out.glob('classes*.dex')):z.write(dex,dex.name)
run(bt/'zipalign','-f','4',out/'unsigned.apk',out/'aligned.apk')
run(bt/'apksigner','sign','--ks',Path.home()/'.android/debug.keystore','--ks-pass','pass:android','--key-pass','pass:android','--out',out/'diagnostics-test-only.apk',out/'aligned.apk')
test_apk=out/('OpenSAAB-'+test_version+'-android-arm64-v8a-test.apk')
shutil.copy2(out/'diagnostics-test-only.apk',test_apk)
run(*adb,'install','-r',test_apk)
try:
 installed=run(*adb,'shell','pm','path',package,capture=True).stdout.strip().removeprefix('package:')
 run(*adb,'pull',installed,out/'installed-test.apk')
 apk_hash=hashlib.sha256(test_apk.read_bytes()).hexdigest()
 if apk_hash!=hashlib.sha256((out/'installed-test.apk').read_bytes()).hexdigest():raise SystemExit('Installed fixture APK differs')
 manifest=run(bt/'aapt','dump','xmltree',out/'installed-test.apk','AndroidManifest.xml',capture=True).stdout
 (out/'installed-manifest.txt').write_text(manifest)
 if 'OpenSAAB '+test_version+' ARM64 TEST ONLY' not in manifest:raise SystemExit('Installed fixture identity differs')
 identity={'version':test_version,'architecture':'arm64-v8a','apk_sha256':apk_hash,'compiled_source_sha256':source_hashes,'suites':suites,'result':'pending'}
 if a.connection_core:identity['workflow_worker_sha256']=hashlib.sha256(a.connection_core.read_bytes()).hexdigest()
 (out/'fixture-identity.json').write_text(json.dumps(identity,indent=2)+'\n')
 for suite in suites:
  result=run(*adb,'shell','am','instrument','-w','-r',package+'/com.opensaab.usb.'+suite,capture=True)
  (out/(suite+'.txt')).write_text(result.stdout+'\n'+result.stderr)
  print(result.stdout)
  if 'PASS:' not in result.stdout or 'INSTRUMENTATION_CODE: -1' not in result.stdout:raise SystemExit(suite+' failed')
  if a.crash_details_only:
   with (out/'chipsoft-compatibility-test.png').open('wb') as image:
    subprocess.run([str(x) for x in [*adb,'exec-out','run-as',package,'cat','files/chipsoft-compatibility-test.png']],check=True,env=env,stdout=image)
 if a.lifecycle:
  spec=importlib.util.spec_from_file_location('review_process',r/'scripts/android/test-review-process.py');module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module);module.exercise(adb,out,env)
 identity['result']='pass';(out/'fixture-identity.json').write_text(json.dumps(identity,indent=2)+'\n')
finally:
 removed=run(*adb,'uninstall',package,capture=True);(out/'uninstall.txt').write_text(removed.stdout+'\n'+removed.stderr)
