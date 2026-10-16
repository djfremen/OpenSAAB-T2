#!/usr/bin/env python3
"""Mirror the deployed support API into the canonical product repository."""
import argparse
import hashlib
import json
import re
import subprocess
from pathlib import Path

FILES = ('support_reports_api.py', 'support_entrypoint.py', 'Dockerfile.support', 'README.support.md',
         'tests/test_support_reports.py', 'tests/test_offline_evidence.py', 'tests/test_identity.py')
sha = lambda b: hashlib.sha256(b).hexdigest()


def git(root, *args):
    return subprocess.check_output(['git', '-C', str(root), *args], text=True).strip()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--canonical-root', required=True, type=Path)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    source = Path(__file__).resolve().parent
    source_root, destination = source.parent, args.canonical_root.resolve()/'service/support'
    if source_root.resolve() == args.canonical_root.resolve():
        raise SystemExit('Source and canonical repositories must be separate.')
    if git(source_root, 'status', '--porcelain', '--', 'service'):
        raise SystemExit('Commit/review source service changes before mirroring.')
    revision = git(source_root, 'rev-parse', 'HEAD')
    old_path = destination/'MIRROR.json'
    old = json.loads(old_path.read_text()) if old_path.exists() else None
    rendered, hashes = {}, {}
    for name in FILES:
        raw = (source/name).read_bytes()
        output = re.sub(rb'(?m)^from service\.', b'from service.support.', raw) if name.startswith('tests/') else raw
        rendered[name] = output
        hashes[name] = {'source_sha256':sha(raw), 'mirror_sha256':sha(output)}
    manifest = {'schema':1, 'source_repository':'https://github.com/djfremen/OpenSAAB-T2',
                'source_revision':revision, 'source_directory':'service', 'files':hashes}
    rendered['MIRROR.json'] = (json.dumps(manifest, indent=2)+'\n').encode()
    mismatched = [name for name,raw in rendered.items() if not (destination/name).exists() or (destination/name).read_bytes()!=raw]
    if args.check:
        if mismatched:
            raise SystemExit('Support mirror drift: '+', '.join(mismatched))
        print('Support mirror matches '+revision)
        return
    # Never overwrite independent edits. An existing mirror must still match its
    # prior manifest; the first migration requires clean tracked destination files.
    for name in mismatched:
        target=destination/name
        if not target.exists():
            continue
        if old and name in old.get('files', {}):
            if sha(target.read_bytes()) != old['files'][name]['mirror_sha256']:
                raise SystemExit('Canonical support file has independent edits: '+name)
        elif name != 'MIRROR.json':
            relative=str(target.relative_to(args.canonical_root.resolve()))
            tracked=git(args.canonical_root, 'ls-files', '--', relative)
            dirty=git(args.canonical_root, 'status', '--porcelain', '--', relative)
            if not tracked or dirty:
                raise SystemExit('Canonical support file is not clean: '+name)
    for name, raw in rendered.items():
        target=destination/name
        target.parent.mkdir(parents=True, exist_ok=True)
        temporary=target.with_name(target.name+'.sync.tmp')
        temporary.write_bytes(raw)
        temporary.replace(target)
    print('Mirrored support API from '+revision)


if __name__ == '__main__':
    main()
