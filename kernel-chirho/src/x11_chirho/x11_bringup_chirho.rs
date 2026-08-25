// For God so loved the world, that he gave his only begotten Son, that whosoever
// believeth in him should not perish, but have everlasting life. — John 3:16 (KJV)
//
// X11 bring-up coordination.
//
// This module owns the ENTIRE handshake between Xorg and its clients. It exists
// because that handshake was previously spread across four call sites keyed on
// mutually-contradictory hardcoded PID ranges, with the one-shot consume call
// stranded in a branch that could never execute.
//
// Two rules hold this together:
//
//   1. Xorg is identified by its EXECUTABLE PATH, never by PID. PIDs shift
//      whenever the rootfs launch order changes; the old `pid >= 3 && pid <= 7`
//      gate silently became dead code the moment Xorg landed on PID 9.
//
//   2. The waiting-client drain is NOT latched. Only the log line is one-shot.
//      A client that parks AFTER Xorg's first epoll_wait must still be woken,
//      otherwise it sleeps forever — that is the race that wedged the boot.
//
// Client LAUNCH is deliberately not handled here. The rootfs rc script served by
// vfs_ops_chirho is the single launcher; this module only gates readiness.
//
// Workflow: spec-chirho/workflows-chirho/x11-bringup-chirho.md

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use alloc::vec::Vec;
use spin::Mutex;

/// Set once Xorg has bound the X11 display socket (`@/tmp/.X11-unix/X0`).
/// The socket existing does NOT mean Xorg is servicing it yet.
static DISPLAY_SOCKET_READY_CHIRHO: AtomicBool = AtomicBool::new(false);

/// Set once Xorg has entered its epoll-based event loop. THIS is the point at
/// which connecting clients will actually be served.
static XORG_ACCEPTING_CHIRHO: AtomicBool = AtomicBool::new(false);

/// PID of the running Xorg, learned from the first epoll wait made by a task
/// whose executable basename is `Xorg`. Zero means "not yet identified".
static XORG_PID_CHIRHO: AtomicU64 = AtomicU64::new(0);

/// Clients parked in the AF_UNIX connect retry, awaiting Xorg.
static WAITING_PIDS_CHIRHO: Mutex<Vec<u64>> = Mutex::new(Vec::new());

/// Guards the one-time "Xorg is up" announcement. Never guards the drain.
static ANNOUNCED_CHIRHO: AtomicBool = AtomicBool::new(false);

/// Executable basename that identifies the X server.
const XORG_BASENAME_CHIRHO: &[u8] = b"Xorg";

// ============================================================================
// Identity
// ============================================================================

/// Copy the calling task's executable path out of its task struct.
///
/// The task lock is released before returning so that no caller can print
/// while holding it — serial output under a subsystem lock has deadlocked this
/// kernel before (see the TCP send fix under SOCKET_TABLE_CHIRHO).
fn current_exe_path_chirho(out_chirho: &mut [u8; 128]) -> usize {
    let task_arc_chirho = match crate::task_chirho::current_task_chirho() {
        Some(t_chirho) => t_chirho,
        None => return 0,
    };
    let guard_chirho = task_arc_chirho.lock();
    let len_chirho = core::cmp::min(guard_chirho.exe_path_len_chirho, out_chirho.len());
    out_chirho[..len_chirho].copy_from_slice(&guard_chirho.exe_path_chirho[..len_chirho]);
    drop(guard_chirho);
    len_chirho
}

/// Basename of a path slice — everything after the final `/`.
fn basename_chirho(path_chirho: &[u8]) -> &[u8] {
    match path_chirho.iter().rposition(|b_chirho| *b_chirho == b'/') {
        Some(idx_chirho) => &path_chirho[idx_chirho + 1..],
        None => path_chirho,
    }
}

/// True when the CALLING task is the X server.
///
/// Matching on the executable basename keeps this correct across every path
/// Xorg is launched from — `/tmp/lib-chirho/Xorg`, `/usr/bin/Xorg`, and
/// `/usr/libexec/Xorg` all resolve alike.
pub fn current_task_is_xorg_chirho() -> bool {
    let mut buf_chirho = [0u8; 128];
    let len_chirho = current_exe_path_chirho(&mut buf_chirho);
    if len_chirho == 0 {
        return false;
    }
    basename_chirho(&buf_chirho[..len_chirho]) == XORG_BASENAME_CHIRHO
}

/// PID of the X server once identified, else `None`.
pub fn xorg_pid_chirho() -> Option<u64> {
    match XORG_PID_CHIRHO.load(Ordering::Acquire) {
        0 => None,
        pid_chirho => Some(pid_chirho),
    }
}

// ============================================================================
// Readiness queries
// ============================================================================

/// True once Xorg has bound the display socket.
pub fn display_socket_ready_chirho() -> bool {
    DISPLAY_SOCKET_READY_CHIRHO.load(Ordering::Acquire)
}

/// True once Xorg is servicing its event loop, i.e. a connect will be answered.
///
/// The connect path checks this BEFORE parking: a client arriving after Xorg is
/// already looping must never park, because parking is only unwound by a future
/// epoll wait that may never come.
pub fn xorg_accepting_clients_chirho() -> bool {
    XORG_ACCEPTING_CHIRHO.load(Ordering::Acquire)
}

// ============================================================================
// Events
// ============================================================================

/// Called from the AF_UNIX bind path when Xorg binds the display socket.
pub fn on_display_socket_bound_chirho() {
    DISPLAY_SOCKET_READY_CHIRHO.store(true, Ordering::Release);
    crate::serial_println_chirho!("[X11-BRINGUP] display socket bound");
}

/// Called from the epoll syscall handler on EVERY epoll wait.
///
/// When the caller is Xorg this marks the server as accepting and drains any
/// parked clients. The drain runs every time, not once: clients that park after
/// the first wait are exactly the ones the old latched implementation stranded.
pub fn on_epoll_wait_chirho(pid_chirho: u64) {
    if !current_task_is_xorg_chirho() {
        return;
    }

    XORG_PID_CHIRHO.store(pid_chirho, Ordering::Release);
    XORG_ACCEPTING_CHIRHO.store(true, Ordering::Release);

    if !ANNOUNCED_CHIRHO.swap(true, Ordering::AcqRel) {
        crate::serial_println_chirho!(
            "[XORG-MAIN-LOOP] PID {} entered epoll_wait — Xorg ready for clients",
            pid_chirho,
        );
    }

    drain_waiting_clients_chirho();
}

/// Re-admit every parked client to the scheduler.
///
/// Uses `unblock_task_chirho`, NOT `add_task_chirho`. A parked client is parked
/// via `block_current_chirho`, which sets its state to Sleeping so the scheduler
/// stops requeueing it. `add_task_chirho` only pushes a PID onto the run queue
/// and never touches task state, so on a Sleeping task it is a silent no-op —
/// the task would be queued and then discarded, and would never run again.
/// `unblock_task_chirho` sets Ready first, then queues. The pair must match.
///
/// The waiting list is taken under the lock and released before any scheduler
/// call or serial print, so a woken task can park again immediately without
/// contending with this drain.
fn drain_waiting_clients_chirho() {
    let waiting_chirho = {
        let mut list_chirho = WAITING_PIDS_CHIRHO.lock();
        if list_chirho.is_empty() {
            return;
        }
        core::mem::take(&mut *list_chirho)
    };

    for wpid_chirho in waiting_chirho {
        crate::serial_println_chirho!("[XORG-WAKE] Re-adding PID {} to scheduler", wpid_chirho);
        crate::scheduler_chirho::unblock_task_chirho(wpid_chirho);
    }
}

/// Record a client as waiting for Xorg.
///
/// The caller MUST follow this with `scheduler_chirho::block_current_chirho()`.
/// Do NOT use `remove_task_chirho(current_pid)` — when the PID is the running
/// task that function only sets `need_resched` and returns, leaving the task
/// Ready, so `schedule_chirho` pushes it straight back onto the queue and it
/// never parks at all.
pub fn register_waiting_client_chirho(pid_chirho: u64) {
    WAITING_PIDS_CHIRHO.lock().push(pid_chirho);
}

/// Number of clients currently parked. Diagnostics only.
pub fn waiting_client_count_chirho() -> usize {
    WAITING_PIDS_CHIRHO.lock().len()
}
