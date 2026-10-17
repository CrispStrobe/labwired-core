/* SPDX-License-Identifier: MIT */
ENTRY(reset_entry)
MEMORY { FLASH(rx) : ORIGIN = 0x4000, LENGTH = 0x74000
         RAM(rwx) : ORIGIN = 0x20000000, LENGTH = 192K }
SECTIONS {
 .vectors : {KEEP(*(.vectors))} > FLASH
 .text : {*(.text*) *(.rodata*) . = ALIGN(4);} > FLASH
 .data : { . = ALIGN(4); _data_start = .; *(.data*) . = ALIGN(4); _data_end = .; } > RAM AT>FLASH
 _data_load = LOADADDR(.data);
 _program_load_end = LOADADDR(.data) + SIZEOF(.data);
 .bss (NOLOAD) : { . = ALIGN(4); _bss_start = .; *(.bss*) *(COMMON) . = ALIGN(4); _bss_end = .;} > RAM
 _stack_top = ORIGIN(RAM)+LENGTH(RAM);
 /DISCARD/ : { *(.ARM.exidx*) *(.ARM.extab*) *(.comment*) *(.eh_frame*) }
 ASSERT(_program_load_end <= 0x78000,"program overlaps reserved storage")
 ASSERT(_bss_end <= _stack_top - 4096,"stack overlaps runtime data")
}
