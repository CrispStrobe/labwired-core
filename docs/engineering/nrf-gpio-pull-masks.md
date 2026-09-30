# Nordic GPIO pull-mask candidate

The Nordic GPIO model previously scanned every physical `PIN_CNF.PULL` field
on each IN read, including the bus's GPIO edge polling. The candidate derives
two private 32-bit resistor masks at valid PIN_CNF writes instead. IN then
combines those masks with the current DIR, OUT, latched input and external-drive
state in constant time. It does not skip GPIO edge detection or change pulls,
missing-pin latch behavior, reserved PULL encodings, or any other GPIO family.

All byte, halfword and word writes still reach the existing register-write
path, including the nRF54 compact-layout translation. Constructors start with
zero masks. PIN_CNF and pin-count fields are private; the model has no
Deserialize or implemented peripheral restore path to bypass mask maintenance.
Both derived masks are serde-skipped, preserving the existing six-register-field
JSON snapshot schema. This does not add previously missing snapshot restoration.

Seven full-crate regressions compare against the original loop: all pins and
PULL encodings, direction, external drive/release, latched levels and outputs;
randomized mixed writes; 0/13/16/32-pin banks; missing pins and reserved encodings;
bulk direction and peripheral pad-latch writes; constructors; translated
subword writes; and snapshot schema. Five pure-model tests passed locally using
the actual source extracted into a standalone rustc test (only serde/census
infrastructure stubbed). That limited proof is not a full-crate test pass; the
two real GpioPort integration tests await hosted execution.

Source was formatted and committed before renewing existing content-bound
validation acknowledgements. No silicon capture is claimed. Full CorePerf
instruction-baseline and unchanged native-motion gates remain required; no
performance improvement, browser qualification or app package update is claimed
before those receipts are available. This branch changes GPIO only, not the
separate generation-scoped CPU discovery candidate.
