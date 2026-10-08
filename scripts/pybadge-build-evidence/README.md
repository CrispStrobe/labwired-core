# Original PyBadge build evidence — prepared, not yet qualified

This hosted-only S1 follow-up uses the original build/request functions in
[Lite source 6a7027e](https://github.com/CrispStrobe/brickwright-lite/tree/6a7027e117a6698866516f52feca259127c80c7d),
the source of [historical run36567239929](https://github.com/CrispStrobe/brickwright-lite/actions/runs/36567239929).
No consumer implementation or engine pin changes. The focused job builds only
`samd51adafruit`, rather than repeating all six historical variants.

The generated original worker request must reproduce SHA256
`19efcdc73769fdfdeb51aa215c528bebad59782cbc538f72f4a194326f1f42b1`.
Serialization is accepted only if it reproduces that hash exactly; a difference
fails instead of silently normalizing/re-pinning. The clean ARM build must
reproduce the historical 376,911-byte HEX with SHA256
`9c2310bd5a65f0543c69a076e51a4067c803202a39228f3a7de41451be0da9ca`.

The separately pinned CDN base has SHA256
`842c30c5fc1db2346a949837c2e21acdb57937aa0a82f4d505725e995ff02b97`
and 361,828 bytes. The same request key does not mean these are identical builds.
Do not confuse the cache's CDN bytes with the historical from-source artifact.

The artifact retains the request, original builder manifest, generated CMake
flags/cache/configuration headers, per-file hashes and notices. Raw generated
files remain unchanged; the firmware binary itself is not uploaded. Admission rejects wrong checkout/builder/request/HEX,
selected symlinks, oversized capture and accidental overwrite. Failures stay
failures; the artifact upload preserves available staged evidence.

This does not execute a guest, load CF2, observe constructor/renderer selection,
qualify a module/LUT or measure RTx. Header/flag retention is not independently
verified effective preprocessing. Review the actual artifact before extending
those claims. Do not close P3/CP14 or move app pins/ACKs/baselines on a build.

Run the tiny admission controls with `node --test scripts/pybadge-build-evidence/test.mjs`.
All worker execution, downloads and ARM builds belong to the focused hosted job,
not a resource-constrained development host. Require that job and every enabled
exact-head check before merge; no passing actual result is asserted yet.
