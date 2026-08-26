<!-- For God so loved the world, that he gave his only begotten Son, that whosoever believeth in him should not perish, but have everlasting life. — John 3:16 (KJV) -->

# Lineluya X11 Desktop Completion Evidence Chirho

Status: active and incomplete  
Canonical requirements: `spec-chirho/prd-x11-desktop-reproducibility-chirho.md`

This ledger distinguishes exploratory evidence from acceptance. No run below
is a Gate D or Gate E pass. In particular, a real fatal page fault disqualifies
the most complete run even though its desktop processes later progress.

Raw exploratory artifacts are retained outside Git under
`target/evidence-chirho/x11-desktop-chirho/`. Final cohort artifacts will use a
new immutable cohort directory and a committed trace-free source revision.

## Attempt ledger Chirho

| Attempt Chirho | Effective source Chirho | Artifact hashes Chirho | Runtime Chirho | Measured milestones Chirho | Health Chirho | Framebuffer Chirho | Result Chirho |
|---|---|---|---|---|---|---|---|
| `opaque-transport-120s-chirho` | `28744d0`; VFS launch still synthetic | kernel `a91a58f…`; BIOS `dc92afca…`; rootfs `a7f4d231…`; serial `8083c997…` | native KVM, `qemu64`, 120 s; memory/port were not retained | twm, xterm, and xgears send real X11 11.0 setup requests; Xorg sends Success replies; xterm and xgears send real `CreateWindow`; PTY opens; xgears reports 319.0–521.6 FPS | panic/fault/OOM 0; synthesis markers 0 | not captured | Supporting opaque-transport proof only; not reproducible evidence because runtime metadata and material launcher were incomplete. |
| `material-handover-400s-chirho` | Built while `db1acdd` plus the one-file VFS patch was dirty; byte-identical kernel source later committed as `2deabbb` | kernel `ccd1b31f…`; BIOS `659254c1…`; immutable rootfs and initial scratch `245267fd…`; serial `549f1cbe…` | native KVM, `qemu64`, 1 GiB, 2 vCPU, 400 s, loopback port 2431; harness terminated QEMU at bound | Xorg launch ≤10 s; xkbcomp reap, real wait, and authentic setup ≤20 s; twm ownership and clients ≤70 s; first FPS 85 s; xterm PTY shell marker between 270–280 s | panic 0; fatal page-fault events 1; OOM 0; invalid-context 0; IRQ storm 0; synthesis markers 0 | screenshot `4a417187…` → `8d288e4a…`, but the latter is visually black; direct physical-byte proof remains open | **FAIL:** PID 2 jumped to RIP `0x81ed` and was killed. Functional desktop progress does not override the fatal gate. |

Ellipses above are presentation only. Full hashes for the retained 400-second
attempt are recorded below.

## Retained exploratory attempt: material handover, 400 seconds Chirho

### Reproduction metadata Chirho

- Effective kernel source: `2deabbb997c93fe98089c4ee753c071f9783bd8e`.
  The build preceded the commit, but its only dirty source file was the exact
  VFS patch committed there; local and dlpChirho file hashes matched.
- Rootfs launcher correction source: `db1acdd`.
- Kernel ELF SHA-256:
  `ccd1b31febc7b3a00e7a2faadfe3de3b6f47998875a967baa9f7aa72483f3509`.
- BIOS image SHA-256:
  `659254c1c07757026e481b50fa98b8455f27ef2b5c12c43e201920bda29d4496`.
- Immutable base-rootfs SHA-256:
  `245267fdde2951f9cf73d1376a2d9669bb80c16c4b67361c533c477326e15204`.
- Scratch image: reflink/copy of that base, using the unique pattern
  `/root/alpine-boot-scratch-2431-<runner-pid>-chirho.img`. The exploratory
  harness deleted it after QEMU exit and did not record the expanded PID or
  final hash. That metadata gap is prohibited for the final cohort.
- Serial-log SHA-256:
  `549f1cbe0d3a144e64b6c91186d36eac84c515497ac191ab4486bc74765be2ac`.
- CPU: `qemu64`; acceleration: native KVM; memory: 1 GiB; vCPUs: 2;
  timeout: 400 seconds; host forward:
  `tcp:127.0.0.1:2431-:2222`.
- QEMU used the BIOS image and a unique writable scratch rootfs with
  `-machine q35`, `virtio-blk`, and `virtio-net-pci`. The host harness sent a
  bounded termination at 400 seconds; the guest did not exit itself.

### Exact serial excerpts Chirho

```text
677:[DESKTOP] Xorg launched; waiting for an authentic XCB setup reply
1358:[PF] NULL deref: pid=2 addr=0x81ed rip=0x81ed rsp=0x7ffffeffe870 — killing
2925:[PROCESS] wait4: reaped child PID=22, exit_code=0
3003:[XORG-MAIN-LOOP] PID 11 entered epoll_wait — Xorg ready for clients
3022:[X11-WAIT] PID 12 woke, connect result=0
3193:[DESKTOP] Xorg returned an authentic XCB setup reply
4727:[DESKTOP] twm owns SubstructureRedirect on the root window
4749:[DESKTOP] clients launched: twm=24 xterm=32 xgears=33
6313:[X11-REQ] #285 pid=32 fd=4 opcode=1(CreateWindow) iovcnt=3 bytes=544 written=544
7524:xgears-chirho: 1214.7 FPS (2432 frames in 2.0s)
14672:xgears-chirho: 2504.0 FPS (5008 frames in 2.0s)
15118:[FORK-OK] PID 32 fork #0 → child=34
15274:[XLOG] pid=34 fd=1: '[XTERM-PTY] shell marker chirho'
```

The xterm child opened `/dev/pts/0`, executed `/bin/sh`, emitted the marker,
and executed its final login shell. Its absence in the preceding 180-second run was
timeout censoring, not evidence of a permanent PTY or fork defect.

Source review later found a numeric-PID `WAIT4-FAST` path that can SIGKILL
xkbcomp and fabricate status zero, plus a kernel-preloaded fallback
`/tmp/server-0.xkm`. Neither may remain in the final source. They do not taint
the retained claims above: `WAIT4-FAST` occurred zero times in this log and in
the three other evidence-bearing logs independently checked on 2026-08-25, so
the recorded xkbcomp exit zero and pipe-lifetime result were genuine. That
non-execution was luck from Xorg's observed PID, not acceptable architecture.

### Framebuffer boundary Chirho

- Before screenshot SHA-256:
  `4a41718714d995b2a59c962843f28e75ef60dc8fc2fa9aa62dfd3d97b8f776e4`.
- After-first-FPS screenshot SHA-256:
  `8d288e4ac1176ded154fce5567dcd13dd0b2a047654a6c7b8111b9267700cef2`.
- Visual inspection shows the before image as the green framebuffer console
  and the after image as black. A changed screenshot hash therefore does not
  yet prove visibly mapped client output.
- The repository evidence runner now captures physical framebuffer bytes via
  HMP `pmemsave`, using the physical address and byte length parsed from the
  guest boot log. A bounded 25-second plumbing run on the known-red artifact
  captured physical address `0xfd000000`, exactly 2,764,800 BGR bytes, raw
  SHA-256 `c76f7f43a8bf7647ef4551e7f3f6a128d87d3c26bc6178213893e4bd2c5358b4`,
  and PPM SHA-256
  `4a41718714d995b2a59c962843f28e75ef60dc8fc2fa9aa62dfd3d97b8f776e4`.
  Mechanical conversion visibly reproduces the green Lineluya console and the
  material `[DESKTOP] Xorg launched` line. Its serial SHA-256 is
  `05011285a855c37fc2bd0b9b638d82694a29fa86d20e0d986e74c44e3984a4c3`.
  This proves the capture range and runner plumbing only: the run was dirty,
  stopped at 25 seconds, and correctly failed first at `xkbcomp_exec_chirho`.
  A physical after-frame showing mapped client output remains an open gate.

## Open acceptance work Chirho

- repair and causally prove the PID-2 SIGCHLD/SYSRET return-target fault;
- remove `WAIT4-FAST` plus the kernel-supplied fallback keymap as one coherent
  xkbcomp authenticity repair;
- capture the physical after-frame and verify meaningful pixel changes after
  mapped client frames (the physical before-capture path is runtime-proven);
- remove temporary PID- and X11-specific diagnostic windows;
- restore an executable regression-test gate;
- build a committed trace-free artifact and complete an unselected 5/5 cohort;
- rebuild independently from the same revision and complete the combined v9
  capability proof.
