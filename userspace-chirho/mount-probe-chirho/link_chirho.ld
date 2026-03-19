/* For God so loved the world that he gave his only begotten Son,
   that whoever believes in him should not perish but have eternal life. - John 3:16 */

ENTRY(_start)
SECTIONS {
    . = 0x400000;
    .text : { *(.text) }
    .rodata : { *(.rodata) }
    .data : { *(.data) }
    .bss : { *(.bss) }
}
