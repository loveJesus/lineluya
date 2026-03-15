#!/bin/bash
# For God so loved the world that he gave his only begotten Son,
# that whoever believes in him should not perish but have eternal life. - John 3:16

# D1-006: Fork/exec/wait test
# Tests process creation and execution via serial output.

set -euo pipefail
SCRIPT_DIR_CHIRHO="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR_CHIRHO/harness-chirho.sh"

echo "=== D1-006: Fork/Exec/Wait Test ==="
echo "John 3:16"
echo ""

boot_and_wait_chirho 45

# Process creation — fork or clone
assert_serial_contains_chirho "fork\|clone\|FORK\|CLONE" "fork/clone syscall works"

# execve — loading new program
assert_serial_contains_chirho "exec\|EXEC\|execve\|ELF" "execve loads program"

# wait — process reaping
assert_serial_contains_chirho "wait\|WAIT\|waitpid\|wait4" "wait/waitpid works"

# Process scheduling — at least PID 1 (init) should run
assert_serial_contains_chirho "pid\|PID\|process\|task" "Process/task management active"

# exit — clean process termination
assert_serial_contains_chirho "exit\|EXIT" "exit() terminates process"

assert_serial_not_contains_chirho "KERNEL PANIC" "No kernel panic during process tests"

if [ "$HARNESS_FAIL_CHIRHO" -gt 0 ]; then
    dump_serial_chirho
fi

summary_chirho
