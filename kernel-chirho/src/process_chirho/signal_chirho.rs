// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! Linux-compatible signal infrastructure for the Lineluya kernel.
//!
//! This module implements the core signal subsystem:
//!
//! - Standard Linux signal number constants (1–31) plus real-time range.
//! - [`SignalActionChirho`] — per-signal disposition (default, ignore, handler).
//! - [`SignalInfoChirho`] — metadata carried with a queued signal.
//! - [`PendingSignalsChirho`] — bitmask + queue of pending signals.
//! - [`SignalStateChirho`] — full per-task signal state.
//! - [`DefaultActionChirho`] — what the kernel does for each signal by default.
//! - Delivery helpers: [`send_signal_chirho`], [`check_pending_signals_chirho`],
//!   [`default_action_chirho`], [`is_fatal_chirho`].
//! - Syscall stubs: [`sys_kill_chirho`], [`sys_rt_sigaction_chirho`],
//!   [`sys_rt_sigprocmask_chirho`].

use alloc::vec::Vec;

use crate::task_chirho::{find_task_by_pid_chirho, TaskChirho, TaskStateChirho};

// ============================================================================
// Signal number constants (Linux x86_64)
// ============================================================================

pub const SIGHUP_CHIRHO: u32 = 1;
pub const SIGINT_CHIRHO: u32 = 2;
pub const SIGQUIT_CHIRHO: u32 = 3;
pub const SIGILL_CHIRHO: u32 = 4;
pub const SIGTRAP_CHIRHO: u32 = 5;
pub const SIGABRT_CHIRHO: u32 = 6;
pub const SIGBUS_CHIRHO: u32 = 7;
pub const SIGFPE_CHIRHO: u32 = 8;
pub const SIGKILL_CHIRHO: u32 = 9;
pub const SIGUSR1_CHIRHO: u32 = 10;
pub const SIGSEGV_CHIRHO: u32 = 11;
pub const SIGUSR2_CHIRHO: u32 = 12;
pub const SIGPIPE_CHIRHO: u32 = 13;
pub const SIGALRM_CHIRHO: u32 = 14;
pub const SIGTERM_CHIRHO: u32 = 15;
pub const SIGSTKFLT_CHIRHO: u32 = 16;
pub const SIGCHLD_CHIRHO: u32 = 17;
pub const SIGCONT_CHIRHO: u32 = 18;
pub const SIGSTOP_CHIRHO: u32 = 19;
pub const SIGTSTP_CHIRHO: u32 = 20;
pub const SIGTTIN_CHIRHO: u32 = 21;
pub const SIGTTOU_CHIRHO: u32 = 22;
pub const SIGURG_CHIRHO: u32 = 23;
pub const SIGXCPU_CHIRHO: u32 = 24;
pub const SIGXFSZ_CHIRHO: u32 = 25;
pub const SIGVTALRM_CHIRHO: u32 = 26;
pub const SIGPROF_CHIRHO: u32 = 27;
pub const SIGWINCH_CHIRHO: u32 = 28;
pub const SIGIO_CHIRHO: u32 = 29;
pub const SIGPWR_CHIRHO: u32 = 30;
pub const SIGSYS_CHIRHO: u32 = 31;
pub const SIGRTMIN_CHIRHO: u32 = 32;
pub const MAX_SIGNAL_CHIRHO: u32 = 64;

// ============================================================================
// Signal action — per-signal disposition
// ============================================================================

/// What the kernel should do when a signal is delivered to a task.
#[derive(Debug, Clone, Copy)]
pub enum SignalActionChirho {
    /// Use the kernel's default action for this signal (terminate, ignore,
    /// stop, or core dump — see [`DefaultActionChirho`]).
    DefaultChirho,

    /// Explicitly ignore this signal.  The signal is discarded on delivery.
    IgnoreChirho,

    /// Invoke a user-space signal handler.
    ///
    /// * `handler_chirho` — virtual address of the handler function.
    /// * `flags_chirho`   — `SA_*` flags (e.g., `SA_SIGINFO`, `SA_RESTART`).
    /// * `mask_chirho`    — additional signals to block while the handler runs.
    HandlerChirho {
        handler_chirho: u64,
        flags_chirho: u64,
        mask_chirho: u64,
        restorer_chirho: u64,
    },
}

impl Default for SignalActionChirho {
    fn default() -> Self {
        Self::DefaultChirho
    }
}

// ============================================================================
// SignalInfoChirho — queued signal metadata
// ============================================================================

/// Metadata associated with a pending signal, loosely modelled on Linux's
/// `siginfo_t`.
#[derive(Debug, Clone, Copy)]
pub struct SignalInfoChirho {
    /// Signal number (1-based).
    pub signo_chirho: u32,
    /// Signal code (e.g., `SI_USER`, `SI_KERNEL`).
    pub code_chirho: i32,
    /// PID of the sender (0 for kernel-generated signals).
    pub pid_chirho: u64,
}

/// Signal code constants (subset).
pub const SI_USER_CHIRHO: i32 = 0;
pub const SI_KERNEL_CHIRHO: i32 = 0x80;

// ============================================================================
// PendingSignalsChirho — bitmask + queue
// ============================================================================

/// Set of signals that have been sent to a task but not yet delivered.
#[derive(Debug, Clone)]
pub struct PendingSignalsChirho {
    /// Bitmask of pending signals.  Bit *n* set means signal *n* is pending.
    /// Bit 0 is unused (signals are 1-based).
    pub mask_chirho: u64,

    /// Queue of signal info structs, one per pending signal.  For standard
    /// (non-real-time) signals the queue holds at most one entry per signal
    /// number; real-time signals can queue multiple entries.
    pub queue_chirho: Vec<SignalInfoChirho>,
}

impl PendingSignalsChirho {
    /// Create an empty pending-signals set.
    pub const fn new_chirho() -> Self {
        Self {
            mask_chirho: 0,
            queue_chirho: Vec::new(),
        }
    }

    /// Add a signal to the pending set.
    ///
    /// For standard signals (< 32) only one instance is queued; duplicates
    /// are silently dropped (matching Linux behaviour).
    pub fn add_chirho(&mut self, info_chirho: SignalInfoChirho) {
        let bit_chirho = 1u64 << info_chirho.signo_chirho;
        if info_chirho.signo_chirho < SIGRTMIN_CHIRHO {
            // Standard signal: only one pending instance.
            if self.mask_chirho & bit_chirho != 0 {
                return; // already pending
            }
        }
        self.mask_chirho |= bit_chirho;
        self.queue_chirho.push(info_chirho);
    }

    /// Remove and return the [`SignalInfoChirho`] for signal `signo_chirho`.
    ///
    /// Clears the corresponding bit in `mask_chirho` only if no more entries
    /// for that signal remain in the queue (relevant for real-time signals).
    pub fn dequeue_chirho(&mut self, signo_chirho: u32) -> Option<SignalInfoChirho> {
        if let Some(pos_chirho) = self
            .queue_chirho
            .iter()
            .position(|si_chirho| si_chirho.signo_chirho == signo_chirho)
        {
            let info_chirho = self.queue_chirho.remove(pos_chirho);

            // If no more entries for this signal remain, clear the bit.
            let still_pending_chirho = self
                .queue_chirho
                .iter()
                .any(|si_chirho| si_chirho.signo_chirho == signo_chirho);
            if !still_pending_chirho {
                self.mask_chirho &= !(1u64 << signo_chirho);
            }

            Some(info_chirho)
        } else {
            self.mask_chirho &= !(1u64 << signo_chirho);
            None
        }
    }

    /// Return `true` if there are no pending signals.
    pub fn is_empty_chirho(&self) -> bool {
        self.mask_chirho == 0
    }
}

impl Default for PendingSignalsChirho {
    fn default() -> Self {
        Self::new_chirho()
    }
}

// ============================================================================
// SignalStateChirho — full per-task signal state
// ============================================================================

/// Per-task signal state, analogous to the signal-related fields inside
/// Linux's `struct task_struct` and `struct sighand_struct`.
#[derive(Debug, Clone)]
pub struct SignalStateChirho {
    /// Bitmask of blocked signals.  Bit *n* set means signal *n* is blocked.
    /// SIGKILL and SIGSTOP can never be blocked — attempts to block them are
    /// silently ignored.
    pub blocked_chirho: u64,

    /// Signals that have been sent but not yet delivered.
    pub pending_chirho: PendingSignalsChirho,

    /// Per-signal disposition table.  Index 0 is unused (signals are 1-based).
    pub actions_chirho: [SignalActionChirho; MAX_SIGNAL_CHIRHO as usize],
}

impl SignalStateChirho {
    /// Create a new signal state with all dispositions set to default and no
    /// blocked or pending signals.
    pub fn new_chirho() -> Self {
        Self {
            blocked_chirho: 0,
            pending_chirho: PendingSignalsChirho::new_chirho(),
            actions_chirho: [SignalActionChirho::DefaultChirho; MAX_SIGNAL_CHIRHO as usize],
        }
    }
}

impl Default for SignalStateChirho {
    fn default() -> Self {
        Self::new_chirho()
    }
}

// ============================================================================
// DefaultActionChirho — kernel default for each signal
// ============================================================================

/// The kernel's default action for a signal when the disposition is
/// [`SignalActionChirho::DefaultChirho`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefaultActionChirho {
    /// Terminate the process.
    TermChirho,
    /// Ignore the signal.
    IgnoreChirho,
    /// Terminate the process and dump core.
    CoreChirho,
    /// Stop (suspend) the process.
    StopChirho,
    /// Continue the process if it is stopped.
    ContChirho,
}

/// Return the kernel's default action for signal `signo_chirho`.
///
/// Signal numbers outside the valid range (1..=MAX_SIGNAL) return
/// [`DefaultActionChirho::TermChirho`] as a conservative fallback.
pub fn default_action_chirho(signo_chirho: u32) -> DefaultActionChirho {
    match signo_chirho {
        SIGHUP_CHIRHO => DefaultActionChirho::TermChirho,
        SIGINT_CHIRHO => DefaultActionChirho::TermChirho,
        SIGQUIT_CHIRHO => DefaultActionChirho::CoreChirho,
        SIGILL_CHIRHO => DefaultActionChirho::CoreChirho,
        SIGTRAP_CHIRHO => DefaultActionChirho::CoreChirho,
        SIGABRT_CHIRHO => DefaultActionChirho::CoreChirho,
        SIGBUS_CHIRHO => DefaultActionChirho::CoreChirho,
        SIGFPE_CHIRHO => DefaultActionChirho::CoreChirho,
        SIGKILL_CHIRHO => DefaultActionChirho::TermChirho,
        SIGUSR1_CHIRHO => DefaultActionChirho::TermChirho,
        SIGSEGV_CHIRHO => DefaultActionChirho::CoreChirho,
        SIGUSR2_CHIRHO => DefaultActionChirho::TermChirho,
        SIGPIPE_CHIRHO => DefaultActionChirho::TermChirho,
        SIGALRM_CHIRHO => DefaultActionChirho::TermChirho,
        SIGTERM_CHIRHO => DefaultActionChirho::TermChirho,
        SIGSTKFLT_CHIRHO => DefaultActionChirho::TermChirho,
        SIGCHLD_CHIRHO => DefaultActionChirho::IgnoreChirho,
        SIGCONT_CHIRHO => DefaultActionChirho::ContChirho,
        SIGSTOP_CHIRHO => DefaultActionChirho::StopChirho,
        SIGTSTP_CHIRHO => DefaultActionChirho::StopChirho,
        SIGTTIN_CHIRHO => DefaultActionChirho::StopChirho,
        SIGTTOU_CHIRHO => DefaultActionChirho::StopChirho,
        SIGURG_CHIRHO => DefaultActionChirho::IgnoreChirho,
        SIGXCPU_CHIRHO => DefaultActionChirho::CoreChirho,
        SIGXFSZ_CHIRHO => DefaultActionChirho::CoreChirho,
        SIGVTALRM_CHIRHO => DefaultActionChirho::TermChirho,
        SIGPROF_CHIRHO => DefaultActionChirho::TermChirho,
        SIGWINCH_CHIRHO => DefaultActionChirho::IgnoreChirho,
        SIGIO_CHIRHO => DefaultActionChirho::TermChirho,
        SIGPWR_CHIRHO => DefaultActionChirho::TermChirho,
        SIGSYS_CHIRHO => DefaultActionChirho::CoreChirho,
        // Real-time signals default to terminate.
        SIGRTMIN_CHIRHO..=MAX_SIGNAL_CHIRHO => DefaultActionChirho::TermChirho,
        _ => DefaultActionChirho::TermChirho,
    }
}

/// Return `true` if signal `signo_chirho` is unconditionally fatal (cannot be
/// caught or ignored).
///
/// In Linux, SIGKILL is the only truly uncatchable fatal signal.  SIGSEGV
/// and similar are fatal by *default* but can be caught.  We include SIGSEGV
/// here because, in this early kernel, we do not yet support user-space
/// signal handlers — so a SIGSEGV is effectively fatal.
pub fn is_fatal_chirho(signo_chirho: u32) -> bool {
    matches!(
        signo_chirho,
        SIGKILL_CHIRHO | SIGSEGV_CHIRHO | SIGBUS_CHIRHO | SIGILL_CHIRHO | SIGFPE_CHIRHO
    )
}

// ============================================================================
// Signal delivery helpers
// ============================================================================

/// Mask of signals that can never be blocked or caught (SIGKILL, SIGSTOP).
const UNBLOCKABLE_MASK_CHIRHO: u64 = (1u64 << SIGKILL_CHIRHO) | (1u64 << SIGSTOP_CHIRHO);

/// Send signal `signo_chirho` to the task identified by `pid_chirho`.
///
/// # Errors
///
/// * `-3` (`ESRCH`) — no task with the given PID exists.
/// * `-22` (`EINVAL`) — `signo_chirho` is out of range.
///
/// Signal 0 is the "null signal" — it checks whether the target exists but
/// does not actually queue anything (matching `kill(pid, 0)` semantics).
pub fn send_signal_chirho(pid_chirho: u64, signo_chirho: u32) -> Result<(), i64> {
    // Validate signal number.
    if signo_chirho > MAX_SIGNAL_CHIRHO {
        return Err(-22); // EINVAL
    }

    // Signal 0: existence check only.
    if signo_chirho == 0 {
        return if find_task_by_pid_chirho(pid_chirho).is_some() {
            Ok(())
        } else {
            Err(-3) // ESRCH
        };
    }

    let task_arc_chirho =
        find_task_by_pid_chirho(pid_chirho).ok_or(-3i64)?; // ESRCH

    let mut task_chirho = task_arc_chirho.lock();

    // Cannot send signals to dead/zombie tasks.
    if task_chirho.is_exited_chirho() {
        return Err(-3); // ESRCH
    }

    // Set the bit in the task's simple pending bitmask (for backward compat
    // with the fields already on TaskChirho).
    task_chirho.pending_signals_chirho |= 1u64 << signo_chirho;

    // Also enqueue into the full signal state for rt_sigaction-aware
    // processes (A2-PROC-005).
    task_chirho.signal_state_chirho.pending_chirho.add_chirho(SignalInfoChirho {
        signo_chirho,
        code_chirho: SI_USER_CHIRHO,
        pid_chirho: crate::task_chirho::current_task_chirho()
            .map(|t_chirho| t_chirho.lock().pid_chirho)
            .unwrap_or(0),
    });

    // If the signal is SIGCONT, wake a stopped/blocked task.
    if signo_chirho == SIGCONT_CHIRHO {
        if task_chirho.state_chirho == TaskStateChirho::StoppedChirho {
            task_chirho.state_chirho = TaskStateChirho::ReadyChirho;
        }
        task_chirho.wake_chirho();
    }

    // For fatal signals (SIGKILL, SIGTERM, SIGHUP, SIGPIPE, etc.),
    // wake blocked/sleeping tasks so the signal can be checked on the
    // next syscall return boundary.
    let is_wakeup_signal_chirho = signo_chirho == SIGKILL_CHIRHO
        || default_action_chirho(signo_chirho) == DefaultActionChirho::TermChirho
        || default_action_chirho(signo_chirho) == DefaultActionChirho::CoreChirho;
    if is_wakeup_signal_chirho {
        if task_chirho.state_chirho == TaskStateChirho::BlockedChirho
            || task_chirho.state_chirho == TaskStateChirho::SleepingChirho
        {
            task_chirho.state_chirho = TaskStateChirho::ReadyChirho;
        }
    }

    crate::serial_debug_chirho!(
        "[SIGNAL] signal {} sent to PID {} via kill()",
        signo_chirho,
        pid_chirho
    );

    Ok(())
}

/// Check for a deliverable pending signal on `task_chirho` and return its
/// signal number.
///
/// A signal is deliverable if:
/// 1. It is pending (`pending_signals_chirho` bitmask).
/// 2. It is not blocked (`signal_mask_chirho`), **or** it is SIGKILL/SIGSTOP
///    (which cannot be blocked).
///
/// The lowest-numbered deliverable signal is returned first (matching Linux
/// priority).  The signal's pending bit is cleared.
///
/// Returns `None` if no signal is deliverable.
pub fn check_pending_signals_chirho(task_chirho: &mut TaskChirho) -> Option<u32> {
    // Deliverable = pending AND (NOT blocked OR unblockable).
    let deliverable_chirho = task_chirho.pending_signals_chirho
        & (!task_chirho.signal_mask_chirho | UNBLOCKABLE_MASK_CHIRHO);

    if deliverable_chirho == 0 {
        return None;
    }

    // Find lowest set bit (lowest signal number).
    let signo_chirho = deliverable_chirho.trailing_zeros() as u32;

    // Clear the pending bit.
    task_chirho.pending_signals_chirho &= !(1u64 << signo_chirho);

    Some(signo_chirho)
}

// ============================================================================
// Syscall stubs
// ============================================================================

/// `sys_kill(pid, sig)` — send signal `sig_chirho` to process `pid_chirho`.
///
/// Simplified implementation: only supports sending to a specific positive
/// PID.  Process groups (`pid <= 0`) are not yet implemented.
///
/// # Returns
///
/// `0` on success, negative errno on failure.
pub fn sys_kill_chirho(pid_chirho: u64, sig_chirho: u32) -> i64 {
    match send_signal_chirho(pid_chirho, sig_chirho) {
        Ok(()) => 0,
        Err(errno_chirho) => errno_chirho,
    }
}

// ============================================================================
// SigactionChirho — Linux-compatible sigaction struct
// ============================================================================

/// SA_* flag constants matching Linux.
pub const SA_NOCLDSTOP_CHIRHO: u64 = 1;
pub const SA_NOCLDWAIT_CHIRHO: u64 = 2;
pub const SA_SIGINFO_CHIRHO: u64 = 4;
pub const SA_ONSTACK_CHIRHO: u64 = 0x0800_0000;
pub const SA_RESTART_CHIRHO: u64 = 0x1000_0000;
pub const SA_NODEFER_CHIRHO: u64 = 0x4000_0000;
pub const SA_RESETHAND_CHIRHO: u64 = 0x8000_0000;

/// Linux `struct sigaction` for x86_64 (kernel form: `struct k_sigaction`).
///
/// Layout:
///   sa_handler:   u64   (SIG_DFL=0, SIG_IGN=1, or handler address)
///   sa_flags:     u64
///   sa_restorer:  u64
///   sa_mask:      u64   (signal set — single u64 for up to 64 signals)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SigactionChirho {
    /// Handler address (0 = SIG_DFL, 1 = SIG_IGN, else function pointer).
    pub sa_handler_chirho: u64,
    /// SA_* flags.
    pub sa_flags_chirho: u64,
    /// Restorer function (set by libc's sigaction wrapper).
    pub sa_restorer_chirho: u64,
    /// Additional signals to block during handler execution.
    pub sa_mask_chirho: u64,
}

impl SigactionChirho {
    pub const fn default_chirho() -> Self {
        Self {
            sa_handler_chirho: 0, // SIG_DFL
            sa_flags_chirho: 0,
            sa_restorer_chirho: 0,
            sa_mask_chirho: 0,
        }
    }
}

/// `sys_rt_sigaction(signum, act, oldact, sigsetsize)` — examine and change
/// a signal action.
///
/// Reads the old sigaction into `oldact_chirho` (if non-null) and installs
/// the new sigaction from `act_chirho` (if non-null) into the current task's
/// signal state.
///
/// # Returns
///
/// `0` on success, `-22` (EINVAL) for invalid signal numbers.
pub fn sys_rt_sigaction_chirho(
    signum_chirho: u32,
    act_chirho: u64,
    oldact_chirho: u64,
    _sigsetsize_chirho: u64,
) -> i64 {
    if signum_chirho == 0 || signum_chirho > MAX_SIGNAL_CHIRHO {
        return -22; // EINVAL
    }
    // SIGKILL and SIGSTOP dispositions cannot be changed.
    if signum_chirho == SIGKILL_CHIRHO || signum_chirho == SIGSTOP_CHIRHO {
        return -22; // EINVAL
    }

    let task_arc_chirho = match crate::task_chirho::current_task_chirho() {
        Some(t_chirho) => t_chirho,
        None => return -3, // ESRCH
    };

    let mut task_chirho = task_arc_chirho.lock();
    // Signal numbers are 1-based (1..=64), array indices are 0-based (0..63)
    let idx_chirho = (signum_chirho as usize).saturating_sub(1);
    if idx_chirho >= MAX_SIGNAL_CHIRHO as usize {
        return -22; // EINVAL — safety check
    }

    // Write old action to userspace if oldact is non-null.
    if oldact_chirho != 0 {
        let old_sigaction_chirho = match &task_chirho.signal_state_chirho.actions_chirho[idx_chirho] {
            SignalActionChirho::DefaultChirho => SigactionChirho::default_chirho(),
            SignalActionChirho::IgnoreChirho => SigactionChirho {
                sa_handler_chirho: 1, // SIG_IGN
                sa_flags_chirho: 0,
                sa_restorer_chirho: 0,
                sa_mask_chirho: 0,
            },
            SignalActionChirho::HandlerChirho {
                handler_chirho,
                flags_chirho,
                mask_chirho,
                restorer_chirho,
            } => SigactionChirho {
                sa_handler_chirho: *handler_chirho,
                sa_flags_chirho: *flags_chirho,
                sa_restorer_chirho: *restorer_chirho,
                sa_mask_chirho: *mask_chirho,
            },
        };
        let size_chirho = core::mem::size_of::<SigactionChirho>();
        let src_chirho = unsafe {
            core::slice::from_raw_parts(
                &old_sigaction_chirho as *const SigactionChirho as *const u8,
                size_chirho,
            )
        };
        if crate::uaccess_chirho::copy_to_user_chirho(oldact_chirho, src_chirho, size_chirho).is_err() {
            return -14; // EFAULT
        }
    }

    // Read new action from userspace if act is non-null.
    if act_chirho != 0 {
        let size_chirho = core::mem::size_of::<SigactionChirho>();
        let mut buf_chirho = [0u8; 32]; // SigactionChirho is 32 bytes
        if crate::uaccess_chirho::copy_from_user_chirho(
            &mut buf_chirho[..size_chirho],
            act_chirho,
            size_chirho,
        )
        .is_err()
        {
            return -14; // EFAULT
        }
        let new_sa_chirho =
            unsafe { core::ptr::read(buf_chirho.as_ptr() as *const SigactionChirho) };

        let action_chirho = match new_sa_chirho.sa_handler_chirho {
            0 => SignalActionChirho::DefaultChirho, // SIG_DFL
            1 => SignalActionChirho::IgnoreChirho,  // SIG_IGN
            handler_chirho => SignalActionChirho::HandlerChirho {
                handler_chirho,
                flags_chirho: new_sa_chirho.sa_flags_chirho,
                mask_chirho: new_sa_chirho.sa_mask_chirho,
                restorer_chirho: new_sa_chirho.sa_restorer_chirho,
            },
        };
        task_chirho.signal_state_chirho.actions_chirho[idx_chirho] = action_chirho;
    }

    0
}

/// `sys_rt_sigprocmask(how, set, oldset, sigsetsize)` — examine and change
/// blocked signals.
///
/// Reads the old blocked mask into `oldset_chirho` (if non-null) and updates
/// the current task's `signal_mask_chirho` according to `how_chirho`:
///   - `SIG_BLOCK`   (0): blocked |= *set
///   - `SIG_UNBLOCK` (1): blocked &= ~*set
///   - `SIG_SETMASK` (2): blocked = *set
///
/// SIGKILL and SIGSTOP can never be blocked.
///
/// # Returns
///
/// `0` on success, `-22` (EINVAL) for an invalid `how_chirho` value.
pub fn sys_rt_sigprocmask_chirho(
    how_chirho: u32,
    set_chirho: u64,
    oldset_chirho: u64,
    _sigsetsize_chirho: u64,
) -> i64 {
    // Valid `how` values: SIG_BLOCK=0, SIG_UNBLOCK=1, SIG_SETMASK=2
    if how_chirho > 2 {
        return -22; // EINVAL
    }

    let task_arc_chirho = match crate::task_chirho::current_task_chirho() {
        Some(t_chirho) => t_chirho,
        None => return -3, // ESRCH
    };

    let mut task_chirho = task_arc_chirho.lock();

    // Write old blocked mask to userspace if oldset is non-null.
    if oldset_chirho != 0 {
        let old_mask_chirho = task_chirho.signal_mask_chirho;
        let bytes_chirho = old_mask_chirho.to_ne_bytes();
        if crate::uaccess_chirho::copy_to_user_chirho(oldset_chirho, &bytes_chirho, 8).is_err() {
            return -14; // EFAULT
        }
    }

    // Read new set from userspace if set is non-null.
    if set_chirho != 0 {
        let mut buf_chirho = [0u8; 8];
        if crate::uaccess_chirho::copy_from_user_chirho(&mut buf_chirho, set_chirho, 8).is_err() {
            return -14; // EFAULT
        }
        let new_set_chirho = u64::from_ne_bytes(buf_chirho);

        // Mask that prevents blocking SIGKILL (9) and SIGSTOP (19).
        let unblockable_chirho: u64 =
            (1u64 << SIGKILL_CHIRHO) | (1u64 << SIGSTOP_CHIRHO);

        match how_chirho {
            0 => {
                // SIG_BLOCK
                task_chirho.signal_mask_chirho |= new_set_chirho;
            }
            1 => {
                // SIG_UNBLOCK
                task_chirho.signal_mask_chirho &= !new_set_chirho;
            }
            2 => {
                // SIG_SETMASK
                task_chirho.signal_mask_chirho = new_set_chirho;
            }
            _ => unreachable!(),
        }

        // Ensure SIGKILL and SIGSTOP are never blocked.
        task_chirho.signal_mask_chirho &= !unblockable_chirho;

        // Also update the signal_state_chirho blocked mask for consistency.
        task_chirho.signal_state_chirho.blocked_chirho = task_chirho.signal_mask_chirho;
    }

    0
}

// ============================================================================
// Additional signal syscalls (Phase 4)
// ============================================================================

/// `sys_tgkill(tgid, tid, sig)` — send signal to a specific thread
/// in a thread group.
///
/// Simplified: we ignore `tgid_chirho` and just send to `tid_chirho`.
pub fn sys_tgkill_chirho(tgid_chirho: u64, tid_chirho: u64, sig_chirho: u32) -> i64 {
    let _ = tgid_chirho; // Thread groups not fully implemented yet
    if sig_chirho > MAX_SIGNAL_CHIRHO {
        return -22; // EINVAL
    }
    if sig_chirho == 0 {
        // Existence check.
        return if crate::task_chirho::find_task_by_pid_chirho(tid_chirho).is_some() {
            0
        } else {
            -3 // ESRCH
        };
    }
    match send_signal_chirho(tid_chirho, sig_chirho) {
        Ok(()) => 0,
        Err(errno_chirho) => errno_chirho,
    }
}

/// `sys_tkill(tid, sig)` — send signal to a thread.
pub fn sys_tkill_chirho(tid_chirho: u64, sig_chirho: u32) -> i64 {
    sys_tgkill_chirho(0, tid_chirho, sig_chirho)
}

/// `sys_rt_sigpending(set_ptr)` — return the set of pending signals.
///
/// Writes the current task's pending signal mask to `set_ptr_chirho`.
pub fn sys_rt_sigpending_chirho(set_ptr_chirho: u64) -> i64 {
    if set_ptr_chirho == 0 {
        return -14; // EFAULT
    }

    let pending_mask_chirho = match crate::task_chirho::current_task_chirho() {
        Some(t_chirho) => t_chirho.lock().pending_signals_chirho,
        None => 0u64,
    };

    let bytes_chirho = pending_mask_chirho.to_ne_bytes();
    if crate::uaccess_chirho::copy_to_user_chirho(set_ptr_chirho, &bytes_chirho, 8).is_err() {
        return -14; // EFAULT
    }

    0
}

/// `sys_rt_sigsuspend(mask_ptr)` — temporarily replace signal mask and
/// suspend execution until a signal is delivered.
///
/// Stub: returns -EINTR immediately (matching expected behavior for callers).
pub fn sys_rt_sigsuspend_chirho(_mask_ptr_chirho: u64) -> i64 {
    -4 // EINTR
}

/// `sys_sigaltstack(ss, old_ss)` — get/set alternate signal stack.
///
/// Stub: returns 0 (success) without doing anything.
pub fn sys_sigaltstack_chirho(_ss_chirho: u64, _old_ss_chirho: u64) -> i64 {
    0
}

// ============================================================================
// A2-PROC-005: Signal delivery infrastructure
// ============================================================================

/// Send SIGCHLD to a parent process when a child exits.
///
/// This is called from `sys_exit_chirho` (which already sets the pending bit)
/// and also wakes blocked parents so they can reap via `wait4`.
///
/// # Arguments
///
/// * `parent_pid_chirho` — PID of the parent process to receive SIGCHLD.
/// * `child_pid_chirho`  — PID of the exiting child (for the SignalInfo sender field).
pub fn deliver_sigchld_chirho(parent_pid_chirho: u64, child_pid_chirho: u64) {
    if parent_pid_chirho == 0 {
        return; // init has no parent to notify
    }

    if let Some(parent_arc_chirho) = find_task_by_pid_chirho(parent_pid_chirho) {
        let mut parent_chirho = parent_arc_chirho.lock();
        let should_unblock_parent_chirho = matches!(
            parent_chirho.state_chirho,
            TaskStateChirho::BlockedChirho | TaskStateChirho::SleepingChirho
        );

        // Set the pending bit for SIGCHLD (signal 17).
        parent_chirho.pending_signals_chirho |= 1u64 << SIGCHLD_CHIRHO;

        // Also enqueue into the full signal state for processes using
        // rt_sigaction to inspect siginfo.
        parent_chirho.signal_state_chirho.pending_chirho.add_chirho(SignalInfoChirho {
            signo_chirho: SIGCHLD_CHIRHO,
            code_chirho: SI_KERNEL_CHIRHO,
            pid_chirho: child_pid_chirho,
        });

        crate::serial_debug_chirho!(
            "[SIGNAL] SIGCHLD delivered to PID {} (child PID {} exited)",
            parent_pid_chirho,
            child_pid_chirho
        );

        drop(parent_chirho);

        // A pending signal alone is not enough if the parent is sleeping in a
        // wait queue: it must be re-inserted into the scheduler's run queue.
        if should_unblock_parent_chirho {
            crate::scheduler_chirho::unblock_task_chirho(parent_pid_chirho);
        }

        // The direct parent should run before unrelated ready tasks so it can
        // process SIGCHLD / wait4 / self-pipe work immediately after child exit.
        crate::scheduler_chirho::promote_task_chirho(parent_pid_chirho);
    }
}

/// Send SIGPIPE to the current (writing) task.
///
/// Called when a write to a pipe whose read end is closed occurs.
/// The signal is set pending on the current task so it will be checked
/// on the next syscall return boundary.
pub fn deliver_sigpipe_to_current_chirho() {
    if let Some(task_arc_chirho) = crate::task_chirho::current_task_chirho() {
        let mut task_chirho = task_arc_chirho.lock();

        // Check the signal disposition — if SIGPIPE is ignored or has a
        // handler, the process won't be killed (matches Linux semantics).
        let idx_chirho = (SIGPIPE_CHIRHO as usize).saturating_sub(1);
        let action_chirho = &task_chirho.signal_state_chirho.actions_chirho[idx_chirho];

        match action_chirho {
            SignalActionChirho::IgnoreChirho => {
                // Process explicitly ignores SIGPIPE — do not set pending.
                return;
            }
            _ => {}
        }

        task_chirho.pending_signals_chirho |= 1u64 << SIGPIPE_CHIRHO;
        task_chirho.signal_state_chirho.pending_chirho.add_chirho(SignalInfoChirho {
            signo_chirho: SIGPIPE_CHIRHO,
            code_chirho: SI_KERNEL_CHIRHO,
            pid_chirho: 0, // kernel-generated
        });

        crate::serial_debug_chirho!(
            "[SIGNAL] SIGPIPE pending on PID {} (write to broken pipe)",
            task_chirho.pid_chirho
        );
    }
}

/// Return `true` if the current task has at least one pending signal that is
/// not blocked by its current signal mask.
///
/// This is used by interruptible blocking syscalls (`select`, `poll`, etc.) so
/// they can return `-EINTR` and let user-space signal delivery happen on the
/// normal syscall return boundary.
pub fn current_has_deliverable_signal_chirho() -> bool {
    let task_arc_chirho = match crate::task_chirho::current_task_chirho() {
        Some(task_arc_chirho) => task_arc_chirho,
        None => return false,
    };

    let task_chirho = task_arc_chirho.lock();
    let deliverable_mask_chirho = task_chirho.pending_signals_chirho
        & !task_chirho.signal_state_chirho.blocked_chirho;
    deliverable_mask_chirho != 0
}

/// Check for fatal pending signals on the current task and terminate it
/// if necessary.
///
/// This is called from the syscall return path (in
/// `syscall_dispatch_wrapper_chirho`) to enforce default-kill signal
/// semantics for SIGTERM, SIGHUP, SIGPIPE, SIGKILL, etc.
///
/// # Returns
///
/// `true` if the task was terminated (caller should not return to userspace).
/// `false` if no fatal signal is pending and normal return should continue.
pub fn check_fatal_signals_on_return_chirho() -> bool {
    let (pid_chirho, ppid_chirho, signo_opt_chirho) = {
        let task_arc_chirho = match crate::task_chirho::current_task_chirho() {
            Some(t_chirho) => t_chirho,
            None => return false,
        };

        let mut task_chirho = task_arc_chirho.lock();

        // Fast path: no pending signals at all.
        if task_chirho.pending_signals_chirho == 0 {
            return false;
        }

        // Find the first deliverable signal (pending AND not blocked, or
        // unblockable).
        let signo_chirho = match check_pending_signals_chirho(&mut task_chirho) {
            Some(s_chirho) => s_chirho,
            None => return false,
        };

        // Determine what to do based on the signal's disposition.
        // Signal numbers are 1-based, array indices are 0-based.
        let idx_chirho = (signo_chirho as usize).saturating_sub(1);
        if idx_chirho < MAX_SIGNAL_CHIRHO as usize {
            let action_chirho = &task_chirho.signal_state_chirho.actions_chirho[idx_chirho];
            match action_chirho {
                SignalActionChirho::IgnoreChirho => {
                    // Signal is explicitly ignored — dequeue and continue.
                    task_chirho
                        .signal_state_chirho
                        .pending_chirho
                        .dequeue_chirho(signo_chirho);
                    return false;
                }
                SignalActionChirho::HandlerChirho { .. } => {
                    // Signal has a user handler — leave it for
                    // deliver_one_signal_on_return_chirho to process.
                    // Do NOT fall through to default action (which would
                    // dequeue and ignore SIGCHLD, breaking the self-pipe).
                    return false;
                }
                SignalActionChirho::DefaultChirho => {
                    // Fall through to default action handling below.
                }
            }
        }

        // Check the default action for this signal.
        let default_chirho = default_action_chirho(signo_chirho);
        match default_chirho {
            DefaultActionChirho::IgnoreChirho => {
                // Default is ignore (e.g. SIGCHLD, SIGURG, SIGWINCH).
                task_chirho
                    .signal_state_chirho
                    .pending_chirho
                    .dequeue_chirho(signo_chirho);
                return false;
            }
            DefaultActionChirho::ContChirho => {
                // SIGCONT: just continue (task is already running).
                task_chirho
                    .signal_state_chirho
                    .pending_chirho
                    .dequeue_chirho(signo_chirho);
                return false;
            }
            DefaultActionChirho::StopChirho => {
                // SIGSTOP/SIGTSTP: stop the task.
                task_chirho.state_chirho = TaskStateChirho::StoppedChirho;
                task_chirho
                    .signal_state_chirho
                    .pending_chirho
                    .dequeue_chirho(signo_chirho);
                crate::serial_debug_chirho!(
                    "[SIGNAL] PID {} stopped by signal {}",
                    task_chirho.pid_chirho,
                    signo_chirho
                );
                // Task should be descheduled by the scheduler.
                return false;
            }
            DefaultActionChirho::TermChirho | DefaultActionChirho::CoreChirho => {
                // Fatal signal — terminate the process.
                let pid_chirho = task_chirho.pid_chirho;
                let ppid_chirho = task_chirho.ppid_chirho;
                // Set exit code to 128 + signo (standard Unix convention).
                let exit_code_chirho = 128 + signo_chirho as i32;
                task_chirho.exit_code_chirho = exit_code_chirho;
                task_chirho.state_chirho = TaskStateChirho::ZombieChirho;

                crate::serial_println_chirho!(
                    "[SIGNAL] PID {} killed by signal {} (exit_code={})",
                    pid_chirho,
                    signo_chirho,
                    exit_code_chirho
                );

                (pid_chirho, ppid_chirho, Some(signo_chirho))
            }
        }
    };

    // If we terminated the task, clean up outside the task lock.
    if let Some(signo_chirho) = signo_opt_chirho {
        // Remove from scheduler run queue.
        crate::scheduler_chirho::remove_task_chirho(pid_chirho);

        // Send SIGCHLD to parent.
        deliver_sigchld_chirho(ppid_chirho, pid_chirho);

        // Wake any parent sleeping in wait4.
        crate::process_chirho::wake_child_exit_waitqueue_chirho();

        crate::serial_debug_chirho!(
            "[SIGNAL] PID {} removed from scheduler after signal {}",
            pid_chirho,
            signo_chirho
        );

        // Yield to let the scheduler pick the next task.
        crate::scheduler_chirho::yield_current_chirho();

        return true;
    }

    false
}

// ============================================================================
// User signal handler delivery (GPT-directed minimal implementation)
// ============================================================================

/// Saved user-mode state pushed onto the user stack before signal handler entry.
/// The handler's sa_restorer points to code that does `mov eax, 15; syscall`
/// (rt_sigreturn), which reads this frame to restore original state.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct RtSigframeChirho {
    /// Address of sa_restorer (handler RETs here → rt_sigreturn)
    pub restorer_chirho: u64,
    /// Saved user RIP (to resume after sigreturn)
    pub saved_rip_chirho: u64,
    /// Saved user RSP
    pub saved_rsp_chirho: u64,
    /// Saved RAX (syscall return value)
    pub saved_rax_chirho: u64,
    /// Saved RCX (user RIP for SYSRET)
    pub saved_rcx_chirho: u64,
    /// Saved R11 (user RFLAGS)
    pub saved_r11_chirho: u64,
    /// Saved RDI
    pub saved_rdi_chirho: u64,
    /// Saved RSI
    pub saved_rsi_chirho: u64,
    /// Saved RDX
    pub saved_rdx_chirho: u64,
    /// Old signal mask (restored on sigreturn)
    pub old_mask_chirho: u64,
    /// Signal number (for reference)
    pub signo_chirho: u64,
}

/// Deliver one pending signal to the current task by setting up a signal
/// frame on the user stack and redirecting execution to the handler.
///
/// Called from syscall_dispatch_wrapper_chirho before returning to userspace.
/// Returns true if a signal was delivered (frame was modified).
pub fn deliver_one_signal_on_return_chirho(
    frame_chirho: &mut crate::syscall_chirho::SyscallFrameChirho,
) -> bool {
    let task_arc_chirho = match crate::task_chirho::current_task_chirho() {
        Some(t) => t,
        None => return false,
    };

    let mut task_chirho = task_arc_chirho.lock();
    let pending_chirho = task_chirho.pending_signals_chirho;
    if pending_chirho == 0 {
        return false;
    }

    let pid_chirho = task_chirho.pid_chirho;
    let blocked_chirho = task_chirho.signal_state_chirho.blocked_chirho;
    // One-shot debug: log when PID 3 has pending signals
    if pid_chirho == 3 {
        use core::sync::atomic::{AtomicU64, Ordering};
        static SIG_DBG_CHIRHO: AtomicU64 = AtomicU64::new(0);
        let cnt_chirho = SIG_DBG_CHIRHO.fetch_add(1, Ordering::Relaxed);
        if cnt_chirho < 3 {
            // Check which signals are pending and which have handlers
            let sigchld_idx_chirho = (SIGCHLD_CHIRHO as usize).saturating_sub(1);
            let action_str_chirho = match &task_chirho.signal_state_chirho.actions_chirho[sigchld_idx_chirho] {
                SignalActionChirho::DefaultChirho => "default",
                SignalActionChirho::IgnoreChirho => "ignore",
                SignalActionChirho::HandlerChirho { .. } => "handler",
            };
            crate::serial_println_chirho!(
                "[SIG-DBG] PID 3 #{}: pending={:#x} blocked={:#x} SIGCHLD action={}",
                cnt_chirho, pending_chirho, blocked_chirho, action_str_chirho,
            );
        }
    }

    // Find the lowest deliverable signal with a user handler.
    let blocked_chirho = task_chirho.signal_state_chirho.blocked_chirho;
    let deliverable_chirho = pending_chirho & !blocked_chirho;
    if deliverable_chirho == 0 {
        return false;
    }

    for signo_chirho in 1..=MAX_SIGNAL_CHIRHO {
        if deliverable_chirho & (1u64 << signo_chirho) == 0 {
            continue;
        }

        let idx_chirho = (signo_chirho as usize).saturating_sub(1);
        let action_chirho = task_chirho.signal_state_chirho.actions_chirho[idx_chirho].clone();

        match action_chirho {
            SignalActionChirho::HandlerChirho {
                handler_chirho,
                flags_chirho: _,
                mask_chirho,
                restorer_chirho,
            } => {
                // Dequeue the signal.
                task_chirho.pending_signals_chirho &= !(1u64 << signo_chirho);
                task_chirho
                    .signal_state_chirho
                    .pending_chirho
                    .dequeue_chirho(signo_chirho);

                // Save old blocked mask and block handler signals.
                let old_mask_chirho = task_chirho.signal_state_chirho.blocked_chirho;
                task_chirho.signal_state_chirho.blocked_chirho |=
                    mask_chirho | (1u64 << signo_chirho);

                // Build the signal frame on the user stack.
                let user_rsp_chirho = frame_chirho.rsp_chirho;

                // Ensure 16-byte alignment for the sigframe.
                let frame_size_chirho =
                    core::mem::size_of::<RtSigframeChirho>() as u64;
                let new_rsp_chirho =
                    (user_rsp_chirho - frame_size_chirho) & !0xF;

                let sigframe_chirho = RtSigframeChirho {
                    restorer_chirho,
                    saved_rip_chirho: frame_chirho.rcx_chirho,
                    saved_rsp_chirho: user_rsp_chirho,
                    saved_rax_chirho: frame_chirho.rax_chirho,
                    saved_rcx_chirho: frame_chirho.rcx_chirho,
                    saved_r11_chirho: frame_chirho.r11_chirho,
                    saved_rdi_chirho: frame_chirho.rdi_chirho,
                    saved_rsi_chirho: frame_chirho.rsi_chirho,
                    saved_rdx_chirho: frame_chirho.rdx_chirho,
                    old_mask_chirho,
                    signo_chirho: signo_chirho as u64,
                };

                // Write the sigframe to the user stack.
                let dst_chirho = new_rsp_chirho as *mut RtSigframeChirho;
                unsafe {
                    core::ptr::write_volatile(dst_chirho, sigframe_chirho);
                }

                // Redirect execution to the signal handler.
                // RDI = signo (first argument to handler)
                // RCX = restorer address (SYSRET return target after handler RETs)
                // RSP = sigframe on user stack
                // The handler does: void handler(int signo) { ... }
                // Then RETs to sa_restorer which does rt_sigreturn.
                frame_chirho.rdi_chirho = signo_chirho as u64;
                frame_chirho.rcx_chirho = handler_chirho;
                frame_chirho.rsp_chirho = new_rsp_chirho;
                // R11 should have user RFLAGS (IF set)
                frame_chirho.r11_chirho = 0x200; // IF

                crate::serial_println_chirho!(
                    "[SIGNAL] Delivering sig {} to PID {} handler={:#x} restorer={:#x} new_rsp={:#x}",
                    signo_chirho, task_chirho.pid_chirho,
                    handler_chirho, restorer_chirho, new_rsp_chirho,
                );

                drop(task_chirho);
                return true;
            }
            SignalActionChirho::IgnoreChirho => {
                // Drop the signal.
                task_chirho.pending_signals_chirho &= !(1u64 << signo_chirho);
                task_chirho
                    .signal_state_chirho
                    .pending_chirho
                    .dequeue_chirho(signo_chirho);
                continue;
            }
            SignalActionChirho::DefaultChirho => {
                // Let the existing fatal signal handler deal with defaults.
                continue;
            }
        }
    }

    false
}
