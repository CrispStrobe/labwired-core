/* LabWired - Firmware Simulation Platform
 * Copyright (C) 2026 Andrii Shylenko
 *
 * This software is released under the MIT License.
 * See the LICENSE file in the project root for full license information.
 *
 * Adafruit PyBadge (SAMD51J19A): 512 KB flash, 192 KB SRAM (DS60001507).
 * The resident UF2 bootloader (uf2-samdx1 arcade_pybadge) owns the first
 * 16 KiB, so the application image and its vector table start at 0x4000.
 */

MEMORY
{
  FLASH : ORIGIN = 0x00004000, LENGTH = 496K
  RAM   : ORIGIN = 0x20000000, LENGTH = 192K
}
