# Two-chip Tier1 blocked-cell diagnostic

The full gate in [run37913923779](https://github.com/CrispStrobe/labwired-core/actions/runs/37913923779)
reported20 committed pass cells becoming blocked across nRF52832 and STM32F103.
Other core/manual checks passed. PR190 remains unmerged; no acknowledgement,
snapshot, threshold or engine correction is made here.

This separate hosted diagnostic builds baseline
`97a8cd99b27539d2ea28e3746998e4d993b18bba` and candidate
`9b819fe1cf9c616936cb06515839f8d831c05527`. Each runs the same two public
committed Tier1 fixtures under the existing CLI invocation with8M-step
and120-second bounds. It captures exact stdout/stderr, exit status and config,
fixture and CLI hashes. Ambient LABWIRED variables are removed, like the
production harness. No guest bytes are patched, fetched or uploaded.

Successful capture is not a passing Tier1 matrix. Inspect original streams,
compare pins and input hashes, and determine whether baseline also blocks
before naming a regression or proposing a correction. Preserve the original
full-gate failure. A reproduction or correction must retain the ratchet and
its immutable baseline; do not regenerate the snapshot merely to clear red.
This narrower CLI build does not reproduce the full workspace's feature
unification; if results differ, investigate that build/environment boundary.
No hardware, app-adoption, runtime-admission or performance claim follows.

## First observation and next comparison

[Original run37939497738](https://github.com/CrispStrobe/labwired-core/actions/runs/37939497738)
completed both captures. Both pins produced CLI SHA256
`38d0fad5b4a403a04ffb04d2c6ad44189c88b4215e5557594be1088dca93fbdb`;
both fixtures exited zero with empty stdout on both pins. The original stream
digests and input hashes were audited without importing production helpers.
This does not clear the ratchet or establish a PR190 regression.

Ordinary ARM `run` currently selects the fast loop automatically unless
single-step/trace is requested; the absence of `--batched` does not force
stepped execution. The next capture compares default-with-stats, forced
single-step, JIT off, idle fast-forward off, and both optimizations off.
Every case records its explicit environment and a120-second timeout, retaining
partial streams on timeout. Sources, fixture bytes and8M fuel bound remain
unchanged. These are diagnostic modes, not a new production default or waiver.
The original run remains the evidence for the original invocation; later
mode comparisons do not replace its artifacts.
