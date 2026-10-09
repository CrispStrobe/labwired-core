# Two-chip Tier1 blocked-cell diagnostic — pending

The full gate in [run37913923779](https://github.com/CrispStrobe/labwired-core/actions/runs/37913923779)
reported20 committed pass cells becoming blocked across nRF52832 and STM32F103.
Other core/manual checks passed. PR190 remains unmerged; no acknowledgement,
snapshot, threshold or engine correction is made here.

This separate hosted diagnostic builds baseline
`97a8cd99b27539d2ea28e3746998e4d993b18bba` and candidate
`9b819fe1cf9c616936cb06515839f8d831c05527`. Each runs the same two public
committed Tier1 fixtures under the existing stepped CLI invocation with8M-step
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
