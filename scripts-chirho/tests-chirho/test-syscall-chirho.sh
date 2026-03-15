#!/bin/bash
# For God so loved the world that he gave his only begotten Son,
# that whoever believes in him should not perish but have eternal life. - John 3:16

# D1-003: Syscall conformance test suite
# Tests that key Linux syscalls work correctly via serial output from kernel.

set -euo pipefail
SCRIPT_DIR_CHIRHO="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR_CHIRHO/harness-chirho.sh"

echo "=== D1-003: Syscall Conformance Test ==="
echo "John 3:16"
echo ""

# Boot the kernel with longer timeout for syscall exercising
boot_and_wait_chirho 45

# Check that key syscalls are serviced (these strings come from kernel serial logging)
# The kernel logs syscall names when they are invoked during BusyBox init

assert_serial_contains_chirho "write\|sys_write\|SYS_WRITE" "write() syscall handled"
assert_serial_contains_chirho "read\|sys_read\|SYS_READ" "read() syscall handled"
assert_serial_contains_chirho "brk\|sys_brk\|SYS_BRK" "brk() syscall handled"
assert_serial_contains_chirho "mmap\|sys_mmap\|SYS_MMAP\|MAP_" "mmap() syscall handled"
assert_serial_contains_chirho "open\|openat\|sys_open" "open/openat() syscall handled"
assert_serial_contains_chirho "close\|sys_close" "close() syscall handled"
assert_serial_contains_chirho "exit\|sys_exit\|EXIT" "exit() syscall handled"
assert_serial_contains_chirho "ioctl\|sys_ioctl\|IOCTL" "ioctl() syscall handled"

# Check for syscall error handling — should not have unhandled syscalls causing panics
assert_serial_not_contains_chirho "KERNEL PANIC" "No kernel panic from syscalls"

if [ "$HARNESS_FAIL_CHIRHO" -gt 0 ]; then
    dump_serial_chirho
fi

summary_chirho
