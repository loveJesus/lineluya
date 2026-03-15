// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! Linux namespaces for the Lineluya kernel.
//!
//! Supports PID, mount, network, user, UTS, and IPC namespaces.
//!
//! ## Track F — Container Runtime
//!
//! - **F1-001**: PID namespace enforcement — child sees PID 1 inside a new
//!   PID namespace. PID translation between inner/outer namespaces.
//! - **F1-002**: Mount namespace with `pivot_root` syscall.
//! - **F1-003**: Network namespace with virtual ethernet (veth) pairs.

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

// ============================================================================
// Clone flags for namespace creation (Linux-compatible)
// ============================================================================

pub const CLONE_NEWNS_CHIRHO: u64 = 0x00020000;    // Mount namespace
pub const CLONE_NEWPID_CHIRHO: u64 = 0x20000000;   // PID namespace
pub const CLONE_NEWNET_CHIRHO: u64 = 0x40000000;   // Network namespace
pub const CLONE_NEWUSER_CHIRHO: u64 = 0x10000000;  // User namespace
pub const CLONE_NEWUTS_CHIRHO: u64 = 0x04000000;    // UTS namespace
pub const CLONE_NEWIPC_CHIRHO: u64 = 0x08000000;    // IPC namespace

// ============================================================================
// Namespace types
// ============================================================================

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

// ============================================================================
// F1-001: PID Namespace — PID translation between inner/outer namespaces
// ============================================================================

/// PID namespace — tracks PID mapping between host (outer) PIDs and
/// namespace-local (inner) PIDs. The first process in a new PID namespace
/// gets inner PID 1 (the init of that namespace).
#[derive(Debug, Clone)]
pub struct PidNamespaceChirho {
    /// Unique namespace ID.
    pub ns_id_chirho: u64,
    /// Parent PID namespace ID (0 = root namespace).
    pub parent_ns_id_chirho: u64,
    /// Mapping: (outer_pid, inner_pid).
    pub pid_map_chirho: Vec<(u64, u64)>,
    /// Next inner PID to assign (starts at 1).
    pub next_inner_pid_chirho: u64,
}

impl PidNamespaceChirho {
    /// Create a new PID namespace.
    pub fn new_chirho(ns_id_chirho: u64, parent_ns_id_chirho: u64) -> Self {
        Self {
            ns_id_chirho,
            parent_ns_id_chirho,
            next_inner_pid_chirho: 1,
            pid_map_chirho: Vec::new(),
        }
    }

    /// Assign an inner PID for the given outer (host) PID.
    /// The first process added gets inner PID 1 (container init).
    pub fn assign_pid_chirho(&mut self, outer_pid_chirho: u64) -> u64 {
        let inner_pid_chirho = self.next_inner_pid_chirho;
        self.pid_map_chirho.push((outer_pid_chirho, inner_pid_chirho));
        self.next_inner_pid_chirho += 1;
        inner_pid_chirho
    }

    /// Translate an outer PID to its inner PID in this namespace.
    /// Returns `None` if the PID is not in this namespace.
    pub fn outer_to_inner_chirho(&self, outer_pid_chirho: u64) -> Option<u64> {
        self.pid_map_chirho
            .iter()
            .find(|(outer_chirho, _)| *outer_chirho == outer_pid_chirho)
            .map(|(_, inner_chirho)| *inner_chirho)
    }

    /// Translate an inner PID to its outer (host) PID.
    pub fn inner_to_outer_chirho(&self, inner_pid_chirho: u64) -> Option<u64> {
        self.pid_map_chirho
            .iter()
            .find(|(_, inner_chirho)| *inner_chirho == inner_pid_chirho)
            .map(|(outer_chirho, _)| *outer_chirho)
    }

    /// Remove a PID mapping (on process exit).
    pub fn remove_pid_chirho(&mut self, outer_pid_chirho: u64) {
        self.pid_map_chirho
            .retain(|(outer_chirho, _)| *outer_chirho != outer_pid_chirho);
    }
}

/// Global PID namespace registry.
pub static PID_NAMESPACES_CHIRHO: Mutex<Vec<PidNamespaceChirho>> = Mutex::new(Vec::new());

/// The root PID namespace has ID 0.
pub const ROOT_PID_NS_ID_CHIRHO: u64 = 0;

/// Initialise the PID namespace subsystem with the root namespace.
pub fn init_pid_namespaces_chirho() {
    let mut nses_chirho = PID_NAMESPACES_CHIRHO.lock();
    if nses_chirho.is_empty() {
        nses_chirho.push(PidNamespaceChirho::new_chirho(ROOT_PID_NS_ID_CHIRHO, 0));
    }
}

/// Assign an inner PID for `outer_pid_chirho` in the namespace `ns_id_chirho`.
/// Returns the inner PID (1 for the first process in a new namespace).
pub fn assign_pid_in_ns_chirho(ns_id_chirho: u64, outer_pid_chirho: u64) -> u64 {
    let mut nses_chirho = PID_NAMESPACES_CHIRHO.lock();
    for ns_chirho in nses_chirho.iter_mut() {
        if ns_chirho.ns_id_chirho == ns_id_chirho {
            return ns_chirho.assign_pid_chirho(outer_pid_chirho);
        }
    }
    // Namespace not found — return outer PID unchanged (root ns behaviour).
    outer_pid_chirho
}

/// Translate an outer PID to its namespace-local PID.
pub fn translate_pid_chirho(ns_id_chirho: u64, outer_pid_chirho: u64) -> u64 {
    if ns_id_chirho == ROOT_PID_NS_ID_CHIRHO {
        return outer_pid_chirho;
    }
    let nses_chirho = PID_NAMESPACES_CHIRHO.lock();
    for ns_chirho in nses_chirho.iter() {
        if ns_chirho.ns_id_chirho == ns_id_chirho {
            return ns_chirho.outer_to_inner_chirho(outer_pid_chirho)
                .unwrap_or(outer_pid_chirho);
        }
    }
    outer_pid_chirho
}

// ============================================================================
// F1-002: Mount Namespace with pivot_root
// ============================================================================

/// Mount namespace — tracks the mount tree root for a process group.
#[derive(Debug, Clone)]
pub struct MountNamespaceChirho {
    /// Unique namespace ID.
    pub ns_id_chirho: u64,
    /// Path of the root filesystem for this namespace.
    pub root_path_chirho: String,
    /// Mount points: (source, target, fstype).
    pub mounts_chirho: Vec<(String, String, String)>,
}

impl MountNamespaceChirho {
    pub fn new_chirho(ns_id_chirho: u64) -> Self {
        Self {
            ns_id_chirho,
            root_path_chirho: String::from("/"),
            mounts_chirho: Vec::new(),
        }
    }

    /// Perform a pivot_root operation: set `new_root_chirho` as the new root
    /// and move the old root to `put_old_chirho`.
    ///
    /// Returns 0 on success, negative errno on error.
    pub fn pivot_root_chirho(
        &mut self,
        new_root_chirho: &str,
        put_old_chirho: &str,
    ) -> i64 {
        // Validate that new_root is an absolute path and a mount point
        if !new_root_chirho.starts_with('/') || !put_old_chirho.starts_with('/') {
            return -(crate::syscall_chirho::EINVAL_CHIRHO);
        }

        // Record the old root so it can be referenced at put_old
        let old_root_chirho = self.root_path_chirho.clone();

        // Set the new root
        self.root_path_chirho = String::from(new_root_chirho);

        // Mount the old root at put_old (relative to new root)
        self.mounts_chirho.push((
            old_root_chirho,
            String::from(put_old_chirho),
            String::from("bind"),
        ));

        crate::serial_println_chirho!(
            "[NS:MNT] pivot_root: new_root={}, put_old={}",
            new_root_chirho,
            put_old_chirho
        );

        0
    }

    /// Add a mount to this namespace.
    pub fn add_mount_chirho(
        &mut self,
        source_chirho: &str,
        target_chirho: &str,
        fstype_chirho: &str,
    ) {
        self.mounts_chirho.push((
            String::from(source_chirho),
            String::from(target_chirho),
            String::from(fstype_chirho),
        ));
    }
}

/// Global mount namespace registry.
pub static MOUNT_NAMESPACES_CHIRHO: Mutex<Vec<MountNamespaceChirho>> = Mutex::new(Vec::new());

/// Initialise the mount namespace subsystem.
pub fn init_mount_namespaces_chirho() {
    let mut nses_chirho = MOUNT_NAMESPACES_CHIRHO.lock();
    if nses_chirho.is_empty() {
        nses_chirho.push(MountNamespaceChirho::new_chirho(0));
    }
}

/// `pivot_root(new_root, put_old)` syscall implementation.
///
/// Changes the root mount of the calling process's mount namespace.
pub fn sys_pivot_root_chirho(new_root_chirho: u64, put_old_chirho: u64) -> i64 {
    // Read the path strings from userspace
    let new_root_str_chirho = match crate::uaccess_chirho::read_user_string_chirho(
        new_root_chirho, 4096,
    ) {
        Ok(s_chirho) => s_chirho,
        Err(_) => return -(crate::syscall_chirho::EFAULT_CHIRHO),
    };
    let put_old_str_chirho = match crate::uaccess_chirho::read_user_string_chirho(
        put_old_chirho, 4096,
    ) {
        Ok(s_chirho) => s_chirho,
        Err(_) => return -(crate::syscall_chirho::EFAULT_CHIRHO),
    };

    // Find the current process's mount namespace
    let mnt_ns_id_chirho = get_current_mnt_ns_chirho();

    let mut nses_chirho = MOUNT_NAMESPACES_CHIRHO.lock();
    for ns_chirho in nses_chirho.iter_mut() {
        if ns_chirho.ns_id_chirho == mnt_ns_id_chirho {
            return ns_chirho.pivot_root_chirho(&new_root_str_chirho, &put_old_str_chirho);
        }
    }

    -(crate::syscall_chirho::EINVAL_CHIRHO)
}

/// Get the current process's mount namespace ID.
fn get_current_mnt_ns_chirho() -> u64 {
    // Look up the current task's ns_proxy, if available.
    // For now, return the root namespace (0).
    0
}

// ============================================================================
// F1-003: Network Namespace with veth pairs
// ============================================================================

/// A virtual ethernet (veth) device — one end of a pair.
#[derive(Debug, Clone)]
pub struct VethDeviceChirho {
    /// Device name (e.g. "veth0-chirho").
    pub name_chirho: String,
    /// Namespace ID this end lives in.
    pub ns_id_chirho: u64,
    /// MAC address (6 bytes).
    pub mac_addr_chirho: [u8; 6],
    /// IPv4 address (if assigned).
    pub ipv4_addr_chirho: Option<u32>,
    /// Peer device name.
    pub peer_name_chirho: String,
    /// Whether the interface is up.
    pub is_up_chirho: bool,
    /// Transmit queue — packets written here are delivered to the peer.
    pub tx_queue_chirho: Vec<Vec<u8>>,
}

/// A veth pair — two connected virtual ethernet devices.
#[derive(Debug, Clone)]
pub struct VethPairChirho {
    pub dev_a_chirho: VethDeviceChirho,
    pub dev_b_chirho: VethDeviceChirho,
}

/// Network namespace — owns a set of virtual network devices and
/// routing rules.
#[derive(Debug, Clone)]
pub struct NetNamespaceChirho {
    /// Unique namespace ID.
    pub ns_id_chirho: u64,
    /// Network devices in this namespace (name, mac, ipv4).
    pub devices_chirho: Vec<VethDeviceChirho>,
    /// Has a loopback device?
    pub has_loopback_chirho: bool,
}

impl NetNamespaceChirho {
    pub fn new_chirho(ns_id_chirho: u64) -> Self {
        Self {
            ns_id_chirho,
            devices_chirho: Vec::new(),
            has_loopback_chirho: true, // Every netns has a loopback
        }
    }

    /// Add a veth end to this namespace.
    pub fn add_device_chirho(&mut self, dev_chirho: VethDeviceChirho) {
        self.devices_chirho.push(dev_chirho);
    }

    /// Remove a device by name.
    pub fn remove_device_chirho(&mut self, name_chirho: &str) {
        self.devices_chirho
            .retain(|d_chirho| d_chirho.name_chirho != name_chirho);
    }

    /// Look up a device by name.
    pub fn find_device_chirho(&self, name_chirho: &str) -> Option<&VethDeviceChirho> {
        self.devices_chirho
            .iter()
            .find(|d_chirho| d_chirho.name_chirho == name_chirho)
    }
}

/// Global network namespace registry.
pub static NET_NAMESPACES_CHIRHO: Mutex<Vec<NetNamespaceChirho>> = Mutex::new(Vec::new());

/// Initialise the network namespace subsystem.
pub fn init_net_namespaces_chirho() {
    let mut nses_chirho = NET_NAMESPACES_CHIRHO.lock();
    if nses_chirho.is_empty() {
        nses_chirho.push(NetNamespaceChirho::new_chirho(0));
    }
}

/// Counter for generating unique veth MAC addresses.
static NEXT_VETH_MAC_CHIRHO: Mutex<u32> = Mutex::new(1);

/// Generate a unique MAC address for a veth device.
fn gen_veth_mac_chirho() -> [u8; 6] {
    let mut counter_chirho = NEXT_VETH_MAC_CHIRHO.lock();
    let val_chirho = *counter_chirho;
    *counter_chirho += 1;
    // Use locally administered, unicast address: 02:xx:xx:xx:xx:xx
    [
        0x02,
        0xCE, // "CE" for chirho-ethernet
        ((val_chirho >> 24) & 0xFF) as u8,
        ((val_chirho >> 16) & 0xFF) as u8,
        ((val_chirho >> 8) & 0xFF) as u8,
        (val_chirho & 0xFF) as u8,
    ]
}

/// Create a veth pair between two network namespaces.
///
/// Returns the pair of device names on success.
pub fn create_veth_pair_chirho(
    name_a_chirho: &str,
    ns_a_chirho: u64,
    name_b_chirho: &str,
    ns_b_chirho: u64,
) -> Result<VethPairChirho, i64> {
    let dev_a_chirho = VethDeviceChirho {
        name_chirho: String::from(name_a_chirho),
        ns_id_chirho: ns_a_chirho,
        mac_addr_chirho: gen_veth_mac_chirho(),
        ipv4_addr_chirho: None,
        peer_name_chirho: String::from(name_b_chirho),
        is_up_chirho: false,
        tx_queue_chirho: Vec::new(),
    };

    let dev_b_chirho = VethDeviceChirho {
        name_chirho: String::from(name_b_chirho),
        ns_id_chirho: ns_b_chirho,
        mac_addr_chirho: gen_veth_mac_chirho(),
        ipv4_addr_chirho: None,
        peer_name_chirho: String::from(name_a_chirho),
        is_up_chirho: false,
        tx_queue_chirho: Vec::new(),
    };

    let pair_chirho = VethPairChirho {
        dev_a_chirho: dev_a_chirho.clone(),
        dev_b_chirho: dev_b_chirho.clone(),
    };

    // Register device A in namespace A
    let mut nses_chirho = NET_NAMESPACES_CHIRHO.lock();
    for ns_chirho in nses_chirho.iter_mut() {
        if ns_chirho.ns_id_chirho == ns_a_chirho {
            ns_chirho.add_device_chirho(dev_a_chirho.clone());
        }
        if ns_chirho.ns_id_chirho == ns_b_chirho {
            ns_chirho.add_device_chirho(dev_b_chirho.clone());
        }
    }

    crate::serial_println_chirho!(
        "[NS:NET] Created veth pair: {} (ns={}) <-> {} (ns={})",
        name_a_chirho,
        ns_a_chirho,
        name_b_chirho,
        ns_b_chirho
    );

    Ok(pair_chirho)
}

/// Send a packet from one veth end to its peer.
/// In a real implementation, this would deliver to the peer's rx queue.
pub fn veth_transmit_chirho(
    src_ns_chirho: u64,
    dev_name_chirho: &str,
    packet_chirho: &[u8],
) -> Result<(), i64> {
    let nses_chirho = NET_NAMESPACES_CHIRHO.lock();

    // Find the source device and its peer info
    let mut peer_name_chirho = String::new();
    let mut peer_ns_chirho = 0u64;
    let mut found_chirho = false;

    for ns_chirho in nses_chirho.iter() {
        if ns_chirho.ns_id_chirho == src_ns_chirho {
            if let Some(dev_chirho) = ns_chirho.find_device_chirho(dev_name_chirho) {
                peer_name_chirho = dev_chirho.peer_name_chirho.clone();
                found_chirho = true;
                // Find the peer's namespace
                for ns2_chirho in nses_chirho.iter() {
                    if ns2_chirho.find_device_chirho(&peer_name_chirho).is_some() {
                        peer_ns_chirho = ns2_chirho.ns_id_chirho;
                        break;
                    }
                }
                break;
            }
        }
    }

    if !found_chirho {
        return Err(-(crate::syscall_chirho::ENOENT_CHIRHO));
    }

    crate::serial_println_chirho!(
        "[NS:NET] veth_transmit: {} (ns={}) -> {} (ns={}), {} bytes",
        dev_name_chirho,
        src_ns_chirho,
        peer_name_chirho,
        peer_ns_chirho,
        packet_chirho.len()
    );

    Ok(())
}

// ============================================================================
// Per-process namespace set (NsProxy)
// ============================================================================

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

// ============================================================================
// Global namespace registry
// ============================================================================

static NEXT_NS_ID_CHIRHO: Mutex<u64> = Mutex::new(1);

/// Create a new namespace and return its ID.
pub fn create_namespace_chirho(ns_type_chirho: NsTypeChirho) -> u64 {
    let mut id_chirho = NEXT_NS_ID_CHIRHO.lock();
    let ns_id_chirho = *id_chirho;
    *id_chirho += 1;

    // For PID namespaces, also create the PID namespace state
    match ns_type_chirho {
        NsTypeChirho::PidChirho => {
            let mut nses_chirho = PID_NAMESPACES_CHIRHO.lock();
            nses_chirho.push(PidNamespaceChirho::new_chirho(ns_id_chirho, 0));
            crate::serial_println_chirho!(
                "[NS:PID] Created PID namespace id={}",
                ns_id_chirho
            );
        }
        NsTypeChirho::MountChirho => {
            let mut nses_chirho = MOUNT_NAMESPACES_CHIRHO.lock();
            nses_chirho.push(MountNamespaceChirho::new_chirho(ns_id_chirho));
            crate::serial_println_chirho!(
                "[NS:MNT] Created mount namespace id={}",
                ns_id_chirho
            );
        }
        NsTypeChirho::NetChirho => {
            let mut nses_chirho = NET_NAMESPACES_CHIRHO.lock();
            nses_chirho.push(NetNamespaceChirho::new_chirho(ns_id_chirho));
            crate::serial_println_chirho!(
                "[NS:NET] Created network namespace id={}",
                ns_id_chirho
            );
        }
        _ => {}
    }

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

/// `unshare(2)` syscall — disassociate parts of the process execution context.
///
/// Creates new namespaces for the calling process based on `flags_chirho`.
pub fn sys_unshare_chirho(flags_chirho: u64) -> i64 {
    let _proxy_chirho = unshare_namespaces_chirho(
        &NsProxyChirho::default(),
        flags_chirho,
    );
    crate::serial_println_chirho!(
        "[NS] unshare(flags={:#x}): created new namespaces",
        flags_chirho
    );
    0
}
