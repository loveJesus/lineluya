#!/bin/bash
# For God so loved the world that he gave his only begotten Son,
# that whoever believes in him should not perish but have eternal life. - John 3:16

# G1-004: QEMU launch script for booting Lineluya with Alpine Linux rootfs
#
# Boots the Lineluya kernel with an Alpine Linux ext4 rootfs disk image.
# Supports both BIOS and UEFI boot modes.
#
# Usage:
#   ./scripts-chirho/qemu-alpine-chirho.sh [--uefi] [--debug] [--memory 1G] [--smp 2]

set -euo pipefail

# ============================================================================
# Configuration constants
# ============================================================================

SCRIPT_DIR_CHIRHO="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR_CHIRHO="$(dirname "$SCRIPT_DIR_CHIRHO")"

# Kernel image paths (try multiple locations)
KERNEL_BIOS_CHIRHO="$PROJECT_DIR_CHIRHO/target/disk-images-chirho/lineluya-bios-chirho.img"
KERNEL_UEFI_CHIRHO="$PROJECT_DIR_CHIRHO/target/disk-images-chirho/lineluya-uefi-chirho.img"
KERNEL_BIN_CHIRHO="$PROJECT_DIR_CHIRHO/target/x86_64-lineluya-chirho/release/lineluya-chirho"

# Alpine rootfs
ALPINE_ROOTFS_CHIRHO="$PROJECT_DIR_CHIRHO/target/alpine-virtio-chirho/alpine-virtio-chirho.img"

# QEMU defaults
MEMORY_CHIRHO="${MEMORY_CHIRHO:-1G}"
SMP_CHIRHO="${SMP_CHIRHO:-2}"
BOOT_MODE_CHIRHO="bios"
DEBUG_MODE_CHIRHO=0
EXTRA_ARGS_CHIRHO=""

# OVMF firmware paths (platform-dependent)
OVMF_PATHS_CHIRHO=(
    "/opt/homebrew/share/qemu/edk2-x86_64-code.fd"
    "/usr/share/OVMF/OVMF_CODE.fd"
    "/usr/share/edk2/x64/OVMF_CODE.fd"
    "/usr/share/qemu/OVMF.fd"
)

# ============================================================================
# Argument parsing
# ============================================================================

parse_args_chirho() {
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --uefi)
                BOOT_MODE_CHIRHO="uefi"
                shift
                ;;
            --debug)
                DEBUG_MODE_CHIRHO=1
                shift
                ;;
            --memory|-m)
                MEMORY_CHIRHO="$2"
                shift 2
                ;;
            --smp)
                SMP_CHIRHO="$2"
                shift 2
                ;;
            --kernel)
                KERNEL_BIN_CHIRHO="$2"
                shift 2
                ;;
            --rootfs)
                ALPINE_ROOTFS_CHIRHO="$2"
                shift 2
                ;;
            --extra)
                EXTRA_ARGS_CHIRHO="$2"
                shift 2
                ;;
            --help|-h)
                print_help_chirho
                exit 0
                ;;
            *)
                echo "Unknown option: $1"
                exit 1
                ;;
        esac
    done
}

print_help_chirho() {
    cat << 'HELP_EOF_CHIRHO'
Usage: qemu-alpine-chirho.sh [OPTIONS]

Boot Lineluya kernel with Alpine Linux rootfs in QEMU.

Options:
  --uefi          Use UEFI boot mode (requires OVMF)
  --debug         Enable QEMU debug options (GDB server on :1234, no KVM)
  --memory SIZE   RAM size (default: 1G)
  --smp N         Number of CPU cores (default: 2)
  --kernel PATH   Path to kernel binary
  --rootfs PATH   Path to Alpine rootfs disk image
  --extra "ARGS"  Additional QEMU arguments
  --help          Show this help

Environment Variables:
  MEMORY_CHIRHO   RAM size override
  SMP_CHIRHO      CPU count override
HELP_EOF_CHIRHO
}

# ============================================================================
# Utility functions
# ============================================================================

log_chirho() {
    echo "[QEMU-ALPINE] $*"
}

find_ovmf_chirho() {
    for path_chirho in "${OVMF_PATHS_CHIRHO[@]}"; do
        if [[ -f "$path_chirho" ]]; then
            echo "$path_chirho"
            return 0
        fi
    done
    return 1
}

check_prereqs_chirho() {
    if ! command -v qemu-system-x86_64 &>/dev/null; then
        echo "ERROR: qemu-system-x86_64 not found."
        echo "  Install with: brew install qemu  (macOS)"
        echo "                apt install qemu-system-x86  (Debian/Ubuntu)"
        exit 1
    fi

    if [[ ! -f "$ALPINE_ROOTFS_CHIRHO" ]]; then
        echo "ERROR: Alpine rootfs image not found at: $ALPINE_ROOTFS_CHIRHO"
        echo "  Run: ./scripts-chirho/setup-alpine-chirho.sh"
        exit 1
    fi
}

# ============================================================================
# Build QEMU command
# ============================================================================

build_qemu_cmd_chirho() {
    local cmd_chirho=(qemu-system-x86_64)

    # ---- Memory and CPU ----
    cmd_chirho+=(-m "$MEMORY_CHIRHO")
    cmd_chirho+=(-smp "$SMP_CHIRHO")

    # ---- Machine type ----
    cmd_chirho+=(-machine q35)
    cmd_chirho+=(-cpu qemu64)

    # ---- Boot mode ----
    if [[ "$BOOT_MODE_CHIRHO" == "uefi" ]]; then
        local ovmf_path_chirho
        ovmf_path_chirho=$(find_ovmf_chirho) || {
            echo "ERROR: OVMF firmware not found for UEFI boot."
            exit 1
        }
        cmd_chirho+=(-bios "$ovmf_path_chirho")

        if [[ -f "$KERNEL_UEFI_CHIRHO" ]]; then
            cmd_chirho+=(-drive "format=raw,file=$KERNEL_UEFI_CHIRHO")
        else
            echo "ERROR: UEFI kernel image not found at: $KERNEL_UEFI_CHIRHO"
            exit 1
        fi
    else
        # BIOS boot
        if [[ -f "$KERNEL_BIOS_CHIRHO" ]]; then
            cmd_chirho+=(-drive "format=raw,file=$KERNEL_BIOS_CHIRHO")
        elif [[ -f "$KERNEL_BIN_CHIRHO" ]]; then
            # Direct kernel boot (if kernel supports -kernel flag)
            cmd_chirho+=(-kernel "$KERNEL_BIN_CHIRHO")
            cmd_chirho+=(-append "root=/dev/sda rw console=ttyS0 init=/sbin/init")
        else
            echo "ERROR: No kernel image found."
            echo "  Expected: $KERNEL_BIOS_CHIRHO"
            echo "  Build with: make  (or cargo build)"
            exit 1
        fi
    fi

    # ---- Alpine rootfs disk (VirtIO-blk) ----
    cmd_chirho+=(-drive "file=$ALPINE_ROOTFS_CHIRHO,format=raw,if=virtio")

    # ---- Serial console (primary I/O for Lineluya) ----
    cmd_chirho+=(-serial stdio)
    cmd_chirho+=(-nographic)

    # ---- Networking (VirtIO-net, user-mode, port forward SSH) ----
    cmd_chirho+=(-netdev "user,id=net0-chirho,hostfwd=tcp::2222-:2222")
    cmd_chirho+=(-device "virtio-net-pci,netdev=net0-chirho")

    # ---- Audio (Intel HDA + AC97 for /dev/dsp + PC speaker) ----
    # Use wav backend on headless/WSL2, pa/pipewire if available
    local audio_backend_chirho="none"
    if command -v pactl &>/dev/null && pactl info &>/dev/null 2>&1; then
        audio_backend_chirho="pa"
    elif command -v pipewire &>/dev/null; then
        audio_backend_chirho="pipewire"
    fi
    cmd_chirho+=(-audiodev "${audio_backend_chirho},id=snd0-chirho")
    cmd_chirho+=(-device "intel-hda")
    cmd_chirho+=(-device "hda-duplex,audiodev=snd0-chirho")
    cmd_chirho+=(-device "AC97,audiodev=snd0-chirho")
    cmd_chirho+=(-machine "pcspk-audiodev=snd0-chirho")

    # ---- Debug mode ----
    if [[ "$DEBUG_MODE_CHIRHO" -eq 1 ]]; then
        cmd_chirho+=(-s)          # GDB server on :1234
        cmd_chirho+=(-S)          # Halt CPU at startup (wait for GDB)
        cmd_chirho+=(-d "int,cpu_reset")
        cmd_chirho+=(-D "$PROJECT_DIR_CHIRHO/target/qemu-debug-chirho.log")
        log_chirho "Debug mode: GDB server on localhost:1234, CPU halted."
        log_chirho "  Connect with: gdb -ex 'target remote :1234'"
    fi

    # ---- Extra arguments ----
    if [[ -n "$EXTRA_ARGS_CHIRHO" ]]; then
        # shellcheck disable=SC2086
        cmd_chirho+=($EXTRA_ARGS_CHIRHO)
    fi

    echo "${cmd_chirho[@]}"
}

# ============================================================================
# Main
# ============================================================================

main_chirho() {
    parse_args_chirho "$@"

    echo "=== Lineluya + Alpine Linux QEMU Launcher (G1-004) ==="
    echo "For God so loved the world... - John 3:16"
    echo ""

    check_prereqs_chirho

    log_chirho "Configuration:"
    log_chirho "  Boot mode:  $BOOT_MODE_CHIRHO"
    log_chirho "  Memory:     $MEMORY_CHIRHO"
    log_chirho "  CPUs:       $SMP_CHIRHO"
    log_chirho "  Rootfs:     $ALPINE_ROOTFS_CHIRHO"
    log_chirho "  Debug:      $([ "$DEBUG_MODE_CHIRHO" -eq 1 ] && echo yes || echo no)"
    echo ""

    local qemu_cmd_chirho
    qemu_cmd_chirho=$(build_qemu_cmd_chirho)

    log_chirho "Launching QEMU..."
    log_chirho "  $qemu_cmd_chirho"
    echo ""

    # Execute
    eval "$qemu_cmd_chirho"
}

main_chirho "$@"
