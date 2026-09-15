#!/usr/bin/env python3
"""Stage only approved support files and locally built Rust executables for Gradle."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import sys
import xml.etree.ElementTree as ET

repo = Path(__file__).resolve().parents[2]
output = Path(sys.argv[1])
bundle_support = '--bundle-support' in sys.argv[2:]
support_root = Path(os.environ.get('OPENSAAB_SUPPORT_ROOT', str(repo)))
manifest_path = repo / 'android/tech2-app/support-files.json'
support = json.loads(manifest_path.read_text())
for name, spec in (support.items() if bundle_support else []):
    data = (support_root / spec['source']).read_bytes()
    if len(data) != spec['bytes'] or hashlib.sha256(data).hexdigest() != spec['sha256']:
        raise SystemExit('Unapproved support input: ' + name)
for part in ('assets', 'jniLibs'):
    shutil.rmtree(output / part, ignore_errors=True)
assets = output / 'assets/system'
assets.mkdir(parents=True, exist_ok=True)
legal = output / 'assets/legal'
legal.mkdir(parents=True, exist_ok=True)
for name in ('LICENSE', 'LICENSING.md', 'THIRD_PARTY_NOTICES.md', 'licenses/ANDROID_CARGO_NOTICES.txt'):
    shutil.copy2(repo / name, legal / Path(name).name)
shutil.copy2(manifest_path, assets / 'manifest.json')
for name, spec in (support.items() if bundle_support else []):
    shutil.copy2(support_root / spec['source'], assets / name)
native = output / 'jniLibs/arm64-v8a'
native.mkdir(parents=True, exist_ok=True)
for source, dest in [('tech2-emu', 'libtech2_emu.so'), ('nano-usb-probe', 'libnano_probe.so'), ('chipsoft-usb-probe', 'libchipsoft_probe.so')]:
    shutil.copy2(repo / 'target/aarch64-linux-android/release' / source, native / dest)
# Keep the existing manifest authoritative; SDK/debug attributes belong to AGP.
ET.register_namespace('android', 'http://schemas.android.com/apk/res/android')
tree = ET.parse(repo / 'android/tech2-app/AndroidManifest.xml')
root = tree.getroot()
root.attrib.pop('package', None)
for sdk in root.findall('uses-sdk'):
    root.remove(sdk)
root.find('application').attrib.pop('{http://schemas.android.com/apk/res/android}debuggable', None)
root.find('application').attrib.pop('{http://schemas.android.com/apk/res/android}extractNativeLibs', None)
tree.write(output / 'AndroidManifest.xml', encoding='utf-8', xml_declaration=True)
print('Staged ARM64 executables and support metadata; ' + ('three OEM support files included by explicit development option.' if bundle_support else 'no OEM firmware bundled.'))
