# Owned ARM state-control qualification — pending

Source fixture: [CODAL-SAMD PR3](https://github.com/CrispStrobe/codal-samd/pull/3),
exact commit `592748e51e6b82238d9c1d6a69ffa4e74ccc4ce1`.
This is an authored abstract transfer-state test, not a CODAL runtime image,
real SPI/DMA transfer, callback/fiber, IRQ or native PyBadge qualification.
The [replacement plan](../../docs/engineering/samd-runtime-replacement-plan.md)
still requires those separate lanes and complete header/build provenance.

The dedicated hosted workflow builds both exact source and a temporary
premature-success header mutation without standard libraries. The normal
guest must reach `0x600d`; the mutant must reach precisely assertion marker 4,
not an arbitrary fault, timeout or host assertion. Resolve `control_status`
from each ELF, poison its BSS witness, and observe guest startup clearing it
and entering the probe before accepting either result. Both engine feature
profiles must run the ignored test explicitly. A normal workspace test skip
is not qualification. Source/image/map hashes and result markers are retained;
ELFs and objects are not uploaded by this workflow. No performance is measured.

The fixture disables interrupts and uses diagnostic startup/memory layout.
Its DMA/TXC calls are authored abstract events, not real peripheral observations.
The first actual hosted guest result and every enabled exact-head PR check
are required before landing this test harness. Preserve any failed result;
never change the expected marker or invent completion to make it pass.
