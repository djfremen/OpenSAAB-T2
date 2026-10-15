#!/usr/bin/env python3
"""Verify catalog welcome/deferred CANdi with original cards, without a device.

Cards and support files are supplied separately and hash-checked. Nothing is
downloaded or opened on USB. Optional recorded keys exercise the first guest
dependency with an intentionally missing CANdi image: it must fail there, before
the guest's missing-CANdi cleanup can overwrite the failure with a processor halt.
This is firmware replay evidence, never an installed GUI or hardware pass.
"""
import argparse
import concurrent.futures
import hashlib
import json
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--binary", type=Path, required=True)
    p.add_argument("--support", type=Path, required=True)
    p.add_argument("--cards", type=Path, required=True)
    p.add_argument("--output", type=Path, required=True)
    p.add_argument("--replay-keys", type=Path)
    args = p.parse_args()
    args.binary = args.binary.resolve(strict=True)
    args.output.mkdir(parents=True, exist_ok=False)
    entries = [e for e in json.loads((ROOT / "ui/firmware-catalog.json").read_text())["entries"] if e["runnable"]]
    support = json.loads((ROOT / "ui/support-files.json").read_text())
    for name, spec in support.items():
        file = args.support / name
        assert file.stat().st_size == spec["bytes"] and digest(file) == spec["sha256"], name
    cards = {}
    for e in entries:
        file = args.cards / (e["id"] + ".bin")
        assert file.stat().st_size == 32 * 1024 * 1024 and digest(file) == e["imageSha"], e["id"]
        cards[e["id"]] = file

    def run(e, replay=False):
        directory = args.output / (("dependency-" if replay else "welcome-") + e["id"])
        directory.mkdir()
        candi = directory / "intentionally-missing-candi.bin" if replay else args.support / "candi.bin"
        command = [str(args.binary), "--headless", "--research-harness", "--candi-native-link",
                   "--candi-on-demand", "--boot", str(args.support / "eprom.bin"),
                   "--opsys", str(args.support / "opsys.dwn"), "--candi-firmware", str(candi),
                   "--max-insns", "455000000" if replay else "45000000", "--output-dir", str(directory)]
        if replay:
            command += ["--replay-keys", str(args.replay_keys)]
        command.append(str(cards[e["id"]]))
        (directory / "command.json").write_text(json.dumps(command, indent=2) + "\n")
        with (directory / "console.log").open("w") as f:
            process = subprocess.run(command, stdout=f, stderr=subprocess.STDOUT,
                                     env={**os.environ, "LOAD_TEST_ACCELERATE": "1"}, timeout=300)
        log = (directory / "console.log").read_text()
        report = json.loads((directory / "report.json").read_text())
        startup = json.loads((directory / "startup.json").read_text())
        assert startup["verified_welcome"] and not startup["candi_started"], e["id"]
        if replay:
            assert process.returncode != 0 and "CANDI_STARTING:" in log, e["id"]
            assert "CANdi initialization failed" in report["reason"], report
            assert "ADDRESS ERROR - PROCESSOR HALT" not in log, e["id"]
        else:
            assert process.returncode == 0 and report["status"] == "success", report
            assert "CANDI_STARTING:" not in log and report["candi"] is None, e["id"]
        return {"id": e["id"], "image_sha256": e["imageSha"], "scenario": "dependency" if replay else "welcome",
                "exit": process.returncode, "instructions": report["instructions"], "pc": report["pc"],
                "startup": startup, "console_sha256": digest(directory / "console.log"), "result": "pass"}

    rows = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=3) as pool:
        for row in pool.map(run, entries):
            rows.append(row)
            print(row["id"], row["scenario"], "pass", flush=True)
    if args.replay_keys:
        for e in entries:
            if e["language"] in ["en", "se"]:
                row = run(e, replay=True)
                rows.append(row)
                print(row["id"], row["scenario"], "pass", flush=True)
    (args.output / "verification.json").write_text(json.dumps({
        "schema": 1, "binary_sha256": digest(args.binary), "replay_sha256": digest(args.replay_keys) if args.replay_keys else None,
        "origin": "original-firmware-replay", "physical_adapter_opened": False,
        "gui_qualified": False, "results": rows}, indent=2) + "\n")


if __name__ == "__main__":
    main()
