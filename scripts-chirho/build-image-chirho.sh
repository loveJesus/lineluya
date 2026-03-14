#!/bin/bash
# For God so loved the world that he gave his only begotten Son,
# that whoever believes in him should not perish but have eternal life. - John 3:16

# Build bootable disk images using Docker (needed for BIOS stage cross-compilation)
# and then run in QEMU.

set -euo pipefail

SCRIPT_DIR_CHIRHO="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR_CHIRHO="$(dirname "$SCRIPT_DIR_CHIRHO")"
IMAGE_DIR_CHIRHO="$PROJECT_DIR_CHIRHO/target/disk-images-chirho"
CONTAINER_NAME_CHIRHO="lineluya-builder-chirho"

echo "=== Lineluya Kernel Image Builder ==="
echo "Project: $PROJECT_DIR_CHIRHO"

# Build Docker image
echo "[1/3] Building Docker build environment..."
docker build \
    -t lineluya-builder-chirho \
    -f "$PROJECT_DIR_CHIRHO/Dockerfile.build-chirho" \
    "$PROJECT_DIR_CHIRHO"

# Extract disk images from container
echo "[2/3] Extracting disk images..."
mkdir -p "$IMAGE_DIR_CHIRHO"

CONTAINER_ID_CHIRHO=$(docker create lineluya-builder-chirho)
docker cp "$CONTAINER_ID_CHIRHO:/lineluya-chirho/target/disk-images-chirho/lineluya-bios-chirho.img" "$IMAGE_DIR_CHIRHO/" 2>/dev/null || true
docker cp "$CONTAINER_ID_CHIRHO:/lineluya-chirho/target/disk-images-chirho/lineluya-uefi-chirho.img" "$IMAGE_DIR_CHIRHO/" 2>/dev/null || true
docker rm "$CONTAINER_ID_CHIRHO" > /dev/null

echo "[3/3] Disk images ready:"
ls -la "$IMAGE_DIR_CHIRHO"/*.img 2>/dev/null || echo "  (no images found - check Docker build output)"

echo ""
echo "To run in QEMU (BIOS):"
echo "  qemu-system-x86_64 -drive format=raw,file=$IMAGE_DIR_CHIRHO/lineluya-bios-chirho.img -serial stdio -m 512M"
echo ""
echo "To run in QEMU (UEFI, requires OVMF):"
echo "  qemu-system-x86_64 -bios /opt/homebrew/share/qemu/edk2-x86_64-code.fd -drive format=raw,file=$IMAGE_DIR_CHIRHO/lineluya-uefi-chirho.img -serial stdio -m 512M"
