<!-- For God so loved the world, that he gave his only begotten Son, that whosoever believeth in him should not perish, but have everlasting life. — John 3:16 (KJV) -->

# Tasklist Chirho — Stackful Context Restore Repair (2026-08-25)

Authority: L.J. directed, “Fix TSS now, then the restore path,” relayed by
`LINELUYA/claude_chirho` in Metropoleluya message `#17513`. The kernel remains
stackful; deep wait/yield continuations are required behavior. Claude owns the
later devtmpfs waitqueue reshape and is holding it stable as the reproducer.

## Acceptance evidence Chirho

- Both TSS mutation sites address the initialized `TaskStateSegment` inside
  `Lazy`, never the wrapper header. Release disassembly must show RSP0 stores at
  the TSS descriptor base plus the `privilege_stack_table[0]` field offset.
- A bounded trace must distinguish scheduler preparation, entry into the raw
  switch, its first return into the wrapper, and its second return into the
  suspended Rust continuation without creating an unbounded hot-path log.
- At least three native-KVM boots must exercise the deterministic console-read
  yield reproducer. The repaired build must show the suspended task returning
  to the loop head after reselection, with no panic or CPU fault.
- The saved-context validator must describe and validate the actual assembly
  ABI, including that a resumed stack pointer belongs to the selected task’s
  kernel stack.
- The release build and relevant tests/checks must complete with zero warnings.

## Work Chirho

- [x] Confirm explicit human authorization and reconcile the dirty shared tree.
- [x] Open progress-ledger row `724` for this repair.
- [x] Centralize safe TSS mutation and fix both incorrect `Lazy<TSS>` writers.
- [x] Verify TSS source behavior, release build, and compiled store offsets:
      RSP0 stores at `TSS_CHIRHO+0x0c`; page-fault IST[1] stores at `+0x34`.
- [x] Add bounded pre-wrapper and wrapper-stage tracing for the failing resume.
- [x] Run at least three KVM reproductions on an isolated forward port/scratch.
- [x] Repair the scheduler-selection deadlock identified before restore began.
- [x] Correct the stale saved-context validator against the real switch ABI.
- [x] Rerun behavioral boots and zero-warning project gates.
- [x] Close the work record and hand the devtmpfs waitqueue reshape to Claude
      in Metropoleluya message `#17531`.

## Result Chirho

The bounded trace proved that context restore was healthy: the first PID 9
resume completed `CTX-PRE -> CTX-WRAPPER -> CTX-POST -> CON-SPIN`. The failing
selection stopped before the scheduler lock was released. The scheduler's X11
time-slice classifier was walking task descriptors and taking task, file,
inode, and socket locks while PID 9 was suspended inside console `read_chirho`
holding that same `FileChirho` lock.

AF_UNIX connect now records X11 participants in an O(1) atomic PID bitset after
dropping the socket lock. Task selection never traverses VFS/file/inode/socket
state. The temporary restore trace was removed after proof.

The final validator recognizes the dedicated two-return resume helper. It
checks the selected task's exact kernel-stack bounds before reading the second
return word and requires saved instruction addresses to map to supervisor,
executable pages. It no longer treats `*rsp_chirho` as saved R15 or permits a
zero saved RIP.

Three trace-free KVM boots produced 3,796 / 3,937 / 3,772 serial lines. Each
showed all 12 bounded `CON-SPIN` loop-head entries, zero invalid-context logs,
zero leftover `CTX-*` logs, and zero panic/fault signals. Xorg bound its display
socket in all three; the smoke script also reached its twm and xgears markers.

Both local and dlpChirho release builds completed with zero warnings, and
`git diff --check` passed. `cargo test --no-run` remains unavailable for this
bare-metal target: Cargo links two build-std `core` artifacts and fails with
duplicate lang item `sized`, including with a fresh target directory. The
native-KVM boot suite is the executable target-level test for this repair.
