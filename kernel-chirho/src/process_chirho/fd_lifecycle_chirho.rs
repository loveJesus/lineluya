// For God so loved the world, that he gave his only begotten Son,
// that whosoever believeth in him should not perish, but have everlasting life.
// — John 3:16 (KJV)

//! Process-exit file-descriptor retirement.
//!
//! Ordinary exits detach and retire descriptor tables immediately. Exception
//! and allocation-failure exits cannot safely acquire VFS locks, so they mark
//! an exact unit of deferred work for a bounded cold-path scan.

use alloc::sync::Arc;
use core::sync::atomic::{AtomicUsize, Ordering};
use spin::Mutex;

use super::{TaskChirho, TaskStateChirho, TASK_LIST_CHIRHO};

/// Number of exited tasks whose descriptor tables still need VFS-safe
/// retirement. An exact count lets the O(1) fast path disarm after the final
/// deferred table is retired.
static DEFERRED_FD_RETIREMENT_COUNT_CHIRHO: AtomicUsize = AtomicUsize::new(0);

/// Bound diagnostics for impossible deferred-retirement counter transitions.
static DEFERRED_FD_RETIREMENT_INVARIANT_COUNT_CHIRHO: AtomicUsize = AtomicUsize::new(0);
const DEFERRED_FD_RETIREMENT_INVARIANT_LIMIT_CHIRHO: usize = 8;

/// Cursor and per-syscall budget for the exceptional-exit cold scan. The task
/// list may grow, so one syscall must not turn cleanup into an unbounded walk.
static DEFERRED_FD_RETIREMENT_SCAN_CURSOR_CHIRHO: AtomicUsize = AtomicUsize::new(0);
const DEFERRED_FD_RETIREMENT_SCAN_BUDGET_CHIRHO: usize = 32;

fn report_deferred_fd_retirement_invariant_chirho(operation_chirho: &str) {
    let report_index_chirho = DEFERRED_FD_RETIREMENT_INVARIANT_COUNT_CHIRHO
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value_chirho| {
            Some(value_chirho.saturating_add(1))
        })
        .unwrap_or_else(|value_chirho| value_chirho);
    if report_index_chirho < DEFERRED_FD_RETIREMENT_INVARIANT_LIMIT_CHIRHO {
        crate::serial_println_chirho!(
            "[FD-RETIRE-INVARIANT] #{} op={} pending={}",
            report_index_chirho,
            operation_chirho,
            DEFERRED_FD_RETIREMENT_COUNT_CHIRHO.load(Ordering::Acquire),
        );
    }
}

fn mark_deferred_fd_retirement_chirho() {
    let previous_count_chirho = DEFERRED_FD_RETIREMENT_COUNT_CHIRHO
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |value_chirho| {
            Some(value_chirho.saturating_add(1))
        })
        .unwrap_or_else(|value_chirho| value_chirho);
    if previous_count_chirho == usize::MAX {
        report_deferred_fd_retirement_invariant_chirho("pending-overflow");
    }
}

pub(super) fn complete_deferred_fd_retirement_chirho(operation_chirho: &str) {
    let previous_count_chirho = DEFERRED_FD_RETIREMENT_COUNT_CHIRHO
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |value_chirho| {
            Some(value_chirho.saturating_sub(1))
        })
        .unwrap_or_else(|value_chirho| value_chirho);
    if previous_count_chirho == 0 {
        report_deferred_fd_retirement_invariant_chirho(operation_chirho);
    }
}

/// Transition one task to Zombie and retire its descriptors after releasing
/// the task lock. Closure happens at exit, not wait4, so pipe EOF and EPIPE do
/// not depend on when the parent reaps.
pub fn exit_task_and_retire_descriptors_chirho(
    task_arc_chirho: &Arc<Mutex<TaskChirho>>,
    exit_code_chirho: i32,
) -> (u64, u64) {
    let (pid_chirho, ppid_chirho, fd_table_chirho, was_deferred_chirho) = {
        let mut task_chirho = task_arc_chirho.lock();
        let was_deferred_chirho =
            task_chirho.is_exited_chirho() && task_chirho.fd_table_chirho.is_some();
        task_chirho.exit_code_chirho = exit_code_chirho;
        task_chirho.state_chirho = TaskStateChirho::ZombieChirho;
        (
            task_chirho.pid_chirho,
            task_chirho.ppid_chirho,
            task_chirho.fd_table_chirho.take(),
            was_deferred_chirho,
        )
    };

    if let Some(mut fd_table_chirho) = fd_table_chirho {
        fd_table_chirho.retire_all_descriptors_chirho();
        if was_deferred_chirho {
            complete_deferred_fd_retirement_chirho("ordinary-exit-after-defer");
        }
    }
    (pid_chirho, ppid_chirho)
}

/// Mark a task exited without touching VFS state.
///
/// Allocation-failure and exception handlers may have interrupted code that
/// owns allocator or VFS locks. A later syscall calls
/// [`drain_deferred_fd_retirements_chirho`] from ordinary task context.
pub fn exit_task_with_deferred_descriptor_retirement_chirho(
    task_arc_chirho: &Arc<Mutex<TaskChirho>>,
    exit_code_chirho: i32,
) -> (u64, u64) {
    let (pid_chirho, ppid_chirho, newly_deferred_chirho) = {
        let mut task_chirho = task_arc_chirho.lock();
        let newly_deferred_chirho =
            !task_chirho.is_exited_chirho() && task_chirho.fd_table_chirho.is_some();
        task_chirho.exit_code_chirho = exit_code_chirho;
        task_chirho.state_chirho = TaskStateChirho::ZombieChirho;
        (
            task_chirho.pid_chirho,
            task_chirho.ppid_chirho,
            newly_deferred_chirho,
        )
    };
    if newly_deferred_chirho {
        mark_deferred_fd_retirement_chirho();
    }
    (pid_chirho, ppid_chirho)
}

/// Drain at most one descriptor table left by an exceptional exit.
///
/// The atomic fast path is O(1). Only pending work triggers a bounded task-list
/// scan, and File -> Inode -> Pipe locks are acquired only after both the task
/// list and candidate task locks have been released.
pub fn drain_deferred_fd_retirements_chirho() {
    if DEFERRED_FD_RETIREMENT_COUNT_CHIRHO.load(Ordering::Acquire) == 0 {
        crate::vfs_chirho::report_unretired_fd_table_drops_chirho();
        return;
    }

    let candidate_task_chirho = {
        let task_list_chirho = TASK_LIST_CHIRHO.lock();
        let task_count_chirho = task_list_chirho.len();
        if task_count_chirho == 0 {
            None
        } else {
            let scan_start_chirho = DEFERRED_FD_RETIREMENT_SCAN_CURSOR_CHIRHO
                .fetch_add(DEFERRED_FD_RETIREMENT_SCAN_BUDGET_CHIRHO, Ordering::Relaxed)
                % task_count_chirho;
            let scan_count_chirho =
                core::cmp::min(task_count_chirho, DEFERRED_FD_RETIREMENT_SCAN_BUDGET_CHIRHO);
            let mut candidate_task_chirho = None;
            for scan_offset_chirho in 0..scan_count_chirho {
                let task_index_chirho =
                    (scan_start_chirho + scan_offset_chirho) % task_count_chirho;
                let task_arc_chirho = &task_list_chirho[task_index_chirho];
                let Some(task_chirho) = task_arc_chirho.try_lock() else {
                    continue;
                };
                if candidate_task_chirho.is_none()
                    && task_chirho.is_exited_chirho()
                    && task_chirho.fd_table_chirho.is_some()
                {
                    candidate_task_chirho = Some(Arc::clone(task_arc_chirho));
                }
            }
            candidate_task_chirho
        }
    };

    if let Some(candidate_task_chirho) = candidate_task_chirho {
        let fd_table_chirho = {
            let mut task_chirho = candidate_task_chirho.lock();
            if task_chirho.is_exited_chirho() {
                task_chirho.fd_table_chirho.take()
            } else {
                None
            }
        };
        if let Some(mut fd_table_chirho) = fd_table_chirho {
            fd_table_chirho.retire_all_descriptors_chirho();
            complete_deferred_fd_retirement_chirho("cold-drain");
        }
    }

    crate::vfs_chirho::report_unretired_fd_table_drops_chirho();
}
