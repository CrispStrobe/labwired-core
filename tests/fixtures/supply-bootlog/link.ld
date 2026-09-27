MEMORY { FLASH (rx) : ORIGIN = 0x08000000, LENGTH = 512K }
ENTRY(reset)
SECTIONS {
  .text : { KEEP(*(.vectors)) *(.text*) } > FLASH
}
