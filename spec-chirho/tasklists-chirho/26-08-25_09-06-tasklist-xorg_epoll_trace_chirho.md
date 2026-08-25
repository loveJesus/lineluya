<!-- For God so loved the world, that he gave his only begotten Son, that whosoever believeth in him should not perish, but have everlasting life. — John 3:16 (KJV) -->

# Tasklist Chirho — Xorg Epoll Trace (2026-08-25)

Goal: determine whether Xorg ever reaches an epoll syscall during the
non-deterministic native-KVM desktop boot, without changing the held X11
readiness/wake architecture or flooding every later syscall onto serial.

- [x] Inspect the 601-line `net_core_chirho.rs` working diff and confirm it
      does not touch the X11 connect-retry/park block.
- [x] Accept Claude's hold on the proposed X11 pre-park behavior change.
- [x] Let epoll create/control/wait calls bypass the 100-entry general
      `[XORG-SC]` trace ceiling.
- [x] Build the diagnostic kernel and boot image on dlpChirho.
- [x] Run three control boots and discover that the inherited `pid >= 5`
      trace predicate excluded Xorg: Xorg was PID 2 in all three, while PID 9
      was `mpg123`.
- [x] Select the traced process by `exe_path_chirho` basename `Xorg`.
- [x] Rebuild the corrected diagnostic kernel and boot image on dlpChirho.
- [x] Run at least three 300-second KVM boots with `CPU_MODEL_CHIRHO=qemu64`.
- [x] Inspect each complete serial log for exe-identified Xorg epoll syscalls, panics,
      faults, X11 progress, and the terminal boot state.
- [x] Report the evidence and ownership answer to Claude in
      `xorg-wake-chirho`.

Architecture review resolved into the single-owner `x11_bringup_chirho`
module. Its net integration is tracked separately in the 09:36 tasklist.

## Evidence learned before the corrected rerun

- The two earlier successful 300-second logs show Xorg PID 2 returning
  `epoll_create1` (291) once and `epoll_ctl` (233) four times after its exec.
  Neither wait syscall returned, which is consistent with the first wait
  blocking.
- `[XORG-SC]` runs after syscall dispatch, so it cannot prove that a blocking
  wait was never entered. The new X11 bring-up hook runs before the wait and is
  the authoritative entry marker.
- The first corrected rerun wedged before Xorg, then its smoke script was
  replaced during execution and left an orphaned QEMU. That run was stopped
  and is not evidence. A later overlapping run from another port shared the
  same writable scratch rootfs and is invalid too.

## Corrected integrated rerun result

- Three isolated 300-second KVM boots ended at 847, 1,149, and 861 serial
  lines, with zero panics, faults, `XORG-MAIN-LOOP`, `XORG-WAKE`, or
  `CreateWindow` markers.
- Run 2 reached both X11 binds. Exe-identified Xorg returned `epoll_create1`
  once at trace index 228 and `epoll_ctl` four times at indices 267-270,
  proving the epoll bypass retains events beyond the old 0-99 ceiling.
- Neither wait syscall entered: return traces for 232/281 and the entry-side
  main-loop hook were all absent. Every boot instead stopped immediately after
  scheduling PID 9 for its second pass through the console-read wait loop.
