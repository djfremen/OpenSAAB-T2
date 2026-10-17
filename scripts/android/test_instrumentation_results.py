import json
from pathlib import Path
import tempfile
import unittest

from instrumentation_results import InstrumentationResults


class ResultRetentionTest(unittest.TestCase):
    def test_prior_pass_does_not_hide_later_instrumentation_failure(self):
        with tempfile.TemporaryDirectory() as directory:
            results = InstrumentationResults(directory)
            results.record("earlier", "PASS: earlier\nINSTRUMENTATION_CODE: -1\n")
            with self.assertRaises(RuntimeError):
                results.record("MCP", "FAIL: Installed version unavailable\nINSTRUMENTATION_CODE: 0\n")
            rows = json.loads((Path(directory) / "results.json").read_text())
            self.assertEqual([(row["suite"], row["pass"]) for row in rows],
                             [("earlier", True), ("MCP", False)])
            results.finish()
            self.assertEqual(json.loads((Path(directory) / "run-status.json").read_text()),
                             {"status": "failed", "attempted": 2, "passed": 1, "failed": 1})

    def test_process_failure_and_timeout_are_retained_before_raising(self):
        for returncode, error in [(1, None), (None, "instrumentation timed out")]:
            with self.subTest(returncode=returncode, error=error), tempfile.TemporaryDirectory() as directory:
                results = InstrumentationResults(directory)
                with self.assertRaises(RuntimeError):
                    results.record("attempt", "", "process failure", returncode, error)
                row = json.loads((Path(directory) / "results.json").read_text())[0]
                self.assertFalse(row["pass"])
                self.assertEqual(row["returncode"], returncode)
                self.assertEqual(json.loads((Path(directory) / "run-status.json").read_text())["status"], "failed")


if __name__ == "__main__":
    unittest.main()
