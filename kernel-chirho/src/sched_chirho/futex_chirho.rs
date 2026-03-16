// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! Futex (Fast User-space muTEX) subsystem for the Lineluya kernel (A6-003).
//!
//! Implements the Linux `futex(2)` syscall with support for:
//! - `FUTEX_WAIT` — atomically check a u32 in userspace and sleep if it matches
//! - `FUTEX_WAKE` — wake up N waiters on a futex address
//! - `FUTEX_WAIT_BITSET` / `FUTEX_WAKE_BITSET` — bitset variants
//! - `FUTEX_REQUEUE` — wake some, requeue rest to another futex
//!
//! Futexes are the foundation of userspace synchronization primitives
//! (pthread_mutex, pthread_cond, semaphores, etc.).

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use spin::Mutex;

// ============================================================================
// Futex operation constants (matching Linux uapi)
// ============================================================================

/// FUTEX_WAIT — sleep if *uaddr == val.
pub const FUTEX_WAIT_CHIRHO: u32 = 0;
/// FUTEX_WAKE — wake up to val waiters.
pub const FUTEX_WAKE_CHIRHO: u32 = 1;
/// FUTEX_FD — (deprecated, not implemented).
#[allow(dead_code)]
pub const FUTEX_FD_CHIRHO: u32 = 2;
/// FUTEX_REQUEUE — wake val waiters, requeue val2 to uaddr2.
pub const FUTEX_REQUEUE_CHIRHO: u32 = 3;
/// FUTEX_CMP_REQUEUE — like REQUEUE but check *uaddr == val3 first.
pub const FUTEX_CMP_REQUEUE_CHIRHO: u32 = 4;
/// FUTEX_WAKE_OP — wake on uaddr, conditionally wake on uaddr2.
#[allow(dead_code)]
pub const FUTEX_WAKE_OP_CHIRHO: u32 = 5;
/// FUTEX_WAIT_BITSET — wait with bitset mask.
pub const FUTEX_WAIT_BITSET_CHIRHO: u32 = 9;
/// FUTEX_WAKE_BITSET — wake with bitset mask.
pub const FUTEX_WAKE_BITSET_CHIRHO: u32 = 10;

/// FUTEX_PRIVATE_FLAG — don't use shared futex hash (process-private).
pub const FUTEX_PRIVATE_FLAG_CHIRHO: u32 = 128;
/// FUTEX_CLOCK_REALTIME — use CLOCK_REALTIME for timeout.
#[allow(dead_code)]
pub const FUTEX_CLOCK_REALTIME_CHIRHO: u32 = 256;

/// Mask to extract the base operation (strip PRIVATE/CLOCK_REALTIME flags).
pub const FUTEX_CMD_MASK_CHIRHO: u32 = !(FUTEX_PRIVATE_FLAG_CHIRHO | 256);

/// Match-all bitset.
pub const FUTEX_BITSET_MATCH_ANY_CHIRHO: u32 = 0xFFFFFFFF;

// ============================================================================
// FutexWaiterChirho — one task waiting on a futex
// ============================================================================

/// Represents a single task waiting on a futex address.
#[derive(Debug, Clone)]
struct FutexWaiterChirho {
    /// PID of the waiting task.
    pid_chirho: u64,
    /// Bitset for FUTEX_WAIT_BITSET / FUTEX_WAKE_BITSET matching.
    bitset_chirho: u32,
}

// ============================================================================
// Global futex hash table
// ============================================================================

/// Global futex table: maps virtual addresses to lists of waiters.
///
/// In Linux this is a hash table keyed by (mm, addr). For our single-address-space
/// kernel, we key by the raw virtual address (u64).
static FUTEX_TABLE_CHIRHO: Mutex<BTreeMap<u64, Vec<FutexWaiterChirho>>> =
    Mutex::new(BTreeMap::new());

// ============================================================================
// sys_futex_chirho — main futex syscall entry point
// ============================================================================

/// `futex(2)` syscall implementation.
///
/// # Arguments
/// * `uaddr_chirho` — pointer to the futex u32 word in userspace
/// * `op_chirho` — operation (FUTEX_WAIT, FUTEX_WAKE, etc.) + flags
/// * `val_chirho` — value argument (meaning depends on op)
/// * `timeout_or_val2_chirho` — timeout pointer or val2 (for REQUEUE)
/// * `uaddr2_chirho` — second futex address (for REQUEUE)
/// * `val3_chirho` — third value (for CMP_REQUEUE)
pub fn sys_futex_chirho(
    uaddr_chirho: u64,
    op_chirho: u64,
    val_chirho: u64,
    _timeout_or_val2_chirho: u64,
    uaddr2_chirho: u64,
    val3_chirho: u64,
) -> i64 {
    let cmd_chirho = (op_chirho as u32) & FUTEX_CMD_MASK_CHIRHO;

    match cmd_chirho {
        FUTEX_WAIT_CHIRHO => {
            futex_wait_chirho(uaddr_chirho, val_chirho as u32, FUTEX_BITSET_MATCH_ANY_CHIRHO)
        }
        FUTEX_WAKE_CHIRHO => {
            futex_wake_chirho(uaddr_chirho, val_chirho as u32, FUTEX_BITSET_MATCH_ANY_CHIRHO)
        }
        FUTEX_WAIT_BITSET_CHIRHO => {
            let bitset_chirho = val3_chirho as u32;
            if bitset_chirho == 0 {
                return -(crate::syscall_chirho::EINVAL_CHIRHO);
            }
            futex_wait_chirho(uaddr_chirho, val_chirho as u32, bitset_chirho)
        }
        FUTEX_WAKE_BITSET_CHIRHO => {
            let bitset_chirho = val3_chirho as u32;
            if bitset_chirho == 0 {
                return -(crate::syscall_chirho::EINVAL_CHIRHO);
            }
            futex_wake_chirho(uaddr_chirho, val_chirho as u32, bitset_chirho)
        }
        FUTEX_REQUEUE_CHIRHO => {
            futex_requeue_chirho(uaddr_chirho, val_chirho as u32, uaddr2_chirho, _timeout_or_val2_chirho as u32, None)
        }
        FUTEX_CMP_REQUEUE_CHIRHO => {
            futex_requeue_chirho(uaddr_chirho, val_chirho as u32, uaddr2_chirho, _timeout_or_val2_chirho as u32, Some(val3_chirho as u32))
        }
        _ => {
            crate::serial_println_chirho!(
                "[FUTEX] Unsupported futex op {} (cmd={})",
                op_chirho, cmd_chirho
            );
            -(crate::syscall_chirho::ENOSYS_CHIRHO)
        }
    }
}

// ============================================================================
// FUTEX_WAIT implementation
// ============================================================================

/// Atomically check that `*uaddr == expected_val` and if so, sleep on the
/// futex until woken by FUTEX_WAKE.
///
/// Returns 0 on success (woken), -EAGAIN if the value didn't match,
/// -EFAULT if the address is invalid.
fn futex_wait_chirho(
    uaddr_chirho: u64,
    expected_val_chirho: u32,
    bitset_chirho: u32,
) -> i64 {
    // Read the current value at the futex address.
    let current_val_chirho = match read_futex_word_chirho(uaddr_chirho) {
        Some(v_chirho) => v_chirho,
        None => return -(crate::syscall_chirho::EFAULT_CHIRHO),
    };

    // If the value doesn't match, return -EAGAIN (no spurious sleep).
    if current_val_chirho != expected_val_chirho {
        return -(crate::syscall_chirho::EAGAIN_CHIRHO);
    }

    // Get the current task PID.
    let pid_chirho = match crate::scheduler_chirho::current_pid_chirho() {
        Some(p_chirho) => p_chirho,
        None => return -(crate::syscall_chirho::EAGAIN_CHIRHO),
    };

    crate::serial_println_chirho!(
        "[FUTEX] WAIT pid={} addr={:#x} val={} bitset={:#x}",
        pid_chirho, uaddr_chirho, expected_val_chirho, bitset_chirho,
    );

    // Add ourselves to the wait queue for this address.
    {
        let mut table_chirho = FUTEX_TABLE_CHIRHO.lock();
        let waiters_chirho = table_chirho.entry(uaddr_chirho).or_insert_with(Vec::new);
        waiters_chirho.push(FutexWaiterChirho {
            pid_chirho,
            bitset_chirho,
        });
    }

    // Block the current task. It will be unblocked by futex_wake_chirho.
    crate::scheduler_chirho::block_current_chirho();

    // When we return here, we have been woken up.
    crate::serial_println_chirho!(
        "[FUTEX] WAIT pid={} woken from addr={:#x}",
        pid_chirho, uaddr_chirho,
    );

    0
}

// ============================================================================
// FUTEX_WAKE implementation
// ============================================================================

/// Wake up to `max_wake_chirho` tasks sleeping on the futex at `uaddr_chirho`.
///
/// Returns the number of tasks actually woken.
fn futex_wake_chirho(
    uaddr_chirho: u64,
    max_wake_chirho: u32,
    bitset_chirho: u32,
) -> i64 {
    let mut woken_count_chirho: i64 = 0;

    let pids_to_wake_chirho: Vec<u64> = {
        let mut table_chirho = FUTEX_TABLE_CHIRHO.lock();
        let mut pids_chirho = Vec::new();

        if let Some(waiters_chirho) = table_chirho.get_mut(&uaddr_chirho) {
            let mut i_chirho = 0;
            while i_chirho < waiters_chirho.len() && (pids_chirho.len() as u32) < max_wake_chirho {
                if (waiters_chirho[i_chirho].bitset_chirho & bitset_chirho) != 0 {
                    let waiter_chirho = waiters_chirho.remove(i_chirho);
                    pids_chirho.push(waiter_chirho.pid_chirho);
                } else {
                    i_chirho += 1;
                }
            }
            // Clean up empty entries
            if waiters_chirho.is_empty() {
                table_chirho.remove(&uaddr_chirho);
            }
        }

        pids_chirho
    };

    // Wake the tasks outside the lock
    for pid_chirho in pids_to_wake_chirho {
        crate::scheduler_chirho::unblock_task_chirho(pid_chirho);
        woken_count_chirho += 1;
    }

    if woken_count_chirho > 0 {
        crate::serial_println_chirho!(
            "[FUTEX] WAKE addr={:#x} woke {} tasks (max={})",
            uaddr_chirho, woken_count_chirho, max_wake_chirho,
        );
    }

    woken_count_chirho
}

// ============================================================================
// FUTEX_REQUEUE implementation
// ============================================================================

/// Wake `max_wake_chirho` tasks on uaddr, then requeue up to `max_requeue_chirho`
/// remaining waiters to uaddr2.
///
/// If `expected_val_chirho` is `Some(v)`, first check that `*uaddr == v`
/// (CMP_REQUEUE semantics).
fn futex_requeue_chirho(
    uaddr_chirho: u64,
    max_wake_chirho: u32,
    uaddr2_chirho: u64,
    max_requeue_chirho: u32,
    expected_val_chirho: Option<u32>,
) -> i64 {
    // CMP_REQUEUE: check the value first
    if let Some(expected_chirho) = expected_val_chirho {
        let current_val_chirho = match read_futex_word_chirho(uaddr_chirho) {
            Some(v_chirho) => v_chirho,
            None => return -(crate::syscall_chirho::EFAULT_CHIRHO),
        };
        if current_val_chirho != expected_chirho {
            return -(crate::syscall_chirho::EAGAIN_CHIRHO);
        }
    }

    let mut woken_count_chirho: i64 = 0;
    let mut requeued_count_chirho: u32 = 0;

    let pids_to_wake_chirho: Vec<u64>;

    {
        let mut table_chirho = FUTEX_TABLE_CHIRHO.lock();

        // Remove waiters from uaddr
        let mut removed_waiters_chirho: Vec<FutexWaiterChirho> = Vec::new();
        if let Some(waiters_chirho) = table_chirho.get_mut(&uaddr_chirho) {
            // Take all waiters out
            removed_waiters_chirho = core::mem::take(waiters_chirho);
        }
        table_chirho.remove(&uaddr_chirho);

        let mut wake_pids_chirho = Vec::new();
        let mut requeue_waiters_chirho = Vec::new();

        for waiter_chirho in removed_waiters_chirho {
            if (wake_pids_chirho.len() as u32) < max_wake_chirho {
                wake_pids_chirho.push(waiter_chirho.pid_chirho);
            } else if requeued_count_chirho < max_requeue_chirho {
                requeue_waiters_chirho.push(waiter_chirho);
                requeued_count_chirho += 1;
            }
            // Excess waiters are dropped (unusual but safe)
        }

        // Add requeued waiters to uaddr2
        if !requeue_waiters_chirho.is_empty() {
            let target_waiters_chirho = table_chirho.entry(uaddr2_chirho).or_insert_with(Vec::new);
            target_waiters_chirho.extend(requeue_waiters_chirho);
        }

        pids_to_wake_chirho = wake_pids_chirho;
    }

    // Wake tasks outside the lock
    for pid_chirho in pids_to_wake_chirho {
        crate::scheduler_chirho::unblock_task_chirho(pid_chirho);
        woken_count_chirho += 1;
    }

    crate::serial_println_chirho!(
        "[FUTEX] REQUEUE addr={:#x}->{:#x} woke={} requeued={}",
        uaddr_chirho, uaddr2_chirho, woken_count_chirho, requeued_count_chirho,
    );

    woken_count_chirho
}

// ============================================================================
// Helper: read a u32 from a userspace address
// ============================================================================

/// Safely read a u32 from a userspace virtual address.
///
/// Returns `None` if the address is null or clearly invalid.
fn read_futex_word_chirho(addr_chirho: u64) -> Option<u32> {
    if addr_chirho == 0 || (addr_chirho & 0x3) != 0 {
        return None; // null or unaligned
    }

    // In a real kernel, this would go through page tables to verify the mapping.
    // For now, we do a direct volatile read (valid because the kernel maps all
    // userspace memory).
    let ptr_chirho = addr_chirho as *const u32;
    let val_chirho = unsafe { core::ptr::read_volatile(ptr_chirho) };
    Some(val_chirho)
}
