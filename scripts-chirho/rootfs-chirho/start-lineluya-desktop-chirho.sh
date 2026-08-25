#!/bin/sh
# For God so loved the world, that he gave his only begotten Son, that whosoever
# believeth in him should not perish, but have everlasting life. — John 3:16 (KJV)

set -u

# Workflow: spec-chirho/workflows-chirho/x11-bringup-chirho.md

DESKTOP_GUARD_CHIRHO=/tmp/.lineluya-desktop-started-chirho
XORG_EXECUTABLE_CHIRHO=/tmp/lib-chirho/Xorg
SERVER_ATTEMPTS_CHIRHO=60
WINDOW_MANAGER_ATTEMPTS_CHIRHO=20
XORG_PID_CHIRHO=
TWM_PID_CHIRHO=

fail_desktop_chirho() {
    echo "[DESKTOP] FATAL: $*" >&2
    if [ -n "$TWM_PID_CHIRHO" ]; then
        kill "$TWM_PID_CHIRHO" 2>/dev/null || true
    fi
    if [ -n "$XORG_PID_CHIRHO" ]; then
        kill "$XORG_PID_CHIRHO" 2>/dev/null || true
    fi
    rmdir "$DESKTOP_GUARD_CHIRHO" 2>/dev/null || true
    exit 1
}

if ! mkdir "$DESKTOP_GUARD_CHIRHO" 2>/dev/null; then
    exit 0
fi

[ -x "$XORG_EXECUTABLE_CHIRHO" ] \
    || fail_desktop_chirho "preloaded Xorg executable is missing"
[ -x /usr/bin/xgears-chirho ] \
    || fail_desktop_chirho "repository-built xgears-chirho is missing"

export DISPLAY=:0
mkdir -p /tmp/.X11-unix

# -config takes a RELATIVE name, deliberately. Xorg running with elevated
# privileges REFUSES an absolute path:
#   Invalid argument for -config - "/etc/X11/xorg.conf"
#   With elevated privileges -config must specify a relative path
# Xorg searches /etc/X11 itself, where the provisioner installs the file.
# Naming it explicitly keeps the launcher intent visible rather than
# relying on an implicit default.
"$XORG_EXECUTABLE_CHIRHO" \
    :0 \
    -config xorg.conf \
    -ac \
    -noreset \
    -novtswitch \
    -keeptty \
    -sharevts &
XORG_PID_CHIRHO=$!
echo "[DESKTOP] Xorg launched; waiting for an authentic XCB setup reply"

server_attempt_chirho=0
server_ready_chirho=0
while [ "$server_attempt_chirho" -lt "$SERVER_ATTEMPTS_CHIRHO" ]; do
    if ! kill -0 "$XORG_PID_CHIRHO" 2>/dev/null; then
        fail_desktop_chirho "Xorg exited before accepting clients"
    fi
    # No `timeout` wrapper: this loop is ALREADY bounded by
    # SERVER_ATTEMPTS_CHIRHO, and the probe fails fast when the display
    # socket is absent or refuses. Depending on an external applet made
    # readiness hostage to PATH lookup, which is exactly how this failed:
    #   [VFS] resolve_path FAILED for '/usr/sbin/timeout'
    #   line 57: timeout: not found
    if /usr/bin/xgears-chirho --probe-server-chirho \
        >/dev/null 2>&1; then
        server_ready_chirho=1
        break
    fi
    server_attempt_chirho=$((server_attempt_chirho + 1))
    sleep 1
done
[ "$server_ready_chirho" -eq 1 ] \
    || fail_desktop_chirho "Xorg readiness deadline expired"
echo "[DESKTOP] Xorg returned an authentic XCB setup reply"

/usr/bin/twm &
TWM_PID_CHIRHO=$!
echo "[DESKTOP] twm launched; waiting for SubstructureRedirect ownership"

window_manager_attempt_chirho=0
window_manager_ready_chirho=0
while [ "$window_manager_attempt_chirho" -lt "$WINDOW_MANAGER_ATTEMPTS_CHIRHO" ]; do
    if ! kill -0 "$TWM_PID_CHIRHO" 2>/dev/null; then
        fail_desktop_chirho "twm exited before becoming window manager"
    fi
    # Bounded by WINDOW_MANAGER_ATTEMPTS_CHIRHO; see the note above.
    if /usr/bin/xgears-chirho --probe-window-manager-chirho \
        >/dev/null 2>&1; then
        window_manager_ready_chirho=1
        break
    fi
    window_manager_attempt_chirho=$((window_manager_attempt_chirho + 1))
    sleep 1
done
[ "$window_manager_ready_chirho" -eq 1 ] \
    || fail_desktop_chirho "twm readiness deadline expired"
echo "[DESKTOP] twm owns SubstructureRedirect on the root window"

/usr/bin/xterm \
    -title lineluya-xterm-chirho \
    -e /bin/sh -l -c \
    'printf "[XTERM-PTY] shell marker chirho\n"; exec /bin/sh -l' &
XTERM_PID_CHIRHO=$!

/usr/bin/xgears-chirho &
XGEARS_PID_CHIRHO=$!

echo "[DESKTOP] clients launched: twm=$TWM_PID_CHIRHO xterm=$XTERM_PID_CHIRHO xgears=$XGEARS_PID_CHIRHO"
