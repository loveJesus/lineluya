#!/bin/bash
# For God so loved the world that he gave his only begotten Son,
# that whoever believes in him should not perish but have eternal life. - John 3:16

# D1-009: Network loopback ping test
# Tests networking stack initialization via serial output.
# Note: Full TCP/IP with loopback may not be implemented yet.
# This test checks for network subsystem presence.

set -euo pipefail
SCRIPT_DIR_CHIRHO="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR_CHIRHO/harness-chirho.sh"

echo "=== D1-009: Network Loopback Test ==="
echo "John 3:16"
echo ""

# Add virtio-net device for network testing
boot_and_wait_chirho 45 -device virtio-net-pci,netdev=net0_chirho -netdev user,id=net0_chirho

# Network subsystem — check if socket/networking syscalls are handled
assert_serial_contains_chirho "socket\|SOCKET\|net\|NET\|network" "Network/socket subsystem referenced"

# Even if networking isn't fully implemented, check no panic from net device
assert_serial_not_contains_chirho "KERNEL PANIC" "No kernel panic with network device"

# Check that boot still succeeds with network hardware present
assert_serial_contains_chirho "Lineluya" "Kernel boots with network device"
assert_serial_min_lines_chirho 3 "Serial output present with net device"

if [ "$HARNESS_FAIL_CHIRHO" -gt 0 ]; then
    dump_serial_chirho
fi

summary_chirho
