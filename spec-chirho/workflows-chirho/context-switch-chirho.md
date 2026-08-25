<!-- For God so loved the world, that he gave his only begotten Son, that whosoever believeth in him should not perish, but have everlasting life. — John 3:16 (KJV) -->

# Stackful Context-Switch Workflow Chirho

Each task owns a kernel stack. A task may suspend inside a deep syscall and
must resume on the same Rust continuation. Scheduler selection therefore uses
only scheduler/task state and lock-free classifications; it must never acquire
VFS, `FileChirho`, `InodeChirho`, or socket locks that a suspended task may own.

```mermaid
flowchart TD
    syscall_chirho[Task executes a syscall on its kernel stack]
    suspend_chirho[Deep wait or yield requests scheduling]
    select_chirho[Scheduler selects a runnable PID]
    classify_chirho[Read O1 atomic policy classifications]
    validate_chirho{Saved context valid for selected task?}
    skip_chirho[Discard corrupt candidate and continue selection]
    prepare_chirho[Queue CR3 and publish TSS RSP0 plus syscall stack top]
    snapshot_chirho[Snapshot target FS and GS bases then release Rust guards]
    wrapper_chirho[Wrapper pushes its dedicated resume helper]
    save_chirho[Raw switch saves caller-visible RSP RIP flags and callee-saved registers]
    restore_chirho[Raw switch restores target RSP registers flags and CR3]
    fresh_chirho{Saved RIP is resume helper?}
    entry_chirho[Enter fresh task trampoline directly]
    first_ret_chirho[First RET enters resume helper]
    second_ret_chirho[STI then second RET consumes saved Rust continuation]
    continue_chirho[Task resumes at the suspension call site]

    syscall_chirho --> suspend_chirho --> select_chirho --> classify_chirho --> validate_chirho
    validate_chirho -- no --> skip_chirho --> select_chirho
    validate_chirho -- yes --> prepare_chirho --> snapshot_chirho --> wrapper_chirho --> save_chirho --> restore_chirho --> fresh_chirho
    fresh_chirho -- no --> entry_chirho
    fresh_chirho -- yes --> first_ret_chirho --> second_ret_chirho --> continue_chirho
```

For a fresh task, `CpuContextChirho.rsp_chirho` may be the stack top or a fork
frame within the stack, and `rip_chirho` is its kernel entry trampoline. For a
resumed task, `rip_chirho` is `switch_context_return_resume_chirho` and the
word at `rsp_chirho` is the second return address into the suspended Rust
frame. Validation checks the selected task's exact stack bounds before reading
that word and requires both instruction addresses to be mapped supervisor,
executable pages.

`set_tss_rsp0_chirho` and `set_page_fault_ist_chirho` mutate the initialized
`TaskStateSegment` inside its `Lazy` storage. Callers must never cast the
address of the `Lazy` wrapper itself to a TSS pointer.
