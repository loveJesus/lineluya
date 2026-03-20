// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! Task/process descriptor module for the Lineluya kernel.
//!
//! This module is the Rust equivalent of Linux's `struct task_struct`. It
//! provides:
//!
//! - [`TaskStateChirho`] — the possible lifecycle states of a task.
//! - [`CpuContextChirho`] — saved x86_64 callee-saved registers for context
//!   switching.
//! - [`TaskChirho`] — the main process/thread descriptor, carrying scheduling,
//!   memory, identity, signal, and TLS metadata.
//! - Global task tracking via [`CURRENT_TASK_CHIRHO`] and [`TASK_LIST_CHIRHO`].
//! - PID allocation via an atomic counter ([`allocate_pid_chirho`]).
//! - Factory methods for kernel and user tasks ([`TaskChirho::new_kernel_chirho`],
//!   [`TaskChirho::new_user_chirho`]).
//! - [`init_tasking_chirho`] — bootstrap the tasking subsystem (PID 0 idle +
//!   PID 1 init).

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;
use x86_64::PhysAddr;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Default kernel stack size (16 KiB).  Chosen to match the typical Linux
/// kernel thread stack while remaining modest enough for early boot.
/// Kernel stack size per task. 64KB to handle deep call chains
/// (e.g., dropbear SSH crypto: curve25519 + chacha20 + poly1305).
pub const KERNEL_STACK_SIZE_CHIRHO: usize = 64 * 1024;
const DEFAULT_KERNEL_STACK_SIZE_CHIRHO: usize = KERNEL_STACK_SIZE_CHIRHO;

/// Default time slice for newly created tasks (in timer ticks).
const DEFAULT_TIME_SLICE_CHIRHO: u64 = 10;

/// Default scheduling priority for normal tasks (higher = lower priority,
/// mirroring Linux's `nice` semantics in simplified form).
const DEFAULT_PRIORITY_CHIRHO: i32 = 0;

/// Maximum length of a task's command/name (`comm_chirho`), matching Linux's
/// `TASK_COMM_LEN`.
const TASK_COMM_LEN_CHIRHO: usize = 16;

// ---------------------------------------------------------------------------
// PID allocator
// ---------------------------------------------------------------------------

/// Monotonically increasing PID counter.  PID 0 is reserved for the idle
/// (swapper) task and PID 1 for init, so the counter starts at 0 and the
/// first two values are consumed by [`init_tasking_chirho`].
static NEXT_PID_CHIRHO: AtomicU64 = AtomicU64::new(0);

/// Allocate a fresh, globally unique PID.
///
/// PIDs are never recycled in this simplified implementation.  A production
/// kernel would maintain a bitmap or free-list for reuse.
pub fn allocate_pid_chirho() -> u64 {
    NEXT_PID_CHIRHO.fetch_add(1, Ordering::Relaxed)
}

// ---------------------------------------------------------------------------
// TaskStateChirho
// ---------------------------------------------------------------------------

/// Lifecycle states of a task.
///
/// Each variant represents a distinct point in the task lifecycle:
///
/// - **`RunningChirho`** -- The task is actively executing instructions on a
///   CPU core.  At most one task per core can be in this state.
///
/// - **`ReadyChirho`** -- The task is runnable and sitting in the scheduler's
///   run queue, waiting for a CPU to become available.
///
/// - **`SleepingChirho`** -- The task is in an interruptible sleep.  It is
///   waiting for an event (e.g., data arriving on a pipe, a timer expiring)
///   and can be woken by a signal.  Corresponds to Linux `TASK_INTERRUPTIBLE`.
///
/// - **`BlockedChirho`** -- The task is in an uninterruptible wait, typically
///   for disk I/O, a lock, or a futex.  Signals do not wake it.  Corresponds
///   to Linux `TASK_UNINTERRUPTIBLE`.
///
/// - **`StoppedChirho`** -- The task has been stopped by a signal (`SIGSTOP`,
///   `SIGTSTP`) or by a ptrace attach.  It will not be scheduled until
///   continued with `SIGCONT`.
///
/// - **`ZombieChirho`** -- The task has called `exit()` (or was killed) but
///   its parent has not yet collected the exit status via `wait()`/`waitpid()`.
///
/// - **`DeadChirho`** -- The task has been reaped and fully cleaned up.  The
///   descriptor may be freed.
///
/// Transitions follow the standard Unix model:
///
/// ```text
///                  schedule
///   ReadyChirho ──────────► RunningChirho
///        ▲                       │
///        │  wake                 │ block / sleep / yield
///        │                      ▼
///        ├──────────────── BlockedChirho
///        │
///        ├──────────────── SleepingChirho  (signal or event wakes)
///        │
///        │  SIGCONT            SIGSTOP / ptrace
///        ├──────────────── StoppedChirho ◄──── RunningChirho
///        │                                │
///        │                        exit()  │
///        │                                ▼
///        └───────────────── ZombieChirho ──► DeadChirho
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStateChirho {
    /// Currently executing on a CPU.
    RunningChirho,
    /// Runnable and waiting in the scheduler's run queue.
    ReadyChirho,
    /// Interruptible sleep — waiting for an event; can be woken by a signal.
    SleepingChirho,
    /// Blocked on I/O, a lock, a futex, or another event (uninterruptible).
    BlockedChirho,
    /// Stopped by a signal (SIGSTOP/SIGTSTP) or ptrace; resumed by SIGCONT.
    StoppedChirho,
    /// Exited but not yet reaped by a parent `wait()` / `waitpid()`.
    ZombieChirho,
    /// Fully cleaned up; the descriptor may be freed.
    DeadChirho,
}

impl core::fmt::Display for TaskStateChirho {
    fn fmt(&self, f_chirho: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::RunningChirho => write!(f_chirho, "Running"),
            Self::ReadyChirho => write!(f_chirho, "Ready"),
            Self::SleepingChirho => write!(f_chirho, "Sleeping"),
            Self::BlockedChirho => write!(f_chirho, "Blocked"),
            Self::StoppedChirho => write!(f_chirho, "Stopped"),
            Self::ZombieChirho => write!(f_chirho, "Zombie"),
            Self::DeadChirho => write!(f_chirho, "Dead"),
        }
    }
}

// ---------------------------------------------------------------------------
// CpuContextChirho
// ---------------------------------------------------------------------------

/// Saved CPU register state for context switching.
///
/// Only the **callee-saved** registers (plus the instruction pointer, stack
/// pointer, and flags) need to be preserved across a context switch because
/// the System V AMD64 ABI guarantees caller-saved registers are already
/// spilled by the compiler.
///
/// `#[repr(C)]` ensures a predictable field layout so that assembly context-
/// switch stubs can load/store registers at known offsets.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct CpuContextChirho {
    /// Stack pointer (`rsp`).
    pub rsp_chirho: u64,
    /// Base pointer (`rbp`).
    pub rbp_chirho: u64,
    /// General-purpose callee-saved register `rbx`.
    pub rbx_chirho: u64,
    /// General-purpose callee-saved register `r12`.
    pub r12_chirho: u64,
    /// General-purpose callee-saved register `r13`.
    pub r13_chirho: u64,
    /// General-purpose callee-saved register `r14`.
    pub r14_chirho: u64,
    /// General-purpose callee-saved register `r15`.
    pub r15_chirho: u64,
    /// Instruction pointer (`rip`) — the address to resume execution at.
    pub rip_chirho: u64,
    /// Flags register (`rflags`).
    pub rflags_chirho: u64,
    // NOTE: FPU/SSE/AVX state (fxsave area) would be stored here in a full
    // implementation.  Omitted for Phase 2 to keep the struct lean.
}

impl CpuContextChirho {
    /// Return a zeroed-out context.  Useful as a starting point before
    /// populating `rip_chirho` and `rsp_chirho` for a new task.
    pub const fn zero_chirho() -> Self {
        Self {
            rsp_chirho: 0,
            rbp_chirho: 0,
            rbx_chirho: 0,
            r12_chirho: 0,
            r13_chirho: 0,
            r14_chirho: 0,
            r15_chirho: 0,
            rip_chirho: 0,
            rflags_chirho: 0,
        }
    }
}

impl Default for CpuContextChirho {
    fn default() -> Self {
        Self::zero_chirho()
    }
}

// ---------------------------------------------------------------------------
// TaskChirho
// ---------------------------------------------------------------------------

/// The main process/thread descriptor.
///
/// Analogous to Linux's `struct task_struct`, this struct carries all the
/// per-task metadata the kernel needs for scheduling, context switching,
/// memory management, signal handling, and identity (uid/gid).
///
/// ## Design notes
///
/// * Fields are intentionally public for Phase 2.  A future refactor will
///   move mutation behind accessor methods once the APIs stabilize.
/// * File descriptors are tracked only as a next-fd counter for now; the
///   actual fd table will live in a separate module.
/// * The `comm_chirho` field is a fixed-size byte array (not a `String`) to
///   avoid heap allocation in the descriptor itself and to match Linux's
///   `TASK_COMM_LEN`.
pub struct TaskChirho {
    // -- Identity / hierarchy -----------------------------------------------

    /// Process ID.
    pub pid_chirho: u64,
    /// Thread group ID.  Equal to `pid_chirho` for the main thread of a
    /// process; threads created via `clone()` share the leader's `tgid`.
    pub tgid_chirho: u64,
    /// Parent process ID.
    pub ppid_chirho: u64,

    // -- State & exit -------------------------------------------------------

    /// Current lifecycle state.
    pub state_chirho: TaskStateChirho,
    /// Exit code set by `exit()` / a fatal signal.  Only meaningful when
    /// `state_chirho` is [`TaskStateChirho::ZombieChirho`] or
    /// [`TaskStateChirho::DeadChirho`].
    pub exit_code_chirho: i32,

    // -- CPU context --------------------------------------------------------

    /// Saved register state, written during a context switch out and restored
    /// on the way back in.
    pub context_chirho: CpuContextChirho,

    // -- Memory -------------------------------------------------------------

    /// Virtual address of the *top* (highest address) of this task's kernel
    /// stack.  The stack grows downward.
    pub kernel_stack_chirho: u64,
    /// Size of the kernel stack in bytes.
    pub kernel_stack_size_chirho: usize,
    /// Saved user-space stack pointer, stored on entry to the kernel (via
    /// syscall or interrupt) and restored on return.
    pub user_rsp_chirho: u64,
    /// Saved user RIP when preempted by the timer trampoline. The
    /// trampoline redirects to a sched_yield SYSCALL; the original RIP
    /// is restored by the sched_yield handler before SYSRET.
    pub preempted_rip_chirho: u64,

    // -- Per-process page table ----------------------------------------------

    /// Physical address of this process's PML4 (top-level page table).
    /// `None` for kernel tasks that share the kernel page tables.
    /// User tasks get their own PML4 allocated at creation/exec time.
    pub page_table_root_chirho: Option<PhysAddr>,

    // -- File descriptors ----------------------------------------------------

    /// The next file descriptor number to hand out.  A proper fd table will
    /// replace this once the VFS layer is in place.
    pub next_fd_chirho: usize,

    /// Per-process file descriptor table.  `None` for kernel tasks that do
    /// not interact with the VFS.  User tasks get a table allocated at
    /// creation time; fork() duplicates it for the child.
    pub fd_table_chirho: Option<crate::vfs_chirho::FdTableChirho>,

    /// Current working directory (absolute path).
    pub cwd_chirho: alloc::string::String,

    // -- Scheduling ---------------------------------------------------------

    /// Scheduling priority.  Lower values indicate higher priority (like
    /// Linux's real-time priorities).  Normal tasks start at
    /// [`DEFAULT_PRIORITY_CHIRHO`].
    pub priority_chirho: i32,
    /// Remaining time-slice in timer ticks.  Decremented on each tick; when
    /// it reaches zero the scheduler preempts this task.
    pub time_slice_chirho: u64,

    // -- Identity (credentials) ---------------------------------------------

    /// Real user ID.
    pub uid_chirho: u32,
    /// Real group ID.
    pub gid_chirho: u32,
    /// Effective user ID (used for permission checks).
    pub euid_chirho: u32,
    /// Effective group ID.
    pub egid_chirho: u32,
    /// Saved user ID used by setuid-family syscalls.
    pub saved_uid_chirho: u32,
    /// Saved group ID used by setgid-family syscalls.
    pub saved_gid_chirho: u32,
    /// Supplementary groups for the task/thread group.
    pub supplementary_groups_chirho: Vec<u32>,

    // -- Name ---------------------------------------------------------------

    /// Process command name, stored as a fixed-size UTF-8-compatible byte
    /// array.  Unused trailing bytes are zero-filled.
    pub comm_chirho: [u8; TASK_COMM_LEN_CHIRHO],

    // -- TLS ----------------------------------------------------------------

    /// FS segment base, typically set by `arch_prctl(ARCH_SET_FS, ...)` for
    /// thread-local storage.
    pub fs_base_chirho: u64,
    /// GS segment base, used by the kernel for per-CPU data.
    pub gs_base_chirho: u64,

    // -- Signals (simplified) -----------------------------------------------

    /// Bitmask of blocked signals.  Bit *n* set means signal *n* is blocked.
    pub signal_mask_chirho: u64,
    /// Bitmask of pending (not yet delivered) signals.
    pub pending_signals_chirho: u64,

    // -- Signals (full state, Phase 4) --------------------------------------

    /// Full per-task signal state including per-signal dispositions, blocked
    /// mask, and pending queue.
    pub signal_state_chirho: crate::signal_chirho::SignalStateChirho,

    // -- Program break (brk) ------------------------------------------------

    /// Current program break address (top of the heap).  Adjusted by the
    /// `brk` syscall.
    pub brk_chirho: u64,
    /// Initial program break set when the binary was loaded.  The kernel
    /// refuses `brk` requests below this address.
    pub brk_start_chirho: u64,
    /// Session ID (PID of session leader).
    pub sid_chirho: u64,
    /// Process group ID.
    pub pgid_chirho: u64,
    /// Controlling terminal PTY number, or `None`.
    pub controlling_tty_chirho: Option<u32>,
}

impl TaskChirho {
    // -- Factory helpers ----------------------------------------------------

    /// Create a new **kernel** task.
    ///
    /// Kernel tasks run in ring 0 with no user-space mappings.  The returned
    /// task is in [`TaskStateChirho::ReadyChirho`] and ready to be placed on a
    /// run queue.
    ///
    /// # Arguments
    ///
    /// * `name_chirho`        — human-readable name (truncated to
    ///   [`TASK_COMM_LEN_CHIRHO`] bytes).
    /// * `entry_point_chirho` — function pointer where execution will begin
    ///   when this task is first scheduled.
    ///
    /// # Panics
    ///
    /// Panics if the kernel stack allocation fails (out of heap memory).
    pub fn new_kernel_chirho(
        name_chirho: &str,
        entry_point_chirho: fn() -> !,
    ) -> Self {
        let pid_chirho = allocate_pid_chirho();
        let stack_chirho = allocate_kernel_stack_chirho(DEFAULT_KERNEL_STACK_SIZE_CHIRHO);
        let stack_top_chirho = stack_chirho + DEFAULT_KERNEL_STACK_SIZE_CHIRHO as u64;

        let mut context_chirho = CpuContextChirho::zero_chirho();
        context_chirho.rip_chirho = entry_point_chirho as u64;
        context_chirho.rsp_chirho = stack_top_chirho;
        // Enable interrupts in the saved flags so that the task can be
        // preempted once it starts running.
        context_chirho.rflags_chirho = 0x200; // IF (Interrupt Flag)

        Self {
            pid_chirho,
            tgid_chirho: pid_chirho,
            ppid_chirho: 0,
            state_chirho: TaskStateChirho::ReadyChirho,
            exit_code_chirho: 0,
            context_chirho,
            kernel_stack_chirho: stack_top_chirho,
            kernel_stack_size_chirho: DEFAULT_KERNEL_STACK_SIZE_CHIRHO,
            user_rsp_chirho: 0,
            preempted_rip_chirho: 0,
            page_table_root_chirho: None, // kernel tasks share the kernel page tables
            next_fd_chirho: 0,
            fd_table_chirho: None,
            priority_chirho: DEFAULT_PRIORITY_CHIRHO,
            time_slice_chirho: DEFAULT_TIME_SLICE_CHIRHO,
            uid_chirho: 0,
            gid_chirho: 0,
            euid_chirho: 0,
            egid_chirho: 0,
            saved_uid_chirho: 0,
            saved_gid_chirho: 0,
            supplementary_groups_chirho: Vec::new(),
            comm_chirho: make_comm_chirho(name_chirho),
            fs_base_chirho: 0,
            gs_base_chirho: 0,
            signal_mask_chirho: 0,
            pending_signals_chirho: 0,
            signal_state_chirho: crate::signal_chirho::SignalStateChirho::new_chirho(),
            brk_chirho: 0,
            brk_start_chirho: 0,
            cwd_chirho: alloc::string::String::from("/"),
            sid_chirho: pid_chirho,
            pgid_chirho: pid_chirho,
            controlling_tty_chirho: None,
        }
    }

    /// Create a new **user-space** task.
    ///
    /// User tasks will eventually transition to ring 3 via `sysretq` or
    /// `iretq`.  The caller must supply a user-space stack pointer.
    ///
    /// # Arguments
    ///
    /// * `name_chirho`       — human-readable name.
    /// * `entry_point_chirho` — user-mode entry address (e.g., `_start`).
    /// * `user_stack_chirho`  — top of the user-space stack.
    pub fn new_user_chirho(
        name_chirho: &str,
        entry_point_chirho: u64,
        user_stack_chirho: u64,
    ) -> Self {
        let pid_chirho = allocate_pid_chirho();
        let stack_chirho = allocate_kernel_stack_chirho(DEFAULT_KERNEL_STACK_SIZE_CHIRHO);
        let stack_top_chirho = stack_chirho + DEFAULT_KERNEL_STACK_SIZE_CHIRHO as u64;

        let mut context_chirho = CpuContextChirho::zero_chirho();
        // For a user task the `rip` in the saved context points to the
        // kernel-side trampoline that will `sysretq`/`iretq` to user space.
        // For now we store the user entry point directly; the actual
        // ring-3-transition mechanism will be implemented in the scheduler /
        // syscall module.
        context_chirho.rip_chirho = entry_point_chirho;
        context_chirho.rsp_chirho = stack_top_chirho;
        context_chirho.rflags_chirho = 0x200; // IF

        // Allocate a per-process page table for user tasks.
        let pt_root_chirho = crate::pagetable_chirho::create_user_page_table_chirho();

        Self {
            pid_chirho,
            tgid_chirho: pid_chirho,
            ppid_chirho: 0,
            state_chirho: TaskStateChirho::ReadyChirho,
            exit_code_chirho: 0,
            context_chirho,
            kernel_stack_chirho: stack_top_chirho,
            kernel_stack_size_chirho: DEFAULT_KERNEL_STACK_SIZE_CHIRHO,
            user_rsp_chirho: user_stack_chirho,
            preempted_rip_chirho: 0,
            page_table_root_chirho: pt_root_chirho,
            next_fd_chirho: 3, // 0=stdin, 1=stdout, 2=stderr pre-allocated
            fd_table_chirho: Some(crate::vfs_chirho::FdTableChirho::new_chirho(256)),
            priority_chirho: DEFAULT_PRIORITY_CHIRHO,
            time_slice_chirho: DEFAULT_TIME_SLICE_CHIRHO,
            // User tasks start as root for now; a proper credential model
            // will set these from the parent or from exec.
            uid_chirho: 0,
            gid_chirho: 0,
            euid_chirho: 0,
            egid_chirho: 0,
            saved_uid_chirho: 0,
            saved_gid_chirho: 0,
            supplementary_groups_chirho: Vec::new(),
            comm_chirho: make_comm_chirho(name_chirho),
            fs_base_chirho: 0,
            gs_base_chirho: 0,
            signal_mask_chirho: 0,
            pending_signals_chirho: 0,
            signal_state_chirho: crate::signal_chirho::SignalStateChirho::new_chirho(),
            brk_chirho: 0,
            brk_start_chirho: 0,
            cwd_chirho: alloc::string::String::from("/"),
            sid_chirho: pid_chirho,
            pgid_chirho: pid_chirho,
            controlling_tty_chirho: None,
        }
    }

    // -- Accessors ----------------------------------------------------------

    /// Return the task's name as a `&str`, trimming NUL padding.
    pub fn name_chirho(&self) -> &str {
        let len_chirho = self
            .comm_chirho
            .iter()
            .position(|&b_chirho| b_chirho == 0)
            .unwrap_or(TASK_COMM_LEN_CHIRHO);
        // The name is always written from valid UTF-8 (`&str`), so this is
        // safe.  We fall back to "<invalid>" if something has corrupted it.
        core::str::from_utf8(&self.comm_chirho[..len_chirho]).unwrap_or("<invalid>")
    }

    /// Return `true` if this task is in a runnable state (Running or Ready).
    pub fn is_runnable_chirho(&self) -> bool {
        matches!(
            self.state_chirho,
            TaskStateChirho::RunningChirho | TaskStateChirho::ReadyChirho
        )
    }

    /// Return `true` if this task has exited (Zombie or Dead).
    pub fn is_exited_chirho(&self) -> bool {
        matches!(
            self.state_chirho,
            TaskStateChirho::ZombieChirho | TaskStateChirho::DeadChirho
        )
    }

    /// Mark this task as blocked.
    pub fn block_chirho(&mut self) {
        self.state_chirho = TaskStateChirho::BlockedChirho;
    }

    /// Wake this task, moving it from Blocked to Ready.
    ///
    /// No-op if the task is not currently blocked.
    pub fn wake_chirho(&mut self) {
        if self.state_chirho == TaskStateChirho::BlockedChirho {
            self.state_chirho = TaskStateChirho::ReadyChirho;
        }
    }

    /// Terminate this task with the given exit code, transitioning it to
    /// Zombie state.  The parent must call `wait()` to move it to Dead.
    pub fn exit_chirho(&mut self, code_chirho: i32) {
        self.exit_code_chirho = code_chirho;
        self.state_chirho = TaskStateChirho::ZombieChirho;
    }

    /// Reap a zombie task, transitioning it to Dead.
    ///
    /// Returns the exit code, or `None` if the task is not a zombie.
    pub fn reap_chirho(&mut self) -> Option<i32> {
        if self.state_chirho == TaskStateChirho::ZombieChirho {
            self.state_chirho = TaskStateChirho::DeadChirho;
            Some(self.exit_code_chirho)
        } else {
            None
        }
    }

    /// Reset the time slice to the default value.  Called by the scheduler
    /// when the task is re-enqueued.
    pub fn reset_time_slice_chirho(&mut self) {
        self.time_slice_chirho = DEFAULT_TIME_SLICE_CHIRHO;
    }
}

impl core::fmt::Debug for TaskChirho {
    fn fmt(&self, f_chirho: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f_chirho
            .debug_struct("TaskChirho")
            .field("pid_chirho", &self.pid_chirho)
            .field("tgid_chirho", &self.tgid_chirho)
            .field("ppid_chirho", &self.ppid_chirho)
            .field("state_chirho", &self.state_chirho)
            .field("comm_chirho", &self.name_chirho())
            .field("priority_chirho", &self.priority_chirho)
            .field("time_slice_chirho", &self.time_slice_chirho)
            .finish()
    }
}

// ---------------------------------------------------------------------------
// Global task tracking
// ---------------------------------------------------------------------------

/// The currently running task on this CPU.
///
/// In a multi-core system each CPU would have its own `current` pointer
/// (typically stored in a per-CPU area accessible via the GS segment base).
/// For the single-CPU Phase 2 kernel we use a global mutex-protected
/// `Option`.
///
/// The `Arc` allows the scheduler and other subsystems to hold references to
/// the task descriptor without lifetime issues.
pub static CURRENT_TASK_CHIRHO: Mutex<Option<Arc<Mutex<TaskChirho>>>> = Mutex::new(None);

/// Global table of all tasks known to the kernel.
///
/// Tasks are never removed from this list while they are in Zombie state
/// (a parent must `wait()` to reap them first).  Dead tasks may be pruned
/// by a periodic reaper in a later phase.
pub static TASK_LIST_CHIRHO: Mutex<Vec<Arc<Mutex<TaskChirho>>>> = Mutex::new(Vec::new());

// ---------------------------------------------------------------------------
// Accessors for the current task
// ---------------------------------------------------------------------------

/// Obtain an `Arc` reference to the currently running task.
///
/// Returns `None` if the tasking subsystem has not been initialized yet
/// (i.e., before [`init_tasking_chirho`] has been called).
pub fn current_task_chirho() -> Option<Arc<Mutex<TaskChirho>>> {
    CURRENT_TASK_CHIRHO.lock().clone()
}

/// Set the current task.  Used by the scheduler during a context switch.
pub fn set_current_task_chirho(task_chirho: Arc<Mutex<TaskChirho>>) {
    *CURRENT_TASK_CHIRHO.lock() = Some(task_chirho);
}

// ---------------------------------------------------------------------------
// Task-list helpers
// ---------------------------------------------------------------------------

/// Register a task in the global task list.
///
/// This should be called immediately after creating a task (before it is
/// eligible for scheduling).
pub fn register_task_chirho(task_chirho: Arc<Mutex<TaskChirho>>) {
    TASK_LIST_CHIRHO.lock().push(task_chirho);
}

/// Check if a task is still runnable (not zombie/dead).
/// Used by the scheduler to decide whether to re-queue a task.
pub fn is_task_runnable_chirho(pid_chirho: u64) -> bool {
    let list_chirho = TASK_LIST_CHIRHO.lock();
    if let Some(task_arc_chirho) = list_chirho
        .iter()
        .find(|t_chirho| t_chirho.lock().pid_chirho == pid_chirho)
    {
        !task_arc_chirho.lock().is_exited_chirho()
    } else {
        false
    }
}

/// Look up a task by PID.
///
/// Returns `None` if no task with the given PID exists.
pub fn find_task_by_pid_chirho(pid_chirho: u64) -> Option<Arc<Mutex<TaskChirho>>> {
    TASK_LIST_CHIRHO
        .lock()
        .iter()
        .find(|t_chirho| t_chirho.lock().pid_chirho == pid_chirho)
        .cloned()
}

/// Return the number of tasks currently tracked by the kernel.
pub fn task_count_chirho() -> usize {
    TASK_LIST_CHIRHO.lock().len()
}

/// Maximum PIDs for the static context array.
const MAX_PIDS_CHIRHO: usize = 64;

/// Static array of CPU contexts — one per PID slot.
///
/// CRITICAL: Context storage MUST be separate from the heap to prevent
/// VirtIO DMA or heap allocator corruption from overwriting saved registers.
/// Previously, contexts lived inside Arc<Mutex<TaskChirho>> on the heap,
/// and the raw pointer returned by context_ptr_mut_chirho was vulnerable
/// to heap corruption (DMA overlaps, allocator reuse, etc.).
///
/// Now: the scheduler reads/writes contexts through these fixed-address
/// static slots. Fork/exec code syncs the TaskChirho.context_chirho to
/// the corresponding slot. Pointers to these slots are valid forever.
static mut CONTEXT_SLOTS_CHIRHO: [CpuContextChirho; MAX_PIDS_CHIRHO] =
    [CpuContextChirho::zero_chirho(); MAX_PIDS_CHIRHO];

/// Boot context slot (PID index 63, never used as a real PID).
const BOOT_SLOT_CHIRHO: usize = MAX_PIDS_CHIRHO - 1;

/// Get a raw mutable pointer to the CpuContext for the given PID.
///
/// Returns a pointer into the static `CONTEXT_SLOTS_CHIRHO` array.
/// The pointer is valid for the kernel's lifetime — no heap involvement.
pub fn context_ptr_mut_chirho(pid_chirho: u64) -> *mut CpuContextChirho {
    let idx_chirho = pid_chirho as usize;
    if idx_chirho < MAX_PIDS_CHIRHO {
        unsafe { &raw mut CONTEXT_SLOTS_CHIRHO[idx_chirho] }
    } else {
        crate::serial_println_chirho!(
            "[TASK] PID {} exceeds context slot limit {}", pid_chirho, MAX_PIDS_CHIRHO
        );
        core::ptr::null_mut()
    }
}

/// Get a raw const pointer to the CpuContext of the task with the given PID.
pub fn context_ptr_chirho(pid_chirho: u64) -> *const CpuContextChirho {
    context_ptr_mut_chirho(pid_chirho) as *const CpuContextChirho
}

/// Get a raw mutable pointer to the boot context (used during first schedule).
pub fn boot_context_ptr_chirho() -> *mut CpuContextChirho {
    unsafe { &raw mut CONTEXT_SLOTS_CHIRHO[BOOT_SLOT_CHIRHO] }
}

/// Sync a context from a TaskChirho into the static slot.
/// Must be called after fork/exec creates or modifies a task's context.
pub fn sync_context_to_slot_chirho(pid_chirho: u64, ctx_chirho: &CpuContextChirho) {
    let idx_chirho = pid_chirho as usize;
    if idx_chirho < MAX_PIDS_CHIRHO {
        unsafe { CONTEXT_SLOTS_CHIRHO[idx_chirho] = *ctx_chirho; }
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Convert a name string into a fixed-size `[u8; TASK_COMM_LEN_CHIRHO]`
/// array, truncating if necessary and zero-padding the remainder.
fn make_comm_chirho(name_chirho: &str) -> [u8; TASK_COMM_LEN_CHIRHO] {
    let mut comm_chirho = [0u8; TASK_COMM_LEN_CHIRHO];
    let bytes_chirho = name_chirho.as_bytes();
    let copy_len_chirho = bytes_chirho.len().min(TASK_COMM_LEN_CHIRHO - 1);
    comm_chirho[..copy_len_chirho].copy_from_slice(&bytes_chirho[..copy_len_chirho]);
    comm_chirho
}

/// Allocate a kernel stack on the heap and return the base (lowest) address.
///
/// The stack memory is leaked intentionally — kernel stacks live for the
/// lifetime of their task and will be reclaimed when the task is destroyed
/// in a later phase.
///
/// # Panics
///
/// Panics if the heap allocator cannot satisfy the request.
pub fn allocate_kernel_stack_chirho(size_chirho: usize) -> u64 {
    // Allocate kernel stacks from the FRAME ALLOCATOR (physical memory)
    // instead of the heap. This prevents heap fragmentation from leaked
    // stacks, which blocked the 64MB buddy allocation for dropbear SSH.
    //
    // Each 4KB page is allocated and mapped individually.
    let num_pages_chirho = (size_chirho + 4095) / 4096;

    // Find a free virtual address range for the stack.
    // Use a simple bump allocator for stack addresses.
    use core::sync::atomic::{AtomicU64, Ordering};
    // Kernel stacks MUST be in the upper half (PML4[256+]) so they are
    // automatically shared across ALL per-process page tables.  The old
    // address 0x4666_0000_0000 was in PML4[140] (lower half) which caused
    // GPF when context-switching back to a parent task — the child's page
    // table didn't map the parent's kernel stack.
    static NEXT_STACK_ADDR_CHIRHO: AtomicU64 = AtomicU64::new(0xFFFF_8100_0000_0000);
    let base_vaddr_chirho = NEXT_STACK_ADDR_CHIRHO.fetch_add(
        (num_pages_chirho as u64 + 1) * 4096, // +1 guard page
        Ordering::SeqCst,
    );

    // Map pages using the global mapper + frame allocator.
    let mut mg_chirho = crate::mm_chirho::GLOBAL_MAPPER_CHIRHO.lock();
    let mut ag_chirho = crate::mm_chirho::GLOBAL_FRAME_ALLOCATOR_CHIRHO.lock();

    if let (Some(mapper_chirho), Some(alloc_chirho)) = (mg_chirho.as_mut(), ag_chirho.as_mut()) {
        use x86_64::structures::paging::{
            FrameAllocator, Mapper, Page, PageTableFlags, Size4KiB,
        };
        use x86_64::VirtAddr;

        let flags_chirho = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;

        for i_chirho in 0..num_pages_chirho {
            let page_addr_chirho = base_vaddr_chirho + (i_chirho as u64) * 4096;
            let page_chirho: Page<Size4KiB> =
                Page::containing_address(VirtAddr::new(page_addr_chirho));

            if let Some(frame_chirho) = alloc_chirho.allocate_frame() {
                let _ = unsafe {
                    mapper_chirho.map_to(page_chirho, frame_chirho, flags_chirho, alloc_chirho)
                }
                .map(|flush_chirho| flush_chirho.flush());

                // No need to map in per-process PT — kernel stacks are in
                // the upper half (PML4[256+]) which is shared automatically
                // by all page tables via create_user_page_table_chirho.
            }
        }

        // Zero the stack.
        unsafe {
            core::ptr::write_bytes(base_vaddr_chirho as *mut u8, 0, num_pages_chirho * 4096);
        }
    }

    base_vaddr_chirho
}

// ---------------------------------------------------------------------------
// Idle task entry point
// ---------------------------------------------------------------------------

/// Entry point for PID 0 — the idle/swapper task.
///
/// This task runs when no other task is ready.  It simply halts the CPU
/// until the next interrupt.
fn idle_task_entry_chirho() -> ! {
    loop {
        // `hlt` is more power-efficient than a spin loop and will wake on
        // the next interrupt (e.g., the timer tick).
        x86_64::instructions::hlt();
    }
}

// ---------------------------------------------------------------------------
// Initialization
// ---------------------------------------------------------------------------

/// Initialize the tasking subsystem.
///
/// Creates:
/// - **PID 0** — the idle/swapper task.  This is the task that runs when the
///   scheduler has nothing else to schedule.
/// - **PID 1** — the init task.  In a full system this would `exec` the
///   user-space init process; for now it is a kernel task placeholder.
///
/// After calling this function, [`current_task_chirho`] will return the idle
/// task (PID 0), and both PID 0 and PID 1 are registered in
/// [`TASK_LIST_CHIRHO`].
pub fn init_tasking_chirho() {
    // -- PID 0: idle / swapper ----------------------------------------------
    let mut idle_task_chirho =
        TaskChirho::new_kernel_chirho("idle", idle_task_entry_chirho);
    // The idle task is the one "running" right now.
    idle_task_chirho.state_chirho = TaskStateChirho::RunningChirho;
    let idle_arc_chirho = Arc::new(Mutex::new(idle_task_chirho));

    register_task_chirho(Arc::clone(&idle_arc_chirho));
    set_current_task_chirho(Arc::clone(&idle_arc_chirho));

    // -- PID 1: init --------------------------------------------------------
    let init_task_chirho =
        TaskChirho::new_kernel_chirho("init", init_task_entry_chirho);
    let init_arc_chirho = Arc::new(Mutex::new(init_task_chirho));

    register_task_chirho(init_arc_chirho);
}

/// Placeholder entry point for PID 1 (init).
///
/// In a full system this would load and exec `/sbin/init`.  For Phase 2 it
/// simply idles.
fn init_task_entry_chirho() -> ! {
    loop {
        x86_64::instructions::hlt();
    }
}
