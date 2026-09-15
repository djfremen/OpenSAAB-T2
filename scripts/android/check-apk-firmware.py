#!/usr/bin/env python3
"""Reject bundled OEM firmware by default; explicit legacy development profile only."""
import argparse
import json
import hashlib
import zipfile
import re
from pathlib import PurePosixPath, Path

NATIVE = {'lib/arm64-v8a/libtech2_emu.so', 'lib/arm64-v8a/libnano_probe.so', 'lib/arm64-v8a/libchipsoft_probe.so'}
SUPPORT = json.loads((Path(__file__).resolve().parents[2] / 'android/tech2-app/support-files.json').read_text())
def check(path, allow_bundled_support=False):
    with zipfile.ZipFile(path) as apk:
        names = apk.namelist()
        if len(names) != len(set(names)):
            raise ValueError('Duplicate APK payload name')
        support_names = {'assets/system/' + name for name in SUPPORT}
        required = {'assets/system/manifest.json'} | (support_names if allow_bundled_support else set())
        if not allow_bundled_support and support_names.intersection(names):
            raise ValueError('OEM support firmware is bundled in the firmware-free build')
        if not required.issubset(names):
            raise ValueError('Missing bundled support files or manifest')
        if json.loads(apk.read('assets/system/manifest.json')) != SUPPORT:
            raise ValueError('Bundled support manifest does not match the build profile')
        for info in apk.infolist():
            name=info.filename
            if info.is_dir():
                continue
            if name in required:
                if name.endswith('/manifest.json'):
                    continue
                spec = SUPPORT[PurePosixPath(name).name]
                if info.file_size != spec['bytes'] or hashlib.sha256(apk.read(name)).hexdigest() != spec['sha256']:
                    raise ValueError(f'Incorrect bundled support file: {name}')
                continue
            legal = {'assets/legal/' + Path(n).name: n for n in ('LICENSE','LICENSING.md','THIRD_PARTY_NOTICES.md','licenses/ANDROID_CARGO_NOTICES.txt')}
            if name in legal:
                expected = Path(__file__).resolve().parents[2] / legal[name]
                if apk.read(name) != expected.read_bytes():
                    raise ValueError('License notice does not match source: ' + name)
                continue
            if name == 'assets/build.json':
                if info.file_size > 4096:
                    raise ValueError('Oversized build metadata')
                receipt = json.loads(apk.read(name))
                if set(receipt) != {'source_repository','source_commit','version_name','version_code','source_license'} or receipt['source_repository'] != 'https://github.com/djfremen/OpenSAAB-T2' or not re.fullmatch('[0-9a-f]{40}',receipt['source_commit']) or receipt['source_license'] != 'MPL-2.0':
                    raise ValueError('Unexpected source metadata')
                continue
            if name in NATIVE:
                with apk.open(info) as source:
                    if source.read(4) != b'\x7fELF':
                        raise ValueError(f'Native program is not ELF: {name}')
                continue
            # Apart from the pinned support files, allow only the compiled manifest,
            # DEX, resource table, small UI resources and Android signing metadata.
            allowed = name in {'AndroidManifest.xml', 'resources.arsc'} or re.fullmatch(r'classes(?:[2-9]|[1-9][0-9]+)?\.dex', name) is not None or name.startswith('META-INF/') or (
                name.startswith('res/') and PurePosixPath(name).suffix.lower() in {'.xml','.png','.webp','.jpg','.jpeg'} and info.file_size <= 1024*1024)
            if not allowed:
                raise ValueError(f'Unexpected APK payload (Saab program images must be downloaded/imported): {name}')
        if not NATIVE.issubset(apk.namelist()):
            raise ValueError('Missing emulator or USB executable')
    print('PASS: ' + ('explicit development profile includes three pinned support files; no Saab card' if allow_bundled_support else 'APK contains our emulator/adapter executables and metadata; no OEM firmware or card archive'))
if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('apk');parser.add_argument('--allow-bundled-support',action='store_true');args=parser.parse_args();check(args.apk,args.allow_bundled_support)
