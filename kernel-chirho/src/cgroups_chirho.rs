// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! cgroups v2 (Control Groups) subsystem for the Lineluya kernel (A6-015).
//!
//! Provides:
//! - Cgroup hierarchy with unified tree
//! - CPU controller (cpu.max, cpu.weight)
//! - Memory controller (memory.max, memory.current)
//! - PID controller (pids.max, pids.current)
//! - Process-to-cgroup assignment
//!
//! The cgroupfs is mounted at `/sys/fs/cgroup` and uses the unified
//! hierarchy model from cgroups v2.
//!
//! Reference: Linux Documentation/admin-guide/cgroup-v2.rst

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

// ============================================================================
// Resource controller types
// ============================================================================

/// CPU controller state.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct CpuControllerChirho {
    /// Maximum bandwidth (quota_us / period_us). "max" means unlimited.
    pub max_quota_us_chirho: i64,
    /// Period for bandwidth enforcement (default 100000us).
    pub period_us_chirho: u64,
    /// Weight for proportional sharing (1..10000, default 100).
    pub weight_chirho: u32,
}

impl Default for CpuControllerChirho {
    fn default() -> Self {
        Self {
            max_quota_us_chirho: -1, // max (unlimited)
            period_us_chirho: 100_000,
            weight_chirho: 100,
        }
    }
}

/// Memory controller state.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct MemoryControllerChirho {
    /// Hard memory limit in bytes (-1 = unlimited).
    pub max_bytes_chirho: i64,
    /// High memory threshold (soft limit).
    pub high_bytes_chirho: i64,
    /// Low memory protection threshold.
    pub low_bytes_chirho: i64,
    /// Current memory usage in bytes.
    pub current_bytes_chirho: u64,
    /// Swap maximum (-1 = unlimited).
    pub swap_max_chirho: i64,
}

impl Default for MemoryControllerChirho {
    fn default() -> Self {
        Self {
            max_bytes_chirho: -1,
            high_bytes_chirho: -1,
            low_bytes_chirho: 0,
            current_bytes_chirho: 0,
            swap_max_chirho: -1,
        }
    }
}

/// PID controller state.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct PidControllerChirho {
    /// Maximum number of processes (-1 = unlimited).
    pub max_pids_chirho: i64,
    /// Current number of processes in this cgroup.
    pub current_pids_chirho: u64,
}

impl Default for PidControllerChirho {
    fn default() -> Self {
        Self {
            max_pids_chirho: -1,
            current_pids_chirho: 0,
        }
    }
}

// ============================================================================
// Cgroup node
// ============================================================================

/// A single cgroup node in the hierarchy.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct CgroupChirho {
    /// Cgroup ID (unique within the hierarchy).
    pub id_chirho: u64,
    /// Path relative to the cgroup root (e.g. "/user.slice/session-1").
    pub path_chirho: String,
    /// Parent cgroup ID (0 for root).
    pub parent_id_chirho: u64,
    /// Child cgroup IDs.
    pub children_ids_chirho: Vec<u64>,
    /// PIDs of processes in this cgroup (leaf only).
    pub pids_chirho: Vec<u64>,
    /// Enabled controllers (bitmask).
    pub controllers_chirho: u32,
    /// CPU controller state.
    pub cpu_chirho: CpuControllerChirho,
    /// Memory controller state.
    pub mem_chirho: MemoryControllerChirho,
    /// PID controller state.
    pub pid_chirho: PidControllerChirho,
    /// Whether this cgroup is frozen.
    pub frozen_chirho: bool,
}

/// Controller type bitmask.
#[allow(dead_code)]
pub const CTRL_CPU_CHIRHO: u32 = 1 << 0;
#[allow(dead_code)]
pub const CTRL_MEMORY_CHIRHO: u32 = 1 << 1;
#[allow(dead_code)]
pub const CTRL_PIDS_CHIRHO: u32 = 1 << 2;
#[allow(dead_code)]
pub const CTRL_IO_CHIRHO: u32 = 1 << 3;

// ============================================================================
// Cgroup hierarchy
// ============================================================================

/// Global cgroup hierarchy.
struct CgroupHierarchyChirho {
    /// All cgroups keyed by ID.
    cgroups_chirho: BTreeMap<u64, CgroupChirho>,
    /// Next cgroup ID to allocate.
    next_id_chirho: u64,
    /// Map of PID -> cgroup ID for quick lookup.
    pid_to_cgroup_chirho: BTreeMap<u64, u64>,
}

impl CgroupHierarchyChirho {
    const fn new_chirho() -> Self {
        Self {
            cgroups_chirho: BTreeMap::new(),
            next_id_chirho: 1,
            pid_to_cgroup_chirho: BTreeMap::new(),
        }
    }
}

static CGROUP_HIERARCHY_CHIRHO: Mutex<CgroupHierarchyChirho> =
    Mutex::new(CgroupHierarchyChirho::new_chirho());

// ============================================================================
// API
// ============================================================================

/// Initialize the cgroups v2 hierarchy with a root cgroup.
#[allow(dead_code)]
pub fn init_cgroups_chirho() {
    let mut hier_chirho = CGROUP_HIERARCHY_CHIRHO.lock();
    let root_chirho = CgroupChirho {
        id_chirho: 0,
        path_chirho: String::from("/"),
        parent_id_chirho: 0,
        children_ids_chirho: Vec::new(),
        pids_chirho: Vec::new(),
        controllers_chirho: CTRL_CPU_CHIRHO | CTRL_MEMORY_CHIRHO | CTRL_PIDS_CHIRHO,
        cpu_chirho: CpuControllerChirho::default(),
        mem_chirho: MemoryControllerChirho::default(),
        pid_chirho: PidControllerChirho::default(),
        frozen_chirho: false,
    };
    hier_chirho.cgroups_chirho.insert(0, root_chirho);
    crate::serial_println_chirho!("[CGROUPS] v2 hierarchy initialized with root cgroup");
}

/// Create a new cgroup under a parent.
///
/// Returns the new cgroup ID, or negative errno.
#[allow(dead_code)]
pub fn create_cgroup_chirho(parent_id_chirho: u64, name_chirho: &str) -> i64 {
    let mut hier_chirho = CGROUP_HIERARCHY_CHIRHO.lock();

    // Verify parent exists
    let parent_path_chirho = match hier_chirho.cgroups_chirho.get(&parent_id_chirho) {
        Some(p_chirho) => p_chirho.path_chirho.clone(),
        None => return -(crate::syscall_chirho::ENOENT_CHIRHO),
    };

    let id_chirho = hier_chirho.next_id_chirho;
    hier_chirho.next_id_chirho += 1;

    let path_chirho = if parent_path_chirho == "/" {
        alloc::format!("/{}", name_chirho)
    } else {
        alloc::format!("{}/{}", parent_path_chirho, name_chirho)
    };

    // Inherit parent's enabled controllers
    let controllers_chirho = hier_chirho
        .cgroups_chirho
        .get(&parent_id_chirho)
        .map(|p_chirho| p_chirho.controllers_chirho)
        .unwrap_or(0);

    let cgroup_chirho = CgroupChirho {
        id_chirho,
        path_chirho: path_chirho.clone(),
        parent_id_chirho,
        children_ids_chirho: Vec::new(),
        pids_chirho: Vec::new(),
        controllers_chirho,
        cpu_chirho: CpuControllerChirho::default(),
        mem_chirho: MemoryControllerChirho::default(),
        pid_chirho: PidControllerChirho::default(),
        frozen_chirho: false,
    };

    hier_chirho.cgroups_chirho.insert(id_chirho, cgroup_chirho);

    // Register as child of parent
    if let Some(parent_chirho) = hier_chirho.cgroups_chirho.get_mut(&parent_id_chirho) {
        parent_chirho.children_ids_chirho.push(id_chirho);
    }

    crate::serial_println_chirho!(
        "[CGROUPS] Created cgroup id={} path={}",
        id_chirho,
        path_chirho
    );

    id_chirho as i64
}

/// Attach a process (by PID) to a cgroup.
#[allow(dead_code)]
pub fn attach_process_chirho(cgroup_id_chirho: u64, pid_chirho: u64) -> i64 {
    let mut hier_chirho = CGROUP_HIERARCHY_CHIRHO.lock();

    // Check PID limit
    if let Some(cg_chirho) = hier_chirho.cgroups_chirho.get(&cgroup_id_chirho) {
        if cg_chirho.pid_chirho.max_pids_chirho >= 0
            && cg_chirho.pid_chirho.current_pids_chirho as i64
                >= cg_chirho.pid_chirho.max_pids_chirho
        {
            return -(crate::syscall_chirho::EAGAIN_CHIRHO);
        }
    } else {
        return -(crate::syscall_chirho::ENOENT_CHIRHO);
    }

    // Remove from old cgroup
    if let Some(&old_cg_id_chirho) = hier_chirho.pid_to_cgroup_chirho.get(&pid_chirho) {
        if let Some(old_cg_chirho) = hier_chirho.cgroups_chirho.get_mut(&old_cg_id_chirho) {
            old_cg_chirho.pids_chirho.retain(|&p_chirho| p_chirho != pid_chirho);
            old_cg_chirho.pid_chirho.current_pids_chirho =
                old_cg_chirho.pid_chirho.current_pids_chirho.saturating_sub(1);
        }
    }

    // Add to new cgroup
    if let Some(cg_chirho) = hier_chirho.cgroups_chirho.get_mut(&cgroup_id_chirho) {
        cg_chirho.pids_chirho.push(pid_chirho);
        cg_chirho.pid_chirho.current_pids_chirho += 1;
    }

    hier_chirho.pid_to_cgroup_chirho.insert(pid_chirho, cgroup_id_chirho);

    0
}

/// Set the memory.max for a cgroup.
#[allow(dead_code)]
pub fn set_memory_max_chirho(cgroup_id_chirho: u64, max_bytes_chirho: i64) -> i64 {
    let mut hier_chirho = CGROUP_HIERARCHY_CHIRHO.lock();
    match hier_chirho.cgroups_chirho.get_mut(&cgroup_id_chirho) {
        Some(cg_chirho) => {
            cg_chirho.mem_chirho.max_bytes_chirho = max_bytes_chirho;
            0
        }
        None => -(crate::syscall_chirho::ENOENT_CHIRHO),
    }
}

/// Set the cpu.max (quota/period) for a cgroup.
#[allow(dead_code)]
pub fn set_cpu_max_chirho(
    cgroup_id_chirho: u64,
    quota_us_chirho: i64,
    period_us_chirho: u64,
) -> i64 {
    let mut hier_chirho = CGROUP_HIERARCHY_CHIRHO.lock();
    match hier_chirho.cgroups_chirho.get_mut(&cgroup_id_chirho) {
        Some(cg_chirho) => {
            cg_chirho.cpu_chirho.max_quota_us_chirho = quota_us_chirho;
            cg_chirho.cpu_chirho.period_us_chirho = period_us_chirho;
            0
        }
        None => -(crate::syscall_chirho::ENOENT_CHIRHO),
    }
}

/// Set the pids.max for a cgroup.
#[allow(dead_code)]
pub fn set_pids_max_chirho(cgroup_id_chirho: u64, max_pids_chirho: i64) -> i64 {
    let mut hier_chirho = CGROUP_HIERARCHY_CHIRHO.lock();
    match hier_chirho.cgroups_chirho.get_mut(&cgroup_id_chirho) {
        Some(cg_chirho) => {
            cg_chirho.pid_chirho.max_pids_chirho = max_pids_chirho;
            0
        }
        None => -(crate::syscall_chirho::ENOENT_CHIRHO),
    }
}
