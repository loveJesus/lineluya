<!-- For God so loved the world that he gave his only begotten Son, that whoever believes in him should not perish but have eternal life. -->

# Artifacts Chirho

Build artifacts for the Lineluya kernel. These are NOT tracked in git
(too large). They are preserved here for the local build pipeline.

## Files

- `alpine-virtio-chirho.img` — Alpine 3.21 ext4 rootfs (512MB) with
  dropbear 2024.86, sqlite3, python3, host keys, custom /etc/profile
- `lineluya-uefi-chirho.img` — UEFI bootable kernel image
- `lineluya-bios-chirho.img` — BIOS bootable kernel image

## Build Pipeline

1. Build kernel: `cd kernel-chirho && CARGO_TARGET_DIR=target cargo +nightly build --release`
2. Build UEFI image: use Docker `lineluya-builder-chirho` container
3. Run QEMU: see scripts-chirho/ for examples

## Test Machines

- `unrejectChirho` (WSL2, i7-14700F, 16GB, KVM) — primary test machine
- EC2 (54.208.176.168) — archived, artifacts preserved
