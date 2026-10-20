#!/usr/bin/env python3
"""Retain exact native payloads beside a versioned package; never change the payload."""
import argparse
import hashlib
import json
from pathlib import Path, PurePosixPath
import re
import zipfile


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def retain(manifest_path, payload_root, output, licenses=None):
    manifest_path, payload_root, output = map(Path, (manifest_path, payload_root, output))
    metadata = json.loads(manifest_path.read_text())
    for field in ['version', 'platform', 'architecture']:
        if not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9_.-]*', metadata.get(field, '')):
            raise ValueError('Missing or unsafe native component identity: ' + field)
    revision = metadata.get('source_commit', metadata.get('source_revision', ''))
    if not re.fullmatch(r'[0-9a-f]{40}', revision):
        raise ValueError('Exact native source revision is required')
    expected = metadata.get('binaries')
    if not isinstance(expected, dict) or not expected:
        raise ValueError('Package manifest must name every retained native component')
    components = []
    root = payload_root.resolve()
    for name, checksum in sorted(expected.items()):
        relative = PurePosixPath(name)
        if relative.is_absolute() or '..' in relative.parts or '\\' in name:
            raise ValueError('Unsafe component path')
        source = root.joinpath(*relative.parts)
        if source.is_symlink() or not source.resolve().is_relative_to(root) or not source.is_file():
            raise ValueError('Missing or indirect native component: ' + name)
        if digest(source) != checksum:
            raise ValueError('Native component hash mismatch: ' + name)
        components.append({'path': 'components/' + name, 'source_name': name,
                           'sha256': checksum, 'bytes': source.stat().st_size})
    record = {
        'schema': 1, 'version': metadata['version'], 'platform': metadata['platform'],
        'architecture': metadata['architecture'], 'source_revision': revision,
        'source_dirty': metadata.get('source_dirty'),
        'package_sha256': metadata.get('sha256', metadata.get('artifact_sha256')),
        'package_manifest_sha256': digest(manifest_path), 'components': components,
        'adapter_scope': metadata.get('native_adapter_scope', metadata.get('mxplus_firmware_engine', 'See package verification receipt')),
        'verification': 'Exact bytes and ZIP integrity only. Architecture, installed workflow and physical checks require their own receipts.',
    }
    if output.exists():
        raise ValueError('Preserve existing native archive: ' + str(output))
    output.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(output, 'x', compression=zipfile.ZIP_DEFLATED, strict_timestamps=False) as archive:
        archive.writestr('native-components.json', json.dumps(record, indent=2) + '\n')
        archive.write(manifest_path, 'package-manifest.json')
        for component in components:
            archive.write(root / component['source_name'], component['path'])
        if licenses:
            license_root = Path(licenses).resolve()
            for path in sorted(license_root.rglob('*')):
                if path.is_file() and not path.is_symlink() and path.resolve().is_relative_to(license_root):
                    archive.write(path, 'licenses/' + str(path.relative_to(license_root)))
            for name in ['LICENSE', 'LICENSING.md', 'THIRD_PARTY_NOTICES.md', 'MDI_LICENSE.txt', 'dependencies.json']:
                path = license_root.parent / name
                if path.is_file() and not path.is_symlink():
                    archive.write(path, name)
    with zipfile.ZipFile(output) as archive:
        if archive.testzip() is not None:
            raise ValueError('Native archive ZIP verification failed')
        for component in components:
            if hashlib.sha256(archive.read(component['path'])).hexdigest() != component['sha256']:
                raise ValueError('Retained native bytes changed')
    return {'artifact': output.name, 'sha256': digest(output), 'bytes': output.stat().st_size,
            'component_count': len(components)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--manifest', required=True, type=Path)
    parser.add_argument('--payload-root', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--licenses', type=Path)
    options = parser.parse_args()
    print(json.dumps(retain(options.manifest, options.payload_root, options.output, options.licenses)))


if __name__ == '__main__':
    main()
