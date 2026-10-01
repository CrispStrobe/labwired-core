# LabWired - Firmware Simulation Platform
# Copyright (C) 2026 Andrii Shylenko
#
# This software is released under the MIT License.
# See the LICENSE file in the project root for full license information.
#
# Unit tests for the pr-workspace-tests shard driver and aggregator. These
# cover the PARSING and the CLASSIFICATION only — no cargo invocation — because
# a gate that mis-reads libtest output is the same false-green class the gate
# itself exists to close. Wired into pr-gate's pytest line in core-ci.yml.

import json
import os
import subprocess
import sys
from pathlib import Path

import pytest
import yaml

sys.path.insert(0, str(Path(__file__).resolve().parent))

import workspace_test_aggregate as agg
import workspace_test_shard as shard


def core_workflow_jobs():
    root = Path(__file__).resolve().parents[2]
    return yaml.safe_load((root / ".github/workflows/core-ci.yml").read_text())["jobs"]


def test_core_pr_lanes_pin_the_existing_ubuntu_24_image_without_changing_nightly_lanes():
    jobs = core_workflow_jobs()
    for name in (
        "release-runner-contract", "fmt-autofix", "pr-default-members",
        "pr-feature-off", "pr-gate", "pr-gate-warm", "browser-layer",
        "pr-scheduler-observable", "xtensa-flash-boot", "pr-workspace-tests",
        "pr-workspace-tests-aggregate",
    ):
        assert jobs[name]["runs-on"] == "ubuntu-24.04"
    for name in ("integrity", "ci-runner-image", "full"):
        assert jobs[name]["runs-on"] == "ubuntu-latest"


def test_core_split_preserves_configuration_commands_and_read_only_cache():
    jobs = core_workflow_jobs()
    default = jobs["pr-default-members"]
    feature_off = jobs["pr-feature-off"]
    trigger = "github.event_name == 'pull_request' || github.event_name == 'workflow_dispatch'"
    for child in (default, feature_off):
        assert child["if"] == trigger
        assert child["runs-on"] == "ubuntu-24.04"
        assert child["timeout-minutes"] == 20
        checkout = next(step for step in child["steps"] if step.get("uses") == "actions/checkout@v4")
        assert checkout["with"]["fetch-depth"] == 0
        rust = next(step for step in child["steps"] if step.get("name") == "Install Rust")
        assert rust["uses"] == "dtolnay/rust-toolchain@1.95.0"
        assert rust["with"]["components"] == "rustfmt, clippy"
        cache = next(step for step in child["steps"] if step.get("name") == "Cache dependencies")
        assert cache["uses"] == "Swatinem/rust-cache@v2"
        assert cache["with"] == {"shared-key": "core-pr-gate", "workspaces": ". -> target\n", "save-if": False}
        python = next(step for step in child["steps"] if step.get("name") == "Set up Python")
        assert python["uses"] == "actions/setup-python@v5"
        assert python["with"]["python-version"] == "3.12"
    commands = {step.get("name"): step.get("run") for step in default["steps"]}
    assert commands["Clippy (default-members)"] == "cargo clippy --all-targets -- -D warnings"
    assert commands["Unit tests (default-members lib)"] == "cargo test --lib"
    commands = {step.get("name"): step.get("run") for step in feature_off["steps"]}
    assert commands["Unit tests (labwired-core, FEATURE-OFF)"] == "cargo test -p labwired-core --lib"
    assert commands["AVR Nano golden survival"] == "cargo test -p labwired-core --test avr_nano_golden_survival -- --nocapture"
    assert commands["Walk-deletion derivation (PR-fast structural arm)"].split() == [
        "scripts/ci/cargo-test-nonvacuous.sh", "-p", "labwired-core", "--test", "esp32_classic_walk_differential",
    ]
    assert commands["Firmware survival (committed ELF fixtures, ~50 chips)"] == "cargo test -p labwired-core --test firmware_survival"
    assert commands["BLE Pong fixture provenance (committed .ino ↔ .bin)"] == "cargo test -p labwired-core --test fixture_ble_pong_provenance"
    assert commands["SoftDevice HLE (unit + conformance vectors, C ABI)"] == (
        "cargo test -p nrf-softdevice-hle --features capi\n"
        "cargo rustc -p nrf-softdevice-hle --features capi --crate-type cdylib\n"
    )


@pytest.mark.parametrize("default_result", ["success", "failure", "cancelled", "skipped", ""])
@pytest.mark.parametrize("feature_off_result", ["success", "failure", "cancelled", "skipped", ""])
def test_core_split_aggregate_fails_closed(default_result, feature_off_result):
    gate = core_workflow_jobs()["pr-gate"]
    assert gate["needs"] == ["pr-default-members", "pr-feature-off"]
    assert gate["if"] == (
        "${{ always() && (github.event_name == 'pull_request' || github.event_name == 'workflow_dispatch') }}"
    )
    step = gate["steps"][0]
    assert step["env"] == {
        "DEFAULT_MEMBERS_RESULT": "${{ needs.pr-default-members.result }}",
        "FEATURE_OFF_RESULT": "${{ needs.pr-feature-off.result }}",
    }
    completed = subprocess.run(
        ["bash", "-e", "-o", "pipefail", "-c", step["run"]],
        env={**os.environ, "DEFAULT_MEMBERS_RESULT": default_result, "FEATURE_OFF_RESULT": feature_off_result},
        capture_output=True, text=True, check=False,
    )
    assert (completed.returncode == 0) == (default_result == feature_off_result == "success")


def test_workspace_cross_compiler_does_not_pull_unused_cxx_archives():
    root = Path(__file__).resolve().parents[2]
    workflow = yaml.safe_load((root / ".github/workflows/core-ci.yml").read_text())
    steps = workflow["jobs"]["pr-workspace-tests"]["steps"]
    install = next(
        step for step in steps
        if step.get("name") == "Add the C cross-compiler (OBD2 ECU firmware)"
    )
    assert "sudo apt-get install -y --no-install-recommends gcc-arm-none-eabi" in install["run"]


# ── libtest output parsing ───────────────────────────────────────────────────

PASSING = """\
   Compiling labwired-core v0.21.0 (/w/crates/core)
    Finished `test` profile [optimized + debuginfo] target(s) in 0.12s
     Running tests/foo.rs (/w/target/debug/deps/foo-1234)

running 3 tests
test alpha ... ok
test beta ... ok
test gamma ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.25s
"""

FAILING = """\
     Running tests/env.rs (/w/target/debug/deps/env-5678)

running 2 tests
test keeps_working ... ok
test prioritizes_failed_assertions ... FAILED

failures:

---- prioritizes_failed_assertions stdout ----
thread 'prioritizes_failed_assertions' (12296) panicked at crates/cli/tests/env.rs:1394:5:
assertion `left == right` failed

failures:
    prioritizes_failed_assertions

test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.56s

error: test failed, to rerun pass `-p labwired-cli --test env`
"""

VACUOUS = """\
     Running tests/empty.rs (/w/target/debug/deps/empty-0000)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
"""

ALL_IGNORED = """\
     Running tests/bench.rs (/w/target/debug/deps/bench-0000)

running 2 tests
test bench_a ... ignored
test bench_b ... ignored

test result: ok. 0 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 0.00s
"""

WITH_SKIP = """\
     Running tests/world.rs (/w/target/debug/deps/world-0000)

running 1 tests
SKIP: iolink ELFs not built; build it: make -C examples/iolink-dido/firmware
test station_boots ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.40s
"""


def test_parse_counts_and_duration():
    p = shard.parse_run(PASSING)
    assert (p["passed"], p["failed"], p["ignored"]) == (3, 0, 0)
    assert p["result_lines"] == 1
    assert p["duration_s"] == 1.25


def test_parse_failed_test_names_from_failed_lines():
    p = shard.parse_run(FAILING)
    assert p["failed"] == 1
    assert p["failed_tests"] == ["prioritizes_failed_assertions"]


def test_parse_skip_channel_is_visible_not_silent():
    p = shard.parse_run(WITH_SKIP)
    assert p["passed"] == 1
    assert any("SKIP:" in s for s in p["skips"])


def test_vacuous_and_all_ignored_are_distinguishable():
    # The vacuity contract: 0/0/0/0 is the empty-binary shape; ignored > 0 is a
    # suite that legitimately ran nothing.
    v = shard.parse_run(VACUOUS)
    assert (v["passed"], v["failed"], v["ignored"], v["filtered"]) == (0, 0, 0, 0)
    i = shard.parse_run(ALL_IGNORED)
    assert i["ignored"] == 2 and i["passed"] == 0


# ── shard assignment ─────────────────────────────────────────────────────────

def test_round_robin_covers_everything_exactly_once():
    runnable = [(f"pkg{c}", f"t{i}", "test") for i in range(10) for c in "ab"]
    runnable.sort()
    slices = [shard.shard_slice(runnable, k, 3) for k in (1, 2, 3)]
    flat = [e for s in slices for e in s]
    assert sorted(flat) == runnable
    # deterministic: same input, same split
    assert slices == [shard.shard_slice(runnable, k, 3) for k in (1, 2, 3)]


def _entry(pkg, tgt, kind="test"):
    return {"package": pkg, "target": tgt, "kind": kind, "exe": None, "cwd": "."}


def test_classification_rejects_stale_exclusion():
    cfg = {"shard_count": 3, "cross_build_excluded": [
        {"package": "p", "target": "ghost", "needs": "x", "reason": "y"}],
        "known_red": []}
    _, problems = shard.classify(cfg, [_entry("p", "real")])
    assert any("ghost" in p for p in problems)


def test_classification_rejects_known_red_without_a_target():
    cfg = {"shard_count": 3, "cross_build_excluded": [], "known_red": [
        {"package": "p", "target": "ghost", "test": "t", "issue": 1, "reason": "r"}]}
    _, problems = shard.classify(cfg, [_entry("p", "real")])
    assert any("ghost" in p for p in problems)


# ── cargo test's runtime environment, reproduced ─────────────────────────────

def test_test_env_sets_manifest_dir_and_bin_exes(tmp_path):
    exe = tmp_path / "debug" / "deps" / "cli_integration-abc"
    exe.parent.mkdir(parents=True)
    exe.touch()
    entry = {
        "package": "labwired-cli", "package_id": "pkg-cli",
        "target": "cli_integration", "kind": "test",
        "exe": str(exe), "cwd": "/w/crates/cli",
    }
    env = shard.test_env(entry, {"pkg-cli": [("labwired", "/w/target/debug/labwired")]})
    assert env["CARGO_MANIFEST_DIR"] == "/w/crates/cli"
    assert env["CARGO_BIN_EXE_labwired"] == "/w/target/debug/labwired"
    # tmpdir's parent resolves to the target dir — test_support's contract.
    assert Path(env["CARGO_TARGET_TMPDIR"]).parent == tmp_path
    assert Path(env["CARGO_TARGET_TMPDIR"]).is_dir()


def test_test_env_mangles_dashed_bin_names(tmp_path):
    exe = tmp_path / "debug" / "deps" / "t-1"
    exe.parent.mkdir(parents=True)
    exe.touch()
    entry = {"package": "p", "package_id": "p1", "target": "t", "kind": "test",
             "exe": str(exe), "cwd": "/w/p"}
    env = shard.test_env(entry, {"p1": [("my-tool", "/b/my-tool")],
                                 "other": [("nope", "/b/nope")]})
    assert env["CARGO_BIN_EXE_my_tool"] == "/b/my-tool"
    assert "CARGO_BIN_EXE_nope" not in env  # only same-package bins


# ── aggregation verdict ──────────────────────────────────────────────────────

def _write_report(root, k, targets, new_red=None, known_red_seen=None, hard=None):
    d = Path(root) / f"workspace-test-shard-{k}"
    d.mkdir(parents=True, exist_ok=True)
    (d / f"shard-{k}.json").write_text(json.dumps({
        "shard": k,
        "shard_count": 3,
        "targets": targets,
        "hard_errors": hard or [],
        "new_red": new_red or [],
        "known_red_seen": known_red_seen or [],
    }))


def _target(pkg, tgt, status="pass", failed_tests=None, dur=1.0):
    return {
        "package": pkg, "target": tgt, "kind": "test", "status": status,
        "passed": 5 if status == "pass" else 4,
        "failed": 0 if status == "pass" else 1,
        "ignored": 0, "filtered": 0, "duration_s": dur,
        "failed_tests": failed_tests or [], "skips": [],
    }


def _cfg(known_red=None):
    return {"shard_count": 3, "cross_build_excluded": [], "known_red": known_red or []}


def test_missing_shard_report_is_a_failure_not_a_pass(tmp_path):
    _write_report(tmp_path, 1, [_target("p", "a")])
    _write_report(tmp_path, 2, [_target("p", "b")])
    failures, _, _, _ = agg.aggregate(tmp_path, _cfg())
    assert any("shard 3" in f and "NO REPORT" in f for f in failures)


def test_new_red_fails_and_names_the_test(tmp_path):
    for k in (1, 2, 3):
        _write_report(tmp_path, k, [_target("p", f"t{k}")])
    _write_report(tmp_path, 4, [])  # ignored: only shards 1..3 are expected
    failures, _, _, _ = agg.aggregate(
        tmp_path,
        _cfg(),
    )
    assert failures == []

    _write_report(tmp_path, 2, [_target("p", "t2")], new_red=[
        {"package": "p", "target": "t2", "test": "broke"}])
    failures, _, _, _ = agg.aggregate(tmp_path, _cfg())
    assert any("NEW RED" in f and "broke" in f for f in failures)


def test_known_red_still_failing_is_a_notice_not_a_failure(tmp_path):
    entry = {"package": "p", "target": "t1", "test": "flaky", "issue": 42, "reason": "r"}
    for k in (1, 2, 3):
        _write_report(tmp_path, k, [_target("p", f"t{k}")])
    _write_report(tmp_path, 1, [
        _target("p", "t1", status="fail", failed_tests=["flaky"])],
        known_red_seen=[{**entry}])
    failures, notices, _, _ = agg.aggregate(tmp_path, _cfg([entry]))
    assert failures == []
    assert any("#42" in n for n in notices)


def test_known_red_now_passing_is_stale_and_fails(tmp_path):
    # The ratchet that shrinks the allow-list: a fixed test must not keep its
    # entry, or the entry silently re-arms and swallows the next regression.
    entry = {"package": "p", "target": "t1", "test": "fixed", "issue": 42, "reason": "r"}
    for k in (1, 2, 3):
        _write_report(tmp_path, k, [_target("p", f"t{k}")])
    failures, _, _, _ = agg.aggregate(tmp_path, _cfg([entry]))
    assert any("STALE" in f and "#42" in f for f in failures)


def test_known_red_whose_target_never_ran_fails(tmp_path):
    entry = {"package": "p", "target": "ghost", "test": "t", "issue": 7, "reason": "r"}
    for k in (1, 2, 3):
        _write_report(tmp_path, k, [_target("p", f"t{k}")])
    failures, _, _, _ = agg.aggregate(tmp_path, _cfg([entry]))
    assert any("ghost" in f and "did not run" in f for f in failures)
