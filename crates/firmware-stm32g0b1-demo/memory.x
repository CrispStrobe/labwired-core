/* LabWired - Firmware Simulation Platform
 * Copyright (C) 2026 Andrii Shylenko
 *
 * This software is released under the MIT License.
 * See the LICENSE file in the project root for full license information.
 *
 * STM32G0B1RE slice: 512 KB flash, 128 KB SRAM (DS13919 / the chip yaml).
 */

MEMORY
{
  FLASH : ORIGIN = 0x08000000, LENGTH = 512K
  RAM : ORIGIN = 0x20000000, LENGTH = 128K
}
