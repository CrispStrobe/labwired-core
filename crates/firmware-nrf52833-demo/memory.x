/* LabWired - Firmware Simulation Platform
 * Copyright (C) 2026 Andrii Shylenko
 *
 * This software is released under the MIT License.
 * See the LICENSE file in the project root for full license information.
 *
 * Nordic nRF52833: 512 KB flash @ 0x00000000, 128 KB RAM @ 0x20000000
 * (nRF52833 Product Specification v1.7, memory map §4.2.4).
 */

MEMORY
{
  FLASH : ORIGIN = 0x00000000, LENGTH = 512K
  RAM   : ORIGIN = 0x20000000, LENGTH = 128K
}
