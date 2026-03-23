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

- 15+ SSH commands verified working end-to-end (demo1 items #1-#4 complete, plus real .ko loading):
  - echo, uname -a, id, date, ls /, cat /proc/meminfo, cat /proc/version
  - ls /proc (18 entries), sqlite3 --version, sqlite3 on-disk CREATE/INSERT/SELECT
  - python3 -c 'print(42)' → 42 (needs 90-180s, fresh QEMU)
  - insmod /lib/modules/hello_chirho.ko → INSMOD_OK (real Alpine .ko parsed, relocated, and loaded via SSH)
- 5 consecutive SSH sessions per QEMU instance (8s delays between sessions)
- 6-7 consecutive SSH sessions per QEMU instance with the custom dropbear build
- frame allocator free list infrastructure ready for future COW frame recycling
- proper zombie reaping: wait4 finds zombies, reap_child removes from TASK_LIST
- blocking pipe read: sys_read_real yields+retries for EAGAIN (POSIX correct)
- debug_serial disabled by default: 60KB smaller kernel, 100x less serial output

Current known limits:

- 8th+ SSH session fails: custom-built dropbear with MAX_UNAUTH_PER_IP=100 (static, from source) removed the stock 5-session userland cap and extended the run to 7 sessions. Remaining kernel-shaped limit is childpipe cleanup: parent select does not yet observe childpipe write-end closure authoritatively. Current live hypothesis is exec-time fd-table mirror cloning inflating pipe endpoint counts when temporary clones are dropped without symmetric counter updates.
- python3 module loading ~130s — ext4 cold I/O, 512-entry block cache, yield every 10 in select
- kernel module load via SSH is real, but init_module is still skipped for Alpine .ko files that require R_X86_64_32S relocations against Linux-style negative-half kernel addresses
- remaining demo items: loop mount via SSH and module-init address-space compatibility

## Immediate Iteration

### Goal

Remove the remaining SSH session-limit artifact by making childpipe endpoint lifetime authoritative. Keep python3 startup speed and the remaining demo items secondary unless they expose a missing generic primitive.

### Definition Of Done

- 10+ consecutive SSH sessions succeed without QEMU restart
- parent select/read observes childpipe EOF from authoritative endpoint state
- the fix does not regress the 13 verified SSH commands
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

1. Typed open file descriptions and fd/OFD lifetime cleanup
2. Authoritative teardown, EOF, SIGCHLD, and session lifecycle
3. Unified blocking and signal interruption semantics
4. Authoritative exec/address-space replacement
5. Single authoritative arch-transition boundary
6. Unsafe substrate boundary cleanup

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
