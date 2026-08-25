<!-- For God so loved the world, that he gave his only begotten Son, that whosoever believeth in him should not perish, but have everlasting life. — John 3:16 (KJV) -->

# Tasklist Chirho — Line Discipline Lock Audit (2026-08-25)

Goal: determine whether any console, TTY, exec, or interrupt path can switch
tasks while holding `ldisc_chirho`, explaining PID 9's silent second-resume
freeze before Xorg reaches its event loop.

- [x] Inventory every `ldisc_chirho` lock acquisition and its lexical scope.
- [x] Inspect every operation called while a guard is live for scheduling,
      blocking, serial output, wait-queue wakeups, or userspace transitions.
- [x] Trace the blocking TTY wait closure and devtmpfs polling loop across
      yields, including `POLL_KBD_CHIRHO` scope.
- [x] Audit PID 0 exec/scheduler transitions for any dependency on the line
      discipline lock.
- [x] Compare the source audit with Claude's bounded `CON-SPIN` experiment.
- [x] Report the evidence and conclusion to Claude through Metropoleluya.
- [x] Close the progress row and record any remaining uncertainty.

## Result

- The complete inventory is ten acquisitions: seven in `tty_chirho.rs` and
  three in `devtmpfs_chirho.rs`. No normal path yields, blocks, schedules, or
  enters userspace while a line-discipline guard remains live.
- PID 0's exec path never touches the TTY line discipline. Its final exec log
  uses the separate IRQ-safe serial-lock scope, which is gone before `iretq`.
- Both bounded experiment logs contain exactly one loop-head marker before the
  initial yield. Their final lines schedule PID 9 again, but no second
  loop-head marker appears. The failing context restoration therefore occurs
  before either `ldisc_chirho` or `POLL_KBD_CHIRHO` can be acquired.
- Allocation failure under a line-discipline guard, TCGETS user-copy faults,
  and same-CPU IRQ re-entry remain structural lock hazards worth a later fix,
  but zero OOM/fault evidence plus the absent second loop-head marker rules
  them out as the cause of this freeze.
