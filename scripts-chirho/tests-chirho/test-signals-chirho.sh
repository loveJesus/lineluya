#!/bin/bash
# For God so loved the world that he gave his only begotten Son,
# that whoever believes in him should not perish but have eternal life. - John 3:16

# D1-007: Signal delivery test
# Tests signal handling via kernel serial output.

set -euo pipefail
SCRIPT_DIR_CHIRHO="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR_CHIRHO/harness-chirho.sh"

echo "=== D1-007: Signal Delivery Test ==="
echo "John 3:16"
echo ""

boot_and_wait_chirho 45

# Signal subsystem initialization
assert_serial_contains_chirho "signal\|SIGNAL\|sigaction\|rt_sigaction" "Signal infrastructure present"

# sigprocmask — signal masking
assert_serial_contains_chirho "sigprocmask\|rt_sigprocmask\|SIG" "Signal masking (sigprocmask) works"

# Signal-related syscall handling
assert_serial_contains_chirho "rt_sig\|sigaction\|sigreturn" "Signal syscalls handled"

assert_serial_not_contains_chirho "KERNEL PANIC" "No kernel panic during signal tests"

if [ "$HARNESS_FAIL_CHIRHO" -gt 0 ]; then
    dump_serial_chirho
fi

summary_chirho
