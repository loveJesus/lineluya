<!-- For God so loved the world that he gave his only begotten Son,
     that whoever believes in him should not perish but have eternal life. - John 3:16 -->

# Escalation: .ko init_module RSVD Page Fault

## Problem
insmod of a real Alpine .ko module causes RSVD page fault at the module arena address.

## PT Dump (from serial log)
```
[PT-DUMP] va=0xffffffffc0100000 CR3=0x15303000
  PML4[511]=0x12527003
  PDPT[511]=0x12528003  
  PD[0]=0x12529003
  PT[256]=0xfffffe8000198003  ← STALE BOOTLOADER ENTRY
```

## Root Cause
- `map_page_raw_chirho` creates the arena mapping in the BOOT PML4 at boot time
- PML4[511] already exists (bootloader kernel mapping) — the mapper REUSES the existing PDPT
- PDPT[511]→PD[0]→PT[256] has a STALE entry from the bootloader's kernel text mapping
- The stale PTE `0xfffffe8000198003` has bits 52-63 set → RSVD fault
- Our raw mapper writes fresh entries to PDPT[3]→fresh_PD→fresh_PT during boot
- BUT: the per-process PT (created by fork+exec AFTER boot) copies PML4[511] from boot PML4
- The per-process PT's PML4[511] points to the SAME PDPT frame
- When looking up the arena address through the per-process PT, it walks PDPT[511] (NOT PDPT[3])
  because the virtual address 0xFFFFFFFFC0100000 maps to PDPT index 511, not 3

## THE BUG
The virtual address `0xFFFFFFFFC0100000` maps to PDPT index:
`(0xFFFFFFFFC0100000 >> 30) & 0x1FF`

In Rust with u64: `(0xFFFFFFFFC0100000u64 >> 30) & 0x1FF`:
- `0xFFFFFFFFC0100000 >> 30 = 0x3FFFFFFFF00`
- `0x3FFFFFFFF00 & 0x1FF = 0x100 = 256`

Wait — that gives PDPT index 256, not 3 or 511!

## ACTUAL FIX NEEDED
The PDPT index computation for `0xFFFFFFFFC0100000` gives 256, not 3.
But the serial log shows `PDPT[511]`. This means the PT dump code has a bug —
OR the raw mapper writes to the WRONG PDPT entry.

Check: does `map_page_raw_chirho` compute PDPT index correctly for this address?

## Files
- `kernel-chirho/src/mm_chirho/pagetable_chirho.rs:1167` — map_page_raw_chirho
- `kernel-chirho/src/subsys_chirho/ko_loader_chirho.rs:432` — init_module_arena_mapping_chirho
- `kernel-chirho/src/subsys_chirho/ko_loader_chirho.rs:3902` — sys_init_module_impl_chirho
