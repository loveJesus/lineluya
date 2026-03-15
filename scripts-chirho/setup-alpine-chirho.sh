#!/bin/bash
# For God so loved the world that he gave his only begotten Son,
# that whoever believes in him should not perish but have eternal life. - John 3:16

# G1-001: Alpine Linux minirootfs setup script for Lineluya kernel
#
# Downloads Alpine Linux minirootfs and creates an ext4 disk image
# suitable for booting under Lineluya (or QEMU with Lineluya kernel).
#
# Usage:
#   ./scripts-chirho/setup-alpine-chirho.sh [--version 3.21] [--arch x86_64] [--size 512M]

set -euo pipefail

# ============================================================================
# Configuration constants
# ============================================================================

ALPINE_VERSION_CHIRHO="${ALPINE_VERSION_CHIRHO:-3.21}"
ALPINE_MINOR_CHIRHO="${ALPINE_MINOR_CHIRHO:-0}"
ALPINE_ARCH_CHIRHO="${ALPINE_ARCH_CHIRHO:-x86_64}"
DISK_SIZE_CHIRHO="${DISK_SIZE_CHIRHO:-512M}"

SCRIPT_DIR_CHIRHO="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR_CHIRHO="$(dirname "$SCRIPT_DIR_CHIRHO")"
OUTPUT_DIR_CHIRHO="$PROJECT_DIR_CHIRHO/target/alpine-rootfs-chirho"
IMAGE_PATH_CHIRHO="$OUTPUT_DIR_CHIRHO/alpine-rootfs-chirho.img"
ROOTFS_DIR_CHIRHO="$OUTPUT_DIR_CHIRHO/rootfs-chirho"

# Alpine download mirror
MIRROR_URL_CHIRHO="https://dl-cdn.alpinelinux.org/alpine"
TARBALL_NAME_CHIRHO="alpine-minirootfs-${ALPINE_VERSION_CHIRHO}.${ALPINE_MINOR_CHIRHO}-${ALPINE_ARCH_CHIRHO}.tar.gz"
DOWNLOAD_URL_CHIRHO="${MIRROR_URL_CHIRHO}/v${ALPINE_VERSION_CHIRHO}/releases/${ALPINE_ARCH_CHIRHO}/${TARBALL_NAME_CHIRHO}"

# ============================================================================
# Argument parsing
# ============================================================================

parse_args_chirho() {
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --version)
                ALPINE_VERSION_CHIRHO="$2"
                shift 2
                ;;
            --arch)
                ALPINE_ARCH_CHIRHO="$2"
                shift 2
                ;;
            --size)
                DISK_SIZE_CHIRHO="$2"
                shift 2
                ;;
            --help|-h)
                echo "Usage: $0 [--version VERSION] [--arch ARCH] [--size SIZE]"
                echo ""
                echo "  --version   Alpine version (default: $ALPINE_VERSION_CHIRHO)"
                echo "  --arch      Architecture (default: $ALPINE_ARCH_CHIRHO)"
                echo "  --size      Disk image size (default: $DISK_SIZE_CHIRHO)"
                exit 0
                ;;
            *)
                echo "Unknown option: $1"
                exit 1
                ;;
        esac
    done
}

# ============================================================================
# Utility functions
# ============================================================================

log_chirho() {
    echo "[ALPINE-SETUP] $*"
}

check_deps_chirho() {
    local missing_chirho=0
    for cmd_chirho in curl tar; do
        if ! command -v "$cmd_chirho" &>/dev/null; then
            echo "ERROR: Required command '$cmd_chirho' not found."
            missing_chirho=1
        fi
    done

    # Check for disk image creation tools
    if command -v mkfs.ext4 &>/dev/null; then
        MKFS_CMD_CHIRHO="mkfs.ext4"
    elif command -v mke2fs &>/dev/null; then
        MKFS_CMD_CHIRHO="mke2fs -t ext4"
    else
        echo "WARNING: No ext4 formatter found (mkfs.ext4 / mke2fs)."
        echo "         Will create raw tarball extraction only (no disk image)."
        MKFS_CMD_CHIRHO=""
    fi

    if [[ $missing_chirho -ne 0 ]]; then
        exit 1
    fi
}

# ============================================================================
# Download Alpine minirootfs
# ============================================================================

download_rootfs_chirho() {
    local tarball_path_chirho="$OUTPUT_DIR_CHIRHO/$TARBALL_NAME_CHIRHO"

    if [[ -f "$tarball_path_chirho" ]]; then
        log_chirho "Tarball already exists: $tarball_path_chirho"
        return
    fi

    log_chirho "Downloading Alpine minirootfs..."
    log_chirho "  URL: $DOWNLOAD_URL_CHIRHO"

    curl -fSL -o "$tarball_path_chirho" "$DOWNLOAD_URL_CHIRHO"

    log_chirho "Download complete: $(du -h "$tarball_path_chirho" | cut -f1)"
}

# ============================================================================
# Extract rootfs
# ============================================================================

extract_rootfs_chirho() {
    local tarball_path_chirho="$OUTPUT_DIR_CHIRHO/$TARBALL_NAME_CHIRHO"

    if [[ -d "$ROOTFS_DIR_CHIRHO/bin" ]]; then
        log_chirho "Rootfs already extracted at $ROOTFS_DIR_CHIRHO"
        return
    fi

    log_chirho "Extracting rootfs to $ROOTFS_DIR_CHIRHO ..."
    mkdir -p "$ROOTFS_DIR_CHIRHO"

    # Use sudo if available for proper ownership; fall back to fakeroot or plain tar
    if command -v sudo &>/dev/null; then
        sudo tar xzf "$tarball_path_chirho" -C "$ROOTFS_DIR_CHIRHO"
    elif command -v fakeroot &>/dev/null; then
        fakeroot tar xzf "$tarball_path_chirho" -C "$ROOTFS_DIR_CHIRHO"
    else
        tar xzf "$tarball_path_chirho" -C "$ROOTFS_DIR_CHIRHO"
    fi

    log_chirho "Rootfs extracted. Key contents:"
    ls -la "$ROOTFS_DIR_CHIRHO/" | head -20
}

# ============================================================================
# Configure rootfs for Lineluya
# ============================================================================

configure_rootfs_chirho() {
    log_chirho "Configuring rootfs for Lineluya boot..."

    # Ensure /dev, /proc, /sys, /tmp, /run exist
    for dir_chirho in dev proc sys tmp run; do
        mkdir -p "$ROOTFS_DIR_CHIRHO/$dir_chirho"
    done

    # Create a minimal /etc/inittab if not present (BusyBox init)
    if [[ ! -f "$ROOTFS_DIR_CHIRHO/etc/inittab" ]]; then
        cat > "$ROOTFS_DIR_CHIRHO/etc/inittab" << 'INITTAB_EOF_CHIRHO'
# For God so loved the world that he gave his only begotten Son,
# that whoever believes in him should not perish but have eternal life. - John 3:16
#
# Lineluya Alpine inittab
::sysinit:/sbin/openrc sysinit
::sysinit:/sbin/openrc boot
::sysinit:/sbin/openrc default
# Console login
ttyS0::respawn:/sbin/getty -L ttyS0 115200 vt100
tty1::respawn:/sbin/getty 38400 tty1
# Shutdown
::shutdown:/sbin/openrc shutdown
::ctrlaltdel:/sbin/reboot
INITTAB_EOF_CHIRHO
        log_chirho "  Created /etc/inittab"
    fi

    # Set hostname
    echo "lineluya-chirho" > "$ROOTFS_DIR_CHIRHO/etc/hostname"

    # Create /etc/fstab
    cat > "$ROOTFS_DIR_CHIRHO/etc/fstab" << 'FSTAB_EOF_CHIRHO'
# For God so loved the world that he gave his only begotten Son,
# that whoever believes in him should not perish but have eternal life. - John 3:16
#
# Lineluya Alpine fstab
/dev/sda    /           ext4    rw,relatime     0 1
proc        /proc       proc    defaults        0 0
sysfs       /sys        sysfs   defaults        0 0
devtmpfs    /dev        devtmpfs defaults       0 0
tmpfs       /tmp        tmpfs   defaults        0 0
tmpfs       /run        tmpfs   defaults        0 0
FSTAB_EOF_CHIRHO
    log_chirho "  Created /etc/fstab"

    # Enable serial console in securetty
    if [[ -f "$ROOTFS_DIR_CHIRHO/etc/securetty" ]]; then
        if ! grep -q ttyS0 "$ROOTFS_DIR_CHIRHO/etc/securetty"; then
            echo "ttyS0" >> "$ROOTFS_DIR_CHIRHO/etc/securetty"
            log_chirho "  Added ttyS0 to /etc/securetty"
        fi
    fi

    # Set root password to empty (login without password for dev)
    if [[ -f "$ROOTFS_DIR_CHIRHO/etc/shadow" ]]; then
        sed -i.bak 's|^root:.*:|root:::|' "$ROOTFS_DIR_CHIRHO/etc/shadow" 2>/dev/null || true
        log_chirho "  Set root password to empty (dev mode)"
    fi

    # DNS configuration
    cat > "$ROOTFS_DIR_CHIRHO/etc/resolv.conf" << 'RESOLV_EOF_CHIRHO'
nameserver 8.8.8.8
nameserver 1.1.1.1
RESOLV_EOF_CHIRHO
    log_chirho "  Created /etc/resolv.conf"

    log_chirho "Rootfs configuration complete."
}

# ============================================================================
# Create ext4 disk image
# ============================================================================

create_disk_image_chirho() {
    if [[ -z "${MKFS_CMD_CHIRHO:-}" ]]; then
        log_chirho "Skipping disk image creation (no ext4 tools available)."
        log_chirho "Rootfs directory is ready at: $ROOTFS_DIR_CHIRHO"
        return
    fi

    log_chirho "Creating ext4 disk image ($DISK_SIZE_CHIRHO)..."

    # Create empty image file
    if command -v truncate &>/dev/null; then
        truncate -s "$DISK_SIZE_CHIRHO" "$IMAGE_PATH_CHIRHO"
    else
        # Fallback: use dd
        local size_bytes_chirho
        case "$DISK_SIZE_CHIRHO" in
            *M) size_bytes_chirho=$(( ${DISK_SIZE_CHIRHO%M} * 1024 * 1024 )) ;;
            *G) size_bytes_chirho=$(( ${DISK_SIZE_CHIRHO%G} * 1024 * 1024 * 1024 )) ;;
            *)  size_bytes_chirho="$DISK_SIZE_CHIRHO" ;;
        esac
        dd if=/dev/zero of="$IMAGE_PATH_CHIRHO" bs=1 count=0 seek="$size_bytes_chirho" 2>/dev/null
    fi

    # Format as ext4
    $MKFS_CMD_CHIRHO -F -L "lineluya-root-chirho" "$IMAGE_PATH_CHIRHO"

    # Mount and copy rootfs
    local mount_dir_chirho="$OUTPUT_DIR_CHIRHO/mnt-chirho"
    mkdir -p "$mount_dir_chirho"

    if command -v sudo &>/dev/null; then
        sudo mount -o loop "$IMAGE_PATH_CHIRHO" "$mount_dir_chirho"
        sudo cp -a "$ROOTFS_DIR_CHIRHO"/. "$mount_dir_chirho"/
        sudo umount "$mount_dir_chirho"
    else
        log_chirho "WARNING: Cannot mount image without sudo. Image is formatted but empty."
        log_chirho "         Run with sudo to populate the disk image, or use the rootfs directory."
    fi

    rmdir "$mount_dir_chirho" 2>/dev/null || true

    log_chirho "Disk image created: $IMAGE_PATH_CHIRHO"
    log_chirho "  Size: $(du -h "$IMAGE_PATH_CHIRHO" | cut -f1)"
}

# ============================================================================
# Verify dynamic linker presence
# ============================================================================

verify_musl_ld_chirho() {
    log_chirho "Verifying musl dynamic linker..."

    local ld_path_chirho="$ROOTFS_DIR_CHIRHO/lib/ld-musl-x86_64.so.1"
    if [[ -f "$ld_path_chirho" ]] || [[ -L "$ld_path_chirho" ]]; then
        log_chirho "  Found: $ld_path_chirho"
        if command -v file &>/dev/null; then
            log_chirho "  Type: $(file "$ld_path_chirho")"
        fi
        if command -v readelf &>/dev/null; then
            log_chirho "  ELF info:"
            readelf -h "$ld_path_chirho" 2>/dev/null | grep -E "Type:|Entry|Machine" || true
        fi
    else
        log_chirho "  WARNING: ld-musl-x86_64.so.1 not found at expected path"
        log_chirho "  Searching..."
        find "$ROOTFS_DIR_CHIRHO" -name "ld-musl*" -o -name "libc.musl*" 2>/dev/null || true
    fi
}

# ============================================================================
# Summary
# ============================================================================

print_summary_chirho() {
    echo ""
    echo "============================================================"
    echo "  Lineluya Alpine Linux Rootfs Setup Complete"
    echo "============================================================"
    echo ""
    echo "  Alpine version:   ${ALPINE_VERSION_CHIRHO}.${ALPINE_MINOR_CHIRHO}"
    echo "  Architecture:     $ALPINE_ARCH_CHIRHO"
    echo "  Rootfs directory: $ROOTFS_DIR_CHIRHO"
    if [[ -f "$IMAGE_PATH_CHIRHO" ]]; then
        echo "  Disk image:       $IMAGE_PATH_CHIRHO"
    fi
    echo ""
    echo "  To boot with QEMU:"
    echo "    ./scripts-chirho/qemu-alpine-chirho.sh"
    echo ""
    echo "  Dynamic linker:   /lib/ld-musl-x86_64.so.1"
    echo "  Init system:      BusyBox init -> OpenRC"
    echo "============================================================"
}

# ============================================================================
# Main
# ============================================================================

main_chirho() {
    parse_args_chirho "$@"

    echo "=== Lineluya Alpine Linux Rootfs Setup (G1-001) ==="
    echo "For God so loved the world... - John 3:16"
    echo ""

    check_deps_chirho
    mkdir -p "$OUTPUT_DIR_CHIRHO"

    download_rootfs_chirho
    extract_rootfs_chirho
    configure_rootfs_chirho
    create_disk_image_chirho
    verify_musl_ld_chirho
    print_summary_chirho
}

main_chirho "$@"
