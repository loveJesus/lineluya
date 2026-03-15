// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! Linux namespaces for the Lineluya kernel.
//! Supports PID, mount, network, user, UTS, and IPC namespaces.

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

// Clone flags for namespace creation (Linux-compatible)
pub const CLONE_NEWNS_CHIRHO: u64 = 0x00020000;    // Mount namespace
pub const CLONE_NEWPID_CHIRHO: u64 = 0x20000000;   // PID namespace
pub const CLONE_NEWNET_CHIRHO: u64 = 0x40000000;   // Network namespace
pub const CLONE_NEWUSER_CHIRHO: u64 = 0x10000000;  // User namespace
pub const CLONE_NEWUTS_CHIRHO: u64 = 0x04000000;    // UTS namespace
pub const CLONE_NEWIPC_CHIRHO: u64 = 0x08000000;    // IPC namespace

/// Namespace types.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NsTypeChirho {
    PidChirho,
    MountChirho,
    NetChirho,
    UserChirho,
    UtsChirho,
    IpcChirho,
}

/// A namespace instance.
#[derive(Debug, Clone)]
pub struct NamespaceChirho {
    pub id_chirho: u64,
    pub ns_type_chirho: NsTypeChirho,
    pub ref_count_chirho: u32,
}

/// Per-process namespace set.
#[derive(Debug, Clone)]
pub struct NsProxyChirho {
    pub pid_ns_chirho: u64,
    pub mnt_ns_chirho: u64,
    pub net_ns_chirho: u64,
    pub user_ns_chirho: u64,
    pub uts_ns_chirho: u64,
    pub ipc_ns_chirho: u64,
}

impl Default for NsProxyChirho {
    fn default() -> Self {
        Self {
            pid_ns_chirho: 0,
            mnt_ns_chirho: 0,
            net_ns_chirho: 0,
            user_ns_chirho: 0,
            uts_ns_chirho: 0,
            ipc_ns_chirho: 0,
        }
    }
}

/// UTS namespace data (hostname, domainname).
#[derive(Debug, Clone)]
pub struct UtsNamespaceChirho {
    pub nodename_chirho: String,
    pub domainname_chirho: String,
    pub sysname_chirho: String,
    pub release_chirho: String,
    pub version_chirho: String,
    pub machine_chirho: String,
}

impl Default for UtsNamespaceChirho {
    fn default() -> Self {
        Self {
            nodename_chirho: String::from("lineluya"),
            domainname_chirho: String::from("(none)"),
            sysname_chirho: String::from("Lineluya"),
            release_chirho: String::from("0.8.0-chirho"),
            version_chirho: String::from("#1 SMP"),
            machine_chirho: String::from("x86_64"),
        }
    }
}

/// Global namespace registry.
static NEXT_NS_ID_CHIRHO: Mutex<u64> = Mutex::new(1);

/// Create a new namespace and return its ID.
pub fn create_namespace_chirho(ns_type_chirho: NsTypeChirho) -> u64 {
    let mut id_chirho = NEXT_NS_ID_CHIRHO.lock();
    let ns_id_chirho = *id_chirho;
    *id_chirho += 1;
    ns_id_chirho
}

/// Create a new namespace set by cloning the parent and creating new namespaces
/// for the specified flags.
pub fn unshare_namespaces_chirho(parent_chirho: &NsProxyChirho, flags_chirho: u64) -> NsProxyChirho {
    let mut proxy_chirho = parent_chirho.clone();

    if flags_chirho & CLONE_NEWPID_CHIRHO != 0 {
        proxy_chirho.pid_ns_chirho = create_namespace_chirho(NsTypeChirho::PidChirho);
    }
    if flags_chirho & CLONE_NEWNS_CHIRHO != 0 {
        proxy_chirho.mnt_ns_chirho = create_namespace_chirho(NsTypeChirho::MountChirho);
    }
    if flags_chirho & CLONE_NEWNET_CHIRHO != 0 {
        proxy_chirho.net_ns_chirho = create_namespace_chirho(NsTypeChirho::NetChirho);
    }
    if flags_chirho & CLONE_NEWUSER_CHIRHO != 0 {
        proxy_chirho.user_ns_chirho = create_namespace_chirho(NsTypeChirho::UserChirho);
    }
    if flags_chirho & CLONE_NEWUTS_CHIRHO != 0 {
        proxy_chirho.uts_ns_chirho = create_namespace_chirho(NsTypeChirho::UtsChirho);
    }
    if flags_chirho & CLONE_NEWIPC_CHIRHO != 0 {
        proxy_chirho.ipc_ns_chirho = create_namespace_chirho(NsTypeChirho::IpcChirho);
    }

    proxy_chirho
}
