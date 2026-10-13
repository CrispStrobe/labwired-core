/* NUCLEO-U545RE-Q: 512 KiB flash, 256 KiB SRAM1+2 (ST STM32U545xx_FLASH.ld). */
MEMORY
{
  FLASH (rx)  : ORIGIN = 0x08000000, LENGTH = 512K
  RAM   (xrw) : ORIGIN = 0x20000000, LENGTH = 256K
}
_estack = ORIGIN(RAM) + LENGTH(RAM);
ENTRY(Reset_Handler)
SECTIONS
{
  .isr_vector : { KEEP(*(.isr_vector)) } >FLASH
  .text : { *(.text*) *(.rodata*) } >FLASH
  /DISCARD/ : { *(.ARM.exidx*) *(.comment) }
}
