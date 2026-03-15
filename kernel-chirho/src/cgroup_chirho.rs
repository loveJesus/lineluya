// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! cgroups v2 — resource control groups for the Lineluya kernel.
//!
//! ## Track F — Container Runtime (F1-005)
//!
//! Implements CPU and memory enforcement wired into the scheduler and
//! allocator:
//!
//! - **CPU enforcement**: `check_cpu_quota_chirho` is called from the
//!   scheduler tick to throttle processes that exceed their CPU quota.
//! - **Memory enforcement**: `check_memory_limit_chirho` /
//!   `account_memory_chirho` are called from the page allocator to
//!   enforce per-cgroup memory limits.
//! - **PID limits**: `check_pids_limit_chirho` enforces fork limits.

extern crate alloc;

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use spin::Mutex;

// ============================================================================
// Controller types
// ============================================================================

/// A cgroup controller type.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CgroupControllerChirho {
    CpuChirho,
    MemoryChirho,
    IoChirho,
    PidsChirho,
}

// ============================================================================
// Resource limits
// ============================================================================

/// Resource limits for a cgroup.
#[derive(Debug, Clone)]
pub struct CgroupLimitsChirho {
    /// CPU quota in microseconds per period (None = unlimited).
    pub cpu_max_chirho: Option<u64>,
    /// CPU period (default 100000us = 100ms).
    pub cpu_period_chirho: u64,
    /// Memory limit in bytes (hard limit — OOM kill above this).
    pub memory_max_chirho: Option<u64>,
    /// Memory high watermark (reclaim pressure above this).
    pub memory_high_chirho: Option<u64>,
    /// Max number of processes (None = unlimited).
    pub pids_max_chirho: Option<u32>,
    /// I/O weight (1-10000, default 100).
    pub io_weight_chirho: u32,
}

impl Default for CgroupLimitsChirho {
    fn default() -> Self {
        Self {
            cpu_max_chirho: None,
            cpu_period_chirho: 100_000,
            memory_max_chirho: None,
            memory_high_chirho: None,
            pids_max_chirho: None,
            io_weight_chirho: 100,
        }
    }
}

// ============================================================================
// Resource usage statistics
// ============================================================================

/// Resource usage statistics for a cgroup.
#[derive(Debug, Clone, Default)]
pub struct CgroupStatsChirho {
    /// Total CPU time consumed in microseconds.
    pub cpu_usage_us_chirho: u64,
    /// CPU time consumed in the current period (for quota enforcement).
    pub cpu_period_usage_us_chirho: u64,
    /// Timestamp (in scheduler ticks) when the current period started.
    pub cpu_period_start_chirho: u64,
    /// Whether this cgroup is currently throttled (CPU quota exceeded).
    pub cpu_throttled_chirho: bool,
    /// Number of times this cgroup has been throttled.
    pub cpu_nr_throttled_chirho: u64,
    /// Current memory usage in bytes.
    pub memory_current_chirho: u64,
    /// Peak memory usage in bytes.
    pub memory_peak_chirho: u64,
    /// Number of OOM kills triggered.
    pub memory_oom_kills_chirho: u64,
    /// Current number of processes.
    pub pids_current_chirho: u32,
    /// I/O bytes read.
    pub io_bytes_read_chirho: u64,
    /// I/O bytes written.
    pub io_bytes_written_chirho: u64,
}

// ============================================================================
// CgroupChirho — a cgroup node in the hierarchy
// ============================================================================

/// A cgroup node in the hierarchy.
#[derive(Debug, Clone)]
pub struct CgroupChirho {
    pub name_chirho: String,
    pub path_chirho: String,
    pub limits_chirho: CgroupLimitsChirho,
    pub stats_chirho: CgroupStatsChirho,
    pub pids_chirho: Vec<u32>,
    pub children_chirho: Vec<String>,
    pub controllers_chirho: Vec<CgroupControllerChirho>,
}

impl CgroupChirho {
    pub fn new_chirho(name_chirho: &str, path_chirho: &str) -> Self {
        Self {
            name_chirho: String::from(name_chirho),
            path_chirho: String::from(path_chirho),
            limits_chirho: CgroupLimitsChirho::default(),
            stats_chirho: CgroupStatsChirho::default(),
            pids_chirho: Vec::new(),
            children_chirho: Vec::new(),
            controllers_chirho: vec![
                CgroupControllerChirho::CpuChirho,
                CgroupControllerChirho::MemoryChirho,
                CgroupControllerChirho::IoChirho,
                CgroupControllerChirho::PidsChirho,
            ],
        }
    }

    /// Add a PID to this cgroup. Enforces pids.max limit.
    pub fn add_pid_chirho(&mut self, pid_chirho: u32) -> Result<(), i32> {
        if let Some(max_chirho) = self.limits_chirho.pids_max_chirho {
            if self.pids_chirho.len() as u32 >= max_chirho {
                crate::serial_println_chirho!(
                    "[CGROUP] pids.max reached for {}: {} >= {}",
                    self.path_chirho,
                    self.pids_chirho.len(),
                    max_chirho
                );
                return Err(-11); // EAGAIN
            }
        }
        if !self.pids_chirho.contains(&pid_chirho) {
            self.pids_chirho.push(pid_chirho);
            self.stats_chirho.pids_current_chirho = self.pids_chirho.len() as u32;
        }
        Ok(())
    }

    /// Remove a PID from this cgroup.
    pub fn remove_pid_chirho(&mut self, pid_chirho: u32) {
        self.pids_chirho.retain(|p_chirho| *p_chirho != pid_chirho);
        self.stats_chirho.pids_current_chirho = self.pids_chirho.len() as u32;
    }

    // ========================================================================
    // F1-005: CPU enforcement
    // ========================================================================

    /// Check if this cgroup has exhausted its CPU quota for the current period.
    ///
    /// Called from the scheduler tick. Returns `true` if the process should
    /// be throttled (not scheduled).
    pub fn check_cpu_quota_chirho(&mut self, current_tick_chirho: u64) -> bool {
        let quota_chirho = match self.limits_chirho.cpu_max_chirho {
            Some(q_chirho) => q_chirho,
            None => return false, // No limit
        };

        let period_chirho = self.limits_chirho.cpu_period_chirho;

        // Check if we've entered a new period
        // Each tick is roughly 1000us at 1kHz PIT
        let tick_us_chirho = 1000u64;
        let elapsed_chirho = current_tick_chirho
            .saturating_sub(self.stats_chirho.cpu_period_start_chirho)
            * tick_us_chirho;

        if elapsed_chirho >= period_chirho {
            // New period — reset usage
            self.stats_chirho.cpu_period_usage_us_chirho = 0;
            self.stats_chirho.cpu_period_start_chirho = current_tick_chirho;
            self.stats_chirho.cpu_throttled_chirho = false;
        }

        // Account one tick of CPU time
        self.stats_chirho.cpu_period_usage_us_chirho += tick_us_chirho;
        self.stats_chirho.cpu_usage_us_chirho += tick_us_chirho;

        // Check if quota is exceeded
        if self.stats_chirho.cpu_period_usage_us_chirho >= quota_chirho {
            if !self.stats_chirho.cpu_throttled_chirho {
                self.stats_chirho.cpu_throttled_chirho = true;
                self.stats_chirho.cpu_nr_throttled_chirho += 1;
                crate::serial_println_chirho!(
                    "[CGROUP:CPU] {} throttled: {}us / {}us quota in period",
                    self.path_chirho,
                    self.stats_chirho.cpu_period_usage_us_chirho,
                    quota_chirho
                );
            }
            return true; // Throttle this process
        }

        false
    }

    // ========================================================================
    // F1-005: Memory enforcement
    // ========================================================================

    /// Check if a memory allocation of `requested_chirho` bytes is allowed
    /// under this cgroup's memory.max limit.
    pub fn check_memory_limit_chirho(&self, requested_chirho: u64) -> bool {
        match self.limits_chirho.memory_max_chirho {
            Some(max_chirho) => {
                self.stats_chirho.memory_current_chirho + requested_chirho <= max_chirho
            }
            None => true,
        }
    }

    /// Check if memory usage exceeds memory.high (reclaim pressure).
    pub fn is_memory_high_chirho(&self) -> bool {
        match self.limits_chirho.memory_high_chirho {
            Some(high_chirho) => self.stats_chirho.memory_current_chirho >= high_chirho,
            None => false,
        }
    }

    /// Account a memory allocation (increase usage).
    pub fn account_memory_alloc_chirho(&mut self, bytes_chirho: u64) -> Result<(), i64> {
        if !self.check_memory_limit_chirho(bytes_chirho) {
            self.stats_chirho.memory_oom_kills_chirho += 1;
            crate::serial_println_chirho!(
                "[CGROUP:MEM] {} OOM: current={}B + request={}B > max={}B",
                self.path_chirho,
                self.stats_chirho.memory_current_chirho,
                bytes_chirho,
                self.limits_chirho.memory_max_chirho.unwrap_or(0)
            );
            return Err(-(crate::syscall_chirho::ENOMEM_CHIRHO));
        }

        self.stats_chirho.memory_current_chirho += bytes_chirho;
        if self.stats_chirho.memory_current_chirho > self.stats_chirho.memory_peak_chirho {
            self.stats_chirho.memory_peak_chirho = self.stats_chirho.memory_current_chirho;
        }

        if self.is_memory_high_chirho() {
            crate::serial_println_chirho!(
                "[CGROUP:MEM] {} memory.high pressure: {}B >= {}B",
                self.path_chirho,
                self.stats_chirho.memory_current_chirho,
                self.limits_chirho.memory_high_chirho.unwrap_or(0)
            );
        }

        Ok(())
    }

    /// Account a memory free (decrease usage).
    pub fn account_memory_free_chirho(&mut self, bytes_chirho: u64) {
        self.stats_chirho.memory_current_chirho =
            self.stats_chirho.memory_current_chirho.saturating_sub(bytes_chirho);
    }
}

// ============================================================================
// Global cgroup hierarchy
// ============================================================================

/// Global cgroup hierarchy.
pub static CGROUP_ROOT_CHIRHO: Mutex<Vec<CgroupChirho>> = Mutex::new(Vec::new());

/// Initialize the cgroup v2 hierarchy with a root group.
pub fn init_cgroups_chirho() {
    let mut root_chirho = CGROUP_ROOT_CHIRHO.lock();
    if root_chirho.is_empty() {
        root_chirho.push(CgroupChirho::new_chirho("root", "/"));
    }
}

// ============================================================================
// Scheduler integration — check CPU quota for a PID
// ============================================================================

/// Called from the scheduler tick to check if a process should be throttled.
///
/// Returns `true` if the process is CPU-throttled (quota exceeded).
pub fn check_cpu_quota_for_pid_chirho(pid_chirho: u32, current_tick_chirho: u64) -> bool {
    let mut cgroups_chirho = CGROUP_ROOT_CHIRHO.lock();
    for cg_chirho in cgroups_chirho.iter_mut() {
        if cg_chirho.pids_chirho.contains(&pid_chirho) {
            return cg_chirho.check_cpu_quota_chirho(current_tick_chirho);
        }
    }
    false // Not in any cgroup — no throttling
}

// ============================================================================
// Allocator integration — check memory limit for a PID
// ============================================================================

/// Called from the page allocator to check if a memory allocation is allowed.
///
/// Returns `Ok(())` if allowed, `Err(ENOMEM)` if the cgroup limit would be
/// exceeded.
pub fn check_memory_for_pid_chirho(pid_chirho: u32, bytes_chirho: u64) -> Result<(), i64> {
    let mut cgroups_chirho = CGROUP_ROOT_CHIRHO.lock();
    for cg_chirho in cgroups_chirho.iter_mut() {
        if cg_chirho.pids_chirho.contains(&pid_chirho) {
            return cg_chirho.account_memory_alloc_chirho(bytes_chirho);
        }
    }
    Ok(()) // Not in any cgroup — no limit
}

/// Called when memory is freed to update cgroup accounting.
pub fn free_memory_for_pid_chirho(pid_chirho: u32, bytes_chirho: u64) {
    let mut cgroups_chirho = CGROUP_ROOT_CHIRHO.lock();
    for cg_chirho in cgroups_chirho.iter_mut() {
        if cg_chirho.pids_chirho.contains(&pid_chirho) {
            cg_chirho.account_memory_free_chirho(bytes_chirho);
            return;
        }
    }
}

// ============================================================================
// Fork integration — check PID limit
// ============================================================================

/// Called from fork/clone to check if creating a new process is allowed
/// under the cgroup's pids.max limit.
///
/// Returns `Ok(())` if allowed, `Err(EAGAIN)` if the limit is reached.
pub fn check_pids_limit_for_pid_chirho(parent_pid_chirho: u32) -> Result<(), i64> {
    let cgroups_chirho = CGROUP_ROOT_CHIRHO.lock();
    for cg_chirho in cgroups_chirho.iter() {
        if cg_chirho.pids_chirho.contains(&parent_pid_chirho) {
            if let Some(max_chirho) = cg_chirho.limits_chirho.pids_max_chirho {
                if cg_chirho.pids_chirho.len() as u32 >= max_chirho {
                    crate::serial_println_chirho!(
                        "[CGROUP:PIDS] Fork denied for PID {}: pids.max={} reached",
                        parent_pid_chirho,
                        max_chirho
                    );
                    return Err(-(crate::syscall_chirho::EAGAIN_CHIRHO));
                }
            }
        }
    }
    Ok(())
}

/// Create a child cgroup under the root.
pub fn create_cgroup_chirho(name_chirho: &str) -> Result<(), i64> {
    let path_chirho = alloc::format!("/{}", name_chirho);
    let mut cgroups_chirho = CGROUP_ROOT_CHIRHO.lock();

    // Check for duplicates
    for cg_chirho in cgroups_chirho.iter() {
        if cg_chirho.path_chirho == path_chirho {
            return Err(-(crate::syscall_chirho::EEXIST_CHIRHO));
        }
    }

    let cg_chirho = CgroupChirho::new_chirho(name_chirho, &path_chirho);
    cgroups_chirho.push(cg_chirho);

    crate::serial_println_chirho!(
        "[CGROUP] Created cgroup: {}",
        path_chirho
    );

    Ok(())
}

/// Set CPU quota for a cgroup.
pub fn set_cpu_limit_chirho(
    path_chirho: &str,
    quota_us_chirho: u64,
    period_us_chirho: u64,
) -> Result<(), i64> {
    let mut cgroups_chirho = CGROUP_ROOT_CHIRHO.lock();
    for cg_chirho in cgroups_chirho.iter_mut() {
        if cg_chirho.path_chirho == path_chirho {
            cg_chirho.limits_chirho.cpu_max_chirho = Some(quota_us_chirho);
            cg_chirho.limits_chirho.cpu_period_chirho = period_us_chirho;
            crate::serial_println_chirho!(
                "[CGROUP:CPU] Set {}: cpu.max={}/{}us",
                path_chirho,
                quota_us_chirho,
                period_us_chirho
            );
            return Ok(());
        }
    }
    Err(-(crate::syscall_chirho::ENOENT_CHIRHO))
}

/// Set memory limit for a cgroup.
pub fn set_memory_limit_chirho(
    path_chirho: &str,
    max_bytes_chirho: u64,
) -> Result<(), i64> {
    let mut cgroups_chirho = CGROUP_ROOT_CHIRHO.lock();
    for cg_chirho in cgroups_chirho.iter_mut() {
        if cg_chirho.path_chirho == path_chirho {
            cg_chirho.limits_chirho.memory_max_chirho = Some(max_bytes_chirho);
            crate::serial_println_chirho!(
                "[CGROUP:MEM] Set {}: memory.max={}B",
                path_chirho,
                max_bytes_chirho
            );
            return Ok(());
        }
    }
    Err(-(crate::syscall_chirho::ENOENT_CHIRHO))
}
