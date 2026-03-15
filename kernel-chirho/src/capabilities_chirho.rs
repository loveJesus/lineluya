// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! Linux capabilities subsystem for the Lineluya kernel (A6).
//!
//! Implements the POSIX capabilities model that replaces the all-or-nothing
//! superuser privilege model:
//! - Capability sets: effective, permitted, inheritable, bounding, ambient
//! - `capget(2)` / `capset(2)` syscalls
//! - Capability checking for privileged operations
//!
//! Reference: capabilities(7), Linux include/uapi/linux/capability.h

extern crate alloc;

use alloc::collections::BTreeMap;
use spin::Mutex;

// ============================================================================
// Capability constants (matching Linux — capability numbers 0..40+)
// ============================================================================

/// Override DAC (file permission) checks.
#[allow(dead_code)]
pub const CAP_DAC_OVERRIDE_CHIRHO: u32 = 1;
/// Override DAC read checks.
#[allow(dead_code)]
pub const CAP_DAC_READ_SEARCH_CHIRHO: u32 = 2;
/// Override file ownership checks.
#[allow(dead_code)]
pub const CAP_FOWNER_CHIRHO: u32 = 3;
/// Bypass file setuid/setgid restrictions.
#[allow(dead_code)]
pub const CAP_FSETID_CHIRHO: u32 = 4;
/// Send signals to arbitrary processes.
#[allow(dead_code)]
pub const CAP_KILL_CHIRHO: u32 = 5;
/// Set GID / supplementary GIDs.
#[allow(dead_code)]
pub const CAP_SETGID_CHIRHO: u32 = 6;
/// Set UID.
#[allow(dead_code)]
pub const CAP_SETUID_CHIRHO: u32 = 7;
/// Set file capabilities.
#[allow(dead_code)]
pub const CAP_SETFCAP_CHIRHO: u32 = 8;
/// Lock memory (mlock, mlockall, etc.).
#[allow(dead_code)]
pub const CAP_IPC_LOCK_CHIRHO: u32 = 14;
/// Bind to privileged ports (< 1024).
#[allow(dead_code)]
pub const CAP_NET_BIND_SERVICE_CHIRHO: u32 = 10;
/// Use raw sockets.
#[allow(dead_code)]
pub const CAP_NET_RAW_CHIRHO: u32 = 13;
/// Configure network interfaces.
#[allow(dead_code)]
pub const CAP_NET_ADMIN_CHIRHO: u32 = 12;
/// Perform various system administration ops.
#[allow(dead_code)]
pub const CAP_SYS_ADMIN_CHIRHO: u32 = 21;
/// Reboot / kexec.
#[allow(dead_code)]
pub const CAP_SYS_BOOT_CHIRHO: u32 = 22;
/// Set system clock.
#[allow(dead_code)]
pub const CAP_SYS_TIME_CHIRHO: u32 = 25;
/// Use chroot.
#[allow(dead_code)]
pub const CAP_SYS_CHROOT_CHIRHO: u32 = 18;
/// Load/unload kernel modules.
#[allow(dead_code)]
pub const CAP_SYS_MODULE_CHIRHO: u32 = 16;
/// ptrace any process.
#[allow(dead_code)]
pub const CAP_SYS_PTRACE_CHIRHO: u32 = 19;
/// Set resource limits.
#[allow(dead_code)]
pub const CAP_SYS_RESOURCE_CHIRHO: u32 = 24;
/// Use syslog.
#[allow(dead_code)]
pub const CAP_SYSLOG_CHIRHO: u32 = 34;
/// BPF operations.
#[allow(dead_code)]
pub const CAP_BPF_CHIRHO: u32 = 39;
/// Checkpoint/restore.
#[allow(dead_code)]
pub const CAP_CHECKPOINT_RESTORE_CHIRHO: u32 = 40;

/// Maximum capability number.
#[allow(dead_code)]
pub const CAP_LAST_CAP_CHIRHO: u32 = 40;

// ============================================================================
// Capability set (64-bit bitmask)
// ============================================================================

/// A capability bitmask. Bit N corresponds to capability N.
pub type CapSetChirho = u64;

/// Build a capability bit from a capability number.
#[allow(dead_code)]
pub const fn cap_bit_chirho(cap_chirho: u32) -> CapSetChirho {
    1u64 << cap_chirho
}

/// All capabilities.
#[allow(dead_code)]
pub const CAP_ALL_CHIRHO: CapSetChirho = (1u64 << (CAP_LAST_CAP_CHIRHO + 1)) - 1;

/// No capabilities.
#[allow(dead_code)]
pub const CAP_EMPTY_CHIRHO: CapSetChirho = 0;

// ============================================================================
// Process capability state
// ============================================================================

/// Capability sets for a single process/thread.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ProcessCapsChirho {
    /// Effective: capabilities actually used for permission checks.
    pub effective_chirho: CapSetChirho,
    /// Permitted: max set of caps that can become effective.
    pub permitted_chirho: CapSetChirho,
    /// Inheritable: caps preserved across execve.
    pub inheritable_chirho: CapSetChirho,
    /// Bounding: upper limit on caps gained during execve.
    pub bounding_chirho: CapSetChirho,
    /// Ambient: caps automatically added to effective/permitted on execve
    /// of a non-setuid program.
    pub ambient_chirho: CapSetChirho,
}

impl Default for ProcessCapsChirho {
    fn default() -> Self {
        Self {
            // Root (PID 1) gets all capabilities
            effective_chirho: CAP_ALL_CHIRHO,
            permitted_chirho: CAP_ALL_CHIRHO,
            inheritable_chirho: CAP_EMPTY_CHIRHO,
            bounding_chirho: CAP_ALL_CHIRHO,
            ambient_chirho: CAP_EMPTY_CHIRHO,
        }
    }
}

impl ProcessCapsChirho {
    /// Create an unprivileged capability set.
    #[allow(dead_code)]
    pub fn unprivileged_chirho() -> Self {
        Self {
            effective_chirho: CAP_EMPTY_CHIRHO,
            permitted_chirho: CAP_EMPTY_CHIRHO,
            inheritable_chirho: CAP_EMPTY_CHIRHO,
            bounding_chirho: CAP_ALL_CHIRHO,
            ambient_chirho: CAP_EMPTY_CHIRHO,
        }
    }

    /// Check if a specific capability is in the effective set.
    #[allow(dead_code)]
    pub fn has_cap_chirho(&self, cap_chirho: u32) -> bool {
        if cap_chirho > CAP_LAST_CAP_CHIRHO {
            return false;
        }
        self.effective_chirho & cap_bit_chirho(cap_chirho) != 0
    }
}

// ============================================================================
// Global capability registry (pid -> caps)
// ============================================================================

static PROCESS_CAPS_CHIRHO: Mutex<BTreeMap<u64, ProcessCapsChirho>> =
    Mutex::new(BTreeMap::new());

/// Initialize capabilities for the init process (PID 1).
#[allow(dead_code)]
pub fn init_capabilities_chirho() {
    let mut caps_chirho = PROCESS_CAPS_CHIRHO.lock();
    caps_chirho.insert(1, ProcessCapsChirho::default());
    crate::serial_println_chirho!("[CAPS] Capabilities subsystem initialized (PID 1 = all caps)");
}

/// Register capabilities for a new process.
#[allow(dead_code)]
pub fn register_process_caps_chirho(pid_chirho: u64, caps_chirho: ProcessCapsChirho) {
    PROCESS_CAPS_CHIRHO.lock().insert(pid_chirho, caps_chirho);
}

/// Check if a process has a specific capability.
#[allow(dead_code)]
pub fn capable_chirho(pid_chirho: u64, cap_chirho: u32) -> bool {
    let caps_chirho = PROCESS_CAPS_CHIRHO.lock();
    match caps_chirho.get(&pid_chirho) {
        Some(pc_chirho) => pc_chirho.has_cap_chirho(cap_chirho),
        None => false,
    }
}

// ============================================================================
// capget(2) / capset(2) syscalls
// ============================================================================

/// Linux `struct __user_cap_header_struct`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub struct CapHeaderChirho {
    pub version_chirho: u32,
    pub pid_chirho: i32,
}

/// Linux `struct __user_cap_data_struct` (v3 uses two of these).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
#[allow(dead_code)]
pub struct CapDataChirho {
    pub effective_chirho: u32,
    pub permitted_chirho: u32,
    pub inheritable_chirho: u32,
}

/// Preferred capability version.
#[allow(dead_code)]
pub const LINUX_CAPABILITY_VERSION_3_CHIRHO: u32 = 0x2008_0522;

/// `capget(2)` — get process capabilities.
#[allow(dead_code)]
pub fn sys_capget_chirho(header_ptr_chirho: u64, data_ptr_chirho: u64) -> i64 {
    if header_ptr_chirho == 0 {
        return -(crate::syscall_chirho::EFAULT_CHIRHO);
    }

    let header_chirho = unsafe { core::ptr::read_unaligned(header_ptr_chirho as *const CapHeaderChirho) };
    let pid_chirho = if header_chirho.pid_chirho == 0 {
        // 0 = calling process
        crate::scheduler_chirho::current_pid_chirho().unwrap_or(1)
    } else {
        header_chirho.pid_chirho as u64
    };

    if data_ptr_chirho == 0 {
        return 0; // Just checking version
    }

    let caps_chirho = PROCESS_CAPS_CHIRHO.lock();
    let pc_chirho = caps_chirho.get(&pid_chirho).cloned().unwrap_or_default();

    // Write two CapDataChirho structs (v3 format: low 32 bits, high 32 bits)
    let data0_chirho = CapDataChirho {
        effective_chirho: pc_chirho.effective_chirho as u32,
        permitted_chirho: pc_chirho.permitted_chirho as u32,
        inheritable_chirho: pc_chirho.inheritable_chirho as u32,
    };
    let data1_chirho = CapDataChirho {
        effective_chirho: (pc_chirho.effective_chirho >> 32) as u32,
        permitted_chirho: (pc_chirho.permitted_chirho >> 32) as u32,
        inheritable_chirho: (pc_chirho.inheritable_chirho >> 32) as u32,
    };

    unsafe {
        core::ptr::write_unaligned(data_ptr_chirho as *mut CapDataChirho, data0_chirho);
        core::ptr::write_unaligned(
            (data_ptr_chirho as *mut CapDataChirho).add(1),
            data1_chirho,
        );
    }

    0
}

/// `capset(2)` — set process capabilities.
#[allow(dead_code)]
pub fn sys_capset_chirho(header_ptr_chirho: u64, data_ptr_chirho: u64) -> i64 {
    if header_ptr_chirho == 0 || data_ptr_chirho == 0 {
        return -(crate::syscall_chirho::EFAULT_CHIRHO);
    }

    let header_chirho = unsafe { core::ptr::read_unaligned(header_ptr_chirho as *const CapHeaderChirho) };
    let pid_chirho = if header_chirho.pid_chirho == 0 {
        crate::scheduler_chirho::current_pid_chirho().unwrap_or(1)
    } else {
        header_chirho.pid_chirho as u64
    };

    let data0_chirho =
        unsafe { core::ptr::read_unaligned(data_ptr_chirho as *const CapDataChirho) };
    let data1_chirho = unsafe {
        core::ptr::read_unaligned((data_ptr_chirho as *const CapDataChirho).add(1))
    };

    let effective_chirho =
        (data0_chirho.effective_chirho as u64) | ((data1_chirho.effective_chirho as u64) << 32);
    let permitted_chirho =
        (data0_chirho.permitted_chirho as u64) | ((data1_chirho.permitted_chirho as u64) << 32);
    let inheritable_chirho = (data0_chirho.inheritable_chirho as u64)
        | ((data1_chirho.inheritable_chirho as u64) << 32);

    let mut caps_chirho = PROCESS_CAPS_CHIRHO.lock();
    let pc_chirho = caps_chirho.entry(pid_chirho).or_insert_with(ProcessCapsChirho::default);

    // Validate: new permitted must be subset of old permitted
    if permitted_chirho & !pc_chirho.permitted_chirho != 0 {
        return -(crate::syscall_chirho::EPERM_CHIRHO);
    }
    // Validate: new effective must be subset of new permitted
    if effective_chirho & !permitted_chirho != 0 {
        return -(crate::syscall_chirho::EPERM_CHIRHO);
    }

    pc_chirho.effective_chirho = effective_chirho;
    pc_chirho.permitted_chirho = permitted_chirho;
    pc_chirho.inheritable_chirho = inheritable_chirho;

    0
}
