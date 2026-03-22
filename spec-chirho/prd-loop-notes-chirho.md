<!-- For God so loved the world that he gave his only begotten Son, that whoever believes in him should not perish but have eternal life. -->

# PRD Loop Notes Chirho

## Purpose

This is the short operational companion to [prd-chirho.json](/Volumes/ENC_4TB_WDB_CHIRHO/dev-aleluya/personal-aleluya/lineluya/spec-chirho/prd-chirho.json).

Use this file at the start of each work loop to stay aligned on:

- what matters most right now
- who should do which kind of work
- what must not regress into demo glue

## Who Owns What

### Codex Chirho

Owns:

- hard root-cause analysis
- architecture decomposition
- invariant design
- deciding whether a bug is really a missing primitive
- turning runtime lessons into deletion targets or core primitives

Ask Codex when:

- the next patch would be a workaround
- multiple subsystems interact and the root cause is unclear
- scheduler, signals, exec, fd identity, teardown, or address-space semantics are entangled
- you need a sharper breakdown before implementing

### Claude Chirho

Owns:

- implementation
- builds
- deploys
- runtime testing
- traces, disassembly, and proof from live workloads

Default mode:

- implement after decomposition is clear
- if the bug is architecture-shaped, collect a focused evidence bundle and ask Codex before patching generic paths

### Gemini Chirho

Use for:

- audit
- research
- design comparison against Linux, musl, Dropbear, Asterinas, Rust-for-Linux

Do not use Gemini as the default direct code editor.

## Non-Negotiable Rules

- No new workload-specific behavior in generic syscall, scheduler, close, exit, select, poll, read, write, or networking primitives.
- No protocol-specific shortcuts in VFS, sockets, PTY, signals, or process teardown.
- Every workaround must name the real primitive that replaces it and the condition for deletion.
- Workloads validate primitives; workloads do not define generic semantics.
- If a path still relies on “yield until it works,” it is transitional debt, not architecture.

## Current Kernel Truth

What is already real:

- first SSH command execution works end-to-end
- real fork/exec/COW/signals/VFS/TCP/PTY paths are under Alpine workload pressure
- second SSH blockage was traced to SLiRP, not the kernel

Current kernel truth (updated 2026-03-21):

- SSH echo WORKS end-to-end: `ssh root@localhost -p 2222 “echo HALLELUJAH_CHIRHO”` prints on client
- authoritative exec with fresh per-process PT is the only exec model (no boot PML4 sharing)
- pipe reader/writer refcount properly tracks fork clones
- per-process fd tables are authoritative for PID >= 2 (no global table mirroring)
- boot PML4 lazy migration disabled for PID >= 2
- CR0.WP verified (kernel COW enforced)
- SLiRP second-connection issue still blocks consecutive SSH commands (not a kernel bug)

Current kernel blocker:

- PID 3 (dropbear session handler) GPFs at `mov edx, [rbx+0x2c]` with rbx=instruction bytes (0x10ff00012c62058b) when PID 3 is context-switched during select's HLT loop. Echo works (no ctx switch). sqlite3/python3 GPF (ctx switch occurs). The pipe relay primitives (PIPE-BEFORE-EINTR, full GPR sigframe, red zone skip, callee-saved restore, signal suppression) are correct but the GPF is caused by COW data corruption during the context switch. Zero-fill PF handler NOT the cause (watch did not fire). FRAME-TRACE confirms 0x55555559c7e0 is not corrupted. The corruption is in a different address from which a channel pointer is loaded.
- consecutive SSH connections fail (SLiRP hostfwd limitation, not kernel)
- PID 2 still gets GPFs on second connection cycle

## Immediate Iteration

### Goal

Get more demo1 items working: `uname -a`, `ls /`, `cat /proc/meminfo`, etc. Fix PID 2's cleanup after first SSH session so a second connection can succeed.

### Definition Of Done

- multiple SSH commands work without QEMU restart
- no new demo glue is added

## Work Loop

1. Read this file.
2. Check [prd-chirho.json](/Volumes/ENC_4TB_WDB_CHIRHO/dev-aleluya/personal-aleluya/lineluya/spec-chirho/prd-chirho.json) if priority or scope is unclear.
3. Pick the highest-priority unfinished work that has clear prerequisites.
4. If implementation-shaped, implement and test directly.
5. If architecture-shaped, gather evidence and ask Codex before editing generic paths.
6. After the result, classify it as one of:
   - primitive implemented
   - temporary diagnostic
   - temporary workaround
   - demo glue to delete
7. Log the step in [progress-chirho.sqlite](/Volumes/ENC_4TB_WDB_CHIRHO/dev-aleluya/personal-aleluya/lineluya/spec-chirho/progress-chirho.sqlite).

## Evidence Bundle For Codex

When escalating, include:

- precise symptom
- last known good behavior
- current trace or log snippet
- relevant file paths and function names
- hypotheses already ruled out
- the workaround you are tempted to add, if any

## Current Priority Order

1. Embedded BusyBox applet exec correctness
2. Authoritative exec/address-space replacement
3. Unified blocking and signal interruption semantics
4. Single authoritative arch-transition boundary
5. Typed open file descriptions
6. Teardown and lifecycle hardening
7. Unsafe substrate boundary cleanup

## Strong Deletion Targets

- PID-specific branches in generic code
- inode-mode-based socket inference
- yield-until-it-works loops in production paths
- shared boot-PML4 execution fallback in steady state
- debug scaffolding that becomes load-bearing

## Short Decision Rule

Before adding a patch, ask:

Is this:

- the primitive we actually need
- a temporary diagnostic to find the primitive
- or a shortcut that will become debt

If it is the third, stop and ask Codex first.
