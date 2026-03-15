// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! inotify and fanotify filesystem event notification for the Lineluya
//! kernel (A6-019).
//!
//! Implements:
//! - `inotify_init1(2)` — create an inotify instance
//! - `inotify_add_watch(2)` — add a watch for filesystem events
//! - `inotify_rm_watch(2)` — remove a watch
//! - `fanotify_init(2)` / `fanotify_mark(2)` — stubs
//!
//! inotify provides per-file event monitoring; fanotify provides
//! global filesystem monitoring (used by antivirus, backup tools).
//!
//! Reference: inotify(7), fanotify(7)

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

// ============================================================================
// inotify event mask constants
// ============================================================================

/// File was accessed (read).
#[allow(dead_code)]
pub const IN_ACCESS_CHIRHO: u32 = 0x0000_0001;
/// File was modified.
#[allow(dead_code)]
pub const IN_MODIFY_CHIRHO: u32 = 0x0000_0002;
/// Metadata changed (chmod, chown, etc.).
#[allow(dead_code)]
pub const IN_ATTRIB_CHIRHO: u32 = 0x0000_0004;
/// File opened for writing was closed.
#[allow(dead_code)]
pub const IN_CLOSE_WRITE_CHIRHO: u32 = 0x0000_0008;
/// File opened read-only was closed.
#[allow(dead_code)]
pub const IN_CLOSE_NOWRITE_CHIRHO: u32 = 0x0000_0010;
/// File was opened.
#[allow(dead_code)]
pub const IN_OPEN_CHIRHO: u32 = 0x0000_0020;
/// File moved from watched directory.
#[allow(dead_code)]
pub const IN_MOVED_FROM_CHIRHO: u32 = 0x0000_0040;
/// File moved to watched directory.
#[allow(dead_code)]
pub const IN_MOVED_TO_CHIRHO: u32 = 0x0000_0080;
/// File created in watched directory.
#[allow(dead_code)]
pub const IN_CREATE_CHIRHO: u32 = 0x0000_0100;
/// File deleted from watched directory.
#[allow(dead_code)]
pub const IN_DELETE_CHIRHO: u32 = 0x0000_0200;
/// Watched file was deleted.
#[allow(dead_code)]
pub const IN_DELETE_SELF_CHIRHO: u32 = 0x0000_0400;
/// Watched file was moved.
#[allow(dead_code)]
pub const IN_MOVE_SELF_CHIRHO: u32 = 0x0000_0800;

/// Watch for all events.
#[allow(dead_code)]
pub const IN_ALL_EVENTS_CHIRHO: u32 = IN_ACCESS_CHIRHO
    | IN_MODIFY_CHIRHO
    | IN_ATTRIB_CHIRHO
    | IN_CLOSE_WRITE_CHIRHO
    | IN_CLOSE_NOWRITE_CHIRHO
    | IN_OPEN_CHIRHO
    | IN_MOVED_FROM_CHIRHO
    | IN_MOVED_TO_CHIRHO
    | IN_CREATE_CHIRHO
    | IN_DELETE_CHIRHO
    | IN_DELETE_SELF_CHIRHO
    | IN_MOVE_SELF_CHIRHO;

/// inotify_init1 flags.
#[allow(dead_code)]
pub const IN_CLOEXEC_CHIRHO: u32 = 0o2000000;
#[allow(dead_code)]
pub const IN_NONBLOCK_CHIRHO: u32 = 0o4000;

// ============================================================================
// inotify event structure
// ============================================================================

/// inotify event (variable length due to name field).
#[repr(C)]
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct InotifyEventChirho {
    /// Watch descriptor.
    pub wd_chirho: i32,
    /// Event mask.
    pub mask_chirho: u32,
    /// Cookie for related events (rename).
    pub cookie_chirho: u32,
    /// Length of the name field.
    pub len_chirho: u32,
    /// Optional filename (NUL-terminated, padded to alignment).
    pub name_chirho: String,
}

// ============================================================================
// inotify watch entry
// ============================================================================

/// A single inotify watch.
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct InotifyWatchChirho {
    /// Watch descriptor (returned to userspace).
    wd_chirho: i32,
    /// Watched path.
    path_chirho: String,
    /// Event mask.
    mask_chirho: u32,
}

/// An inotify instance.
struct InotifyInstanceChirho {
    /// Watches keyed by watch descriptor.
    watches_chirho: BTreeMap<i32, InotifyWatchChirho>,
    /// Next watch descriptor.
    next_wd_chirho: i32,
    /// Pending events queue.
    events_chirho: Vec<InotifyEventChirho>,
    /// Flags.
    flags_chirho: u32,
}

impl InotifyInstanceChirho {
    fn new_chirho(flags_chirho: u32) -> Self {
        Self {
            watches_chirho: BTreeMap::new(),
            next_wd_chirho: 1,
            events_chirho: Vec::new(),
            flags_chirho,
        }
    }
}

// ============================================================================
// Global inotify registry
// ============================================================================

static INOTIFY_INSTANCES_CHIRHO: Mutex<BTreeMap<i32, InotifyInstanceChirho>> =
    Mutex::new(BTreeMap::new());

static NEXT_INOTIFY_FD_CHIRHO: Mutex<i32> = Mutex::new(5000);

// ============================================================================
// Syscall implementations
// ============================================================================

/// `inotify_init1(2)` — create an inotify instance.
#[allow(dead_code)]
pub fn sys_inotify_init1_chirho(flags_chirho: u64) -> i64 {
    let mut next_fd_chirho = NEXT_INOTIFY_FD_CHIRHO.lock();
    let fd_chirho = *next_fd_chirho;
    *next_fd_chirho += 1;

    let instance_chirho = InotifyInstanceChirho::new_chirho(flags_chirho as u32);
    INOTIFY_INSTANCES_CHIRHO.lock().insert(fd_chirho, instance_chirho);

    crate::serial_println_chirho!(
        "[INOTIFY] Created instance fd={} flags={:#x}",
        fd_chirho,
        flags_chirho,
    );

    fd_chirho as i64
}

/// `inotify_add_watch(2)` — add a watch to an inotify instance.
#[allow(dead_code)]
pub fn sys_inotify_add_watch_chirho(
    fd_chirho: u64,
    pathname_ptr_chirho: u64,
    mask_chirho: u64,
) -> i64 {
    let fd_i32_chirho = fd_chirho as i32;

    // Read the pathname from userspace
    let path_chirho = if pathname_ptr_chirho != 0 {
        crate::uaccess_chirho::read_user_string_chirho(pathname_ptr_chirho)
            .unwrap_or_else(|_e_chirho| String::from("<invalid>"))
    } else {
        return -(crate::syscall_chirho::EFAULT_CHIRHO);
    };

    let mut instances_chirho = INOTIFY_INSTANCES_CHIRHO.lock();
    let instance_chirho = match instances_chirho.get_mut(&fd_i32_chirho) {
        Some(inst_chirho) => inst_chirho,
        None => return -(crate::syscall_chirho::EBADF_CHIRHO),
    };

    // Check if this path is already watched
    for (wd_chirho, watch_chirho) in instance_chirho.watches_chirho.iter_mut() {
        if watch_chirho.path_chirho == path_chirho {
            // Update mask
            watch_chirho.mask_chirho = mask_chirho as u32;
            return *wd_chirho as i64;
        }
    }

    let wd_chirho = instance_chirho.next_wd_chirho;
    instance_chirho.next_wd_chirho += 1;

    instance_chirho.watches_chirho.insert(
        wd_chirho,
        InotifyWatchChirho {
            wd_chirho,
            path_chirho: path_chirho.clone(),
            mask_chirho: mask_chirho as u32,
        },
    );

    crate::serial_println_chirho!(
        "[INOTIFY] Added watch wd={} path={} mask={:#x}",
        wd_chirho,
        path_chirho,
        mask_chirho,
    );

    wd_chirho as i64
}

/// `inotify_rm_watch(2)` — remove a watch from an inotify instance.
#[allow(dead_code)]
pub fn sys_inotify_rm_watch_chirho(fd_chirho: u64, wd_chirho: u64) -> i64 {
    let fd_i32_chirho = fd_chirho as i32;
    let wd_i32_chirho = wd_chirho as i32;

    let mut instances_chirho = INOTIFY_INSTANCES_CHIRHO.lock();
    let instance_chirho = match instances_chirho.get_mut(&fd_i32_chirho) {
        Some(inst_chirho) => inst_chirho,
        None => return -(crate::syscall_chirho::EBADF_CHIRHO),
    };

    match instance_chirho.watches_chirho.remove(&wd_i32_chirho) {
        Some(_removed_chirho) => 0,
        None => -(crate::syscall_chirho::EINVAL_CHIRHO),
    }
}

// ============================================================================
// fanotify stubs
// ============================================================================

/// `fanotify_init(2)` — create a fanotify group (stub).
#[allow(dead_code)]
pub fn sys_fanotify_init_chirho(_flags_chirho: u64, _event_f_flags_chirho: u64) -> i64 {
    crate::serial_println_chirho!("[FANOTIFY] fanotify_init — stub, returning -ENOSYS");
    -(crate::syscall_chirho::ENOSYS_CHIRHO)
}

/// `fanotify_mark(2)` — add/modify/remove a mark (stub).
#[allow(dead_code)]
pub fn sys_fanotify_mark_chirho(
    _fanotify_fd_chirho: u64,
    _flags_chirho: u64,
    _mask_chirho: u64,
    _dirfd_chirho: u64,
    _pathname_chirho: u64,
) -> i64 {
    crate::serial_println_chirho!("[FANOTIFY] fanotify_mark — stub, returning -ENOSYS");
    -(crate::syscall_chirho::ENOSYS_CHIRHO)
}
