#!/usr/bin/env python3
"""Validate parity inventory; qualification additionally binds receipts to artifact hashes.

This checks declared evidence and provenance, not the truth of manual test claims.
No hardware calls, downloads, publishing, or user-data changes occur here.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[2]
BASE = ROOT / 'releases/consistent-baseline'
HEX40 = re.compile(r'[0-9a-f]{40}')
HEX64 = re.compile(r'[0-9a-f]{64}')

def local_file(root, name):
    if not isinstance(name, str) or not name or Path(name).is_absolute():
        raise ValueError('Expected a relative evidence/artifact path')
    path = (root / name).resolve()
    if not path.is_relative_to(root.resolve()) or not path.is_file():
        raise ValueError('Missing or out-of-root file: ' + name)
    return path

def sha(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()

def is_visual_evidence(path):
    """Reject text/checklist substitutes; content review still requires an observer."""
    with path.open('rb') as stream:
        head = stream.read(16)
    return ((path.suffix.lower() == '.png' and head.startswith(b'\x89PNG\r\n\x1a\n'))
            or (path.suffix.lower() in {'.jpg', '.jpeg'} and head.startswith(b'\xff\xd8\xff'))
            or (path.suffix.lower() == '.webp' and head[:4] == b'RIFF' and head[8:12] == b'WEBP')
            or (path.suffix.lower() == '.mp4' and head[4:8] == b'ftyp')
            or (path.suffix.lower() == '.webm' and head.startswith(b'\x1a\x45\xdf\xa3')))

def validate(data, contract, qualify=False, artifacts=None):
    errors = []
    def need(ok, message):
        if not ok: errors.append(message)
    need(data.get('schema_version') == 1, 'schema_version must be 1')
    need(data.get('contract') == contract['id'], 'Wrong baseline contract')
    targets = data.get('targets', {})
    need(set(targets) == set(contract['targets']), 'Target set must exactly match contract')
    shared = data.get('shared_components', {})
    interface_file = local_file(ROOT, contract['interface_contract'])
    interface = json.loads(interface_file.read_text())
    interface_sha = sha(interface_file)
    need(shared.get('interface_contract_sha256') == interface_sha,
         'Interface reference changed; review and update the shared contract pin')
    need(set(shared) == set(contract['shared_components']), 'Shared component keys differ')
    for name in contract['shared_components']:
        value = shared.get(name)
        pattern = HEX40 if name.endswith('revision') else HEX64
        need(value is None or isinstance(value, str) and pattern.fullmatch(value), 'Invalid shared ' + name)
        if qualify: need(value is not None, 'Unpinned shared ' + name)
    for ident, target in targets.items():
        prefix = ident + ': '
        need(target.get('source_repository') in {'OpenSAAB', 'OpenSAAB-T2'}, prefix+'Unknown source repository')
        need(isinstance(target.get('source_revision'), str) and HEX40.fullmatch(target['source_revision']), prefix+'Full source revision required')
        need(type(target.get('source_dirty')) is bool, prefix+'source_dirty must be boolean')
        components = target.get('shared_components', {})
        need(set(components) == set(contract['shared_components']), prefix+'Shared component keys differ')
        for name in contract['shared_components']:
            value = components.get(name)
            pattern = HEX40 if name.endswith('revision') else HEX64
            need(value is None or isinstance(value, str) and pattern.fullmatch(value), prefix+'Invalid shared '+name)
        checks = target.get('checks', {})
        need(set(checks) == set(contract['checks']), prefix+'Check set must exactly match contract')
        for name, check in checks.items():
            need(check.get('status') in contract['statuses'], prefix+name+' invalid status')
            need(isinstance(check.get('note'), str) and bool(check['note'].strip()), prefix+name+' needs a scope/limitation note')
            evidence = check.get('evidence')
            need(isinstance(evidence, list), prefix+name+' evidence must be a list')
            for ref in evidence if isinstance(evidence, list) else []:
                # Pinned companion-repository references identify local commits; they
                # do not imply that the commit has been pushed or a URL is available.
                if isinstance(ref, str) and re.fullmatch(r'OpenSAAB-T2@[0-9a-f]{40}:[\w./-]+', ref):
                    continue
                try: local_file(ROOT, ref)
                except ValueError as e: errors.append(prefix+str(e))
            if check.get('status') == 'verified': need(bool(evidence), prefix+name+' verified without evidence')
            if qualify: need(check.get('status') == 'verified', prefix+name+' not qualified ('+str(check.get('status'))+')')
        if not qualify: continue
        need(target.get('source_dirty') is False, prefix+'Dirty/unrecorded source')
        need(components == shared, prefix+'Shared core/catalog drift')
        artifact = target.get('artifact')
        receipt = target.get('qualification')
        if not isinstance(artifact, dict) or not isinstance(receipt, dict):
            errors.append(prefix+'Artifact and qualification receipt required'); continue
        if artifacts is None:
            errors.append(prefix+'--artifacts required for qualification'); continue
        try:
            file = local_file(artifacts, artifact.get('path'))
            digest = sha(file)
            need(digest == artifact.get('sha256'), prefix+'Artifact checksum mismatch')
            need(file.stat().st_size == artifact.get('size'), prefix+'Artifact size mismatch')
            receipt_file = local_file(artifacts, receipt.get('path'))
            need(sha(receipt_file) == receipt.get('sha256'), prefix+'Receipt checksum mismatch')
            record = json.loads(receipt_file.read_text())
            need(record.get('target') == ident, prefix+'Receipt target mismatch')
            need(record.get('artifact_sha256') == digest, prefix+'Receipt belongs to another artifact')
            need(record.get('source_revision') == target['source_revision'], prefix+'Receipt source mismatch')
            need(record.get('shared_components') == shared, prefix+'Receipt component mismatch')
            need(bool(record.get('environment')) and bool(record.get('tested_at')) and bool(record.get('reviewed_by')), prefix+'Receipt missing environment/date/reviewer')
            results = record.get('checks', {})
            need(set(results) == set(contract['checks']), prefix+'Receipt check set differs')
            for check_name, result in results.items():
                need(result.get('result') == 'pass', prefix+check_name+' receipt did not pass')
                evidence_file = local_file(artifacts, result.get('evidence_path'))
                need(sha(evidence_file) == result.get('evidence_sha256'), prefix+check_name+' evidence checksum mismatch')
            visual = record.get('interface_conformance', {})
            need(visual.get('reference_sha256') == interface_sha, prefix+'Interface evidence uses a different reference')
            scenarios = visual.get('scenarios', {})
            need(set(scenarios) == set(interface['scenarios']), prefix+'Interface scenario set differs')
            for scenario, result in scenarios.items():
                need(result.get('result') == 'pass', prefix+scenario+' interface scenario did not pass')
                evidence_file = local_file(artifacts, result.get('evidence_path'))
                need(sha(evidence_file) == result.get('evidence_sha256'), prefix+scenario+' visual evidence checksum mismatch')
                need(is_visual_evidence(evidence_file), prefix+scenario+' requires screenshot/video evidence')
        except (ValueError, OSError, TypeError) as e:
            errors.append(prefix+str(e))
    return errors

def markdown(data, contract):
    targets = contract['targets']
    lines = ['# OpenSAAB Consistent Baseline — current matrix', '',
             'Generated from `status.json`. Inventory is not release qualification. Historical evidence is scoped in the inventory notes.', '',
             '| Requirement | '+' | '.join(targets.values())+' |',
             '| --- | '+' | '.join('---' for _ in targets)+' |']
    for key, label in contract['checks'].items():
        lines.append('| '+label+' | '+' | '.join(data['targets'][t]['checks'][key]['status'] for t in targets)+' |')
    lines += ['', '**Not release-ready.**' if any(c['status']!='verified' for t in data['targets'].values() for c in t['checks'].values()) else '**Feature inventory complete; artifact qualification still required.**', '',
              '`verified` means the documented check passed in its stated environment. `partial` means narrower evidence exists; `implemented` is not a live test; `missing`, `blocked`, and `unverified` are unresolved. Same-source and exact-artifact receipts remain mandatory.', '']
    return '\n'.join(lines)

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--status',type=Path,default=BASE/'status.json')
    p.add_argument('--qualify',action='store_true')
    p.add_argument('--artifacts',type=Path)
    p.add_argument('--write-matrix',action='store_true')
    p.add_argument('--check-matrix',action='store_true')
    args=p.parse_args()
    try:
        data=json.loads(args.status.read_text());contract=json.loads((BASE/'contract.json').read_text())
        errors=validate(data,contract,args.qualify,args.artifacts)
        if errors:
            print('\n'.join('BLOCKED: '+e for e in errors));return 1
        matrix=markdown(data,contract);path=BASE/'MATRIX.md'
        if args.write_matrix:path.write_text(matrix)
        if args.check_matrix and (not path.exists() or path.read_text()!=matrix):
            print('BLOCKED: MATRIX.md is stale; run --write-matrix');return 1
        print('PASS: '+('declared evidence and exact artifacts qualified; publication is separate' if args.qualify else 'inventory valid; no release-readiness claim'))
        return 0
    except (ValueError, OSError, KeyError, TypeError, AttributeError) as e:
        print('BLOCKED: malformed input: '+str(e));return 1
if __name__=='__main__':sys.exit(main())
