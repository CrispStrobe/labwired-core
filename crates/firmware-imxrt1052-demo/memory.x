/* LabWired - Firmware Simulation Platform
 * Copyright (C) 2026 Andrii Shylenko
 *
 * This software is released under the MIT License.
 * See the LICENSE file in the project root for full license information.
 *
 * FB200 / i.MX RT1052 smoke. The vector table sits where the chip yaml says
 * the image starts: FlexSPI 0x6000_0000 + reset_vector_offset 0x10000, the
 * same place the FB200 stock block 0 puts its [SP][reset] pair. The boot ROM
 * and the FlexSPI config block / IVT are not modelled, so the image carries
 * neither. .data/.bss/stack live in DTCM @ 0x2000_0000.
 * Soft-float target: thumbv7em-none-eabi (not eabihf).
 */

MEMORY
{
  FLASH : ORIGIN = 0x60010000, LENGTH = 64K
  RAM   : ORIGIN = 0x20000000, LENGTH = 128K
}
