#!/bin/bash
# For God so loved the world that he gave his only begotten Son,
# that whoever believes in him should not perish but have eternal life. - John 3:16
#
# Build the Lineluya kernel and create a bootable BIOS disk image.
# Runs DIRECTLY on the Parallels VM — no Docker needed.
#
# Usage:
#   ssh hallelujah@10.211.55.7 'bash /tmp/lineluya-build/scripts-chirho/build-and-run-chirho.sh'

set -euo pipefail

export PATH="$HOME/.cargo/bin:$PATH"

PROJECT_DIR_CHIRHO="/tmp/lineluya-build"
KERNEL_DIR_CHIRHO="$PROJECT_DIR_CHIRHO/kernel-chirho"
OUTPUT_DIR_CHIRHO="$PROJECT_DIR_CHIRHO/output-chirho"
ALPINE_IMG_CHIRHO="/tmp/alpine-virtio-chirho.img"
BIOS_IMG_CHIRHO="$OUTPUT_DIR_CHIRHO/lineluya-bios-chirho.img"

echo "=== Building Lineluya Kernel ==="
cd "$KERNEL_DIR_CHIRHO"
cargo +nightly-2026-03-10 build --release 2>&1 | tail -5
KERNEL_BIN_CHIRHO="$PROJECT_DIR_CHIRHO/target/x86_64-unknown-none/release/kernel-chirho"
echo "Kernel: $(ls -la $KERNEL_BIN_CHIRHO | awk '{print $5}') bytes"

echo "=== Creating boot image ==="
mkdir -p "$OUTPUT_DIR_CHIRHO"
# Use the Docker image builder (cached, always works)
docker run --rm \
  -v "$KERNEL_BIN_CHIRHO:/kernel:ro" \
  -v "$OUTPUT_DIR_CHIRHO:/out" \
  lineluya-builder-chirho bash -c \
  '/image-builder-chirho/target/release/image-builder-chirho /kernel && cp /lineluya-chirho/output-chirho/*.img /out/'
echo "BIOS image: $(ls -la $BIOS_IMG_CHIRHO | awk '{print $5}') bytes"

echo "=== Launching QEMU ==="
kill $(pgrep qemu) 2>/dev/null || true
rm -f /tmp/lineluya-serial.log /tmp/qemu-serial-in-chirho
mkfifo /tmp/qemu-serial-in-chirho
(sleep 999999 > /tmp/qemu-serial-in-chirho &)
nohup bash -c "cat /tmp/qemu-serial-in-chirho | qemu-system-x86_64 \
  -drive format=raw,file=$BIOS_IMG_CHIRHO \
  -drive file=$ALPINE_IMG_CHIRHO,format=raw,if=virtio \
  -netdev user,id=net0,hostfwd=tcp::3333-:2222 \
  -device virtio-net-pci,netdev=net0 \
  -m 512M -nographic > /tmp/lineluya-serial.log 2>&1" > /dev/null 2>&1 &
echo "QEMU PID: $!"
echo "=== Done — waiting for boot ==="
sleep 12
tail -3 /tmp/lineluya-serial.log
