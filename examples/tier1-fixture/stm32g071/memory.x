/* LabWired - Firmware Simulation Platform
 * Copyright (C) 2026 Andrii Shylenko
 *
 * This software is released under the MIT License.
 * See the LICENSE file in the project root for full license information.
 *
 * STM32G071RB: 128 KB flash @ 0x08000000, 36 KB SRAM @ 0x20000000
 * (DS12232), matching configs/chips/stm32g071.yaml (the simulator's wiring).
 */

MEMORY
{
  FLASH : ORIGIN = 0x08000000, LENGTH = 128K
  RAM : ORIGIN = 0x20000000, LENGTH = 36K
}
