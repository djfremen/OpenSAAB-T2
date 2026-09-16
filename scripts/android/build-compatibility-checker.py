#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""Build the Java-only, firmware-free Setup app. Release signing is mandatory."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import shutil
import zipfile

repo = Path(__file__).resolve().parents[2]
sdk = Path(os.environ.get('ANDROID_HOME', str(Path.home() / 'Library/Android/sdk')))
jdk = Path(os.environ.get('JAVA_HOME', '/Applications/Android Studio.app/Contents/jbr/Contents/Home'))
bt = sdk / 'build-tools/36.0.0'
android = sdk / 'platforms/android-36/android.jar'
build = repo / 'target/compatibility-checker'
classes = build / 'classes'
shutil.rmtree(classes, ignore_errors=True)
classes.mkdir(parents=True, exist_ok=True)
source = repo / 'android/compatibility-checker'
shared = repo / 'android/shared/com/opensaab/usb'
key = os.environ.get('OPENSAAB_RELEASE_KEYSTORE')
password = os.environ.get('OPENSAAB_RELEASE_PASSWORD_FILE')
if not key or not password or not Path(key).is_file() or not Path(password).is_file():
    raise SystemExit('Set OPENSAAB_RELEASE_KEYSTORE and OPENSAAB_RELEASE_PASSWORD_FILE')
if subprocess.check_output(['git', 'status', '--porcelain'], cwd=repo, text=True).strip():
    raise SystemExit('Commit the checker source before building a signed distributable')
commit = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=repo, text=True).strip()
env = dict(os.environ, JAVA_HOME=str(jdk), PATH=str(jdk / 'bin') + os.pathsep + os.environ['PATH'])
def run(*args):
    subprocess.run([str(arg) for arg in args], check=True, env=env)
run(jdk/'bin/javac', '-source', '8', '-target', '8', '-classpath', android, '-d', classes,
    *sorted((source/'com/opensaab/checker').glob('*.java')),
    *[shared/name for name in ('CompatibilityCheck.java', 'DeviceCompatibility.java', 'BrandHeader.java', 'AppBuildProfile.java', 'InstallerChoice.java', 'EmulatorArchitecture.java')])
run(bt/'d8', '--min-api', '21', '--lib', android, '--output', build, *sorted(classes.rglob('*.class')))
unsigned = build/'unsigned.apk'
run(bt/'aapt', 'package', '-f', '-M', source/'AndroidManifest.xml', '-S', repo/'android/tech2-app/res', '-I', android, '-F', unsigned)
with zipfile.ZipFile(unsigned, 'a') as archive:
    archive.write(build/'classes.dex', 'classes.dex')
    archive.write(repo/'LICENSE', 'assets/legal/LICENSE')
    archive.writestr('assets/build.json', json.dumps({'source_commit':commit, 'source_repository':'https://github.com/djfremen/OpenSAAB-T2', 'product':'OpenSAAB Setup', 'version':'0.3.3'}))
aligned = build/'aligned.apk'
apk = build/'OpenSAAB-Setup.apk'
run(bt/'zipalign', '-f', '4', unsigned, aligned)
run(bt/'apksigner', 'sign', '--ks', key, '--ks-key-alias', 'opensaab-release', '--ks-pass', 'file:'+password, '--out', apk, aligned)
run(bt/'apksigner', 'verify', apk)
with zipfile.ZipFile(apk) as archive:
    assert not any(n.startswith('lib/') or n.endswith(('.bin','.dwn')) for n in archive.namelist())
(build/'SHA256SUMS').write_text(hashlib.sha256(apk.read_bytes()).hexdigest()+'  '+apk.name+'\n')
print(apk)
