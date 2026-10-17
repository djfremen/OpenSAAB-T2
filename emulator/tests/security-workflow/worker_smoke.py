#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""Exercise the packaged worker in an isolated synthetic workspace, never a vehicle."""
import base64
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile

binary = Path(sys.argv[1]).resolve()
SIZE, OFFSET = 714, 0xFE0000
PRIMARY = "https://relevant-diann-djfremen2-c013cdc3.koyeb.app/api/process"
PROTECTED = {'authorize', 'prepare', 'prepare_existing', 'http_result', 'import'}

def call(op, **fields):
    result = subprocess.run([str(binary)], input=json.dumps(dict(schema=1, op=op, **fields)),
                            text=True, capture_output=True, timeout=30, check=True)
    envelope = json.loads(result.stdout)
    assert envelope['ok'] is True
    return envelope['result']

def blocked(op, **fields):
    result = subprocess.run([str(binary)], input=json.dumps(dict(schema=1, op=op, **fields)),
                            text=True, capture_output=True, timeout=30)
    envelope = json.loads(result.stdout)
    assert result.returncode != 0 and envelope['ok'] is False
    assert 'cancelled' in envelope['error'] or 'cancellation state' in envelope['error'] or 'API changed' in envelope['error'] or 'Invalid' in envelope['error']
    assert 'result' not in envelope

assert call('authorize')['mode'] == 'no_operator_password'
for op in PROTECTED:
    blocked(op, cancelled=True)
    blocked(op, cancelled='false')
for value in ['', 'wrong', 'opensaab', None, False]:
    assert call('authorize', password=value)['authorized'] is True
print('PASS packaged open access: no operator credential; cancellation and malformed cancellation rejected on every processing route')

def write(path, value):
    path.write_text(json.dumps(value))

with tempfile.TemporaryDirectory(prefix='opensaab-security-worker-test-') as folder:
    state = Path(folder).resolve()
    run = state / 'runs' / 'collected'
    run.mkdir(parents=True)
    (state / 'firmware').mkdir()
    (state / 'firmware' / 'active.txt').write_text('synthetic')
    write(state / 'vehicle-status.json', dict(vin='YS3FF49Y541000000'))
    write(state / 'latest-session.json', dict(run=str(run), firmware='synthetic'))
    card = bytearray(b'\x42' * (32 * 1024 * 1024))
    before = b'\xff' * SIZE
    card[OFFSET:OFFSET+SIZE] = before
    (run / 'card.bin').write_bytes(card)
    (run / 'ssa-card-before.bin').write_bytes(before)
    collected = bytearray(before)
    collected[0] = 0xb1
    collected[20:37] = b'YS3FF49Y541000000'
    collected[0x132:0x138] = bytes([0, 1, 3, 0x61, 0x12, 0x34])
    (run / 'ssa-card-after.bin').write_bytes(collected)
    (run / 'screen.txt').write_text('You need Security Access from TIS2000\nDisconnect Tech 2 from Vehicle')
    write(run / 'report.json', {})
    write(run / 'native-security-snapshot.json', dict(origin='original-guest-memory',card_offset=OFFSET,
        bytes=SIZE,ssa_memory_flash_enabled=True,ssa_erases=1,ssa_programmed_bytes=SIZE))
    # Regression: restart at the splash screen, offline, with the same saved seeds.
    reopened = state / 'runs' / 'reopened'
    reopened.mkdir()
    card[OFFSET:OFFSET+SIZE] = collected
    (reopened / 'card.bin').write_bytes(card)
    (reopened / 'working-ssa.bin').write_bytes(collected)
    (reopened / 'screen.txt').write_text('Tech 2 — Press ENTER to continue')
    write(reopened / 'report.json', {})
    write(state / 'latest-session.json', dict(run=str(reopened), firmware='synthetic', mode='offline'))
    run = reopened
    selected = call('inspect_existing', state=str(state), run=str(run), firmware='synthetic')
    blocked('prepare_existing', state=str(state), run=str(run), firmware='synthetic',
            input_hash=selected['input_hash'], cancelled=True)
    prepared = call('prepare_existing', state=str(state), run=str(run), firmware='synthetic',
                    allow_fallback=False, input_hash=selected['input_hash'])
    assert base64.b64decode(prepared['payload']['SSA_DATA']) == collected
    after = collected.copy()
    after[1] = 0
    after[0x26:0x2e] = b'AAAAAAAA'
    after[0x138:0x13a] = b'\x56\x78'
    blocked('import', attempt=prepared['attempt'], provider=PRIMARY, cancelled=True,
            response=dict(SSA_DATA=base64.b64encode(after).decode()))
    corrupt = after.copy(); corrupt[20] = ord('A')
    blocked('import', attempt=prepared['attempt'], provider=PRIMARY,
            response=dict(SSA_DATA=base64.b64encode(corrupt).decode()))
    assert (run / 'card.bin').read_bytes() == card
    assert not (Path(prepared['attempt']) / 'result.json').exists()
    result = call('import', attempt=prepared['attempt'], provider=PRIMARY,
        response=dict(SSA_DATA=base64.b64encode(after).decode()))
    assert result['vehicle_access_verified'] is False
    active = Path(json.loads((state / 'latest-session.json').read_text())['run'])
    assert active != run and (run / 'card.bin').read_bytes() == card
    expected = card.copy()
    expected[OFFSET:OFFSET+SIZE] = after
    assert (active / 'card.bin').read_bytes() == expected
    receipt = dict(line.split('=',1) for line in (active / 'security-processing-status.properties').read_text().splitlines())
    assert receipt['stage'] == 'imported' and receipt['ssa_hash'] == hashlib.sha256(after).hexdigest()
    assert (Path(prepared['attempt']) / 'card-before-security.bin').read_bytes() == card
    assert call('http_result', status=503, network='', primary=True, allow_fallback=False)['action'] == 'error'
    assert call('http_result', status=503, network='', primary=True, allow_fallback=True)['action'] == 'fallback'
print('PASS packaged worker: reopened/offline existing seeds -> synthetic reply -> verified SSA-only import -> bound receipt; original retained; no network/vehicle')
