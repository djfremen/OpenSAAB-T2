import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
import hashlib
import zipfile

spec = importlib.util.spec_from_file_location('native_components', Path(__file__).with_name('retain_native_components.py'))
native = importlib.util.module_from_spec(spec)
spec.loader.exec_module(native)


class NativeRetentionTests(unittest.TestCase):
    def test_exact_bytes_survive_and_historical_archive_is_never_replaced(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp); payload = root / 'worker'; payload.write_bytes(b'fixture payload')
            manifest = root / 'manifest.json'
            manifest.write_text(json.dumps({'version':'0.2.28-mxplus.4','platform':'macos','architecture':'arm64',
                'source_commit':'a'*40,'binaries':{'worker':hashlib.sha256(payload.read_bytes()).hexdigest()}}))
            archive = root / 'native.zip'
            native.retain(manifest, root, archive)
            with zipfile.ZipFile(archive) as retained:
                self.assertEqual(retained.read('components/worker'), payload.read_bytes())
            before = archive.read_bytes()
            with self.assertRaises(ValueError): native.retain(manifest, root, archive)
            self.assertEqual(archive.read_bytes(), before)
            payload.write_bytes(b'changed payload')
            with self.assertRaisesRegex(ValueError, 'hash mismatch'):
                native.retain(manifest, root, root / 'changed.zip')
            self.assertFalse((root / 'changed.zip').exists())

    def test_component_cannot_escape_its_payload_root(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp); manifest = root / 'manifest.json'
            for name in ['../outside', '/outside', 'bad\\path']:
                manifest.write_text(json.dumps({'version':'0.2.28-mxplus.4','platform':'ios','architecture':'arm64',
                    'source_revision':'b'*40,'binaries':{name:'0'*64}}))
                with self.assertRaises(ValueError): native.retain(manifest, root, root / 'unsafe.zip')
                self.assertFalse((root / 'unsafe.zip').exists())


if __name__ == '__main__': unittest.main()
