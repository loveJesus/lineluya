#!/bin/bash
# For God so loved the world that he gave his only begotten Son,
# that whoever believes in him should not perish but have eternal life. - John 3:16

# E1-002: Write the Lineluya kernel to a USB drive for real hardware boot.
#
# Creates a bootable USB with GRUB2 + the Lineluya kernel ELF.
#
# Usage:
#   ./scripts-chirho/make-usb-chirho.sh /dev/sdX
#   ./scripts-chirho/make-usb-chirho.sh /dev/diskN          # macOS
#   ./scripts-chirho/make-usb-chirho.sh --image output.img  # disk image only
#
# WARNING: This script will DESTROY ALL DATA on the target device.
# Triple-check the device path before running.

set -euo pipefail

# ============================================================================
# Configuration
# ============================================================================

SCRIPT_DIR_CHIRHO="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR_CHIRHO="$(dirname "$SCRIPT_DIR_CHIRHO")"
KERNEL_ELF_CHIRHO="$PROJECT_DIR_CHIRHO/kernel-chirho/target/x86_64-unknown-none/release/kernel-chirho"
GRUB_CFG_CHIRHO="$PROJECT_DIR_CHIRHO/boot-chirho/grub-chirho/grub-chirho.cfg"
MOUNT_POINT_CHIRHO="/tmp/lineluya-usb-mount-chirho"
IMAGE_SIZE_MB_CHIRHO=64

# ============================================================================
# Color output helpers
# ============================================================================

RED_CHIRHO='\033[0;31m'
GREEN_CHIRHO='\033[0;32m'
YELLOW_CHIRHO='\033[1;33m'
NC_CHIRHO='\033[0m' # No Color

info_chirho() { echo -e "${GREEN_CHIRHO}[INFO]${NC_CHIRHO} $*"; }
warn_chirho() { echo -e "${YELLOW_CHIRHO}[WARN]${NC_CHIRHO} $*"; }
error_chirho() { echo -e "${RED_CHIRHO}[ERROR]${NC_CHIRHO} $*" >&2; }

# ============================================================================
# Argument parsing
# ============================================================================

IMAGE_MODE_CHIRHO=false
TARGET_DEVICE_CHIRHO=""
OUTPUT_IMAGE_CHIRHO=""

if [ $# -lt 1 ]; then
    echo "Usage: $0 <device>        # e.g. /dev/sdb, /dev/disk2"
    echo "       $0 --image <file>  # create a disk image file"
    echo ""
    echo "WARNING: All data on the target device will be destroyed!"
    exit 1
fi

if [ "$1" = "--image" ]; then
    IMAGE_MODE_CHIRHO=true
    OUTPUT_IMAGE_CHIRHO="${2:-$PROJECT_DIR_CHIRHO/target/lineluya-usb-chirho.img}"
    info_chirho "Image mode: will create $OUTPUT_IMAGE_CHIRHO"
else
    TARGET_DEVICE_CHIRHO="$1"
    if [ ! -b "$TARGET_DEVICE_CHIRHO" ]; then
        error_chirho "Device $TARGET_DEVICE_CHIRHO does not exist or is not a block device."
        exit 1
    fi
fi

# ============================================================================
# Pre-flight checks
# ============================================================================

if [ ! -f "$KERNEL_ELF_CHIRHO" ]; then
    error_chirho "Kernel ELF not found at: $KERNEL_ELF_CHIRHO"
    error_chirho "Build the kernel first:"
    error_chirho "  cd kernel-chirho && cargo +nightly build --release"
    exit 1
fi

if [ ! -f "$GRUB_CFG_CHIRHO" ]; then
    error_chirho "GRUB config not found at: $GRUB_CFG_CHIRHO"
    exit 1
fi

# Check for required tools
for tool_chirho in grub-install parted mkfs.ext4; do
    if ! command -v "$tool_chirho" &>/dev/null; then
        # On macOS, grub-install may not be available — try grub2-install
        if [ "$tool_chirho" = "grub-install" ] && command -v grub2-install &>/dev/null; then
            continue
        fi
        error_chirho "Required tool '$tool_chirho' not found. Install it first."
        exit 1
    fi
done

# Determine grub-install command name
GRUB_INSTALL_CMD_CHIRHO="grub-install"
if ! command -v grub-install &>/dev/null && command -v grub2-install &>/dev/null; then
    GRUB_INSTALL_CMD_CHIRHO="grub2-install"
fi

# ============================================================================
# Safety confirmation for real devices
# ============================================================================

if [ "$IMAGE_MODE_CHIRHO" = false ]; then
    echo ""
    warn_chirho "This will ERASE ALL DATA on $TARGET_DEVICE_CHIRHO"
    echo ""
    # Show device info for confirmation
    if command -v lsblk &>/dev/null; then
        lsblk "$TARGET_DEVICE_CHIRHO" 2>/dev/null || true
    elif command -v diskutil &>/dev/null; then
        diskutil info "$TARGET_DEVICE_CHIRHO" 2>/dev/null | head -20 || true
    fi
    echo ""
    read -p "Type 'YES' to confirm: " CONFIRM_CHIRHO
    if [ "$CONFIRM_CHIRHO" != "YES" ]; then
        error_chirho "Aborted by user."
        exit 1
    fi
fi

# ============================================================================
# Create disk image (if image mode)
# ============================================================================

if [ "$IMAGE_MODE_CHIRHO" = true ]; then
    info_chirho "Creating ${IMAGE_SIZE_MB_CHIRHO}MB disk image..."
    dd if=/dev/zero of="$OUTPUT_IMAGE_CHIRHO" bs=1M count=$IMAGE_SIZE_MB_CHIRHO status=progress
    TARGET_DEVICE_CHIRHO=$(losetup --find --show "$OUTPUT_IMAGE_CHIRHO" 2>/dev/null || \
                           hdiutil attach -nomount "$OUTPUT_IMAGE_CHIRHO" 2>/dev/null | awk '{print $1}')
    info_chirho "Loop device: $TARGET_DEVICE_CHIRHO"
fi

# ============================================================================
# Partition and format
# ============================================================================

info_chirho "Partitioning $TARGET_DEVICE_CHIRHO..."

# Create MBR partition table with one bootable partition
parted -s "$TARGET_DEVICE_CHIRHO" mklabel msdos
parted -s "$TARGET_DEVICE_CHIRHO" mkpart primary ext4 1MiB 100%
parted -s "$TARGET_DEVICE_CHIRHO" set 1 boot on

# Determine partition device name
PARTITION_CHIRHO="${TARGET_DEVICE_CHIRHO}1"
if [ ! -b "$PARTITION_CHIRHO" ]; then
    PARTITION_CHIRHO="${TARGET_DEVICE_CHIRHO}p1"
fi
if [ ! -b "$PARTITION_CHIRHO" ]; then
    # Wait for kernel to create partition device
    sleep 2
    partprobe "$TARGET_DEVICE_CHIRHO" 2>/dev/null || true
    PARTITION_CHIRHO="${TARGET_DEVICE_CHIRHO}1"
fi

info_chirho "Formatting partition $PARTITION_CHIRHO as ext4..."
mkfs.ext4 -F -L "LINELUYA-CHIRHO" "$PARTITION_CHIRHO"

# ============================================================================
# Mount and install files
# ============================================================================

info_chirho "Mounting partition..."
mkdir -p "$MOUNT_POINT_CHIRHO"
mount "$PARTITION_CHIRHO" "$MOUNT_POINT_CHIRHO"

# Create boot directory structure
mkdir -p "$MOUNT_POINT_CHIRHO/boot/grub"

# Copy kernel
info_chirho "Copying kernel ELF..."
cp "$KERNEL_ELF_CHIRHO" "$MOUNT_POINT_CHIRHO/boot/lineluya-chirho.elf"

# Copy GRUB config
info_chirho "Installing GRUB configuration..."
cp "$GRUB_CFG_CHIRHO" "$MOUNT_POINT_CHIRHO/boot/grub/grub.cfg"

# ============================================================================
# Install GRUB bootloader
# ============================================================================

info_chirho "Installing GRUB bootloader..."
$GRUB_INSTALL_CMD_CHIRHO \
    --target=i386-pc \
    --boot-directory="$MOUNT_POINT_CHIRHO/boot" \
    --recheck \
    "$TARGET_DEVICE_CHIRHO"

# ============================================================================
# Cleanup
# ============================================================================

info_chirho "Syncing and unmounting..."
sync
umount "$MOUNT_POINT_CHIRHO"
rmdir "$MOUNT_POINT_CHIRHO" 2>/dev/null || true

if [ "$IMAGE_MODE_CHIRHO" = true ]; then
    losetup -d "$TARGET_DEVICE_CHIRHO" 2>/dev/null || \
    hdiutil detach "$TARGET_DEVICE_CHIRHO" 2>/dev/null || true
    info_chirho "Disk image created: $OUTPUT_IMAGE_CHIRHO"
    info_chirho ""
    info_chirho "To test with QEMU:"
    info_chirho "  qemu-system-x86_64 -drive format=raw,file=$OUTPUT_IMAGE_CHIRHO -serial stdio -m 512M"
else
    info_chirho "USB drive is ready!"
    info_chirho "Remove the drive and boot from it on target hardware."
fi

info_chirho "Done. Hallelujah!"
