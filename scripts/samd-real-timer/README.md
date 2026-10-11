# Prospective actual Timer guest harness

This is a separate **unexecuted** harness for [CODAL PR13](https://github.com/CrispStrobe/codal-samd/pull/13),
not another run of the already qualified five authored paired-policy cases.
It compiles against current LabWired APIs only after its hosted control job runs.
No automatic actual guest execution is enabled by this preparation.

The ignored test requires `LABWIRED_TIMER_FIXTURE_DIR` and an explicitly reviewed
`LABWIRED_TIMER_QUALIFIED_SOURCE`. That source parameter verifies identity, not
GitHub qualification: require every enabled source build/control before selecting
it. The builder's source/core/ELF provenance must match the consumed image.

The harness poisons full diagnostic BSS and data RAM, then checks their reset
initialization at the actual caller entry, including copied executable data.
It validates all vector words, nine distinct BSS witnesses, each of six ordered
caller milestones, exactly two actual IRQ0 entries, closed paired state at IRQ
delivery, exception16/mask0 in the handler, thread return, fault0 and masked
terminal `0x600d`. The bound is 250,000 instructions; timeout is never success.
Three manufactured witness controls are not actual guest qualification.

Still required: hosted compilation of this harness, the source's ARM build,
both scheduler-profile actual runs, guest adversaries, a source/log/run-bound
original receipt with manufactured reader controls, and separate packet audit.
Keep the PR draft until every affected final-head check and actual guest passes.
Do not move existing consumer pins or overwrite earlier original packets.

Clock, allocation, component registration and event dispatch remain explicitly
limited by the [runtime fixture contract](https://github.com/CrispStrobe/codal-samd/blob/5d35c01e331acd83d1adb4854a18ecb027b2557e/timer-caller/README.md).
This does not qualify peripheral timers, real fibers, event bus, full runtime,
native Arcade boot, physical calibration, licensing or active RTx.
