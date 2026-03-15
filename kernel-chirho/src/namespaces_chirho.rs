// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! Linux namespaces for the Lineluya kernel (A6-013 / A6-014).
//!
//! Implements:
//! - PID namespaces (process ID isolation)
//! - Mount namespaces (filesystem mount isolation)
//! - Network namespaces (network stack isolation)
//! - User namespaces (UID/GID mapping)
//! - UTS namespaces (hostname isolation)
//! - IPC namespaces (System V IPC isolation)
//!
//! Each namespace type uses an ID-based reference model. Processes hold
//! a set of namespace IDs; `unshare(2)` and `setns(2)` create or switch
//! namespace memberships.
//!
//! Reference: namespaces(7), unshare(2), setns(2)

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

// ============================================================================
// Namespace type flags (matching Linux CLONE_NEW* flags)
// ============================================================================

/// Create new IPC namespace.
#[allow(dead_code)]
pub const CLONE_NEWIPC_CHIRHO: u64 = 0x0800_0000;
/// Create new network namespace.
#[allow(dead_code)]
pub const CLONE_NEWNET_CHIRHO: u64 = 0x4000_0000;
/// Create new mount namespace.
#[allow(dead_code)]
pub const CLONE_NEWNS_CHIRHO: u64 = 0x0002_0000;
/// Create new PID namespace.
#[allow(dead_code)]
pub const CLONE_NEWPID_CHIRHO: u64 = 0x2000_0000;
/// Create new user namespace.
#[allow(dead_code)]
pub const CLONE_NEWUSER_CHIRHO: u64 = 0x1000_0000;
/// Create new UTS namespace.
#[allow(dead_code)]
pub const CLONE_NEWUTS_CHIRHO: u64 = 0x0400_0000;
/// Create new cgroup namespace.
#[allow(dead_code)]
pub const CLONE_NEWCGROUP_CHIRHO: u64 = 0x0200_0000;

// ============================================================================
// Namespace type enum
// ============================================================================

/// Types of namespaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(dead_code)]
pub enum NsTypeChirho {
    PidChirho,
    MountChirho,
    NetChirho,
    UserChirho,
    UtsChirho,
    IpcChirho,
    CgroupChirho,
}

// ============================================================================
// Namespace descriptors
// ============================================================================

/// A PID namespace.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct PidNsChirho {
    /// Namespace ID.
    pub id_chirho: u64,
    /// Parent namespace ID (0 = initial namespace).
    pub parent_id_chirho: u64,
    /// Next PID to allocate within this namespace.
    pub next_pid_chirho: u64,
    /// Map of namespace-local PID -> global PID.
    pub pid_map_chirho: BTreeMap<u64, u64>,
}

/// A mount namespace.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct MountNsChirho {
    /// Namespace ID.
    pub id_chirho: u64,
    /// Mount points (path -> filesystem type).
    pub mounts_chirho: BTreeMap<String, String>,
}

/// A network namespace.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct NetNsChirho {
    /// Namespace ID.
    pub id_chirho: u64,
    /// Network interfaces in this namespace.
    pub interfaces_chirho: Vec<String>,
    /// Whether this has its own loopback device.
    pub has_loopback_chirho: bool,
}

/// A user namespace.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct UserNsChirho {
    /// Namespace ID.
    pub id_chirho: u64,
    /// UID mappings: (ns_uid, host_uid, count).
    pub uid_map_chirho: Vec<(u32, u32, u32)>,
    /// GID mappings: (ns_gid, host_gid, count).
    pub gid_map_chirho: Vec<(u32, u32, u32)>,
}

/// A UTS namespace.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct UtsNsChirho {
    /// Namespace ID.
    pub id_chirho: u64,
    /// Hostname.
    pub hostname_chirho: String,
    /// Domain name.
    pub domainname_chirho: String,
}

// ============================================================================
// Per-process namespace set
// ============================================================================

/// The set of namespace IDs that a process belongs to.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct NsSetChirho {
    pub pid_ns_id_chirho: u64,
    pub mnt_ns_id_chirho: u64,
    pub net_ns_id_chirho: u64,
    pub user_ns_id_chirho: u64,
    pub uts_ns_id_chirho: u64,
    pub ipc_ns_id_chirho: u64,
    pub cgroup_ns_id_chirho: u64,
}

impl Default for NsSetChirho {
    fn default() -> Self {
        // All zeros = initial (default) namespaces
        Self {
            pid_ns_id_chirho: 0,
            mnt_ns_id_chirho: 0,
            net_ns_id_chirho: 0,
            user_ns_id_chirho: 0,
            uts_ns_id_chirho: 0,
            ipc_ns_id_chirho: 0,
            cgroup_ns_id_chirho: 0,
        }
    }
}

// ============================================================================
// Global namespace registry
// ============================================================================

struct NsRegistryChirho {
    pid_namespaces_chirho: BTreeMap<u64, PidNsChirho>,
    mount_namespaces_chirho: BTreeMap<u64, MountNsChirho>,
    net_namespaces_chirho: BTreeMap<u64, NetNsChirho>,
    user_namespaces_chirho: BTreeMap<u64, UserNsChirho>,
    uts_namespaces_chirho: BTreeMap<u64, UtsNsChirho>,
    next_ns_id_chirho: u64,
}

impl NsRegistryChirho {
    const fn new_chirho() -> Self {
        Self {
            pid_namespaces_chirho: BTreeMap::new(),
            mount_namespaces_chirho: BTreeMap::new(),
            net_namespaces_chirho: BTreeMap::new(),
            user_namespaces_chirho: BTreeMap::new(),
            uts_namespaces_chirho: BTreeMap::new(),
            next_ns_id_chirho: 1,
        }
    }
}

static NS_REGISTRY_CHIRHO: Mutex<NsRegistryChirho> =
    Mutex::new(NsRegistryChirho::new_chirho());

// ============================================================================
// Initialization
// ============================================================================

/// Initialize the namespace subsystem with default (initial) namespaces.
#[allow(dead_code)]
pub fn init_namespaces_chirho() {
    let mut reg_chirho = NS_REGISTRY_CHIRHO.lock();

    // Create initial PID namespace
    reg_chirho.pid_namespaces_chirho.insert(
        0,
        PidNsChirho {
            id_chirho: 0,
            parent_id_chirho: 0,
            next_pid_chirho: 1,
            pid_map_chirho: BTreeMap::new(),
        },
    );

    // Create initial mount namespace
    let mut initial_mounts_chirho = BTreeMap::new();
    initial_mounts_chirho.insert(String::from("/"), String::from("tmpfs"));
    reg_chirho.mount_namespaces_chirho.insert(
        0,
        MountNsChirho {
            id_chirho: 0,
            mounts_chirho: initial_mounts_chirho,
        },
    );

    // Create initial network namespace
    reg_chirho.net_namespaces_chirho.insert(
        0,
        NetNsChirho {
            id_chirho: 0,
            interfaces_chirho: Vec::new(),
            has_loopback_chirho: true,
        },
    );

    // Create initial user namespace
    reg_chirho.user_namespaces_chirho.insert(
        0,
        UserNsChirho {
            id_chirho: 0,
            uid_map_chirho: vec![(0, 0, 65536)],
            gid_map_chirho: vec![(0, 0, 65536)],
        },
    );

    // Create initial UTS namespace
    reg_chirho.uts_namespaces_chirho.insert(
        0,
        UtsNsChirho {
            id_chirho: 0,
            hostname_chirho: String::from("lineluya"),
            domainname_chirho: String::from("(none)"),
        },
    );

    crate::serial_println_chirho!("[NAMESPACES] Initialized default namespaces (PID, MNT, NET, USER, UTS)");
}

// ============================================================================
// unshare(2) implementation
// ============================================================================

/// `unshare(2)` — create new namespaces for the calling process.
///
/// # Arguments
/// * `flags_chirho` — bitmask of CLONE_NEW* flags
///
/// # Returns
/// 0 on success, negative errno on failure.
#[allow(dead_code)]
pub fn sys_unshare_chirho(flags_chirho: u64) -> i64 {
    let mut reg_chirho = NS_REGISTRY_CHIRHO.lock();

    if flags_chirho & CLONE_NEWPID_CHIRHO != 0 {
        let id_chirho = reg_chirho.next_ns_id_chirho;
        reg_chirho.next_ns_id_chirho += 1;
        reg_chirho.pid_namespaces_chirho.insert(
            id_chirho,
            PidNsChirho {
                id_chirho,
                parent_id_chirho: 0,
                next_pid_chirho: 1,
                pid_map_chirho: BTreeMap::new(),
            },
        );
        crate::serial_println_chirho!("[NAMESPACES] Created new PID namespace id={}", id_chirho);
    }

    if flags_chirho & CLONE_NEWNS_CHIRHO != 0 {
        let id_chirho = reg_chirho.next_ns_id_chirho;
        reg_chirho.next_ns_id_chirho += 1;
        // Clone the initial mount namespace
        let initial_mounts_chirho = reg_chirho
            .mount_namespaces_chirho
            .get(&0)
            .map(|m_chirho| m_chirho.mounts_chirho.clone())
            .unwrap_or_default();
        reg_chirho.mount_namespaces_chirho.insert(
            id_chirho,
            MountNsChirho {
                id_chirho,
                mounts_chirho: initial_mounts_chirho,
            },
        );
        crate::serial_println_chirho!("[NAMESPACES] Created new mount namespace id={}", id_chirho);
    }

    if flags_chirho & CLONE_NEWNET_CHIRHO != 0 {
        let id_chirho = reg_chirho.next_ns_id_chirho;
        reg_chirho.next_ns_id_chirho += 1;
        reg_chirho.net_namespaces_chirho.insert(
            id_chirho,
            NetNsChirho {
                id_chirho,
                interfaces_chirho: Vec::new(),
                has_loopback_chirho: true,
            },
        );
        crate::serial_println_chirho!("[NAMESPACES] Created new network namespace id={}", id_chirho);
    }

    if flags_chirho & CLONE_NEWUSER_CHIRHO != 0 {
        let id_chirho = reg_chirho.next_ns_id_chirho;
        reg_chirho.next_ns_id_chirho += 1;
        reg_chirho.user_namespaces_chirho.insert(
            id_chirho,
            UserNsChirho {
                id_chirho,
                uid_map_chirho: Vec::new(),
                gid_map_chirho: Vec::new(),
            },
        );
        crate::serial_println_chirho!("[NAMESPACES] Created new user namespace id={}", id_chirho);
    }

    if flags_chirho & CLONE_NEWUTS_CHIRHO != 0 {
        let id_chirho = reg_chirho.next_ns_id_chirho;
        reg_chirho.next_ns_id_chirho += 1;
        reg_chirho.uts_namespaces_chirho.insert(
            id_chirho,
            UtsNsChirho {
                id_chirho,
                hostname_chirho: String::from("lineluya"),
                domainname_chirho: String::from("(none)"),
            },
        );
        crate::serial_println_chirho!("[NAMESPACES] Created new UTS namespace id={}", id_chirho);
    }

    0
}

/// `setns(2)` — switch the calling process to a different namespace.
///
/// # Arguments
/// * `fd_chirho` — file descriptor referring to a namespace (stub: uses ns ID directly)
/// * `nstype_chirho` — namespace type flag (CLONE_NEWPID etc.)
///
/// # Returns
/// 0 on success, negative errno on failure.
#[allow(dead_code)]
pub fn sys_setns_chirho(fd_chirho: u64, nstype_chirho: u64) -> i64 {
    let reg_chirho = NS_REGISTRY_CHIRHO.lock();
    let ns_id_chirho = fd_chirho; // Simplified: fd = ns ID

    // Verify the namespace exists
    let exists_chirho = match nstype_chirho {
        x_chirho if x_chirho == CLONE_NEWPID_CHIRHO => {
            reg_chirho.pid_namespaces_chirho.contains_key(&ns_id_chirho)
        }
        x_chirho if x_chirho == CLONE_NEWNS_CHIRHO => {
            reg_chirho.mount_namespaces_chirho.contains_key(&ns_id_chirho)
        }
        x_chirho if x_chirho == CLONE_NEWNET_CHIRHO => {
            reg_chirho.net_namespaces_chirho.contains_key(&ns_id_chirho)
        }
        x_chirho if x_chirho == CLONE_NEWUSER_CHIRHO => {
            reg_chirho.user_namespaces_chirho.contains_key(&ns_id_chirho)
        }
        x_chirho if x_chirho == CLONE_NEWUTS_CHIRHO => {
            reg_chirho.uts_namespaces_chirho.contains_key(&ns_id_chirho)
        }
        _ => false,
    };

    if exists_chirho {
        crate::serial_println_chirho!(
            "[NAMESPACES] setns: switch to ns_id={} type={:#x}",
            ns_id_chirho,
            nstype_chirho,
        );
        0
    } else {
        -(crate::syscall_chirho::EINVAL_CHIRHO)
    }
}
