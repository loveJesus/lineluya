<!-- For God so loved the world that he gave his only begotten Son, that whoever believes in him should not perish but have eternal life. -->

# Kernel Architecture Synthesis Chirho

## Purpose

This document compiles architecture feedback for Lineluya from multiple sources so the project can converge toward a **real kernel architecture** instead of drifting into demo-only fixes.

This is a living synthesis document. New review papers can be appended and compared here.

## Inputs So Far

### Codex Chirho

Core position:

- the architecture is viable
- the kernel is already beyond toy status
- the main risk is not lack of features but too much policy leaking into primitives
- the correct direction is to convert workload-specific bug fixes into general kernel primitives and delete the workaround afterward

Main concerns:

- inconsistent blocking semantics
- brittle fd identity
- too much global execution-state coordination
- lingering shared-address-space fallbacks
- teardown/close semantics not yet authoritative

### Gemini Chirho

Grade:

- `C+`

Main message:

- Lineluya has crossed into real-kernel territory
- but it still has too much “demo-driven connective tissue”
- the biggest structural problems are sleep/wake semantics, address-space ownership, brittle fd typing, and global coordination

Gemini’s strongest recommendations:

- move to stronger typed file/fd identity
- move to a stricter context-switch substrate
- unify blocking around waitqueues
- split a small unsafe substrate from higher-level kernel services

### Grok Chirho

Grade:

- `B-`

Main message:

- the project already has the hard parts in place
- the next step is not adding more features, but making the primitives authoritative
- treat the current stage as the “mid-life crisis” of a kernel: either fix the foundations now or compound debt permanently

Grok’s strongest recommendations:

- make every blocking syscall either sleep on a real waitqueue or return `-EINTR`
- replace inode-mode-based fd dispatch with typed file descriptions
- separate scheduler mechanism from scheduler policy
- delete shared-PML4 and yield-loop fallbacks on a schedule

### GPT-5.4 Pro Extended Chirho

Grade:

- `B-`

Main message:

- Lineluya is already a real kernel project
- the right move is not a rewrite
- the right move is to harden a small set of primitives and delete the glue aggressively

GPT-5.4 Pro Extended’s strongest recommendations:

- make architecture-critical state transitions authoritative and concentrated
- force all blocking into one explicit contract: waitqueue sleep, successful wake, or `-EINTR`
- define typed open file descriptions instead of inode-mode inference
- split a small unsafe substrate from higher-level kernel services in an Asterinas-like way
- treat teardown and close ordering as a first-class architectural problem

### Claude Chirho

Grade:

- `B-`

Main message:

- Lineluya is impressive and real, but structurally strained
- the main debt is not feature absence but local bug-fix accumulation without
  enough global discipline
- PID-specific branches, shared boot-PML4 exec behavior, and multiple blocking
  strategies are the clearest signs of that strain

Claude’s strongest recommendations:

- make exec replace the old address space authoritatively instead of mapping
  through shared boot-PML4 fallbacks
- remove PID-specific logic from generic syscall, scheduler, select, exit, and
  networking paths
- unify all blocking around `wait_event_chirho` plus `-EINTR`
- create one `switch_to_chirho()` boundary for current-task, kernel stack,
  CR3, and FS/GS state
- route socket behavior through typed file dispatch rather than inode-mode
  inference

## Consensus

There is already strong agreement across Codex, Gemini, Grok, GPT-5.4 Pro
Extended, and Claude.

### 1. Lineluya is real enough to deserve architecture-first decisions

All current reviewers agree on this.

This is not a toy kernel anymore. Real Alpine workloads are already forcing semantic correctness problems in:

- fork/exec/COW
- signal delivery
- blocking semantics
- socket teardown
- fd preservation and identity
- PTY/session behavior

That means the project should now optimize for **authoritative primitives**, not just the next passing demo.

### 2. Blocking semantics are the biggest cross-cutting weakness

This is the clearest point of agreement.

The kernel still mixes:

- real waitqueues
- bounded polling loops
- yield loops
- partially interruptible blocking paths

This creates scheduler pressure, signal-delivery edge cases, and teardown bugs that look unrelated but are not.

Consensus direction:

- every blocking syscall should either:
  - sleep on a waitqueue and wake for a real event
  - or return `-EINTR` for a deliverable signal

No hybrid production behavior should remain once the proper primitive exists.

### 3. FD identity must stop depending on inode-mode inference

This is another strong consensus point.

The current style of deciding socket/pipe/file behavior by doing:

- `lookup_fd`
- inspect inode metadata
- branch on mode bits

is too brittle for a kernel that already runs real daemons.

Consensus direction:

- move toward typed file descriptions
- dispatch by object type or file description kind
- make global fallback lookup paths narrower over time

### 4. Global execution-state choreography is still too fragile

All reviewers converged on this even if they phrase it differently.

The kernel has already paid for mistakes in:

- `current_task`
- kernel stack top
- CR3 sequencing
- FS/GS restoration
- interrupt/syscall/user-return boundaries

Consensus direction:

- concentrate these transitions into a small number of explicit APIs
- reduce the number of modules allowed to manipulate arch-critical state
- define one authoritative arch-transition boundary for:
  - current-task installation
  - kernel stack selection
  - CR3 switching
  - FS/GS restore
  - signal entry/return
  - return to userspace

Claude’s review sharpened this further:

- one explicit `switch_to_chirho()`-style boundary should own all of that state
- no scheduler branch, idle wake path, or resume path should partially recreate
  the same transition logic

### 5. Workload-specific glue must be treated as debt, not architecture

All reviewers agree that demo glue is the wrong long-term direction.

The project is healthiest when:

- a bug in Dropbear or python3 becomes a missing primitive
- the primitive gets implemented
- the workaround gets deleted

The project becomes unhealthy when:

- the workaround stays
- the primitive remains fuzzy
- later work depends on the workaround being there

### 6. Exec must own full address-space replacement

Claude’s review adds a concrete architectural warning that fits the broader
consensus well:

- exec through shared boot-PML4 staging stayed around too long
- stale mappings and transitional mirroring logic are too dangerous in steady
  state

Consensus direction:

- exec should clear and replace the old user mappings authoritatively
- boot-PML4 execution should shrink to a tightly bounded bootstrap role only

## Ranked Architectural Risks Chirho

This section normalizes the strongest recurring critiques into one ranked list.

### 1. Architecture-critical state transitions are too diffuse

The same class of bug keeps reappearing under different names:

- `current_task` drift
- kernel stack top drift
- CR3 sequencing mistakes
- FS/GS restore ordering bugs
- inconsistent syscall vs interrupt return paths

This is the most serious structural risk because it means correctness still
depends on choreography across globals instead of one authoritative transition
boundary.

### 2. Blocking, wakeup, and signal interruption are still hybrid

The reviewers converge on this point most strongly.

The kernel still mixes:

- waitqueues
- bounded polling loops
- yield/retry loops
- partial `-EINTR` behavior

That makes `select`, `poll`, `wait4`, socket reads, pipe reads, and signal
delivery harder to reason about than they should be.

### 3. Address-space ownership is not authoritative enough yet

Even after major progress on per-process page tables, the review consensus is
that transitional shared-space behavior stayed alive too long.

The architectural goal is simple:

- every normal user task owns one authoritative address space
- all user-memory mutation routes through that authority
- no late shared-PML4 fallback remains in steady-state execution

Claude’s review is the strongest concrete expression of this risk so far:

- boot-PML4 sharing across exec boundaries is itself a top-tier architecture
  smell, not just an implementation bug

### 4. Lifetime and teardown semantics are still under-modeled

The SSH teardown issues are only the canary.

The deeper problem is that the kernel still needs stronger ordering around:

- child exit publication
- `SIGCHLD`
- EOF and half-close
- last-reader / last-writer behavior
- listener/session lifetime

### 5. FD identity is still too weak

The project is still paying for “what kind of object is this fd?” being a
runtime inference problem instead of a typed description problem.

### 6. Policy still leaks into primitives

Any time generic code in scheduling, blocking, close, or readiness paths starts
to absorb workload-specific fixes, the architecture gets weaker.

Claude highlighted the sharpest example here:

- `if pid == N` logic in generic scheduler, syscall, and teardown paths must be
  treated as emergency debt and removed on a schedule

### 7. Too many invariants remain social instead of mechanical

Rust is helping with memory safety, but not enough invariants are enforced by:

- types
- narrow APIs
- ownership boundaries
- state-machine transitions

That leaves too much correctness in the realm of “remember to do this in the
right order.”

## Structural Diagnosis

### What Is Already Architecturally Sound

- Rust-first kernel implementation
- real subsystem structure
- real workloads driving semantics
- VFS/file-ops layering as a genuine architectural backbone
- real process/address-space work rather than a fake shell demo

### What Is Not Yet Structurally Sound

- sleep/wake semantics are still transitional
- fd/file/socket identity is too weak
- teardown semantics are under-modeled
- architecture-critical state is too globally coordinated
- legacy shared-memory fallbacks stayed around too long

## Asterinas Ideas Worth Adopting

The feedback so far is consistent with borrowing **ideas**, not code.

### 1. Small unsafe substrate

Create a clearer separation between:

- low-level unsafe substrate
  - interrupt entry/exit
  - raw context switch
  - page-table mutation
- CR3 / FS / GS / MSR handling
- DMA / low-level device transport
- higher-level services
  - VFS
  - sockets
  - signals
  - exec/fork
  - PTY/session logic
  - scheduler policy

### 2. Type-enforced invariants

Adopt more APIs where invalid states are hard to express:

- typed file descriptions
- explicit blocked/woken reasons
- explicit signal-delivery state
- explicit address-space ownership
- explicit task lifecycle transitions

### 4. Framekernel-style substrate discipline

One of the most useful additions from the latest review is not “copy Asterinas,”
but “copy its boundary discipline.”

Lineluya should evolve toward a substrate split like:

- unsafe substrate
  - trap/syscall frames
  - context switch substrate
  - page-table mutation
  - user-memory copy
  - waitqueue core
  - raw device transport
- higher-level services
  - VFS
  - sockets
  - signals
  - exec/fork
  - PTY/session
  - scheduler policy

That split is useful even if Lineluya never literally mirrors Asterinas’s
crate layout.

Claude’s review reinforces one especially practical piece of that split:

- a single safe-ish entry point for page-table activation and CR3 writes
- a single explicit context-switch boundary instead of many partial state sync
  sites

### 3. Verification-friendly boundaries

Even before formal verification, the code should evolve toward:

- crisp contracts
- narrow unsafe boundaries
- deterministic wake/sleep behavior
- auditable `copy_to_user` / `copy_from_user` callsites

### 4. Sensitive-core discipline

Another strong idea worth adopting from the Asterinas direction is to treat the
following as substrate-only concerns:

- trap and syscall frame mutation
- user-memory copy
- page-table mutation
- raw scheduler context switching
- waitqueue core rules
- low-level device transport and DMA

The rest of the kernel should consume these through safe, higher-level
interfaces rather than mutating them directly.

## Recommended 90-Day Direction

This section compiles the strongest overlapping suggestions into one plan.

### Days 1–30: Fix the Cross-Cutting Primitives

Priority order:

1. unify blocking semantics
2. make signal interruption authoritative
3. reduce scheduler/workload entanglement
4. define one authoritative arch-transition API

Concrete outcomes:

- no production yield loops in blocking syscalls
- no partially-blocking `select` / `poll` / `wait4` behavior
- clear `-EINTR` semantics for deliverable signals
- no ad hoc mutation of CR3 / FS / GS / current-task outside the transition boundary

### Days 31–60: Fix Identity and Ownership

Priority order:

1. typed file description layer
2. authoritative process address-space ownership
3. removal of shared-PML4 user fallbacks

Concrete outcomes:

- no inode-mode-based socket identity in syscall dispatch
- no ambiguous ownership of user mappings
- cleaner fork/exec/signal-frame semantics

### Days 61–90: Fix Lifecycle and Teardown

Priority order:

1. pipe/socket EOF and close semantics
2. session/child-exit ordering
3. scheduler policy/mechanism separation

Concrete outcomes:

- clean SSH session close and repeat connections
- deterministic wake order for child exit, EOF, and signal delivery
- fewer workload-specific patches in generic kernel paths
- a smaller, better-audited unsafe substrate

## Invariants To Encode In Rust

This is the most important design section in the whole synthesis.

These should become API or type-level invariants over time:

### Execution-State Invariants

- a task cannot return to userspace unless its arch return state is fully restored by one authoritative boundary
- a runnable task and a sleeping task are distinct scheduler states with no hybrid representation
- a blocked task is not in the run queue
- only the arch-transition layer may mutate CR3, kernel stack top, FS/GS, or user return frames

### Address-Space Invariants

- every user task runs with an explicit address-space owner
- user-space execution on the boot page table becomes impossible except in a tightly bounded bootstrap phase

### FD/File Invariants

- every fd resolves to a typed file description, not to a “guess its type from inode mode” branch
- a syscall that requires a socket gets a socket object, not an arbitrary file first and a runtime inference later
- fd table entry lifetime and open file description lifetime are separate and explicit

### Signal Invariants

- a blocking syscall either sleeps or returns `-EINTR`
- signal delivery and signal return are explicit state transitions
- signal handlers cannot be silently eaten by default-action logic

### Teardown Invariants

- EOF/close semantics have one authoritative path per object type
- child exit wakeup ordering is explicit
- resource closure is reference-counted and not inferred from one process’s local close alone
- session teardown does not depend on workload-specific scheduler luck

## Suggested Deletion Targets

These should be named as explicit debt:

- inode-mode-based socket detection in generic dispatch
- workload-specific conditionals in generic syscall/scheduler paths
- “keep yielding until parent/child runs” loops
- shared-user-address-space fallback after per-process PT is authoritative
- duplicated or fuzzy wake paths for teardown and disconnect handling

## Near-Term Decision Rule Chirho

The most useful synthesis rule so far is:

- workloads validate primitives
- workloads do not get to define generic semantics

In practice, that means every debugging session should end by classifying the
result into one of four buckets:

- core primitive implemented
- temporary diagnostic added
- temporary workaround added
- demo glue identified for deletion

If a workaround remains after the primitive exists, it should be scheduled for
deletion explicitly instead of lingering by default.

## Current Grade Range

At this moment, the external feedback range is:

- Gemini: `C+`
- Grok: `B-`
- GPT-5.4 Pro Extended: `B-`
- Claude: `B-`
- Codex view: viable architecture, but not yet clean enough to call solid without qualification

My synthesis of that range:

- **Current practical grade: B-/C+**

Interpretation:

- the foundations are real
- the architecture is not fake
- but the kernel is still in the dangerous middle where tactical fixes can either become good primitives or permanent structural debt

## Immediate Guidance For Lineluya

The next chapter of work should not be:

- “what is the next impressive demo item?”

It should be:

- “which primitive is still non-authoritative, and what workaround does that primitive let us delete once fixed?”

That is the correct decision rule for a real kernel.

## Pending Inputs

Add these when available:

- GPT-5.4 Pro Extended review
- any future human-authored suggestion paper

When adding new material, prefer:

- explicit agreement/disagreement with prior reviewers
- updated consensus list
- updated deletion targets
- updated 90-day plan
