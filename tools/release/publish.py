#!/usr/bin/env python3
"""The gated GitHub release entry point. Defaults to a draft; --publish promotes it.

Only explicitly listed public artifacts and release notes are uploaded. The
private deployment record/evidence remain local. Use deployment.py --phase
complete after checking the live website, README and updater.
"""
import argparse
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys
import tempfile
from urllib.parse import quote

from deployment import bound_file, digest, load_ledger, reserve, security_summary, validate, verify_source


def gh(*args):
    result = subprocess.run(['gh', *args], capture_output=True, text=True)
    if result.returncode:
        raise ValueError('GitHub command failed: ' + result.stderr.strip())
    return result.stdout


def api_optional(endpoint):
    result = subprocess.run(['gh', 'api', endpoint], capture_output=True, text=True)
    if result.returncode and '(HTTP 404)' in result.stderr:
        return None
    if result.returncode:
        raise ValueError('GitHub lookup failed: ' + result.stderr.strip())
    return json.loads(result.stdout)


def lookup_release(repo, tag):
    """Drafts without an existing Git tag may be absent from releases/tags."""
    release = api_optional(f'repos/{repo}/releases/tags/{quote(tag, safe="")}')
    if release is not None:
        return release
    pages = json.loads(gh('api', '--paginate', '--slurp', f'repos/{repo}/releases?per_page=100'))
    if not isinstance(pages, list) or any(not isinstance(page, list) for page in pages):
        raise ValueError('invalid release listing; cannot confirm a missing draft')
    matches = [r for page in pages for r in page if isinstance(r, dict) and r.get('tag_name') == tag]
    if len(matches) > 1:
        raise ValueError('multiple releases have the candidate tag; inspect before retrying')
    return matches[0] if matches else None


def release_config(data):
    config = data.get('release', {})
    if not isinstance(config, dict):
        raise ValueError('release configuration required')
    repo, tag, title = (config.get(k) for k in ('repository', 'tag', 'title'))
    if not isinstance(repo, str) or not re.fullmatch(r'[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+', repo):
        raise ValueError('release repository must be owner/name')
    if not isinstance(tag, str) or not re.fullmatch(r'(?:[A-Za-z0-9_-]+-)?v' + re.escape(data['version']), tag):
        raise ValueError('release tag must include the exact full version')
    if not isinstance(title, str) or data['version'] not in title:
        raise ValueError('release title must include the full version')
    return repo, tag, title


def verify_tag(repo, tag, revision):
    ref = api_optional(f'repos/{repo}/git/ref/tags/{quote(tag, safe="")}')
    if ref is None:
        return
    obj = ref['object']
    for _ in range(8):
        if obj['type'] == 'commit':
            if obj['sha'] != revision:
                raise ValueError('existing release tag points to a different source revision')
            return
        if obj['type'] != 'tag':
            break
        obj = json.loads(gh('api', f'repos/{repo}/git/tags/{obj["sha"]}'))['object']
    raise ValueError('cannot resolve release tag to the recorded source revision')


def verify_remote_source(repo, revision):
    commit = api_optional(f'repos/{repo}/commits/{revision}')
    if commit is None or commit.get('sha') != revision:
        raise ValueError('matching source commit must exist in the release repository before publication')


def release_notes(data, bundle):
    # The remote draft reserves all prepared bytes before the first upload.
    # This persists across CI runners and prevents changed-content retries.
    identity = {
        'product': data['product'], 'version': data['version'], 'source_revision': data['source']['revision'],
        'artifacts': sorted([{'name': Path(a['path']).name, **{k: a[k] for k in ('sha256', 'size', 'role')}} for a in data['artifacts']], key=lambda a: a['name']),
        'targets': sorted([{
            'platform': t['platform'], 'architecture': t['architecture'],
            'capabilities': sorted(t['capabilities']),
            'security_access': security_summary(t),
            'identity': {k: v for k, v in t['identity'].items() if k != 'evidence'},
        } for t in data['targets']], key=lambda t: (t['platform'], t['architecture'])),
    }
    fingerprint = hashlib.sha256(json.dumps(identity, sort_keys=True, separators=(',', ':')).encode()).hexdigest()
    return bound_file(bundle, data['notes']).read_text().rstrip() + '\n\n<!-- OpenSAAB candidate sha256=' + fingerprint + ' -->\n'


def verify_draft(data, bundle, release, repo, tag, allow_partial=False, published=False):
    if release.get('tag_name') != tag or release.get('target_commitish') != data['source']['revision']:
        raise ValueError('release tag or target source differs from the reviewed candidate')
    if release.get('draft') is not (not published):
        raise ValueError('release state differs; published assets cannot be replaced')
    expected = {Path(a['path']).name: a for a in data['artifacts']}
    actual = {a['name'] for a in release['assets']}
    if not actual <= set(expected) or (not allow_partial and actual != set(expected)):
        raise ValueError('draft asset set differs from candidate; inspect it, do not clobber or reuse its version')
    notes = release_notes(data, bundle)
    if release.get('body', '').replace('\r\n', '\n').rstrip() != notes.replace('\r\n', '\n').rstrip():
        raise ValueError('draft release notes differ from the reviewed candidate')
    if release.get('name') != data['release']['title']:
        raise ValueError('draft title differs from the reviewed candidate')
    if release.get('prerelease') != ('-' in data['version']):
        raise ValueError('draft prerelease flag differs from candidate channel')
    with tempfile.TemporaryDirectory(prefix='opensaab-release-verify-') as directory:
        if actual:
            gh('release', 'download', tag, '--repo', repo, '--dir', directory)
        for name in actual:
            asset = expected[name]
            path = Path(directory) / name
            if not path.is_file() or digest(path) != asset['sha256'] or path.stat().st_size != asset['size']:
                raise ValueError('downloaded draft asset differs: ' + name)


def publish(data, bundle, source_root, ledger, do_publish=False):
    errors = validate(data, bundle, 'prepare', load_ledger(ledger))
    if errors:
        raise ValueError('\n'.join(errors))
    if data['kind'] != 'application':
        raise ValueError('GitHub publisher accepts application releases; website deployments use their own gated entry point')
    verify_source(data, source_root)
    repo, tag, title = release_config(data)
    if any('#' in str(bound_file(bundle, a)) for a in data['artifacts']):
        raise ValueError('public asset paths must not contain the GitHub CLI asset-label delimiter #')
    verify_remote_source(repo, data['source']['revision'])
    verify_tag(repo, tag, data['source']['revision'])
    release = lookup_release(repo, tag)
    if release is not None:
        verify_draft(data, bundle, release, repo, tag, allow_partial=True)
    # Reserve even failed attempts: changed contents always require a new version.
    reserve(data, ledger)
    if release is None:
        with tempfile.TemporaryDirectory(prefix='opensaab-release-notes-') as directory:
            notes_path = Path(directory) / 'notes.md'
            notes_path.write_text(release_notes(data, bundle))
            args = ['release', 'create', tag, '--repo', repo, '--draft', '--latest=false',
                    '--target', data['source']['revision'], '--title', title,
                    '--notes-file', str(notes_path)]
            if '-' in data['version']:
                args.append('--prerelease')
            gh(*args)
        release = lookup_release(repo, tag)
        if release is None:
            raise ValueError('draft creation was not confirmed; inspect the remote state before retrying')
        verify_draft(data, bundle, release, repo, tag, allow_partial=True)
    # Resume an interrupted upload only for absent assets. Existing bytes were
    # downloaded and checked above; never replace an uploaded asset.
    present = {a['name'] for a in release['assets']}
    missing = [str(bound_file(bundle, a)) for a in data['artifacts'] if Path(a['path']).name not in present]
    if missing:
        gh('release', 'upload', tag, '--repo', repo, *missing)
        release = lookup_release(repo, tag)
        if release is None:
            raise ValueError('draft disappeared while resuming upload')
        verify_draft(data, bundle, release, repo, tag)
    if not do_publish:
        return {'state': 'verified_draft', 'url': release['html_url'], 'tag': tag}
    # Recheck local integrity/source and remote draft immediately before promotion.
    errors = validate(data, bundle, 'prepare', load_ledger(ledger))
    if errors:
        raise ValueError('\n'.join(errors))
    verify_source(data, source_root)
    verify_tag(repo, tag, data['source']['revision'])
    release = lookup_release(repo, tag)
    if release is None:
        raise ValueError('draft disappeared before publication')
    verify_draft(data, bundle, release, repo, tag)
    gh('release', 'edit', tag, '--repo', repo, '--draft=false', '--latest=false')
    published = lookup_release(repo, tag)
    if published is None or published.get('draft'):
        raise ValueError('publication was not confirmed; inspect remote state before retrying')
    verify_tag(repo, tag, data['source']['revision'])
    # Download again after publishing; do not treat an upload as verification.
    verify_draft(data, bundle, published, repo, tag, published=True)
    return {'state': 'published_pending_live_surface_verification', 'url': published['html_url'], 'tag': tag}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--record', type=Path, required=True)
    parser.add_argument('--bundle', type=Path, required=True)
    parser.add_argument('--source-root', type=Path, required=True)
    parser.add_argument('--ledger', type=Path, default=Path.home() / '.local/share/opensaab/deployment-ledger.json')
    parser.add_argument('--publish', action='store_true', help='publish the verified draft; use only within user authorization')
    parser.add_argument('--receipt', type=Path, required=True, help='new local receipt path; must not exist')
    args = parser.parse_args()
    receipt_claimed = False
    try:
        if args.receipt.exists() or not args.receipt.parent.is_dir():
            raise ValueError('receipt must be a new file in an existing directory')
        record_bytes = args.record.read_bytes()
        data = json.loads(record_bytes)
        snapshot = args.receipt.with_suffix('.record.json')
        with snapshot.open('xb') as stream:
            stream.write(record_bytes)
        started = {'state': 'started', 'record_sha256': hashlib.sha256(record_bytes).hexdigest(),
                   'record_snapshot': str(snapshot),
                   'started_at': datetime.now(timezone.utc).isoformat()}
        with args.receipt.open('x') as stream:
            json.dump(started, stream, indent=2)
            stream.write('\n')
        receipt_claimed = True
        result = publish(data, args.bundle.resolve(), args.source_root, args.ledger, args.publish)
        result.update(started_at=started['started_at'], finished_at=datetime.now(timezone.utc).isoformat(), record_snapshot=str(snapshot),
                      record_sha256=started['record_sha256'], source_revision=data['source']['revision'],
                      artifacts=[{k: a[k] for k in ('path', 'sha256', 'size')} for a in data['artifacts']])
        with args.receipt.open('w') as stream:
            json.dump(result, stream, indent=2)
            stream.write('\n')
        print(json.dumps(result, indent=2))
        return 0
    except (ValueError, OSError, KeyError, TypeError, AttributeError, subprocess.SubprocessError) as error:
        if receipt_claimed:
            started.update(state='blocked_or_uncertain', error=str(error), finished_at=datetime.now(timezone.utc).isoformat())
            args.receipt.write_text(json.dumps(started, indent=2) + '\n')
        print('BLOCKED: ' + str(error))
        return 1


if __name__ == '__main__':
    sys.exit(main())
