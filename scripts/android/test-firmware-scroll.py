#!/usr/bin/env python3
"""Check the native scroll binding against the common contract; never installs an APK."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile

repo = Path(__file__).resolve().parents[2]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--contract', type=Path, default=repo.parent/'OpenSAAB/emulator/ui/interface-contract.json')
args = parser.parse_args()
contract = json.loads(args.contract.read_text())
gesture = contract['gesture_scrolling']
expected = [gesture['preference_key'], gesture['label'], str(gesture['default_natural']).lower()]
expected += [str(contract['key_encoders'][gesture[mode][direction]])
             for mode in ('directional', 'natural') for direction in ('up', 'down')]
expected += [gesture['directional_description'], gesture['natural_description']]
jdk = Path(os.environ.get('JAVA_HOME', '/Applications/Android Studio.app/Contents/jbr/Contents/Home'))
sdk = Path(os.environ.get('ANDROID_HOME', str(Path.home()/'Library/Android/sdk')))
android = sdk/'platforms/android-36/android.jar'
with tempfile.TemporaryDirectory(prefix='opensaab-scroll-contract-') as output:
    sources = [repo/'android/shared/com/opensaab/usb'/f'{name}.java'
               for name in ('FirmwareScrollPreferences', 'SessionSheet', 'SessionStyle')]
    subprocess.run([str(jdk/'bin/javac'), '-cp', str(android),
                    '-sourcepath', str(repo/'android/shared'), '-d', output,
                    *map(str, sources), str(repo/'android/tests/FirmwareScrollContractTest.java')], check=True)
    subprocess.run([str(jdk/'bin/java'), '-cp', output+os.pathsep+str(android),
                    'com.opensaab.usb.FirmwareScrollContractTest', *expected], check=True)
