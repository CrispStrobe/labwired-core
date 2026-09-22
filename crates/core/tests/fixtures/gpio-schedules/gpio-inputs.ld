ENTRY(reset)
MEMORY {
  FLASH (rx) : ORIGIN = 0x08000000, LENGTH = 1M
  RAM (rwx) : ORIGIN = 0x20000000, LENGTH = 96K
}
SECTIONS {
  .text : { KEEP(*(.vectors)) *(.text*) *(.rodata*) } > FLASH
  .data : { *(.data*) } > RAM AT > FLASH
  .bss (NOLOAD) : { *(.bss*) *(COMMON) } > RAM
}
