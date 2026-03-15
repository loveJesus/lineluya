// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! seccomp-BPF — syscall filtering for the Lineluya kernel.

extern crate alloc;

use alloc::vec::Vec;
use spin::Mutex;

// seccomp operations
pub const SECCOMP_SET_MODE_STRICT_CHIRHO: u32 = 0;
pub const SECCOMP_SET_MODE_FILTER_CHIRHO: u32 = 1;

// seccomp actions
pub const SECCOMP_RET_KILL_PROCESS_CHIRHO: u32 = 0x80000000;
pub const SECCOMP_RET_KILL_THREAD_CHIRHO: u32 = 0x00000000;
pub const SECCOMP_RET_TRAP_CHIRHO: u32 = 0x00030000;
pub const SECCOMP_RET_ERRNO_CHIRHO: u32 = 0x00050000;
pub const SECCOMP_RET_ALLOW_CHIRHO: u32 = 0x7FFF0000;
pub const SECCOMP_RET_LOG_CHIRHO: u32 = 0x7FFC0000;

/// BPF instruction for seccomp filters.
#[derive(Debug, Clone, Copy)]
pub struct BpfInsnChirho {
    pub code_chirho: u16,
    pub jt_chirho: u8,
    pub jf_chirho: u8,
    pub k_chirho: u32,
}

/// A seccomp BPF filter program.
#[derive(Debug, Clone)]
pub struct SeccompFilterChirho {
    pub insns_chirho: Vec<BpfInsnChirho>,
}

/// Per-process seccomp state.
#[derive(Debug, Clone)]
pub struct SeccompStateChirho {
    pub mode_chirho: u32, // 0=disabled, 1=strict, 2=filter
    pub filters_chirho: Vec<SeccompFilterChirho>,
}

impl Default for SeccompStateChirho {
    fn default() -> Self {
        Self {
            mode_chirho: 0,
            filters_chirho: Vec::new(),
        }
    }
}

impl SeccompStateChirho {
    /// Check if a syscall is allowed under current seccomp policy.
    /// Returns the action to take.
    pub fn check_syscall_chirho(&self, syscall_nr_chirho: u64) -> u32 {
        match self.mode_chirho {
            0 => SECCOMP_RET_ALLOW_CHIRHO, // Disabled
            1 => {
                // Strict mode: only read, write, exit, sigreturn allowed
                match syscall_nr_chirho {
                    0 | 1 | 60 | 15 => SECCOMP_RET_ALLOW_CHIRHO,
                    _ => SECCOMP_RET_KILL_PROCESS_CHIRHO,
                }
            }
            2 => {
                // Filter mode: run BPF programs
                // Simplified: if any filter exists, allow all for now
                // Full BPF interpreter would go here
                if self.filters_chirho.is_empty() {
                    SECCOMP_RET_ALLOW_CHIRHO
                } else {
                    SECCOMP_RET_ALLOW_CHIRHO // Stub: always allow
                }
            }
            _ => SECCOMP_RET_ALLOW_CHIRHO,
        }
    }

    /// Enable strict mode.
    pub fn set_strict_chirho(&mut self) {
        self.mode_chirho = 1;
    }

    /// Add a BPF filter program.
    pub fn add_filter_chirho(&mut self, filter_chirho: SeccompFilterChirho) {
        self.mode_chirho = 2;
        self.filters_chirho.push(filter_chirho);
    }
}
