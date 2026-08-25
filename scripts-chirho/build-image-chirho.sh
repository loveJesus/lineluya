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

# The image-builder writes to /lineluya-chirho/output-chirho (WORKDIR-relative
# "output-chirho"), NOT to target/disk-images-chirho.  Copying from the wrong
# path under "|| true" is how this script used to report success while
# producing nothing, so the copy is now allowed to fail the script.
CONTAINER_ID_CHIRHO=$(docker create lineluya-builder-chirho)
copy_failed_chirho=0
for img_chirho in lineluya-bios-chirho.img lineluya-uefi-chirho.img; do
    if ! docker cp \
        "$CONTAINER_ID_CHIRHO:/lineluya-chirho/output-chirho/$img_chirho" \
        "$IMAGE_DIR_CHIRHO/"
    then
        echo "ERROR: $img_chirho not found in the container." >&2
        copy_failed_chirho=1
    fi
done
docker rm "$CONTAINER_ID_CHIRHO" > /dev/null

if [[ $copy_failed_chirho -ne 0 ]]; then
    echo "ERROR: image extraction failed - check the Docker build output above." >&2
    exit 1
fi

echo "[3/3] Disk images ready:"
ls -la "$IMAGE_DIR_CHIRHO"/*.img

echo ""
echo "To run in QEMU (BIOS):"
echo "  qemu-system-x86_64 -drive format=raw,file=$IMAGE_DIR_CHIRHO/lineluya-bios-chirho.img -serial stdio -m 512M"
echo ""
echo "To run in QEMU (UEFI, requires OVMF):"
echo "  qemu-system-x86_64 -bios /opt/homebrew/share/qemu/edk2-x86_64-code.fd -drive format=raw,file=$IMAGE_DIR_CHIRHO/lineluya-uefi-chirho.img -serial stdio -m 512M"
