"""Validate complete, version-bound native inputs before packaging their exact bytes."""
import hashlib
import json
import re
from pathlib import Path

from build_profiles import PROFILES, validate_elf

NATIVE_NAMES = frozenset('lib/arm64-v8a/' + name for name in (
    'libtech2_emu.so', 'libtech2_mx.so', 'libtech2_recovered.so',
    'libtech2_mdi.so', 'libnano_probe.so', 'libchipsoft_probe.so',
    'libopensaab_mdi_android.so', 'libopensaab_connection.so'))


def validate_manifest(spec, version, source_commit, files=None):
    if (spec.get('schema') != 1 or spec.get('application_version') != version
            or spec.get('architecture') != 'arm64-v8a'
            or spec.get('source_revision') != source_commit
            or set(spec.get('components', {})) != NATIVE_NAMES):
        raise ValueError('Native inputs must cover the exact application version, source and all eight ARM64 components')
    for name, item in spec['components'].items():
        if (not re.fullmatch('[0-9a-f]{64}', item.get('sha256', ''))
                or not re.fullmatch('[0-9a-f]{40}', item.get('source_revision', ''))
                or not item.get('source_application_version')):
            raise ValueError('Missing native source/hash provenance: ' + name)
        if item.get('kind') == 'built':
            if item['source_revision'] != source_commit or item['source_application_version'] != version:
                raise ValueError('Rebuilt native input must use the current source/version: ' + name)
        elif item.get('kind') == 'retained':
            if (not re.fullmatch('[0-9a-f]{64}', item.get('retained_from_apk_sha256', ''))
                    or not item.get('retained_from_application_version')):
                raise ValueError('Retained native input requires its original package pin: ' + name)
        else:
            raise ValueError('Unknown native input provenance: ' + name)
        if files is not None:
            data = Path(files[name]).read_bytes()
            validate_elf(data[:20], PROFILES['arm64'])
            if hashlib.sha256(data).hexdigest() != item['sha256']:
                raise ValueError('Native input hash mismatch: ' + name)
    return spec


def load_inputs(path, version, source_commit):
    path = Path(path).resolve()
    spec = json.loads(path.read_text())
    files = {name: (path.parent / item['path']).resolve()
             for name, item in spec.get('components', {}).items()}
    validate_manifest(spec, version, source_commit, files)
    # Host paths do not belong in the distributable provenance asset.
    public = {**spec, 'components': {
        name: {key: value for key, value in item.items() if key != 'path'}
        for name, item in spec['components'].items()}}
    return files, public
