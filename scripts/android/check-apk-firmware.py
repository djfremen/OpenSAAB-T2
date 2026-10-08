#!/usr/bin/env python3
"""Reject bundled OEM firmware by default; explicit legacy development profile only."""
import argparse
import json
import hashlib
import zipfile
import re
from pathlib import PurePosixPath, Path
from build_profiles import PROFILES, validate_elf

KOTLIN_RESOURCES = json.loads((Path(__file__).resolve().parents[2] / 'android/shared/kotlin-runtime-resources.json').read_text())
SUPPORT = json.loads((Path(__file__).resolve().parents[2] / 'android/tech2-app/support-files.json').read_text())
def check(path, allow_bundled_support=False, profile_name='arm64', connection_core_sha256=None):
    profile = PROFILES[profile_name]
    native = {f'lib/{profile.abi}/{name}' for name in ('libtech2_emu.so', 'libnano_probe.so', 'libchipsoft_probe.so')}
    connection_core = 'lib/arm64-v8a/libopensaab_connection.so'
    if connection_core_sha256 is not None and (profile_name != 'arm64' or not re.fullmatch('[0-9a-f]{64}', connection_core_sha256)):
        raise ValueError('vLinker connection core requires an exact ARM64 SHA-256 pin')
    with zipfile.ZipFile(path) as apk:
        names = apk.namelist()
        if len(names) != len(set(names)):
            raise ValueError('Duplicate APK payload name')
        support_names = {'assets/system/' + name for name in SUPPORT}
        required = {'assets/system/manifest.json'} | (support_names if allow_bundled_support else set())
        legal = {'assets/legal/' + Path(n).name: n for n in ('LICENSE','LICENSING.md','THIRD_PARTY_NOTICES.md','licenses/ANDROID_CARGO_NOTICES.txt', 'licenses/ANDROID_BLUETOOTH_CARGO_NOTICES.txt', 'licenses/ANDROIDX_APACHE_2_0.txt', 'licenses/OPENVCX_LGPL_3_0.txt', 'licenses/OPENVCX_GPL_3_0.txt', 'licenses/OPENVCX_SOURCE_ORIGIN.txt')}
        required_legal = set(legal)
        if not required_legal.issubset(names):
            raise ValueError('Missing required license or source-origin notice')
        if not allow_bundled_support and support_names.intersection(names):
            raise ValueError('OEM support firmware is bundled in the firmware-free build')
        if not required.issubset(names):
            raise ValueError('Missing bundled support files or manifest')
        if json.loads(apk.read('assets/system/manifest.json')) != SUPPORT:
            raise ValueError('Bundled support manifest does not match the build profile')
        if not set(KOTLIN_RESOURCES).issubset(names):raise ValueError('Missing pinned Kotlin runtime resources')
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
            if name in KOTLIN_RESOURCES:
                if hashlib.sha256(apk.read(name)).hexdigest()!=KOTLIN_RESOURCES[name]:raise ValueError('Pinned Kotlin resource changed: '+name)
                continue
            if name in legal:
                expected = Path(__file__).resolve().parents[2] / legal[name]
                if apk.read(name) != expected.read_bytes():
                    raise ValueError('License notice does not match source: ' + name)
                continue
            # AGP release builds merge small ART profiles from AndroidX dependencies.
            profile_headers = {'assets/dexopt/baseline.prof': b'pro\x00010\x00', 'assets/dexopt/baseline.profm': b'prm\x00002\x00'}
            if name in profile_headers:
                data = apk.read(name)
                if not 8 < len(data) <= 65536 or not data.startswith(profile_headers[name]):
                    raise ValueError('Invalid ART baseline profile: ' + name)
                continue
            if name == 'assets/build.json':
                if info.file_size > 4096:
                    raise ValueError('Oversized build metadata')
                receipt = json.loads(apk.read(name))
                if set(receipt) != {'source_repository','source_commit','version_name','version_code','source_license'} or receipt['source_repository'] != 'https://github.com/djfremen/OpenSAAB-T2' or not re.fullmatch('[0-9a-f]{40}',receipt['source_commit']) or receipt['source_license'] != 'MPL-2.0 AND LGPL-3.0-only':
                    raise ValueError('Unexpected source metadata')
                continue
            if name in native:
                with apk.open(info) as source:
                    validate_elf(source.read(20), profile)
                continue
            if name == connection_core and connection_core_sha256 is not None:
                data = apk.read(name)
                validate_elf(data[:20], profile)
                if hashlib.sha256(data).hexdigest() != connection_core_sha256:
                    raise ValueError('vLinker connection executable differs from the locally built source pin')
                continue
            # Apart from the pinned support files, allow only the compiled manifest,
            # DEX, resource table, small UI resources and Android signing metadata.
            allowed = name in {'AndroidManifest.xml', 'resources.arsc'} or re.fullmatch(r'classes(?:[2-9]|[1-9][0-9]+)?\.dex', name) is not None or name.startswith('META-INF/') or (
                name.startswith('res/') and PurePosixPath(name).suffix.lower() in {'.xml','.png','.webp','.jpg','.jpeg'} and info.file_size <= 1024*1024)
            if not allowed:
                raise ValueError(f'Unexpected APK payload (Saab program images must be downloaded/imported): {name}')
        if not native.issubset(apk.namelist()):
            raise ValueError('Missing emulator or USB executable')
        if connection_core_sha256 is not None and connection_core not in names:
            raise ValueError('Missing pinned vLinker connection executable')
    print('PASS: ' + ('explicit development profile includes three pinned support files; no Saab card' if allow_bundled_support else 'APK contains our emulator/adapter executables and metadata; no OEM firmware or card archive'))
if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('apk');parser.add_argument('--allow-bundled-support',action='store_true');parser.add_argument('--profile',choices=PROFILES,default='arm64');parser.add_argument('--connection-core-sha256');args=parser.parse_args();check(args.apk,args.allow_bundled_support,args.profile,args.connection_core_sha256)
