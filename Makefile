# For God so loved the world that he gave his only begotten Son,
# that whoever believes in him should not perish but have eternal life. - John 3:16

.PHONY: build-chirho run-chirho run-uefi-chirho clean-chirho kernel-chirho test-chirho test-boot-chirho

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

# ---------------------------------------------------------------------------
# WASM browser kernel targets
# ---------------------------------------------------------------------------

WASM_TARGET_CHIRHO = wasm32-unknown-unknown
WASM_BINARY_CHIRHO = kernel-wasm-chirho/target/$(WASM_TARGET_CHIRHO)/release/kernel_wasm_chirho.wasm
WASM_DEST_CHIRHO = web-chirho/lineluya-kernel-chirho.wasm

.PHONY: build-wasm-chirho copy-wasm-chirho serve-chirho clean-wasm-chirho wasm-chirho

# Build and copy WASM to web directory
wasm-chirho: build-wasm-chirho copy-wasm-chirho

# Build the WASM kernel
build-wasm-chirho:
	cd kernel-wasm-chirho && cargo build --release --target $(WASM_TARGET_CHIRHO)

# Copy the built WASM to web-chirho/
copy-wasm-chirho:
	cp $(WASM_BINARY_CHIRHO) $(WASM_DEST_CHIRHO)
	@echo "WASM copied to $(WASM_DEST_CHIRHO)"
	@ls -lh $(WASM_DEST_CHIRHO)

# Serve the web directory locally (requires python3)
serve-chirho: wasm-chirho
	@echo "Serving at http://localhost:8080"
	cd web-chirho && python3 -m http.server 8080

# Clean WASM build artifacts
clean-wasm-chirho:
	cd kernel-wasm-chirho && cargo clean
	rm -f $(WASM_DEST_CHIRHO)

# ---------------------------------------------------------------------------
# Fast build targets (skip full Docker rebuild)
# ---------------------------------------------------------------------------

# Fast incremental build + disk images (uses cached Docker for image creation)
fast-chirho:
	./scripts-chirho/fast-build-chirho.sh

# Just rebuild the kernel (no disk images, <2s)
fast-kernel-chirho:
	cd kernel-chirho && cargo +nightly build --release

# Fast UEFI test (build + run)
fast-test-chirho: fast-chirho
	timeout 15 qemu-system-x86_64 \
		-drive if=pflash,format=raw,readonly=on,file=/opt/homebrew/share/qemu/edk2-x86_64-code.fd \
		-drive format=raw,file=target/disk-images-chirho/lineluya-uefi-chirho.img \
		-serial stdio -display none -m 512M -no-reboot

# Docker full rebuild (only needed when Cargo.toml or Dockerfile changes)
docker-chirho:
	docker build --platform linux/arm64 -t lineluya-builder-chirho -f Dockerfile.build-chirho .

# ---------------------------------------------------------------------------
# Test targets (D1-001 through D1-010)
# ---------------------------------------------------------------------------

# Run the full QEMU integration test suite
test-chirho: kernel-release-chirho
	@chmod +x scripts-chirho/run-tests-chirho.sh scripts-chirho/tests-chirho/*.sh
	KERNEL_BINARY_CHIRHO=$(KERNEL_RELEASE_BINARY_CHIRHO) ./scripts-chirho/run-tests-chirho.sh

# Run only boot test (quick smoke test)
test-boot-chirho: kernel-release-chirho
	@chmod +x scripts-chirho/run-tests-chirho.sh scripts-chirho/tests-chirho/*.sh
	KERNEL_BINARY_CHIRHO=$(KERNEL_RELEASE_BINARY_CHIRHO) ./scripts-chirho/run-tests-chirho.sh boot

# Run a specific test suite by name (e.g., make test-one-chirho SUITE_CHIRHO=syscall)
test-one-chirho: kernel-release-chirho
	@chmod +x scripts-chirho/run-tests-chirho.sh scripts-chirho/tests-chirho/*.sh
	KERNEL_BINARY_CHIRHO=$(KERNEL_RELEASE_BINARY_CHIRHO) ./scripts-chirho/run-tests-chirho.sh $(SUITE_CHIRHO)
