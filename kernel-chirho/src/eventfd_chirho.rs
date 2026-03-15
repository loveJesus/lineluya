// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! eventfd, timerfd, and signalfd implementations for the Lineluya kernel
//! (A6-018).
//!
//! These are Linux-specific file descriptor types that integrate with
//! epoll/poll/select for event-driven programming:
//!
//! - `eventfd(2)`: inter-process / inter-thread signaling via a 64-bit counter
//! - `timerfd_create/settime/gettime`: timer-based file descriptors
//! - `signalfd(2)`: receive signals via a file descriptor
//!
//! Reference: eventfd(2), timerfd_create(2), signalfd(2)

extern crate alloc;

use alloc::collections::BTreeMap;
use spin::Mutex;
use core::sync::atomic::{AtomicU64, Ordering};

// ============================================================================
// eventfd
// ============================================================================

/// eventfd flags.
#[allow(dead_code)]
pub const EFD_CLOEXEC_CHIRHO: u32 = 0o2000000;
#[allow(dead_code)]
pub const EFD_NONBLOCK_CHIRHO: u32 = 0o4000;
#[allow(dead_code)]
pub const EFD_SEMAPHORE_CHIRHO: u32 = 1;

/// An eventfd instance.
#[derive(Debug)]
#[allow(dead_code)]
struct EventFdChirho {
    /// Current counter value.
    counter_chirho: AtomicU64,
    /// Flags (EFD_SEMAPHORE, etc.).
    flags_chirho: u32,
}

/// Global eventfd registry: maps fd number to eventfd instance.
static EVENTFD_REGISTRY_CHIRHO: Mutex<BTreeMap<i32, EventFdChirho>> =
    Mutex::new(BTreeMap::new());

/// Next eventfd fd number (starting high to avoid conflicts).
static NEXT_EVENTFD_FD_CHIRHO: Mutex<i32> = Mutex::new(2000);

/// `eventfd2(2)` — create an eventfd file descriptor.
#[allow(dead_code)]
pub fn sys_eventfd2_chirho(initval_chirho: u64, flags_chirho: u64) -> i64 {
    let mut next_fd_chirho = NEXT_EVENTFD_FD_CHIRHO.lock();
    let fd_chirho = *next_fd_chirho;
    *next_fd_chirho += 1;

    let efd_chirho = EventFdChirho {
        counter_chirho: AtomicU64::new(initval_chirho),
        flags_chirho: flags_chirho as u32,
    };

    EVENTFD_REGISTRY_CHIRHO.lock().insert(fd_chirho, efd_chirho);

    crate::serial_println_chirho!(
        "[EVENTFD] Created fd={} initval={} flags={:#x}",
        fd_chirho,
        initval_chirho,
        flags_chirho,
    );

    fd_chirho as i64
}

/// Read from an eventfd (returns the counter and resets it).
#[allow(dead_code)]
pub fn eventfd_read_chirho(fd_chirho: i32) -> i64 {
    let registry_chirho = EVENTFD_REGISTRY_CHIRHO.lock();
    match registry_chirho.get(&fd_chirho) {
        Some(efd_chirho) => {
            let val_chirho = if efd_chirho.flags_chirho & EFD_SEMAPHORE_CHIRHO != 0 {
                // Semaphore mode: decrement by 1
                loop {
                    let cur_chirho = efd_chirho.counter_chirho.load(Ordering::Relaxed);
                    if cur_chirho == 0 {
                        return -(crate::syscall_chirho::EAGAIN_CHIRHO);
                    }
                    if efd_chirho
                        .counter_chirho
                        .compare_exchange(
                            cur_chirho,
                            cur_chirho - 1,
                            Ordering::SeqCst,
                            Ordering::Relaxed,
                        )
                        .is_ok()
                    {
                        break 1u64;
                    }
                }
            } else {
                // Normal mode: read and reset
                efd_chirho.counter_chirho.swap(0, Ordering::SeqCst)
            };

            if val_chirho == 0 {
                return -(crate::syscall_chirho::EAGAIN_CHIRHO);
            }
            val_chirho as i64
        }
        None => -(crate::syscall_chirho::EBADF_CHIRHO),
    }
}

/// Write to an eventfd (adds value to the counter).
#[allow(dead_code)]
pub fn eventfd_write_chirho(fd_chirho: i32, value_chirho: u64) -> i64 {
    let registry_chirho = EVENTFD_REGISTRY_CHIRHO.lock();
    match registry_chirho.get(&fd_chirho) {
        Some(efd_chirho) => {
            let max_chirho = u64::MAX - 1;
            let current_chirho = efd_chirho.counter_chirho.load(Ordering::Relaxed);
            if current_chirho > max_chirho - value_chirho {
                return -(crate::syscall_chirho::EAGAIN_CHIRHO);
            }
            efd_chirho
                .counter_chirho
                .fetch_add(value_chirho, Ordering::SeqCst);
            8 // wrote 8 bytes
        }
        None => -(crate::syscall_chirho::EBADF_CHIRHO),
    }
}

// ============================================================================
// timerfd
// ============================================================================

/// Clock IDs.
#[allow(dead_code)]
pub const CLOCK_REALTIME_CHIRHO: i32 = 0;
#[allow(dead_code)]
pub const CLOCK_MONOTONIC_CHIRHO: i32 = 1;

/// timerfd flags.
#[allow(dead_code)]
pub const TFD_CLOEXEC_CHIRHO: u32 = 0o2000000;
#[allow(dead_code)]
pub const TFD_NONBLOCK_CHIRHO: u32 = 0o4000;
#[allow(dead_code)]
pub const TFD_TIMER_ABSTIME_CHIRHO: u32 = 1;

/// A timerfd instance.
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct TimerFdChirho {
    /// Clock ID.
    clock_id_chirho: i32,
    /// Flags.
    flags_chirho: u32,
    /// Interval in nanoseconds (0 = one-shot).
    interval_ns_chirho: u64,
    /// Next expiration in nanoseconds (absolute, monotonic).
    expiration_ns_chirho: u64,
    /// Number of expirations since last read.
    expirations_chirho: u64,
}

/// Global timerfd registry.
static TIMERFD_REGISTRY_CHIRHO: Mutex<BTreeMap<i32, TimerFdChirho>> =
    Mutex::new(BTreeMap::new());

/// Next timerfd fd number.
static NEXT_TIMERFD_FD_CHIRHO: Mutex<i32> = Mutex::new(3000);

/// `timerfd_create(2)` — create a timer file descriptor.
#[allow(dead_code)]
pub fn sys_timerfd_create_chirho(clock_id_chirho: u64, flags_chirho: u64) -> i64 {
    let mut next_fd_chirho = NEXT_TIMERFD_FD_CHIRHO.lock();
    let fd_chirho = *next_fd_chirho;
    *next_fd_chirho += 1;

    let tfd_chirho = TimerFdChirho {
        clock_id_chirho: clock_id_chirho as i32,
        flags_chirho: flags_chirho as u32,
        interval_ns_chirho: 0,
        expiration_ns_chirho: 0,
        expirations_chirho: 0,
    };

    TIMERFD_REGISTRY_CHIRHO.lock().insert(fd_chirho, tfd_chirho);

    crate::serial_println_chirho!(
        "[TIMERFD] Created fd={} clockid={}",
        fd_chirho,
        clock_id_chirho,
    );

    fd_chirho as i64
}

/// `timerfd_settime(2)` — arm/disarm a timer fd.
///
/// `new_value_ptr_chirho` points to `struct itimerspec`:
/// ```c
/// struct timespec { long tv_sec; long tv_nsec; };
/// struct itimerspec { struct timespec it_interval; struct timespec it_value; };
/// ```
#[allow(dead_code)]
pub fn sys_timerfd_settime_chirho(
    fd_chirho: u64,
    _flags_chirho: u64,
    new_value_ptr_chirho: u64,
    _old_value_ptr_chirho: u64,
) -> i64 {
    let fd_i32_chirho = fd_chirho as i32;

    if new_value_ptr_chirho == 0 {
        return -(crate::syscall_chirho::EFAULT_CHIRHO);
    }

    // Read itimerspec (4 longs = 32 bytes on x86_64)
    let it_interval_sec_chirho =
        unsafe { core::ptr::read_unaligned(new_value_ptr_chirho as *const i64) };
    let it_interval_nsec_chirho =
        unsafe { core::ptr::read_unaligned((new_value_ptr_chirho + 8) as *const i64) };
    let it_value_sec_chirho =
        unsafe { core::ptr::read_unaligned((new_value_ptr_chirho + 16) as *const i64) };
    let it_value_nsec_chirho =
        unsafe { core::ptr::read_unaligned((new_value_ptr_chirho + 24) as *const i64) };

    let interval_ns_chirho =
        (it_interval_sec_chirho as u64) * 1_000_000_000 + (it_interval_nsec_chirho as u64);
    let value_ns_chirho =
        (it_value_sec_chirho as u64) * 1_000_000_000 + (it_value_nsec_chirho as u64);

    let mut registry_chirho = TIMERFD_REGISTRY_CHIRHO.lock();
    match registry_chirho.get_mut(&fd_i32_chirho) {
        Some(tfd_chirho) => {
            tfd_chirho.interval_ns_chirho = interval_ns_chirho;
            tfd_chirho.expiration_ns_chirho = value_ns_chirho;
            tfd_chirho.expirations_chirho = 0;
            0
        }
        None => -(crate::syscall_chirho::EBADF_CHIRHO),
    }
}

/// `timerfd_gettime(2)` — get the current timer settings.
#[allow(dead_code)]
pub fn sys_timerfd_gettime_chirho(fd_chirho: u64, curr_value_ptr_chirho: u64) -> i64 {
    let fd_i32_chirho = fd_chirho as i32;

    if curr_value_ptr_chirho == 0 {
        return -(crate::syscall_chirho::EFAULT_CHIRHO);
    }

    let registry_chirho = TIMERFD_REGISTRY_CHIRHO.lock();
    match registry_chirho.get(&fd_i32_chirho) {
        Some(tfd_chirho) => {
            let interval_sec_chirho = (tfd_chirho.interval_ns_chirho / 1_000_000_000) as i64;
            let interval_nsec_chirho = (tfd_chirho.interval_ns_chirho % 1_000_000_000) as i64;
            let value_sec_chirho = (tfd_chirho.expiration_ns_chirho / 1_000_000_000) as i64;
            let value_nsec_chirho = (tfd_chirho.expiration_ns_chirho % 1_000_000_000) as i64;

            unsafe {
                core::ptr::write_unaligned(curr_value_ptr_chirho as *mut i64, interval_sec_chirho);
                core::ptr::write_unaligned(
                    (curr_value_ptr_chirho + 8) as *mut i64,
                    interval_nsec_chirho,
                );
                core::ptr::write_unaligned(
                    (curr_value_ptr_chirho + 16) as *mut i64,
                    value_sec_chirho,
                );
                core::ptr::write_unaligned(
                    (curr_value_ptr_chirho + 24) as *mut i64,
                    value_nsec_chirho,
                );
            }
            0
        }
        None => -(crate::syscall_chirho::EBADF_CHIRHO),
    }
}

// ============================================================================
// signalfd
// ============================================================================

/// A signalfd instance.
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct SignalFdChirho {
    /// Signal mask (which signals to accept).
    sigmask_chirho: u64,
    /// Flags.
    flags_chirho: u32,
}

/// Global signalfd registry.
static SIGNALFD_REGISTRY_CHIRHO: Mutex<BTreeMap<i32, SignalFdChirho>> =
    Mutex::new(BTreeMap::new());

/// Next signalfd fd number.
static NEXT_SIGNALFD_FD_CHIRHO: Mutex<i32> = Mutex::new(4000);

/// `signalfd4(2)` — create or modify a signalfd.
#[allow(dead_code)]
pub fn sys_signalfd4_chirho(
    fd_chirho: u64,
    sigmask_ptr_chirho: u64,
    _sigsetsize_chirho: u64,
    flags_chirho: u64,
) -> i64 {
    let sigmask_chirho = if sigmask_ptr_chirho != 0 {
        unsafe { core::ptr::read_unaligned(sigmask_ptr_chirho as *const u64) }
    } else {
        0
    };

    if fd_chirho as i32 == -1 {
        // Create new signalfd
        let mut next_fd_chirho = NEXT_SIGNALFD_FD_CHIRHO.lock();
        let new_fd_chirho = *next_fd_chirho;
        *next_fd_chirho += 1;

        SIGNALFD_REGISTRY_CHIRHO.lock().insert(
            new_fd_chirho,
            SignalFdChirho {
                sigmask_chirho,
                flags_chirho: flags_chirho as u32,
            },
        );

        crate::serial_println_chirho!(
            "[SIGNALFD] Created fd={} sigmask={:#x}",
            new_fd_chirho,
            sigmask_chirho,
        );

        new_fd_chirho as i64
    } else {
        // Modify existing signalfd
        let fd_i32_chirho = fd_chirho as i32;
        let mut registry_chirho = SIGNALFD_REGISTRY_CHIRHO.lock();
        match registry_chirho.get_mut(&fd_i32_chirho) {
            Some(sfd_chirho) => {
                sfd_chirho.sigmask_chirho = sigmask_chirho;
                fd_i32_chirho as i64
            }
            None => -(crate::syscall_chirho::EBADF_CHIRHO),
        }
    }
}
