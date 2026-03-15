// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! cgroups v2 — resource control groups for the Lineluya kernel.

extern crate alloc;

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use spin::Mutex;

/// A cgroup controller type.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CgroupControllerChirho {
    CpuChirho,
    MemoryChirho,
    IoChirho,
    PidsChirho,
}

/// Resource limits for a cgroup.
#[derive(Debug, Clone)]
pub struct CgroupLimitsChirho {
    pub cpu_max_chirho: Option<u64>,         // CPU quota in microseconds per period
    pub cpu_period_chirho: u64,              // CPU period (default 100000us)
    pub memory_max_chirho: Option<u64>,      // Memory limit in bytes
    pub memory_high_chirho: Option<u64>,     // Memory high watermark
    pub pids_max_chirho: Option<u32>,        // Max number of processes
    pub io_weight_chirho: u32,               // I/O weight (1-10000)
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

/// Resource usage statistics for a cgroup.
#[derive(Debug, Clone, Default)]
pub struct CgroupStatsChirho {
    pub cpu_usage_us_chirho: u64,
    pub memory_current_chirho: u64,
    pub pids_current_chirho: u32,
    pub io_bytes_read_chirho: u64,
    pub io_bytes_written_chirho: u64,
}

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

    pub fn add_pid_chirho(&mut self, pid_chirho: u32) -> Result<(), i32> {
        if let Some(max_chirho) = self.limits_chirho.pids_max_chirho {
            if self.pids_chirho.len() as u32 >= max_chirho {
                return Err(-11); // EAGAIN
            }
        }
        if !self.pids_chirho.contains(&pid_chirho) {
            self.pids_chirho.push(pid_chirho);
            self.stats_chirho.pids_current_chirho = self.pids_chirho.len() as u32;
        }
        Ok(())
    }

    pub fn remove_pid_chirho(&mut self, pid_chirho: u32) {
        self.pids_chirho.retain(|p_chirho| *p_chirho != pid_chirho);
        self.stats_chirho.pids_current_chirho = self.pids_chirho.len() as u32;
    }

    pub fn check_memory_limit_chirho(&self, requested_chirho: u64) -> bool {
        match self.limits_chirho.memory_max_chirho {
            Some(max_chirho) => self.stats_chirho.memory_current_chirho + requested_chirho <= max_chirho,
            None => true,
        }
    }
}

/// Global cgroup hierarchy.
pub static CGROUP_ROOT_CHIRHO: Mutex<Vec<CgroupChirho>> = Mutex::new(Vec::new());

/// Initialize the cgroup v2 hierarchy with a root group.
pub fn init_cgroups_chirho() {
    let mut root_chirho = CGROUP_ROOT_CHIRHO.lock();
    if root_chirho.is_empty() {
        root_chirho.push(CgroupChirho::new_chirho("root", "/"));
    }
}
