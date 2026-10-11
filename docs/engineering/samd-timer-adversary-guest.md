# Timer adapter negative guests — prospective harness

The separate `samd_real_timer_adversary_guest` integration test consumes the
three derivative images from [PR201](https://github.com/CrispStrobe/labwired-core/pull/201).
Neither compilation nor actual negative execution has passed yet. Its ordinary
workflow compiles the harness and runs manufactured controls only; the actual
guest test remains ignored until dedicated qualification is enabled.

The actual test requires `LABWIRED_TIMER_MUTANT_DIR`, matching immutable adapter
source, each declared case and the consumed ELF byte identity. It validates all
vectors, nine distinct aligned BSS witness locations, complete poisoned BSS/data
reset initialization and a bounded actual rejection function extent. It checks
every terminal observation against these fixed expectations, not against a
producer's freely selectable expected status:

| Case | Status | Fault | Mask | Exception | Actual IRQ entries | Caller milestones |
| --- | --- | --- | --- | --- | --- | --- |
| Unconditional restore | `0xb02` | 0 | 1 | 0 | 0 | 0 |
| Early restore | `0xa14` | 0 | 1 | 16 | 1 | 1 |
| Unsupported event | `0xa07` | 0 | 1 | 0 | 0 | 0 |

All nine guest witnesses must equal the exact case-specific tuple, including
component/compare counts and recorded handler mask/context. Wrong failure,
success status, engine fault or timeout refuses. The CPU must execute 64
consecutive instructions within the actual compiled `reject` function, preserve
all terminal witnesses and show changing PC, within 250,000 total instructions.
This is bounded terminal behavior, not indefinite liveness or hardware safety.

Still required: hosted mutant builds and harness controls, fuller source/header/
compiler/object/reader adversaries, dedicated actual execution in both scheduler
profiles, a bound original receipt and separate audit, and every enabled final
check before the Timer source lane merges. Existing positive Timer and authored
paired-policy receipts remain unchanged. This lane makes no runtime, licensing,
physical qualification, native-boot or RTx claim.
