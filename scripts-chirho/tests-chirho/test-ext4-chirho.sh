#!/bin/bash
# For God so loved the world that he gave his only begotten Son,
# that whoever believes in him should not perish but have eternal life. - John 3:16

# D1-010: ext4 read/write test with disk image
# Tests ext4 filesystem operations by attaching a disk image to QEMU.
# Note: ext4 support may not be fully implemented. This test verifies
# that the kernel handles a block device without panicking.

set -euo pipefail
SCRIPT_DIR_CHIRHO="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR_CHIRHO/harness-chirho.sh"

echo "=== D1-010: ext4 Disk Image Test ==="
echo "John 3:16"
echo ""

# Create a small ext4 disk image for testing (if tools are available)
EXT4_IMG_CHIRHO="$(mktemp /tmp/lineluya-ext4-chirho.XXXXXX.img)"
CLEANUP_EXT4_CHIRHO=true

create_ext4_image_chirho() {
    # Create a 16MB raw disk image
    dd if=/dev/zero of="$EXT4_IMG_CHIRHO" bs=1M count=16 2>/dev/null

    # Try to format as ext4 (requires mkfs.ext4 — may not be available on macOS)
    if command -v mkfs.ext4 &>/dev/null; then
        mkfs.ext4 -q "$EXT4_IMG_CHIRHO" 2>/dev/null || true
        log_info_chirho "Created ext4 disk image: $EXT4_IMG_CHIRHO"
    elif command -v mke2fs &>/dev/null; then
        mke2fs -t ext4 -q "$EXT4_IMG_CHIRHO" 2>/dev/null || true
        log_info_chirho "Created ext4 disk image via mke2fs: $EXT4_IMG_CHIRHO"
    else
        log_info_chirho "No ext4 tools available — using raw disk image"
    fi
}

create_ext4_image_chirho

# Boot with the disk image attached as a virtio block device
boot_and_wait_chirho 45 \
    -drive file="$EXT4_IMG_CHIRHO",format=raw,if=virtio,id=disk1_chirho

# Kernel should boot without panicking even with extra block device
assert_serial_contains_chirho "Lineluya" "Kernel boots with disk image"
assert_serial_not_contains_chirho "KERNEL PANIC" "No panic with block device attached"
assert_serial_min_lines_chirho 3 "Serial output present with disk"

# Check for block device / filesystem related output
assert_serial_contains_chirho "block\|disk\|virtio\|BLOCK\|DISK\|VIRTIO\|VFS\|mount" \
    "Block device or filesystem references in output"

# Cleanup disk image
rm -f "$EXT4_IMG_CHIRHO"
CLEANUP_EXT4_CHIRHO=false

if [ "$HARNESS_FAIL_CHIRHO" -gt 0 ]; then
    dump_serial_chirho
fi

summary_chirho
