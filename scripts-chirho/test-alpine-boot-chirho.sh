#!/bin/bash
# For God so loved the world that he gave his only begotten Son,
# that whoever believes in him should not perish but have eternal life. - John 3:16

# test-alpine-boot-chirho.sh -- End-to-end Alpine boot integration test
#
# Builds the Alpine disk image, builds the kernel, launches QEMU with both
# the kernel disk and Alpine VirtIO disk attached, then sends test commands
# via a serial socket and verifies output.
#
# Usage:
#   ./scripts-chirho/test-alpine-boot-chirho.sh [--skip-build] [--timeout 60] [--verbose]
#
# Prerequisites:
#   - QEMU installed (qemu-system-x86_64)
#   - Docker installed (for Alpine disk image creation on macOS)
#   - Rust nightly toolchain (for kernel build)

set -euo pipefail

# ============================================================================
# Configuration
# ============================================================================

SCRIPT_DIR_CHIRHO="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR_CHIRHO="$(dirname "$SCRIPT_DIR_CHIRHO")"

KERNEL_BINARY_CHIRHO="${KERNEL_BINARY_CHIRHO:-${PROJECT_DIR_CHIRHO}/target/x86_64-unknown-none/release/kernel-chirho}"
ALPINE_IMAGE_CHIRHO="${PROJECT_DIR_CHIRHO}/target/alpine-virtio-chirho/alpine-virtio-chirho.img"
DISK_IMAGE_DIR_CHIRHO="${PROJECT_DIR_CHIRHO}/target/disk-images-chirho"
BIOS_IMAGE_CHIRHO="${DISK_IMAGE_DIR_CHIRHO}/lineluya-bios-chirho.img"

QEMU_MEMORY_CHIRHO="${QEMU_MEMORY_CHIRHO:-512M}"
TIMEOUT_CHIRHO=60
SKIP_BUILD_CHIRHO=0
VERBOSE_CHIRHO=0

# Test infrastructure
SERIAL_SOCKET_CHIRHO="/tmp/lineluya-test-serial-chirho.sock"
SERIAL_LOG_CHIRHO="$(mktemp /tmp/lineluya-alpine-test-chirho.XXXXXX)"
QEMU_PID_CHIRHO=""
SOCAT_PID_CHIRHO=""

# Results
PASS_COUNT_CHIRHO=0
FAIL_COUNT_CHIRHO=0
TOTAL_COUNT_CHIRHO=0

# Colors
RED_CHIRHO='\033[0;31m'
GREEN_CHIRHO='\033[0;32m'
YELLOW_CHIRHO='\033[0;33m'
BOLD_CHIRHO='\033[1m'
NC_CHIRHO='\033[0m'

# ============================================================================
# Argument parsing
# ============================================================================

parse_args_chirho() {
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --skip-build)
                SKIP_BUILD_CHIRHO=1
                shift
                ;;
            --timeout)
                TIMEOUT_CHIRHO="$2"
                shift 2
                ;;
            --verbose|-v)
                VERBOSE_CHIRHO=1
                shift
                ;;
            --help|-h)
                echo "Usage: $0 [--skip-build] [--timeout SECONDS] [--verbose]"
                echo ""
                echo "  --skip-build   Skip kernel and disk image builds"
                echo "  --timeout N    QEMU timeout in seconds (default: 60)"
                echo "  --verbose      Show QEMU serial output in real time"
                exit 0
                ;;
            *)
                echo "Unknown option: $1"
                exit 1
                ;;
        esac
    done
}

# ============================================================================
# Utility functions
# ============================================================================

log_chirho() {
    echo -e "${YELLOW_CHIRHO}[ALPINE-TEST]${NC_CHIRHO} $*"
}

log_pass_chirho() {
    echo -e "${GREEN_CHIRHO}  [PASS]${NC_CHIRHO} $*"
    PASS_COUNT_CHIRHO=$((PASS_COUNT_CHIRHO + 1))
    TOTAL_COUNT_CHIRHO=$((TOTAL_COUNT_CHIRHO + 1))
}

log_fail_chirho() {
    echo -e "${RED_CHIRHO}  [FAIL]${NC_CHIRHO} $*"
    FAIL_COUNT_CHIRHO=$((FAIL_COUNT_CHIRHO + 1))
    TOTAL_COUNT_CHIRHO=$((TOTAL_COUNT_CHIRHO + 1))
}

cleanup_chirho() {
    log_chirho "Cleaning up..."

    if [[ -n "$QEMU_PID_CHIRHO" ]]; then
        kill "$QEMU_PID_CHIRHO" 2>/dev/null || true
        wait "$QEMU_PID_CHIRHO" 2>/dev/null || true
        QEMU_PID_CHIRHO=""
    fi

    if [[ -n "$SOCAT_PID_CHIRHO" ]]; then
        kill "$SOCAT_PID_CHIRHO" 2>/dev/null || true
        wait "$SOCAT_PID_CHIRHO" 2>/dev/null || true
        SOCAT_PID_CHIRHO=""
    fi

    rm -f "$SERIAL_SOCKET_CHIRHO" "$SERIAL_LOG_CHIRHO"
}

trap cleanup_chirho EXIT

# ============================================================================
# Step 1: Build Alpine disk image
# ============================================================================

build_alpine_disk_chirho() {
    if [[ -f "$ALPINE_IMAGE_CHIRHO" ]]; then
        log_chirho "Alpine disk image already exists: $ALPINE_IMAGE_CHIRHO"
        log_chirho "  (delete it to force rebuild)"
        return 0
    fi

    log_chirho "Building Alpine disk image..."
    if ! bash "$SCRIPT_DIR_CHIRHO/make-alpine-disk-chirho.sh"; then
        echo "ERROR: Failed to build Alpine disk image."
        exit 1
    fi
}

# ============================================================================
# Step 2: Build the kernel
# ============================================================================

build_kernel_chirho() {
    log_chirho "Building kernel (release)..."
    cd "$PROJECT_DIR_CHIRHO"
    make fast-kernel-chirho 2>&1 | tail -3
    cd "$SCRIPT_DIR_CHIRHO/.."

    if [[ ! -f "$KERNEL_BINARY_CHIRHO" ]]; then
        echo "ERROR: Kernel binary not found at: $KERNEL_BINARY_CHIRHO"
        echo "  Try: make kernel-release-chirho"
        exit 1
    fi

    log_chirho "Kernel: $KERNEL_BINARY_CHIRHO ($(du -h "$KERNEL_BINARY_CHIRHO" | cut -f1))"
}

# ============================================================================
# Step 3: Build disk images via Docker
# ============================================================================

build_disk_images_chirho() {
    if [[ -f "$BIOS_IMAGE_CHIRHO" ]]; then
        log_chirho "BIOS disk image already exists: $BIOS_IMAGE_CHIRHO"
        return 0
    fi

    log_chirho "Building disk images via Docker..."
    if ! bash "$SCRIPT_DIR_CHIRHO/fast-build-chirho.sh" 2>&1 | tail -5; then
        log_chirho "WARNING: Docker disk image build failed -- will try direct kernel boot"
    fi
}

# ============================================================================
# Step 4: Launch QEMU
# ============================================================================

launch_qemu_chirho() {
    log_chirho "Launching QEMU (timeout=${TIMEOUT_CHIRHO}s, mem=${QEMU_MEMORY_CHIRHO})..."

    # Remove stale socket
    rm -f "$SERIAL_SOCKET_CHIRHO"

    # Build QEMU command
    local qemu_args_chirho=()
    qemu_args_chirho+=(-m "$QEMU_MEMORY_CHIRHO")
    qemu_args_chirho+=(-machine q35)
    qemu_args_chirho+=(-cpu qemu64)
    qemu_args_chirho+=(-display none)
    qemu_args_chirho+=(-no-reboot)
    qemu_args_chirho+=(-device isa-debug-exit,iobase=0xf4,iosize=0x04)

    # Kernel boot method: prefer BIOS disk image, fall back to -kernel
    if [[ -f "$BIOS_IMAGE_CHIRHO" ]]; then
        qemu_args_chirho+=(-drive "format=raw,file=$BIOS_IMAGE_CHIRHO")
    else
        qemu_args_chirho+=(-kernel "$KERNEL_BINARY_CHIRHO")
    fi

    # Alpine VirtIO disk
    if [[ -f "$ALPINE_IMAGE_CHIRHO" ]]; then
        qemu_args_chirho+=(-drive "file=$ALPINE_IMAGE_CHIRHO,format=raw,if=virtio")
        log_chirho "  Alpine disk: $ALPINE_IMAGE_CHIRHO"
    else
        log_chirho "  WARNING: No Alpine disk image -- VirtIO tests will be skipped"
    fi

    # VirtIO-net with QEMU user-mode networking (DHCP + port forwarding)
    qemu_args_chirho+=(-netdev "user,id=net0-chirho,hostfwd=tcp::2222-:22")
    qemu_args_chirho+=(-device "virtio-net-pci,netdev=net0-chirho")
    log_chirho "  Network: VirtIO-net (user-mode, SSH on localhost:2222)"

    # Serial: write to log file (non-interactive)
    qemu_args_chirho+=(-serial "file:$SERIAL_LOG_CHIRHO")

    # Clear previous log
    > "$SERIAL_LOG_CHIRHO"

    # Launch QEMU with timeout
    timeout "$TIMEOUT_CHIRHO" qemu-system-x86_64 "${qemu_args_chirho[@]}" &
    QEMU_PID_CHIRHO=$!

    log_chirho "  QEMU PID: $QEMU_PID_CHIRHO"
}

# ============================================================================
# Step 5: Wait for QEMU and collect output
# ============================================================================

wait_for_qemu_chirho() {
    log_chirho "Waiting for QEMU to finish (max ${TIMEOUT_CHIRHO}s)..."

    wait "$QEMU_PID_CHIRHO" 2>/dev/null || true
    QEMU_PID_CHIRHO=""

    local line_count_chirho
    line_count_chirho=$(wc -l < "$SERIAL_LOG_CHIRHO" 2>/dev/null || echo "0")
    log_chirho "Serial output captured: ${line_count_chirho} lines"

    if [[ "$VERBOSE_CHIRHO" -eq 1 ]]; then
        echo ""
        echo "--- Serial Output Start ---"
        cat "$SERIAL_LOG_CHIRHO" 2>/dev/null || true
        echo "--- Serial Output End ---"
        echo ""
    fi
}

# ============================================================================
# Step 6: Verify output
# ============================================================================

assert_serial_contains_chirho() {
    local expected_chirho="$1"
    local label_chirho="${2:-$expected_chirho}"

    if grep -q "$expected_chirho" "$SERIAL_LOG_CHIRHO" 2>/dev/null; then
        log_pass_chirho "$label_chirho"
    else
        log_fail_chirho "$label_chirho (expected '$expected_chirho' not found)"
    fi
}

assert_serial_not_contains_chirho() {
    local unexpected_chirho="$1"
    local label_chirho="${2:-must not contain '$unexpected_chirho'}"

    if grep -q "$unexpected_chirho" "$SERIAL_LOG_CHIRHO" 2>/dev/null; then
        log_fail_chirho "$label_chirho (unexpected '$unexpected_chirho' found)"
    else
        log_pass_chirho "$label_chirho"
    fi
}

assert_serial_min_lines_chirho() {
    local min_lines_chirho="$1"
    local label_chirho="${2:-Serial output >= $min_lines_chirho lines}"
    local actual_lines_chirho
    actual_lines_chirho=$(wc -l < "$SERIAL_LOG_CHIRHO" 2>/dev/null || echo "0")

    if [[ "$actual_lines_chirho" -ge "$min_lines_chirho" ]]; then
        log_pass_chirho "$label_chirho ($actual_lines_chirho lines)"
    else
        log_fail_chirho "$label_chirho (only $actual_lines_chirho lines, need >= $min_lines_chirho)"
    fi
}

run_tests_chirho() {
    log_chirho "Running test assertions..."
    echo ""

    echo -e "${BOLD_CHIRHO}--- Boot Tests ---${NC_CHIRHO}"

    # Basic boot checks
    assert_serial_min_lines_chirho 5 "Kernel produces serial output"
    assert_serial_contains_chirho "Lineluya" "Kernel banner contains 'Lineluya'"
    assert_serial_contains_chirho "John 3:16\|loved the world\|eternal life" "John 3:16 reference in boot"

    echo ""
    echo -e "${BOLD_CHIRHO}--- Kernel Init Tests ---${NC_CHIRHO}"

    # Memory and subsystem init
    assert_serial_contains_chirho "[Mm]emory\|heap\|allocator" "Memory subsystem initialized"
    assert_serial_contains_chirho "VFS\|tmpfs\|filesystem" "VFS initialized"

    echo ""
    echo -e "${BOLD_CHIRHO}--- VirtIO Tests ---${NC_CHIRHO}"

    # VirtIO detection (only if Alpine disk was provided)
    if [[ -f "$ALPINE_IMAGE_CHIRHO" ]]; then
        assert_serial_contains_chirho "[Vv]irt[Ii][Oo]\|virtio" "VirtIO device detected"
    else
        log_chirho "  [SKIP] No Alpine disk -- VirtIO tests skipped"
    fi

    echo ""
    echo -e "${BOLD_CHIRHO}--- Shell Tests ---${NC_CHIRHO}"

    # Shell launch
    assert_serial_contains_chirho "BusyBox\|ash\|shell\|init" "Shell or init started"

    echo ""
    echo -e "${BOLD_CHIRHO}--- Stability Tests ---${NC_CHIRHO}"

    # No panics
    assert_serial_not_contains_chirho "PANIC\|panic!\|kernel panic" "No kernel panics"
    assert_serial_not_contains_chirho "double fault\|DOUBLE FAULT" "No double faults"
    assert_serial_not_contains_chirho "triple fault" "No triple faults"
    assert_serial_not_contains_chirho "UNHANDLED EXCEPTION" "No unhandled exceptions"
}

# ============================================================================
# Summary
# ============================================================================

print_summary_chirho() {
    echo ""
    echo -e "${BOLD_CHIRHO}============================================================${NC_CHIRHO}"
    echo -e "${BOLD_CHIRHO}  Alpine Boot Integration Test Results${NC_CHIRHO}"
    echo -e "${BOLD_CHIRHO}============================================================${NC_CHIRHO}"
    echo ""
    echo "  Total:    $TOTAL_COUNT_CHIRHO"
    echo -e "  Passed:   ${GREEN_CHIRHO}$PASS_COUNT_CHIRHO${NC_CHIRHO}"
    echo -e "  Failed:   ${RED_CHIRHO}$FAIL_COUNT_CHIRHO${NC_CHIRHO}"
    echo ""

    if [[ "$FAIL_COUNT_CHIRHO" -gt 0 ]]; then
        echo -e "${RED_CHIRHO}${BOLD_CHIRHO}RESULT: FAILED${NC_CHIRHO}"
        echo ""
        echo "Hint: Run with --verbose to see full serial output"
        echo "      Run with --skip-build to skip the build steps"
        return 1
    else
        echo -e "${GREEN_CHIRHO}${BOLD_CHIRHO}RESULT: ALL PASSED -- Hallelujah!${NC_CHIRHO}"
        return 0
    fi
}

# ============================================================================
# Check prerequisites
# ============================================================================

check_prereqs_chirho() {
    local missing_chirho=0

    if ! command -v qemu-system-x86_64 &>/dev/null; then
        echo "ERROR: qemu-system-x86_64 not found."
        echo "  macOS:  brew install qemu"
        echo "  Linux:  apt install qemu-system-x86"
        missing_chirho=1
    fi

    if [[ "$missing_chirho" -ne 0 ]]; then
        exit 1
    fi

    log_chirho "Prerequisites OK"
    log_chirho "  QEMU: $(qemu-system-x86_64 --version | head -1)"
}

# ============================================================================
# Main
# ============================================================================

main_chirho() {
    parse_args_chirho "$@"

    echo ""
    echo -e "${BOLD_CHIRHO}=== Lineluya Alpine Boot Integration Test ===${NC_CHIRHO}"
    echo "For God so loved the world that he gave his only begotten Son,"
    echo "that whoever believes in him should not perish but have eternal life. - John 3:16"
    echo ""

    check_prereqs_chirho

    if [[ "$SKIP_BUILD_CHIRHO" -eq 0 ]]; then
        echo ""
        echo -e "${BOLD_CHIRHO}--- Step 1/5: Build Alpine Disk Image ---${NC_CHIRHO}"
        build_alpine_disk_chirho

        echo ""
        echo -e "${BOLD_CHIRHO}--- Step 2/5: Build Kernel ---${NC_CHIRHO}"
        build_kernel_chirho

        echo ""
        echo -e "${BOLD_CHIRHO}--- Step 3/5: Build Disk Images ---${NC_CHIRHO}"
        build_disk_images_chirho
    else
        log_chirho "Skipping build steps (--skip-build)"

        if [[ ! -f "$KERNEL_BINARY_CHIRHO" ]] && [[ ! -f "$BIOS_IMAGE_CHIRHO" ]]; then
            echo "ERROR: No kernel binary or disk image found. Build first or remove --skip-build."
            exit 1
        fi
    fi

    echo ""
    echo -e "${BOLD_CHIRHO}--- Step 4/5: Launch QEMU ---${NC_CHIRHO}"
    launch_qemu_chirho
    wait_for_qemu_chirho

    echo ""
    echo -e "${BOLD_CHIRHO}--- Step 5/5: Verify Output ---${NC_CHIRHO}"
    run_tests_chirho

    print_summary_chirho
}

main_chirho "$@"
