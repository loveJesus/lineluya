<!-- For God so loved the world, that he gave his only begotten Son, that whosoever believeth in him should not perish, but have everlasting life. — John 3:16 (KJV) -->

# Tasklist Chirho — Alpine Disk Regression + Kernel Warnings (2026-08-24)

Goal: continue development while `hallelujah-i7Chirho` is offline, and make the
Alpine rootfs disk reproducible on **either** host (Mac or i7).

## Findings

- [x] `make-alpine-disk-chirho.sh` had a silent Linux/macOS split: the Linux
      native loop-mount path extracted the **bare minirootfs only** — no
      sqlite3, python3, dropbear, xorg-server, xterm, twm, mesa. Only the
      macOS Docker path installed them. The i7 would have built a useless disk.
- [x] `alpine:latest` drifted past 3.21 to **apk-tools 3.x**, which writes
      **zero-length files** for `apk --root` installs. The first rebuild looked
      green (`sqlite3 INSTALLED`, 505 entries in `/usr/bin`) while every
      apk-installed binary was 0 bytes and the filesystem held 1.8 MB.
      Proof: old March image = 65 MB used, real binaries; alpine:3.21 ships
      apk-tools 2.14.6 and extracts correctly.
- [x] The `ls`-based verification passed on zero-byte files — it is what let
      the broken disk be reported as good.
- [x] `--size` was ignored: the container hardcoded `truncate -s 512M`.
- [x] The host ext4 formatter was a hard requirement even on the Docker path
      (the Mac only passed by accident, via Android SDK's `mke2fs`).

## Fixes landed (`scripts-chirho/make-alpine-disk-chirho.sh`)

- [x] Docker path is now the canonical path on **every** host; loop mount is a
      fallback that warns loudly about the missing package set.
- [x] `populate_image_darwin_chirho` → `populate_image_docker_chirho`.
- [x] Builder container pinned to `alpine:${ALPINE_VERSION_CHIRHO}` (3.21), so
      minirootfs and packages share one branch and one musl ABI.
- [x] `--size` wired through to the container (`DISK_SIZE_INNER_CHIRHO`).
- [x] Host ext4 formatter no longer required when Docker is available.
- [x] Verification hardened: every key binary must exist **and be non-empty**,
      or the build fails instead of shipping the disk.
- [x] Summary text now names the real package set and its Docker-only caveat.

## Kernel warnings (was 4, now 1)

- [x] `syscall_chirho.rs` brk path: `map_page_in_pt_chirho` result was dropped —
      brk could extend past an unmapped page and hand userspace an address that
      faults on first touch. Now stops at the old break, matching the
      frame-exhaustion path.
- [x] `process_core_chirho.rs` WAIT4-FAST: dropped `send_signal_chirho` SIGKILL
      result. Now logs a failed kill (the fake-success return is deliberate).
- [x] `main_chirho.rs`: removed unused `#![feature(custom_test_frameworks)]`
      (no `test_runner` attribute anywhere in the crate).
- [ ] `net_core_chirho.rs:10527` unreachable pattern (`X11_CREATE_PIXMAP_OPCODE_CHIRHO`
      == 53 == `0x35`, already listed). **Left alone** — another agent holds
      601 lines of uncommitted work in that file.

## Two more failures found while verifying the fix

- [x] The container build script was **silently truncated**. It was passed as a
      single-quoted argument to `sh -c`, and the apostrophes in the sample SQL
      (`VALUES (1, 'Hallelujah! SQLite runs on Lineluya!')`) closed that quote.
      Everything after it — including the final `sync` and `umount` — was parsed
      as stray arguments and never ran. Result: the image was copied out with an
      unflushed journal (`needs_recovery` set, which the known-good March image
      does not have) and without `/root/hello_chirho.py` or `test_chirho.sql`.
      **Fixed structurally**: the inner script is now written to a file and
      `docker cp`'d in, so there is no nested quoting left to break.
- [x] The `needs_recovery` gate I added could not fail — `dumpe2fs` lives in
      `e2fsprogs-extra`, which the container did not install, so `grep` simply
      found nothing. Now `e2fsprogs-extra` is installed and a missing `dumpe2fs`
      is itself fatal. Re-ran the build to prove the gate executes.

## Verification

- [x] Kernel builds: `target/x86_64-unknown-none/release/kernel-chirho`.
- [x] Disk built and gated: Xorg 1900792 B, xterm 644152 B, twm 152112 B,
      dropbear 265888 B, sqlite3 1945920 B, mpg123 127144 B, busybox 808712 B.
      410 MB used (45%), 127 packages, journal clean, feature set now identical
      to the known-good March image.
- [x] Bootable images via `Dockerfile.build-chirho`: BIOS 4.7 MB (MBR, active),
      UEFI 4.3 MB (GPT). Extracted with the corrected `build-image-chirho.sh`
      path (`/lineluya-chirho/output-chirho`).
- [x] **Booted the new disk end-to-end** (QEMU TCG on the arm64 Mac, 10066
      serial lines, 0 panics/faults). Reached: module arena → ext4 root inode →
      boot beep → DHCP `IP=10.0.2.15` → `insmod`/`losetup` (`loop0 LOOP_CONFIGURE
      bound successfully`) → dropbear `PID 7 called listen — marked as daemon` →
      Xorg bound `@/tmp/.X11-unix/X0` with clients connecting over AF_UNIX.
      Programs actually exec'd from the disk: Xorg, xterm, twm, xkbcomp, mpg123,
      dropbear, losetup, insmod, /bin/sh, ld-musl (x9).
- [ ] **Not** observed this run: `XORG-MAIN-LOOP`, window creation, xgears
      frames. The boot settles into the known shell re-exec loop (1395
      `Jumping to userspace`), which under cross-arch TCG plausibly starves the
      X path. Needs a KVM run on the i7 to say anything more.
- [ ] Re-run the whole flow on the i7 once it is back online.

## Notes

- Dropbear host keys are absent from `/etc/dropbear` and the boot logs
  `resolve_path FAILED ... err=-2` for all three. **Not a regression** — the
  known-good March image has the same empty directory; `dropbear -R` generates
  them at runtime, and the daemon reaches `listen` either way.
- `hallelujah-i7Chirho` went offline (suspended) at the start of this session
  and stayed unreachable; all of the above was done on the Mac instead.

## Still open (reported, not acted on)

- [ ] `scripts-chirho/build-image-chirho.sh` copies from the wrong container
      path under `|| true`, so it silently produces nothing.
- [ ] A fresh clone cannot build: `include_bytes!` needs two gitignored
      binaries (`busybox-chirho`, `hello-chirho`) with no documented bootstrap.
- [ ] `[KO] arena PTE WRONG` appears in the boot log before the module arena
      is stored.
