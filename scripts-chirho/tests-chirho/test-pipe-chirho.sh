#!/bin/bash
# For God so loved the world that he gave his only begotten Son,
# that whoever believes in him should not perish but have eternal life. - John 3:16

# D1-008: Pipe and IPC test
# Tests pipe creation and inter-process communication via serial output.

set -euo pipefail
SCRIPT_DIR_CHIRHO="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR_CHIRHO/harness-chirho.sh"

echo "=== D1-008: Pipe and IPC Test ==="
echo "John 3:16"
echo ""

boot_and_wait_chirho 45

# pipe/pipe2 syscall
assert_serial_contains_chirho "pipe\|PIPE\|pipe2" "pipe() syscall present"

# File descriptor operations needed for piping
assert_serial_contains_chirho "dup\|DUP\|dup2\|F_DUPFD" "dup/dup2 for pipe redirection"

# read/write on pipe fds
assert_serial_contains_chirho "read\|write" "read/write on pipe file descriptors"

assert_serial_not_contains_chirho "KERNEL PANIC" "No kernel panic during pipe tests"

if [ "$HARNESS_FAIL_CHIRHO" -gt 0 ]; then
    dump_serial_chirho
fi

summary_chirho
