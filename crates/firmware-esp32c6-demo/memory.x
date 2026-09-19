/* ESP32-C6-DevKitC-1 (ESP32-C6-WROOM-1, 8 MiB flash).
 *
 * The C6 has ONE unified external-memory window: both instruction fetch and
 * constant data go through 0x4200_0000..0x42FF_FFFF (ESP32-C6 Memory Map;
 * esp-idf v5.3 soc/esp32c6/include/soc/soc.h: SOC_IROM_LOW = SOC_DROM_LOW =
 * 0x42000000) — unlike the C3's separate IROM/DROM windows.
 *
 * HP SRAM is at 0x4080_0000 (SOC_IRAM_LOW) and is reachable from both the
 * instruction and data sides at that single address; 512 KB, matching
 * configs/chips/esp32c6.yaml `ram`.
 */
MEMORY
{
  FLASH : ORIGIN = 0x42000000, LENGTH = 8M
  RAM   : ORIGIN = 0x40800000, LENGTH = 512K
}

REGION_ALIAS("REGION_TEXT", FLASH);
REGION_ALIAS("REGION_RODATA", FLASH);
REGION_ALIAS("REGION_DATA", RAM);
REGION_ALIAS("REGION_BSS", RAM);
REGION_ALIAS("REGION_HEAP", RAM);
REGION_ALIAS("REGION_STACK", RAM);
