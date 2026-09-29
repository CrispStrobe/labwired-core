/* STM32F401RE. 0x20000000..0x20001000 is left to the RESULT block and the
 * boot counter, which the firmware reads and writes by address; .bss starts
 * above it and is zeroed by reset(). */
ENTRY(reset)
MEMORY {
  FLASH (rx) : ORIGIN = 0x08000000, LENGTH = 512K
  RAM (rwx) : ORIGIN = 0x20001000, LENGTH = 92K
}
SECTIONS {
  .text : { KEEP(*(.vectors)) *(.text*) *(.rodata*) } > FLASH
  .bss (NOLOAD) : { __bss_start = .; *(.bss*) *(COMMON) *(.data*) __bss_end = .; } > RAM
}
