# For God so loved the world that he gave his only begotten Son,
# that whoever believes in him should not perish but have eternal life. - John 3:16

# Codex Assist Request — Context Switch GPF Bug

## The Problem
After a context switch from a child process (PID 2) back to the parent (PID 0), the parent
crashes with a General Protection Fault. The GPF instruction pointer is always a garbage
address like `0x41fffffd1be80c24`.

## What Works
- `switch_context_chirho` assembly (in `kernel-chirho/src/arch_chirho/context_switch_chirho.rs`)
  saves and restores callee-saved registers correctly
- We verified with raw serial `out` instruction that code executes AFTER switch_context returns
- Physical frames for each task's kernel stack are unique (no overlap)
- Kernel stacks are in the upper half (0xFFFF810000000000, PML4[258]) — shared across all page tables
- PML4 upper-half entries are now copied from boot PML4 (not current CR3)
- 0 exceptions during the switch itself

## What Fails
- The Rust function epilogue of `schedule_chirho()` crashes after switch_context returns
- The GPF shows RSP at `0xffff81000000fc90` (valid stack) but RIP at garbage
- A magic value (0xDEADBEEFCAFEF00D) written to `[RSP-256]` before the switch
  is NOT found after — proving the physical memory backing PID 0's stack was modified
- Tried: CLI/STI instead of without_interrupts, immediate `return`, #[inline(never)]
- All produce the same GPF

## Key Files
- `kernel-chirho/src/sched_chirho/scheduler_chirho.rs` — `schedule_chirho()` function (~line 260)
- `kernel-chirho/src/arch_chirho/context_switch_chirho.rs` — assembly `switch_context_chirho`
- `kernel-chirho/src/sched_chirho/task_chirho.rs` — `allocate_kernel_stack_chirho` (~line 720)
- `kernel-chirho/src/mm_chirho/pagetable_chirho.rs` — `create_user_page_table_chirho` (~line 239)

## The Core Mystery
Physical frames for PID 0's stack (0x126ef000-0x12701000) and PID 2's stack (0x13734000-0x13743000)
are distinct. Yet the magic value test proves the data at PID 0's virtual stack address changes
while PID 2 runs.

Possible theories:
1. The `GLOBAL_MAPPER_CHIRHO` (`OffsetPageTable`) modifies intermediate page table entries
   (PDPT/PD/PT) that are NOT shared between the boot PML4 and per-process PML4s, causing
   the same virtual address to resolve to different physical frames depending on which CR3 is active
2. The `context_ptr_mut_chirho()` function returns a raw pointer from inside a MutexGuard — after
   the guard drops, the data could be modified by another lock acquisition, writing stale context
3. Something about the Rust-generated function frame for `schedule_chirho` doesn't survive the
   round-trip through switch_context (callee-saved registers are fine but stack-spilled locals
   are at wrong offsets after the switch)

## What Codex Should Check
1. In `pagetable_chirho.rs`, trace what happens to PML4[258] when `create_user_page_table_chirho`
   copies from `source_pml4_chirho`. Does it deep-copy the PDPT/PD/PT frames, or share them?
   If shared, do subsequent `map_to()` calls through `GLOBAL_MAPPER_CHIRHO` affect all PTs?

2. In `context_switch_chirho.rs`, verify the assembly: after the switch, RSP points to the
   new task's stack with the return address at [RSP]. Does the `ret` correctly jump to the
   saved RIP? What about the `pushfq/popfq` for RFLAGS — does it modify [RSP-8] on the new stack?

3. In `scheduler_chirho.rs`, check if any code between `switch_context_chirho` and the function
   `return` dereferences pointers that were on the OLD stack frame. The match arms, unsafe blocks,
   and drop calls could access stale locals.

4. Check if `alloc_chirho.allocate_frame()` inside `mapper_chirho.map_to()` at line 758 of
   `task_chirho.rs` could allocate a frame that was already used for PID 0's stack data —
   even though our logging shows no overlap in the explicit data frames, the INTERMEDIATE
   page table frames (allocated internally by map_to) might collide.

## Put Your Analysis Below This Line
---
### Analysis

The strongest explanation is that the original corruption was real stack corruption during
the restore half of `switch_context_chirho`, not a mysterious Rust epilogue bug.

The key detail is in `kernel-chirho/src/arch_chirho/context_switch_chirho.rs:94-109`.
The current assembly now restores `RSP` first and only then restores `RFLAGS`:

- `movq 0(%rsi), %rsp`
- `movq 64(%rsi), %rax`
- `pushq %rax`
- `popfq`

That ordering matters. `pushq` writes to `[rsp-8]`. If `RFLAGS` is restored before
switching `RSP`, the write lands on the OLD task's stack, not the NEW task's stack.
That exactly matches the symptom you recorded:

- PID 0's saved stack contents changed while PID 2 ran
- the resumed PID 0 stack pointer was valid
- the resumed return address / epilogue state was garbage

So the most likely root cause was:

1. save PID 0 context
2. start restoring PID 2
3. `pushq` for `RFLAGS` still uses PID 0's `RSP`
4. PID 0's saved frame gets clobbered at `[RSP-8]`
5. switch back later
6. `schedule_chirho()` resumes on a valid stack page with corrupted saved frame / return state
7. Rust epilogue returns into garbage RIP and faults

### Theory-By-Theory Check

#### 1. PML4 sharing / `GLOBAL_MAPPER_CHIRHO`

This does not look like the primary corruption mechanism anymore, but it was a real secondary
bug source before the current fixes landed.

In `kernel-chirho/src/mm_chirho/pagetable_chirho.rs:255-323`, `create_user_page_table_chirho()`
now copies kernel mappings from the boot PML4, not from the currently active CR3. That is the
right behavior because `GLOBAL_MAPPER_CHIRHO` mutates the boot kernel mapping set over time
(heap, kernel stacks, modules, and other kernel-only mappings).

Important consequence:

- upper-half kernel mappings are intentionally shared
- that sharing is correct
- `map_to()` through `GLOBAL_MAPPER_CHIRHO` should affect the shared kernel half
- the previous failure mode was not "shared kernel mappings are bad"
- the previous failure mode was "some per-process page tables were missing later kernel mappings"

So the page-table side mattered, but mainly as a prerequisite for making kernel stacks and
other kernel mappings visible in every process after CR3 switches.

#### 2. `switch_context_chirho` restore path

This is the main bug.

`kernel-chirho/src/arch_chirho/context_switch_chirho.rs:94-109` already contains the right
fix and even documents the exact failure:

- old code restored `RFLAGS` before `RSP`
- `pushq/popfq` touched the old task's stack
- that corrupted the saved frame

The `ret` logic itself is fine as long as the stack is not corrupted:

- `rip_chirho` is loaded from offset 56
- it is pushed onto the now-restored new stack
- `ret` transfers control to that saved continuation

That matches the intended ABI for both resumed tasks and first-time dispatch.

#### 3. Rust frame / `schedule_chirho()` epilogue

This now looks like a symptom amplifier, not the root cause.

`kernel-chirho/src/sched_chirho/scheduler_chirho.rs:262-268` already avoids
`interrupts::without_interrupts()` because that helper stores state on the caller's stack.
Across a context switch, that is fragile for exactly the kind of cross-stack resume happening
here.

After `switch_context_chirho()` returns in the current `schedule_chirho()`, very little happens:

- `sti`
- normal Rust function epilogue

So if the epilogue faults, the more plausible explanation is that the saved frame was already
corrupted before Rust resumed, not that the compiler built an invalid frame by itself.

#### 4. Intermediate page-table-frame overlap

This now looks unlikely.

In `kernel-chirho/src/sched_chirho/task_chirho.rs:724-760`, kernel stacks are allocated in the
upper half starting at `0xFFFF_8100_0000_0000`. Those mappings are created through the global
kernel mapper and are meant to be shared by all page tables via the copied boot-PML4 entries.

That means the relevant intermediate page tables for kernel-stack mappings are part of the shared
kernel mapping tree, not a private per-process tree that could silently diverge per task after the
fixes above.

If frame reuse were the main bug, I would expect evidence of explicit physical overlap or random
data damage in more than the saved return path. Your observations fit the assembly write-to-old-
stack explanation much more tightly.

### Proposed Fix

The source tree already appears to contain the fix set that best explains the bug:

1. In `kernel-chirho/src/arch_chirho/context_switch_chirho.rs`, restore `RSP` before any
   stack-mutating instruction such as `pushq` / `popfq`.
2. In `kernel-chirho/src/sched_chirho/task_chirho.rs`, keep kernel stacks in the shared
   upper half so every task sees the same kernel stack mappings after CR3 changes.
3. In `kernel-chirho/src/mm_chirho/pagetable_chirho.rs`, copy kernel mappings from the boot
   PML4, not from the currently running process CR3.
4. In `kernel-chirho/src/sched_chirho/scheduler_chirho.rs`, avoid wrappers like
   `without_interrupts()` whose saved state lives on a stack frame that may not be the one
   resumed after the switch.

### Recommended Hardening

I would keep the current fix and add only small hardening around it:

- add a regression comment or test note stating that no instruction that writes to the stack may
  execute between "start restoring new task" and "RSP has been switched to the new task"
- keep `schedule_chirho()` `#[inline(never)]` while this path is being stabilized
- leave the current debug validation of `new_rip_chirho` / `new_rsp_chirho` in place
- if the bug is ever seen again, instrument `[old_rsp-16, old_rsp+16]` before and after the
  switch, because that is exactly where a pre-`RSP` `pushq` bug would show up first

### Bottom Line

This does not read like "Rust locals became invalid after a context switch." It reads like
"assembly wrote to the wrong stack before the stack switch completed."

The current codebase already reflects the correct fix:

- shared upper-half kernel stacks
- boot-PML4-sourced kernel mappings
- `RSP` restored before `RFLAGS`

That combination is the most coherent explanation for both the original GPF and why the current
tree should no longer exhibit that exact corruption pattern.

### Addendum: Save-Side `pushfq` Was Also Dangerous

A later follow-up found that the save half has the same structural hazard as the restore half:
`pushfq` writes to `[rsp-8]`, so using it on the live task stack is only safe if the kernel can
guarantee that nothing meaningful exists below `RSP`.

More importantly, an attempted workaround that set `RSP = &old_context.rflags_chirho` before
`pushfq` was incorrect. Because `pushfq` decrements `RSP` before writing, that sequence stores
the flags value at `old_context.rip_chirho` (offset `56`), not at `old_context.rflags_chirho`
(offset `64`). In other words, it can directly corrupt the saved continuation address.

The correct pattern is:

1. save the live task `RSP` in a caller-saved register
2. point `RSP` at `&context.rflags_chirho + 8`
3. use `pushfq` / `pop`
4. restore the live task `RSP`

That is now what `kernel-chirho/src/arch_chirho/context_switch_chirho.rs` does on both the save
and restore paths, so the flag save/restore no longer writes into either task's live stack and
also no longer risks clobbering `rip_chirho`.
