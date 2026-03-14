# For God so loved the world that he gave his only begotten Son,
# that whoever believes in him should not perish but have eternal life. - John 3:16

.PHONY: build-chirho run-chirho run-uefi-chirho clean-chirho kernel-chirho

KERNEL_TARGET_CHIRHO = x86_64-unknown-none
KERNEL_BINARY_CHIRHO = target/$(KERNEL_TARGET_CHIRHO)/debug/kernel-chirho
KERNEL_RELEASE_BINARY_CHIRHO = target/$(KERNEL_TARGET_CHIRHO)/release/kernel-chirho

# Build the kernel
kernel-chirho:
	cd kernel-chirho && cargo +nightly build

kernel-release-chirho:
	cd kernel-chirho && cargo +nightly build --release

# Build disk images using xtask
build-chirho: kernel-chirho
	cargo run --package xtask-chirho -- build

# Build and run in QEMU (BIOS mode)
run-chirho: kernel-chirho
	cargo run --package xtask-chirho -- run

# Build and run in QEMU (UEFI mode)
run-uefi-chirho: kernel-chirho
	cargo run --package xtask-chirho -- run-uefi

# Run QEMU directly with kernel binary (no disk image, uses multiboot)
qemu-direct-chirho: kernel-chirho
	qemu-system-x86_64 \
		-serial stdio \
		-display none \
		-m 256M \
		-kernel $(KERNEL_BINARY_CHIRHO) \
		-no-reboot

# Clean build artifacts
clean-chirho:
	cargo clean

# Check the kernel compiles
check-chirho:
	cd kernel-chirho && cargo +nightly check

# Run clippy
clippy-chirho:
	cd kernel-chirho && cargo +nightly clippy -- -D warnings

# Format code
fmt-chirho:
	cargo fmt --all

# Show kernel binary info
info-chirho: kernel-chirho
	rust-objdump -h $(KERNEL_BINARY_CHIRHO)
	rust-size $(KERNEL_BINARY_CHIRHO)
