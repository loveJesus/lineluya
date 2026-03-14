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

    // If the signal is SIGCONT, wake a stopped/blocked task.
    if signo_chirho == SIGCONT_CHIRHO {
        task_chirho.wake_chirho();
    }

    // For SIGKILL, ensure the task becomes runnable so the scheduler can
    // reap it.
    if signo_chirho == SIGKILL_CHIRHO
        && task_chirho.state_chirho == TaskStateChirho::BlockedChirho
    {
        task_chirho.wake_chirho();
    }

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

/// `sys_rt_sigaction(signum, act, oldact, sigsetsize)` — examine and change
/// a signal action.
///
/// **Stub**: validates the signal number and returns success without actually
/// modifying dispositions.  User-space handler installation requires the
/// signal-return trampoline (`sigreturn`), which is not yet implemented.
///
/// # Returns
///
/// `0` on success, `-22` (EINVAL) for invalid signal numbers.
pub fn sys_rt_sigaction_chirho(
    signum_chirho: u32,
    _act_chirho: u64,
    _oldact_chirho: u64,
    _sigsetsize_chirho: u64,
) -> i64 {
    if signum_chirho == 0 || signum_chirho > MAX_SIGNAL_CHIRHO {
        return -22; // EINVAL
    }
    // SIGKILL and SIGSTOP dispositions cannot be changed.
    if signum_chirho == SIGKILL_CHIRHO || signum_chirho == SIGSTOP_CHIRHO {
        return -22; // EINVAL
    }
    // Stub: silently succeed.
    0
}

/// `sys_rt_sigprocmask(how, set, oldset, sigsetsize)` — examine and change
/// blocked signals.
///
/// **Stub**: returns success without modifying the mask.  Full
/// implementation will read/write `task.signal_mask_chirho`.
///
/// # Returns
///
/// `0` on success, `-22` (EINVAL) for an invalid `how_chirho` value.
pub fn sys_rt_sigprocmask_chirho(
    how_chirho: u32,
    _set_chirho: u64,
    _oldset_chirho: u64,
    _sigsetsize_chirho: u64,
) -> i64 {
    // Valid `how` values: SIG_BLOCK=0, SIG_UNBLOCK=1, SIG_SETMASK=2
    if how_chirho > 2 {
        return -22; // EINVAL
    }
    // Stub: silently succeed.
    0
}
