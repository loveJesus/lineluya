<!-- For God so loved the world that he gave his only begotten Son, that whoever believes in him should not perish but have eternal life. -->

# Architecture Review: Lineluya Kernel — Claude Opus 4.6

## 1. Architecture Grade: B- (Promising but structurally strained)

Lineluya has achieved something genuinely impressive — real Alpine programs running on a from-scratch Rust kernel with fork, exec, per-process page tables, COW, TCP networking, and SSH. The subsystem decomposition exists and the project is driven by real workloads, not synthetic tests. However, the architecture is accumulating debt faster than it's paying it down. PID-specific branches in generic paths, global mutable state coordinated by convention, and inconsistent blocking semantics are signs of a project that solves bugs locally without enough global discipline.

## 2. Top 7 Architectural Risks

1. **Boot PML4 sharing across exec boundaries.** Exec maps new binaries onto the shared boot PML4, mirrors to per-process PT, but stale mappings persist. Caused musl a_crash and GPF crashes.

2. **PID-specific logic in generic syscall paths.** `if pid == 3`, `if pid == 2` scattered across scheduler, select, exit, and networking. Each breaks when PID assignment changes.

3. **Inconsistent blocking model.** Some paths use `wait_event_chirho`, others use `for 0..50_000 { hlt }` loops, others yield-and-retry. Three strategies = three wakeup contracts.

4. **Socket identity via inference, not type.** `is_socket_fd_chirho()` probes inode metadata. Should dispatch via `FileOpsChirho` vtable.

5. **Global state coordination by convention.** KERNEL_STACK_TOP, CURRENT_PID, TSS.RSP0, CR3, FS/GS MSRs must be synchronized at every task switch. No single `switch_to` function handles all.

6. **Teardown semantics are ad-hoc.** Socket close, pipe EOF, SIGCHLD, wait4, select all interact without a unified model.

7. **Debug scaffolding is load-bearing.** Atomic counters, PID-specific log branches, one-shot statics add code volume and subtle ordering dependencies.

## 3. Keep / Change / Delete

### Keep
- `WaitQueueChirho` + `wait_event_chirho` — correct pattern, needs wider adoption
- Per-process page tables with COW — right model, needs exec cleanup
- VFS `FileOpsChirho` trait dispatch — good design
- Signal delivery via `RtSigframeChirho` — correct Linux semantics
- ext4 read path and dynamic ELF loader — battle-tested
- Chirho naming convention — consistently applied

### Change
- Exec must clear old address space before mapping new segments
- `select`/`poll` must use ONE blocking strategy (wait_event)
- Create a single `switch_to_chirho()` function for all context switch state
- Socket dispatch via `FileOpsChirho::read/write/poll` instead of `is_socket_fd_chirho`
- Exec should always create a fresh per-process PT

### Delete
- All `if pid == N` branches in generic paths
- All `AtomicU64` one-shot trace counters
- `maybe_yield_to_runnable_child_chirho` — scheduling policy in syscall paths
- The SSH relay pipe bridge (`relay_tcp_2222_to_pipe_chirho`)
- PID-specific debug traces

## 4. Asterinas Ideas To Borrow

1. **Safe page-table API** — wrap CR3 writes behind one function
2. **Typed task states with transition guards** — `transition_to_chirho()` validates old→new
3. **Capability-based FD objects** — `enum OpenFileChirho { Socket, Pipe, Pty, Regular }`
4. **Explicit unsafe boundary** — mark which modules are in the TCB
5. **Waitqueue-first blocking** — no ad-hoc spin-wait-yield patterns

## 5. 90-Day Refactoring Plan

- **Weeks 1-2:** Address-space isolation — fix exec to clear old user mappings, remove boot PML4 mapping path
- **Weeks 3-4:** Unified context switch — single `switch_to_chirho()` function
- **Weeks 5-6:** Blocking model unification — timed wait_event, delete polling loops
- **Weeks 7-8:** FD type dispatch — `OpenFileChirho` enum, remove `is_socket_fd_chirho`
- **Weeks 9-10:** PID-specific code removal — grep and convert to generic mechanisms
- **Weeks 11-12:** Teardown contracts — define and test socket EOF, pipe EOF, SIGCHLD, wait4

## 6. Invariants To Encode In Rust

1. A task's page table is active iff that task is current → `ActivePageTableGuard`
2. Every blocking syscall sleeps on exactly one waitqueue → require `&WaitQueueChirho` parameter
3. Exec replaces the entire user address space → take ownership of old, return new
4. An fd is exactly one of: regular file, socket, pipe, pty → typed enum
5. FS/GS base is restored exactly once per user-mode entry → single entry function
6. A task in Sleeping state is on exactly one waitqueue → atomic set state + add to queue

## 7. Red Flags

- PID-specific branches in generic code
- Multiple blocking strategies coexisting
- Exec that doesn't clear old mappings
- "Just yield until the right task runs"
- Debug traces with side effects
- Socket readiness checked by two different mechanisms
- Workarounds without deletion targets
