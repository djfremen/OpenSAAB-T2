#!/usr/bin/env python3
"""Build the dependency-free Android firmware LCD/keypad app with installed SDK/JDK tools."""
import os
import json
import hashlib
import shutil
from pathlib import Path
import subprocess
import zipfile
import argparse
import re
import xml.etree.ElementTree as ET
from build_profiles import PROFILES, validate_elf

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--release', action='store_true')
parser.add_argument('--version-name', default='0.1-dev')
parser.add_argument('--version-code', type=int, default=1)
parser.add_argument('--profile', choices=PROFILES, default='arm64')
args = parser.parse_args()
profile = PROFILES[args.profile]
experimental = args.profile == 'headunit-arm32'
if experimental and not args.release:
    args.version_name = '0.1-headunit-arm32-dev'
if args.version_code < 1 or args.version_code > 2100000000:
    parser.error('version-code must be between 1 and 2100000000')
if args.release and (args.version_name == '0.1-dev' or args.version_code == 1):
    parser.error('Release builds require explicit version-name and version-code')
if args.release:
    pattern = r'(\d{1,2})\.(\d{1,2})\.(\d{1,2})-headunit\.([1-9][0-9]?)' if experimental else r'(\d{1,2})\.(\d{1,2})\.(\d{1,2})(?:-preview\.([1-9][0-9]?))?'
    version = re.fullmatch(pattern, args.version_name)
    if not version:
        parser.error('Head-unit release requires MAJOR.MINOR.PATCH-headunit.N' if experimental else 'Release version must be MAJOR.MINOR.PATCH or MAJOR.MINOR.PATCH-preview.N')
    major, minor, patch = map(int, version.groups()[:3])
    expected = major * 10000000 + minor * 100000 + patch * 1000 + (int(version[4]) if version[4] else 999)
    if args.version_code != expected:
        parser.error(f'version-code must be {expected} for {args.version_name}; keep update ordering consistent')

repo = Path(__file__).resolve().parents[2]
sdk = Path(os.environ.get('ANDROID_HOME', str(Path.home() / 'Library/Android/sdk')))
jdk = Path(os.environ.get('JAVA_HOME', '/Applications/Android Studio.app/Contents/jbr/Contents/Home'))
bt = sdk / 'build-tools/36.0.0'
android_jar = sdk / 'platforms/android-36/android.jar'
source = repo / 'android/tech2-app'
support_manifest = source / 'support-files.json'
support = json.loads(support_manifest.read_text())
bundle_support = os.environ.get('OPENSAAB_BUNDLE_SUPPORT', '1') == '1'
support_root = Path(os.environ.get('OPENSAAB_SUPPORT_ROOT', str(repo)))
for name, spec in (support.items() if bundle_support else []):
    data = (support_root / spec['source']).read_bytes()
    if len(data) != spec['bytes'] or hashlib.sha256(data).hexdigest() != spec['sha256']:
        raise SystemExit(f'Bundled support file does not match the approved build profile: {name}')
build = repo / ('target/android-tech2-release' if args.release else 'target/android-tech2-app')
if experimental:
    build = repo / ('target/android-headunit-arm32-release' if args.release else 'target/android-headunit-arm32')
classes = build / 'classes'
shutil.rmtree(classes, ignore_errors=True)
classes.mkdir(parents=True, exist_ok=True)
native_root = repo / 'target' / profile.rust_target / 'release'
emulator = native_root / 'tech2-emu'
probe = native_root / 'nano-usb-probe'
chipsoft = native_root / 'chipsoft-usb-probe'
if not emulator.is_file() or not probe.is_file() or not chipsoft.is_file():
    raise SystemExit('Build the Android Rust emulator with build-headless.sh first.')
for executable in (emulator, probe, chipsoft):
    with executable.open('rb') as stream:
        validate_elf(stream.read(20), profile)
env = dict(os.environ, JAVA_HOME=str(jdk), PATH=str(jdk / 'bin') + os.pathsep + os.environ['PATH'])

def run(*args):
    subprocess.run([str(x) for x in args], check=True, env=env)

run(jdk / 'bin/javac', '-source', '8', '-target', '8', '-classpath', android_jar,
    '-d', classes, source / 'com/opensaab/tech2/MainActivity.java', *sorted((repo / 'android/shared/com/opensaab/usb').glob('*.java')))
run(bt / 'd8', '--min-api', '26', '--lib', android_jar, '--output', build,
    *sorted(classes.rglob('*.class')))
unsigned = build / 'unsigned.apk'
ns = 'http://schemas.android.com/apk/res/android'
ET.register_namespace('android', ns)
manifest = ET.parse(source / 'AndroidManifest.xml')
if experimental:
    manifest.getroot().set('package', profile.package)
    app = manifest.getroot().find('application')
    app.set('{'+ns+'}label', profile.label)
    ET.SubElement(app, 'meta-data', {'{'+ns+'}name': 'com.opensaab.build_profile',
                                    '{'+ns+'}value': args.profile})
    for activity in app.findall('activity'):
        if activity.get('{'+ns+'}name') == '.MainActivity':
            activity.set('{'+ns+'}name', 'com.opensaab.tech2.MainActivity')
    for provider in app.findall('provider'):
        key = '{'+ns+'}authorities'
        provider.set(key, provider.get(key).replace('com.opensaab.tech2.', profile.package+'.'))
manifest.getroot().set('{'+ns+'}versionName', args.version_name)
manifest.getroot().set('{'+ns+'}versionCode', str(args.version_code))
manifest.getroot().find('application').set('{'+ns+'}debuggable', 'false' if args.release else 'true')
staged_manifest = build / 'AndroidManifest.xml'
manifest.write(staged_manifest, encoding='utf-8', xml_declaration=True)
run(bt / 'aapt', 'package', '-f', '-M', staged_manifest, '-S', source / 'res', '-I', android_jar, '-F', unsigned)
with zipfile.ZipFile(unsigned, 'a') as z:
    if args.release:
        if subprocess.check_output(['git','status','--porcelain'],cwd=repo,text=True).strip():
            raise SystemExit('Official release builds require a clean source checkout')
        commit = subprocess.check_output(['git','rev-parse','HEAD'],cwd=repo,text=True).strip()
        z.writestr('assets/build.json', json.dumps({'source_repository':'https://github.com/djfremen/OpenSAAB-T2','source_commit':commit,'version_name':args.version_name,'version_code':args.version_code,'source_license':'MPL-2.0'},sort_keys=True))
    z.write(build / 'classes.dex', 'classes.dex')
    for name in ('LICENSE', 'LICENSING.md', 'THIRD_PARTY_NOTICES.md', 'licenses/ANDROID_CARGO_NOTICES.txt'):
        z.write(repo / name, 'assets/legal/' + Path(name).name, compress_type=zipfile.ZIP_DEFLATED)
    z.write(support_manifest, 'assets/system/manifest.json', compress_type=zipfile.ZIP_DEFLATED)
    for name, spec in (support.items() if bundle_support else []):
        z.write(support_root / spec['source'], 'assets/system/' + name, compress_type=zipfile.ZIP_DEFLATED)
    # Package the standalone Rust ELF as an extracted native executable; all
    # executable code is shipped in the APK, never downloaded into app data.
    z.write(emulator, f'lib/{profile.abi}/libtech2_emu.so')
    z.write(probe, f'lib/{profile.abi}/libnano_probe.so')
    z.write(chipsoft, f'lib/{profile.abi}/libchipsoft_probe.so')
aligned = build / 'aligned.apk'
run(bt / 'zipalign', '-f', '4', unsigned, aligned)
apk = build / ('OpenSAAB-T2-arm64-v8a.apk' if args.release else 'opensaab-tech2.apk')
if experimental:
    apk = build / ('OpenSAAB-T2-headunit-armeabi-v7a.apk' if args.release else 'OpenSAAB-T2-headunit-armeabi-v7a-dev.apk')
if args.release:
    keystore = os.environ.get('OPENSAAB_RELEASE_KEYSTORE')
    password_file = os.environ.get('OPENSAAB_RELEASE_PASSWORD_FILE')
    if not keystore or not password_file or not Path(keystore).is_file() or not Path(password_file).is_file():
        raise SystemExit('Set OPENSAAB_RELEASE_KEYSTORE and OPENSAAB_RELEASE_PASSWORD_FILE; release signing never falls back to the debug key.')
    run(bt / 'apksigner', 'sign', '--ks', keystore, '--ks-key-alias', 'opensaab-release',
        '--ks-pass', 'file:'+password_file, '--out', apk, aligned)
else:
    run(bt / 'apksigner', 'sign', '--ks', Path.home() / '.android/debug.keystore',
        '--ks-pass', 'pass:android', '--key-pass', 'pass:android', '--out', apk, aligned)
run(bt / 'apksigner', 'verify', apk)
run('python3', repo / 'scripts/android/check-apk-firmware.py', apk, '--profile', args.profile, *(['--allow-bundled-support'] if bundle_support else []))
print(apk)
