<!-- For God so loved the world, that he gave his only begotten Son, that whosoever believeth in him should not perish, but have everlasting life. — John 3:16 (KJV) -->

# Tasklist Chirho — Reproducible X11 Desktop Completion (2026-08-25)

Canonical requirements:
`spec-chirho/prd-x11-desktop-reproducibility-chirho.md`

The checkboxes record completion evidence, not effort. A single lucky boot or
serial marker does not close a phase whose gate requires a cohort.

## Baseline and integration Chirho

- [x] Replace numeric-PID Xorg readiness with executable-basename identity.
- [x] Centralize readiness and the non-latched waiting-client drain in
      `x11_bringup_chirho.rs`.
- [x] Pair AF_UNIX client parking with
      `block_current_chirho`/`unblock_task_chirho`.
- [x] Correct both `Lazy<TSS>` writers and the saved-context validator.
- [x] Remove VFS/file/inode/socket traversal from scheduler task selection.
- [x] Move console input from polling-yield to IRQ-fed waitqueue blocking.
- [x] Route redirected fd-0 `readv(2)` through VFS; prove the old xkbcomp
      pipe/EOF/wait4 deadlock chain.
- [x] Observe one real Xorg wait entry, two sleeping-client wakes, and two
      successful reconnects.
- [x] Carry remote `e9e95f1` lock release and `-EAGAIN_CHIRHO` semantics into
      local kernel commit `93a918b`.
- [x] Rebase/integrate the five local commits onto `gh_chirho/main_chirho`
      without losing either side, then push with a clean owned-path diff.
- [ ] Record the final result of Claude's in-flight 3x400-second cohort.
- [ ] Preserve one bounded hit/miss comparison if available and remove the
      temporary `[XORG-ENTRY]` block before the next build.

## Deterministic server progress Chirho

- [ ] Add a guest regression for pipe-redirected fd-0 `readv(2)` plus a control
      case for genuine console fd 0.
- [x] Delete the parent-PID `3..=7` `WAIT4-FAST` path that SIGKILLs xkbcomp and
      fabricates status zero; retain actual wait/reap semantics for every PID.
- [x] Remove the kernel-preloaded `/tmp/server-0.xkm` fallback and prove the
      keymap consumed by Xorg was produced by the real xkbcomp process.
- [ ] Produce a milestone timeline from Xorg exec through xkbcomp exit/reap,
      display bind, and first epoll wait.
- [ ] Split the first remaining hit/miss divergence with one bounded,
      hypothesis-specific trace.
- [ ] Repair the generic primitive responsible for that divergence.
- [ ] Reach the Xorg event loop in three consecutive exploratory native-KVM
      boots before moving to downstream client work.

## Authentic AF_UNIX and X11 transport Chirho

- [x] Inventory every X11 parser, reply builder, synthetic event, atom table,
      and injection call site in `net_core_chirho.rs`.
- [ ] Add or run generic AF_UNIX regressions for split/coalesced streams,
      backpressure, EOF, accept/connect, and edge-triggered epoll readiness.
- [x] Delete kernel-generated X11 setup, extension, atom, property, font,
      color, MapNotify, Expose, and generic replies.
- [x] Prove all server-to-client X11 bytes originate from Xorg's accepted
      endpoint, not a kernel response helper.
- [x] Remove X11 protocol parsing from the production socket transport after
      the proof trace has served its purpose.

## Rootfs-owned launch and xgears Chirho

- [x] Move the maintained `xgears_chirho.c` source into a userspace-owned
      directory and define its pinned, reproducible build command.
- [x] Install the resulting executable as `/usr/bin/xgears-chirho` through
      `make-alpine-disk-chirho.sh` and validate ELF class, mode, and deps.
- [x] Materialize Xorg configuration and one desktop launcher in the rootfs.
- [x] Remove kernel-synthetic `/etc/profile` desktop content and duplicate
      launch/injection paths.
- [x] Replace the generic exec-time duplicate-Xorg special case with rootfs
      launch ownership and generic process semantics.
- [x] Make launch readiness bounded and explicit rather than timing-based.

## Real desktop proof Chirho

- [x] Prove twm connects to Xorg and becomes the active window manager.
- [x] Prove xterm sends a real attributable `CreateWindow` request.
- [x] Prove xterm allocates `/dev/pts/N`, execs `/bin/sh -l`, and returns a
      shell marker through the PTY.
- [x] Prove `xgears-chirho` execs from the generated disk, creates/maps a
      window, renders nonzero frames, and prints measured FPS.
- [ ] Capture framebuffer before/after evidence showing mapped client output.
- [x] Verify no success evidence came from preload logs, synthetic protocol
      bytes, a manual rootfs edit, or a stale prior image.

## Cleanup and quality gates Chirho

- [x] Remove net-core PID/X11 payload traces, trace-only lock acquisition,
      relay diagnostics, and framebuffer sampling; prove a zero-warning build
      and the same first runtime wall as the untouched baseline.
- [x] Remove scheduler decision/drop traces, syscall-return PID-2 probes, and
      waitqueue PID-2/PID-3 probes; prove a zero-warning build and bounded KVM
      progress to the already-known address-space exhaustion wall.
- [ ] Remove temporary `[XORG-ENTRY]`, `[XORG-SC]`, `[CTX-*]`, PID-specific,
      and equivalent completed diagnostics from the final build.
- [x] Add a preallocated O(1) per-physical-frame ownership table and route
      managed user-PTE publication, fork clone, COW, and unmap through it.
- [x] Add the refcounted address-space handle primitive whose `Drop` performs
      atomic accounting only, plus explicit last-owner retirement that refuses
      the boot root and currently active CR3.
- [x] Execute the host-testable owner/leaf regressions, including the exact
      parent-fork, child-COW, unmap, and last-owner-exit sequence.
- [x] KVM-smoke the MM primitives through the known raw-lifecycle exhaustion
      wall with zero ownership-accounting errors; do not call that wall fixed.
- [x] Replace raw `Option<PhysAddr>` in `TaskChirho`, wire exec/fork/
      `CLONE_VM`/exit/reap to the handle, and prove obsolete roots retire only
      after switching CR3.
- [x] Prove repeated lifecycle cycles reuse page-table and last-reference leaf
      frames without `LeafFrameExhaustedChirho`, double-free, or active-root
      retirement. The bounded discriminator reached 119 genuine reaps without
      those failures; the separate context-slot capacity failure remains red.
- [x] Make render-task registry capacity/overflow behavior explicit while
      retaining O(1), lock-free scheduler lookup.
- [ ] Replace the scheduler's numeric `pid >= 12` SSH/late-child time-slice
      boost with an explicit workload class; do not let a timing policy depend
      on cumulative boot order.
- [x] Update the X11 workflow Mermaid diagram to match the final rootfs,
      socket, waitqueue, and protocol-authenticity paths.
- [ ] Update version/headline documentation to contain only newly measured
      xgears and full-desktop results.
- [ ] Complete a zero-warning pinned-nightly release build.
- [ ] Pass `git diff --check`.
- [ ] Repair the Cargo bare-metal test gate or obtain explicit approval for an
      executable replacement; do not label the current duplicate-`core`
      failure green.

## Newly exposed lifecycle blockers Chirho

- [x] Make `poll(2)` derive readiness from the open object and TTY line
      discipline, not task name, listener PID, or raw UART status.
- [x] Honour `poll(2)` timeouts: a negative timeout never returns zero, zero is
      a single scan, and a positive timeout returns zero only after its real
      tick deadline. Internal scheduler handoff must not leak out as a fake
      userspace timeout. Commit `9893b82` passed a 150-second native-KVM
      discriminator with Xorg main-loop entry, 3,000 xgears frames, max PID 26,
      and zero panic, primary page-fault, invalid-context, or leaf-exhaustion
      markers; the numeric exit workaround remained deliberately in scope.
- [x] Delete both BusyBox re-launch paths after the poll repair: the
      `sys_exit_chirho` post-yield fallback and the `exit_group(2)` PID-under-3
      split. Every user task must use one zombie/SIGCHLD/wait/reap lifecycle.
      Commit `dfaa3c6` also removed the `ppid == 0` SIGCHLD sentinel that had
      stranded the real PID-0 shell. Its frozen 150-second KVM discriminator
      launched exactly one login shell, reaped PID 2 with status zero, reached
      3,000 xgears frames, and emitted zero `[EXIT-INVARIANT]` or fatal markers;
      maximum observed PID was 25, not a context-capacity proof.
- [ ] Obtain L.J.'s explicit process-topology disposition: make the real user
      init PID 1, or deliberately retain a user shell in PID 0. Do not infer
      init/shell authority from a numeric PID threshold.
- [ ] Separate boot-context storage from bounded task-context slots. Allocate,
      validate, and release an owned slot independently of monotonic PID; a
      real PID must never alias the boot slot or index beyond the table.

## Reproducibility and release gates Chirho

- [x] Make the smoke runner fail fast on a busy port or QEMU startup failure.
- [x] Bind every host forward to `127.0.0.1` and use a unique per-run port.
- [x] Use an immutable hashed base rootfs and unique per-run writable scratch.
- [x] Record source, kernel, rootfs, image, configuration, and log hashes.
- [ ] Complete one authentic bounded instrumented proof boot.
- [ ] Complete five consecutive trace-free native-KVM boots from one artifact,
      each satisfying the entire desktop gate within 400 seconds.
- [ ] Rebuild new artifacts from the same source revision and complete one
      additional trace-free confirmation boot.
- [ ] In at least one final pipeline boot, re-prove the advertised loop-module,
      audio, Xorg, twm, xterm/PTY shell, xgears, and loopback-only SSH set.
- [ ] Publish the bounded evidence summary at
      `spec-chirho/evidence-chirho/x11-desktop-completion-chirho.md`.
- [ ] Close progress records and mark the active goal complete only after every
      hard PRD gate above is green.
