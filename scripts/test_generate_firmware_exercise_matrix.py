# LabWired - Firmware Simulation Platform
# Copyright (C) 2026 Andrii Shylenko
# SPDX-License-Identifier: MIT
"""Tests for the chip-completeness gate in generate_firmware_exercise_matrix.py.

The rendered document is covered by CI running the generator with `--check`
against the committed doc. What is tested here is the gate every chip must pass
to get there. It exists because a chip missing from the ledger used to render a
document that silently omitted it — nothing failed, so the omission was a
"this ran and told you nothing" bug, and a regression in the gate itself would
rot just as quietly. Each arm is pinned: missing, extra, duplicate, missing
headline, plus the two non-errors — `ci-fixture-*` configs are out of scope, and
a complete ledger writes the doc.

Every case builds a synthetic tree and monkeypatches the generator's four
module-level paths, so the real checkout is never read or written.
"""

import json
import sys
from pathlib import Path

import yaml

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent))

import generate_firmware_exercise_matrix as gfm  # noqa: E402


def make_tree(tmp_path: Path, chips, ledger) -> Path:
    """A synthetic repo: `chips` become configs/chips/<id>.yaml, `ledger` the yaml."""
    root = tmp_path / "repo"
    (root / "configs" / "chips").mkdir(parents=True)
    (root / "docs" / "coverage").mkdir(parents=True)
    (root / "docs" / "boards").mkdir(parents=True)
    (root / "validation").mkdir(parents=True)
    for cid in chips:
        (root / "configs" / "chips" / f"{cid}.yaml").write_text(f"id: {cid}\n")
    (root / "docs" / "coverage" / "tier1-matrix.json").write_text(
        json.dumps({cid: {} for cid in chips if not cid.startswith("ci-fixture")})
    )
    (root / "validation" / "firmware_exercise.yaml").write_text(
        yaml.safe_dump({"chips": ledger}, sort_keys=False)
    )
    return root


def run(monkeypatch, root: Path, *argv) -> int:
    monkeypatch.setattr(gfm, "CORE_ROOT", root)
    monkeypatch.setattr(gfm, "YAML_SRC", root / "validation" / "firmware_exercise.yaml")
    monkeypatch.setattr(gfm, "TIER1_JSON", root / "docs" / "coverage" / "tier1-matrix.json")
    monkeypatch.setattr(gfm, "OUT_DOC", root / "docs" / "boards" / "FIRMWARE_EXERCISE_MATRIX.md")
    monkeypatch.setattr(sys, "argv", ["generate_firmware_exercise_matrix.py", *argv])
    return gfm.main()


def entry(cid: str, headline: str = "A real headline.") -> dict:
    return {"id": cid, "headline": headline}


# ── Each rejecting arm ────────────────────────────────────────────────────────


def test_missing_chip_is_rejected(tmp_path, monkeypatch, capsys):
    root = make_tree(tmp_path, ["demo", "unledgered"], [entry("demo")])
    assert run(monkeypatch, root, "--check") == 1
    err = capsys.readouterr().err
    assert "unledgered" in err
    assert "Add an entry" in err


def test_extra_id_is_rejected(tmp_path, monkeypatch, capsys):
    root = make_tree(tmp_path, ["demo"], [entry("demo"), entry("ghost")])
    assert run(monkeypatch, root, "--check") == 1
    err = capsys.readouterr().err
    assert "ghost" in err
    assert "top-level" in err


def test_ci_fixture_prefixed_extra_names_the_exemption(tmp_path, monkeypatch, capsys):
    root = make_tree(tmp_path, ["demo"], [entry("demo"), entry("ci-fixture-riscv")])
    assert run(monkeypatch, root, "--check") == 1
    err = capsys.readouterr().err
    assert "ci-fixture-riscv" in err
    assert "CI fixtures" in err


def test_duplicate_entry_is_rejected(tmp_path, monkeypatch, capsys):
    root = make_tree(tmp_path, ["demo"], [entry("demo"), entry("demo")])
    assert run(monkeypatch, root, "--check") == 1
    err = capsys.readouterr().err
    assert "more than one entry" in err
    assert "demo" in err


def test_entry_without_headline_is_rejected(tmp_path, monkeypatch, capsys):
    root = make_tree(tmp_path, ["demo"], [{"id": "demo"}])
    assert run(monkeypatch, root, "--check") == 1
    err = capsys.readouterr().err
    assert "no headline" in err
    assert "demo" in err


# ── Malformed fields surface as ValueError, not an attribute error ───────────
#
# `shim`/`functional` were read with `c.get(...) or []`, so a scalar rendered as
# a confusing AttributeError instead of the schema error the other list fields
# raise. Each of these must fail through list_field.


def test_functional_non_list_is_rejected(tmp_path, monkeypatch):
    root = make_tree(tmp_path, ["demo"], [{**entry("demo"), "functional": "oops"}])
    with pytest.raises(ValueError, match="functional.*must be a list"):
        run(monkeypatch, root, "--check")


def test_shim_non_list_is_rejected(tmp_path, monkeypatch):
    root = make_tree(tmp_path, ["demo"], [{**entry("demo"), "shim": "oops"}])
    with pytest.raises(ValueError, match="shim.*must be a list"):
        run(monkeypatch, root, "--check")


def test_dangling_dead_note_is_rejected(tmp_path, monkeypatch):
    root = make_tree(tmp_path, ["demo"], [{**entry("demo"), "dead_note": "explains nothing"}])
    with pytest.raises(ValueError, match="dead_note.*advanced_dead.*is empty"):
        run(monkeypatch, root, "--check")


def test_unknown_functional_gate_is_rejected(tmp_path, monkeypatch):
    bad = {**entry("demo"), "functional": [{"what": "x", "fw": "f", "ev": "e", "gate": "sometimes"}]}
    root = make_tree(tmp_path, ["demo"], [bad])
    with pytest.raises(ValueError, match="unknown gate"):
        run(monkeypatch, root, "--check")


# ── The non-errors ────────────────────────────────────────────────────────────


def test_ci_fixture_config_is_out_of_scope(tmp_path, monkeypatch, capsys):
    root = make_tree(tmp_path, ["demo", "ci-fixture-riscv"], [entry("demo")])
    assert run(monkeypatch, root) == 0
    assert "wrote" in capsys.readouterr().out


def test_complete_ledger_writes_the_doc_and_passes_check(tmp_path, monkeypatch, capsys):
    root = make_tree(tmp_path, ["demo"], [entry("demo")])
    out_doc = root / "docs" / "boards" / "FIRMWARE_EXERCISE_MATRIX.md"

    assert run(monkeypatch, root) == 0
    capsys.readouterr()
    assert out_doc.exists()
    assert "`demo`" in out_doc.read_text()

    assert run(monkeypatch, root, "--check") == 0, "the doc just written must not read as stale"
