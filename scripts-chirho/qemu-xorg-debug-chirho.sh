#!/bin/bash
# For God so loved the world that he gave his only begotten Son,
# that whoever believes in him should not perish but have eternal life. - John 3:16
#
# QEMU launch script for debugging the Xorg crash with GDB.
#
# Usage:
#   Terminal 1:  ./scripts-chirho/qemu-xorg-debug-chirho.sh
#   Terminal 2:  gdb -x scripts-chirho/gdb-xorg-crash-chirho.gdb
#
# After GDB connects and continues:
#   - The kernel boots and drops to a shell
#   - In the QEMU serial console, type:  Xorg :0
#   - GDB will break at the crash point and print the full backtrace

set -euo pipefail

SCRIPT_DIR_CHIRHO="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR_CHIRHO="$(dirname "$SCRIPT_DIR_CHIRHO")"

KERNEL_BIOS_CHIRHO="$PROJECT_DIR_CHIRHO/target/disk-images-chirho/lineluya-bios-chirho.img"
KERNEL_UEFI_CHIRHO="$PROJECT_DIR_CHIRHO/target/disk-images-chirho/lineluya-uefi-chirho.img"
ALPINE_ROOTFS_CHIRHO="$PROJECT_DIR_CHIRHO/target/alpine-virtio-chirho/alpine-virtio-chirho.img"

# ============================================================================
# Detect OVMF firmware
# ============================================================================
OVMF_CHIRHO=""
for path_chirho in \
    /usr/share/OVMF/OVMF_CODE_4M.fd \
    /usr/share/qemu/OVMF.fd \
    /usr/share/ovmf/OVMF.fd \
    /usr/share/edk2/x64/OVMF_CODE.fd \
    /opt/homebrew/share/qemu/edk2-x86_64-code.fd; do
    if [[ -f "$path_chirho" ]]; then
        OVMF_CHIRHO="$path_chirho"
        break
    fi
done

# ============================================================================
# Build QEMU command
# ============================================================================
CMD_CHIRHO=(qemu-system-x86_64)

# Memory — Xorg needs more for framebuffer + heap
CMD_CHIRHO+=(-m 1G)
CMD_CHIRHO+=(-smp 1)
CMD_CHIRHO+=(-machine q35)
CMD_CHIRHO+=(-cpu qemu64)

# Boot mode: prefer UEFI if available, else BIOS
if [[ -n "$OVMF_CHIRHO" && -f "$KERNEL_UEFI_CHIRHO" ]]; then
    echo "[DEBUG-QEMU] Using UEFI boot with $OVMF_CHIRHO"
    CMD_CHIRHO+=(-bios "$OVMF_CHIRHO")
    CMD_CHIRHO+=(-drive "format=raw,file=$KERNEL_UEFI_CHIRHO")
elif [[ -f "$KERNEL_BIOS_CHIRHO" ]]; then
    echo "[DEBUG-QEMU] Using BIOS boot"
    CMD_CHIRHO+=(-drive "format=raw,file=$KERNEL_BIOS_CHIRHO")
else
    echo "ERROR: No kernel image found."
    exit 1
fi

# Alpine rootfs (VirtIO-blk)
CMD_CHIRHO+=(-drive "file=$ALPINE_ROOTFS_CHIRHO,format=raw,if=virtio")

# Serial console
CMD_CHIRHO+=(-serial mon:stdio)

# Display — need VGA for framebuffer so Xorg can start
CMD_CHIRHO+=(-display default)

# Networking (for any potential needs)
CMD_CHIRHO+=(-netdev "user,id=net0-chirho,hostfwd=tcp::2222-:2222")
CMD_CHIRHO+=(-device "virtio-net-pci,netdev=net0-chirho")

# Audio (minimal — suppress QEMU warnings)
CMD_CHIRHO+=(-audiodev "none,id=snd0-chirho")
CMD_CHIRHO+=(-device "intel-hda")
CMD_CHIRHO+=(-device "hda-duplex,audiodev=snd0-chirho")

# ============================================================================
# GDB debug flags
# ============================================================================
CMD_CHIRHO+=(-s)          # GDB server on localhost:1234
CMD_CHIRHO+=(-S)          # Halt CPU at startup — wait for GDB to connect

# Optionally log interrupts (large output, disable if too noisy):
# CMD_CHIRHO+=(-d "int,cpu_reset")
# CMD_CHIRHO+=(-D "$PROJECT_DIR_CHIRHO/target/qemu-xorg-debug-chirho.log")

# ============================================================================
# Pre-flight checks
# ============================================================================
if ! command -v qemu-system-x86_64 &>/dev/null; then
    echo "ERROR: qemu-system-x86_64 not found."
    exit 1
fi
if [[ ! -f "$ALPINE_ROOTFS_CHIRHO" ]]; then
    echo "ERROR: Alpine rootfs not found at: $ALPINE_ROOTFS_CHIRHO"
    exit 1
fi

# ============================================================================
# Extract Xorg and musl for GDB (if not already done)
# ============================================================================
if [[ ! -f /tmp/Xorg-chirho || ! -f /tmp/ld-musl-chirho.so ]]; then
    echo "[DEBUG-QEMU] Extracting Xorg and musl from Alpine rootfs for GDB..."
    LOOP_DEV_CHIRHO=$(sudo losetup --show -f -P "$ALPINE_ROOTFS_CHIRHO")
    MOUNT_DIR_CHIRHO=$(mktemp -d)
    sudo mount -o ro "$LOOP_DEV_CHIRHO" "$MOUNT_DIR_CHIRHO"
    cp "$MOUNT_DIR_CHIRHO/usr/libexec/Xorg" /tmp/Xorg-chirho 2>/dev/null || true
    cp "$MOUNT_DIR_CHIRHO/lib/ld-musl-x86_64.so.1" /tmp/ld-musl-chirho.so 2>/dev/null || true
    sudo umount "$MOUNT_DIR_CHIRHO"
    sudo losetup -d "$LOOP_DEV_CHIRHO"
    rmdir "$MOUNT_DIR_CHIRHO"
    echo "[DEBUG-QEMU] Extracted to /tmp/Xorg-chirho and /tmp/ld-musl-chirho.so"
fi

# ============================================================================
# Launch
# ============================================================================
echo ""
echo "=========================================================="
echo " Lineluya Xorg Debug Session"
echo " For God so loved the world... - John 3:16"
echo "=========================================================="
echo ""
echo " QEMU GDB server: localhost:1234 (CPU halted)"
echo ""
echo " In another terminal, run:"
echo "   gdb -x $SCRIPT_DIR_CHIRHO/gdb-xorg-crash-chirho.gdb"
echo ""
echo " After GDB continues, type in this console:"
echo "   Xorg :0"
echo ""
echo " GDB will catch the crash and print the full backtrace."
echo "=========================================================="
echo ""
echo " Command: ${CMD_CHIRHO[*]}"
echo ""

exec "${CMD_CHIRHO[@]}"
