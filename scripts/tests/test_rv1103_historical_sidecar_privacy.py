"""Public historical derivatives preserve commitments without publishing originals."""
import hashlib
import json
from pathlib import Path
import unittest


ROOT = Path(__file__).resolve().parents[2]
EVIDENCE = ROOT / "examples/luckfox-pico-mini-a-linux/evidence"
ORIGINALS = {
    "gate-inputs-2026-10-12.json": (445, "38ed9688fb247b9af42a68963a1ef0abd8affcc7f31cda1adc57292d00e7b9a5", ["artifacts_directory", "boot_receipt", "cli_path"]),
    "cli-identity-2026-10-12.json": (472, "37beaf927d1fae43fde0d0f934c4528298d8201a1f8ff3682ce86c38be0573b4", ["cli", "build_log"]),
}


class HistoricalSidecarPrivacy(unittest.TestCase):
    def setUp(self):
        self.index = json.loads((EVIDENCE / "historical-sidecars-provenance.json").read_text())

    def test_original_commitments_and_absence(self):
        entries = self.index["entries"]
        self.assertEqual({x["original_file"] for x in entries}, set(ORIGINALS))
        self.assertEqual(len(entries), 2)
        for entry in entries:
            name = entry["original_file"]
            size, sha, fields = ORIGINALS[name]
            self.assertEqual(entry["original"], {"bytes": size, "sha256": sha})
            self.assertEqual(entry["redacted_fields"], fields)
            self.assertEqual(entry["original_availability"], "private-offline")
            self.assertFalse((EVIDENCE / name).exists())

    def test_derivative_identities_and_redaction_inventory(self):
        for entry in self.index["entries"]:
            name = entry["public_derivative"]
            self.assertEqual(name, Path(name).name)
            self.assertTrue(name.endswith(".redacted.json"))
            raw = (EVIDENCE / name).read_bytes()
            self.assertEqual(entry["derivative"], {"bytes": len(raw), "sha256": hashlib.sha256(raw).hexdigest()})
            derivative = json.loads(raw)
            self.assertEqual(derivative["kind"], "redacted-historical-observation")
            self.assertEqual(derivative["original"], entry["original"])
            self.assertEqual(derivative["redacted_fields"], entry["redacted_fields"])
            replaced = {k for k, v in derivative["observation"].items() if v == "[REDACTED_LOCAL_PATH]"}
            self.assertEqual(replaced, set(entry["redacted_fields"]))
            for prefix in (b"/home/", b"/mnt/", b"/tmp/"):
                self.assertNotIn(prefix, raw)

    def test_observed_cli_identity_and_not_run_gate_are_preserved(self):
        gate = json.loads((EVIDENCE / "gate-inputs-2026-10-12.redacted.json").read_text())["observation"]
        cli = json.loads((EVIDENCE / "cli-identity-2026-10-12.redacted.json").read_text())["observation"]
        sha = "d2d11eef64c9879ab97adf164e7cae0b45fa08c5556ea19f55f1da7b490c0055"
        self.assertEqual(gate["cli_sha256"], sha)
        self.assertEqual(cli["sha256"], sha)
        self.assertEqual(gate["guest_boot"], "NOT_RUN")
        self.assertEqual(gate["gate"], "production_zimage_reaches_pid1_and_advancing_guest_uptime")
        self.assertEqual(cli["bytes"], 56807416)
        self.assertEqual(cli["source_head"], "28bb66c5dbcc52ead1311664551d400db51026ec")
        self.assertEqual(cli["build_exit"], 0)
        self.assertFalse(cli["task4_runtime_rust_changes"])


if __name__ == "__main__":
    unittest.main()
