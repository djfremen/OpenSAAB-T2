#!/usr/bin/env python3
"""Build the Android firmware LCD/keypad app with installed SDK/JDK tools."""
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
from report_dependencies import ReportDependencies

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--release', action='store_true')
parser.add_argument('--private-release-signing', action='store_true', help='Sign a private debuggable build with the existing app certificate')
parser.add_argument('--version-name', default='0.1-dev')
parser.add_argument('--version-code', type=int, default=1)
parser.add_argument('--profile', choices=PROFILES, default='arm64')
args = parser.parse_args()
profile = PROFILES[args.profile]
experimental = profile.abi == 'armeabi-v7a'
demand_test = args.profile == 'headunit-arm32-test'
if demand_test and args.release: parser.error('The demand-start integration profile is development-only')
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
if demand_test: build = repo / 'target/android-headunit32-interactive-test'
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

mdi_module = Path(os.environ['OPENSAAB_MDI_MODULE']) if os.environ.get('OPENSAAB_MDI_MODULE') else None
mdi_bridge = Path(os.environ['OPENSAAB_MDI_BRIDGE']) if os.environ.get('OPENSAAB_MDI_BRIDGE') else None
if (mdi_module is None)!=(mdi_bridge is None): raise SystemExit('MDI requires both packaged module and firmware bridge')
if mdi_module is not None:
    if profile.abi != 'arm64-v8a': raise SystemExit('MDI module qualification currently covers ARM64 only')
    for binary in (mdi_module,mdi_bridge):validate_elf(binary.read_bytes()[:20],profile)
connection_core = None
if profile.abi == 'arm64-v8a':
    # Private picker-only build reuses the exact preview.56 native payloads.
    if os.environ.get('OPENSAAB_PINNED_NATIVE') != 'preview56':
        run('sh', repo / 'scripts/android/build-vlinker-workflow.sh')
    connection_core = Path(os.environ.get('OPENSAAB_VLINKER_CONNECTION_CORE') or os.environ.get('OPENSAAB_SIMULATOR_CONNECTION_CORE') or repo / 'target/vlinker-workflow/aarch64-linux-android/release/opensaab-connection')
    validate_elf(connection_core.read_bytes()[:20], profile)
deps = ReportDependencies(repo, build, env)
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
        # Follow the installed display on fixed-landscape head units. ARM64 stays unchanged.
        activity.attrib.pop('{'+ns+'}screenOrientation', None)
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
deps.package(bt, android_jar, staged_manifest, source / "res", unsigned)
deps.compile(jdk, android_jar, classes, [source / 'com/opensaab/tech2/MainActivity.java', *sorted((repo / 'android/shared').rglob('*.java')), *sorted((repo / 'android/adapters').rglob('*.java'))])
deps.dex(bt, android_jar, classes)
with zipfile.ZipFile(unsigned, 'a') as z:
    deps.add_runtime_resources(z)
    if args.release:
        if subprocess.check_output(['git','status','--porcelain'],cwd=repo,text=True).strip():
            raise SystemExit('Official release builds require a clean source checkout')
        commit = subprocess.check_output(['git','rev-parse','HEAD'],cwd=repo,text=True).strip()
        z.writestr('assets/build.json', json.dumps({'source_repository':'https://github.com/djfremen/OpenSAAB-T2','source_commit':commit,'version_name':args.version_name,'version_code':args.version_code,'source_license':'MPL-2.0 AND LGPL-3.0-only'},sort_keys=True))
    for dex in sorted(build.glob('classes*.dex')):z.write(dex, dex.name)
    for name in ('LICENSE', 'LICENSING.md', 'THIRD_PARTY_NOTICES.md', 'licenses/ANDROID_CARGO_NOTICES.txt', 'licenses/ANDROID_BLUETOOTH_CARGO_NOTICES.txt', 'licenses/ANDROIDX_APACHE_2_0.txt', 'licenses/OPENVCX_LGPL_3_0.txt', 'licenses/OPENVCX_GPL_3_0.txt', 'licenses/OPENVCX_SOURCE_ORIGIN.txt'):
        z.write(repo / name, 'assets/legal/' + Path(name).name, compress_type=zipfile.ZIP_DEFLATED)
    z.write(support_manifest, 'assets/system/manifest.json', compress_type=zipfile.ZIP_DEFLATED)
    for name, spec in (support.items() if bundle_support else []):
        z.write(support_root / spec['source'], 'assets/system/' + name, compress_type=zipfile.ZIP_DEFLATED)
    # Package the standalone Rust ELF as an extracted native executable; all
    # executable code is shipped in the APK, never downloaded into app data.
    z.write(emulator, f'lib/{profile.abi}/libtech2_emu.so')
    z.write(probe, f'lib/{profile.abi}/libnano_probe.so')
    z.write(chipsoft, f'lib/{profile.abi}/libchipsoft_probe.so')
    if connection_core is not None:z.write(connection_core, 'lib/arm64-v8a/libopensaab_connection.so')
    if mdi_module is not None:
        z.write(mdi_module,'lib/arm64-v8a/libopensaab_mdi_android.so')
        z.write(mdi_bridge,'lib/arm64-v8a/libtech2_mdi.so')
        z.write(repo/'android/adapters/mdi/MDI_NOTICES.txt','assets/legal/MDI_NOTICES.txt')
aligned = build / 'aligned.apk'
run(bt / 'zipalign', '-f', '4', unsigned, aligned)
apk = build / ('OpenSAAB-T2-arm64-v8a.apk' if args.release else 'opensaab-tech2.apk')
if experimental:
    apk = build / ('OpenSAAB-T2-headunit-armeabi-v7a.apk' if args.release else 'OpenSAAB-T2-headunit-armeabi-v7a-dev.apk')
if demand_test: apk = build / 'OpenSAAB-32-bit-interactive-test.apk'
if args.release:
    keystore = os.environ.get('OPENSAAB_RELEASE_KEYSTORE')
    password_file = os.environ.get('OPENSAAB_RELEASE_PASSWORD_FILE')
    if not keystore or not password_file or not Path(keystore).is_file() or not Path(password_file).is_file():
        raise SystemExit('Set OPENSAAB_RELEASE_KEYSTORE and OPENSAAB_RELEASE_PASSWORD_FILE; release signing never falls back to the debug key.')
    run(bt / 'apksigner', 'sign', '--ks', keystore, '--ks-key-alias', 'opensaab-release',
        '--ks-pass', 'file:'+password_file, '--out', apk, aligned)
else:
    if args.private_release_signing:
        run(bt / 'apksigner', 'sign', '--ks', Path.home() / '.local/share/opensaab/release-signing/opensaab-release.p12',
            '--ks-key-alias', 'opensaab-release', '--ks-pass', 'file:'+str(Path.home() / '.local/share/opensaab/release-signing/password.txt'), '--out', apk, aligned)
    else:
        run(bt / 'apksigner', 'sign', '--ks', Path.home() / '.android/debug.keystore',
            '--ks-pass', 'pass:android', '--key-pass', 'pass:android', '--out', apk, aligned)
run(bt / 'apksigner', 'verify', apk)
run('python3', repo / 'scripts/android/check-apk-firmware.py', apk, '--profile', args.profile, *(['--allow-bundled-support'] if bundle_support else []), *(['--connection-core-sha256', hashlib.sha256(connection_core.read_bytes()).hexdigest()] if connection_core else []), *(['--mdi-module-sha256', hashlib.sha256(mdi_module.read_bytes()).hexdigest(), '--mdi-bridge-sha256', hashlib.sha256(mdi_bridge.read_bytes()).hexdigest()] if mdi_module else []))
print(apk)
