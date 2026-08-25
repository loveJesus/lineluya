<!-- For God so loved the world, that he gave his only begotten Son, that whosoever believeth in him should not perish, but have everlasting life. — John 3:16 (KJV) -->

# Tasklist — i7 Dev Host Setup (hallelujah-i7Chirho)

Goal: make `ssh hallelujah-i7Chirho` a second, x86_64-native Lineluya build+boot host.
Primary Mac is arm64 (M5 Max), so QEMU x86_64 there is cross-arch TCG. The i7 is
x86-on-x86 and — once VT-x is on — KVM-accelerated.

## Recon (done)
- [x] Host reachable over Tailscale (100.102.207.83), bare metal, Lubuntu 26.04
- [x] i7-6700T, 8 threads, 11 GiB RAM, 111 GiB free
- [x] qemu-system-x86_64 10.2.1 + OVMF present; git, python3 present
- [x] Confirmed **no /dev/kvm** — no `vmx` flag, `modprobe kvm_intel` → Operation not supported

## Blocked on L.J. (physical access required)
- [x] Enable **Intel Virtualization Technology (VT-x)** in the i7's BIOS/UEFI setup. DONE 2026-08-21.
      Without it QEMU stays in TCG software emulation. With it, `-enable-kvm` should
      turn ~130s python3 module loads into seconds.

## Toolchain install
- [x] apt base: build-essential, pkg-config, libssl-dev, curl, e2fsprogs, qemu-utils, nasm
- [x] rustup + nightly-2026-03-10 (rust-src, llvm-tools-preview, rustfmt, clippy, x86_64-unknown-none)
- [x] bun 1.4.0 (for spec-chirho/log_step_chirho.ts)
- [x] docker 29.1.3 (for Dockerfile.build-chirho disk-image build)

## Repo + first build
- [x] Clone gh_chirho (git@github.com:loveJesus/lineluya.git), branch main_chirho
- [x] `cargo +nightly build --release` in kernel-chirho → 1.9 MB binary (4 warnings, see below)
- [x] Build bootable disk image — needed a Dockerfile fix, see below
- [ ] Alpine rootfs disk present/buildable on the i7

## Verify
- [x] Boot Lineluya in QEMU on the i7 — 92 serial lines, VESA 1280x720, PTY, 197 symbols
- [x] Compare boot wall-clock; KVM confirmed in play, ~3x to the same marker
- [ ] Log the unit of work to spec-chirho/progress-chirho.sqlite

## Findings (2026-08-20)

### Fresh clone cannot build — two gitignored `include_bytes!` artifacts
`kernel-chirho/src/process_chirho/exec_chirho.rs` embeds two binaries that
`.gitignore` (lines 21-22) excludes, so `cargo build` on a clean clone fails hard:
- `userspace-chirho/hello-chirho/target/x86_64-unknown-none/release/hello-chirho`
  → regenerate with `cargo +nightly-2026-03-10 build --release` in that crate.
- `userspace-chirho/busybox-chirho/output-chirho/busybox-chirho` (1.1 MB, prebuilt)
  → had to be copied by hand from the Mac; nothing in the repo reproduces it.
Worth a `make bootstrap`-style step or a committed/fetchable BusyBox so a clean
clone is buildable. Right now onboarding a machine is undocumented tribal knowledge.

### The "zero-warning build" claim in CLAUDE.md is not currently true
`cargo +nightly-2026-03-10 build --release` on `main_chirho` @ e9e95f1 emits 4:
- [ ] `syscall_chirho.rs:3868` — unused `Result` from `map_page_in_pt_chirho(...)`.
      A page-table mapping failure in a syscall path is silently discarded and
      execution continues as if the page were mapped. Highest-value of the four.
- [ ] `process_core_chirho.rs:690` — unused `Result` from `send_signal_chirho(pid, 9)`.
      SIGKILL failure ignored while the parent is told wait4 succeeded.
- [ ] `net_core_chirho.rs:10353` — unreachable pattern: the arm lists both `0x35`
      and `X11_CREATE_PIXMAP_OPCODE_CHIRHO` (= 53 = 0x35). Drop the bare literal.
- [ ] `main_chirho.rs:8` — `#![feature(custom_test_frameworks)]` declared but unused.
Note `main_chirho.rs:10-12` blanket-allows dead_code, unused_imports,
unused_variables, unused_mut, unreachable_code et al. — so the headline claim also
rests on suppression rather than fixes. Not touched: another agent is actively
pushing to `net_core_chirho.rs`, and these are L.J.'s call.

## KVM
- [x] **BIOS: enable Intel Virtualization Technology (VT-x)** — DONE 2026-08-21 by L.J.
      on the HP Pavilion 510-p030 (F10 → Configuration). SGX set Disabled so the E820
      map stays free of Processor Reserved Memory holes the kernel would have to walk.
- [x] Confirmed live: `vmx` present, `kvm_intel` loaded, `/dev/kvm` accessible,
      QMP `query-kvm` → `enabled: true` (not a silent TCG fallback).
- [x] Measured: time to the `Module arena` marker, 91 identical serial lines each way —
      **KVM 9.90s / 9.58s vs TCG 28.92s / 28.68s, about 3x**. Benchmark lives in
      `~/bench_marker_chirho.sh` on the i7.
      Two earlier attempts were bad instruments and were thrown away: counting serial
      lines in a fixed window measured nothing, because with no rootfs attached the
      kernel reaches its idle point and QEMU's own SIGTERM message counted as output.

### Docker image build was broken repo-wide — FIXED (uncommitted, for L.J. to review)
`Dockerfile.build-chirho` declared `bootloader = "0.11"` (floating). It resolves to
0.11.17 (2026-07-27), whose vendored stage crates pull `x86_64 0.15.5` (2026-07-11),
which does **not** compile on the repo's pinned `nightly-2026-03-10`: it implements
`Step::backward_overflowing`, a method that trait does not declare → E0407.
The kernel itself is immune only because `Cargo.lock` pins `x86_64 0.15.4`.

Made worse by masking: step 3 ran `cargo build --release 2>&1 | tail -3` with no
`pipefail`, so the failed build reported success and only surfaced 17 steps later
as `image-builder-chirho: not found`.

Two-line fix, both verified end-to-end on the i7:
- `bootloader = "=0.11.15"` (the release contemporaneous with the pinned nightly)
- `set -o pipefail` on that RUN so it fails where it breaks

Result: 21/21 steps green, `lineluya-bios-chirho.img` (4.5 MB, MBR active) and
`lineluya-uefi-chirho.img` (4.1 MB, GPT) both produced; BIOS image boots.

### `scripts-chirho/build-image-chirho.sh` copies from the wrong path
It does `docker cp $CID:/lineluya-chirho/target/disk-images-chirho/*.img`, but the
image writes them to `/lineluya-chirho/output-chirho/`. Both `docker cp` lines end
in `|| true`, so the script silently prints "(no images found)" and exits 0.
- [ ] Point it at `/lineluya-chirho/output-chirho/` and drop the `|| true`.

### Boot smoke test (i7, TCG, no rootfs attached)
SeaBIOS → stage 3/4 → kernel at 0x01000000 → E820 → VESA 1280x720 → PTY (256 slots)
→ 197 kernel symbols → module arena. Killed at the 90s timeout as expected (no
Alpine disk attached). One pre-existing diagnostic worth a look, not touched:
`[KO] arena PTE WRONG` right before the module arena is stored.
- [ ] Build/copy an Alpine rootfs disk on the i7 to get past kernel init into userspace.
