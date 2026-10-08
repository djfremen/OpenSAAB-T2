#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""Create a readable, byte-identical public APK alias; retain legacy updater assets."""
import argparse
import datetime
import hashlib
import os
from pathlib import Path
import re
import shutil
import subprocess
import zipfile

MONTHS = ('JAN', 'FEB', 'MAR', 'APR', 'MAY', 'JUN', 'JUL', 'AUG', 'SEPT', 'OCT', 'NOV', 'DEC')
PACKAGES = {
    'com.opensaab.checker': ('Setup', None),
    'com.opensaab.tech2': ('64', 'arm64-v8a'),
    'com.opensaab.tech2.headunit32': ('32', 'armeabi-v7a'),
}

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('apk', type=Path)
    parser.add_argument('--release-date', required=True, help='YYYY-MM-DD publication date in America/Los_Angeles')
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--aapt', type=Path, default=Path(os.environ.get('ANDROID_HOME', str(Path.home() / 'Library/Android/sdk'))) / 'build-tools/36.0.0/aapt')
    args = parser.parse_args()
    try:
        date = datetime.date.fromisoformat(args.release_date)
        if args.release_date != date.isoformat(): raise ValueError('Use YYYY-MM-DD')
        badging = subprocess.check_output([str(args.aapt), 'dump', 'badging', str(args.apk)], text=True)
        match = re.search(r"^package: name='([^']+)' versionCode='([0-9]+)' versionName='([^']+)'", badging, re.M)
        if not match or match[1] not in PACKAGES: raise ValueError('Unknown OpenSAAB package')
        kind, abi = PACKAGES[match[1]]
        version = match[3]
        if not re.fullmatch(r'[0-9]+\.[0-9]+\.[0-9]+(?:-(?:preview|headunit)\.[0-9]+)?', version):
            raise ValueError('APK must have a release version; version comes from its manifest')
        with zipfile.ZipFile(args.apk) as archive:
            natives = {n.split('/')[1] for n in archive.namelist() if n.startswith('lib/') and n.endswith('.so')}
            if natives != ({abi} if abi else set()): raise ValueError('Package and native architecture disagree')
        stamp = f'{date.day:02d}{MONTHS[date.month-1]}{date.year%100:02d}'
        name = f'OpenSAAB_{kind}_{stamp}_v{version}.apk'
        args.output.mkdir(parents=True, exist_ok=True)
        target = args.output / name
        digest = hashlib.sha256(args.apk.read_bytes()).hexdigest()
        if target.exists() and hashlib.sha256(target.read_bytes()).hexdigest() != digest:
            raise ValueError('Refusing to replace a named release with different bytes')
        if args.apk.resolve() != target.resolve(): shutil.copy2(args.apk, target)
        if hashlib.sha256(target.read_bytes()).hexdigest() != digest: raise ValueError('Copy verification failed')
        target.with_suffix('.apk.sha256').write_text(digest+'  '+name+'\n')
        print(target)
    except (ValueError, OSError, subprocess.CalledProcessError, zipfile.BadZipFile) as error:
        parser.exit(1, str(error)+'\n')

if __name__ == '__main__': main()
