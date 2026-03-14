// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! Preemptive round-robin task scheduler for the Lineluya kernel.
//!
//! This module implements the core scheduling infrastructure. The initial policy
//! is a simple round-robin scheduler with fixed time slices.  The design is
//! structured so that the scheduling policy can be swapped out later — the plan
//! is to evolve this into an EEVDF (Earliest Eligible Virtual Deadline First)
//! scheduler, which replaced CFS in the upstream Linux kernel starting with
//! version 6.6.
//!
//! # Architecture
//!
//! The scheduler centres on [`RunQueueChirho`], a FIFO queue of runnable task
//! PIDs.  On each timer tick ([`schedule_tick_chirho`]), the current task's
//! remaining time slice is decremented.  When it reaches zero the
//! `need_resched_chirho` flag is set, signalling that the next return to
//! kernel-mode scheduling code should invoke [`schedule_chirho`] to pick the
//! next task.
//!
//! The actual register-level context switch is performed by an external
//! assembly routine ([`switch_context_chirho`]) that saves and restores
//! callee-saved registers, the stack pointer, and the instruction pointer.
//!
//! # Locking
//!
//! The global scheduler state is protected by a [`spin::Mutex`].  Interrupts
//! should be disabled (or the lock should be acquired with an interrupt-safe
//! guard) whenever the scheduler is manipulated from an interrupt context to
//! prevent deadlocks — the timer interrupt handler is the primary case.

extern crate alloc;

use alloc::collections::VecDeque;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Default number of timer ticks a task may run before being preempted.
///
/// With a typical PIT frequency of ~1000 Hz this yields a 10 ms time slice,
/// which is a reasonable starting point for interactive responsiveness vs.
/// throughput.  Linux's CFS/EEVDF uses a more dynamic scheme, but round-robin
/// with a fixed quantum is fine for the initial bring-up.
pub const DEFAULT_TIME_SLICE_CHIRHO: u64 = 10;

/// Maximum number of tasks the scheduler supports concurrently.
///
/// This is a soft limit used for sanity checks.  The `VecDeque`-backed run
/// queue can grow dynamically, but we guard against runaway task creation.
pub const MAX_TASKS_CHIRHO: usize = 1024;

// ---------------------------------------------------------------------------
// Atomic flags (accessible without acquiring the scheduler lock)
// ---------------------------------------------------------------------------

/// Global reschedule-needed flag.  Set by [`schedule_tick_chirho`] from the
/// timer interrupt and checked by kernel return paths.  Using an atomic avoids
/// the need to hold the scheduler spinlock just to poll the flag.
static NEED_RESCHED_ATOMIC_CHIRHO: AtomicBool = AtomicBool::new(false);

/// Monotonically increasing tick counter.  Useful for timekeeping, profiling,
/// and as a coarse timestamp source.  Updated on every timer interrupt.
static GLOBAL_TICK_COUNT_CHIRHO: AtomicU64 = AtomicU64::new(0);

// ---------------------------------------------------------------------------
// External assembly routine for context switching
// ---------------------------------------------------------------------------

extern "C" {
    /// Switch the CPU context from one task to another.
    ///
    /// Saves the callee-saved registers (`rbx`, `rbp`, `r12`–`r15`), the stack
    /// pointer (`rsp`), and the return address (`rip`) into the
    /// [`CpuContextChirho`] pointed to by `old_context_chirho`, then restores
    /// the same set of registers from `new_context_chirho`.  After the restore
    /// the function "returns" — but it returns *into the new task*, which will
    /// resume execution at whatever `rip` was saved in its context.
    ///
    /// # Safety
    ///
    /// Both pointers must point to valid, properly aligned
    /// [`CpuContextChirho`] structures.  The `new_context_chirho` must have
    /// been previously saved by an earlier call to `switch_context_chirho` (or
    /// synthesised during task creation) so that the register values are sane.
    fn switch_context_chirho(
        old_context_chirho: *mut crate::task_chirho::CpuContextChirho,
        new_context_chirho: *const crate::task_chirho::CpuContextChirho,
    );
}

// ---------------------------------------------------------------------------
// Run queue
// ---------------------------------------------------------------------------

/// The kernel's run queue — holds the set of runnable task PIDs and the
/// scheduler's bookkeeping state.
///
/// Tasks that are blocked (sleeping, waiting on I/O, etc.) are removed from
/// the run queue and re-inserted when they become runnable again.
pub struct RunQueueChirho {
    /// FIFO queue of runnable task PIDs.  The front of the deque is the *next*
    /// task to run; when a task is preempted it is pushed to the back.
    tasks_chirho: VecDeque<u64>,

    /// PID of the currently executing task, or `None` if no task is running
    /// (e.g. during early boot before the first task is scheduled).
    current_pid_chirho: Option<u64>,

    /// Number of timer ticks remaining in the current task's time slice.
    /// When this reaches zero the task is preempted.
    remaining_ticks_chirho: u64,

    /// Local scheduler tick counter (mirrors the atomic global counter, but
    /// is only updated while the lock is held — useful for assertions).
    tick_count_chirho: u64,

    /// Flag indicating that a reschedule is needed.  This is the lock-protected
    /// counterpart of [`NEED_RESCHED_ATOMIC_CHIRHO`]; both are kept in sync.
    need_resched_chirho: bool,
}

impl RunQueueChirho {
    /// Create a new, empty run queue with default settings.
    pub const fn new_chirho() -> Self {
        Self {
            tasks_chirho: VecDeque::new(),
            current_pid_chirho: None,
            remaining_ticks_chirho: DEFAULT_TIME_SLICE_CHIRHO,
            tick_count_chirho: 0,
            need_resched_chirho: false,
        }
    }

    /// Return the number of runnable tasks (excluding the currently running
    /// task, which is not in the queue while it executes).
    #[inline]
    pub fn len_chirho(&self) -> usize {
        self.tasks_chirho.len()
    }

    /// Return `true` if no tasks are in the run queue.
    #[inline]
    pub fn is_empty_chirho(&self) -> bool {
        self.tasks_chirho.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Global scheduler state
// ---------------------------------------------------------------------------

/// Global scheduler instance, protected by a spinlock.
///
/// Initialised to `None`; call [`init_scheduler_chirho`] during boot to
/// populate it.  All scheduler operations acquire this lock.
///
/// # Deadlock avoidance
///
/// Code that acquires this lock must **not** be interrupted by a handler that
/// also acquires it.  In practice this means disabling interrupts (or using
/// `spin::Mutex::try_lock`) in interrupt context.
static SCHEDULER_CHIRHO: spin::Mutex<Option<RunQueueChirho>> =
    spin::Mutex::new(None);

// ---------------------------------------------------------------------------
// Initialisation
// ---------------------------------------------------------------------------

/// Initialise the global scheduler.
///
/// Creates an empty [`RunQueueChirho`] and stores it in the global
/// [`SCHEDULER_CHIRHO`] mutex.  Must be called exactly once during kernel boot,
/// after the heap allocator is available (because `VecDeque` may allocate) but
/// before any tasks are created or the timer interrupt is enabled.
///
/// # Panics
///
/// Panics if called more than once (the scheduler is already initialised).
pub fn init_scheduler_chirho() {
    let mut scheduler_guard_chirho = SCHEDULER_CHIRHO.lock();

    if scheduler_guard_chirho.is_some() {
        panic!("init_scheduler_chirho: scheduler already initialised");
    }

    *scheduler_guard_chirho = Some(RunQueueChirho::new_chirho());
}

// ---------------------------------------------------------------------------
// Core scheduling
// ---------------------------------------------------------------------------

/// The main scheduling function.
///
/// Determines the next task to run and, if it differs from the current task,
/// performs a context switch.  The overall flow is:
///
/// 1. Clear the reschedule-needed flag.
/// 2. Take the current task (if any) and push it to the back of the run queue
///    so that it gets another turn later (round-robin).
/// 3. Pop the next task from the front of the run queue.
/// 4. If the next task is the same as the current one (or there is nothing to
///    switch to), just replenish the time slice and return.
/// 5. Otherwise, look up both tasks' contexts via the task table (provided by
///    `crate::task_chirho`) and invoke the assembly context-switch routine.
///
/// # Safety
///
/// This function performs a context switch which is inherently unsafe — it
/// manipulates the CPU's register file and stack pointer.  The caller must
/// ensure that:
/// - Interrupts are disabled while this function executes.
/// - The task table (`crate::task_chirho`) has valid context structures for the
///   involved PIDs.
pub fn schedule_chirho() {
    // Disable interrupts for the duration of the scheduling decision and the
    // context switch.  We re-enable them after the switch (the new task will
    // return with interrupts enabled because its saved rflags has IF set).
    x86_64::instructions::interrupts::without_interrupts(|| {
        let mut scheduler_guard_chirho = SCHEDULER_CHIRHO.lock();
        let scheduler_chirho = scheduler_guard_chirho
            .as_mut()
            .expect("schedule_chirho: scheduler not initialised");

        // 1. Clear the reschedule flag.
        scheduler_chirho.need_resched_chirho = false;
        NEED_RESCHED_ATOMIC_CHIRHO.store(false, Ordering::Release);

        // 2. If there is a current task, push it to the back of the run queue
        //    so it participates in future rounds.
        let old_pid_chirho = scheduler_chirho.current_pid_chirho;
        if let Some(pid_chirho) = old_pid_chirho {
            scheduler_chirho.tasks_chirho.push_back(pid_chirho);
        }

        // 3. Pop the next task from the front.
        let next_pid_chirho = scheduler_chirho.tasks_chirho.pop_front();

        match next_pid_chirho {
            None => {
                // No runnable tasks.  Clear the current PID and return — the
                // kernel's idle loop will take over.
                scheduler_chirho.current_pid_chirho = None;
                return;
            }
            Some(next_chirho) => {
                // 4. If the next task is the same as the old one, no context
                //    switch is necessary — just reset the time slice.
                if old_pid_chirho == Some(next_chirho) {
                    scheduler_chirho.current_pid_chirho = Some(next_chirho);
                    scheduler_chirho.remaining_ticks_chirho = DEFAULT_TIME_SLICE_CHIRHO;
                    return;
                }

                // 5. Different task — perform a context switch.
                scheduler_chirho.current_pid_chirho = Some(next_chirho);
                scheduler_chirho.remaining_ticks_chirho = DEFAULT_TIME_SLICE_CHIRHO;

                // Obtain raw pointers to the CPU contexts *before* dropping the
                // scheduler lock.  The task table is expected to provide stable
                // pointers (pinned allocations) for the lifetime of the task.
                //
                // NOTE: In a real implementation the task table lookup would go
                // here.  For now we reference the task module's accessor API.
                // The actual context switch is unsafe because it manipulates
                // raw register state.
                let old_ctx_ptr_chirho = match old_pid_chirho {
                    Some(pid_chirho) => {
                        crate::task_chirho::context_ptr_mut_chirho(pid_chirho)
                    }
                    None => {
                        // No previous task (first schedule).  We still need a
                        // valid pointer to save into, so use the boot context.
                        crate::task_chirho::boot_context_ptr_chirho()
                    }
                };
                let new_ctx_ptr_chirho =
                    crate::task_chirho::context_ptr_chirho(next_chirho);

                // Switch page tables if the new task has its own address space.
                // Look up the new task's page_table_root_chirho and switch CR3
                // before the context switch so the new task resumes in the
                // correct address space.
                let new_pt_root_chirho = {
                    let list_chirho = crate::task_chirho::TASK_LIST_CHIRHO.lock();
                    list_chirho
                        .iter()
                        .find(|t_chirho| t_chirho.lock().pid_chirho == next_chirho)
                        .and_then(|t_chirho| t_chirho.lock().page_table_root_chirho)
                };

                // Drop the scheduler lock before performing the actual context
                // switch so that the new task can acquire it if needed.
                drop(scheduler_guard_chirho);

                // Switch to the new task's page table if it has one.
                if let Some(pt_root_chirho) = new_pt_root_chirho {
                    unsafe {
                        crate::pagetable_chirho::switch_page_table_chirho(pt_root_chirho);
                    }
                }

                // SAFETY: Both pointers are obtained from the task table which
                // guarantees their validity for the lifetime of the respective
                // tasks.  Interrupts are disabled (we are inside
                // `without_interrupts`).
                unsafe {
                    switch_context_chirho(old_ctx_ptr_chirho, new_ctx_ptr_chirho);
                }
            }
        }
    });
}

// ---------------------------------------------------------------------------
// Timer tick handler
// ---------------------------------------------------------------------------

/// Called from the timer interrupt handler on every PIT/APIC tick.
///
/// Responsibilities:
/// 1. Increment the global tick counter (atomic, lock-free).
/// 2. Decrement the current task's remaining time slice.
/// 3. If the time slice is exhausted, set the reschedule flag so that the
///    kernel's interrupt return path calls [`schedule_chirho`].
///
/// This function acquires the scheduler spinlock.  The timer interrupt handler
/// must ensure that the lock is not already held (i.e., do not nest timer
/// interrupts).  Using `try_lock` is a defensive option to avoid deadlocking
/// in case of unexpected re-entrancy.
pub fn schedule_tick_chirho() {
    // Bump the global atomic tick counter (no lock needed).
    GLOBAL_TICK_COUNT_CHIRHO.fetch_add(1, Ordering::Relaxed);

    // Try to acquire the scheduler lock.  If it is already held (e.g. the
    // timer fired while `schedule_chirho` was in progress), skip this tick
    // rather than deadlocking.
    let maybe_guard_chirho = SCHEDULER_CHIRHO.try_lock();
    let mut scheduler_guard_chirho = match maybe_guard_chirho {
        Some(guard_chirho) => guard_chirho,
        None => return, // Lock contention — skip this tick.
    };

    let scheduler_chirho = match scheduler_guard_chirho.as_mut() {
        Some(s_chirho) => s_chirho,
        None => return, // Scheduler not yet initialised.
    };

    // Keep the local tick counter in sync.
    scheduler_chirho.tick_count_chirho =
        GLOBAL_TICK_COUNT_CHIRHO.load(Ordering::Relaxed);

    // Only meaningful if a task is currently running.
    if scheduler_chirho.current_pid_chirho.is_some() {
        // Decrement the remaining time slice.  Saturating subtraction avoids
        // underflow in case of a missed decrement.
        scheduler_chirho.remaining_ticks_chirho =
            scheduler_chirho.remaining_ticks_chirho.saturating_sub(1);

        if scheduler_chirho.remaining_ticks_chirho == 0 {
            // Time slice exhausted — request a reschedule.
            scheduler_chirho.need_resched_chirho = true;
            NEED_RESCHED_ATOMIC_CHIRHO.store(true, Ordering::Release);
        }
    }
}

// ---------------------------------------------------------------------------
// Task management helpers
// ---------------------------------------------------------------------------

/// Add a task to the run queue.
///
/// The task identified by `pid_chirho` becomes runnable and will be scheduled
/// in round-robin order.  If the scheduler is not yet initialised, or the run
/// queue has reached [`MAX_TASKS_CHIRHO`], the call is a no-op (in a more
/// mature kernel this would return an error).
///
/// # Duplicate PID check
///
/// A linear scan guards against inserting the same PID twice, which would
/// corrupt the round-robin ordering.  This O(n) check is acceptable for the
/// current task-count limit.
pub fn add_task_chirho(pid_chirho: u64) {
    x86_64::instructions::interrupts::without_interrupts(|| {
        let mut scheduler_guard_chirho = SCHEDULER_CHIRHO.lock();
        let scheduler_chirho = match scheduler_guard_chirho.as_mut() {
            Some(s_chirho) => s_chirho,
            None => return,
        };

        // Enforce the maximum task limit.
        if scheduler_chirho.tasks_chirho.len() >= MAX_TASKS_CHIRHO {
            return;
        }

        // Prevent duplicate insertion.
        if scheduler_chirho.current_pid_chirho == Some(pid_chirho)
            || scheduler_chirho.tasks_chirho.contains(&pid_chirho)
        {
            return;
        }

        scheduler_chirho.tasks_chirho.push_back(pid_chirho);
    });
}

/// Remove a task from the run queue.
///
/// If the task is currently running it is removed (the next call to
/// [`schedule_chirho`] will pick a different task).  If it is in the ready
/// queue it is spliced out.  Removing a PID that is not present is a no-op.
pub fn remove_task_chirho(pid_chirho: u64) {
    x86_64::instructions::interrupts::without_interrupts(|| {
        let mut scheduler_guard_chirho = SCHEDULER_CHIRHO.lock();
        let scheduler_chirho = match scheduler_guard_chirho.as_mut() {
            Some(s_chirho) => s_chirho,
            None => return,
        };

        // If the task is the currently running one, clear it.  The next
        // schedule_chirho() call will notice `current_pid_chirho == None`.
        if scheduler_chirho.current_pid_chirho == Some(pid_chirho) {
            scheduler_chirho.current_pid_chirho = None;
            scheduler_chirho.need_resched_chirho = true;
            NEED_RESCHED_ATOMIC_CHIRHO.store(true, Ordering::Release);
            return;
        }

        // Otherwise scan the ready queue and remove the first match.
        if let Some(index_chirho) = scheduler_chirho
            .tasks_chirho
            .iter()
            .position(|&p_chirho| p_chirho == pid_chirho)
        {
            scheduler_chirho.tasks_chirho.remove(index_chirho);
        }
    });
}

/// Block the currently running task.
///
/// Removes the current task from the run queue so it will not be rescheduled
/// until explicitly unblocked via [`unblock_task_chirho`].  The caller is
/// responsible for recording the PID somewhere (e.g. a wait-queue) so that it
/// can be unblocked later.
///
/// After calling this, a reschedule is forced so that another task can run.
///
/// Returns the PID of the blocked task, or `None` if there was no current task.
pub fn block_current_chirho() -> Option<u64> {
    let blocked_pid_chirho = x86_64::instructions::interrupts::without_interrupts(|| {
        let mut scheduler_guard_chirho = SCHEDULER_CHIRHO.lock();
        let scheduler_chirho = match scheduler_guard_chirho.as_mut() {
            Some(s_chirho) => s_chirho,
            None => return None,
        };

        let pid_chirho = scheduler_chirho.current_pid_chirho.take();

        if pid_chirho.is_some() {
            // Force a reschedule — the current task is no longer runnable.
            scheduler_chirho.need_resched_chirho = true;
            NEED_RESCHED_ATOMIC_CHIRHO.store(true, Ordering::Release);
        }

        pid_chirho
    });

    // Trigger the actual context switch (will pick the next runnable task).
    if blocked_pid_chirho.is_some() {
        schedule_chirho();
    }

    blocked_pid_chirho
}

/// Unblock a task and place it back on the run queue.
///
/// This is the counterpart of [`block_current_chirho`].  The task identified
/// by `pid_chirho` is added to the back of the run queue so it becomes eligible
/// for scheduling again.  If the PID is already in the queue (or is the
/// currently running task), the call is a no-op.
pub fn unblock_task_chirho(pid_chirho: u64) {
    add_task_chirho(pid_chirho);
}

/// Voluntarily yield the CPU.
///
/// The current task gives up its remaining time slice and goes to the back of
/// the run queue.  This is the kernel-side implementation of the `sched_yield`
/// system call.
///
/// If there are no other runnable tasks the same task will be immediately
/// rescheduled (with a fresh time slice).
pub fn yield_current_chirho() {
    // Forcibly set the reschedule flag and invoke the scheduler.
    x86_64::instructions::interrupts::without_interrupts(|| {
        let mut scheduler_guard_chirho = SCHEDULER_CHIRHO.lock();
        if let Some(scheduler_chirho) = scheduler_guard_chirho.as_mut() {
            scheduler_chirho.remaining_ticks_chirho = 0;
            scheduler_chirho.need_resched_chirho = true;
            NEED_RESCHED_ATOMIC_CHIRHO.store(true, Ordering::Release);
        }
    });

    schedule_chirho();
}

// ---------------------------------------------------------------------------
// Query helpers
// ---------------------------------------------------------------------------

/// Check whether a reschedule has been requested (lock-free).
///
/// Kernel return paths (e.g. after handling an interrupt or a system call) can
/// poll this cheaply and call [`schedule_chirho`] if it returns `true`.
#[inline]
pub fn need_resched_chirho() -> bool {
    NEED_RESCHED_ATOMIC_CHIRHO.load(Ordering::Acquire)
}

/// Return the PID of the currently running task, or `None` during idle.
pub fn current_pid_chirho() -> Option<u64> {
    x86_64::instructions::interrupts::without_interrupts(|| {
        let scheduler_guard_chirho = SCHEDULER_CHIRHO.lock();
        scheduler_guard_chirho
            .as_ref()
            .and_then(|s_chirho| s_chirho.current_pid_chirho)
    })
}

/// Return the global tick count (monotonically increasing, lock-free).
#[inline]
pub fn tick_count_chirho() -> u64 {
    GLOBAL_TICK_COUNT_CHIRHO.load(Ordering::Relaxed)
}

// ---------------------------------------------------------------------------
// Scheduling policy trait
// ---------------------------------------------------------------------------

/// Trait abstracting a scheduling policy.
///
/// The [`RunQueueChirho`] above hard-codes a round-robin policy.  As the
/// kernel matures the plan is to introduce an EEVDF scheduler (the algorithm
/// that replaced CFS in Linux 6.6).  This trait defines the interface that any
/// policy must implement so that the core scheduler machinery can remain
/// policy-agnostic.
///
/// # Type parameter
///
/// All methods operate on PIDs (`u64`).  A more sophisticated design might
/// parameterise over a task handle type, but PIDs keep things simple for now.
pub trait SchedulerPolicyChirho {
    /// Select the next task to run.
    ///
    /// Returns `Some(pid)` if there is a runnable task, or `None` if the CPU
    /// should idle.  The implementation is free to use any algorithm (FIFO,
    /// priority-based, virtual-deadline, etc.).
    fn pick_next_chirho(&mut self) -> Option<u64>;

    /// Notify the policy that a timer tick has occurred for `pid_chirho`.
    ///
    /// The policy can use this to update accounting (e.g. consumed CPU time,
    /// virtual runtime in CFS/EEVDF) and decide whether the task should be
    /// preempted.
    fn task_tick_chirho(&mut self, pid_chirho: u64);

    /// Add a task to the policy's internal data structures.
    ///
    /// Called when a task becomes runnable (creation, unblock, migration).
    fn enqueue_chirho(&mut self, pid_chirho: u64);

    /// Remove a task from the policy's internal data structures.
    ///
    /// Called when a task is no longer runnable (exit, block, migration).
    fn dequeue_chirho(&mut self, pid_chirho: u64);
}

// ---------------------------------------------------------------------------
// Round-robin policy (reference implementation)
// ---------------------------------------------------------------------------

/// A simple round-robin scheduling policy.
///
/// Tasks are served in FIFO order.  Each task runs for one full time slice
/// ([`DEFAULT_TIME_SLICE_CHIRHO`] ticks) before being preempted.  There is no
/// notion of priority or dynamic time-slice adjustment.
///
/// This struct implements [`SchedulerPolicyChirho`] and can serve as a
/// reference for more advanced policies.
pub struct RoundRobinPolicyChirho {
    /// Queue of runnable task PIDs in FIFO order.
    queue_chirho: VecDeque<u64>,
}

impl RoundRobinPolicyChirho {
    /// Create a new, empty round-robin policy.
    pub const fn new_chirho() -> Self {
        Self {
            queue_chirho: VecDeque::new(),
        }
    }
}

impl SchedulerPolicyChirho for RoundRobinPolicyChirho {
    fn pick_next_chirho(&mut self) -> Option<u64> {
        self.queue_chirho.pop_front()
    }

    fn task_tick_chirho(&mut self, _pid_chirho: u64) {
        // Round-robin does not perform per-tick accounting beyond the time
        // slice countdown, which is handled by the core scheduler.
    }

    fn enqueue_chirho(&mut self, pid_chirho: u64) {
        // Avoid duplicates.
        if !self.queue_chirho.contains(&pid_chirho) {
            self.queue_chirho.push_back(pid_chirho);
        }
    }

    fn dequeue_chirho(&mut self, pid_chirho: u64) {
        if let Some(index_chirho) = self
            .queue_chirho
            .iter()
            .position(|&p_chirho| p_chirho == pid_chirho)
        {
            self.queue_chirho.remove(index_chirho);
        }
    }
}
