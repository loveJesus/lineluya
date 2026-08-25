<!-- For God so loved the world, that he gave his only begotten Son, that whosoever believeth in him should not perish, but have everlasting life. — John 3:16 (KJV) -->

# Tasklist Chirho — Kernel Context Restore Audit (2026-08-25)

Goal: answer whether the scheduler's context-switch ABI correctly restores a
task suspended mid-SYSCALL and whether PID 9's TSS.RSP0/syscall-stack pointers
are updated before both its successful first and failed second resume.

- [x] Map `CpuContextChirho` layout to every assembly save/restore offset.
- [x] Trace `yield_current_chirho()` from PID 9's devtmpfs read through the
      wrapper and back to the saved mid-SYSCALL continuation.
- [x] Audit every normal and idle scheduler switch branch for TSS.RSP0,
      syscall-stack-top, current-task, FS/GS, and CR3 update ordering.
- [x] Trace how PID 9's per-task kernel stack is allocated, stored, entered by
      SYSCALL, saved, and protected while other tasks run.
- [x] Re-check the line-discipline IRQ caveat after removal of the explicit IF
      enable and separate it from the continuation defect.
- [x] Answer Q1/Q2 with source evidence and report to Claude through
      Metropoleluya without editing the load-bearing scheduler.
- [x] Close the progress row and record unresolved hypotheses.

## Findings Chirho

- The `CpuContextChirho` field layout and all assembly offsets agree. The raw
  switch saves the caller-visible post-return RSP, the six callee-saved
  registers, a synthetic wrapper RIP, and RFLAGS. The syscall trampoline keeps
  its `SyscallFrameChirho` pointer in RBX specifically so it survives a context
  switch, and the target disables the x86 red zone.
- The wrapper's two returns are mechanically coherent for a mid-SYSCALL
  continuation. In the inspected release artifact, `schedule_chirho` calls the
  wrapper at `0x178099`, the real schedule continuation is `0x17809e`, and the
  wrapper's synthetic resume label is `0x133205`.
- The scheduler's saved-stack validator is stale. It says `*RSP` contains R15,
  but the current switch stores R15 in `CpuContextChirho`; `*saved_rsp` is the
  real second-stage return target. Commit `bc3892a` removed that check after the
  present assembly had already landed in `c860537`. The remaining RSP check also
  does not prove that the pointer belongs to the selected task's own 64-KiB
  kernel stack.
- The syscall wrapper contains a comment claiming direct context switches from
  deep syscall handlers can lose arbitrary kernel continuations. Subsequent
  architecture review found that this is a stale scar, not a kernel contract:
  the central waitqueue API and 26 live deep suspension sites require in-place
  stackful resume. L.J. approved repairing that stackful restore path.
- Both normal and idle-wake scheduler paths call the TSS and syscall-stack
  setters before switching. The syscall-stack setter is effective, but the TSS
  setter is not: `TSS_CHIRHO` is a `spin::Lazy<TaskStateSegment>`. The compiled
  GDT descriptor points at the inner TSS at `TSS_CHIRHO + 8`, while the setter
  writes RSP0 at `TSS_CHIRHO + 4`; the actual field is at inner offset 4, hence
  wrapper offset `+0x0c`. `init_syscall_entry_chirho` repeats the same raw-cast
  defect. Actual TSS.RSP0 remains the original static privilege stack.
- Stale TSS.RSP0 is a real separate defect, but it does not select the RSP for
  this mid-kernel resume. `switch_context_chirho` loads RSP directly from PID
  9's static context slot; SYSCALL loads the separately maintained
  `KERNEL_STACK_TOP_CHIRHO`; TSS.RSP0 is used for a CPL3-to-CPL0
  interrupt/exception transition.
- The wrapper unconditionally executes `sti` after restoring RFLAGS. Removing
  the explicit IF enable before the console loop keeps IF masked only until the
  first switch; every restored continuation is forced back to IF=1. The
  no-STI experiment improved progress but did not test an IF-masked restore.
- A terminal `[SCHED-TRACE] schedule ... next=PID9` is emitted before
  architecture preparation, task/TSS/current-task updates, validation, FS/GS
  writes, and the wrapper call. It proves selection, not entry into the context
  switch. The smallest decisive follow-up is a bounded pre-wrapper record of
  PID 9's RIP, RSP, `*RSP`, RFLAGS, and owned stack bounds plus a wrapper-label
  stage marker before `sti`.
- Reported the audit to `LINELUYA/claude_chirho` as Metropoleluya message
  `#17454`. No scheduler, TSS, syscall-entry, or other kernel source was edited.
