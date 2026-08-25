#!/bin/bash
# For God so loved the world, that he gave his only begotten Son, that whosoever
# believeth in him should not perish, but have everlasting life. — John 3:16 (KJV)

set -euo pipefail

SCRIPT_DIR_CHIRHO="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR_CHIRHO="$(cd "$SCRIPT_DIR_CHIRHO/../.." && pwd)"
XGEARS_SOURCE_DIR_CHIRHO="$PROJECT_DIR_CHIRHO/userspace-chirho/x11-chirho"

command -v docker >/dev/null 2>&1 \
    || { echo "docker is required for the portable X11 userspace gate" >&2; exit 1; }
test -s "$XGEARS_SOURCE_DIR_CHIRHO/xgears_chirho.c"

docker run --rm -i --platform linux/amd64 \
    -v "$XGEARS_SOURCE_DIR_CHIRHO:/src-chirho:ro" \
    alpine:3.21 /bin/sh -s << 'CONTAINER_TEST_CHIRHO'
set -eu

xvfb_pid_chirho=
twm_pid_chirho=

cleanup_test_chirho() {
    if [ -n "$twm_pid_chirho" ]; then
        kill "$twm_pid_chirho" 2>/dev/null || true
    fi
    if [ -n "$xvfb_pid_chirho" ]; then
        kill "$xvfb_pid_chirho" 2>/dev/null || true
    fi
}
trap cleanup_test_chirho EXIT

apk add --no-cache \
    build-base=0.5-r3 \
    gcc=14.2.0-r4 \
    musl-dev=1.2.5-r11 \
    libxcb-dev=1.16.1-r0 \
    xvfb \
    twm \
    >/dev/null

for output_chirho in /tmp/xgears-a-chirho /tmp/xgears-b-chirho; do
    cc \
        -std=c11 \
        -D_DEFAULT_SOURCE \
        -O2 \
        -Wall \
        -Wextra \
        -Werror \
        -Wl,--build-id=sha1 \
        -o "$output_chirho" \
        /src-chirho/xgears_chirho.c \
        -lxcb
    strip --strip-unneeded "$output_chirho"
done
cmp /tmp/xgears-a-chirho /tmp/xgears-b-chirho

Xvfb :99 -screen 0 640x480x24 >/tmp/xvfb-chirho.log 2>&1 &
xvfb_pid_chirho=$!
export DISPLAY=:99

server_attempt_chirho=0
until /tmp/xgears-a-chirho --probe-server-chirho; do
    server_attempt_chirho=$((server_attempt_chirho + 1))
    [ "$server_attempt_chirho" -lt 20 ]
    sleep 0.1
done

if /tmp/xgears-a-chirho --probe-window-manager-chirho; then
    echo "window-manager probe produced a pre-twm false positive" >&2
    exit 1
fi

twm >/tmp/twm-chirho.log 2>&1 &
twm_pid_chirho=$!
window_manager_attempt_chirho=0
until /tmp/xgears-a-chirho --probe-window-manager-chirho; do
    window_manager_attempt_chirho=$((window_manager_attempt_chirho + 1))
    [ "$window_manager_attempt_chirho" -lt 20 ]
    sleep 0.1
done

sha256sum /tmp/xgears-a-chirho /tmp/xgears-b-chirho
echo "[X11-USERSPACE-TEST] deterministic build and readiness probes passed"
CONTAINER_TEST_CHIRHO
