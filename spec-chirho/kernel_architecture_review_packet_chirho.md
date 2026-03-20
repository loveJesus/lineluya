<!-- For God so loved the world that he gave his only begotten Son, that whoever believes in him should not perish but have eternal life. -->

# Kernel Architecture Review Packet Chirho

## Purpose

This document is meant to be shared with Claude, Gemini, GPT-5.4 Pro Extended, or any other strong model as a **review packet** for Lineluya.

The goal is **not** to get ideas for a one-off demo pass.

The goal is to get ideas for turning Lineluya into a **sensibly designed Rust kernel** with:

- real kernel primitives
- clean subsystem boundaries
- minimal demo-specific behavior
- explicit architectural direction
- a clear path to deleting workarounds once the real primitive exists

## How To Use This Packet

Give the model this document and ask for a response with these sections:

1. `What is already architecturally sound`
2. `What is structurally dangerous or fragile`
3. `What should be deleted as temporary glue`
4. `What ideas from Asterinas are worth adopting`
5. `A prioritized 30/60/90 day architecture plan`
6. `A list of invariants that Lineluya should enforce in code, not by convention`

Also ask the model to be **blunt** and to optimize for a real kernel, not for demo momentum.

## Project Snapshot

Project: `Lineluya`

Language: primarily Rust

Branch: `main_chirho`

Goal: Linux-compatible Rust kernel that runs real Alpine userspace programs and converges toward a real kernel architecture rather than a demo-oriented pile of fixes.

Current verified wins include:

- real fork/exec running multiple Alpine programs
- per-process page tables and COW
- real VFS/file-ops structure
- ext4 root + loop mount path
- dynamic musl ELF loading
- Dropbear SSH progressing through real KEX/auth/session paths
- PTY, signals, sockets, fd-table preservation across exec

The project is already beyond “toy kernel” territory. The question is whether the **architecture** is being strengthened as capabilities grow.

## Non-Negotiable Direction

Lineluya should become a kernel with the following properties:

- no workload-specific behavior in generic syscall or scheduler paths
- no protocol-specific shortcuts in VFS, networking, or signal handling
- no “make this one demo work” logic living permanently in core primitives
- no correctness that depends on fragile sequencing spread across unrelated globals
- no pretending a primitive exists when a workaround is really papering over it

If a temporary workaround is added, it must be treated as debt with an explicit deletion target.

## Current Architectural Strengths

These are the parts of the project that feel like a real kernel foundation:

- **Rust-first implementation**
  The project is large enough that Rust is already paying off in maintainability and refactoring safety.

- **Subsystem structure exists**
  There are real modules for scheduling, task state, VFS, networking, exec, signals, paging, PTY, and drivers.

- **Real programs are driving development**
  sqlite3, python3, BusyBox, Dropbear, losetup, mount, and loop.ko exercise real kernel interfaces.

- **The kernel is learning through real semantic bugs**
  The hard bugs now are about signal delivery, wait semantics, fd identity, scheduler order, memory isolation, and close/teardown behavior. That is exactly where real kernels live.

- **The project is already deleting some glue**
  SSH-specific relays and fake transport shortcuts have been removed in favor of proper socket and pipe behavior.

## Current Structural Risks

These are the main architectural concerns.

### 1. Too much policy leaks into primitive paths

Examples:

- scheduler behavior being patched in response to one workload rather than expressed as explicit policy
- exit/teardown logic carrying session-specific assumptions
- fd readiness paths that have historically mixed shell, daemon, socket, and pipe cases too freely

Desired direction:

- generic primitives must not know the application protocol
- workload-specific fixes should be turned into general kernel semantics or deleted

### 2. Blocking semantics are still inconsistent

This is one of the biggest risks.

The kernel has made progress toward waitqueues, but the architecture is still mid-transition:

- some paths properly block and wake
- some still rely on loops or transitional behavior
- signal interruption of blocking syscalls is only now being made real

Desired direction:

- every blocking syscall should either:
  - sleep on a waitqueue and wake for a real condition
  - or return `-EINTR` for a deliverable signal

No hybrid “block, wake, spin, retry, maybe yield” behavior should remain in core APIs.

### 3. FD identity is too brittle

The current socket classification path depends on looking up an fd and inferring socket-ness from inode mode bits. That is a warning sign.

Desired direction:

- open file descriptions should have stronger typed identity
- socket behavior should be routed by object type or file-ops identity, not only by inode metadata checks
- global fallback lookup paths should be narrow and temporary

### 4. Core execution state still depends on global coordination

The project has already been bitten by:

- current-task identity drift
- kernel stack top drift
- CR3 / address-space sequencing
- FS/GS restoration ordering

Desired direction:

- architecture state transitions should be concentrated in a few safe APIs
- “switch task”, “return to user”, “enter signal handler”, and “resume from block” should be explicit, high-integrity boundaries

### 5. Legacy shared-address-space behavior lasted too long

Boot PML4 sharing, late migration, and stack overlap bugs have already shown how expensive it is when process isolation is not authoritative early enough.

Desired direction:

- every user process should have a clear, authoritative address-space owner
- compatibility fallbacks to shared mappings should shrink over time, not expand

### 6. Teardown/close semantics remain under-specified

This is now visible in SSH session close behavior:

- child exits
- parent wakeup order matters
- select/read/EOF/CloseWait interplay matters
- signal handler delivery matters
- acceptability of new listener events depends on who stays alive and blocked where

Desired direction:

- define teardown semantics explicitly for:
  - socket EOF
  - pipe EOF
  - SIGCHLD wakeup
  - child reaping
  - session process lifetime

## Asterinas-Inspired Ideas Worth Adopting

These are design instincts worth borrowing from Asterinas, without copying its codebase or pretending Lineluya should become Asterinas.

Primary reference:

- Asterinas official repository: <https://github.com/asterinas/asterinas>

Useful ideas to borrow:

### 1. Minimize the unsafe trusted core

Asterinas explicitly emphasizes a small and clearly bounded unsafe TCB. Lineluya should do the same.

Desired Lineluya direction:

- track which modules are allowed to manipulate:
  - CR3/page tables
  - raw kernel/user stack transitions
  - MSRs for FS/GS
  - interrupt frames
  - DMA / device memory
- shrink everything else behind safe interfaces

### 2. Separate low-level substrate from policy-heavy kernel services

Asterinas has a clear distinction between lower-level substrate/tooling and higher-level kernel pieces. Lineluya should evolve similarly.

Desired Lineluya split:

- low-level substrate:
  - architecture state transitions
  - page-table primitives
  - interrupt entry/exit
  - waitqueue core
  - typed synchronization
  - low-level device transport
- higher-level services:
  - VFS
  - sockets
  - signals
  - exec/fork
  - PTY/session logic

### 3. Make invariants explicit in types and APIs

Rust is most valuable when invalid states are hard to express.

Desired Lineluya direction:

- stronger typed fd identities
- stronger typed task lifecycle transitions
- stronger typed socket states
- explicit “blocked on queue X” and “woken by reason Y”
- explicit “in signal handler” vs “normal user return”

### 4. Design for verification-friendly boundaries

Asterinas has public verification-oriented efforts around kernel correctness. Lineluya should not wait for formal verification to benefit from the same style.

Desired Lineluya direction:

- isolate modules with crisp contracts
- prefer deterministic helpers over global side effects
- make wake/sleep rules precise
- keep copy_to_user/copy_from_user callsites auditable
- encode scheduler invariants in one place

### 5. Build for deletion of workarounds

Asterinas has a clearer “system design first” posture. Lineluya should adopt that discipline:

- if a workaround exists, document:
  - why it exists
  - what real primitive replaces it
  - which test proves it can be deleted

## What Must Be Deleted Over Time

These categories should shrink steadily:

- any workload-specific logic in generic `select`, `poll`, `read`, `write`, `exit`, or scheduler paths
- any port- or protocol-specific behavior in socket or VFS logic
- any shell/session-specific shortcuts in generic process code
- any “just keep yielding until the right task runs” loops that survive after proper wake semantics exist
- any shared-address-space fallback that remains after per-process PT ownership is real

## Architectural Questions For External Reviewers

Please answer these directly.

### Process / Memory

- What is the cleanest end-state for Lineluya’s address-space model?
- Which lazy migration or shared-page fallbacks should be removed first?
- How should COW, exec, and signal-frame writes interact without surprising hidden aliasing?

### Scheduler / Wait Semantics

- What scheduler invariants should Lineluya enforce explicitly?
- How should blocked syscall wakeup, signal interruption, and parent-child wake ordering be modeled?
- What should be the minimal correct kernel contract for `select`, `poll`, `wait4`, and socket read wakeups?

### FD / VFS / Socket Identity

- How should Lineluya represent sockets, pipes, PTYs, and regular files so dispatch does not depend on brittle metadata inference?
- Should file descriptions carry a stronger typed enum or capability object?
- What is the cleanest migration path from the current inode-mode-based tests?

### Signal Delivery

- What is the minimal but correct signal-delivery model Lineluya should stabilize around first?
- Which pieces can be deferred safely, and which must be first-class immediately?
- How should `-EINTR`, handler delivery, and syscall restart semantics be phased in?

### Network / Session Close

- What close/EOF/session invariants should the socket layer guarantee so userland SSH daemons behave normally?
- Which semantics belong in the socket layer versus in signal and wait semantics?

### Rust Usage

- Where is Lineluya still “writing C in Rust”?
- Which invariants should be moved from comments/debugging into types, traits, or narrower safe APIs?

## What A Good Answer Looks Like

A strong answer should:

- separate **real architectural debt** from **temporary debug scaffolding**
- identify 3–5 system-wide invariants Lineluya should enforce
- recommend where to simplify, not just where to add more code
- describe what to delete, not only what to build
- preserve real Linux-like semantics instead of inventing demo-specific behavior

## My Current Working Thesis

Lineluya is not a toy kernel anymore.

It already has enough real machinery that the right move is **not** to rewrite it from scratch and **not** to keep piling on tactical fixes indefinitely.

The right move is:

1. keep using real workloads to expose semantic bugs
2. aggressively convert those bug fixes into general kernel primitives
3. delete workload-specific glue as soon as the primitive exists
4. make more invariants explicit in Rust APIs and data types
5. narrow the unsafe, global, architecture-critical core

## Requested Output Format For Other Models

Please respond with:

1. `Architecture Grade`
   Give Lineluya a grade as a kernel architecture today and justify it.

2. `Top 7 Architectural Risks`
   Rank them by seriousness, not by convenience.

3. `Keep / Change / Delete`
   Three lists, each with concrete examples.

4. `Asterinas Ideas To Borrow`
   Be specific about which ideas, not just “use Asterinas”.

5. `90-Day Refactoring Plan`
   Organized by week ranges, with clear priorities.

6. `Invariants To Encode In Rust`
   Give specific invariants that should become API/type-level guarantees.

7. `Red Flags`
   Things Lineluya should never normalize as acceptable kernel design.

## Bottom Line

Treat Lineluya as a real kernel project under architectural pressure, not as a demo that happens to boot Alpine.

The right question is not:

- “How do we make this next demo step pass?”

The right question is:

- “What kernel primitive is actually missing, and how do we implement it so the workaround can be deleted?”
