# Multiplexed segment display

N-digit 7-, 14- or 16-segment LED display without a driver chip. The MCU drives the segment lines and one select line per digit from GPIO, and lights one digit at a time.

## Status at a glance

| Aspect | Status |
|--------|--------|
| Device descriptor | [`configs/devices/segment_display_mux.yaml`](../../configs/devices/segment_display_mux.yaml) (primitive `segment_display`) |
| Buses | GPIO: any number of segment pads and digit-select pads, on any ports |
| Catalog type id | `segment-display-mux` |
| Example system | [`tests/fixtures/segment-mux/`](../../tests/fixtures/segment-mux/) (STM32F103, 3 digits, "1.23") |
| Tier | modeled |

## Pins / attachment

| Config key | What |
|------------|------|
| `segment_pins` | list of pads, one per segment line; the index is the segment bit |
| `digit_pins` | list of pads, one per digit select, leftmost digit first |
| `segment_names` | name per segment pin; default for 7/8 pins `a b c d e f g dp`, for 14/15 pins `a b c d e f g1 g2 h j k l m n dp` |
| `segment_active_high`, `digit_active_high` | active level of the lines (default `true`) |
| `font` | built-in font: `seven_segment`, `fourteen_segment` or `none` (default by pin count) |
| `glyphs` | map of character to segment names, checked before the built-in font |
| `persistence_us`, `threshold_pct`, `min_duty_pct` | persistence window (20000), visibility threshold relative to the brightest LED (50), and minimum duty in the window (1) |

14-segment names: `a`..`f` outer, `g1`/`g2` middle bar, `h` upper-left diagonal, `j` upper vertical, `k` upper-right diagonal, `l` lower-left diagonal, `m` lower vertical, `n` lower-right diagonal, `dp` decimal point.

```yaml
external_devices:
  - id: "display"
    type: "segment-display-mux"
    connection: "gpioa"
    config:
      segment_pins: [PA0, PA1, PA2, PA3, PA4, PA5, PA6, PA7]
      digit_pins: [PA8, PA9, PA10]
```

## Support matrix

| Behavior | Status | Notes |
|----------|--------|-------|
| Segment lit only when its line AND its digit select are active | ✅ | active levels from config |
| Persistence of vision | ✅ | lit time integrated exactly per GPIO store; visible per 20 ms window |
| Ghosting (segments change before the select) | ✅ | a short ghost stays dark (threshold) |
| Blank when not refreshed | ✅ | |
| Font decode | ✅ | built-in 7/14-segment font + placement glyphs; unknown pattern shows `?`; `dp` shows `.` |
| Readback | ✅ | logs `text` and `frames` (`peripheral_log`), `text_display` artifact |
| Brightness levels | ❌ | a segment is visible or not |
| Pad direction | ❌ | the output register level is the pad level |

## How to run

```yaml
assertions:
  - peripheral_log: {peripheral: display, log: text, contains: "\"1.23\""}
```

`labwired test --script ...` with the system above. The `text` log has one line per change of the visible text, for example `"1.23" at cycle 1440000`.

## Limitations

See the header of the descriptor YAML: no brightness, no pad direction, no current sharing between two selected digits.

## Related

- [Seven-segment](seven-segment.md) (one digit, combinational)
- [Parts index](index.md)
