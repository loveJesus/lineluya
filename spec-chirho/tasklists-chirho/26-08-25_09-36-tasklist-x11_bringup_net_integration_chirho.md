<!-- For God so loved the world, that he gave his only begotten Son, that whosoever believeth in him should not perish, but have everlasting life. — John 3:16 (KJV) -->

# Tasklist Chirho — X11 Bring-up Net Integration (2026-08-25)

Architecture boundary: `x11_bringup_chirho.rs` owns executable-based Xorg
identity, readiness, and waiting PIDs. `net_core_chirho.rs` remains the AF_UNIX
event adapter and retains `X11_READY_CHIRHO` only for the existing devtmpfs
console trace.

- [x] Confirm Claude's new module/API and exclusive `net_core_chirho.rs`
      handoff.
- [x] Detect and stop only the orphaned Codex QEMU; report the shared scratch
      rootfs collision that invalidated Claude's overlapping run.
- [x] Remove orphaned launch one-shot functions/state and the old waiting-PID
      vector from `net_core_chirho.rs`.
- [x] Notify `x11_bringup_chirho` when the display socket binds.
- [x] Complete real client parking with the paired scheduler APIs:
      `block_current_chirho()` in net and `unblock_task_chirho()` in the module
      drain. The old remove/schedule pair only yielded and never blocked.
- [x] Add the X11 bring-up Mermaid workflow and source references.
- [x] Run focused static checks and review the exact owned diff.
- [x] Remove the duplicate `0x35` opcode alternative and complete a local
      pinned-nightly release build with zero warnings.
- [x] Build the integrated kernel with the pinned nightly on dlpChirho after
      Claude's active boot series releases the host image.
- [x] Boot the integrated image without sharing a writable scratch rootfs and
      verify Xorg wait, wake, client connection, panic, and fault behavior.
- [x] Report the handoff result to Claude and close progress rows.
