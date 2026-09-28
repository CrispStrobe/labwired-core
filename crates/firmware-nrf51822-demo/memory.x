/* LabWired - Firmware Simulation Platform
 * Copyright (C) 2026 Andrii Shylenko
 *
 * This software is released under the MIT License.
 * See the LICENSE file in the project root for full license information.
 *
 * Nordic nRF51822 as configs/chips/nrf51822.yaml maps it: the APPLICATION
 * region of an S110 v8 part. Flash is the 160 KB window from 0x18000 (the
 * MBR + SoftDevice range below it is not loaded, nordic_hole); RAM is the
 * 16 KB of the QFAA part (nRF51822 PS v3.x). This firmware does not use the
 * SoftDevice: it runs bare, with its vector table at the application base.
 */

MEMORY
{
  FLASH : ORIGIN = 0x00018000, LENGTH = 160K
  RAM   : ORIGIN = 0x20000000, LENGTH = 16K
}
