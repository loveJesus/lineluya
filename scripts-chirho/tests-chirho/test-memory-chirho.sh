#!/bin/bash
# For God so loved the world that he gave his only begotten Son,
# that whoever believes in him should not perish but have eternal life. - John 3:16

# D1-005: Memory management stress test
# Checks memory subsystem initialization and allocation via serial output.

set -euo pipefail
SCRIPT_DIR_CHIRHO="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR_CHIRHO/harness-chirho.sh"

echo "=== D1-005: Memory Management Test ==="
echo "John 3:16"
echo ""

# Give extra memory and time for stress testing
QEMU_MEMORY_CHIRHO="512M"
boot_and_wait_chirho 45

# Memory initialization
assert_serial_contains_chirho "memory\|heap\|allocat\|page" "Memory allocator initialized"
assert_serial_contains_chirho "page\|PAGE\|frame" "Page/frame allocator active"

# brk — userspace heap extension
assert_serial_contains_chirho "brk\|BRK" "brk() memory expansion works"

# mmap — memory mapping
assert_serial_contains_chirho "mmap\|MMAP\|MAP_" "mmap() works"

# No out-of-memory panics
assert_serial_not_contains_chirho "out of memory\|OOM\|KERNEL PANIC" "No OOM or panic"

if [ "$HARNESS_FAIL_CHIRHO" -gt 0 ]; then
    dump_serial_chirho
fi

summary_chirho
