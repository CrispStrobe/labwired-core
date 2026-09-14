# Python Sim SDK Implementation Plan

> For agentic workers: use superpowers:subagent-driven-development. User explicitly requested subagents.

**Goal:** Installable, offline Python Session interface and pytest factory over the existing Session implementation.

**Architecture:** Preserve legacy Machine, expose native Session through labwired._native, wrap it with a small Python Sim class. Reuse existing Rust builder and Session, with packaged catalog files resolved explicitly. Base is feat/core-session 922c0208, a dependency not yet merged to main.

**Tech Stack:** Existing pyo3, maturin, Python, pytest, Rust Session. Existing merged design: parent docs/superpowers/specs/2026-09-14-core-session-and-python-sdk-design.md, Step3. This delivery is the local installable SDK slice; global CLI/WASM rebasing and public release are separate.

## Task 1: Native bindings and package

Files: crates/python/src/{lib,session}.rs, crates/python/Cargo.toml, crates/python/pyproject.toml, crates/python/python/labwired/{__init__,sim}.py; catalog additions only if explicit root resolution requires them.

- [ ] Add real-engine tests for context lifetime, UART expect overshoot, exact virtual-time advancement, memory reads/writes and snapshots; demonstrate absence of Sim fails first.
- [ ] Preserve Machine and StopReason exports, export native Session wrapper with clear errors and closed-state checks. Use Session::open and existing builder; never reimplement emulation.
- [ ] Add Sim(elf, chip or system, uart), run_for, expect, send/send_bytes, read_uart, uart_transcript, set_pin by binding ID, set_input(s), read_memory/read_u32/write_u32/symbol, frames, snapshot/restore, close/context manager. Numeric durations are seconds; support s/ms/us/ns strings. Unsupported operations raise explicit NotSupported. Expose actual stop reason, do not claim run reached target when firmware halted.
- [ ] Package configs without global env mutation or dependence on checkout path. Validate requested UART against actual console support; reject unsupported names rather than silently ignore.
- [ ] Build/install wheel in a venv and exercise from /tmp with real committed firmware, explicit system and packaged named chip. No test engine mock counts as simulator proof.

## Task 2: pytest and docs

Files: crates/python/python/labwired/pytest_plugin.py, crates/python/tests/test_sim.py and test_pytest_plugin.py, docs/python-sdk.md, crates/python/pyproject.toml.

- [ ] Implement managed sim factory and --labwired-chip, close all instances, attach UART on failure including after context close; explicit NotSupported becomes skip. Plain pytest discovery must not need firmware or network.
- [ ] Add run_firmware convenience helper. Do not introduce YAML collection until existing scripted fixture runner supports it; document that limit.
- [ ] Test plugin through subprocess pytest including failed assertions, unsupported skip, teardown and JUnit transcript; test installed wheel outside repo.
- [ ] Document runnable quick start, virtual timeout semantics, pin binding IDs, one selected UART, supported and unavailable APIs, installation, dependency branch, and no claim of device parity.

## Task 3: Review and delivery

- [ ] Independent spec review then code-quality review, fix findings.
- [ ] Run Python tests on installed wheel, cargo check for Python crate, scoped formatting and package-content checks. Report exact outcomes and limitations.
- [ ] Commit scoped changes and create reviewable draft PR stacked on feat/core-session when repository access allows. Do not publish to PyPI or merge unfinished dependency.
