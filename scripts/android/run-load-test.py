#!/usr/bin/env python3
"""Measure separate APK from before Activity launch; preserve private evidence."""
import argparse
import json
import subprocess
import time
from pathlib import Path

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--serial', required=True)
parser.add_argument('--output', type=Path, required=True)
parser.add_argument('--runs', type=int, default=3)
parser.add_argument('--fresh-firmware', action='store_true')
args = parser.parse_args()
package = 'com.opensaab.loadtest'
adb = ['adb', '-s', args.serial]

def shell(*command, check=True):
    return subprocess.run(adb + ['shell', *command], check=check, capture_output=True, text=True)

args.output.mkdir(parents=True, exist_ok=True)
results = []
for index in range(args.runs):
    shell('am', 'force-stop', package)
    if index == 0 and args.fresh_firmware:
        for name in ['eprom.bin', 'opsys.dwn', 'card.bin']:
            shell('run-as', package, 'rm', '-f', 'files/firmware/' + name)
    shell('run-as', package, 'rm', '-f', 'files/latest-result.json')
    origin = int(float(shell('cat', '/proc/uptime').stdout.split()[0]) * 1000)
    launch = shell('am', 'start', '-W', '-n', package + '/.MainActivity', '--el', 'launch_origin_elapsed_ms', str(origin))
    deadline = time.monotonic() + 120
    result = None
    while time.monotonic() < deadline:
        data = shell('run-as', package, 'cat', 'files/latest-result.json', check=False)
        if data.returncode == 0:
            result = json.loads(data.stdout)
            break
        time.sleep(0.5)
    if result is None:
        raise SystemExit('APK did not produce a result within 120 seconds')
    folder = args.output / result['run_id']
    folder.mkdir(exist_ok=True)
    (folder / 'am-start.txt').write_text(launch.stdout)
    for name in ['load-result.json', 'report.json', 'host-performance.json', 'screen.txt', 'console.log', 'lcd.ppm']:
        data = subprocess.check_output(adb + ['exec-out', 'run-as', package, 'cat', 'files/runs/' + result['run_id'] + '/' + name])
        (folder / name).write_bytes(data)
    result['fresh_firmware_extraction'] = index == 0 and args.fresh_firmware
    results.append(result)
    (args.output / 'results.json').write_text(json.dumps(results, indent=2) + '\n')
    print(json.dumps(result), flush=True)
if not all(r['goal_met'] for r in results):
    raise SystemExit('At least one launch exceeded 10 seconds or did not verify the welcome screen')
