#!/bin/bash
# For God so loved the world that he gave his only begotten Son,
# that whoever believes in him should not perish but have eternal life. - John 3:16

# P2-005: Create an Alpine Linux ext4 disk image for VirtIO-blk testing.
#
# Downloads Alpine minirootfs x86_64, creates a 256MB ext4 disk image,
# extracts the rootfs into it, and configures /etc/inittab, /etc/fstab,
# and hostname for booting under Lineluya with VirtIO-blk transport.
#
# The resulting image can be attached to QEMU with:
#   -drive file=target/alpine-virtio-chirho/alpine-virtio-chirho.img,format=raw,if=virtio
#
# Usage:
#   ./scripts-chirho/make-alpine-disk-chirho.sh [--version 3.21] [--size 256M]

set -euo pipefail

# ============================================================================
# Configuration constants
# ============================================================================

ALPINE_VERSION_CHIRHO="${ALPINE_VERSION_CHIRHO:-3.21}"
ALPINE_MINOR_CHIRHO="${ALPINE_MINOR_CHIRHO:-0}"
ALPINE_ARCH_CHIRHO="x86_64"
DISK_SIZE_CHIRHO="${DISK_SIZE_CHIRHO:-512M}"

SCRIPT_DIR_CHIRHO="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR_CHIRHO="$(dirname "$SCRIPT_DIR_CHIRHO")"
OUTPUT_DIR_CHIRHO="$PROJECT_DIR_CHIRHO/target/alpine-virtio-chirho"
IMAGE_PATH_CHIRHO="$OUTPUT_DIR_CHIRHO/alpine-virtio-chirho.img"
ROOTFS_DIR_CHIRHO="$OUTPUT_DIR_CHIRHO/rootfs-chirho"
MOUNT_DIR_CHIRHO="$OUTPUT_DIR_CHIRHO/mnt-chirho"

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
            --size)
                DISK_SIZE_CHIRHO="$2"
                shift 2
                ;;
            --help|-h)
                echo "Usage: $0 [--version VERSION] [--size SIZE]"
                echo ""
                echo "  --version   Alpine version (default: $ALPINE_VERSION_CHIRHO)"
                echo "  --size      Disk image size (default: $DISK_SIZE_CHIRHO)"
                echo ""
                echo "Output: $IMAGE_PATH_CHIRHO"
                echo ""
                echo "QEMU usage:"
                echo "  -drive file=$IMAGE_PATH_CHIRHO,format=raw,if=virtio"
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
    echo "[MAKE-ALPINE-DISK] $*"
}

err_chirho() {
    echo "[MAKE-ALPINE-DISK] ERROR: $*" >&2
    exit 1
}

check_deps_chirho() {
    local missing_chirho=0

    for cmd_chirho in curl tar; do
        if ! command -v "$cmd_chirho" &>/dev/null; then
            echo "ERROR: Required command '$cmd_chirho' not found."
            missing_chirho=1
        fi
    done

    # Determine ext4 formatter
    if command -v mkfs.ext4 &>/dev/null; then
        MKFS_CMD_CHIRHO="mkfs.ext4"
    elif command -v mke2fs &>/dev/null; then
        MKFS_CMD_CHIRHO="mke2fs -t ext4"
    else
        err_chirho "No ext4 formatter found (mkfs.ext4 / mke2fs). Install e2fsprogs."
    fi

    # Need mount/umount (via sudo on macOS/Linux) or a Docker fallback
    if ! command -v sudo &>/dev/null; then
        log_chirho "WARNING: sudo not available — will attempt unsudo mount or Docker fallback."
    fi

    if [[ $missing_chirho -ne 0 ]]; then
        exit 1
    fi
}

# ============================================================================
# Step 1: Download Alpine minirootfs
# ============================================================================

download_rootfs_chirho() {
    local tarball_path_chirho="$OUTPUT_DIR_CHIRHO/$TARBALL_NAME_CHIRHO"

    if [[ -f "$tarball_path_chirho" ]]; then
        log_chirho "Tarball already cached: $tarball_path_chirho"
        return
    fi

    log_chirho "Downloading Alpine minirootfs ${ALPINE_VERSION_CHIRHO}.${ALPINE_MINOR_CHIRHO} (${ALPINE_ARCH_CHIRHO})..."
    log_chirho "  URL: $DOWNLOAD_URL_CHIRHO"

    curl -fSL --progress-bar -o "$tarball_path_chirho" "$DOWNLOAD_URL_CHIRHO"

    log_chirho "Download complete: $(du -h "$tarball_path_chirho" | cut -f1)"
}

# ============================================================================
# Step 2: Create empty ext4 disk image
# ============================================================================

create_empty_image_chirho() {
    if [[ -f "$IMAGE_PATH_CHIRHO" ]]; then
        log_chirho "Removing old disk image..."
        rm -f "$IMAGE_PATH_CHIRHO"
    fi

    log_chirho "Creating ${DISK_SIZE_CHIRHO} ext4 disk image..."

    # Create sparse file
    if command -v truncate &>/dev/null; then
        truncate -s "$DISK_SIZE_CHIRHO" "$IMAGE_PATH_CHIRHO"
    else
        # Fallback: dd
        local size_bytes_chirho
        case "$DISK_SIZE_CHIRHO" in
            *M) size_bytes_chirho=$(( ${DISK_SIZE_CHIRHO%M} * 1024 * 1024 )) ;;
            *G) size_bytes_chirho=$(( ${DISK_SIZE_CHIRHO%G} * 1024 * 1024 * 1024 )) ;;
            *)  size_bytes_chirho="$DISK_SIZE_CHIRHO" ;;
        esac
        dd if=/dev/zero of="$IMAGE_PATH_CHIRHO" bs=1 count=0 seek="$size_bytes_chirho" 2>/dev/null
    fi

    # Format as ext4 with label
    $MKFS_CMD_CHIRHO -F -L "lineluya-vblk-chirho" "$IMAGE_PATH_CHIRHO"

    log_chirho "Image formatted: $IMAGE_PATH_CHIRHO"
}

# ============================================================================
# Step 3: Extract Alpine rootfs into the image
# ============================================================================

populate_image_chirho() {
    local tarball_path_chirho="$OUTPUT_DIR_CHIRHO/$TARBALL_NAME_CHIRHO"

    log_chirho "Mounting image and extracting Alpine rootfs..."

    mkdir -p "$MOUNT_DIR_CHIRHO"

    # Mount the image (requires root on Linux; on macOS use hdiutil or Docker)
    if [[ "$(uname)" == "Darwin" ]]; then
        populate_image_darwin_chirho "$tarball_path_chirho"
    else
        populate_image_linux_chirho "$tarball_path_chirho"
    fi
}

populate_image_linux_chirho() {
    local tarball_path_chirho="$1"

    sudo mount -o loop "$IMAGE_PATH_CHIRHO" "$MOUNT_DIR_CHIRHO"

    # Extract rootfs
    sudo tar xzf "$tarball_path_chirho" -C "$MOUNT_DIR_CHIRHO"

    # Configure the rootfs while mounted
    configure_rootfs_chirho "$MOUNT_DIR_CHIRHO"

    # Sync and unmount
    sync
    sudo umount "$MOUNT_DIR_CHIRHO"
    rmdir "$MOUNT_DIR_CHIRHO" 2>/dev/null || true

    log_chirho "Image populated (Linux mount)."
}

populate_image_darwin_chirho() {
    local tarball_path_chirho="$1"

    # On macOS, ext4 mount is not natively supported.
    # Strategy: use a temporary Docker container with ext4 tools.
    if command -v docker &>/dev/null; then
        log_chirho "Using Docker to populate ext4 image on macOS..."

        local abs_image_chirho
        abs_image_chirho="$(cd "$(dirname "$IMAGE_PATH_CHIRHO")" && pwd)/$(basename "$IMAGE_PATH_CHIRHO")"
        local abs_tarball_chirho
        abs_tarball_chirho="$(cd "$(dirname "$tarball_path_chirho")" && pwd)/$(basename "$tarball_path_chirho")"

        # colima on macOS corrupts binary files via volume mounts and docker cp.
        # Workaround: use docker exec to build, then pipe the image out via
        # stdout using docker exec cat.
        log_chirho "Building ext4 image inside Docker container..."

        local cid_chirho
        cid_chirho=$(docker create --privileged alpine:latest sleep 3600)
        docker start "$cid_chirho" >/dev/null

        # Pipe the rootfs tarball into the container via docker exec
        cat "$abs_tarball_chirho" | docker exec -i "$cid_chirho" sh -c 'cat > /tmp/rootfs.tar.gz'

        # Run the build
        docker exec "$cid_chirho" /bin/sh -c '
set -e

# Create a fresh ext4 image inside the container
apk add --no-cache e2fsprogs >/dev/null 2>&1
truncate -s 512M /tmp/disk.img
mkfs.ext4 -F -L "lineluya-vblk" /tmp/disk.img >/dev/null 2>&1

mkdir -p /mnt-chirho
mount -o loop /tmp/disk.img /mnt-chirho

tar xzf /tmp/rootfs.tar.gz -C /mnt-chirho
rm /tmp/rootfs.tar.gz

# Ensure required directories
for d in dev proc sys tmp run; do
    mkdir -p /mnt-chirho/$d
done

# /etc/inittab for BusyBox init
cat > /mnt-chirho/etc/inittab << '\''INITTAB_CHIRHO'\''
# For God so loved the world that he gave his only begotten Son,
# that whoever believes in him should not perish but have eternal life. - John 3:16
#
# Lineluya Alpine inittab (VirtIO-blk boot)
::sysinit:/bin/mount -t proc proc /proc
::sysinit:/bin/mount -t sysfs sysfs /sys
::sysinit:/bin/mount -t devtmpfs devtmpfs /dev
::sysinit:/bin/mkdir -p /dev/pts
::sysinit:/bin/mount -t devpts devpts /dev/pts
::sysinit:/bin/mount -o remount,rw /
::sysinit:/bin/hostname -F /etc/hostname
ttyS0::respawn:/sbin/getty -L ttyS0 115200 vt100
tty1::respawn:/sbin/getty 38400 tty1
::shutdown:/bin/umount -a -r
::ctrlaltdel:/sbin/reboot
INITTAB_CHIRHO

# /etc/fstab — VirtIO disk is /dev/vda
cat > /mnt-chirho/etc/fstab << '\''FSTAB_CHIRHO'\''
# For God so loved the world that he gave his only begotten Son,
# that whoever believes in him should not perish but have eternal life. - John 3:16
#
# Lineluya Alpine fstab (VirtIO-blk)
/dev/vda    /           ext4    rw,relatime     0 1
proc        /proc       proc    defaults        0 0
sysfs       /sys        sysfs   defaults        0 0
devtmpfs    /dev        devtmpfs defaults       0 0
tmpfs       /tmp        tmpfs   defaults        0 0
tmpfs       /run        tmpfs   defaults        0 0
FSTAB_CHIRHO

# hostname
echo "lineluya-chirho" > /mnt-chirho/etc/hostname

# resolv.conf
cat > /mnt-chirho/etc/resolv.conf << '\''RESOLV_CHIRHO'\''
nameserver 8.8.8.8
nameserver 1.1.1.1
RESOLV_CHIRHO

# Enable ttyS0 in securetty
if [ -f /mnt-chirho/etc/securetty ]; then
    grep -q ttyS0 /mnt-chirho/etc/securetty || echo ttyS0 >> /mnt-chirho/etc/securetty
fi

# Set root password empty for dev
if [ -f /mnt-chirho/etc/shadow ]; then
    sed -i "s|^root:.*:|root:::|" /mnt-chirho/etc/shadow
fi

# ---------------------------------------------------------------
# P5: Pre-install Alpine packages for Lineluya testing
# These bypass the need for networking (apk) inside the kernel.
# ---------------------------------------------------------------
echo "[DOCKER] Installing P5 packages into rootfs..." >&2

# Install packages into the rootfs using the HOST apk (which matches
# the container arch).  We use --allow-untrusted because the host keys
# may not match the target repo version. We use --arch x86_64 to ensure
# we get x86_64 packages even if running on ARM64.
# Use the HOST repos (same Alpine version as container) with x86_64 arch.
HOST_VER=$(cat /etc/alpine-release | cut -d. -f1-2)
mkdir -p /mnt-chirho/etc/apk/keys
cp -a /etc/apk/keys/* /mnt-chirho/etc/apk/keys/ 2>/dev/null || true

cat > /tmp/repos-chirho << REPOS_CHIRHO
https://dl-cdn.alpinelinux.org/alpine/v${HOST_VER}/main
https://dl-cdn.alpinelinux.org/alpine/v${HOST_VER}/community
REPOS_CHIRHO

apk --root /mnt-chirho --initdb \
    --arch x86_64 \
    --allow-untrusted \
    --repositories-file /tmp/repos-chirho \
    add \
    sqlite sqlite-libs \
    python3 \
    dropbear dropbear-scp \
    mpg123 \
    xterm \
    >&2 2>&1

# loop.ko is injected after disk build via inject-loop-ko-chirho.sh

# Verify installation
ls /mnt-chirho/usr/bin/sqlite3 >/dev/null 2>&1 && echo "[DOCKER] sqlite3 INSTALLED" >&2 || echo "[DOCKER] sqlite3 MISSING" >&2
ls /mnt-chirho/usr/bin/python3 >/dev/null 2>&1 && echo "[DOCKER] python3 INSTALLED" >&2 || echo "[DOCKER] python3 MISSING" >&2

echo "[DOCKER] P5 packages installed into rootfs." >&2

# Create /etc/profile to auto-start dropbear SSH on shell login
cat > /mnt-chirho/etc/profile << 'PROFILE_CHIRHO'
# For God so loved the world that he gave his only begotten Son,
# that whoever believes in him should not perish but have eternal life. - John 3:16

# Auto-start dropbear SSH server on port 2222
if [ ! -f /tmp/.dropbear_started ]; then
    dropbear -p 2222 -B -R 2>/dev/null &
    touch /tmp/.dropbear_started
    echo "[SSH] Dropbear started on port 2222"
fi
PROFILE_CHIRHO
chmod 644 /mnt-chirho/etc/profile
echo "[DOCKER] /etc/profile created (auto-starts dropbear)" >&2

# Create a test Python script
cat > /mnt-chirho/root/hello_chirho.py << '\''PYTEST_CHIRHO'\''
# For God so loved the world that he gave his only begotten Son,
# that whoever believes in him should not perish but have eternal life. - John 3:16
print("Hallelujah! Python3 runs on Lineluya!")
print("For God so loved the world - John 3:16")
PYTEST_CHIRHO

# Create a test SQLite script
cat > /mnt-chirho/root/test_chirho.sql << '\''SQLTEST_CHIRHO'\''
-- For God so loved the world that he gave his only begotten Son,
-- that whoever believes in him should not perish but have eternal life. - John 3:16
CREATE TABLE praise_chirho (id_chirho INTEGER PRIMARY KEY, msg_chirho TEXT);
INSERT INTO praise_chirho VALUES (1, 'Hallelujah! SQLite runs on Lineluya!');
SELECT * FROM praise_chirho;
SQLTEST_CHIRHO

sync
umount /mnt-chirho
echo "[DOCKER] rootfs populated and configured."

echo "[DOCKER] Build complete."
'
        # Extract the finished image via docker cp (not pipe/cat which corrupts on macOS)
        log_chirho "Extracting disk image from container via docker cp..."
        docker cp "$cid_chirho:/tmp/disk.img" "$abs_image_chirho"

        # Clean up container
        docker stop "$cid_chirho" >/dev/null 2>&1 || true
        docker rm "$cid_chirho" >/dev/null 2>&1 || true

        log_chirho "Image populated via Docker ($(du -h "$abs_image_chirho" | cut -f1))."
    else
        # Fallback: extract to a directory, user can manually dd or use Linux
        log_chirho "WARNING: No Docker available on macOS — cannot mount ext4 natively."
        log_chirho "Extracting rootfs to directory instead: $ROOTFS_DIR_CHIRHO"
        mkdir -p "$ROOTFS_DIR_CHIRHO"
        tar xzf "$tarball_path_chirho" -C "$ROOTFS_DIR_CHIRHO"
        configure_rootfs_chirho "$ROOTFS_DIR_CHIRHO"
        log_chirho "Rootfs extracted to $ROOTFS_DIR_CHIRHO (image NOT populated)."
        log_chirho "To populate the image, run this script on Linux or install Docker."
    fi
}

# ============================================================================
# Step 4: Configure rootfs for Lineluya VirtIO boot
# ============================================================================

configure_rootfs_chirho() {
    local root_chirho="$1"
    local sudo_prefix_chirho=""

    # Use sudo if the target is owned by root
    if [[ -d "$root_chirho/bin" ]] && [[ "$(stat -f '%u' "$root_chirho/bin" 2>/dev/null || stat -c '%u' "$root_chirho/bin" 2>/dev/null)" == "0" ]]; then
        sudo_prefix_chirho="sudo"
    fi

    log_chirho "Configuring rootfs at $root_chirho ..."

    # Ensure required directories
    for dir_chirho in dev proc sys tmp run; do
        $sudo_prefix_chirho mkdir -p "$root_chirho/$dir_chirho"
    done

    # /etc/inittab for BusyBox init
    $sudo_prefix_chirho tee "$root_chirho/etc/inittab" > /dev/null << 'INITTAB_EOF_CHIRHO'
# For God so loved the world that he gave his only begotten Son,
# that whoever believes in him should not perish but have eternal life. - John 3:16
#
# Lineluya Alpine inittab (VirtIO-blk boot)
::sysinit:/bin/mount -t proc proc /proc
::sysinit:/bin/mount -t sysfs sysfs /sys
::sysinit:/bin/mount -t devtmpfs devtmpfs /dev
::sysinit:/bin/mkdir -p /dev/pts
::sysinit:/bin/mount -t devpts devpts /dev/pts
::sysinit:/bin/mount -o remount,rw /
::sysinit:/bin/hostname -F /etc/hostname
ttyS0::respawn:/sbin/getty -L ttyS0 115200 vt100
tty1::respawn:/sbin/getty 38400 tty1
::shutdown:/bin/umount -a -r
::ctrlaltdel:/sbin/reboot
INITTAB_EOF_CHIRHO
    log_chirho "  Created /etc/inittab"

    # /etc/fstab — VirtIO block disk appears as /dev/vda
    $sudo_prefix_chirho tee "$root_chirho/etc/fstab" > /dev/null << 'FSTAB_EOF_CHIRHO'
# For God so loved the world that he gave his only begotten Son,
# that whoever believes in him should not perish but have eternal life. - John 3:16
#
# Lineluya Alpine fstab (VirtIO-blk)
/dev/vda    /           ext4    rw,relatime     0 1
proc        /proc       proc    defaults        0 0
sysfs       /sys        sysfs   defaults        0 0
devtmpfs    /dev        devtmpfs defaults       0 0
tmpfs       /tmp        tmpfs   defaults        0 0
tmpfs       /run        tmpfs   defaults        0 0
FSTAB_EOF_CHIRHO
    log_chirho "  Created /etc/fstab (root=/dev/vda for VirtIO)"

    # hostname
    echo "lineluya-chirho" | $sudo_prefix_chirho tee "$root_chirho/etc/hostname" > /dev/null
    log_chirho "  Set hostname: lineluya-chirho"

    # DNS
    $sudo_prefix_chirho tee "$root_chirho/etc/resolv.conf" > /dev/null << 'RESOLV_EOF_CHIRHO'
nameserver 8.8.8.8
nameserver 1.1.1.1
RESOLV_EOF_CHIRHO
    log_chirho "  Created /etc/resolv.conf"

    # Enable serial console in securetty
    if [[ -f "$root_chirho/etc/securetty" ]]; then
        if ! grep -q ttyS0 "$root_chirho/etc/securetty"; then
            echo "ttyS0" | $sudo_prefix_chirho tee -a "$root_chirho/etc/securetty" > /dev/null
            log_chirho "  Added ttyS0 to /etc/securetty"
        fi
    fi

    # Set root password to empty for dev
    if [[ -f "$root_chirho/etc/shadow" ]]; then
        $sudo_prefix_chirho sed -i.bak 's|^root:.*:|root:::|' "$root_chirho/etc/shadow" 2>/dev/null || true
        log_chirho "  Set root password to empty (dev mode)"
    fi

    log_chirho "Rootfs configuration complete."
}

# ============================================================================
# Summary
# ============================================================================

print_summary_chirho() {
    echo ""
    echo "============================================================"
    echo "  Lineluya Alpine VirtIO-blk Disk Image (P2-005)"
    echo "============================================================"
    echo ""
    echo "  Alpine version:   ${ALPINE_VERSION_CHIRHO}.${ALPINE_MINOR_CHIRHO}"
    echo "  Architecture:     $ALPINE_ARCH_CHIRHO"
    echo "  Image size:       $DISK_SIZE_CHIRHO"
    echo "  Image path:       $IMAGE_PATH_CHIRHO"
    echo "  Root device:      /dev/vda (VirtIO-blk)"
    echo "  Hostname:         lineluya-chirho"
    echo ""
    echo "  Pre-installed: sqlite3, python3, dropbear"
    echo ""
    echo "  QEMU usage:"
    echo "    qemu-system-x86_64 \\"
    echo "      -drive file=$IMAGE_PATH_CHIRHO,format=raw,if=virtio \\"
    echo "      -kernel target/x86_64-lineluya-chirho/release/lineluya-chirho \\"
    echo "      -append 'root=/dev/vda rw console=ttyS0 init=/sbin/init' \\"
    echo "      -serial stdio -nographic -m 512M"
    echo ""
    echo "============================================================"
}

# ============================================================================
# Main
# ============================================================================

main_chirho() {
    parse_args_chirho "$@"

    echo "=== Lineluya Alpine VirtIO-blk Disk Builder (P2-005) ==="
    echo "For God so loved the world that he gave his only begotten Son,"
    echo "that whoever believes in him should not perish but have eternal life. - John 3:16"
    echo ""

    check_deps_chirho
    mkdir -p "$OUTPUT_DIR_CHIRHO"

    download_rootfs_chirho
    create_empty_image_chirho
    populate_image_chirho
    print_summary_chirho
}

main_chirho "$@"
