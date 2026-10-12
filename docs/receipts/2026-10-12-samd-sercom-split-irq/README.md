# SERCOM4 split-IRQ original hosted qualification

Status: the original candidate passed on its recorded base. Latest main was
then integrated normally; combined-tree qualification and merge remain pending.
Do not substitute this receipt for the refreshed candidate's checks.

## Immutable source and hosted input

- Candidate: `6d86aaafaaaf94acc7df58b051fbc10c189a32d7`.
- Base: `296c43d8bc462e188e6e71cf0b35a3a258b38de9`.
- Original [Rust run38252022767](https://github.com/CrispStrobe/labwired-core/actions/runs/38252022767),
  attempt1, completed SUCCESS. All26 enabled PR checks passed; the four declared
  full/warm/image checks skipped. No thresholds, pins or capture ACKs changed.
- Both actual job logs identify checkout merge
  `56613de1483be51efce032fd256ee93909b75c81`; its official Git commit has the
  exact base/candidate parents above. Its tree
  `5604eb8af59b91f9af4fa5375b0f12c6f3c81c53` equals the reviewed candidate tree.
- Original logs were acquired once from official job-log API endpoints after
  run/job/source/attempt/step admission. A separate standard-library reader
  checked the records without importing producer helpers or replaying a guest.

| Profile | Actual original job | Cases | Entries and returns | Original log bytes / SHA256 |
| --- | --- | --- | --- | --- |
| Feature off | [114826641722](https://github.com/CrispStrobe/labwired-core/actions/runs/38252022767/job/114826641722) | 24 | 8 / 8 | 642946 / `28c3923110d30c26e8b3291b649062ee750f52cf873e62be40e1bfd1938da84f` |
| Feature on | [114826641879](https://github.com/CrispStrobe/labwired-core/actions/runs/38252022767/job/114826641879) | 48 | 16 / 16 | 110449 / `2c548f7cd4a4e016a4c0301eb6d953c486416b6cc5715ca38c040fe648777e69` |

## Observed boundary

The authored Thumb guests configure SPI and NVIC themselves and exercise
DRE/TXC/RXC/ERROR on IRQ62/63/64/65 (exceptions78/79/80/81). Each unmasked
positive enters and returns once, with an actual handler-written count/vector
witness and final thread marker. PRIMASK-blocked controls show no entry or
return. Reset-before-unmask controls show no entry, cleared pending state and
cleared SPI flags after SWRST followed by MCLK removal. All rows finish in
thread mode; source assertions also require zero CFSR/HFSR.

Feature-off covers the forced walk and ordinary feature-off path. Feature-on
covers the forced one-cycle reference and ordinary scheduler intervals1/64/1024.
No case is missing or duplicated. The same integration target passed separate
MMIO assertions for simultaneous levels across the NVIC bank boundary,
per-source W1C/masking, FIFO consumption, non-consuming debug peeks/upper DATA
lanes, software-pend preservation and reset. Existing controller, clock pause,
cancelled-wake and ST7735 guest checks also passed in both affected jobs.

The independent reader's manufactured controls passed two row positives/eight
row refusals and one metadata positive/fourteen metadata refusals before actual
adoption. Those controls validate the reader, not emulator fidelity. This is
source-bound hosted test/log evidence, not full source-to-binary provenance,
physical capture, CPU batch-quantum qualification, dynamic clock-rate or
asynchronous clock-gated IRQ qualification, DMAC, real CODAL fiber services,
runtime/store admission, or an active Arcade RTx result.

Continue with the [bounded IRQ contract](../../engineering/samd-sercom-split-irq.md)
and [ordered runtime plan](../../engineering/samd-runtime-replacement-plan.md).
