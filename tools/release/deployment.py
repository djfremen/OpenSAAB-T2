#!/usr/bin/env python3
"""Check the Deployment Bible's evidence record before publication or completion.

Checks file integrity and declared scope. Human and hardware observations remain
reviewed evidence, never inferred by this validator. No network or publishing.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
from datetime import datetime, timezone
from pathlib import PurePosixPath
import stat
import zipfile

ROOT = Path(__file__).resolve().parents[2]
CHECKS = (
    'product_contract', 'source', 'identity', 'build', 'artifact_tests',
    'user_data', 'documentation', 'publication', 'claims', 'deployment',
)
SHA = re.compile(r'[0-9a-f]{64}')
REV = re.compile(r'[0-9a-f]{40}')
VERSION = re.compile(r'(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-[0-9A-Za-z-]+(\.[0-9A-Za-z-]+)*)?')
ARCH_LABELS = {
    'x86_64': ('x86_64', 'x64', 'Intel'), 'arm64': ('arm64', 'ARM64', 'Apple silicon'),
    'arm64-v8a': ('arm64-v8a', 'ARM64'), 'armeabi-v7a': ('armeabi-v7a', 'ARM32'),
    'x86': ('x86', '32-bit x86'), 'universal': ('universal', 'Universal'),
}


def identity_architecture_matches(label, platform, architecture, field, version):
    """Honor the owner's dated Android ARM64 filename without widening UI names."""
    if not text(label):
        return False
    if any(token in label for token in ARCH_LABELS.get(architecture, ())):
        return True
    if platform != 'android' or architecture not in {'arm64', 'arm64-v8a'} or field not in {'installer_name', 'manifest_name'} or not isinstance(version, str):
        return False
    suffix = r'\.apk' if field == 'installer_name' else r'-manifest\.json'
    match = re.fullmatch(r'OpenSAAB_64_([0-9]{2}[A-Z]{3}[0-9]{2})_v' + re.escape(version) + suffix, label)
    if not match:
        return False
    try:
        datetime.strptime(match[1], '%d%b%y')
    except ValueError:
        return False
    return True
SURFACES = {'readme', 'homepage', 'downloads', 'updater', 'release_notes'}
SECURITY_POLICY = 'security-access-password-1'
SECURITY_PASSWORD_SHA256 = hashlib.sha256(b'OpenSAAB').hexdigest()
SECURITY_CASES = {
    'correct_password': 'accepted',
    'empty_password': 'blocked',
    'incorrect_password': 'blocked',
    'case_changed_password': 'blocked',
    'cancelled_password': 'blocked',
    'fallback_without_password': 'blocked',
    'invalid_remembered_password': 'blocked',
    'import_without_password': 'blocked',
}


def security_summary(target):
    access = target.get('security_access', {})
    return {k: access.get(k) for k in ('policy', 'mode')} if isinstance(access, dict) else {}


def validate_security_access(target, asset, version, revision, file, need):
    """Require reviewed, exact-package observations; never infer runtime protection."""
    access = target.get('security_access')
    if not isinstance(access, dict):
        need(False, 'security access: every target requires a password-policy record')
        return
    need(access.get('policy') == SECURITY_POLICY, 'security access: required password policy missing')
    mode = access.get('mode')
    need(isinstance(mode, str) and mode in {'password_protected', 'unavailable'}, 'security access: invalid protection mode')
    path = file(access.get('evidence'), 'security access evidence')
    if path is None:
        return
    try:
        proof = json.loads(path.read_text())
    except (ValueError, OSError, UnicodeError) as error:
        need(False, 'security access: invalid JSON evidence: ' + str(error))
        return
    if not isinstance(proof, dict):
        need(False, 'security access: evidence must be an object')
        return
    expected = {
        'policy': SECURITY_POLICY, 'mode': mode, 'version': version,
        'platform': target.get('platform'), 'architecture': target.get('architecture'),
        'artifact_sha256': asset['sha256'], 'source_revision': revision,
    }
    need(all(proof.get(k) == v for k, v in expected.items()), 'security access: evidence belongs to another package or policy')
    need(proof.get('status') == 'pass', 'security access: verification cannot be pending or waived, including previews')
    need(text(proof.get('reviewed_by')), 'security access: reviewer required')
    observations = proof.get('observations')
    need(isinstance(observations, list) and bool(observations), 'security access: retained installed-artifact observations required')
    for ref in observations if isinstance(observations, list) else []:
        file(ref, 'security access observation')
    cases = proof.get('cases')
    cases = cases if isinstance(cases, dict) else {}
    required = SECURITY_CASES if mode == 'password_protected' else {'all_security_entry_points': 'unavailable'}
    if mode == 'password_protected':
        need(proof.get('password_sha256') == SECURITY_PASSWORD_SHA256, 'security access: password must be exactly OpenSAAB, case sensitive')
    for name, result in required.items():
        case = cases.get(name)
        need(isinstance(case, dict) and case.get('result') == result and text(case.get('note')),
             'security access: missing or failed case ' + name)


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def tree_digest(root):
    if not root.is_dir():
        raise ValueError('rendered site directory missing')
    entries = []
    for path in sorted(root.rglob('*')):
        if path.is_symlink():
            raise ValueError('rendered site must not contain symlinks')
        if path.is_file():
            entries.append([path.relative_to(root).as_posix(), digest(path)])
    if not entries:
        raise ValueError('rendered site is empty')
    return hashlib.sha256(json.dumps(entries, separators=(',', ':')).encode()).hexdigest()


def site_archive_digest(path):
    entries, names, total = [], set(), 0
    with zipfile.ZipFile(path) as archive:
        if len(archive.infolist()) > 10000:
            raise ValueError('site archive has too many members')
        for info in archive.infolist():
            name = PurePosixPath(info.filename)
            if name.is_absolute() or '..' in name.parts or '\\' in info.filename or stat.S_ISLNK(info.external_attr >> 16):
                raise ValueError('unsafe site archive path')
            if info.is_dir():
                continue
            total += info.file_size
            if total > 2 * 1024 ** 3 or str(name) in names:
                raise ValueError('oversized or duplicate site archive member')
            names.add(str(name))
            with archive.open(info) as stream:
                entries.append([str(name), hashlib.file_digest(stream, 'sha256').hexdigest()])
    if not entries:
        raise ValueError('site archive is empty')
    entries.sort()
    return hashlib.sha256(json.dumps(entries, separators=(',', ':')).encode()).hexdigest()


def text(value):
    return isinstance(value, str) and bool(value.strip())


def bound_file(bundle, ref):
    if not isinstance(ref, dict) or not text(ref.get('path')):
        raise ValueError('file reference requires path and sha256')
    name = Path(ref['path'])
    if name.is_absolute():
        raise ValueError('file reference must be relative to the bundle')
    path = (bundle / name).resolve()
    if not path.is_relative_to(bundle.resolve()) or not path.is_file():
        raise ValueError('missing or out-of-bundle file: ' + ref['path'])
    if not isinstance(ref.get('sha256'), str) or not SHA.fullmatch(ref['sha256']):
        raise ValueError('invalid sha256: ' + ref['path'])
    if digest(path) != ref['sha256']:
        raise ValueError('checksum mismatch: ' + ref['path'])
    return path


def validate(data, bundle, phase='prepare', ledger=None):
    errors = []
    def need(ok, message):
        if not ok:
            errors.append(message)
    def file(ref, label):
        try:
            return bound_file(bundle, ref)
        except (ValueError, OSError, TypeError) as error:
            errors.append(label + ': ' + str(error))
    if not isinstance(data, dict):
        return ['record must be an object']
    need(data.get('schema_version') == 1, 'schema_version must be 1')
    kind, version, channel = data.get('kind'), data.get('version'), data.get('channel')
    need(kind in {'application', 'website'}, 'kind must be application or website')
    need(text(data.get('product')), 'product is required')
    need(isinstance(version, str) and bool(VERSION.fullmatch(version)), 'full semver version is required')
    need(channel in {'preview', 'qualified', 'website'}, 'invalid channel')
    need((kind == 'website') == (channel == 'website'), 'kind/channel disagree')
    if kind == 'application' and channel == 'preview':
        need(isinstance(version, str) and '-' in version, 'preview needs an experiment/prerelease suffix and revision')
        need(isinstance(version, str) and bool(re.search(r'\.[1-9][0-9]*$', version)), 'preview needs a numeric attempt revision')
    source = data.get('source', {})
    if not isinstance(source, dict):
        source = {}
    need(isinstance(source.get('revision'), str) and bool(REV.fullmatch(source['revision'])), 'full source revision required')
    need(source.get('dirty') is False, 'source must be clean')
    need(text(source.get('reviewed_by')), 'source reviewer required')
    contract = data.get('interface_contract')
    path = file(contract, 'interface contract')
    canonical = ROOT / 'emulator/ui/interface-contract.json'
    if path and canonical.is_file():
        need(digest(path) == digest(canonical), 'interface contract differs from the canonical contract')
    notes = file(data.get('notes'), 'release notes')
    notes_text = notes.read_text() if notes else ''
    need(text(notes_text) and isinstance(version, str) and version in notes_text, 'release notes must name the full version')
    assets, targets = data.get('artifacts'), data.get('targets')
    need(isinstance(assets, list) and bool(assets), 'public artifacts are required')
    asset_paths, roles, asset_names = {}, set(), set()
    for asset in assets if isinstance(assets, list) else []:
        path = file(asset, 'public artifact')
        if not isinstance(asset, dict):
            continue
        role = asset.get('role')
        need(role in {'package', 'source', 'manifest', 'site'}, 'invalid artifact role')
        roles.add(role)
        if path:
            need(path.name not in asset_names, 'duplicate public asset filename: ' + path.name)
            need(not path.name.startswith('-'), 'asset filename must not start with a dash')
            asset_names.add(path.name)
            need(type(asset.get('size')) is int and asset['size'] == path.stat().st_size, 'artifact size mismatch: ' + path.name)
            asset_paths[asset['path']] = asset
    if kind == 'application':
        need({'package', 'source', 'manifest'} <= roles, 'application needs package, matching source and public manifest')
        need(isinstance(targets, list) and bool(targets), 'application targets are required')
    else:
        need('site' in roles, 'website needs a retained rendered site artifact')
        need(targets == [], 'website targets must be empty')
        need(isinstance(data.get('site_tree_sha256'), str) and bool(SHA.fullmatch(data['site_tree_sha256'])), 'website needs a rendered tree checksum')
        for name, asset in asset_paths.items():
            if asset.get('role') == 'site':
                try:
                    need(site_archive_digest(bound_file(bundle, asset)) == data.get('site_tree_sha256'), 'retained site ZIP differs from the rendered tree')
                except (ValueError, OSError, zipfile.BadZipFile, RuntimeError) as error:
                    errors.append('site archive: ' + str(error))
    target_keys, covered = set(), set()
    for target in targets if isinstance(targets, list) else []:
        if not isinstance(target, dict):
            errors.append('target must be an object')
            continue
        platform, arch = target.get('platform'), target.get('architecture')
        need(platform in {'macos', 'windows', 'android', 'linux', 'ios'}, 'invalid target platform')
        need(isinstance(arch, str) and arch in ARCH_LABELS, 'unsupported target architecture')
        key = (platform, arch)
        need(key not in target_keys, 'duplicate target')
        target_keys.add(key)
        asset = asset_paths.get(target.get('artifact'))
        need(asset is not None and asset.get('role') == 'package', 'target must reference a package artifact')
        if not asset:
            continue
        covered.add(target['artifact'])
        need(isinstance(target.get('capabilities'), list) and bool(target['capabilities']) and all(text(x) for x in target['capabilities']), 'target capability list required')
        identity = target.get('identity', {})
        if not isinstance(identity, dict):
            identity = {}
        need(identity.get('artifact_sha256') == asset['sha256'], 'installed identity belongs to another artifact')
        need(identity.get('version_source') == 'packaged_metadata', 'identity must read packaged metadata')
        for field in ('installed_name', 'program_name', 'window_title', 'installer_name', 'manifest_name'):
            label = identity.get(field)
            need(text(label) and isinstance(version, str) and version in label, field + ' must include the full version')
            need(identity_architecture_matches(label, platform, arch, field, version), field + ' must include architecture')
            normalized_label = re.sub(r'[^a-z0-9]', '', label.lower()) if text(label) else ''
            normalized_product = re.sub(r'[^a-z0-9]', '', data['product'].lower()) if text(data.get('product')) else ''
            need(bool(normalized_product) and normalized_product in normalized_label, field + ' must name the product')
        need(identity.get('installer_name') == Path(asset['path']).name, 'installer name differs from package filename')
        file(identity.get('evidence'), 'installed identity evidence')
        validate_security_access(target, asset, version, source.get('revision'), file, need)
        manifest = asset_paths.get(target.get('manifest'))
        need(manifest is not None and manifest.get('role') == 'manifest', 'target needs a public package manifest')
        if manifest:
            need(identity.get('manifest_name') == Path(manifest['path']).name, 'manifest name differs from public manifest filename')
            try:
                metadata = json.loads(bound_file(bundle, manifest).read_text())
                expected = {'product': data['product'], 'version': version, 'source_revision': source.get('revision'),
                            'platform': platform, 'architecture': arch, 'artifact_sha256': asset['sha256'],
                            'capabilities': target.get('capabilities'),
                            'security_access': security_summary(target)}
                need(isinstance(metadata, dict) and all(metadata.get(k) == v for k, v in expected.items()), 'public manifest differs from candidate identity/capabilities')
            except (ValueError, OSError, TypeError) as error:
                errors.append('public manifest: ' + str(error))
        entries = (ledger or {}).get('entries', [])
        for prior in entries:
            if (prior.get('product'), prior.get('version'), prior.get('platform'), prior.get('architecture')) == (data.get('product'), version, platform, arch):
                need(prior.get('artifact_sha256') == asset['sha256'] and prior.get('capabilities') == target.get('capabilities'), 'version reused for changed executable/capabilities')
                need(prior.get('security_access') == security_summary(target), 'version reused for changed security-access policy')
    need(covered == {p for p, a in asset_paths.items() if a.get('role') == 'package'}, 'every package must have its own target identity')
    checks = data.get('checks', {})
    if not isinstance(checks, dict):
        checks = {}
    need(set(checks) == set(CHECKS), 'all ten commandment checks are required')
    for name in CHECKS:
        check = checks.get(name, {})
        if not isinstance(check, dict):
            errors.append(name + ': check must be an object')
            continue
        status = check.get('status')
        allowed = {'pass'}
        if name == 'deployment' and phase == 'prepare':
            allowed.add('pending')
        if name == 'artifact_tests' and channel == 'preview':
            allowed.add('pending')
        if kind == 'website' and name in {'identity', 'artifact_tests', 'user_data'}:
            allowed.add('not_applicable')
        need(status in allowed, name + ': unresolved or invalid status')
        need(text(check.get('note')), name + ': scope/result note required')
        evidence = check.get('evidence')
        if status in {'pass', 'pending'} and not (name == 'deployment' and phase == 'prepare' and status == 'pending'):
            need(isinstance(evidence, list) and bool(evidence), name + ': evidence required')
        for ref in evidence if isinstance(evidence, list) else []:
            file(ref, name + ' evidence')
        if status == 'pending' and name != 'deployment':
            need(text(check.get('note')) and check['note'] in notes_text, 'pending test limit must appear verbatim in release notes')
    surfaces = data.get('surfaces')
    need(isinstance(surfaces, list) and len(surfaces) == len(SURFACES), 'five publication surfaces required')
    if isinstance(surfaces, list):
        need({s.get('name') for s in surfaces if isinstance(s, dict)} == SURFACES, 'publication surfaces differ')
        for surface in surfaces:
            if not isinstance(surface, dict):
                errors.append('publication surface must be an object')
                continue
            need(surface.get('action') in {'update', 'unchanged'}, 'surface action must be update or unchanged')
            need(text(surface.get('note')), 'surface scope/reason required')
            file(surface.get('evidence'), 'publication surface evidence')
    rollback = data.get('rollback', {})
    need(isinstance(rollback, dict) and text(rollback.get('target')) and text(rollback.get('procedure')), 'rollback target and procedure required')
    need(text(data.get('reviewed_by')), 'deployment record reviewer required')
    if channel == 'qualified':
        qualification = data.get('baseline_qualification', {})
        candidate = file(qualification.get('status') if isinstance(qualification, dict) else None, 'baseline status')
        if candidate:
            from baseline import BASE, validate as baseline_validate
            try:
                errors.extend(baseline_validate(json.loads(candidate.read_text()), json.loads((BASE / 'contract.json').read_text()), True, bundle))
            except (ValueError, OSError, KeyError, TypeError) as error:
                errors.append('baseline qualification: ' + str(error))
    return errors


def verify_source(data, source_root):
    def git(*args):
        return subprocess.check_output(['git', '-C', str(source_root), *args], text=True, stderr=subprocess.PIPE).strip()
    if git('rev-parse', 'HEAD') != data['source']['revision']:
        raise ValueError('source checkout does not match the recorded revision')
    if git('status', '--porcelain', '--untracked-files=all'):
        raise ValueError('source checkout is dirty; use a clean reviewed snapshot')


def load_ledger(path):
    if not path.exists():
        return {'schema_version': 1, 'entries': []}
    data = json.loads(path.read_text())
    if not isinstance(data, dict) or data.get('schema_version') != 1 or not isinstance(data.get('entries'), list) or any(not isinstance(x, dict) for x in data['entries']):
        raise ValueError('malformed deployment ledger')
    return data


def reserve(data, path):
    """Reserve attempted identities before remote writes, with a fail-closed lock."""
    path.parent.mkdir(parents=True, exist_ok=True)
    lock = path.with_suffix(path.suffix + '.lock')
    fd = os.open(lock, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
    try:
        os.close(fd)
        ledger = load_ledger(path)
        assets = {a['path']: a for a in data['artifacts']}
        for target in data['targets']:
            entry = {k: data[k] for k in ('product', 'version')}
            entry.update({k: target[k] for k in ('platform', 'architecture', 'capabilities')})
            entry['artifact_sha256'] = assets[target['artifact']]['sha256']
            entry['security_access'] = security_summary(target)
            key = ('product', 'version', 'platform', 'architecture')
            prior = [x for x in ledger['entries'] if all(x.get(k) == entry[k] for k in key)]
            if prior and any(x != entry for x in prior):
                raise ValueError('version already reserved for different contents/capabilities')
            if not prior:
                ledger['entries'].append(entry)
        with tempfile.NamedTemporaryFile(mode='w', dir=path.parent, delete=False) as stream:
            temp = Path(stream.name)
            json.dump(ledger, stream, indent=2)
            stream.write('\n')
        try:
            os.replace(temp, path)
        finally:
            temp.unlink(missing_ok=True)
    finally:
        lock.unlink()


def template():
    ref = {'path': '', 'sha256': ''}
    return {
        'schema_version': 1, 'kind': 'application', 'product': 'OpenSAAB',
        'version': '', 'channel': 'preview', 'reviewed_by': '',
        'source': {'revision': '', 'dirty': False, 'reviewed_by': ''},
        'interface_contract': ref.copy(), 'notes': ref.copy(),
        'release': {'repository': 'djfremen/OpenSAAB-T2', 'tag': '', 'title': ''},
        'artifacts': [], 'targets': [],
        'checks': {name: {'status': 'pending', 'note': '', 'evidence': []} for name in CHECKS},
        'surfaces': [{'name': name, 'action': 'update', 'note': '', 'evidence': ref.copy()} for name in sorted(SURFACES)],
        'rollback': {'target': '', 'procedure': ''},
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--init', type=Path, help='write an incomplete record template; never overwrite')
    parser.add_argument('--record', type=Path)
    parser.add_argument('--bundle', type=Path)
    parser.add_argument('--source-root', type=Path)
    parser.add_argument('--site-root', type=Path, help='rendered site directory; required for website records')
    parser.add_argument('--site-digest', type=Path, help='print a rendered directory checksum and exit')
    parser.add_argument('--expect-kind', choices=['application', 'website'])
    parser.add_argument('--phase', choices=['prepare', 'complete'], default='prepare')
    parser.add_argument('--receipt', type=Path, help='new local verification receipt; required for completion')
    parser.add_argument('--ledger', type=Path, default=Path.home() / '.local/share/opensaab/deployment-ledger.json')
    args = parser.parse_args()
    try:
        if args.site_digest:
            print(tree_digest(args.site_digest))
            return 0
        if args.init:
            with args.init.open('x') as stream:
                json.dump(template(), stream, indent=2)
                stream.write('\n')
            print('Created incomplete template. Fill evidence before checking; no release authorized or published.')
            return 0
        if not args.record or not args.bundle or not args.source_root:
            parser.error('--record, --bundle and --source-root are required')
        if args.phase == 'complete' and not args.receipt:
            parser.error('--receipt is required for completion')
        if args.receipt and (args.receipt.exists() or not args.receipt.parent.is_dir()):
            raise ValueError('receipt must be a new file in an existing directory')
        data = json.loads(args.record.read_text())
        errors = validate(data, args.bundle, args.phase, load_ledger(args.ledger))
        if args.expect_kind and data.get('kind') != args.expect_kind:
            errors.append('deployment kind differs from the entry point')
        if data.get('kind') == 'website':
            if not args.site_root:
                errors.append('--site-root is required for website records')
            elif tree_digest(args.site_root) != data.get('site_tree_sha256'):
                errors.append('rendered site differs from reviewed deployment record')
        if not errors:
            verify_source(data, args.source_root)
        if errors:
            print('\n'.join('BLOCKED: ' + e for e in errors))
            return 1
        if args.receipt:
            with args.receipt.open('x') as stream:
                json.dump({'schema_version': 1, 'phase': args.phase, 'result': 'pass',
                           'verified_at': datetime.now(timezone.utc).isoformat(),
                           'record_sha256': digest(args.record), 'version': data['version'],
                           'source_revision': data['source']['revision'], 'reviewed_by': data['reviewed_by'],
                           'artifact_hashes': {a['path']: a['sha256'] for a in data['artifacts']}}, stream, indent=2)
                stream.write('\n')
        print('PASS: ' + ('prepublication evidence verified; live deployment completion remains separate' if args.phase == 'prepare' else 'declared completion evidence verified'))
        return 0
    except (ValueError, OSError, KeyError, TypeError, AttributeError, subprocess.SubprocessError) as error:
        print('BLOCKED: ' + str(error))
        return 1


if __name__ == '__main__':
    sys.exit(main())
