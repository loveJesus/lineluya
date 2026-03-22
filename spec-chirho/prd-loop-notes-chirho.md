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

Current kernel truth (updated 2026-03-22):

- SSH echo, uname, ls, id, date, sqlite3, python3, cat /proc/meminfo ALL WORK end-to-end
- 6+ consecutive SSH connections work without QEMU restart (SLiRP blocker resolved)
- python3 -c 'print(42)' → 42 via SSH
- sqlite3 on-disk DB: CREATE/INSERT/SELECT all work
- sqlite3 --version: 3.51.2 2026-01-09 (64-bit)
- cat /proc/meminfo: MemTotal 294912 kB, MemFree 147456 kB
- root cause FIXED: create_user_page_table copied USER_ACCESSIBLE lower-half PML4 entries from boot PML4
- GLOBAL_MAPPER eliminated from all user-space paths
- blocking pipe read: sys_read_real yields+retries for EAGAIN on blocking pipes (POSIX correct)
- debug_serial disabled by default: 60KB smaller kernel, 100x less serial output

Current kernel blocker:

- 6th+ consecutive SSH session fails: dropbear accepts but doesn't fork due to COW page exhaustion in bump frame allocator (never frees frames). Architecture fix needed: frame freeing or frame reuse.

## Immediate Iteration

### Goal

Fix frame allocator to reclaim COW pages, enabling unlimited consecutive SSH sessions. Verify remaining demo1 items (loop mount, kernel module load).

### Definition Of Done

- 10+ consecutive SSH commands work without QEMU restart
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
