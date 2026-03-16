// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! Kernel tracing / perf events stub for the Lineluya kernel.
//!
//! Provides a placeholder for `perf_event_open(2)` and a basic
//! `TraceEventChirho` struct.  Real tracing (ftrace, perf counters,
//! tracepoints) is not yet implemented.

use crate::syscall_chirho::ENOSYS_CHIRHO;

// ============================================================================
// Trace event structure
// ============================================================================

/// Placeholder descriptor for a kernel trace event.
#[allow(dead_code)]
pub struct TraceEventChirho {
    /// Event type (hardware, software, tracepoint, etc.).
    pub event_type_chirho: u32,
    /// Configuration value (which specific event within the type).
    pub config_chirho: u64,
    /// Whether this event is currently enabled.
    pub enabled_chirho: bool,
}

// ============================================================================
// Syscall stub
// ============================================================================

/// `perf_event_open(2)` stub.
///
/// # Arguments
/// * `_attr_ptr_chirho` — pointer to `struct perf_event_attr`
/// * `_pid_chirho` — target process (0 = calling process, -1 = all)
/// * `_cpu_chirho` — target CPU (-1 = any)
/// * `_group_fd_chirho` — group leader fd (-1 = create new group)
/// * `_flags_chirho` — flags (PERF_FLAG_*)
///
/// # Returns
/// Always `-ENOSYS`; perf events are not yet implemented.
pub fn sys_perf_event_open_chirho(
    _attr_ptr_chirho: u64,
    _pid_chirho: u64,
    _cpu_chirho: u64,
    _group_fd_chirho: u64,
    _flags_chirho: u64,
) -> i64 {
    crate::serial_println_chirho!(
        "[TRACE] perf_event_open() -- perf events not yet implemented"
    );
    -ENOSYS_CHIRHO
}
