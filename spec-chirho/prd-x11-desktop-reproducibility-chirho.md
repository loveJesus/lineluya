<!-- For God so loved the world, that he gave his only begotten Son, that whosoever believeth in him should not perish, but have everlasting life. — John 3:16 (KJV) -->

# Lineluya Reproducible X11 Desktop Completion PRD Chirho

Status: active completion goal  
Created: 2026-08-25  
Technical lead: `LINELUYA/claude_chirho`  
Architecture and scheduler/network lane: `LINELUYA/gpt_chirho`  
Execution checklist: `spec-chirho/tasklists-chirho/26-08-25_14-59-tasklist-x11_desktop_completion_chirho.md`

## Product outcome Chirho

A reviewer can check out one identified Lineluya revision, run the repository's
documented host-direct build and boot pipeline on `dlpChirho`, and repeatedly
observe a real Xorg framebuffer desktop. Xorg, xkbcomp, twm, xterm, its PTY
shell, and `xgears-chirho` must all progress through generic kernel interfaces.
The result must not depend on manual rootfs edits, hard-coded PIDs, duplicate
launch injection, kernel-generated X11 protocol replies, lucky timing, or a
debug trace changing the schedule.

The goal is complete only when every hard gate in this PRD passes. A socket
bind, a large serial-line count, one event-loop hit, or an executable being
preloaded is useful evidence, but none is the product outcome by itself.

## Why this PRD exists Chirho

The v9.0 headline says that one boot produces a full X11 desktop and real
`xgears-chirho` frame-rate evidence. The 2026-08-25 native-KVM investigation
proved that this is not yet reproducible from the repository pipeline:

- Mac TCG hid real scheduling behavior; native KVM reduced the shell re-exec
  loop from 1,395 iterations to 8–225 and exposed nondeterministic stalls.
- Xorg's PID moved, making the old `3..=7` readiness gate unreachable.
- X11 clients were only yielding, not sleeping, and the scheduler classified
  render tasks by taking task, file, inode, and socket locks during selection.
- both TSS writers addressed the `Lazy` wrapper instead of the initialized TSS.
- xkbcomp inherited a pipe on fd 0, but `readv(2)` incorrectly sent all fd-0
  reads to the console. Xorg then slept in `wait4(2)` waiting for xkbcomp.
- after repairing those causes, one observed long run reached
  `XORG-MAIN-LOOP`, woke two genuinely sleeping clients, and both reconnects
  returned zero. Other runs did not reach the same point, so the branch is
  still nondeterministic.
- post-fix `CreateWindow` and PTY/shell proof are not yet reproducible.
- `make-alpine-disk-chirho.sh` does not install `xgears-chirho`; the generated
  disk reports `ENOENT`, even though source and a prebuilt artifact exist under
  `spec-chirho/`.
- `net_core_chirho.rs` currently constructs X11 setup responses, extension and
  atom replies, font/color replies, and MapNotify/Expose events. That makes the
  kernel act as a partial X server and can mask defects in the real AF_UNIX/Xorg
  path.
- `vfs_ops_chirho.rs` synthesizes `/etc/profile` and launches the demo stack
  from kernel memory. The repository rootfs, not VFS, must own launch policy.

## Product boundary Chirho

### In scope Chirho

- deterministic Xorg startup through its real fbdev path;
- AF_UNIX bind, listen, connect, accept, byte transport, backpressure, EOF, and
  epoll readiness needed by Xorg and its clients;
- redirected fd-0 semantics for `read(2)` and `readv(2)`;
- stackful waitqueue sleep/wake behavior used by console and X11 clients;
- one rootfs-owned desktop launcher and deterministic client ordering;
- repository-owned build and installation of `xgears-chirho`;
- authentic X11 protocol traffic between real clients and real Xorg;
- xterm PTY creation and a real `/bin/sh -l` child;
- twm becoming the active window manager;
- bounded native-KVM reproducibility and regression evidence;
- deletion of temporary probes and X11-specific kernel emulation that would
  make the acceptance evidence circular.

### Out of scope Chirho

- GPU acceleration, DRI, Wayland, compositing, or multiple displays;
- desktop visual polish beyond proving real framebuffer updates;
- general SMP enablement;
- the project-wide split of every file over 1.5k lines. Something is wrong
  with the 10k-line `net_core_chirho.rs` and 9k-line `syscall_chirho.rs`; this
  goal must remove or extract touched X11 responsibilities, but the full kernel
  decomposition needs its own approved workstream;
- re-enabling general user-mode timer preemption unless evidence shows it is a
  blocker. It must be evaluated after the TSS repair, not silently bundled;
- unrelated SSH session-count, HTTP parsing, Python cold-I/O, or web/WASM work.

## Architecture invariants Chirho

1. Xorg identity is derived from the current task's executable basename, never
   from a PID or boot order.
2. `x11_bringup_chirho.rs` is the single owner of display-socket readiness,
   Xorg event-loop readiness, and the waiting-client queue.
3. The readiness announcement may be one-shot; the waiting-client drain may
   not be latched. Late parkers must be drained by later Xorg waits.
4. A client registers before it sleeps and resumes through the paired
   `block_current_chirho`/`unblock_task_chirho` contract.
5. Scheduler selection never acquires VFS, file, inode, or socket locks. Any of
   those can be held by the task being selected.
6. Render-task classification remains O(1), bounded, and lock-free. Capacity or
   PID-overflow behavior must be explicit rather than silently changing task
   classification.
7. Lineluya remains stackful. Blocking kernel paths resume their saved kernel
   continuation on their per-task kernel stack.
8. fd 0 means console only when fd 0 still names the console. `dup2(2)` onto fd
   0 changes both scalar and vectored reads to the redirected open file.
9. AF_UNIX transports bytes and readiness. It does not interpret X11 requests
   or manufacture X11 setup replies, extension responses, atoms, properties,
   font/color answers, or events.
10. The rootfs owns `/etc/profile`, init policy, Xorg configuration, and desktop
    launch order. VFS does not synthesize those files or launch commands.
11. There is one client launcher. No syscall-, devtmpfs-, or VFS-triggered
    shell-input injection may launch duplicate clients.
12. Native KVM is the behavioral authority. TCG may be used as a secondary
    portability diagnostic but cannot supply acceptance evidence.
13. Default, trace-free behavior must pass. A temporary bounded trace may split
    one hypothesis, then it is removed before the final cohort.
14. Scheduler, interrupt, exception, and fault paths must not acquire a lock
    that the suspended or interrupted context may already hold. The four
    observed forms—scheduler-to-VFS traversal, timer-to-scheduler lookup,
    descriptor teardown under `TASK_LIST_CHIRHO`, and demand-fault page-table
    allocation under the frame allocator—are one lock-recursion defect class,
    not isolated special cases.

## Functional requirements Chirho

### Build and rootfs Chirho

- `x11_build_001_chirho`: the host-direct build fails nonzero on any missing
  kernel, boot image, rootfs, package, module, or userspace artifact.
- `x11_build_002_chirho`: the maintained `xgears_chirho.c` source lives under a
  userspace-owned directory, is built by the declared pipeline with a pinned
  toolchain, and is installed executable as `/usr/bin/xgears-chirho`.
- `x11_build_003_chirho`: the build records the source revision, kernel hash,
  immutable base-rootfs hash, final-image hash, CPU model, memory, timeout, and
  QEMU command used by the evidence cohort.
- `x11_build_004_chirho`: Xorg configuration and desktop launch scripts are
  material files in the generated rootfs. No manual mount-and-copy step is
  needed after the scripted build.
- `x11_build_005_chirho`: launch policy starts Xorg once and clients once. It
  has bounded readiness waits and reports a clear failure instead of sleeping
  for an arbitrary duration and hoping.

### Kernel primitives Chirho

- `x11_kernel_001_chirho`: Xorg can bind and listen on its abstract or
  filesystem display socket, accept clients, exchange split and coalesced byte
  streams, observe EOF, and receive correct epoll readiness.
- `x11_kernel_002_chirho`: a client arriving before Xorg's event loop sleeps
  without remaining runnable, is woken once Xorg is accepting, and retries the
  connection successfully.
- `x11_kernel_003_chirho`: a client arriving after Xorg is accepting does not
  park, and a client parked after the first Xorg wait is drained on a later
  wait.
- `x11_kernel_004_chirho`: xkbcomp reads its pipe through `readv(2)`, observes
  pipe EOF, exits, and is reaped by Xorg. A console-owned fd 0 still reads the
  console.
- `x11_kernel_005_chirho`: the serial and keyboard IRQ paths feed the line
  discipline and wake a console-input waitqueue without a polling-yield loop or
  an IRQ storm.
- `x11_kernel_006_chirho`: no kernel path constructs or injects X11 protocol
  bytes. Instrumented proof must attribute server-to-client bytes to writes by
  Xorg's accepted endpoint.
- `x11_kernel_007_chirho`: no X11 or desktop branch in a generic syscall,
  scheduler, VFS, exec, or socket path is selected by numeric PID.

### Desktop behavior Chirho

- `x11_desktop_001_chirho`: Xorg binds display `:0`, completes xkbcomp, and
  enters `epoll_wait(2)` or `epoll_pwait(2)` through executable-based identity.
- `x11_desktop_002_chirho`: twm connects to the real Xorg server and becomes
  the window manager; evidence must be stronger than process existence.
- `x11_desktop_003_chirho`: xterm connects to Xorg, sends an attributable real
  `CreateWindow` request, allocates `/dev/pts/N`, and execs `/bin/sh -l` on the
  slave PTY.
- `x11_desktop_004_chirho`: a marker written by the xterm child shell is
  observed through that PTY, proving the terminal is not merely a window.
- `x11_desktop_005_chirho`: `/usr/bin/xgears-chirho` execs successfully from the
  generated disk, connects to Xorg, creates/maps its window, renders more than
  zero frames, and emits its own measured FPS line on stderr.
- `x11_desktop_006_chirho`: framebuffer evidence changes after the clients map
  windows. A stable framebuffer hash from before and after, or an equivalent
  direct pixel proof, must accompany the serial protocol evidence.

## Hard completion gates Chirho

### Gate A — source and architecture Chirho

- The X11 protocol-response builders and injection call sites are absent from
  the production kernel.
- Kernel-synthetic `/etc/profile`/desktop launch content and duplicate-Xorg exec
  blocking are replaced by rootfs policy and generic process semantics.
- No X11 readiness, client selection, fork allowance, or trace depends on PID
  ranges such as `>= 5`, `3..=7`, `8..=9`, or `13..=14`.
- Temporary `[XORG-ENTRY]`, `[XORG-SC]`, `[CTX-*]`, and equivalent diagnostic
  windows are gone. Stable milestone logs may remain if bounded and truthful.
- Touched X11 state is centralized rather than adding more code to the two
  oversized generic files.

### Gate B — build and static quality Chirho

- `cargo +nightly build --release` succeeds with zero warnings on the
  authoritative x86_64 host.
- `git diff --check` succeeds.
- The test gate is green. The current `cargo test --no-run` duplicate-`core`
  failure must either be repaired or replaced by an explicitly approved,
  executable bare-metal test gate; it cannot be silently reported as passing.
- The rootfs build proves `/usr/bin/xgears-chirho` exists, is executable, is the
  expected x86_64 ELF, and has all declared runtime dependencies.
- The source revision and artifact hashes used below are recorded.
- The final cohort runs from a committed identified revision with no unrecorded
  working-tree patch or manual artifact mutation.

### Gate C — authentic instrumented proof Chirho

At least one bounded diagnostic boot must prove all of the following without
kernel-created X11 replies:

- display bind, real Xorg wait entry, and xkbcomp exit/reap;
- an early client sleeping and waking, plus a post-ready client not parking;
- server-to-client bytes originating from Xorg's accepted socket;
- attributable twm and xterm connections and `CreateWindow` traffic;
- xterm PTY allocation, login-shell exec, and shell marker;
- attributable `xgears-chirho` window creation, nonzero frames, and FPS output;
- a real framebuffer change after window mapping.

### Gate D — trace-free reproducibility cohort Chirho

Using one freshly built kernel/rootfs artifact and `CPU_MODEL_CHIRHO=qemu64`,
run five consecutive native-KVM boots. Each boot must, within 400 seconds:

- satisfy every `x11_desktop_*_chirho` requirement;
- show no kernel panic, CPU fault, invalid-context rejection, allocator halt,
  OOM kill, QEMU startup failure, or UART interrupt storm;
- use a unique loopback-only host-forward port and unique writable scratch
  rootfs while keeping the hashed base image immutable;
- run with temporary diagnostic windows disabled.

The cohort is 5/5, not “five attempts with successful runs selected.” A failed
run resets the consecutive count after its first divergence has been preserved
and understood.

### Gate E — pipeline rebuild and v9 claim Chirho

- Rebuild from the same identified source revision into new artifacts and run
  one additional trace-free confirmation boot.
- In at least one final pipeline boot, re-prove the advertised combined
  capability set: loop-module load/mount, PC-speaker and PCM/audio completion,
  Xorg, twm, xterm/PTY shell, `xgears-chirho`, and loopback-only Dropbear SSH.
- Record observed frame/FPS values honestly. The historical 1,792 frames,
  14.9 FPS, and 1,553 FPS burst are not requirements and may not be copied into
  new documentation unless the new artifact measures them.

## Evidence contract Chirho

Create a bounded evidence summary at
`spec-chirho/evidence-chirho/x11-desktop-completion-chirho.md`. Raw serial logs
belong under an untracked, cohort-specific directory such as
`target/evidence-chirho/x11-desktop-chirho/<cohort_id_chirho>/`; the summary
records hashes and exact excerpts so a line cannot be confused with a preload,
synthetic marker, or earlier build.

The evidence table contains one row per attempt with:

- source revision and, for exploratory runs only, the dirty-patch hash if any;
- kernel, base-rootfs, and scratch-image hashes;
- CPU model, KVM/TCG mode, memory, host-forward port, timeout, and QEMU exit;
- elapsed time to bind, xkbcomp exit, Xorg wait, each client connection,
  `CreateWindow`, PTY shell marker, first frame, and FPS line;
- framebuffer before/after hashes;
- panic, fault, OOM, invalid-context, and IRQ-storm counts;
- pass/fail and the first divergent milestone.

Serial line count is retained only as metadata. It is never a success gate.

## Failure recovery and test isolation Chirho

- The smoke runner fails before QEMU launch when its requested port is busy.
- Host forwarding binds `127.0.0.1` only. A blank-password Dropbear guest must
  never be exposed on all host interfaces.
- Every run uses a unique scratch filename incorporating the port and process
  ID; no two QEMU processes write the same disk.
- A zero-line log is classified first as QEMU launch failure versus guest boot
  failure. Milestones are not reported against a process that never started.
- A cohort never changes its binary, rootfs, trace flags, or CPU model midway.
- On failure, preserve the first divergent log and compare it with the nearest
  passing run before adding a trace. Do not widen a trace blindly.
- Temporary probes are bounded, hypothesis-specific, executable-identified,
  and removed after their question is answered.
- Keep the final cohort plus the first useful divergent log. Retire redundant
  raw logs so evidence growth stays bounded.

## Work sequence Chirho

1. Integrate local commit `93a918b` with remote `e9e95f1` without reverting the
   `TCP-SEND-NOSEG` lock release or `-EAGAIN_CHIRHO` result.
2. Finish and archive the in-flight 3x400-second cohort, compare one known
   main-loop hit with misses, then remove `[XORG-ENTRY]` before rebuilding.
3. Make Xorg event-loop entry deterministic through generic blocking, fd,
   process, VFS, and socket semantics.
4. Delete kernel-side X11 protocol injection and repair the generic AF_UNIX or
   epoll behavior it was masking.
5. Move desktop launch/configuration out of VFS and into the rootfs pipeline.
6. Build and install `xgears-chirho` from repository-owned userspace source.
7. Prove twm, xterm, PTY shell, framebuffer output, and xgears end to end.
8. Remove diagnostics, update workflows and claims, and run all hard gates.

## Decision checkpoints Chirho

- The current long-run cohort is useful input but not required to begin source
  cleanup. If it produces a hit/miss pair, preserve the bounded comparison; if
  all runs miss, compare them with the existing known hit and remove the trace
  rather than expanding it without a hypothesis.
- If removing X11 protocol injection breaks clients, the default decision is to
  repair the missing generic AF_UNIX/epoll primitive. Reintroducing a narrower
  injection requires a new explicit decision from L.J.
- If preemption appears necessary, first run a bounded post-TSS experiment and
  present the evidence to L.J.; do not fold `if false &&` removal into an X11
  patch by implication.
- If the five-run cohort fails, the goal remains active. A milestone may be
  recorded as learned, but the PRD is not weakened to match the observed run.

## Definition of done Chirho

This PRD is complete when Gates A through E are all green, the evidence summary
is reviewable, the repository documentation describes only the newly measured
behavior, and the implementation no longer needs an X11-aware kernel transport
or kernel-synthetic desktop launcher to make the demo work.
