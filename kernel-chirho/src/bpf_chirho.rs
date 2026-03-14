// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! eBPF virtual machine stub for the Lineluya kernel.
//!
//! Provides placeholder structures and a syscall handler for `bpf(2)`.
//! All operations return `-ENOSYS` until a real eBPF verifier and JIT are
//! implemented.

extern crate alloc;

use alloc::vec::Vec;
use alloc::string::String;
use crate::syscall_chirho::ENOSYS_CHIRHO;

// ============================================================================
// BPF program types
// ============================================================================

/// eBPF program type, matching `enum bpf_prog_type` in Linux.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
#[allow(dead_code)]
pub enum BpfProgTypeChirho {
    /// Unspecified / invalid.
    UnspecifiedChirho = 0,
    /// Classic socket filter.
    SocketFilterChirho = 1,
    /// Kprobe/kretprobe program.
    KprobeChirho = 2,
    /// Scheduler classifier (TC).
    SchedClsChirho = 3,
    /// Scheduler action (TC).
    SchedActChirho = 4,
    /// Tracepoint program.
    TracepointChirho = 5,
    /// XDP (eXpress Data Path) program.
    XdpChirho = 6,
    /// Perf event program.
    PerfEventChirho = 7,
    /// Cgroup SKB program.
    CgroupSkbChirho = 8,
    /// Cgroup socket program.
    CgroupSockChirho = 9,
    /// Lightweight tunnel program.
    LwtInChirho = 10,
    /// Raw tracepoint program.
    RawTracepointChirho = 11,
    /// Socket operations program.
    SockOpsChirho = 12,
    /// LSM (Linux Security Module) program.
    LsmChirho = 29,
}

// ============================================================================
// BPF structures
// ============================================================================

/// Descriptor for a loaded BPF program.
#[allow(dead_code)]
pub struct BpfProgChirho {
    /// Program type.
    pub prog_type_chirho: BpfProgTypeChirho,
    /// Raw BPF instructions (each instruction is 8 bytes, packed as u64).
    pub insns_chirho: Vec<u64>,
    /// License string (e.g. "GPL").
    pub license_chirho: String,
}

/// Descriptor for a BPF map.
#[allow(dead_code)]
pub struct BpfMapChirho {
    /// Map type (hash, array, etc.).
    pub map_type_chirho: u32,
    /// Size of each key in bytes.
    pub key_size_chirho: u32,
    /// Size of each value in bytes.
    pub value_size_chirho: u32,
    /// Maximum number of entries.
    pub max_entries_chirho: u32,
}

// ============================================================================
// Syscall stub
// ============================================================================

/// `bpf(2)` syscall stub.
///
/// # Arguments
/// * `_cmd_chirho` — BPF command (BPF_PROG_LOAD, BPF_MAP_CREATE, etc.)
/// * `_attr_ptr_chirho` — pointer to `union bpf_attr`
/// * `_size_chirho` — size of the attr structure
///
/// # Returns
/// Always `-ENOSYS`; eBPF is not yet implemented.
pub fn sys_bpf_chirho(_cmd_chirho: u64, _attr_ptr_chirho: u64, _size_chirho: u64) -> i64 {
    crate::serial_println_chirho!(
        "[BPF] bpf() cmd={} -- eBPF not yet implemented",
        _cmd_chirho
    );
    -ENOSYS_CHIRHO
}
