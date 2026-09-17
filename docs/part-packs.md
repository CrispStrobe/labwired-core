# Part packs — `labwired.part/v1`

A **part pack** is one file describing one part, completely. It is the only
thing you need to connect a part LabWired has never seen — from a private
catalog, a customer's internal library, a vendor's own repo, or a directory on
your laptop — without a line of code in this repository and without publishing
anything.

The contract exists because the alternative had already grown four places to
edit per part: a descriptor in `configs/devices/`, a `KITS` entry, a catalog
record in the app, and a hand-mirrored emitter. All four are derivable from one
document, so this is that document.

## Why one file and not four

A part is one physical thing. Splitting its description across repos means the
halves drift, and drift in this domain is silent: a catalog record claiming 6
pins against a model with 4 does not fail a build, it fails a customer's
firmware at 3am with a wiring error that reads like their bug. One file, one
part, one source of truth — and the loader refuses a second definition of the
same `type` rather than picking a winner.

## The document

```yaml
schema: labwired.part/v1       # required — the contract version this file obeys
type: acme:tmp999              # required — globally unique id, `vendor:part`
source: acme-private           # optional — provenance; who shipped this pack
overrides: tmp102              # optional — the ONLY way to shadow a built-in

behavior:                      # required — how it behaves on the wire (the sim)
  primitive: i2c_device
  i2c:
    default_address: 0x4A
    registers: [ ... ]

emit:                          # optional — canvas wiring → system-manifest entry
  connection: i2c
  config: [ ... ]

metadata:                      # optional — label, summary, stimulus channels
  label: "ACME TMP999"
  inputs: [ ... ]

catalog:                       # optional — the app-layer record (pins, class)
  deviceClass: i2c_device
  refPrefix: U
  pins: [ ... ]
```

`behavior`, `emit` and `metadata` are the existing `configs/devices/*.yaml`
schema, unchanged — every shipped descriptor is already a valid pack body, which
is deliberate: the built-in parts and your private parts are the same kind of
object, so the private path is the one we dogfood daily rather than a bolted-on
side door.

`catalog` is ignored by this crate. It is carried through for the app layer
(`@labwired/board-config`), so a pack stays one file end to end. Its fields are
the `CatalogPart` fields, spelled exactly as TypeScript spells them
(`deviceClass`, `refPrefix`, `defaultI2cAddress`, `pins`) — the block is handed
over verbatim, and a rename step here would only be one more thing to get wrong.

### `type` must be namespaced

Use `vendor:part`. A bare `tmp999` is accepted but risks colliding with a
built-in we add later; a collision is a hard error at load, so an un-namespaced
private pack is a future build break you have chosen to schedule.

### `overrides` is the only way to shadow a built-in

If a pack's `type` names a part the engine already ships, loading fails:

```
part pack 'tmp102' (source: acme-private) shadows a built-in part.
Set `overrides: tmp102` to replace it deliberately, or rename the pack.
```

Setting `overrides:` to the same string makes the replacement explicit and
attributable in a bug report. Silence is never the answer to "which model ran?".

## Connecting a pack

Packs travel in the system manifest, so every transport that already carries a
manifest carries packs too — the CLI, the browser wasm build, and the hosted
builder's `/run`. There is no new endpoint and no new file the runtime has to
find on disk.

```yaml
# system.yaml
chip: "esp32c3"
parts:
  - path: "./private/acme-tmp999.yaml"    # CLI-only convenience, inlined on load
  - schema: labwired.part/v1              # or the pack inline, verbatim
    type: acme:hum1
    behavior: { ... }

external_devices:
  - id: t1
    type: acme:tmp999                     # resolves against `parts:` above
    connection: i2c0
    route: { sda: "GPIO4", scl: "GPIO5" }
```

`path:` is a `labwired` CLI convenience: `SystemManifest::from_file` reads the
file and replaces the entry with its contents, exactly as it already does for a
`can-player`'s `path:`. The simulation core never sees a `path:` — it has no
filesystem in wasm, and a contract that only works on one of our three runtimes
is not a contract.

## Resolution order

For an `external_devices[].type`, in order:

1. `parts:` in this manifest
2. the built-in `PeripheralKit` registry (`peripherals::kit::registry`)
3. the embedded declarative descriptors (`configs/devices/*.yaml`)
4. the legacy hand-written attach arms

A pack at step 1 that also exists at step 2 or 3 is the collision error above,
not a silent win.

## Connecting a source to the app

The engine reads packs out of a manifest. The app is what puts them there:

```ts
import { registerPartSource } from '@labwired/board-config';

registerPartSource({
  id: 'acme-private',
  packs: await fetchEntitledCatalog(orgId),   // any origin: HTTP, file, bundle
});
```

From that call on, the part behaves like any other:

- `getCatalogPart('acme:tmp999')` resolves it, so ERC, wiring, netlist export
  and the compiler treat it as a first-class part.
- `listCatalogParts()` includes it, so the palette offers it. (Enumerate with
  that, never `Object.values(CATALOG)` — the latter sees only parts we ship,
  which is how a connected part ends up simulating correctly while being
  invisible in the palette meant to offer it.)
- `compile()` inlines the packs the diagram actually uses into the manifest's
  `parts:`, so the lab runs on an engine that has never heard of your catalog —
  and keeps running after the source that supplied the part is gone.
- The canvas draws it from its declared `pins` via the generic renderer. Shipping
  hand-drawn artwork is an improvement, never a prerequisite.

Registration enforces the same rules the engine does, and one more: the app's
built-in set is not the engine's. A part can be catalogued in the app and
modelled in the engine, or modelled in the engine and absent from the app
catalog (`tmp102` is). Each side therefore checks its own set, and a pack has to
clear both.

## Register encoding keys

`encode:` is the register's measurement encoding. Beyond `scale` / `offset` /
`clamp_min` / `clamp_max` / `wrap` it carries four keys that exist because a
real part needed them, and each is documented here with the part that found it.

### `encode: { bcd: true }` — binary-coded decimal, both directions

Two decimal digits per byte, tens in the high nibble. **Symmetric**: a read
encodes, a write decodes. It is the LAST step of the read encode (after scale,
offset, the clamp window and `wrap`) and the FIRST step of the write decode, so
the word the model STORES is always decimal — `reg()`, `field()` and
`scale_from` all read a number, never a pair of nibbles.

```yaml
# DS3231 0x00: the seconds of a settable clock. `write_mask` on a BCD register
# is a plain AND on the byte the master wrote (the wire domain), because the
# flag packed alongside is not part of the number.
- { name: SECONDS, addr: 0x00, width: 1, endian: be, access: rw,
    source: unix_time, calendar: second, write_mask: 0x7F,
    encode: { bcd: true } }

# A plain BCD storage register — an alarm byte, clamped to the range it holds.
- { name: ALARM1_SECONDS, addr: 0x07, width: 1, endian: be, access: rw,
    write_mask: 0x7F, encode: { bcd: true, clamp_min: 0.0, clamp_max: 59.0 } }
```

A nibble above 9 is not a decimal digit; on the write side it is decoded the way
a counter chain reads it (`0x1A` is 20), and on the read side a count with more
digits than the register holds saturates at all-nines.

### `calendar:` — one civil field of a settable clock

On a register whose `source:` carries Unix seconds. A **read** reports that
civil field of the instant; a **write** RECOMPOSES — it replaces that field and
leaves the other six. Fields: `second` `minute` `hour` `weekday` `day` `month`
`year` (`year` is the two digits an RTC holds, `weekday` is 1..7 Sunday-first).

```yaml
- { name: HOURS, addr: 0x02, width: 1, endian: be, access: rw,
    source: unix_time, calendar: hour, write_mask: 0x3F,
    encode: { bcd: true } }
```

Without the write half a `source`d register is read-only and `RTClib::adjust()`
— the first call almost every RTC sketch makes — does nothing at all. Without
the read half the seven registers are seven independent bytes that can disagree
with each other about what day it is. The arithmetic is Hinnant's
civil-from-days / days-from-civil pair, UTC, no leap seconds.

⚠️ `input(KEY)` **skips** a `calendar:` register when it looks for the encoding
to report a channel through: such a register reports a FIELD, not the value, so
a rule asking for `input(unix_time)` gets the truncated engineering value.

### `encode: { clamp_from: [...] }` — a field-driven saturation window

The mirror of `scale_from`. The window is read from another register's
bit-field instead of being a constant, because on many parts the saturation
point is something firmware chose.

```yaml
# ADXL345 DATAX0. FULL_RES (bit 3) and the range bits (1:0) are not contiguous,
# so one mask picks all three and the map is keyed by the combination.
- name: DATAX0
  addr: 0x32
  width: 2
  endian: le
  access: r
  signed: true
  source: x
  scale_from: { register: DATA_FORMAT, mask: 0x0B,
                map: { 0x00: 256.0, 0x03: 32.0, 0x0B: 256.0 } }
  encode:
    clamp_from:
      - register: DATA_FORMAT
        mask: 0x0B
        map:
          0x00: { min: -512.0,  max: 512.0 }
          0x03: { min: -512.0,  max: 512.0 }
          0x0B: { min: -4096.0, max: 4096.0 }
```

A field value absent from `map` leaves the constant window (or none) in force —
the same "unmapped ⇒ neutral" rule `scale_from` has. Several entries INTERSECT,
each narrowing the window. A constant `clamp_max` here would be right for
exactly one of eight settings and would silently stop the part saturating at
the other seven.

### `encode: { round: floor | ceil | trunc | nearest }`

How the encoded value becomes an integer count. `nearest` (`f64::round`) is the
default and is what every descriptor written before the key existed means.

## Stimulus-channel keys

### `noise_sigma_key` — one `config:` value over a channel SET

A channel's `noise_sigma` is a property of the part; `noise_sigma_key` names the
`config:` key a PLACEMENT can set to override it. Spelled once per channel, so
one key reaches a whole set:

```yaml
metadata:
  config_keys:
    - { name: noise_sigma, ty: float,
        doc: "Gaussian noise sigma in channel units (g accel, °/s gyro)." }
  inputs:
    - { key: ax, label: "Accel X", unit: g, min: -16, max: 16,
        noise_sigma_key: noise_sigma }
    - { key: ay, label: "Accel Y", unit: g, min: -16, max: 16,
        noise_sigma_key: noise_sigma }
    # …and the other four motion axes. `temp` deliberately does NOT carry it:
    # the documented sigma is in g and °/s.
```

Per channel rather than as a group so a part whose axes have genuinely different
figures can still say so, and so reading one channel's entry tells you
everything that moves it. The key must also appear in `metadata.config_keys` to
be advertised in the peripheral manifest.

### `expr_scale` — counts per engineering unit, for a rule expression

The rule language is integers. On a register device `input(KEY)` is already the
value the register reports, so a rule comparing it against `reg(DATA)` compares
like with like. A pins-only part has no register to borrow an encoding from, so
it states the same thing directly:

```yaml
# HX711: the channel is grams and the frame is 24 bits at 100 counts per gram.
- { key: weight, label: "Weight", unit: g, min: -50000, max: 50000,
    default: 0, expr_scale: 100.0 }
```

Without it `input(weight)` truncates to whole grams and a load cell loses
exactly the digits it exists to measure — silently, because 10 g and 10.5 g
would shift out the same word.

## Edge-driven `gpio_device` parts

A `gpio_device` is serviced on the peripheral tick. That is right for a part
sampled on a schedule and **wrong for a part clocked by firmware**: a
`digitalWrite(SCK, HIGH); digitalWrite(SCK, LOW)` pair is two MMIO stores inside
one tick interval, so a tick-only pass samples the pad after both and sees no
change. A 24-bit shift-out clocked by 48 stores would deliver one edge, or none.

A descriptor whose `rules:` listen for a pin EDGE is therefore serviced
synchronously inside the MMIO write path, and nothing extra is declared to get
it — the engine reads the rules:

```yaml
behavior:
  primitive: gpio_device
  pins:    { SCK: sck_pin }      # observed: pads the MCU drives
  outputs: [DOUT]                # driven: pads the MCU samples
  output_pins: { DOUT: dt_pin }
  rules:
    - on: { pin: SCK, edge: rising }   # ⇐ this makes the part edge-driven
      when: "var(shifting) && var(bit_index) < 24"
      do:
        - { var: { name: dout_level, value: "(var(raw) >> (23 - var(bit_index))) & 1" } }
        - { var: { name: bit_index, value: "var(bit_index) + 1" } }
        - { pin: DOUT, level: "var(dout_level)" }
```

⚠️ **Rule order is load-bearing.** Rules fire in declaration order and each
`do:` runs to completion, so a later rule sees what an earlier one assigned. In
`hx711.yaml` the rule that CLOSES the frame is declared before the rule that
shifts a bit: the other way round, the 24th edge would set `bit_index` to 24 and
the close rule would fire in the SAME event, dropping DOUT before the master
sampled the last bit. The frame reads one bit short, in the low bit only, every
time.

The pads such a part drives still go out through the narrowed `DevicePins` port,
exactly as on the tick pass — this changes WHEN `service` runs, not what it may
touch.

## What a pack cannot do

A pack is data interpreted by a **primitive** — `i2c_device`, `spi_device`,
`analog_source`, `display`, `gpio_device`, `quadrature`, `matrix`, `one_wire`,
`pulse_echo`.
Those primitives are the irreducible timing algorithms, and they live in Rust in
this repository.

`analog_source` is the primitive for parts whose whole interface is one
analogue voltage (a Sharp IR ranger's `Vo`, an MQ-x module's `AOUT`): the
descriptor carries the datasheet's output curve as `(input, mV)` points plus
stated out-of-band rules (`below_first: clamp`, `above_last.floor_mv`), and the
engine owns the rest (SimInput plumbing, mV→ADC count, attach). The proof part
is `gp2y0a21.yaml`.

`display` is the primitive for framebuffer panels. The descriptor carries the
frame memory's geometry and pixel format, how a command byte is told apart from
a data byte (a D/C pad on 4-wire SPI, a control byte on I²C), the command table
as `{ opcode, args, do }`, and how the address counters wrap per addressing
mode. The engine owns the counter arithmetic, the window wrap, the orientation
map and the paint artifact — one implementation for every panel. Pixel VALUES
are never transformed: contrast, gamma and inversion are reported as flags, so
what the artifact holds is what firmware wrote and a photograph of the glass can
be compared against it. The proof parts are `ssd1306.yaml` (I²C, page-major
1 bpp) and `st7789.yaml` (SPI, row-major RGB565 with MADCTL orientation).

So: a part whose datasheet behaviour is a register map, a command/response
protocol, a framebuffer command table, or one of the pin-timing shapes above is
pure data and needs nothing from us. A part with a genuinely new wire protocol needs a new primitive, which
is a change to this crate. That boundary is honest and worth stating to a
customer up front: we can onboard your sensor catalogue without seeing it, but a
novel protocol is engineering, not configuration.

The same split applies to silicon. A private MCU is a chip descriptor —
`chip: "./acme-soc.yaml"` on the CLI, or the `chipYaml` field on the hosted
builder's `/run` — and needs no code here as long as its peripheral blocks are
ones the engine models. A novel peripheral block does not.

## Regenerating the cross-boundary fixture

`crates/core/tests/fixtures/emitted-part-pack-manifest.yaml` is `compile()`
output captured verbatim, so the engine test proves it can run the manifest the
app actually writes. Regenerate it from `packages/board-config`:

```sh
npx tsx -e "
import { compile } from './src/compile';
import { registerPartSource } from './src/part-sources';
registerPartSource({ id: 'acme-private', packs: [/* the pack in the test */] });
console.log(compile(/* the diagram in the test */).systemYaml);
" > ../../core/crates/core/tests/fixtures/emitted-part-pack-manifest.yaml
```

A diff there is a real cross-boundary change and wants reading, not blessing.

## Not yet covered

Named so nobody discovers them by surprise:

- **Private boards.** A private *chip* runs today through the paths above, but
  the app's `BOARDS` and `CHIP_YAMLS` are still build-time constants, so a
  private board cannot be offered in the picker the way a private part can be
  offered in the palette. Extending this contract to boards is the natural next
  increment — it carries more than a part does (pin map, renderer, compile
  toolchains, PlatformIO profile), which is why it is not folded in here.
- **Reverse mapping.** `system-to-diagram.ts` turns a manifest back into a
  canvas from `CATALOG` alone, so a shared lab containing a pack part will not
  round-trip into a diagram until that reads through `getCatalogPart` too.
- **Legacy compat maps.** `component-meta.ts` derives `COMPONENT_META` from
  `CATALOG` at module-init, so a pack declaring `boardIoKind` is not seen by the
  wire-derived board_io path. Same for `partSeedIntent.ts` and the ERC
  "did you mean" suggestions.
- **Entitlement.** Nothing here decides WHO may load which catalog. A pack is
  data; deciding that an org may fetch it is the API's job, not this contract's.
