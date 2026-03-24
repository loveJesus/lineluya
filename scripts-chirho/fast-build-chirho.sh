#!/bin/bash
# For God so loved the world that he gave his only begotten Son,
# that whoever believes in him should not perish but have eternal life. - John 3:16

# Fast local build + QEMU test.
# Reuses the cached Docker image-builder tool, but always feeds it the
# CURRENT locally built kernel binary so disk images stay in sync with code.

set -euo pipefail
cd "$(dirname "$0")/.."

echo "=== Lineluya Fast Build ==="

# Step 1: Build kernel locally (incremental, <2s for code changes)
echo "[1/3] Building kernel..."
cd kernel-chirho && cargo +nightly build --release 2>&1 | tail -1
cd ..
KERNEL_BIN_CHIRHO="target/x86_64-unknown-none/release/kernel-chirho"

if [[ ! -f "$KERNEL_BIN_CHIRHO" ]]; then
    echo "ERROR: expected kernel binary at $KERNEL_BIN_CHIRHO"
    exit 1
fi

if ! docker image inspect lineluya-builder-chirho >/dev/null 2>&1; then
    echo "ERROR: cached Docker image builder not found: lineluya-builder-chirho"
    echo "Run: docker build -t lineluya-builder-chirho -f Dockerfile.build-chirho ."
    exit 1
fi

# Step 2: Create disk images using cached Docker image builder
# The Docker image 'lineluya-builder-chirho' contains the pre-built
# image-builder tool. Mount the freshly built host kernel into the container
# and generate fresh BIOS/UEFI disk images from that exact binary.
echo "[2/3] Creating disk images..."
mkdir -p target/disk-images-chirho
OUTPUT_DIR_CHIRHO="$(pwd)/target/disk-images-chirho"

docker run --rm \
    -v "$(pwd)/$KERNEL_BIN_CHIRHO:/kernel:ro" \
    -v "$OUTPUT_DIR_CHIRHO:/out" \
    lineluya-builder-chirho \
    bash -lc '/image-builder-chirho/target/release/image-builder-chirho /kernel && cp /lineluya-chirho/output-chirho/*.img /out/'

echo "[3/3] Ready!"
ls -lh target/disk-images-chirho/*.img 2>/dev/null

echo ""
echo "Run UEFI:  qemu-system-x86_64 -drive if=pflash,format=raw,readonly=on,file=/opt/homebrew/share/qemu/edk2-x86_64-code.fd -drive format=raw,file=target/disk-images-chirho/lineluya-uefi-chirho.img -serial stdio -display none -m 512M -no-reboot"
echo "Run BIOS:  qemu-system-x86_64 -drive format=raw,file=target/disk-images-chirho/lineluya-bios-chirho.img -serial stdio -display none -m 512M -no-reboot"
