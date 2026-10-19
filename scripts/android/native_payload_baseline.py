"""Require explicitly unchanged native payloads to match an approved APK baseline."""
import hashlib
import re
from pathlib import Path

def verify_baseline(spec, digests):
    if set(spec)!={'schema','baseline_version','baseline_apk_sha256','unchanged','allowed_changed'} or spec['schema']!=1:
        raise ValueError('Invalid native payload baseline schema')
    if not isinstance(spec['baseline_version'],str) or not re.fullmatch('[0-9a-f]{64}',spec['baseline_apk_sha256']):
        raise ValueError('Invalid native baseline identity')
    fixed=spec['unchanged'];changed=spec['allowed_changed']
    if not isinstance(fixed,dict) or not isinstance(changed,list) or len(changed)!=len(set(changed)) or set(fixed)&set(changed):
        raise ValueError('Invalid native baseline coverage')
    if set(fixed)|set(changed)!=set(digests):
        raise ValueError('Native baseline must account for every packaged executable')
    for name,expected in fixed.items():
        if not re.fullmatch('[0-9a-f]{64}',expected) or digests[name]!=expected:
            raise ValueError('Unchanged native payload differs from approved baseline: '+name)
    return len(fixed)

def verify_files(spec, files):
    return verify_baseline(spec,{name:hashlib.sha256(Path(path).read_bytes()).hexdigest() for name,path in files.items()})
