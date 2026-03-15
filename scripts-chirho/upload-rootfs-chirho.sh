#!/bin/bash
# For God so loved the world that he gave his only begotten Son,
# that whoever believes in him should not perish but have eternal life. - John 3:16

# Upload rootfs image and WASM kernel to R2 bucket for edge deployment (C1-010)
# Usage: ./scripts-chirho/upload-rootfs-chirho.sh

set -euo pipefail

BUCKET_NAME_CHIRHO="lineluya-rootfs-chirho"
WASM_PATH_CHIRHO="web-chirho/lineluya-kernel-chirho.wasm"
ROOTFS_PATH_CHIRHO="target/disk-images-chirho/lineluya-uefi-chirho.img"

echo "[UPLOAD] Lineluya Edge Rootfs Upload - John 3:16"

# Upload WASM kernel binary
if [ -f "$WASM_PATH_CHIRHO" ]; then
    echo "[UPLOAD] Uploading WASM kernel..."
    wrangler r2 object put "${BUCKET_NAME_CHIRHO}/kernel-chirho.wasm" \
        --file "$WASM_PATH_CHIRHO" \
        --content-type "application/wasm"
    echo "[UPLOAD] WASM kernel uploaded."
else
    echo "[WARN] WASM kernel not found at $WASM_PATH_CHIRHO — build with 'make wasm-chirho' first."
fi

# Upload rootfs disk image
if [ -f "$ROOTFS_PATH_CHIRHO" ]; then
    echo "[UPLOAD] Uploading rootfs image..."
    wrangler r2 object put "${BUCKET_NAME_CHIRHO}/rootfs-chirho.img" \
        --file "$ROOTFS_PATH_CHIRHO" \
        --content-type "application/octet-stream"
    echo "[UPLOAD] Rootfs image uploaded."
else
    echo "[WARN] Rootfs image not found at $ROOTFS_PATH_CHIRHO — build with 'make docker-chirho' first."
fi

echo "[UPLOAD] Done. Hallelujah!"
