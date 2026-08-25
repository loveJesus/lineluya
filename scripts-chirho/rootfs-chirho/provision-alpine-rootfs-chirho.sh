#!/bin/sh
# For God so loved the world, that he gave his only begotten Son, that whosoever
# believeth in him should not perish, but have everlasting life. — John 3:16 (KJV)

set -eu

# Workflow: spec-chirho/workflows-chirho/x11-bringup-chirho.md

ALPINE_BRANCH_CHIRHO="${ALPINE_BRANCH_CHIRHO:-3.21}"
ROOTFS_BUILD_DIR_CHIRHO="/tmp/lineluya-rootfs-build-chirho"
XGEARS_SOURCE_CHIRHO="$ROOTFS_BUILD_DIR_CHIRHO/xgears_chirho.c"
XGEARS_OUTPUT_CHIRHO="/usr/bin/xgears-chirho"
TOOLCHAIN_RECORD_DIR_CHIRHO="/etc/lineluya-build-chirho"
BUILD_VIRTUAL_PACKAGE_CHIRHO=".lineluya-xgears-build-chirho"

fail_chirho() {
    echo "[ROOTFS] FATAL: $*" >&2
    exit 1
}

if [ "$ALPINE_BRANCH_CHIRHO" != "3.21" ]; then
    fail_chirho "the xgears toolchain is pinned for Alpine 3.21, not $ALPINE_BRANCH_CHIRHO"
fi

for source_file_chirho in \
    "$XGEARS_SOURCE_CHIRHO" \
    "$ROOTFS_BUILD_DIR_CHIRHO/profile-chirho" \
    "$ROOTFS_BUILD_DIR_CHIRHO/start-lineluya-desktop-chirho.sh" \
    "$ROOTFS_BUILD_DIR_CHIRHO/xorg-chirho.conf"
do
    [ -s "$source_file_chirho" ] \
        || fail_chirho "missing build input $source_file_chirho"
done

cat > /etc/apk/repositories << 'REPOSITORIES_CHIRHO'
https://dl-cdn.alpinelinux.org/alpine/v3.21/main
https://dl-cdn.alpinelinux.org/alpine/v3.21/community
REPOSITORIES_CHIRHO

echo "[ROOTFS] Installing Lineluya runtime packages..." >&2
apk update >&2
apk add --no-cache \
    sqlite sqlite-libs \
    python3 \
    dropbear dropbear-scp \
    mpg123 \
    twm \
    xterm \
    xorg-server \
    xf86-video-fbdev \
    xkeyboard-config xkbcomp \
    mesa-gl mesa-dri-gallium \
    mesa-demos \
    libxcb \
    >&2

echo "[ROOTFS] Installing the pinned xgears build toolchain..." >&2
apk add --no-cache --virtual "$BUILD_VIRTUAL_PACKAGE_CHIRHO" \
    "build-base=0.5-r3" \
    "gcc=14.2.0-r4" \
    "musl-dev=1.2.5-r11" \
    "libxcb-dev=1.16.1-r0" \
    >&2

mkdir -p "$TOOLCHAIN_RECORD_DIR_CHIRHO" /usr/local/sbin /etc/X11

cc \
    -std=c11 \
    -D_DEFAULT_SOURCE \
    -O2 \
    -Wall \
    -Wextra \
    -Werror \
    -Wl,--build-id=sha1 \
    -o "$XGEARS_OUTPUT_CHIRHO" \
    "$XGEARS_SOURCE_CHIRHO" \
    -lxcb
strip --strip-unneeded "$XGEARS_OUTPUT_CHIRHO"
chmod 0755 "$XGEARS_OUTPUT_CHIRHO"

readelf -h "$XGEARS_OUTPUT_CHIRHO" \
    | grep -Eq 'Class:[[:space:]]+ELF64' \
    || fail_chirho "xgears is not ELF64"
readelf -h "$XGEARS_OUTPUT_CHIRHO" \
    | grep -Eq 'Machine:[[:space:]]+Advanced Micro Devices X86-64' \
    || fail_chirho "xgears is not x86_64"

{
    echo "alpine_branch_chirho=$ALPINE_BRANCH_CHIRHO"
    echo "source_sha256_chirho=$(sha256sum "$XGEARS_SOURCE_CHIRHO" | cut -d' ' -f1)"
    cc --version
    apk info -v \
        | grep -E '^(build-base|gcc|musl-dev|libxcb-dev)-'
} > "$TOOLCHAIN_RECORD_DIR_CHIRHO/xgears-toolchain-chirho.txt"

cp "$ROOTFS_BUILD_DIR_CHIRHO/profile-chirho" /etc/profile
chmod 0644 /etc/profile
cp "$ROOTFS_BUILD_DIR_CHIRHO/start-lineluya-desktop-chirho.sh" \
    /usr/local/sbin/start-lineluya-desktop-chirho.sh
chmod 0755 /usr/local/sbin/start-lineluya-desktop-chirho.sh
cp "$ROOTFS_BUILD_DIR_CHIRHO/xorg-chirho.conf" /etc/X11/xorg.conf
chmod 0644 /etc/X11/xorg.conf

apk del "$BUILD_VIRTUAL_PACKAGE_CHIRHO" >&2

XGEARS_DEPENDENCIES_CHIRHO="$(ldd "$XGEARS_OUTPUT_CHIRHO" 2>&1)" \
    || fail_chirho "ldd could not inspect xgears"
printf '%s\n' "$XGEARS_DEPENDENCIES_CHIRHO" \
    > "$TOOLCHAIN_RECORD_DIR_CHIRHO/xgears-runtime-dependencies-chirho.txt"
if printf '%s\n' "$XGEARS_DEPENDENCIES_CHIRHO" | grep -q 'not found'; then
    fail_chirho "xgears has an unresolved runtime dependency"
fi

verify_failed_chirho=0
for binary_chirho in \
    /usr/bin/sqlite3 \
    /usr/bin/python3 \
    /usr/bin/mpg123 \
    /usr/bin/twm \
    /usr/bin/xterm \
    /usr/libexec/Xorg \
    /usr/sbin/dropbear \
    /usr/bin/xgears-chirho \
    /bin/busybox
do
    if [ ! -s "$binary_chirho" ]; then
        echo "[ROOTFS] MISSING OR EMPTY: $binary_chirho" >&2
        verify_failed_chirho=1
    fi
done

[ -x "$XGEARS_OUTPUT_CHIRHO" ] || verify_failed_chirho=1
[ "$verify_failed_chirho" -eq 0 ] \
    || fail_chirho "rootfs artifact verification failed"

echo "[ROOTFS] xgears-chirho OK ($(wc -c < "$XGEARS_OUTPUT_CHIRHO") bytes)" >&2
echo "[ROOTFS] Lineluya runtime, desktop policy, and xgears build complete." >&2
