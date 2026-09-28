MEMORY { FLASH (rx) : ORIGIN = 0x08000000, LENGTH = 64K }
SECTIONS {
  .text : { KEEP(*(.vectors)) *(.text*) } > FLASH
}
