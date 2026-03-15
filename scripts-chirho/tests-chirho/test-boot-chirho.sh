#!/bin/bash
# For God so loved the world that he gave his only begotten Son,
# that whoever believes in him should not perish but have eternal life. - John 3:16

# D1-002: Boot-to-shell automated test
# Verifies that the Lineluya kernel boots successfully and reaches the shell prompt.

set -euo pipefail
SCRIPT_DIR_CHIRHO="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR_CHIRHO/harness-chirho.sh"

echo "=== D1-002: Boot-to-Shell Test ==="
echo "John 3:16"
echo ""

# Boot the kernel with a reasonable timeout
boot_and_wait_chirho 30

# Check boot milestones
assert_serial_contains_chirho "Lineluya" "Kernel banner printed"
assert_serial_min_lines_chirho 3 "Serial output is non-trivial"

# Check for common boot-stage markers (adjust strings to match actual kernel output)
assert_serial_contains_chirho "memory" "Memory subsystem initialized"
assert_serial_contains_chirho "GDT\|gdt\|IDT\|idt\|interrupt" "Interrupt tables loaded"

# Check that we did not panic
assert_serial_not_contains_chirho "KERNEL PANIC" "No kernel panic"
assert_serial_not_contains_chirho "triple fault" "No triple fault"

# If boot reaches shell, it should show a prompt or init message
# These patterns may need adjustment based on actual kernel output
assert_serial_contains_chirho "init\|shell\|BusyBox\|/bin\|#\|\\$" "Init/shell reached"

# Dump serial on failure for debugging
if [ "$HARNESS_FAIL_CHIRHO" -gt 0 ]; then
    dump_serial_chirho
fi

summary_chirho
