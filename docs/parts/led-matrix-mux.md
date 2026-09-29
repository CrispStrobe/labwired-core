# Multiplexed LED matrix

Row/column LED matrix without a driver chip — the BBC micro:bit display, a bare 8×8 matrix. The MCU drives the row lines and the column lines from GPIO and lights one row at a time.

## Status at a glance

| Aspect | Status |
|--------|--------|
| Device descriptor | [`configs/devices/led_matrix_mux.yaml`](../../configs/devices/led_matrix_mux.yaml) (primitive `led_matrix`) |
| Buses | GPIO: any number of row pads and column pads, on any ports |
| Catalog type id | `led-matrix-mux` |
| Example systems | [`configs/systems/microbit-v2.yaml`](../../configs/systems/microbit-v2.yaml) (5×5, columns driven by GPIOTE); the `layout` key also covers a matrix wired differently from how it is arranged (the micro:bit V1: 3 × 9 lines shown as 5×5) |
| Tier | modeled |

## Pins / attachment

| Config key | What |
|------------|------|
| `row_pins` | list of pads, one per row line, electrical order |
| `col_pins` | list of pads, one per column line, electrical order |
| `row_active_high`, `col_active_high` | active level of the lines (default `true`; the micro:bit's columns are active low) |
| `layout` | one list per row line, one cell per column line: `"x,y"` (the picture pixel of that LED) or `"-"` (no LED). Default: the electrical matrix |
| `persistence_us` | persistence window (20000) |

```yaml
external_devices:
  - id: "led_matrix"
    type: "led-matrix-mux"
    connection: "gpio0"
    config:
      row_pins: ["P0.21", "P0.22", "P0.15", "P0.24", "P0.19"]
      col_pins: ["P0.28", "P0.11", "P0.31", "P1.05", "P0.30"]
      col_active_high: false
```

## Support matrix

| Behavior | Status | Notes |
|----------|--------|-------|
| LED lit only when its row AND its column line are active | ✅ | active levels from config |
| Persistence of vision | ✅ | lit time integrated exactly per GPIO store / pad drive; picture per 20 ms window |
| Brightness | ✅ | on-time per window; 255 = lit for a whole row slot (window / row lines) |
| Blank when not refreshed | ✅ | |
| Electrical-to-picture layout | ✅ | `layout` (micro:bit V1 3×9 → 5×5) |
| Pads owned by a peripheral | ✅ | reads the pad level, so an nRF GPIOTE Task-mode column counts |
| Readback | ✅ | log `frames` (`peripheral_log`, 0–9 per pixel), `framebuffer` artifact format `gray8` |
| Current sharing, LED colour | ❌ | a lit LED is a duty |
| Light sensing through the matrix | ❌ | column lines are never read back |

## How to run

```yaml
assertions:
  - peripheral_log: {peripheral: led_matrix, log: frames, contains: "09090 99999 99999 09990 00900"}
```

The `frames` log has one line per change of the picture, one digit group per row, top first, for example `09090 99999 99999 09990 00900 at cycle 3200000` (the micro:bit heart).
