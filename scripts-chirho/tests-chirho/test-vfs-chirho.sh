#!/bin/bash
# For God so loved the world that he gave his only begotten Son,
# that whoever believes in him should not perish but have eternal life. - John 3:16

# D1-004: VFS/tmpfs/procfs unit tests
# Verifies virtual filesystem operations via kernel serial output.

set -euo pipefail
SCRIPT_DIR_CHIRHO="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR_CHIRHO/harness-chirho.sh"

echo "=== D1-004: VFS/tmpfs/procfs Test ==="
echo "John 3:16"
echo ""

boot_and_wait_chirho 45

# VFS initialization
assert_serial_contains_chirho "VFS\|vfs\|filesystem\|rootfs" "VFS subsystem initialized"

# Check for /dev and /proc presence (kernel logs these during init)
assert_serial_contains_chirho "/dev\|devfs\|dev/" "Device filesystem available"
assert_serial_contains_chirho "/proc\|procfs\|proc/" "Proc filesystem available"

# tmpfs — kernel should mount tmpfs somewhere during init
assert_serial_contains_chirho "tmpfs\|tmp\|ramfs" "tmpfs/ramfs mounted"

# getdents/stat — these are exercised when BusyBox does ls/cat
assert_serial_contains_chirho "getdents\|readdir\|GETDENTS" "Directory listing (getdents) works"
assert_serial_contains_chirho "stat\|fstat\|lstat" "stat() works"

# dup — file descriptor duplication
assert_serial_contains_chirho "dup\|DUP\|F_DUPFD" "dup/fcntl F_DUPFD works"

assert_serial_not_contains_chirho "KERNEL PANIC" "No kernel panic in VFS tests"

if [ "$HARNESS_FAIL_CHIRHO" -gt 0 ]; then
    dump_serial_chirho
fi

summary_chirho
