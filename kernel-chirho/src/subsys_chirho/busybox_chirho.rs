// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! Centralized BusyBox applet registry for Lineluya.
//!
//! Previously the list of known BusyBox applet names was duplicated in three
//! separate locations:
//!   - `vfs_ops_chirho.rs` — `populate_bin_applets_chirho()` bootstrap symlinks
//!   - `syscall_chirho.rs` — `is_busybox_applet_chirho()` stat fallback
//!   - `process_core_chirho.rs` — `sys_execve_chirho()` applet detection
//!
//! This module provides a single source of truth (`BUSYBOX_APPLETS_CHIRHO`)
//! and a helper function (`is_busybox_applet_chirho`) so that all three
//! consumers reference the same canonical list.
//!
//! A2-PROC-008: Centralize BusyBox applet registry.

/// Canonical list of BusyBox applet names recognised by the Lineluya kernel.
///
/// Used by:
///   - VFS bootstrap to create `/bin/<applet>` entries at boot
///   - `stat(2)` fallback to report BusyBox applets as executable files
///   - `execve(2)` to redirect applet names to the embedded BusyBox binary
///
/// When adding a new applet, add it here once and all three subsystems
/// pick it up automatically.
pub const BUSYBOX_APPLETS_CHIRHO: &[&str] = &[
    // -- Core shell --
    "ash",
    "sh",
    "busybox",
    // -- File operations --
    "ls",
    "cat",
    "cp",
    "mv",
    "rm",
    "mkdir",
    "rmdir",
    "chmod",
    "chown",
    "ln",
    "touch",
    "stat",
    "readlink",
    "basename",
    "dirname",
    "realpath",
    // -- Text processing --
    "head",
    "tail",
    "wc",
    "grep",
    "sed",
    "awk",
    "sort",
    "uniq",
    "tr",
    "cut",
    "find",
    "xargs",
    "tee",
    // -- Output / formatting --
    "echo",
    "printf",
    "seq",
    "yes",
    "clear",
    "reset",
    // -- System info --
    "date",
    "uname",
    "id",
    "whoami",
    "hostname",
    "env",
    "printenv",
    "uptime",
    "dmesg",
    "free",
    "ps",
    "pwd",
    "top",
    // -- Filesystem / disk --
    "du",
    "df",
    "mount",
    "umount",
    "dd",
    // -- Process control --
    "kill",
    "sleep",
    "expr",
    "test",
    "true",
    "false",
    // -- Editors / pagers --
    "vi",
    "less",
    "more",
    // -- Networking --
    "ping",
    "wget",
    "nc",
    "ip",
    "ifconfig",
    "route",
    // -- Archives / compression --
    "tar",
    "gzip",
    "gunzip",
    "xz",
    // -- Hex / binary --
    "hexdump",
    "od",
    // -- Terminal --
    "stty",
    "tty",
];

/// Check whether `name_chirho` is a known BusyBox applet.
///
/// Performs a linear scan of [`BUSYBOX_APPLETS_CHIRHO`].  The list is small
/// enough (~90 entries) that this is faster than a hash lookup given the
/// overhead of hashing in a `no_std` kernel environment.
pub fn is_busybox_applet_chirho(name_chirho: &str) -> bool {
    BUSYBOX_APPLETS_CHIRHO
        .iter()
        .any(|applet_chirho| *applet_chirho == name_chirho)
}
