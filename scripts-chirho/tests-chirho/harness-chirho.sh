#!/bin/bash
# For God so loved the world that he gave his only begotten Son,
# that whoever believes in him should not perish but have eternal life. - John 3:16

# harness-chirho.sh — QEMU integration test harness for Lineluya kernel
# D1-001: Boots kernel in QEMU, captures serial output, checks for expected strings.
#
# Usage:
#   source harness-chirho.sh
#   boot_qemu_chirho [timeout_seconds] [extra_qemu_args...]
#   assert_serial_contains_chirho "expected string"
#   cleanup_qemu_chirho

set -euo pipefail

# --- Configuration ---
PROJECT_ROOT_CHIRHO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
KERNEL_BINARY_CHIRHO="${KERNEL_BINARY_CHIRHO:-${PROJECT_ROOT_CHIRHO}/target/x86_64-unknown-none/release/kernel-chirho}"
SERIAL_LOG_CHIRHO="$(mktemp /tmp/lineluya-serial-chirho.XXXXXX)"
QEMU_PID_CHIRHO=""
QEMU_MEMORY_CHIRHO="${QEMU_MEMORY_CHIRHO:-256M}"
DEFAULT_TIMEOUT_CHIRHO=30

# Counters
HARNESS_PASS_CHIRHO=0
HARNESS_FAIL_CHIRHO=0
HARNESS_TOTAL_CHIRHO=0
CURRENT_TEST_NAME_CHIRHO=""

# --- Colors ---
RED_CHIRHO='\033[0;31m'
GREEN_CHIRHO='\033[0;32m'
YELLOW_CHIRHO='\033[0;33m'
NC_CHIRHO='\033[0m' # No Color

# --- Functions ---

log_info_chirho() {
    echo -e "${YELLOW_CHIRHO}[HARNESS]${NC_CHIRHO} $*"
}

log_pass_chirho() {
    echo -e "${GREEN_CHIRHO}  [PASS]${NC_CHIRHO} $*"
    HARNESS_PASS_CHIRHO=$((HARNESS_PASS_CHIRHO + 1))
    HARNESS_TOTAL_CHIRHO=$((HARNESS_TOTAL_CHIRHO + 1))
}

log_fail_chirho() {
    echo -e "${RED_CHIRHO}  [FAIL]${NC_CHIRHO} $*"
    HARNESS_FAIL_CHIRHO=$((HARNESS_FAIL_CHIRHO + 1))
    HARNESS_TOTAL_CHIRHO=$((HARNESS_TOTAL_CHIRHO + 1))
}

# Check that the kernel binary exists
check_kernel_chirho() {
    if [ ! -f "$KERNEL_BINARY_CHIRHO" ]; then
        echo "ERROR: Kernel binary not found at: $KERNEL_BINARY_CHIRHO"
        echo "Build with: make kernel-release-chirho"
        exit 1
    fi
    log_info_chirho "Kernel: $KERNEL_BINARY_CHIRHO ($(du -h "$KERNEL_BINARY_CHIRHO" | cut -f1))"
}

# Boot QEMU with the kernel, capturing serial output to a file.
# Args: [timeout_seconds] [extra_qemu_args...]
boot_qemu_chirho() {
    local timeout_chirho="${1:-$DEFAULT_TIMEOUT_CHIRHO}"
    shift 2>/dev/null || true

    check_kernel_chirho

    # Clear previous log
    > "$SERIAL_LOG_CHIRHO"

    log_info_chirho "Booting QEMU (timeout=${timeout_chirho}s, mem=${QEMU_MEMORY_CHIRHO})..."

    # Start QEMU in background
    timeout "$timeout_chirho" qemu-system-x86_64 \
        -kernel "$KERNEL_BINARY_CHIRHO" \
        -serial file:"$SERIAL_LOG_CHIRHO" \
        -display none \
        -m "$QEMU_MEMORY_CHIRHO" \
        -no-reboot \
        -device isa-debug-exit,iobase=0xf4,iosize=0x04 \
        "$@" &
    QEMU_PID_CHIRHO=$!

    log_info_chirho "QEMU PID: $QEMU_PID_CHIRHO"
}

# Wait for QEMU to finish (or timeout)
wait_qemu_chirho() {
    if [ -n "$QEMU_PID_CHIRHO" ]; then
        wait "$QEMU_PID_CHIRHO" 2>/dev/null || true
        QEMU_PID_CHIRHO=""
    fi
}

# Boot QEMU and wait for it to finish (convenience wrapper)
boot_and_wait_chirho() {
    boot_qemu_chirho "$@"
    wait_qemu_chirho
    log_info_chirho "Serial output: $(wc -l < "$SERIAL_LOG_CHIRHO") lines captured"
}

# Assert that the serial log contains a given string
assert_serial_contains_chirho() {
    local expected_chirho="$1"
    local label_chirho="${2:-$expected_chirho}"

    if grep -q "$expected_chirho" "$SERIAL_LOG_CHIRHO" 2>/dev/null; then
        log_pass_chirho "$label_chirho"
    else
        log_fail_chirho "$label_chirho (expected '$expected_chirho' not found in serial output)"
    fi
}

# Assert that the serial log does NOT contain a given string
assert_serial_not_contains_chirho() {
    local unexpected_chirho="$1"
    local label_chirho="${2:-must not contain '$unexpected_chirho'}"

    if grep -q "$unexpected_chirho" "$SERIAL_LOG_CHIRHO" 2>/dev/null; then
        log_fail_chirho "$label_chirho (unexpected '$unexpected_chirho' found in serial output)"
    else
        log_pass_chirho "$label_chirho"
    fi
}

# Assert serial log has at least N lines
assert_serial_min_lines_chirho() {
    local min_lines_chirho="$1"
    local label_chirho="${2:-Serial output >= $min_lines_chirho lines}"
    local actual_lines_chirho
    actual_lines_chirho=$(wc -l < "$SERIAL_LOG_CHIRHO")

    if [ "$actual_lines_chirho" -ge "$min_lines_chirho" ]; then
        log_pass_chirho "$label_chirho ($actual_lines_chirho lines)"
    else
        log_fail_chirho "$label_chirho (only $actual_lines_chirho lines, expected >= $min_lines_chirho)"
    fi
}

# Get the serial log contents
get_serial_log_chirho() {
    cat "$SERIAL_LOG_CHIRHO"
}

# Print the serial log (for debugging)
dump_serial_chirho() {
    log_info_chirho "--- Serial Output Start ---"
    cat "$SERIAL_LOG_CHIRHO" || true
    log_info_chirho "--- Serial Output End ---"
}

# Cleanup: kill QEMU if still running, remove temp files
cleanup_qemu_chirho() {
    if [ -n "$QEMU_PID_CHIRHO" ]; then
        kill "$QEMU_PID_CHIRHO" 2>/dev/null || true
        wait "$QEMU_PID_CHIRHO" 2>/dev/null || true
        QEMU_PID_CHIRHO=""
    fi
    rm -f "$SERIAL_LOG_CHIRHO"
}

# Print test summary and return exit code
summary_chirho() {
    echo ""
    echo "============================================"
    echo "  Test Results: $HARNESS_PASS_CHIRHO passed, $HARNESS_FAIL_CHIRHO failed (of $HARNESS_TOTAL_CHIRHO)"
    echo "============================================"

    if [ "$HARNESS_FAIL_CHIRHO" -gt 0 ]; then
        echo -e "${RED_CHIRHO}SOME TESTS FAILED${NC_CHIRHO}"
        return 1
    else
        echo -e "${GREEN_CHIRHO}ALL TESTS PASSED — Hallelujah!${NC_CHIRHO}"
        return 0
    fi
}

# Set a trap for cleanup on exit
trap cleanup_qemu_chirho EXIT
