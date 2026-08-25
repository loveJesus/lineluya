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
DISK_SIZE_CHIRHO="${DISK_SIZE_CHIRHO:-1024M}"
FILESYSTEM_LABEL_CHIRHO="lineluya-chirho"

SCRIPT_DIR_CHIRHO="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR_CHIRHO="$(dirname "$SCRIPT_DIR_CHIRHO")"
OUTPUT_DIR_CHIRHO="$PROJECT_DIR_CHIRHO/target/alpine-virtio-chirho"
IMAGE_PATH_CHIRHO="$OUTPUT_DIR_CHIRHO/alpine-virtio-chirho.img"
ROOTFS_DIR_CHIRHO="$OUTPUT_DIR_CHIRHO/rootfs-chirho"
MOUNT_DIR_CHIRHO="$OUTPUT_DIR_CHIRHO/mnt-chirho"
ROOTFS_ASSET_DIR_CHIRHO="$SCRIPT_DIR_CHIRHO/rootfs-chirho"
XGEARS_SOURCE_PATH_CHIRHO="$PROJECT_DIR_CHIRHO/userspace-chirho/x11-chirho/xgears_chirho.c"

ACTIVE_DOCKER_CID_CHIRHO=""
NATIVE_IMAGE_MOUNTED_CHIRHO=0
NATIVE_DEV_MOUNTED_CHIRHO=0
NATIVE_PROC_MOUNTED_CHIRHO=0
ROOTFS_BUILD_MODE_CHIRHO="unknown-chirho"

# Alpine download mirror
MIRROR_URL_CHIRHO="https://dl-cdn.alpinelinux.org/alpine"
TARBALL_NAME_CHIRHO="alpine-minirootfs-${ALPINE_VERSION_CHIRHO}.${ALPINE_MINOR_CHIRHO}-${ALPINE_ARCH_CHIRHO}.tar.gz"
DOWNLOAD_URL_CHIRHO="${MIRROR_URL_CHIRHO}/v${ALPINE_VERSION_CHIRHO}/releases/${ALPINE_ARCH_CHIRHO}/${TARBALL_NAME_CHIRHO}"
CHECKSUM_URL_CHIRHO="$DOWNLOAD_URL_CHIRHO.sha256"
BUILD_MANIFEST_PATH_CHIRHO="$OUTPUT_DIR_CHIRHO/rootfs-build-manifest-chirho.txt"

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

run_as_root_chirho() {
    if [[ "$EUID" -eq 0 ]]; then
        "$@"
    elif command -v sudo &>/dev/null; then
        sudo "$@"
    else
        err_chirho "root privileges are required for: $*"
    fi
}

sha256_file_chirho() {
    local file_chirho="$1"

    if command -v sha256sum &>/dev/null; then
        sha256sum "$file_chirho" | cut -d' ' -f1
    elif command -v shasum &>/dev/null; then
        shasum -a 256 "$file_chirho" | cut -d' ' -f1
    else
        err_chirho "sha256sum or shasum is required"
    fi
}

cleanup_resources_chirho() {
    if [[ "$NATIVE_PROC_MOUNTED_CHIRHO" -eq 1 ]]; then
        run_as_root_chirho umount "$MOUNT_DIR_CHIRHO/proc" 2>/dev/null || true
        NATIVE_PROC_MOUNTED_CHIRHO=0
    fi
    if [[ "$NATIVE_DEV_MOUNTED_CHIRHO" -eq 1 ]]; then
        run_as_root_chirho umount "$MOUNT_DIR_CHIRHO/dev" 2>/dev/null || true
        NATIVE_DEV_MOUNTED_CHIRHO=0
    fi
    if [[ "$NATIVE_IMAGE_MOUNTED_CHIRHO" -eq 1 ]]; then
        run_as_root_chirho umount "$MOUNT_DIR_CHIRHO" 2>/dev/null || true
        NATIVE_IMAGE_MOUNTED_CHIRHO=0
    fi
    if [[ -n "$ACTIVE_DOCKER_CID_CHIRHO" ]]; then
        docker stop "$ACTIVE_DOCKER_CID_CHIRHO" >/dev/null 2>&1 || true
        docker rm "$ACTIVE_DOCKER_CID_CHIRHO" >/dev/null 2>&1 || true
        ACTIVE_DOCKER_CID_CHIRHO=""
    fi
}

stage_rootfs_build_inputs_chirho() {
    # Workflow: spec-chirho/workflows-chirho/x11-bringup-chirho.md
    local root_chirho="$1"
    local stage_dir_chirho="$root_chirho/tmp/lineluya-rootfs-build-chirho"

    run_as_root_chirho mkdir -p "$stage_dir_chirho"
    run_as_root_chirho cp \
        "$ROOTFS_ASSET_DIR_CHIRHO/provision-alpine-rootfs-chirho.sh" \
        "$ROOTFS_ASSET_DIR_CHIRHO/profile-chirho" \
        "$ROOTFS_ASSET_DIR_CHIRHO/start-lineluya-desktop-chirho.sh" \
        "$ROOTFS_ASSET_DIR_CHIRHO/xorg-chirho.conf" \
        "$XGEARS_SOURCE_PATH_CHIRHO" \
        "$stage_dir_chirho/"
}

check_deps_chirho() {
    local missing_chirho=0

    for cmd_chirho in curl tar; do
        if ! command -v "$cmd_chirho" &>/dev/null; then
            echo "ERROR: Required command '$cmd_chirho' not found."
            missing_chirho=1
        fi
    done

    if ! command -v sha256sum &>/dev/null \
        && ! command -v shasum &>/dev/null; then
        echo "ERROR: Required SHA-256 tool (sha256sum or shasum) not found."
        missing_chirho=1
    fi

    for asset_chirho in \
        "$ROOTFS_ASSET_DIR_CHIRHO/provision-alpine-rootfs-chirho.sh" \
        "$ROOTFS_ASSET_DIR_CHIRHO/profile-chirho" \
        "$ROOTFS_ASSET_DIR_CHIRHO/start-lineluya-desktop-chirho.sh" \
        "$ROOTFS_ASSET_DIR_CHIRHO/xorg-chirho.conf" \
        "$XGEARS_SOURCE_PATH_CHIRHO"
    do
        if [[ ! -s "$asset_chirho" ]]; then
            echo "ERROR: Required rootfs source '$asset_chirho' is missing or empty."
            missing_chirho=1
        fi
    done

    # Determine ext4 formatter.  Only the native loop-mount path needs one on
    # the host; the Docker path formats inside the container, so do not make a
    # missing host e2fsprogs a hard failure when Docker is available.
    if command -v mkfs.ext4 &>/dev/null; then
        MKFS_CMD_CHIRHO="mkfs.ext4"
    elif command -v mke2fs &>/dev/null; then
        MKFS_CMD_CHIRHO="mke2fs -t ext4"
    elif command -v docker &>/dev/null; then
        MKFS_CMD_CHIRHO=""
        log_chirho "No host ext4 formatter - Docker will format inside the container."
    else
        err_chirho "No ext4 formatter found (mkfs.ext4 / mke2fs). Install e2fsprogs."
    fi

    if [[ "$EUID" -ne 0 ]] \
        && ! command -v sudo &>/dev/null \
        && ! command -v docker &>/dev/null; then
        err_chirho "native rootfs build needs root privileges or sudo"
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
    local checksum_path_chirho="$tarball_path_chirho.sha256"
    local expected_sha256_chirho
    local actual_sha256_chirho

    if [[ ! -f "$tarball_path_chirho" ]]; then
        log_chirho "Downloading Alpine minirootfs ${ALPINE_VERSION_CHIRHO}.${ALPINE_MINOR_CHIRHO} (${ALPINE_ARCH_CHIRHO})..."
        log_chirho "  URL: $DOWNLOAD_URL_CHIRHO"

        curl -fSL --progress-bar -o "$tarball_path_chirho" "$DOWNLOAD_URL_CHIRHO"
        log_chirho "Download complete: $(du -h "$tarball_path_chirho" | cut -f1)"
    else
        log_chirho "Tarball already cached: $tarball_path_chirho"
    fi

    curl -fsSL -o "$checksum_path_chirho" "$CHECKSUM_URL_CHIRHO"
    expected_sha256_chirho="$(cut -d' ' -f1 "$checksum_path_chirho")"
    actual_sha256_chirho="$(sha256_file_chirho "$tarball_path_chirho")"
    if [[ ! "$expected_sha256_chirho" =~ ^[0-9a-f]{64}$ ]]; then
        err_chirho "Alpine checksum response is malformed"
    fi
    if [[ "$actual_sha256_chirho" != "$expected_sha256_chirho" ]]; then
        err_chirho "Alpine minirootfs SHA-256 mismatch: expected $expected_sha256_chirho, got $actual_sha256_chirho"
    fi
    log_chirho "Verified immutable base rootfs SHA-256: $actual_sha256_chirho"
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

    # Format as ext4 with label (skipped when only Docker can format: the
    # container rebuilds and formats /tmp/disk.img itself, then copies it out)
    if [[ -n "$MKFS_CMD_CHIRHO" ]]; then
        $MKFS_CMD_CHIRHO -F -L "$FILESYSTEM_LABEL_CHIRHO" "$IMAGE_PATH_CHIRHO"
        log_chirho "Image formatted: $IMAGE_PATH_CHIRHO"
    else
        log_chirho "Image placeholder created: $IMAGE_PATH_CHIRHO (formatted by Docker)"
    fi
}

# ============================================================================
# Step 3: Extract Alpine rootfs into the image
# ============================================================================

populate_image_chirho() {
    # Workflow: spec-chirho/workflows-chirho/x11-bringup-chirho.md
    local tarball_path_chirho="$OUTPUT_DIR_CHIRHO/$TARBALL_NAME_CHIRHO"

    log_chirho "Mounting image and extracting Alpine rootfs..."

    mkdir -p "$MOUNT_DIR_CHIRHO"

    # Both paths invoke the same target-Alpine provisioner. Docker remains the
    # portable path for macOS; dlpChirho uses the host-direct loop-mount path.
    if command -v docker &>/dev/null || [[ "$(uname)" == "Darwin" ]]; then
        ROOTFS_BUILD_MODE_CHIRHO="docker-amd64-chirho"
        populate_image_docker_chirho "$tarball_path_chirho"
    else
        ROOTFS_BUILD_MODE_CHIRHO="native-host-direct-chirho"
        log_chirho "Docker not found - using the native host-direct loop-mount path."
        populate_image_linux_chirho "$tarball_path_chirho"
    fi
}

populate_image_linux_chirho() {
    local tarball_path_chirho="$1"

    for native_cmd_chirho in mount umount chroot; do
        command -v "$native_cmd_chirho" &>/dev/null \
            || err_chirho "native rootfs build requires $native_cmd_chirho"
    done

    run_as_root_chirho mount -o loop "$IMAGE_PATH_CHIRHO" "$MOUNT_DIR_CHIRHO"
    NATIVE_IMAGE_MOUNTED_CHIRHO=1

    # Extract rootfs
    run_as_root_chirho tar xzf "$tarball_path_chirho" -C "$MOUNT_DIR_CHIRHO"

    # Configure the rootfs while mounted
    configure_rootfs_chirho "$MOUNT_DIR_CHIRHO"
    stage_rootfs_build_inputs_chirho "$MOUNT_DIR_CHIRHO"

    run_as_root_chirho mount --bind /dev "$MOUNT_DIR_CHIRHO/dev"
    NATIVE_DEV_MOUNTED_CHIRHO=1
    run_as_root_chirho mount -t proc proc "$MOUNT_DIR_CHIRHO/proc"
    NATIVE_PROC_MOUNTED_CHIRHO=1
    run_as_root_chirho env ALPINE_BRANCH_CHIRHO="$ALPINE_VERSION_CHIRHO" \
        chroot "$MOUNT_DIR_CHIRHO" \
        /bin/sh /tmp/lineluya-rootfs-build-chirho/provision-alpine-rootfs-chirho.sh
    run_as_root_chirho rm -rf \
        "$MOUNT_DIR_CHIRHO/tmp/lineluya-rootfs-build-chirho"

    # Sync and unmount
    sync
    run_as_root_chirho umount "$MOUNT_DIR_CHIRHO/proc"
    NATIVE_PROC_MOUNTED_CHIRHO=0
    run_as_root_chirho umount "$MOUNT_DIR_CHIRHO/dev"
    NATIVE_DEV_MOUNTED_CHIRHO=0
    run_as_root_chirho umount "$MOUNT_DIR_CHIRHO"
    NATIVE_IMAGE_MOUNTED_CHIRHO=0
    rmdir "$MOUNT_DIR_CHIRHO" 2>/dev/null || true

    log_chirho "Image populated (Linux mount)."
}

populate_image_docker_chirho() {
    local tarball_path_chirho="$1"

    # Populate the ext4 image inside a privileged Alpine container.  On macOS
    # this is the only way (no native ext4 mount); on Linux it is still the
    # preferred way, because the container's own apk installs the x86_64
    # package set into the rootfs regardless of the host's arch or distro.
    if command -v docker &>/dev/null; then
        log_chirho "Using Docker to populate ext4 image..."

        local abs_image_chirho
        abs_image_chirho="$(cd "$(dirname "$IMAGE_PATH_CHIRHO")" && pwd)/$(basename "$IMAGE_PATH_CHIRHO")"
        local abs_tarball_chirho
        abs_tarball_chirho="$(cd "$(dirname "$tarball_path_chirho")" && pwd)/$(basename "$tarball_path_chirho")"

        log_chirho "Building ext4 image inside Docker container..."

        local cid_chirho
        # Pin the builder container to the SAME Alpine branch as the minirootfs.
        # 'alpine:latest' drifts: once it moved past 3.21 its apk-tools 3.x
        # wrote ZERO-LENGTH files for every --root install, so the disk looked
        # populated (paths present, /usr/bin full) while every binary was empty.
        # Matching the branch also keeps musl and the packages on one ABI.
        cid_chirho=$(docker create --platform linux/amd64 --privileged \
            "alpine:${ALPINE_VERSION_CHIRHO}" sleep 3600)
        ACTIVE_DOCKER_CID_CHIRHO="$cid_chirho"
        docker start "$cid_chirho" >/dev/null

        docker cp "$abs_tarball_chirho" "$cid_chirho:/tmp/rootfs.tar.gz" >/dev/null
        docker exec "$cid_chirho" mkdir -p /tmp/lineluya-rootfs-build-chirho
        docker cp "$ROOTFS_ASSET_DIR_CHIRHO/." \
            "$cid_chirho:/tmp/lineluya-rootfs-build-chirho/" >/dev/null
        docker cp "$XGEARS_SOURCE_PATH_CHIRHO" \
            "$cid_chirho:/tmp/lineluya-rootfs-build-chirho/xgears_chirho.c" >/dev/null

        # Run the build
        # The rootfs build runs as a real script FILE inside the container.
        # Passing it as a single-quoted argument to `sh -c` silently truncated
        # it: the apostrophes in the sample SQL ("VALUES (1, 'Hallelujah!...')")
        # closed the outer quote, so everything after that point - including the
        # final `sync` and `umount` - was parsed as stray arguments and never
        # ran, and the image was copied out with an unflushed journal.
        local script_path_chirho="$OUTPUT_DIR_CHIRHO/build-rootfs-chirho.sh"
        cat > "$script_path_chirho" << 'BUILD_ROOTFS_SCRIPT_CHIRHO'

set -e

image_mounted_chirho=0
dev_mounted_chirho=0
proc_mounted_chirho=0

cleanup_inner_build_chirho() {
    if [ "$proc_mounted_chirho" -eq 1 ]; then
        umount /mnt-chirho/proc 2>/dev/null || true
        proc_mounted_chirho=0
    fi
    if [ "$dev_mounted_chirho" -eq 1 ]; then
        umount /mnt-chirho/dev 2>/dev/null || true
        dev_mounted_chirho=0
    fi
    if [ "$image_mounted_chirho" -eq 1 ]; then
        umount /mnt-chirho 2>/dev/null || true
        image_mounted_chirho=0
    fi
}

trap cleanup_inner_build_chirho EXIT

# Create a fresh ext4 image inside the container
# e2fsprogs-extra carries dumpe2fs, which the journal gate below depends on.
apk add --no-cache e2fsprogs e2fsprogs-extra >/dev/null 2>&1
truncate -s "${DISK_SIZE_INNER_CHIRHO:-512M}" /tmp/disk.img
mkfs.ext4 -F -L "${FILESYSTEM_LABEL_INNER_CHIRHO:-lineluya-chirho}" \
    /tmp/disk.img >/dev/null 2>&1

mkdir -p /mnt-chirho
mount -o loop /tmp/disk.img /mnt-chirho
image_mounted_chirho=1

tar xzf /tmp/rootfs.tar.gz -C /mnt-chirho
rm /tmp/rootfs.tar.gz

# Ensure required directories
for dir_chirho in dev proc sys tmp run; do
    mkdir -p "/mnt-chirho/$dir_chirho"
done

# /etc/inittab for BusyBox init
cat > /mnt-chirho/etc/inittab << 'INITTAB_CHIRHO'
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
cat > /mnt-chirho/etc/fstab << 'FSTAB_CHIRHO'
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
cat > /mnt-chirho/etc/resolv.conf << 'RESOLV_CHIRHO'
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

# Run the same target-Alpine provisioner used by dlpChirho's host-direct path.
# The builder container is explicitly amd64, so the chroot executes the exact
# x86_64 compiler and target binaries that will ship in the image.
cp -a /tmp/lineluya-rootfs-build-chirho \
    /mnt-chirho/tmp/lineluya-rootfs-build-chirho
mount --bind /dev /mnt-chirho/dev
dev_mounted_chirho=1
mount -t proc proc /mnt-chirho/proc
proc_mounted_chirho=1
env ALPINE_BRANCH_CHIRHO="${ALPINE_BRANCH_CHIRHO:-3.21}" \
    chroot /mnt-chirho \
    /bin/sh /tmp/lineluya-rootfs-build-chirho/provision-alpine-rootfs-chirho.sh
umount /mnt-chirho/proc
proc_mounted_chirho=0
umount /mnt-chirho/dev
dev_mounted_chirho=0
rm -rf /mnt-chirho/tmp/lineluya-rootfs-build-chirho

# loop.ko is injected after disk build via inject-loop-ko-chirho.sh

# Create a tiny test MP3 for HDA/mpg123 playback validation
base64 -d > /mnt-chirho/root/test-tone-chirho.mp3 << 'MP3DATA_CHIRHO'
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
3Ik0OSb8uGAi1nSFnLoiAEAITJhUUs0qhZySI4GqTEFNRTMuMTAwqqqqqqqqqqqqqqqqqqr/+xJk
6Q/w+wdIgz7AmgAADSAAAAEE8B8YDXtCQAAANIAAAASqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq
qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqr/+xBk6A/w+wdIgz7A
mgAADSAAAAEEHB0eDPsiaAAANIAAAASqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq
qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqv/7EmTpj/EVB8cDXsiaAAANIAAAAQQg
HR4M+yJoAAA0gAAABKqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq
qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqv/7EGTqD/EQCEgDXsiYAAANIAAAAQTEHxwNeyJAAAA0
gAAABKqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq
qqqqqqqqqqqqqqqqqqqq//sSZOeP8PkHSQM+wJoAAA0gAAABBBAdIAz7ImgAADSAAAAEqqqqqqqq
qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq
qqqqqqqq//sQZOoAAREHSoV0AAoAAA0goAABBmxnQBmzgAAAADSDAAAAqqqqqqqqqqqqqqqqqqqq
qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqr/+xJk
4Q/wege/ByQACgAADSDgAAEAAAGkAAAAIAAANIAAAASqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq
qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqo=
MP3DATA_CHIRHO
echo "[DOCKER] /root/test-tone-chirho.mp3 created" >&2

# Create a test Python script
cat > /mnt-chirho/root/hello_chirho.py << 'PYTEST_CHIRHO'
# For God so loved the world that he gave his only begotten Son,
# that whoever believes in him should not perish but have eternal life. - John 3:16
print("Hallelujah! Python3 runs on Lineluya!")
print("For God so loved the world - John 3:16")
PYTEST_CHIRHO

# Create a test SQLite script
cat > /mnt-chirho/root/test_chirho.sql << 'SQLTEST_CHIRHO'
-- For God so loved the world that he gave his only begotten Son,
-- that whoever believes in him should not perish but have eternal life. - John 3:16
CREATE TABLE praise_chirho (id_chirho INTEGER PRIMARY KEY, msg_chirho TEXT);
INSERT INTO praise_chirho VALUES (1, 'Hallelujah! SQLite runs on Lineluya!');
SELECT * FROM praise_chirho;
SQLTEST_CHIRHO

sync
umount /mnt-chirho
image_mounted_chirho=0

# A disk copied out with an unflushed journal carries needs_recovery, and a
# kernel with no journal replay can then read stale or missing metadata.  This
# is exactly what a truncated build script produced before, so gate on it.
if ! command -v dumpe2fs >/dev/null 2>&1; then
    echo "[DOCKER] FATAL: dumpe2fs missing - the journal gate cannot run." >&2
    exit 1
fi
if dumpe2fs -h /tmp/disk.img 2>/dev/null | grep -q needs_recovery; then
    echo "[DOCKER] FATAL: needs_recovery still set after umount - not shipping." >&2
    exit 1
fi

echo "[DOCKER] rootfs populated and configured (journal clean)."

echo "[DOCKER] Build complete."
trap - EXIT
BUILD_ROOTFS_SCRIPT_CHIRHO

        docker cp "$script_path_chirho" \
            "$cid_chirho:/tmp/build-rootfs-chirho.sh" >/dev/null
        docker exec \
            -e DISK_SIZE_INNER_CHIRHO="$DISK_SIZE_CHIRHO" \
            -e FILESYSTEM_LABEL_INNER_CHIRHO="$FILESYSTEM_LABEL_CHIRHO" \
            -e ALPINE_BRANCH_CHIRHO="$ALPINE_VERSION_CHIRHO" \
            "$cid_chirho" /bin/sh /tmp/build-rootfs-chirho.sh
        # Extract the finished image via docker cp (not pipe/cat which corrupts on macOS)
        log_chirho "Extracting disk image from container via docker cp..."
        docker cp "$cid_chirho:/tmp/disk.img" "$abs_image_chirho"

        # Clean up container
        docker stop "$cid_chirho" >/dev/null 2>&1 || true
        docker rm "$cid_chirho" >/dev/null 2>&1 || true
        ACTIVE_DOCKER_CID_CHIRHO=""

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

    # Use sudo only when the caller is not already root and the target is
    # root-owned. dlpChirho invokes this script directly as root.
    if [[ "$EUID" -ne 0 ]] \
        && [[ -d "$root_chirho/bin" ]] \
        && [[ "$(stat -f '%u' "$root_chirho/bin" 2>/dev/null || stat -c '%u' "$root_chirho/bin" 2>/dev/null)" == "0" ]]; then
        command -v sudo &>/dev/null \
            || err_chirho "rootfs configuration needs sudo for $root_chirho"
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
    local tarball_path_chirho="$OUTPUT_DIR_CHIRHO/$TARBALL_NAME_CHIRHO"
    local source_revision_chirho
    local source_dirty_chirho
    local base_rootfs_sha256_chirho
    local final_image_sha256_chirho
    local xgears_source_sha256_chirho

    if git -C "$PROJECT_DIR_CHIRHO" rev-parse --is-inside-work-tree \
        >/dev/null 2>&1; then
        source_revision_chirho="$(git -C "$PROJECT_DIR_CHIRHO" rev-parse HEAD)"
        if git -C "$PROJECT_DIR_CHIRHO" diff --quiet \
            && git -C "$PROJECT_DIR_CHIRHO" diff --cached --quiet \
            && [[ -z "$(git -C "$PROJECT_DIR_CHIRHO" ls-files --others --exclude-standard)" ]]; then
            source_dirty_chirho="false"
        else
            source_dirty_chirho="true"
        fi
    else
        source_revision_chirho="unknown-chirho"
        source_dirty_chirho="unknown-chirho"
    fi
    base_rootfs_sha256_chirho="$(sha256_file_chirho "$tarball_path_chirho")"
    final_image_sha256_chirho="$(sha256_file_chirho "$IMAGE_PATH_CHIRHO")"
    xgears_source_sha256_chirho="$(sha256_file_chirho "$XGEARS_SOURCE_PATH_CHIRHO")"

    {
        echo "source_revision_chirho=$source_revision_chirho"
        echo "source_dirty_chirho=$source_dirty_chirho"
        echo "rootfs_build_mode_chirho=$ROOTFS_BUILD_MODE_CHIRHO"
        echo "alpine_release_chirho=${ALPINE_VERSION_CHIRHO}.${ALPINE_MINOR_CHIRHO}"
        echo "alpine_arch_chirho=$ALPINE_ARCH_CHIRHO"
        echo "base_rootfs_sha256_chirho=$base_rootfs_sha256_chirho"
        echo "xgears_source_sha256_chirho=$xgears_source_sha256_chirho"
        echo "final_image_sha256_chirho=$final_image_sha256_chirho"
    } > "$BUILD_MANIFEST_PATH_CHIRHO"

    echo ""
    echo "============================================================"
    echo "  Lineluya Alpine VirtIO-blk Disk Image (P2-005)"
    echo "============================================================"
    echo ""
    echo "  Alpine version:   ${ALPINE_VERSION_CHIRHO}.${ALPINE_MINOR_CHIRHO}"
    echo "  Architecture:     $ALPINE_ARCH_CHIRHO"
    echo "  Image size:       $DISK_SIZE_CHIRHO"
    echo "  Image path:       $IMAGE_PATH_CHIRHO"
    echo "  Build manifest:   $BUILD_MANIFEST_PATH_CHIRHO"
    echo "  Base SHA-256:     $base_rootfs_sha256_chirho"
    echo "  Image SHA-256:    $final_image_sha256_chirho"
    echo "  Root device:      /dev/vda (VirtIO-blk)"
    echo "  Hostname:         lineluya-chirho"
    echo ""
    echo "  Pre-installed: sqlite3, python3, dropbear, mpg123,"
    echo "                 xorg-server, xf86-video-fbdev, xterm, twm, mesa,"
    echo "                 repository-built /usr/bin/xgears-chirho"
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

trap cleanup_resources_chirho EXIT
main_chirho "$@"
