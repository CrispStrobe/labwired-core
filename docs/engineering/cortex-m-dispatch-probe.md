# Bounded Cortex-M dispatch diagnostic candidate

This opt-in probe measures why existing cached T16 executors are attempted,
rejected or stopped. It changes no admission predicate or execution decision.
It is a diagnostic candidate awaiting independent source review, integrated
feature tests and affected guest qualification. No speedup is claimed.

Compile Core with `cortex-m-dispatch-probe`, then call
`CortexM::enable_dispatch_probe(event_limit)` explicitly on the selected own
CPU. Accepted limits are 1 through 1,000,000 events. Invalid limits return false
and preserve an existing collection. Re-enabling starts a fresh collection;
`take_dispatch_probe()` stops collection and transfers its fixed-size result.
There is no environment switch, automatic output, new dependency or default
feature. The ordinary build has no probe field, module, call or condition.

The result contains per-stage and per-reason event totals, returned retirement
and zero-return totals, partial-stop totals and their retired prefixes. Stages
nest: dispatch and child executor retirement must never be summed together.
Early zero returns have explicit terminal events. `Returned` records normal
budget or executor-limit completion as well as early stops; preceding rejection
or branch-exit events identify early stops. A terminal event after a partial
stop reports the same retirement as that prefix, not additional retirement.

Discovery records block-cache hit/miss, generation-memo rejection, initial
admission, the duplicate admission at search offset zero, other search
admissions, shape discovery and rejection. Shape rejection includes missing
cache entries, tag mismatch, non-T16 width, unsupported instruction, branch
shape and the fixed discovery-window limit. Admission failures preserve their
own counters and also report their structural cause. Multiple candidate
rejections can belong to one attempted block discovery. Cached runs record
cold/tag/width/unsupported and live-RAM refusal boundaries, with retirement
before that boundary. Live-RAM refusal includes unmapped/MMIO addresses and
blocked writes; the probe does not infer a peripheral reason or inspect it.

Rejection rows retain the first 64 distinct `(stage, reason, PC, opcode)` keys.
These are exact retained keys, **not a top-N histogram**. Additional unseen
keys increment `untracked_rejections`; later hits to retained keys still count.
PC identifies the failing instruction, or the candidate start for window-limit
and aggregate shape failures. Opcode is taken only from an existing matching
cache entry and may be wide; cold or collided entries have no opcode. There is
no extra instruction fetch, bus access or peripheral inspection.

The counter object is bounded below 16 KiB by a storage control. Recording does
not allocate, log, read a clock or use atomics; each event scans at most 64 rows.
At the finite prefix cap no counters or histograms change. The diagnostic build
continues paying function/cell/option/cap checks after exhaustion. This is a
bounded amount of collection work, **not a wall-time overhead guarantee**.
A collection may end between entry, stop and terminal events; `exhausted()`
marks that truncation. Missing terminals at the cap are not zero returns.
Counters saturate; collecting up to the cap cannot wrap the event count.

A private per-CPU cell permits read-only discovery methods to record without
changing their signatures. Its borrow ends before any guest execution or bus
operation; no callbacks occur while it is borrowed. This diagnostic state is
not architectural snapshot state. It does not cache peripheral capabilities or
skip any IRQ, MMIO, fault, debug, observer, deadline or invalidation check.

Before any hosted use, run the feature's integrated tests together with the
existing discovery, scalar differential, fault/MPU, MMIO/IRQ, debug/observer,
deadline and self-modifying-code suites. Keep the actual WASM cached-run
selection distinct from native selection; the feature never enables WASM-only
execution on native targets. Bind an admitted diagnostic build to the exact
source and own guest, collect one finite prefix, and retain source/toolchain/
input identities with the output. Instrumented timing cannot establish a
speedup. Any later optimization needs independent review, uninstrumented
same-runner A/B qualification and unchanged qualification thresholds.

The prepared integrated controls include the complete cached dispatcher across
rotated entries, executor shapes, budgets, RAM/MMIO addresses and cap cutoffs.
A separate authored read-clear peripheral checks that a declined dispatcher
performs no read, capability or IRQ-level hook, then exercises the actual
interpreter fallback. Its live read-deassert capability changes true/false/true;
word-read counts, MMIO accounting and level-marked NVIC pends must follow the
current capability with the probe both enabled and disabled and with optimized
bus access on and off. These controls are prepared, not yet compiled or run.
