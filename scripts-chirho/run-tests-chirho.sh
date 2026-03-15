#!/bin/bash
# For God so loved the world that he gave his only begotten Son,
# that whoever believes in him should not perish but have eternal life. - John 3:16

# run-tests-chirho.sh — Master test runner for Lineluya kernel
# Runs all QEMU integration tests in sequence and reports results.
#
# Usage:
#   ./scripts-chirho/run-tests-chirho.sh              # Run all tests
#   ./scripts-chirho/run-tests-chirho.sh boot          # Run only boot test
#   ./scripts-chirho/run-tests-chirho.sh boot syscall   # Run specific tests
#
# Environment variables:
#   KERNEL_BINARY_CHIRHO  - Path to kernel binary (default: release build)
#   QEMU_MEMORY_CHIRHO    - QEMU memory size (default: 256M)
#   VERBOSE_CHIRHO        - Set to 1 for verbose output

set -euo pipefail

PROJECT_ROOT_CHIRHO="$(cd "$(dirname "$0")/.." && pwd)"
TESTS_DIR_CHIRHO="$PROJECT_ROOT_CHIRHO/scripts-chirho/tests-chirho"

# Colors
RED_CHIRHO='\033[0;31m'
GREEN_CHIRHO='\033[0;32m'
YELLOW_CHIRHO='\033[0;33m'
BOLD_CHIRHO='\033[1m'
NC_CHIRHO='\033[0m'

# Results tracking
TOTAL_SUITES_CHIRHO=0
PASSED_SUITES_CHIRHO=0
FAILED_SUITES_CHIRHO=0
SKIPPED_SUITES_CHIRHO=0
FAILED_NAMES_CHIRHO=()

# Export kernel binary path for test harness
export KERNEL_BINARY_CHIRHO="${KERNEL_BINARY_CHIRHO:-${PROJECT_ROOT_CHIRHO}/target/x86_64-unknown-none/release/kernel-chirho}"

echo ""
echo -e "${BOLD_CHIRHO}╔══════════════════════════════════════════════════╗${NC_CHIRHO}"
echo -e "${BOLD_CHIRHO}║       Lineluya Kernel Test Suite                 ║${NC_CHIRHO}"
echo -e "${BOLD_CHIRHO}║  John 3:16 — For God so loved the world...       ║${NC_CHIRHO}"
echo -e "${BOLD_CHIRHO}╚══════════════════════════════════════════════════╝${NC_CHIRHO}"
echo ""

# Check prerequisites
check_prerequisites_chirho() {
    local missing_chirho=0

    if ! command -v qemu-system-x86_64 &>/dev/null; then
        echo -e "${RED_CHIRHO}ERROR: qemu-system-x86_64 not found. Install QEMU.${NC_CHIRHO}"
        echo "  macOS:  brew install qemu"
        echo "  Linux:  apt install qemu-system-x86"
        missing_chirho=1
    fi

    if [ ! -f "$KERNEL_BINARY_CHIRHO" ]; then
        echo -e "${RED_CHIRHO}ERROR: Kernel binary not found at: $KERNEL_BINARY_CHIRHO${NC_CHIRHO}"
        echo "  Build with: make kernel-release-chirho"
        missing_chirho=1
    fi

    if [ "$missing_chirho" -ne 0 ]; then
        echo ""
        echo "Prerequisites not met. Exiting."
        exit 1
    fi

    echo -e "${GREEN_CHIRHO}Prerequisites OK${NC_CHIRHO}"
    echo "  QEMU:   $(qemu-system-x86_64 --version | head -1)"
    echo "  Kernel: $KERNEL_BINARY_CHIRHO ($(du -h "$KERNEL_BINARY_CHIRHO" | cut -f1))"
    echo ""
}

# All available test suites in order
ALL_TESTS_CHIRHO=(
    "boot:test-boot-chirho.sh:D1-002 Boot-to-shell"
    "syscall:test-syscall-chirho.sh:D1-003 Syscall conformance"
    "vfs:test-vfs-chirho.sh:D1-004 VFS/tmpfs/procfs"
    "memory:test-memory-chirho.sh:D1-005 Memory management"
    "fork-exec:test-fork-exec-chirho.sh:D1-006 Fork/exec/wait"
    "signals:test-signals-chirho.sh:D1-007 Signal delivery"
    "pipe:test-pipe-chirho.sh:D1-008 Pipe and IPC"
    "network:test-network-chirho.sh:D1-009 Network loopback"
    "ext4:test-ext4-chirho.sh:D1-010 ext4 disk image"
)

# Determine which tests to run
determine_tests_chirho() {
    local requested_chirho=("$@")

    if [ ${#requested_chirho[@]} -eq 0 ]; then
        # Run all tests
        TESTS_TO_RUN_CHIRHO=("${ALL_TESTS_CHIRHO[@]}")
    else
        TESTS_TO_RUN_CHIRHO=()
        for req_chirho in "${requested_chirho[@]}"; do
            local found_chirho=false
            for test_chirho in "${ALL_TESTS_CHIRHO[@]}"; do
                local shortname_chirho="${test_chirho%%:*}"
                if [ "$shortname_chirho" = "$req_chirho" ]; then
                    TESTS_TO_RUN_CHIRHO+=("$test_chirho")
                    found_chirho=true
                    break
                fi
            done
            if [ "$found_chirho" = false ]; then
                echo -e "${YELLOW_CHIRHO}WARNING: Unknown test '$req_chirho' — skipping${NC_CHIRHO}"
            fi
        done
    fi
}

# Run a single test suite
run_test_suite_chirho() {
    local entry_chirho="$1"
    local shortname_chirho="${entry_chirho%%:*}"
    local rest_chirho="${entry_chirho#*:}"
    local script_chirho="${rest_chirho%%:*}"
    local description_chirho="${rest_chirho#*:}"
    local script_path_chirho="$TESTS_DIR_CHIRHO/$script_chirho"

    TOTAL_SUITES_CHIRHO=$((TOTAL_SUITES_CHIRHO + 1))

    echo -e "${BOLD_CHIRHO}--- [$TOTAL_SUITES_CHIRHO] $description_chirho ---${NC_CHIRHO}"

    if [ ! -f "$script_path_chirho" ]; then
        echo -e "${YELLOW_CHIRHO}  [SKIP] Script not found: $script_path_chirho${NC_CHIRHO}"
        SKIPPED_SUITES_CHIRHO=$((SKIPPED_SUITES_CHIRHO + 1))
        return 0
    fi

    local start_time_chirho
    start_time_chirho=$(date +%s)

    if bash "$script_path_chirho"; then
        PASSED_SUITES_CHIRHO=$((PASSED_SUITES_CHIRHO + 1))
        local end_time_chirho
        end_time_chirho=$(date +%s)
        local duration_chirho=$((end_time_chirho - start_time_chirho))
        echo -e "${GREEN_CHIRHO}  Suite PASSED (${duration_chirho}s)${NC_CHIRHO}"
    else
        FAILED_SUITES_CHIRHO=$((FAILED_SUITES_CHIRHO + 1))
        FAILED_NAMES_CHIRHO+=("$description_chirho")
        local end_time_chirho
        end_time_chirho=$(date +%s)
        local duration_chirho=$((end_time_chirho - start_time_chirho))
        echo -e "${RED_CHIRHO}  Suite FAILED (${duration_chirho}s)${NC_CHIRHO}"
    fi
    echo ""
}

# Print final summary
print_summary_chirho() {
    echo ""
    echo -e "${BOLD_CHIRHO}╔══════════════════════════════════════════════════╗${NC_CHIRHO}"
    echo -e "${BOLD_CHIRHO}║                 TEST SUMMARY                     ║${NC_CHIRHO}"
    echo -e "${BOLD_CHIRHO}╚══════════════════════════════════════════════════╝${NC_CHIRHO}"
    echo ""
    echo "  Total suites:   $TOTAL_SUITES_CHIRHO"
    echo -e "  Passed:         ${GREEN_CHIRHO}$PASSED_SUITES_CHIRHO${NC_CHIRHO}"
    echo -e "  Failed:         ${RED_CHIRHO}$FAILED_SUITES_CHIRHO${NC_CHIRHO}"
    echo -e "  Skipped:        ${YELLOW_CHIRHO}$SKIPPED_SUITES_CHIRHO${NC_CHIRHO}"

    if [ ${#FAILED_NAMES_CHIRHO[@]} -gt 0 ]; then
        echo ""
        echo -e "${RED_CHIRHO}Failed suites:${NC_CHIRHO}"
        for name_chirho in "${FAILED_NAMES_CHIRHO[@]}"; do
            echo -e "  ${RED_CHIRHO}- $name_chirho${NC_CHIRHO}"
        done
    fi

    echo ""
    if [ "$FAILED_SUITES_CHIRHO" -gt 0 ]; then
        echo -e "${RED_CHIRHO}${BOLD_CHIRHO}RESULT: FAILED${NC_CHIRHO}"
        return 1
    else
        echo -e "${GREEN_CHIRHO}${BOLD_CHIRHO}RESULT: ALL PASSED — Hallelujah!${NC_CHIRHO}"
        return 0
    fi
}

# --- Main ---
check_prerequisites_chirho
determine_tests_chirho "$@"

echo "Running ${#TESTS_TO_RUN_CHIRHO[@]} test suite(s)..."
echo ""

for test_entry_chirho in "${TESTS_TO_RUN_CHIRHO[@]}"; do
    run_test_suite_chirho "$test_entry_chirho"
done

print_summary_chirho
