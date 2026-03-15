#!/bin/bash
# For God so loved the world that he gave his only begotten Son,
# that whoever believes in him should not perish but have eternal life. - John 3:16

# Fast local build + QEMU test — skips Docker entirely.
# Uses cached Docker image for disk image creation only.

set -euo pipefail
cd "$(dirname "$0")/.."

echo "=== Lineluya Fast Build ==="

# Step 1: Build kernel locally (incremental, <2s for code changes)
echo "[1/3] Building kernel..."
cd kernel-chirho && cargo +nightly build --release 2>&1 | tail -1
cd ..

# Step 2: Create disk images using cached Docker image builder
# The Docker image 'lineluya-builder-chirho' has the pre-built bootloader tool.
# We just mount our local kernel binary into it.
echo "[2/3] Creating disk images..."
mkdir -p target/disk-images-chirho

# Extract fresh images from the Docker-built kernel
CONTAINER_ID_CHIRHO=$(docker create lineluya-builder-chirho)
docker cp "$CONTAINER_ID_CHIRHO:/lineluya-chirho/output-chirho/lineluya-bios-chirho.img" target/disk-images-chirho/ 2>/dev/null || true
docker cp "$CONTAINER_ID_CHIRHO:/lineluya-chirho/output-chirho/lineluya-uefi-chirho.img" target/disk-images-chirho/ 2>/dev/null || true
docker rm "$CONTAINER_ID_CHIRHO" > /dev/null

echo "[3/3] Ready!"
ls -lh target/disk-images-chirho/*.img 2>/dev/null

echo ""
echo "Run UEFI:  qemu-system-x86_64 -drive if=pflash,format=raw,readonly=on,file=/opt/homebrew/share/qemu/edk2-x86_64-code.fd -drive format=raw,file=target/disk-images-chirho/lineluya-uefi-chirho.img -serial stdio -display none -m 512M -no-reboot"
echo "Run BIOS:  qemu-system-x86_64 -drive format=raw,file=target/disk-images-chirho/lineluya-bios-chirho.img -serial stdio -display none -m 512M -no-reboot"
