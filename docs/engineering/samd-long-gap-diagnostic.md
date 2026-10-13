# Actual Timer long-gap diagnostic — prospective

The real service/clock slice landed through
[PR222](https://github.com/CrispStrobe/labwired-core/pull/222), merge
`708da30a2492f2d0c0503de5e7ab4b6a208def2a`, after22 enabled checks passed.
This next diagnostic is not whole native Arcade or a repaired time contract.

Pin own runtime source
[`07de1e4bbc0910a5f6b192df13c5df3a6fe467af`](https://github.com/CrispStrobe/codal-samd/commit/07de1e4bbc0910a5f6b192df13c5df3a6fe467af),
whose [five-image build38465804291](https://github.com/CrispStrobe/codal-samd/actions/runs/38465804291)
passed. Actual core remains `2ed19d64ec3359143409a4b0a9758b4aeef3ee7d`;
Timer and clock implementations are unchanged for this first diagnostic.

The guest arms a real20 ms Event, masks interrupts, waits80 ms through actual
DWT-based low-level captures, and reads Timer only before/after the gap.
The pinned Timer narrows elapsed to16 bits. Its `sigma` and millisecond remainder
are also16-bit; changing just the elapsed cast is not a sufficient repair.
Source review predicts lost time but does not establish an observed failure.

The existing eight cases retain their four-million-step bound. Only the new
long case receives a separately named twelve-million-step bound. Both profiles
must bind exact source/image/map/startup and actual architectural pending
exception15 under PRIMASK, with no IRQ entry, Event or context switch. Guest
COUNTFLAG is recorded separately from the architectural pending observation.

The first diagnostic accepts only guest assertion `0xe0000c35`, with actual
low-level elapsed80–81 ms and Timer elapsed lower by approximately65536 us
within the source-defined overhead bound. Raw ordered stage/pending/time
witnesses must support that exact outcome. Timeout, a different panic,
architectural fault or arbitrary manufactured marker cannot qualify it.
A passing diagnostic means **expected bug reproduced**, not correct time.

Preserve the actual first original before any Timer repair. Then review counter
width/modular arithmetic, `sigma` and remainder together; qualify microseconds
and milliseconds, overdue Event delivery and all original cases against the
fixed source. No host clock advances, pending-bit mutation, injected Event or
success stub. Default physical builds, historical image rejection, app pins,
captures, acknowledgements and performance thresholds remain unchanged.
