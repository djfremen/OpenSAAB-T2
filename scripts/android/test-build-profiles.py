#!/usr/bin/env python3
"""Reject mixed and mislabeled native payloads in both Android build profiles."""
import contextlib
import importlib.util
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock
import zipfile
from build_profiles import PROFILES, validate_elf

spec = importlib.util.spec_from_file_location('apk_check', Path(__file__).with_name('check-apk-firmware.py'))
checker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(checker)
REPO = Path(__file__).resolve().parents[2]
LEGAL_FIXTURE_FILES = (
    'LICENSE', 'LICENSING.md', 'THIRD_PARTY_NOTICES.md',
    'licenses/ANDROID_CARGO_NOTICES.txt',
    'licenses/ANDROID_BLUETOOTH_CARGO_NOTICES.txt',
    'licenses/ANDROIDX_APACHE_2_0.txt',
    'licenses/OPENVCX_LGPL_3_0.txt',
    'licenses/OPENVCX_GPL_3_0.txt',
    'licenses/OPENVCX_SOURCE_ORIGIN.txt',
)

def header(profile):
    data = bytearray(20)
    data[:4] = b'\x7fELF'
    data[4:6] = bytes([profile.elf_class, 1])
    data[18:20] = profile.elf_machine.to_bytes(2, 'little')
    return data

class ArchitectureTests(unittest.TestCase):
    def test_class_and_machine(self):
        for name, profile in PROFILES.items():
            validate_elf(header(profile), profile)
            other = PROFILES['headunit-arm32' if name == 'arm64' else 'arm64']
            for data in (header(other), b'\x7fELF', b'not an ELF file'):
                with self.assertRaises(ValueError):
                    validate_elf(data, profile)
            data = header(profile)
            data[18:20] = other.elf_machine.to_bytes(2, 'little')
            with self.assertRaises(ValueError):
                validate_elf(data, profile)

    def test_apk_architecture_isolation(self):
        with tempfile.TemporaryDirectory() as folder:
            for name, profile in PROFILES.items():
                other = PROFILES['headunit-arm32' if name == 'arm64' else 'arm64']
                for mode in ('correct', 'mislabeled', 'extra', 'missing'):
                    with self.subTest(profile=name, mode=mode):
                        path = Path(folder) / 'test.apk'
                        with zipfile.ZipFile(path, 'w') as apk:
                            apk.writestr('assets/system/manifest.json', json.dumps(checker.SUPPORT))
                            for relative in LEGAL_FIXTURE_FILES:
                                apk.writestr('assets/legal/' + Path(relative).name,
                                             (REPO / relative).read_bytes())
                            for binary in ('libtech2_emu.so', 'libnano_probe.so', 'libchipsoft_probe.so'):
                                if mode == 'missing' and binary == 'libnano_probe.so':
                                    continue
                                apk.writestr(f'lib/{profile.abi}/{binary}', header(other if mode == 'mislabeled' else profile))
                            if mode == 'extra':
                                apk.writestr(f'lib/{other.abi}/libtech2_emu.so', header(other))
                        # This fixture isolates native architecture validation;
                        # dependency-resource integrity is checked on real APKs.
                        with contextlib.redirect_stdout(io.StringIO()), mock.patch.object(checker, 'KOTLIN_RESOURCES', {}):
                            if mode == 'correct':
                                checker.check(path, profile_name=name)
                            else:
                                with self.assertRaises(ValueError):
                                    checker.check(path, profile_name=name)

if __name__ == '__main__':
    unittest.main()
