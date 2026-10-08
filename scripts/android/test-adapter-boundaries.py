#!/usr/bin/env python3
"""Fail if one direct-USB adapter depends on the other's wire implementation."""
from pathlib import Path
import re
repo=Path(__file__).resolve().parents[2]
for bucket,other in [('chipsoft',r'\b(?:nano_(?:usb|channel|backend|native)|vcx_nano)\b'),('vcx_nano',r'\bchipsoft(?:_\w+)?\b')]:
    for p in (repo/'src/adapters'/bucket).glob('*.rs'):
        assert not re.search(other,p.read_text()),f'Cross-adapter dependency in {p}'
for bucket,other in [('chipsoft','NanoProbeActivity'),('vcx_nano','ChipsoftUsbActivity')]:
    for p in (repo/'android/adapters'/bucket).rglob('*.java'):
        assert other not in p.read_text(),f'Cross-adapter owner in {p}'
print('ADAPTER_BOUNDARIES PASS; direct USB implementations do not depend on each other')
