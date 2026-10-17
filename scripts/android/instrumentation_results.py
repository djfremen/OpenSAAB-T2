"""Retain every attempted instrumentation result before returning or raising."""
import hashlib
import json
import os
from pathlib import Path


class InstrumentationResults:
    def __init__(self, directory):
        self.directory = Path(directory)
        self.rows = []
        self.status = "running"
        self.save()

    def save(self):
        documents = {
            "results.json": self.rows,
            "run-status.json": {
                "status": self.status,
                "attempted": len(self.rows),
                "passed": sum(row["pass"] for row in self.rows),
                "failed": sum(not row["pass"] for row in self.rows),
            },
        }
        for name, document in documents.items():
            destination = self.directory / name
            temporary = destination.with_suffix(destination.suffix + ".tmp")
            temporary.write_text(json.dumps(document, indent=2) + "\n")
            os.replace(temporary, destination)

    def record(self, suite, stdout, stderr="", returncode=0, error=None):
        output = stdout + stderr
        log = self.directory / (suite + ".log")
        log.write_text(output)
        passed = (error is None and returncode == 0 and "PASS:" in stdout
                  and "INSTRUMENTATION_CODE: -1" in stdout and "FAIL:" not in output)
        row = {"suite": suite, "pass": passed, "returncode": returncode,
               "log": log.name, "log_sha256": hashlib.sha256(log.read_bytes()).hexdigest()}
        if error is not None:
            row["error"] = str(error)
        self.rows.append(row)
        if not passed:
            self.status = "failed"
        self.save()  # A failing attempt is durable before the runner stops.
        if not passed:
            raise RuntimeError(f"{suite} failed; retained in {self.directory / 'results.json'}")

    def finish(self):
        if self.status != "failed":
            self.status = "pass" if self.rows else "incomplete"
        self.save()
