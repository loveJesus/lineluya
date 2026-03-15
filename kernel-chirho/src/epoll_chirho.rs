// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! epoll(7) I/O event notification subsystem for the Lineluya kernel (A6).
//!
//! Implements:
//! - `epoll_create1(2)` — create an epoll file descriptor
//! - `epoll_ctl(2)`     — add/modify/delete fd watches
//! - `epoll_wait(2)`    — wait for I/O events
//! - `epoll_pwait(2)`   — wait with signal mask
//!
//! epoll is the primary event loop primitive on Linux, used by libc,
//! nginx, Node.js, systemd, and virtually all async runtimes.
//!
//! Reference: epoll(7) man page, Linux fs/eventpoll.c

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use spin::Mutex;

// ============================================================================
// epoll_event structure (matches Linux uapi)
// ============================================================================

/// Events bitmask.
pub const EPOLLIN_CHIRHO: u32 = 0x001;
#[allow(dead_code)]
pub const EPOLLPRI_CHIRHO: u32 = 0x002;
pub const EPOLLOUT_CHIRHO: u32 = 0x004;
#[allow(dead_code)]
pub const EPOLLERR_CHIRHO: u32 = 0x008;
#[allow(dead_code)]
pub const EPOLLHUP_CHIRHO: u32 = 0x010;
#[allow(dead_code)]
pub const EPOLLRDNORM_CHIRHO: u32 = 0x040;
#[allow(dead_code)]
pub const EPOLLRDBAND_CHIRHO: u32 = 0x080;
#[allow(dead_code)]
pub const EPOLLWRNORM_CHIRHO: u32 = 0x100;
#[allow(dead_code)]
pub const EPOLLWRBAND_CHIRHO: u32 = 0x200;
#[allow(dead_code)]
pub const EPOLLET_CHIRHO: u32 = 1 << 31;
#[allow(dead_code)]
pub const EPOLLONESHOT_CHIRHO: u32 = 1 << 30;

/// epoll_ctl operations.
pub const EPOLL_CTL_ADD_CHIRHO: i32 = 1;
pub const EPOLL_CTL_DEL_CHIRHO: i32 = 2;
pub const EPOLL_CTL_MOD_CHIRHO: i32 = 3;

/// Mirrors `struct epoll_event` from Linux (12 bytes, packed on x86_64).
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub struct EpollEventChirho {
    /// Event mask (EPOLLIN, EPOLLOUT, etc.).
    pub events_chirho: u32,
    /// User data (typically the fd or a pointer).
    pub data_chirho: u64,
}

impl Default for EpollEventChirho {
    fn default() -> Self {
        Self {
            events_chirho: 0,
            data_chirho: 0,
        }
    }
}

// ============================================================================
// Epoll instance
// ============================================================================

/// A watched file descriptor entry.
#[derive(Debug, Clone)]
struct EpollItemChirho {
    /// The file descriptor being watched.
    fd_chirho: i32,
    /// The event mask and user data.
    event_chirho: EpollEventChirho,
}

/// An epoll instance (one per epoll_create1 call).
struct EpollInstanceChirho {
    /// Map of watched fds to their epoll items.
    items_chirho: BTreeMap<i32, EpollItemChirho>,
}

impl EpollInstanceChirho {
    fn new_chirho() -> Self {
        Self {
            items_chirho: BTreeMap::new(),
        }
    }
}

// ============================================================================
// Global epoll registry
// ============================================================================

/// Next epoll fd to allocate. We use fd numbers >= 1000 to avoid
/// conflicts with regular file descriptors.
static NEXT_EPOLL_FD_CHIRHO: Mutex<i32> = Mutex::new(1000);

/// Map of epoll fd -> instance.
static EPOLL_INSTANCES_CHIRHO: Mutex<BTreeMap<i32, EpollInstanceChirho>> =
    Mutex::new(BTreeMap::new());

// ============================================================================
// Syscall implementations
// ============================================================================

/// `epoll_create1(2)` — create a new epoll instance.
///
/// # Arguments
/// * `flags_chirho` — flags (EPOLL_CLOEXEC = 0x80000, currently ignored)
///
/// # Returns
/// The new epoll file descriptor, or negative errno.
#[allow(dead_code)]
pub fn sys_epoll_create1_chirho(_flags_chirho: u64) -> i64 {
    let mut next_fd_chirho = NEXT_EPOLL_FD_CHIRHO.lock();
    let epfd_chirho = *next_fd_chirho;
    *next_fd_chirho += 1;

    let instance_chirho = EpollInstanceChirho::new_chirho();
    EPOLL_INSTANCES_CHIRHO.lock().insert(epfd_chirho, instance_chirho);

    crate::serial_println_chirho!("[EPOLL] Created epoll fd={}", epfd_chirho);

    epfd_chirho as i64
}

/// `epoll_ctl(2)` — control an epoll instance.
///
/// # Arguments
/// * `epfd_chirho` — epoll file descriptor
/// * `op_chirho` — EPOLL_CTL_ADD, EPOLL_CTL_MOD, or EPOLL_CTL_DEL
/// * `fd_chirho` — target file descriptor
/// * `event_ptr_chirho` — pointer to `struct epoll_event` (userspace)
///
/// # Returns
/// 0 on success, negative errno on failure.
#[allow(dead_code)]
pub fn sys_epoll_ctl_chirho(
    epfd_chirho: u64,
    op_chirho: u64,
    fd_chirho: u64,
    event_ptr_chirho: u64,
) -> i64 {
    let epfd_i32_chirho = epfd_chirho as i32;
    let fd_i32_chirho = fd_chirho as i32;
    let op_i32_chirho = op_chirho as i32;

    let mut instances_chirho = EPOLL_INSTANCES_CHIRHO.lock();
    let instance_chirho = match instances_chirho.get_mut(&epfd_i32_chirho) {
        Some(inst_chirho) => inst_chirho,
        None => return -(crate::syscall_chirho::EBADF_CHIRHO),
    };

    match op_i32_chirho {
        EPOLL_CTL_ADD_CHIRHO => {
            if instance_chirho.items_chirho.contains_key(&fd_i32_chirho) {
                return -(crate::syscall_chirho::EEXIST_CHIRHO);
            }
            let event_chirho = if event_ptr_chirho != 0 {
                unsafe { core::ptr::read_unaligned(event_ptr_chirho as *const EpollEventChirho) }
            } else {
                EpollEventChirho::default()
            };
            instance_chirho.items_chirho.insert(
                fd_i32_chirho,
                EpollItemChirho {
                    fd_chirho: fd_i32_chirho,
                    event_chirho,
                },
            );
            0
        }
        EPOLL_CTL_MOD_CHIRHO => {
            let item_chirho = match instance_chirho.items_chirho.get_mut(&fd_i32_chirho) {
                Some(it_chirho) => it_chirho,
                None => return -(crate::syscall_chirho::ENOENT_CHIRHO),
            };
            if event_ptr_chirho != 0 {
                item_chirho.event_chirho = unsafe {
                    core::ptr::read_unaligned(event_ptr_chirho as *const EpollEventChirho)
                };
            }
            0
        }
        EPOLL_CTL_DEL_CHIRHO => {
            if instance_chirho.items_chirho.remove(&fd_i32_chirho).is_none() {
                return -(crate::syscall_chirho::ENOENT_CHIRHO);
            }
            0
        }
        _ => -(crate::syscall_chirho::EINVAL_CHIRHO),
    }
}

/// `epoll_wait(2)` / `epoll_pwait(2)` — wait for events.
///
/// # Arguments
/// * `epfd_chirho` — epoll file descriptor
/// * `events_ptr_chirho` — userspace buffer for returned events
/// * `maxevents_chirho` — max events to return
/// * `timeout_ms_chirho` — timeout in milliseconds (-1 = block forever)
///
/// # Returns
/// Number of ready events, or negative errno.
#[allow(dead_code)]
pub fn sys_epoll_wait_chirho(
    epfd_chirho: u64,
    events_ptr_chirho: u64,
    maxevents_chirho: u64,
    _timeout_ms_chirho: u64,
) -> i64 {
    let epfd_i32_chirho = epfd_chirho as i32;
    let max_chirho = maxevents_chirho as usize;

    if max_chirho == 0 || events_ptr_chirho == 0 {
        return -(crate::syscall_chirho::EINVAL_CHIRHO);
    }

    let instances_chirho = EPOLL_INSTANCES_CHIRHO.lock();
    let instance_chirho = match instances_chirho.get(&epfd_i32_chirho) {
        Some(inst_chirho) => inst_chirho,
        None => return -(crate::syscall_chirho::EBADF_CHIRHO),
    };

    // For now, report all watched fds as ready for their requested events.
    // A real implementation would check each fd's actual readiness state.
    let mut count_chirho = 0usize;
    let out_ptr_chirho = events_ptr_chirho as *mut EpollEventChirho;

    for (_fd_chirho, item_chirho) in instance_chirho.items_chirho.iter() {
        if count_chirho >= max_chirho {
            break;
        }

        // Simulate: all fds are ready for the events they're watching
        let item_events_chirho = { item_chirho.event_chirho.events_chirho };
        let item_data_chirho = { item_chirho.event_chirho.data_chirho };
        let ready_events_chirho = item_events_chirho & (EPOLLIN_CHIRHO | EPOLLOUT_CHIRHO);

        if ready_events_chirho != 0 {
            let out_event_chirho = EpollEventChirho {
                events_chirho: ready_events_chirho,
                data_chirho: item_data_chirho,
            };
            unsafe {
                core::ptr::write_unaligned(out_ptr_chirho.add(count_chirho), out_event_chirho);
            }
            count_chirho += 1;
        }
    }

    count_chirho as i64
}

/// Close an epoll instance (called when the epoll fd is closed).
#[allow(dead_code)]
pub fn close_epoll_chirho(epfd_chirho: i32) {
    EPOLL_INSTANCES_CHIRHO.lock().remove(&epfd_chirho);
}
