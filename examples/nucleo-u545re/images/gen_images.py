#!/usr/bin/env python3
"""Generate board.svg and pinout.svg for the NUCLEO-U545RE-Q example.

Layout provenance: component and header positions are measured from ST's
product photograph of the MB1841 board (pixel coordinates, ~4 px/mm, one SVG
unit = one photo pixel) and the Arduino-header pin map is taken from the Zephyr
`nucleo_u545re_q` board (arduino_r3_connector.dtsi). ST's CAD files (board
design project / manufacturing files on st.com) were not reachable when this
was written, so this is NOT CAD-derived; regenerate from the CAD when available.
No vendor logos or wordmarks are drawn.

    python3 gen_images.py
"""
from pathlib import Path

OUT = Path(__file__).parent
PCB, PCB_EDGE = "#dfe9f2", "#8fa6bb"
HDR, PIN, SILK = "#1c1f23", "#c9a94a", "#14324f"   # SILK on PCB: contrast > 10:1

# Arduino header rows: (x, first_y, pitch, [(label, mcu_pin)])
CN5 = [("D15", "PB6"), ("D14", "PB7"), ("AVDD", ""), ("GND", ""), ("D13", "PA5"),
       ("D12", "PA6"), ("D11", "PA7"), ("D10", "PC9"), ("D9", "PC6"), ("D8", "PC7")]
CN9 = [("D7", "PA8"), ("D6", "PB10"), ("D5", "PB4"), ("D4", "PB5"),
       ("D3", "PB3"), ("D2", "PC8"), ("D1", "PA2"), ("D0", "PA3")]
CN6 = [("NC", ""), ("IOREF", ""), ("NRST", ""), ("3V3", ""), ("5V", ""),
       ("GND", ""), ("GND", ""), ("VIN", "")]
CN8 = [("A0", "PA0"), ("A1", "PA1"), ("A2", "PA4"), ("A3", "PB0"),
       ("A4", "PC1"), ("A5", "PC0")]


def rect(x, y, w, h, fill, stroke="none", rx=0, extra=""):
    return (f'<rect x="{x:.1f}" y="{y:.1f}" width="{w:.1f}" height="{h:.1f}" rx="{rx}" '
            f'fill="{fill}" stroke="{stroke}" {extra}/>')


def text(x, y, s, size=7, fill=SILK, anchor="middle", weight="normal"):
    return (f'<text x="{x:.1f}" y="{y:.1f}" font-family="ui-sans-serif,system-ui,sans-serif" '
            f'font-size="{size}" font-weight="{weight}" fill="{fill}" text-anchor="{anchor}">{s}</text>')


def header(cx, y0, pitch, n, cols=1, colgap=9.6):
    w = 9.0 + (cols - 1) * colgap
    out = [rect(cx - 4.5, y0 - 5, w, n * pitch, HDR, rx=1)]
    for i in range(n):
        for c in range(cols):
            out.append(f'<circle cx="{cx + c * colgap:.1f}" cy="{y0 + i * pitch + pitch / 2 - 5:.1f}" r="2.1" fill="{PIN}"/>')
    return out


def board(with_labels):
    g = []
    g.append(rect(74, 44, 277, 330, PCB, PCB_EDGE, rx=7, extra='stroke-width="1.5"'))
    g.append(rect(144, 340, 36, 42, "#b9bec4", "#555b61", rx=2))            # USER USB-C (CN3)
    g.append(rect(246, 44, 38, 40, "#b9bec4", "#555b61", rx=2))             # ST-LINK USB-C (CN1)
    for hx, hy in ((84, 93), (342, 118), (298, 357)):
        g.append(f'<circle cx="{hx}" cy="{hy}" r="4" fill="#fff" stroke="{PCB_EDGE}"/>')
    g.append(rect(171, 87, 41, 40, "#20252a", rx=2))                         # ST-LINK MCU
    g.append(rect(193, 263, 40, 40, "#20252a", rx=2))                        # STM32U545RET6 (LQFP64)
    g.append(f'<circle cx="199" cy="269" r="2" fill="#555b61"/>')
    g.append(rect(190, 259, 47, 47, "none", "#4a5158", extra='stroke-dasharray="1.5 1.5"'))
    g.append(rect(90, 104, 30, 30, "#9aa3ad", "#555b61", rx=2))             # MIPI10 / SWD (CN4)
    g.append(rect(295, 101, 21, 51, HDR, rx=1))                              # JP3 5V_SEL
    g.append(rect(81, 54, 12, 24, HDR, rx=1)); g.append(rect(335, 54, 12, 24, HDR, rx=1))  # JP1, JP2
    g.append(f'<circle cx="178.7" cy="160.7" r="10" fill="#2f6fd0" stroke="#14324f"/>')    # B1 USER
    g.append(f'<circle cx="245.7" cy="161.7" r="10" fill="#2a2d31" stroke="#0d1013"/>')    # B2 RESET
    g.append(f'<circle cx="299" cy="64.7" r="4" fill="#e8c14a"/>')                          # PWR LED
    g.append(f'<circle cx="320" cy="64.7" r="4" fill="#e8c14a"/>')                          # COM LED
    g.append(f'<circle cx="283" cy="157" r="3" fill="#3fbf5f" stroke="#14324f"/>')           # LD2 (PA5)
    g.append(rect(256, 351, 26, 9, "#c9a94a", "#14324f"))                                   # solder-bridge pads
    g += header(85.4, 175.7, 10.07, 19, cols=2)                                             # CN7
    g += header(331.4, 175.7, 10.07, 19, cols=2)                                              # CN10
    g += header(114.5, 219, 9.9, 8)                                                         # CN6
    g += header(114.5, 304, 9.8, 6)                                                         # CN8
    g += header(312, 175.7, 10.1, 10)                                                       # CN5
    g += header(312, 283, 10.1, 8)                                                          # CN9
    if with_labels:
        g += [text(178.7, 182, "USER", 6), text(178.7, 163, "B1", 6, "#ffffff"),
              text(245.7, 182, "RESET", 6), text(245.7, 164, "B2", 6, "#ffffff"),
              text(283, 150, "LD2", 6), text(213, 280, "U545RE", 7, "#e8eee9", weight="bold"),
              text(265, 76, "ST-LINK", 6, "#14324f"), text(162, 364, "USER", 6, "#14324f"),
              text(191.5, 109, "ST-LINK", 6, "#e8eee9"),
              text(90, 168, "CN7", 6), text(336, 168, "CN10", 6), text(114.5, 212, "CN6", 6),
              text(114.5, 298, "CN8", 6), text(312, 168, "CN5", 6), text(312, 276, "CN9", 6)]
    return g


def svg(title, desc, body):
    return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="60 30 300 365" width="600" height="730">\n'
            f'  <title>{title}</title>\n  <desc>{desc}</desc>\n'
            f'  <rect x="60" y="30" width="300" height="365" fill="#ffffff"/>\n  '
            + "\n  ".join(body) + "\n</svg>\n")


def pinout():
    g = board(True)
    def label_rows(rows, x, y0, pitch, anchor, dx):
        for i, (lab, pin) in enumerate(rows):
            y = y0 + i * pitch + pitch / 2 - 2
            s = f"{lab} {pin}".strip()
            g.append(text(x + dx, y, s, 6, SILK, anchor))
    # Arduino rows: labels sit on the PCB, inboard of each header.
    label_rows(CN5, 312, 175.7, 10.1, "end", -8)
    label_rows(CN9, 312, 283, 10.1, "end", -8)
    label_rows(CN6, 114.5, 219, 9.9, "start", 8)
    label_rows(CN8, 114.5, 304, 9.8, "start", 8)
    g.append(text(213, 330, "LD2 PA5   B1 PC13", 7, SILK, weight="bold"))
    g.append(text(213, 340, "VCP USART1 PA9 TX / PA10 RX", 6, SILK))
    return g


if __name__ == "__main__":
    (OUT / "board.svg").write_text(svg(
        "NUCLEO-U545RE-Q top illustration",
        "Flat vector of the 64-pin Nucleo board: ST-LINK USB-C at the top, user USB-C at the "
        "bottom, B1 USER and B2 RESET buttons, STM32U545 LQFP64 mid-lower, Arduino headers "
        "CN5/CN6/CN8/CN9 and Morpho CN7/CN10. No vendor logos.", board(True)), encoding="utf-8")
    (OUT / "pinout.svg").write_text(svg(
        "NUCLEO-U545RE-Q pinout",
        "Arduino header map (Zephyr nucleo_u545re_q), LD2 PA5, B1 PC13, VCP USART1 PA9/PA10. "
        "Morpho pins are not enumerated.", pinout()), encoding="utf-8")
