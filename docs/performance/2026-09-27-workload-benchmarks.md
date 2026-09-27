# Representative workload performance

The all-chip `firmware-perf-spin` gate is the deterministic engine-cost and
absolute-RTx contract. It intentionally cannot describe peripheral-heavy user
workloads. `scripts/perf/workload_perf.py` complements it with three real
firmware cases:

| Case | Product path | Evidence |
|---|---|---|
| `esp32c3-oled` | ROM boot, FreeRTOS, UART, I²C and SSD1306 | setup and first-paint latency, RTx, batch width, idle fast-forward, serial/display liveness |
| `nrf54l15-embassy-events` | Embassy executor, GRTC, GPIO and WFE | accelerated/reference throughput, exact GPIO event cycles, complete CPU/RAM/peripheral identity |
| `esp32c3-oled-identity` | same OLED firmware at tick 1 and the shipped recommendation | byte-identical framebuffer plus the cost of each lane |

The harness builds each Rust test first and runs its executable directly. Its
per-case `/usr/bin/time` receipt therefore includes process cold start and lab
construction, but excludes Cargo and compiler work. The structured JSON also
separates simulator setup from the measured run where the test exposes it.

Run all cases:

```sh
python3 scripts/perf/workload_perf.py --status-json workload-perf-status.json
```

Run a subset:

```sh
python3 scripts/perf/workload_perf.py \
  --workloads esp32c3-oled,nrf54l15-embassy-events
```

Wall time, CPU time and peak RSS vary by host and are diagnostic measurements,
not merge-blocking thresholds. The deterministic contracts remain:

- `scripts/perf/baselines.json` for host instructions per simulated step;
- the 1024 minimum recommended tick interval in `board_perf.py`;
- the workload tests' framebuffer, event-cycle and complete-state assertions.

The GitHub `Representative Workload Performance` workflow supplies a pinned
Ubuntu 24.04 comparison host and uploads both the human report and JSON receipt.

## First local receipt

Measured on the shared validation VPS from the 1024-interval tree:

| Workload | Result |
|---|---:|
| C3 OLED | 0.218x RTx; 333.4 mean batch; first paint at 137.5 ms guest / 851 ms wall; 21.8 MiB peak RSS |
| C3 tick 1 → 1024 | framebuffer identical; 5.51 s → 1.37 s (4.0x) |
| nRF54 Embassy GRTC | 298.3x accelerated RTx; GPIO intervals 32,000,323 and 32,000,256 cycles; complete state identical |

The C3 result is the important new finding: synthetic execution has ample
headroom, but peripheral-heavy boot-to-display is still below real time. The
receipt isolates that as follow-up optimization work rather than weakening the
fleet contract. GitHub receipts should be used for host-to-host comparison.
