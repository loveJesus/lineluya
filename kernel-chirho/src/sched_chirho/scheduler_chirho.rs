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
pub const DEFAULT_TIME_SLICE_CHIRHO: u64 = 50;

/// Boosted time slice for active X11 client/render tasks.
///
/// These tasks tend to alternate short AF_UNIX request bursts with immediate
/// follow-up drawing work. Giving them a few consecutive ticks improves frame
/// rate substantially without changing the rest of the run-queue policy.
pub const X11_RENDER_TIME_SLICE_CHIRHO: u64 = DEFAULT_TIME_SLICE_CHIRHO * 3;

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

/// Limited scheduler tick trace for debugging PID 4 post-yield starvation.
static TICK_TRACE_COUNTER_CHIRHO: AtomicU64 = AtomicU64::new(0);

/// Limited scheduler lock contention trace from the timer path.
static TICK_LOCK_MISS_COUNTER_CHIRHO: AtomicU64 = AtomicU64::new(0);

/// Limited trace for times the timer observes no current task.
static TICK_IDLE_TRACE_COUNTER_CHIRHO: AtomicU64 = AtomicU64::new(0);

/// Limited high-PID scheduler trace for X11/fork-chain debugging.
static SCHED_TRACE_COUNTER_CHIRHO: AtomicU64 = AtomicU64::new(0);

/// Only trace scheduler activity once user-space helper chains are active.
const SCHED_TRACE_PID_FLOOR_CHIRHO: u64 = 5;

/// Hard cap so scheduler tracing does not flood the serial log.
const SCHED_TRACE_LIMIT_CHIRHO: u64 = 256;

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

/// Thin wrapper around [`switch_context_chirho`] so the resumed task lands in
/// a tiny post-switch frame first, not directly in the middle of the larger
/// `schedule_chirho` body.
///
/// The raw assembly already preserves the resumed task's callee-saved state.
/// This wrapper's job is only to keep the immediate post-switch resume path
/// structurally simple and to re-enable interrupts before returning to the
/// caller.
#[unsafe(naked)]
unsafe extern "C" fn switch_context_return_wrapper_chirho(
    old_context_chirho: *mut crate::task_chirho::CpuContextChirho,
    new_context_chirho: *const crate::task_chirho::CpuContextChirho,
) {
    core::arch::naked_asm!(
        // Enter switch_context_chirho with a synthetic return address to the
        // local resume label below. This avoids a compiler-generated wrapper
        // frame while still giving switch_context_chirho the call/return shape
        // it expects: [RSP] holds the continuation RIP.
        "lea rax, [rip + 2f]",
        "push rax",
        "jmp {switch_context_chirho}",
        "2:",
        "sti",
        "ret",
        switch_context_chirho = sym switch_context_chirho,
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

fn task_state_name_for_pid_chirho(pid_chirho: u64) -> &'static str {
    let Some(task_arc_chirho) = crate::task_chirho::find_task_by_pid_chirho(pid_chirho) else {
        return "Missing";
    };

    let task_state_chirho = task_arc_chirho.lock().state_chirho;
    match task_state_chirho {
        crate::task_chirho::TaskStateChirho::RunningChirho => "Running",
        crate::task_chirho::TaskStateChirho::ReadyChirho => "Ready",
        crate::task_chirho::TaskStateChirho::SleepingChirho => "Sleeping",
        crate::task_chirho::TaskStateChirho::BlockedChirho => "Blocked",
        crate::task_chirho::TaskStateChirho::StoppedChirho => "Stopped",
        crate::task_chirho::TaskStateChirho::ZombieChirho => "Zombie",
        crate::task_chirho::TaskStateChirho::DeadChirho => "Dead",
    }
}

fn task_has_x11_render_socket_chirho(pid_chirho: u64) -> bool {
    let Some(task_arc_chirho) = crate::task_chirho::find_task_by_pid_chirho(pid_chirho) else {
        return false;
    };

    let task_guard_chirho = task_arc_chirho.lock();
    let Some(fd_table_chirho) = task_guard_chirho.fd_table_chirho.as_ref() else {
        return false;
    };

    for file_option_chirho in fd_table_chirho.fds_chirho.iter() {
        let Some(file_arc_chirho) = file_option_chirho.as_ref() else {
            continue;
        };

        let (inode_mode_chirho, inode_number_chirho) = {
            let file_guard_chirho = file_arc_chirho.lock();
            let inode_guard_chirho = file_guard_chirho.inode_chirho.lock();
            (inode_guard_chirho.mode_chirho, inode_guard_chirho.ino_chirho)
        };

        if (inode_mode_chirho & 0o170000) != 0o140000 {
            continue;
        }

        if crate::net_chirho::is_x11_connected_unix_socket_idx_chirho(
            inode_number_chirho as usize,
        ) {
            return true;
        }
    }

    false
}

fn time_slice_for_pid_chirho(pid_chirho: u64) -> u64 {
    if task_has_x11_render_socket_chirho(pid_chirho) {
        X11_RENDER_TIME_SLICE_CHIRHO
    } else {
        DEFAULT_TIME_SLICE_CHIRHO
    }
}

fn should_trace_scheduler_event_chirho(
    old_pid_chirho: Option<u64>,
    next_pid_chirho: Option<u64>,
    tasks_chirho: &VecDeque<u64>,
) -> bool {
    old_pid_chirho.is_some_and(|pid_chirho| pid_chirho >= SCHED_TRACE_PID_FLOOR_CHIRHO)
        || next_pid_chirho.is_some_and(|pid_chirho| pid_chirho >= SCHED_TRACE_PID_FLOOR_CHIRHO)
        || tasks_chirho
            .iter()
            .take(4)
            .any(|&pid_chirho| pid_chirho >= SCHED_TRACE_PID_FLOOR_CHIRHO)
}

fn trace_scheduler_event_chirho(
    label_chirho: &str,
    old_pid_chirho: Option<u64>,
    next_pid_chirho: Option<u64>,
    tasks_chirho: &VecDeque<u64>,
) {
    if !should_trace_scheduler_event_chirho(old_pid_chirho, next_pid_chirho, tasks_chirho) {
        return;
    }

    let trace_idx_chirho = SCHED_TRACE_COUNTER_CHIRHO.fetch_add(1, Ordering::Relaxed);
    if trace_idx_chirho >= SCHED_TRACE_LIMIT_CHIRHO {
        return;
    }

    let old_state_chirho = old_pid_chirho
        .map(task_state_name_for_pid_chirho)
        .unwrap_or("None");
    let next_state_chirho = next_pid_chirho
        .map(task_state_name_for_pid_chirho)
        .unwrap_or("None");
    let front0_chirho = tasks_chirho.get(0).copied().unwrap_or(0);
    let front1_chirho = tasks_chirho.get(1).copied().unwrap_or(0);
    let front2_chirho = tasks_chirho.get(2).copied().unwrap_or(0);
    let front3_chirho = tasks_chirho.get(3).copied().unwrap_or(0);

    crate::serial_println_chirho!(
        "[SCHED-TRACE] #{} {} old={:?}/{} next={:?}/{} qlen={} front=[{},{},{},{}]",
        trace_idx_chirho,
        label_chirho,
        old_pid_chirho,
        old_state_chirho,
        next_pid_chirho,
        next_state_chirho,
        tasks_chirho.len(),
        front0_chirho,
        front1_chirho,
        front2_chirho,
        front3_chirho,
    );
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
// Architecture-specific context switch preparation
// ---------------------------------------------------------------------------

/// Perform architecture-specific context switch preparation.
///
/// Switches the page table (CR3) to the target task's address space. If the
/// task has a per-process page table, CR3 is loaded with that physical
/// address. Otherwise, CR3 is switched back to the boot PML4 so the task
/// runs in the shared kernel address space.
///
/// This helper separates arch-specific concerns from the scheduling policy
/// logic in [`schedule_chirho`], making it easier to port to other
/// architectures or to swap out page-table switching strategies.
fn arch_prepare_switch_chirho(old_pid_chirho: Option<u64>, next_pid_chirho: u64) {
    use x86_64::registers::model_specific::Msr;
    const IA32_FS_BASE_CHIRHO: u32 = 0xC000_0100;
    const IA32_KERNEL_GS_BASE_CHIRHO: u32 = 0xC000_0102;

    if let Some(old_pid_value_chirho) = old_pid_chirho {
        let old_fs_base_chirho = unsafe { Msr::new(IA32_FS_BASE_CHIRHO).read() };
        let old_gs_base_chirho = unsafe { Msr::new(IA32_KERNEL_GS_BASE_CHIRHO).read() };

        let list_chirho = crate::task_chirho::TASK_LIST_CHIRHO.lock();
        if let Some(old_task_arc_chirho) = list_chirho
            .iter()
            .find(|task_arc_chirho| task_arc_chirho.lock().pid_chirho == old_pid_value_chirho)
        {
            let mut old_task_guard_chirho = old_task_arc_chirho.lock();
            old_task_guard_chirho.fs_base_chirho = old_fs_base_chirho;
            old_task_guard_chirho.gs_base_chirho = old_gs_base_chirho;
        }
    }

    // Look up the target task's address-space so CR3 can be switched after
    // the low-level asm has already moved RSP onto the new task's kernel stack.
    let new_pt_root_chirho = {
        let list_chirho = crate::task_chirho::TASK_LIST_CHIRHO.lock();
        list_chirho
            .iter()
            .find(|t_chirho| t_chirho.lock().pid_chirho == next_pid_chirho)
            .map(|t_chirho| {
                let task_guard_chirho = t_chirho.lock();
                task_guard_chirho.page_table_root_chirho
            })
            .unwrap_or(None)
    };

    // Queue the CR3 load for the assembly switch path.
    // Switching CR3 here is unsafe because the current Rust frames still live
    // on the old task's kernel stack; if that stack is not mapped in the new PT
    // the resumed task returns to corrupted state or silently disappears.
    if let Some(pt_root_chirho) = new_pt_root_chirho {
        crate::context_switch_chirho::set_pending_cr3_chirho(pt_root_chirho.as_u64());
    } else {
        // Task has no per-process PT — switch back to boot PML4
        // so the task runs in the shared boot address space.
        let boot_pml4_chirho = crate::pagetable_chirho::get_boot_pml4_chirho();
        if boot_pml4_chirho.as_u64() != 0 {
            crate::context_switch_chirho::set_pending_cr3_chirho(boot_pml4_chirho.as_u64());
        } else {
            crate::context_switch_chirho::set_pending_cr3_chirho(0);
        }
    }

    // Switch kernel stack for syscall entry: KERNEL_STACK_TOP_CHIRHO must
    // point to the NEW task's kernel stack so that if the new task enters a
    // syscall, it uses ITS OWN kernel stack (not the old task's).
    // Without this, all tasks share one kernel stack and yield inside
    // syscalls corrupts stack frames.
    {
        let list_chirho = crate::task_chirho::TASK_LIST_CHIRHO.lock();
        if let Some(task_arc_chirho) = list_chirho
            .iter()
            .find(|t_chirho| t_chirho.lock().pid_chirho == next_pid_chirho)
        {
            let kstack_chirho = task_arc_chirho.lock().kernel_stack_chirho;
            if kstack_chirho != 0 {
                crate::syscall_entry_chirho::set_kernel_stack_top_chirho(kstack_chirho);
            }
        }
    }

    // NOTE: FS/GS MSR writes moved to right before switch_context_chirho
    // in schedule_chirho(). Writing user FS base here (before Rust code for
    // TSS/CURRENT_TASK setup) caused Rust stack protector or TLS accesses
    // to read from user memory, corrupting state ~40% of the time on KVM.
    // The new FS base is only written immediately before the asm context
    // switch (and also by fork_child_return for first-time tasks).
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
#[inline(never)]
pub fn schedule_chirho() {
    crate::fb_device_chirho::service_framebuffer_dump_request_chirho();

    // Disable interrupts manually (CLI/STI) instead of using without_interrupts.
    // The without_interrupts closure saves RFLAGS on the stack before calling
    // the closure.  After switch_context returns (to a DIFFERENT invocation of
    // schedule), the saved RFLAGS is at a stack location that belongs to the
    // restored task's schedule call — not the original one.  This mismatch
    // causes without_interrupts to read garbage and crash.
    unsafe { core::arch::asm!("cli", options(nomem, nostack)); }

    let mut scheduler_guard_chirho = SCHEDULER_CHIRHO.lock();
    let scheduler_chirho = match scheduler_guard_chirho.as_mut() {
        Some(s_chirho) => {
            s_chirho
        }
        None => {
            unsafe { core::arch::asm!("sti", options(nomem, nostack)); }
            return;
        }
    };

        // 1. Clear the reschedule flag.
        scheduler_chirho.need_resched_chirho = false;
        NEED_RESCHED_ATOMIC_CHIRHO.store(false, Ordering::Release);

        // 2. If there is a current task, push it to the back of the run queue
        //    so it participates in future rounds — but only if it is still
        //    runnable (not zombie/exited).
        let old_pid_chirho = scheduler_chirho.current_pid_chirho;
        if let Some(pid_chirho) = old_pid_chirho {
            let is_runnable_chirho = crate::task_chirho::is_task_runnable_chirho(pid_chirho);
            if is_runnable_chirho {
                scheduler_chirho.tasks_chirho.push_back(pid_chirho);
            }
            if !is_runnable_chirho && pid_chirho >= 4 {
                let old_state_chirho = task_state_name_for_pid_chirho(pid_chirho);
                crate::serial_println_chirho!(
                    "[SCHED-DROP] PID {} NOT pushed (state={})",
                    pid_chirho,
                    old_state_chirho,
                );
            }
        }

        // 3. Pop the next RUNNABLE task from the front, skipping dead/zombie.
        let queue_len_chirho = scheduler_chirho.tasks_chirho.len();
        let mut next_pid_chirho = None;
        for _ in 0..queue_len_chirho {
            if let Some(candidate_chirho) = scheduler_chirho.tasks_chirho.pop_front() {
                if crate::task_chirho::is_task_runnable_chirho(candidate_chirho) {
                    next_pid_chirho = Some(candidate_chirho);
                    break;
                }
                // Dead/zombie task — discard silently.
                // Trace discarded PID 4 for debugging
                if candidate_chirho == 4 {
                    crate::serial_println_chirho!(
                        "[SCHED-DROP] PID 4 discarded (not runnable) queue_len={}",
                        queue_len_chirho,
                    );
                }
            }
        }

        // Debug: log scheduler decisions when PID 3+ is involved
        if queue_len_chirho > 1 || (next_pid_chirho.is_some() && next_pid_chirho != old_pid_chirho) {
            crate::serial_debug_chirho!(
                "[SCHED] queue_len={} old={:?} next={:?}",
                queue_len_chirho, old_pid_chirho, next_pid_chirho
            );
        }
        trace_scheduler_event_chirho(
            "schedule",
            old_pid_chirho,
            next_pid_chirho,
            &scheduler_chirho.tasks_chirho,
        );

    match next_pid_chirho {
        None => {
            // No runnable tasks. Save the current task's context before
            // entering the idle HLT loop, so it can be properly resumed later.
            // Without this, the idle loop's switch_context_return_wrapper
            // saves to boot_ctx (not the old task's slot), losing the
            // old task's callee-saved registers permanently.
            if let Some(old_pid_chirho) = old_pid_chirho {
                let old_ctx_chirho = crate::task_chirho::context_ptr_mut_chirho(old_pid_chirho);
                let boot_ctx_chirho = crate::task_chirho::boot_context_ptr_chirho();
                if !old_ctx_chirho.is_null() {
                    drop(scheduler_guard_chirho);
                    unsafe {
                        switch_context_return_wrapper_chirho(old_ctx_chirho, boot_ctx_chirho);
                    }
                    // We're now in the boot context. Re-acquire the scheduler.
                    // Fall through to the idle HLT loop below.
                    let mut sg_chirho = SCHEDULER_CHIRHO.lock();
                    let s_chirho = match sg_chirho.as_mut() {
                        Some(s) => s,
                        None => {
                            unsafe { core::arch::asm!("sti", options(nomem, nostack)); }
                            return;
                        }
                    };
                    s_chirho.current_pid_chirho = None;
                    drop(sg_chirho);
                } else {
                    drop(scheduler_guard_chirho);
                }
            } else {
                drop(scheduler_guard_chirho);
            }
            loop {
                x86_64::instructions::interrupts::enable_and_hlt();
                // After HLT, timer handler ran poll_network + schedule_tick.
                // Check if any task became runnable (added to queue by wake_up).
                if let Some(mut guard_chirho) = SCHEDULER_CHIRHO.try_lock() {
                    if let Some(sched_chirho) = guard_chirho.as_mut() {
                        if let Some(pid_chirho) = sched_chirho.tasks_chirho.pop_front() {
                            sched_chirho.current_pid_chirho = Some(pid_chirho);
                            sched_chirho.remaining_ticks_chirho =
                                time_slice_for_pid_chirho(pid_chirho);
                            drop(guard_chirho);
                            // Switch to the newly runnable task
                            arch_prepare_switch_chirho(None, pid_chirho);
                            // Set TSS and current task
                            {
                                let list_chirho = crate::task_chirho::TASK_LIST_CHIRHO.lock();
                                if let Some(task_arc_chirho) = list_chirho
                                    .iter()
                                    .find(|t_chirho| t_chirho.lock().pid_chirho == pid_chirho)
                                {
                                    let kstack_top_chirho = task_arc_chirho.lock().kernel_stack_chirho;
                                    unsafe {
                                        crate::gdt_chirho::set_tss_rsp0_chirho(kstack_top_chirho);
                                    }
                                    // CRITICAL: also update KERNEL_STACK_TOP for SYSCALL entry.
                                    // Without this, the woken task's SYSCALLs use the OLD
                                    // task's kernel stack, corrupting it (e.g., PID 3's
                                    // write result overwrites PID 2's saved context).
                                    crate::syscall_entry_chirho::set_kernel_stack_top_chirho(kstack_top_chirho);
                                }
                                if let Some(task_arc_chirho) = list_chirho.iter()
                                    .find(|t_chirho| t_chirho.lock().pid_chirho == pid_chirho)
                                {
                                    crate::task_chirho::set_current_task_chirho(
                                        alloc::sync::Arc::clone(task_arc_chirho)
                                    );
                                }
                            }
                            // GPT-directed: restore FS/GS base MSRs before context switch.
                            // Without this, musl TLS uses stale FS base → GPF on resume.
                            {
                                use x86_64::registers::model_specific::Msr;
                                let list2_chirho = crate::task_chirho::TASK_LIST_CHIRHO.lock();
                                if let Some(task_chirho) = list2_chirho.iter()
                                    .find(|t| t.lock().pid_chirho == pid_chirho)
                                {
                                    let tg_chirho = task_chirho.lock();
                                    unsafe {
                                        Msr::new(0xC000_0100).write(tg_chirho.fs_base_chirho);
                                        Msr::new(0xC000_0102).write(tg_chirho.gs_base_chirho);
                                    }
                                }
                            }
                            // Context switch to the woken task
                            let new_ctx_ptr_chirho = crate::task_chirho::context_ptr_chirho(pid_chirho);
                            let boot_ctx_ptr_chirho = crate::task_chirho::boot_context_ptr_chirho();
                            unsafe {
                                switch_context_return_wrapper_chirho(boot_ctx_ptr_chirho, new_ctx_ptr_chirho);
                            }
                            return;
                        }
                    }
                }
            }
        }
        Some(next_chirho) => {
                // 4. If the next task is the same as the old one, no context
                //    switch is necessary — just reset the time slice.
            if old_pid_chirho == Some(next_chirho) {
                scheduler_chirho.current_pid_chirho = Some(next_chirho);
                scheduler_chirho.remaining_ticks_chirho =
                    time_slice_for_pid_chirho(next_chirho);
                unsafe { core::arch::asm!("sti", options(nomem, nostack)); }
                return;
            }

                // 5. Different task — perform a context switch.
                scheduler_chirho.current_pid_chirho = Some(next_chirho);
            scheduler_chirho.remaining_ticks_chirho =
                time_slice_for_pid_chirho(next_chirho);

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

                // Drop the scheduler lock before performing the actual context
                // switch so that the new task can acquire it if needed.
                drop(scheduler_guard_chirho);

                // Save old task's FS/GS base and switch CR3.
                // CRITICAL: Do NOT write the new task's FS base yet!
                // The Rust code below (TSS setup, CURRENT_TASK update) might
                // use FS-relative addressing (stack protector, TLS). Writing
                // the child's user FS base here would cause kernel code to
                // read from user memory, corrupting state ~40% of the time.
                arch_prepare_switch_chirho(old_pid_chirho, next_chirho);

                // Set kernel stack + current task for the new task.
                {
                    let list_chirho = crate::task_chirho::TASK_LIST_CHIRHO.lock();
                    if let Some(task_arc_chirho) = list_chirho
                        .iter()
                        .find(|t_chirho| t_chirho.lock().pid_chirho == next_chirho)
                    {
                        let kstack_top_chirho = task_arc_chirho.lock().kernel_stack_chirho;
                        unsafe {
                            crate::gdt_chirho::set_tss_rsp0_chirho(kstack_top_chirho);
                        }
                        crate::syscall_entry_chirho::set_kernel_stack_top_chirho(kstack_top_chirho);

                        // FD table swap temporarily disabled to test if it causes
                        // the context switch #UD crash. The lookup_fd_chirho
                        // fallback in VFS handles per-task fds without swapping.

                        // NOW update CURRENT_TASK.
                        crate::task_chirho::set_current_task_chirho(
                            alloc::sync::Arc::clone(task_arc_chirho),
                        );
                    }
                }

                if old_ctx_ptr_chirho.is_null() || new_ctx_ptr_chirho.is_null() {
                    return;
                }
                unsafe {
                    let new_rip_chirho = (*new_ctx_ptr_chirho).rip_chirho;
                    let new_rsp_chirho = (*new_ctx_ptr_chirho).rsp_chirho;
                    // Verify new RIP is in kernel code range (0x10000...)
                    // For RESUMED tasks, validate the return address at saved RSP.
                    // First-time tasks have a zero-filled stack — detect by checking
                    // if the value at RSP is 0 (entry point is in rip, not on stack).
                    // Validate the saved context is reasonable.
                    // Kernel code can be at various addresses depending on
                    // the bootloader and QEMU mode (TCG vs KVM):
                    //   TCG:  0x1_0000_0000_0000 range (PML4[2])
                    //   KVM:  0x0000_8000_0000 range (lower address)
                    // Validate RIP is in kernel code range and RSP is in
                    // kernel stack range. Do NOT check *RSP — after switch_context
                    // saves, *RSP is the last pushed callee-saved register (R15),
                    // which can be any value (e.g., 0x17 = write return value).
                    let rip_ok_chirho = new_rip_chirho > 0x80_0000_0000
                        || new_rip_chirho == 0; // zero for first-time dispatch
                    let rsp_ok_chirho = new_rsp_chirho >= 0xFFFF_8000_0000_0000
                        || (new_rsp_chirho >= 0x4000_0000_0000
                            && new_rsp_chirho < 0x8000_0000_0000);

                    if !rip_ok_chirho || !rsp_ok_chirho {
                        crate::serial_println_chirho!(
                            "[SCHED] ABORT switch {:?}->{}: rip={:#x} rsp={:#x}",
                            old_pid_chirho, next_chirho, new_rip_chirho, new_rsp_chirho,
                        );
                        unsafe { core::arch::asm!("sti", options(nomem, nostack)); }
                        return;
                    }
                    crate::serial_debug_chirho!(
                        "[SCHED] switch {:?}->{}: rip={:#x} rsp={:#x}",
                        old_pid_chirho, next_chirho, new_rip_chirho, new_rsp_chirho,
                    );
                    // NOW write the new task's FS/GS base, right before
                    // the asm context switch. No more Rust code runs after
                    // this until we're in the new task's context.
                    {
                        use x86_64::registers::model_specific::Msr;
                        let list_chirho = crate::task_chirho::TASK_LIST_CHIRHO.lock();
                        if let Some(task_chirho) = list_chirho.iter()
                            .find(|t| t.lock().pid_chirho == next_chirho)
                        {
                            let tg_chirho = task_chirho.lock();
                            let new_fs_chirho = tg_chirho.fs_base_chirho;
                            unsafe {
                                Msr::new(0xC000_0100).write(new_fs_chirho);
                                Msr::new(0xC000_0102).write(tg_chirho.gs_base_chirho);
                            }
                            // FS-WRITE trace removed (was flooding log on every PID 2 context switch)
                        }
                    }
                    switch_context_return_wrapper_chirho(
                        old_ctx_ptr_chirho,
                        new_ctx_ptr_chirho,
                    );
                    // After the wrapper returns we are back on the restored
                    // task's stack and interrupts have already been re-enabled.
                    return;
            }
        }
    }
    unsafe { core::arch::asm!("sti", options(nomem, nostack)); }
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
    let tick_count_chirho = GLOBAL_TICK_COUNT_CHIRHO.fetch_add(1, Ordering::Relaxed) + 1;

    // Try to acquire the scheduler lock.  If it is already held (e.g. the
    // timer fired while `schedule_chirho` was in progress), skip this tick
    // rather than deadlocking.
    let maybe_guard_chirho = SCHEDULER_CHIRHO.try_lock();
    let mut scheduler_guard_chirho = match maybe_guard_chirho {
        Some(guard_chirho) => guard_chirho,
        None => {
            let miss_count_chirho =
                TICK_LOCK_MISS_COUNTER_CHIRHO.fetch_add(1, Ordering::Relaxed);
            if miss_count_chirho < 16 {
                crate::serial_println_chirho!(
                    "[TICK-SKIP] tick={} sched_lock_busy miss={}",
                    tick_count_chirho,
                    miss_count_chirho,
                );
            }
            return; // Lock contention — skip this tick.
        }
    };

    let scheduler_chirho = match scheduler_guard_chirho.as_mut() {
        Some(s_chirho) => s_chirho,
        None => return, // Scheduler not yet initialised.
    };

    // Keep the local tick counter in sync.
    scheduler_chirho.tick_count_chirho =
        GLOBAL_TICK_COUNT_CHIRHO.load(Ordering::Relaxed);

    // Only meaningful if a task is currently running.
    if let Some(pid_chirho) = scheduler_chirho.current_pid_chirho {
        let remaining_before_chirho = scheduler_chirho.remaining_ticks_chirho;
        let need_resched_before_chirho = scheduler_chirho.need_resched_chirho;
        scheduler_chirho.remaining_ticks_chirho =
            scheduler_chirho.remaining_ticks_chirho.saturating_sub(1);

        if scheduler_chirho.remaining_ticks_chirho == 0 {
            scheduler_chirho.need_resched_chirho = true;
            NEED_RESCHED_ATOMIC_CHIRHO.store(true, Ordering::Release);
        }

        if pid_chirho == 4 {
            let trace_count_chirho =
                TICK_TRACE_COUNTER_CHIRHO.fetch_add(1, Ordering::Relaxed);
            if trace_count_chirho < 64 {
                crate::serial_debug_chirho!(
                    "[TICK-TRACE] tick={} pid={} before={} after={} need_before={} need_after={} current={:?}",
                    tick_count_chirho,
                    pid_chirho,
                    remaining_before_chirho,
                    scheduler_chirho.remaining_ticks_chirho,
                    need_resched_before_chirho,
                    scheduler_chirho.need_resched_chirho,
                    scheduler_chirho.current_pid_chirho,
                );
            }
        }
    } else {
        let idle_count_chirho =
            TICK_IDLE_TRACE_COUNTER_CHIRHO.fetch_add(1, Ordering::Relaxed);
        if idle_count_chirho < 16 {
            crate::serial_println_chirho!(
                "[TICK-IDLE] tick={} no_current_pid cnt={}",
                tick_count_chirho,
                idle_count_chirho,
            );
        }
    }
}

/// Reset the current task's time slice and clear need_resched.
///
/// Called after blocking syscalls (select, poll, etc.) return.
/// These syscalls cooperatively yield via HLT loops, consuming the
/// time slice. Without reset, the NEXT non-blocking syscall (accept,
/// read, etc.) would trigger a context switch that crashes.
pub fn reset_time_slice_chirho() {
    NEED_RESCHED_ATOMIC_CHIRHO.store(false, Ordering::Release);
    if let Some(mut guard_chirho) = SCHEDULER_CHIRHO.try_lock() {
        if let Some(ref mut sched_chirho) = *guard_chirho {
            sched_chirho.remaining_ticks_chirho = sched_chirho
                .current_pid_chirho
                .map(time_slice_for_pid_chirho)
                .unwrap_or(DEFAULT_TIME_SLICE_CHIRHO);
            sched_chirho.need_resched_chirho = false;
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

        // Push to FRONT so newly forked children run before older tasks.
        // Without this, the shell's wait4 loop monopolizes the CPU and
        // the fork child (SSH handler) never gets scheduled.
        scheduler_chirho.tasks_chirho.push_front(pid_chirho);
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

        // If the task is the currently running one, DON'T clear current_pid.
        // The task is still executing (e.g., running its exit path). Clearing
        // current_pid would cause schedule_chirho to save the context to the
        // boot context instead of the task's CpuContextChirho, corrupting
        // the next task's restore. Instead, just set need_resched so the
        // scheduler will switch away from this task at the next yield.
        if scheduler_chirho.current_pid_chirho == Some(pid_chirho) {
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

        // Read the current PID but do NOT clear current_pid yet.
        // schedule_chirho() needs current_pid set to save the task's
        // CpuContext to its slot. If we take() here, schedule sees
        // old_pid=None and skips the context save — PID 2's slot
        // retains the initial dispatch RIP, causing it to resume
        // via fork child return (IRETQ without FS/GS restore).
        let pid_chirho = scheduler_chirho.current_pid_chirho;

        if let Some(pid_val_chirho) = pid_chirho {
            // Mark the task as sleeping so schedule doesn't push it
            // back to the run queue (it's blocked until unblocked).
            let list_chirho = crate::task_chirho::TASK_LIST_CHIRHO.lock();
            if let Some(task_chirho) = list_chirho.iter()
                .find(|t| t.lock().pid_chirho == pid_val_chirho)
            {
                task_chirho.lock().state_chirho =
                    crate::task_chirho::TaskStateChirho::SleepingChirho;
            }
            drop(list_chirho);

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
    // Set the task state back to Ready before adding to the run queue.
    // block_current_chirho sets it to SleepingChirho.
    {
        let list_chirho = crate::task_chirho::TASK_LIST_CHIRHO.lock();
        if let Some(task_chirho) = list_chirho.iter()
            .find(|t| t.lock().pid_chirho == pid_chirho)
        {
            task_chirho.lock().state_chirho =
                crate::task_chirho::TaskStateChirho::ReadyChirho;
        }
    }
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
            trace_scheduler_event_chirho(
                "yield",
                scheduler_chirho.current_pid_chirho,
                None,
                &scheduler_chirho.tasks_chirho,
            );
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

/// Set the need-resched flag (stub for compatibility).
pub fn set_need_resched_chirho() {
    NEED_RESCHED_ATOMIC_CHIRHO.store(true, Ordering::Release);
}

/// Check if there are other runnable tasks in the run queue.
///
/// Used by blocking syscalls (select) to yield CPU to fork children
/// that need to run. Without this, cooperative scheduling means the
/// parent monopolizes the CPU in the HLT loop.
pub fn has_runnable_tasks_chirho() -> bool {
    if let Some(guard_chirho) = SCHEDULER_CHIRHO.try_lock() {
        if let Some(ref sched_chirho) = *guard_chirho {
            // Check if there are ANY other tasks in the run queue.
            // The current task is NOT in the queue (it was popped),
            // so len > 0 means at least one other task can run.
            return !sched_chirho.tasks_chirho.is_empty();
        }
    }
    false
}

/// Move a specific PID to the front of the run queue.
/// Used by wait4 to ensure the child task gets scheduled next.
pub fn promote_task_chirho(pid_chirho: u64) {
    x86_64::instructions::interrupts::without_interrupts(|| {
        let mut guard_chirho = SCHEDULER_CHIRHO.lock();
        if let Some(ref mut sched_chirho) = *guard_chirho {
            // Remove pid from wherever it is in the queue
            if let Some(pos_chirho) = sched_chirho.tasks_chirho.iter().position(|&p| p == pid_chirho) {
                sched_chirho.tasks_chirho.remove(pos_chirho);
                sched_chirho.tasks_chirho.push_front(pid_chirho);
                trace_scheduler_event_chirho(
                    "promote",
                    sched_chirho.current_pid_chirho,
                    Some(pid_chirho),
                    &sched_chirho.tasks_chirho,
                );
            }
        }
    });
}

/// Return the total number of tasks (current + queued).
pub fn task_count_chirho() -> usize {
    if let Some(guard_chirho) = SCHEDULER_CHIRHO.try_lock() {
        if let Some(ref sched_chirho) = *guard_chirho {
            let queued_chirho = sched_chirho.tasks_chirho.len();
            let current_chirho = if sched_chirho.current_pid_chirho.is_some() { 1 } else { 0 };
            return queued_chirho + current_chirho;
        }
    }
    0
}

/// Set the current running PID.  Called during boot to register PID 0
/// with the scheduler so it participates in scheduling (gets pushed to
/// the run queue when yielding).
pub fn set_current_pid_chirho(pid_chirho: u64) {
    x86_64::instructions::interrupts::without_interrupts(|| {
        let mut scheduler_guard_chirho = SCHEDULER_CHIRHO.lock();
        if let Some(scheduler_chirho) = scheduler_guard_chirho.as_mut() {
            scheduler_chirho.current_pid_chirho = Some(pid_chirho);
        }
    });
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
