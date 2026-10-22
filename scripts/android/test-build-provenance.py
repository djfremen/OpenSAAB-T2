#!/usr/bin/env python3
"""Reject incomplete, mislabeled or substituted native/dependency build inputs."""
import hashlib
import json
import tempfile
import unittest
from pathlib import Path

from native_payload_inputs import NATIVE_NAMES, load_inputs
from report_dependencies import verify_artifacts


class NativeInputsTest(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.commit = 'a' * 40
        self.version = '0.1.1-preview.31'
        # A minimal ELF header is enough for the existing ABI check; never executed.
        data = b'\x7fELF\x02\x01' + b'\0' * 12 + (183).to_bytes(2, 'little')
        self.spec = {'schema': 1, 'application_version': self.version, 'architecture': 'arm64-v8a',
                     'source_revision': self.commit, 'components': {}}
        for name in NATIVE_NAMES:
            path = self.root / Path(name).name
            path.write_bytes(data)
            self.spec['components'][name] = {
                'kind': 'retained', 'sha256': hashlib.sha256(data).hexdigest(), 'path': path.name,
                'source_revision': 'b' * 40, 'source_application_version': '0.1.1-preview.1',
                'retained_from_application_version': '0.1.1-preview.30',
                'retained_from_apk_sha256': 'c' * 64}

    def load(self):
        path = self.root / 'inputs.json'
        path.write_text(json.dumps(self.spec))
        return load_inputs(path, self.version, self.commit)

    def test_retained_core_keeps_original_identity_and_exact_hash(self):
        _, public = self.load()
        self.assertEqual(public['components']['lib/arm64-v8a/libtech2_recovered.so']['source_application_version'], '0.1.1-preview.1')
        self.assertTrue(all('path' not in item for item in public['components'].values()))

    def test_substituted_component_rejected(self):
        (self.root / 'libtech2_mdi.so').write_bytes(b'\0' * 20)
        with self.assertRaises(ValueError):
            self.load()

    def test_missing_component_rejected(self):
        del self.spec['components']['lib/arm64-v8a/libtech2_mdi.so']
        with self.assertRaises(ValueError):
            self.load()

    def test_old_build_cannot_claim_new_native_source(self):
        self.spec['components']['lib/arm64-v8a/libtech2_recovered.so']['kind'] = 'built'
        with self.assertRaises(ValueError):
            self.load()

    def test_other_app_version_rejected(self):
        self.spec['application_version'] = '0.1.1-preview.30'
        with self.assertRaises(ValueError):
            self.load()

    def test_unpinned_retention_rejected(self):
        del self.spec['components']['lib/arm64-v8a/libnano_probe.so']['retained_from_apk_sha256']
        with self.assertRaises(ValueError):
            self.load()


class DependencyInputsTest(unittest.TestCase):
    def test_dependency_graph_and_bytes_are_both_pinned(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'library.jar'
            path.write_bytes(b'original')
            artifacts = [{'coordinate': 'example:library:1.0', 'path': str(path)}]
            lock = {'schema': 1, 'artifacts': [{'coordinate': 'example:library:1.0',
                     'filename': path.name, 'sha256': hashlib.sha256(b'original').hexdigest()}]}
            self.assertEqual(len(verify_artifacts(artifacts, lock)), 1)
            with self.assertRaises(ValueError):
                verify_artifacts([], lock)
            path.write_bytes(b'substituted')
            with self.assertRaises(ValueError):
                verify_artifacts(artifacts, lock)


if __name__ == '__main__':
    unittest.main()
