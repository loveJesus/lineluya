#!/bin/bash
# For God so loved the world, that he gave his only begotten Son, that whosoever
# believeth in him should not perish, but have everlasting life. — John 3:16 (KJV)

set -Eeuo pipefail
umask 077

# Workflow: spec-chirho/workflows-chirho/x11-evidence-cohort-chirho.md
#
# This runner measures an already-built host-direct artifact. It deliberately
# does not rebuild between attempts: a Gate D cohort must exercise one immutable
# kernel/rootfs pair. Gate E rebuilds those artifacts separately and invokes
# this runner again against the fresh pair.

SCRIPT_DIR_CHIRHO="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR_CHIRHO="${PROJECT_DIR_CHIRHO:-$(cd "$SCRIPT_DIR_CHIRHO/../.." && pwd)}"
DEFAULT_KERNEL_IMAGE_CHIRHO="$PROJECT_DIR_CHIRHO/target/disk-images-chirho/lineluya-bios-chirho.img"
if [ ! -f "$DEFAULT_KERNEL_IMAGE_CHIRHO" ] \
    && [ -f "$PROJECT_DIR_CHIRHO/output-chirho/lineluya-bios-chirho.img" ]; then
    DEFAULT_KERNEL_IMAGE_CHIRHO="$PROJECT_DIR_CHIRHO/output-chirho/lineluya-bios-chirho.img"
fi
DEFAULT_KERNEL_ELF_CHIRHO="$PROJECT_DIR_CHIRHO/target/x86_64-lineluya-chirho/release/lineluya-chirho"
if [ ! -f "$DEFAULT_KERNEL_ELF_CHIRHO" ] \
    && [ -f "$PROJECT_DIR_CHIRHO/kernel-chirho/target/x86_64-lineluya-chirho/release/lineluya-chirho" ]; then
    DEFAULT_KERNEL_ELF_CHIRHO="$PROJECT_DIR_CHIRHO/kernel-chirho/target/x86_64-lineluya-chirho/release/lineluya-chirho"
fi
KERNEL_IMAGE_CHIRHO="${KERNEL_IMAGE_CHIRHO:-$DEFAULT_KERNEL_IMAGE_CHIRHO}"
KERNEL_ELF_CHIRHO="${KERNEL_ELF_CHIRHO:-$DEFAULT_KERNEL_ELF_CHIRHO}"
BASE_ROOTFS_CHIRHO="${BASE_ROOTFS_CHIRHO:-$PROJECT_DIR_CHIRHO/target/alpine-virtio-chirho/alpine-virtio-chirho.img}"
EVIDENCE_ROOT_CHIRHO="${EVIDENCE_ROOT_CHIRHO:-$PROJECT_DIR_CHIRHO/target/evidence-chirho/x11-desktop-chirho}"

RUN_COUNT_CHIRHO="${RUN_COUNT_CHIRHO:-1}"
BASE_SSH_FWD_PORT_CHIRHO="${BASE_SSH_FWD_PORT_CHIRHO:-2420}"
TIMEOUT_CHIRHO="${TIMEOUT_CHIRHO:-400}"
POST_SUCCESS_OBSERVE_CHIRHO="${POST_SUCCESS_OBSERVE_CHIRHO:-10}"
FRAMEBUFFER_SETTLE_SECONDS_CHIRHO="${FRAMEBUFFER_SETTLE_SECONDS_CHIRHO:-2}"
CPU_MODEL_CHIRHO="${CPU_MODEL_CHIRHO:-qemu64}"
MEMORY_CHIRHO="${MEMORY_CHIRHO:-1G}"
SMP_CHIRHO="${SMP_CHIRHO:-2}"
REQUIRE_CLEAN_SOURCE_CHIRHO="${REQUIRE_CLEAN_SOURCE_CHIRHO:-1}"
REQUIRE_TRACE_FREE_CHIRHO="${REQUIRE_TRACE_FREE_CHIRHO:-1}"
KEEP_SCRATCH_CHIRHO="${KEEP_SCRATCH_CHIRHO:-0}"

# These names come from source blocks whose own comments identify them as
# temporary, one-shot, PID-specific, or hypothesis-directed traces. Stable
# milestones such as X11-BRINGUP, X11-WAIT, XORG-MAIN-LOOP, and TICK-SKIP are
# deliberately absent.
TEMPORARY_TRACE_NAMES_CHIRHO='XORG-ENTRY|XORG-SC|CTX-PRE|CTX-POST|PIPE-REF|PF-PID[0-9]+|PF-STACK|CON-SPIN|SCHED-TRACE|SCHED-DROP'
TEMPORARY_TRACE_NAMES_CHIRHO+='|X11-REQ|X11-WRITE|XORG-WRITE|X11-RECVFROM13|X11-RECVMSG13|FS-RETURN|PID2-SEL|PID2-RSP'
TEMPORARY_TRACE_NAMES_CHIRHO+='|P3-[A-Z0-9_-]+|P5-[A-Z0-9_-]+|P7-[A-Z0-9_-]+|POLL13|SELECT-P3|SELECT-PID2|PID5-SELECT|PID5-SELECT-FD'
TEMPORARY_TRACE_NAMES_CHIRHO+='|SIG-DBG|WE3|FCNTL-P3|CLOSE9|FD4-HAS-DATA|RD6-VFS|WRITE1-TRACE|TRAP-34B|RSP-8|EXEC-TRACE|TICK-TRACE'
TEMPORARY_TRACE_NAMES_CHIRHO+='|UNIX-POLL-TRACE|UNIX-RECVFROM-TRACE|UNIX-RECVMSG-TRACE|UNIX-SELECT-TRACE|UNIX-RECV'
TEMPORARY_TRACE_NAMES_CHIRHO+='|EXT4-DBG|EXT4-RD|EXT4-DIR-FAIL|EXT4-LEGACY|POLL-DBG|VFS-DBG|MOUNT-DBG|RELAY-DBG'
TEMPORARY_TRACE_NAMES_CHIRHO+='|EP0-RESUME|EP0-YIELD|EP7-COUNT|EP7-HLT|EPOLL-RET7|EPOLL-X11-HOT|EPOLL-YIELD0|READ-FD0|DUP2|WV-TRACE'
FORBIDDEN_POLICY_MARKER_NAMES_CHIRHO='WAIT4-FAST|GPF-HLT-SKIP'
FORBIDDEN_SOURCE_LITERAL_PATTERN_CHIRHO='/tmp/server-0\.xkm|XKM_DEFAULT_CHIRHO'
RUNTIME_FAILURE_MARKER_NAMES_CHIRHO='SYSRET-GUARD|WAIT4-FAST|GPF-HLT-SKIP|PRELOAD-SKIP|FD-MIRROR-INVARIANT|FD-RETIRE-INVARIANT|PIPE-REF-INVARIANT'

SOURCE_REVISION_CHIRHO=""
COHORT_ID_CHIRHO="${COHORT_ID_CHIRHO:-}"
COHORT_DIR_CHIRHO=""
BASE_ROOTFS_HASH_CHIRHO=""
KERNEL_IMAGE_HASH_CHIRHO=""
QEMU_PID_CHIRHO=""
CURRENT_SCRATCH_CHIRHO=""
CURRENT_MONITOR_CHIRHO=""

fatal_chirho() {
    echo "FATAL: $*" >&2
    exit 1
}

cleanup_chirho() {
    if [ -n "$QEMU_PID_CHIRHO" ] && kill -0 "$QEMU_PID_CHIRHO" 2>/dev/null; then
        kill "$QEMU_PID_CHIRHO" 2>/dev/null || true
        wait "$QEMU_PID_CHIRHO" 2>/dev/null || true
    fi
    if [ -n "$CURRENT_MONITOR_CHIRHO" ]; then
        rm -f "$CURRENT_MONITOR_CHIRHO"
    fi
    if [ "$KEEP_SCRATCH_CHIRHO" != "1" ] && [ -n "$CURRENT_SCRATCH_CHIRHO" ]; then
        rm -f "$CURRENT_SCRATCH_CHIRHO"
    fi
}
trap cleanup_chirho EXIT INT TERM

require_command_chirho() {
    command -v "$1" >/dev/null 2>&1 || fatal_chirho "required command is missing: $1"
}

git_chirho() {
    # dlpChirho receives the checkout from macOS with its numeric UID intact.
    # Trust this exact caller-selected worktree for this process only; do not
    # mutate global Git configuration or broadly waive ownership checks.
    git -c "safe.directory=$PROJECT_DIR_CHIRHO" "$@"
}

require_unsigned_integer_chirho() {
    case "$2" in
        0|[1-9]|[1-9][0-9]*) ;;
        *) fatal_chirho "$1 must be an unsigned decimal integer without leading zeros, got '$2'" ;;
    esac
}

hash_file_chirho() {
    sha256sum "$1" | cut -d ' ' -f 1
}

count_pattern_chirho() {
    local pattern_chirho="$1"
    local file_chirho="$2"
    grep -aEic "$pattern_chirho" "$file_chirho" 2>/dev/null || true
}

first_pattern_chirho() {
    local pattern_chirho="$1"
    local file_chirho="$2"
    grep -aEim1 "$pattern_chirho" "$file_chirho" 2>/dev/null || true
}

port_is_busy_chirho() {
    ss -ltnH "sport = :$1" 2>/dev/null | grep -q .
}

append_metadata_chirho() {
    local metadata_file_chirho="$1"
    local key_chirho="$2"
    local value_chirho="$3"
    printf '%s=%s\n' "$key_chirho" "$value_chirho" >>"$metadata_file_chirho"
}

record_hash_if_present_chirho() {
    local metadata_file_chirho="$1"
    local key_chirho="$2"
    local path_chirho="$3"
    if [ -f "$path_chirho" ]; then
        append_metadata_chirho "$metadata_file_chirho" "${key_chirho}_path_chirho" "$path_chirho"
        append_metadata_chirho "$metadata_file_chirho" "${key_chirho}_sha256_chirho" "$(hash_file_chirho "$path_chirho")"
    else
        append_metadata_chirho "$metadata_file_chirho" "${key_chirho}_path_chirho" "absent_chirho"
        append_metadata_chirho "$metadata_file_chirho" "${key_chirho}_sha256_chirho" "absent_chirho"
    fi
}

wait_for_file_chirho() {
    local path_chirho="$1"
    local expected_size_chirho="${2:-}"
    local wait_iteration_chirho=0
    while [ "$wait_iteration_chirho" -lt 40 ]; do
        if [ -s "$path_chirho" ]; then
            if [ -z "$expected_size_chirho" ] \
                || [ "$(stat -c '%s' "$path_chirho" 2>/dev/null || true)" = "$expected_size_chirho" ]; then
                return 0
            fi
        fi
        sleep 0.1
        wait_iteration_chirho=$((wait_iteration_chirho + 1))
    done
    return 1
}

send_hmp_chirho() {
    local command_chirho="$1"
    local output_chirho="$2"
    printf '%s\n' "$command_chirho" \
        | socat -T 3 - "UNIX-CONNECT:$CURRENT_MONITOR_CHIRHO" \
        >"$output_chirho" 2>&1
}

capture_screendump_chirho() {
    local output_path_chirho="$1"
    rm -f "$output_path_chirho"
    send_hmp_chirho \
        "screendump \"$output_path_chirho\"" \
        "${output_path_chirho}.monitor-chirho.log"
    wait_for_file_chirho "$output_path_chirho"
}

capture_physical_framebuffer_chirho() {
    local serial_log_chirho="$1"
    local output_path_chirho="$2"
    local metadata_file_chirho="$3"
    local capture_stage_chirho="$4"
    local framebuffer_info_chirho
    local framebuffer_phys_line_chirho
    local framebuffer_size_chirho
    local framebuffer_phys_chirho
    local framebuffer_width_chirho
    local framebuffer_height_chirho
    local framebuffer_format_chirho

    framebuffer_info_chirho="$(grep -am1 '^INFO : Framebuffer info:' "$serial_log_chirho" || true)"
    framebuffer_phys_line_chirho="$(grep -am1 '^\[FB\] virt=.*phys=0x' "$serial_log_chirho" || true)"
    framebuffer_size_chirho="$(printf '%s\n' "$framebuffer_info_chirho" \
        | sed -E 's/.*byte_len: ([0-9]+).*/\1/')"
    framebuffer_width_chirho="$(printf '%s\n' "$framebuffer_info_chirho" \
        | sed -E 's/.*width: ([0-9]+).*/\1/')"
    framebuffer_height_chirho="$(printf '%s\n' "$framebuffer_info_chirho" \
        | sed -E 's/.*height: ([0-9]+).*/\1/')"
    framebuffer_format_chirho="$(printf '%s\n' "$framebuffer_info_chirho" \
        | sed -E 's/.*pixel_format: ([A-Za-z0-9_]+).*/\1/')"
    framebuffer_phys_chirho="$(printf '%s\n' "$framebuffer_phys_line_chirho" \
        | sed -E 's/.*phys=(0x[0-9a-fA-F]+).*/\1/')"

    printf '%s\n' "$framebuffer_size_chirho" | grep -Eq '^[0-9]+$' \
        || return 1
    printf '%s\n' "$framebuffer_width_chirho" | grep -Eq '^[0-9]+$' \
        || return 1
    printf '%s\n' "$framebuffer_height_chirho" | grep -Eq '^[0-9]+$' \
        || return 1
    printf '%s\n' "$framebuffer_phys_chirho" | grep -Eq '^0x[0-9a-fA-F]+$' \
        || return 1
    [ "$framebuffer_format_chirho" = "Bgr" ] || return 1

    append_metadata_chirho "$metadata_file_chirho" "framebuffer_${capture_stage_chirho}_phys_chirho" "$framebuffer_phys_chirho"
    append_metadata_chirho "$metadata_file_chirho" "framebuffer_${capture_stage_chirho}_size_chirho" "$framebuffer_size_chirho"
    append_metadata_chirho "$metadata_file_chirho" "framebuffer_${capture_stage_chirho}_width_chirho" "$framebuffer_width_chirho"
    append_metadata_chirho "$metadata_file_chirho" "framebuffer_${capture_stage_chirho}_height_chirho" "$framebuffer_height_chirho"
    append_metadata_chirho "$metadata_file_chirho" "framebuffer_${capture_stage_chirho}_format_chirho" "$framebuffer_format_chirho"

    rm -f "$output_path_chirho"
    # HMP parses an unquoted absolute filename as an arithmetic expression.
    send_hmp_chirho \
        "pmemsave $framebuffer_phys_chirho $framebuffer_size_chirho \"$output_path_chirho\"" \
        "${output_path_chirho}.monitor-chirho.log"
    wait_for_file_chirho "$output_path_chirho" "$framebuffer_size_chirho"
}

convert_raw_framebuffer_chirho() {
    local raw_path_chirho="$1"
    local png_path_chirho="$2"
    local width_chirho="$3"
    local height_chirho="$4"

    if command -v magick >/dev/null 2>&1; then
        magick -size "${width_chirho}x${height_chirho}" -depth 8 \
            "BGR:$raw_path_chirho" "$png_path_chirho"
    elif command -v convert >/dev/null 2>&1; then
        convert -size "${width_chirho}x${height_chirho}" -depth 8 \
            "BGR:$raw_path_chirho" "$png_path_chirho"
    else
        return 2
    fi
}

observe_milestone_chirho() {
    local milestone_key_chirho="$1"
    local pattern_chirho="$2"
    local serial_log_chirho="$3"
    local elapsed_chirho="$4"
    local timeline_file_chirho="$5"

    if [ -z "${MILESTONE_SECONDS_CHIRHO[$milestone_key_chirho]+present_chirho}" ] \
        && grep -aEq "$pattern_chirho" "$serial_log_chirho"; then
        MILESTONE_SECONDS_CHIRHO[$milestone_key_chirho]="$elapsed_chirho"
        printf '%s\t%s\n' "$elapsed_chirho" "$milestone_key_chirho" >>"$timeline_file_chirho"
    fi
}

write_milestones_chirho() {
    local metadata_file_chirho="$1"
    local milestone_key_chirho
    for milestone_key_chirho in \
        xorg_launch_chirho \
        display_bind_chirho \
        xkbcomp_exec_chirho \
        xkbcomp_exit_zero_chirho \
        xorg_main_loop_chirho \
        authentic_setup_reply_chirho \
        twm_ownership_chirho \
        clients_launched_chirho \
        xgears_exec_chirho \
        first_fps_chirho \
        xterm_pty_shell_chirho \
        framebuffer_before_chirho \
        framebuffer_after_chirho
    do
        append_metadata_chirho \
            "$metadata_file_chirho" \
            "${milestone_key_chirho}_seconds_chirho" \
            "${MILESTONE_SECONDS_CHIRHO[$milestone_key_chirho]:-missing_chirho}"
    done
}

first_missing_milestone_chirho() {
    local milestone_key_chirho
    for milestone_key_chirho in \
        xorg_launch_chirho \
        display_bind_chirho \
        xkbcomp_exec_chirho \
        xkbcomp_exit_zero_chirho \
        xorg_main_loop_chirho \
        authentic_setup_reply_chirho \
        twm_ownership_chirho \
        clients_launched_chirho \
        xgears_exec_chirho \
        first_fps_chirho \
        xterm_pty_shell_chirho \
        framebuffer_before_chirho \
        framebuffer_after_chirho
    do
        if [ -z "${MILESTONE_SECONDS_CHIRHO[$milestone_key_chirho]+present_chirho}" ]; then
            printf '%s\n' "$milestone_key_chirho"
            return 0
        fi
    done
    printf '%s\n' "none_chirho"
}

all_runtime_milestones_present_chirho() {
    [ -n "${MILESTONE_SECONDS_CHIRHO[xorg_launch_chirho]+present_chirho}" ] \
        && [ -n "${MILESTONE_SECONDS_CHIRHO[display_bind_chirho]+present_chirho}" ] \
        && [ -n "${MILESTONE_SECONDS_CHIRHO[xkbcomp_exec_chirho]+present_chirho}" ] \
        && [ -n "${MILESTONE_SECONDS_CHIRHO[xkbcomp_exit_zero_chirho]+present_chirho}" ] \
        && [ -n "${MILESTONE_SECONDS_CHIRHO[xorg_main_loop_chirho]+present_chirho}" ] \
        && [ -n "${MILESTONE_SECONDS_CHIRHO[authentic_setup_reply_chirho]+present_chirho}" ] \
        && [ -n "${MILESTONE_SECONDS_CHIRHO[twm_ownership_chirho]+present_chirho}" ] \
        && [ -n "${MILESTONE_SECONDS_CHIRHO[clients_launched_chirho]+present_chirho}" ] \
        && [ -n "${MILESTONE_SECONDS_CHIRHO[xgears_exec_chirho]+present_chirho}" ] \
        && [ -n "${MILESTONE_SECONDS_CHIRHO[first_fps_chirho]+present_chirho}" ] \
        && [ -n "${MILESTONE_SECONDS_CHIRHO[xterm_pty_shell_chirho]+present_chirho}" ]
}

fatal_serial_line_chirho() {
    local serial_log_chirho="$1"
    first_pattern_chirho \
        "^\\[($RUNTIME_FAILURE_MARKER_NAMES_CHIRHO)\\]|^\\[PF\\] |!!! PAGE FAULT|kernel panic|panicked at|general protection fault|\\bGPF\\b|double fault|invalid.?opcode|LeafFrameExhaustedChirho|allocator halt|out of memory|OOM kill|invalid[-_ ]context|CTX-REJECT|UART.*storm|IRQ4.*storm" \
        "$serial_log_chirho"
}

temporary_trace_count_chirho() {
    local serial_log_chirho="$1"
    count_pattern_chirho \
        "^\\[($TEMPORARY_TRACE_NAMES_CHIRHO)\\]" \
        "$serial_log_chirho"
}

run_attempt_chirho() {
    local attempt_number_chirho="$1"
    local attempt_label_chirho
    local attempt_dir_chirho
    local metadata_file_chirho
    local timeline_file_chirho
    local serial_log_chirho
    local qemu_stderr_chirho
    local qemu_command_file_chirho
    local ssh_fwd_port_chirho
    local scratch_initial_hash_chirho
    local scratch_final_hash_chirho
    local base_hash_after_chirho
    local kernel_hash_after_chirho
    local qemu_exit_chirho=0
    local qemu_ended_early_chirho=0
    local start_epoch_chirho=0
    local elapsed_chirho=0
    local xkbcomp_pid_chirho=""
    local fatal_line_chirho=""
    local first_divergence_chirho="none_chirho"
    local result_chirho="fail_chirho"
    local before_ppm_chirho
    local after_ppm_chirho
    local before_raw_chirho
    local after_raw_chirho
    local before_raw_hash_chirho="missing_chirho"
    local after_raw_hash_chirho="missing_chirho"
    local before_ppm_hash_chirho="missing_chirho"
    local after_ppm_hash_chirho="missing_chirho"
    local changed_raw_bytes_chirho=0
    local after_nonzero_bytes_chirho=0
    local launcher_count_chirho=0
    local xorg_launch_count_chirho=0
    local client_launch_count_chirho=0
    local synthesis_count_chirho=0
    local trace_count_chirho=0
    local post_success_start_chirho=-1
    local framebuffer_width_chirho=""
    local framebuffer_height_chirho=""
    local qemu_argument_chirho
    local -a qemu_command_chirho
    declare -gA MILESTONE_SECONDS_CHIRHO=()

    printf -v attempt_label_chirho 'attempt-%02d-chirho' "$attempt_number_chirho"
    attempt_dir_chirho="$COHORT_DIR_CHIRHO/$attempt_label_chirho"
    metadata_file_chirho="$attempt_dir_chirho/metadata-chirho.txt"
    timeline_file_chirho="$attempt_dir_chirho/timeline-chirho.tsv"
    serial_log_chirho="$attempt_dir_chirho/serial-chirho.log"
    qemu_stderr_chirho="$attempt_dir_chirho/qemu-stderr-chirho.log"
    qemu_command_file_chirho="$attempt_dir_chirho/qemu-command-chirho.txt"
    before_ppm_chirho="$attempt_dir_chirho/framebuffer-before-chirho.ppm"
    after_ppm_chirho="$attempt_dir_chirho/framebuffer-after-chirho.ppm"
    before_raw_chirho="$attempt_dir_chirho/framebuffer-before-chirho.raw"
    after_raw_chirho="$attempt_dir_chirho/framebuffer-after-chirho.raw"
    ssh_fwd_port_chirho=$((BASE_SSH_FWD_PORT_CHIRHO + attempt_number_chirho - 1))

    [ "$ssh_fwd_port_chirho" -le 65535 ] \
        || fatal_chirho "attempt port exceeds 65535: $ssh_fwd_port_chirho"
    if port_is_busy_chirho "$ssh_fwd_port_chirho"; then
        fatal_chirho "loopback forward port is busy: $ssh_fwd_port_chirho"
    fi

    mkdir -p "$attempt_dir_chirho"
    : >"$metadata_file_chirho"
    : >"$timeline_file_chirho"
    : >"$serial_log_chirho"
    : >"$qemu_stderr_chirho"

    CURRENT_SCRATCH_CHIRHO="$attempt_dir_chirho/rootfs-scratch-${ssh_fwd_port_chirho}-$$-chirho.img"
    # AF_UNIX paths are limited (108 bytes on Linux). Evidence directories are
    # intentionally descriptive, so keep this ephemeral transport path short.
    CURRENT_MONITOR_CHIRHO="/tmp/lx11-${ssh_fwd_port_chirho}-$$-${attempt_number_chirho}-chirho.sock"
    cp --reflink=auto "$BASE_ROOTFS_CHIRHO" "$CURRENT_SCRATCH_CHIRHO"
    scratch_initial_hash_chirho="$(hash_file_chirho "$CURRENT_SCRATCH_CHIRHO")"
    [ "$scratch_initial_hash_chirho" = "$BASE_ROOTFS_HASH_CHIRHO" ] \
        || fatal_chirho "scratch image is not byte-identical to immutable base"

    append_metadata_chirho "$metadata_file_chirho" "attempt_number_chirho" "$attempt_number_chirho"
    append_metadata_chirho "$metadata_file_chirho" "source_revision_chirho" "$SOURCE_REVISION_CHIRHO"
    append_metadata_chirho "$metadata_file_chirho" "cpu_model_chirho" "$CPU_MODEL_CHIRHO"
    append_metadata_chirho "$metadata_file_chirho" "acceleration_chirho" "kvm_chirho"
    append_metadata_chirho "$metadata_file_chirho" "memory_chirho" "$MEMORY_CHIRHO"
    append_metadata_chirho "$metadata_file_chirho" "smp_chirho" "$SMP_CHIRHO"
    append_metadata_chirho "$metadata_file_chirho" "timeout_seconds_chirho" "$TIMEOUT_CHIRHO"
    append_metadata_chirho "$metadata_file_chirho" "ssh_forward_chirho" "tcp:127.0.0.1:${ssh_fwd_port_chirho}-:2222"
    append_metadata_chirho "$metadata_file_chirho" "base_rootfs_sha256_chirho" "$BASE_ROOTFS_HASH_CHIRHO"
    append_metadata_chirho "$metadata_file_chirho" "kernel_image_sha256_chirho" "$KERNEL_IMAGE_HASH_CHIRHO"
    append_metadata_chirho "$metadata_file_chirho" "scratch_initial_sha256_chirho" "$scratch_initial_hash_chirho"
    append_metadata_chirho "$metadata_file_chirho" "scratch_path_chirho" "$CURRENT_SCRATCH_CHIRHO"

    qemu_command_chirho=(
        qemu-system-x86_64
        -enable-kvm
        -m "$MEMORY_CHIRHO"
        -smp "$SMP_CHIRHO"
        -machine q35
        -cpu "$CPU_MODEL_CHIRHO"
        # The BIOS disk expects writable block semantics. QEMU's temporary
        # snapshot overlay supplies them without mutating the hashed source.
        -drive "format=raw,snapshot=on,file=$KERNEL_IMAGE_CHIRHO"
        -drive "file=$CURRENT_SCRATCH_CHIRHO,format=raw,if=virtio"
        -netdev "user,id=net0-chirho,hostfwd=tcp:127.0.0.1:${ssh_fwd_port_chirho}-:2222"
        -device "virtio-net-pci,netdev=net0-chirho"
        -serial "file:$serial_log_chirho"
        -monitor "unix:$CURRENT_MONITOR_CHIRHO,server=on,wait=off"
        -display none
        -no-reboot
        -name "lineluya-x11-${attempt_number_chirho}-chirho"
    )

    : >"$qemu_command_file_chirho"
    for qemu_argument_chirho in "${qemu_command_chirho[@]}"; do
        printf '%q ' "$qemu_argument_chirho" >>"$qemu_command_file_chirho"
    done
    printf '\n' >>"$qemu_command_file_chirho"

    echo "[$attempt_label_chirho] starting on loopback port $ssh_fwd_port_chirho"
    start_epoch_chirho="$(date +%s)"
    "${qemu_command_chirho[@]}" 2>"$qemu_stderr_chirho" &
    QEMU_PID_CHIRHO=$!

    local startup_wait_chirho=0
    while [ ! -S "$CURRENT_MONITOR_CHIRHO" ] && [ "$startup_wait_chirho" -lt 50 ]; do
        if ! kill -0 "$QEMU_PID_CHIRHO" 2>/dev/null; then
            break
        fi
        sleep 0.1
        startup_wait_chirho=$((startup_wait_chirho + 1))
    done
    if ! kill -0 "$QEMU_PID_CHIRHO" 2>/dev/null || [ ! -S "$CURRENT_MONITOR_CHIRHO" ]; then
        first_divergence_chirho="qemu_startup_chirho"
        fatal_line_chirho="$(head -n 1 "$qemu_stderr_chirho" || true)"
    fi

    while [ "$elapsed_chirho" -lt "$TIMEOUT_CHIRHO" ] \
        && [ "$first_divergence_chirho" = "none_chirho" ]; do
        if ! kill -0 "$QEMU_PID_CHIRHO" 2>/dev/null; then
            qemu_ended_early_chirho=1
            first_divergence_chirho="qemu_exited_before_gate_chirho"
            break
        fi

        sleep 1
        elapsed_chirho=$(($(date +%s) - start_epoch_chirho))

        observe_milestone_chirho \
            xorg_launch_chirho \
            '\[DESKTOP\] Xorg launched' \
            "$serial_log_chirho" "$elapsed_chirho" "$timeline_file_chirho"
        observe_milestone_chirho \
            display_bind_chirho \
            '\[X11-BRINGUP\] display socket bound' \
            "$serial_log_chirho" "$elapsed_chirho" "$timeline_file_chirho"
        observe_milestone_chirho \
            xkbcomp_exec_chirho \
            '\[EXEC\] pid=[0-9]+ path="/usr/bin/xkbcomp"' \
            "$serial_log_chirho" "$elapsed_chirho" "$timeline_file_chirho"
        observe_milestone_chirho \
            xorg_main_loop_chirho \
            '\[XORG-MAIN-LOOP\].*entered epoll_(wait|pwait)' \
            "$serial_log_chirho" "$elapsed_chirho" "$timeline_file_chirho"
        observe_milestone_chirho \
            authentic_setup_reply_chirho \
            '\[DESKTOP\] Xorg returned an authentic XCB setup reply' \
            "$serial_log_chirho" "$elapsed_chirho" "$timeline_file_chirho"
        observe_milestone_chirho \
            twm_ownership_chirho \
            '\[DESKTOP\] twm owns SubstructureRedirect on the root window' \
            "$serial_log_chirho" "$elapsed_chirho" "$timeline_file_chirho"
        observe_milestone_chirho \
            clients_launched_chirho \
            '\[DESKTOP\] clients launched: twm=[0-9]+ xterm=[0-9]+ xgears=[0-9]+' \
            "$serial_log_chirho" "$elapsed_chirho" "$timeline_file_chirho"
        observe_milestone_chirho \
            xgears_exec_chirho \
            '\[EXEC\] pid=[0-9]+ path="/usr/bin/xgears-chirho"' \
            "$serial_log_chirho" "$elapsed_chirho" "$timeline_file_chirho"
        observe_milestone_chirho \
            first_fps_chirho \
            'xgears-chirho: [0-9]+\.[0-9]+ FPS \([1-9][0-9]* frames in ' \
            "$serial_log_chirho" "$elapsed_chirho" "$timeline_file_chirho"
        observe_milestone_chirho \
            xterm_pty_shell_chirho \
            '\[XTERM-PTY\] shell marker chirho' \
            "$serial_log_chirho" "$elapsed_chirho" "$timeline_file_chirho"

        if [ -z "$xkbcomp_pid_chirho" ]; then
            xkbcomp_pid_chirho="$(grep -am1 '\[EXEC\] pid=[0-9][0-9]* path="/usr/bin/xkbcomp"' "$serial_log_chirho" \
                | sed -E 's/.*\[EXEC\] pid=([0-9]+).*/\1/' || true)"
        fi
        if [ -n "$xkbcomp_pid_chirho" ]; then
            observe_milestone_chirho \
                xkbcomp_exit_zero_chirho \
                "\\[PROCESS\\] wait4: reaped child PID=${xkbcomp_pid_chirho}, exit_code=0" \
                "$serial_log_chirho" "$elapsed_chirho" "$timeline_file_chirho"
        fi

        if [ -n "${MILESTONE_SECONDS_CHIRHO[xorg_launch_chirho]+present_chirho}" ] \
            && [ -z "${MILESTONE_SECONDS_CHIRHO[framebuffer_before_chirho]+present_chirho}" ]; then
            if capture_screendump_chirho "$before_ppm_chirho" \
                && capture_physical_framebuffer_chirho \
                    "$serial_log_chirho" "$before_raw_chirho" "$metadata_file_chirho" before; then
                MILESTONE_SECONDS_CHIRHO[framebuffer_before_chirho]="$elapsed_chirho"
                printf '%s\t%s\n' "$elapsed_chirho" "framebuffer_before_chirho" >>"$timeline_file_chirho"
            else
                first_divergence_chirho="framebuffer_before_capture_chirho"
                fatal_line_chirho="failed to capture the logged physical framebuffer range"
                break
            fi
        fi

        if all_runtime_milestones_present_chirho \
            && [ -z "${MILESTONE_SECONDS_CHIRHO[framebuffer_after_chirho]+present_chirho}" ]; then
            if [ $((elapsed_chirho + FRAMEBUFFER_SETTLE_SECONDS_CHIRHO)) -gt "$TIMEOUT_CHIRHO" ]; then
                first_divergence_chirho="framebuffer_after_timeout_chirho"
                break
            fi
            sleep "$FRAMEBUFFER_SETTLE_SECONDS_CHIRHO"
            elapsed_chirho=$(($(date +%s) - start_epoch_chirho))
            if capture_screendump_chirho "$after_ppm_chirho" \
                && capture_physical_framebuffer_chirho \
                    "$serial_log_chirho" "$after_raw_chirho" "$metadata_file_chirho" after; then
                elapsed_chirho=$(($(date +%s) - start_epoch_chirho))
                if [ "$elapsed_chirho" -gt "$TIMEOUT_CHIRHO" ]; then
                    first_divergence_chirho="framebuffer_after_timeout_chirho"
                    break
                fi
                MILESTONE_SECONDS_CHIRHO[framebuffer_after_chirho]="$elapsed_chirho"
                printf '%s\t%s\n' "$elapsed_chirho" "framebuffer_after_chirho" >>"$timeline_file_chirho"
                post_success_start_chirho="$elapsed_chirho"
            else
                first_divergence_chirho="framebuffer_after_capture_chirho"
                fatal_line_chirho="failed to capture the logged physical framebuffer range"
                break
            fi
        fi

        fatal_line_chirho="$(fatal_serial_line_chirho "$serial_log_chirho")"
        if [ -n "$fatal_line_chirho" ]; then
            first_divergence_chirho="fatal_guest_signal_chirho"
            break
        fi

        if [ -n "${MILESTONE_SECONDS_CHIRHO[framebuffer_after_chirho]+present_chirho}" ]; then
            if [ $((elapsed_chirho - post_success_start_chirho)) -ge "$POST_SUCCESS_OBSERVE_CHIRHO" ]; then
                break
            fi
        fi

        if [ $((elapsed_chirho % 10)) -eq 0 ]; then
            printf '[%s] %ss lines=%s main-loop=%s twm=%s xterm=%s fps=%s\n' \
                "$attempt_label_chirho" \
                "$elapsed_chirho" \
                "$(wc -l <"$serial_log_chirho")" \
                "$(count_pattern_chirho '\[XORG-MAIN-LOOP\]' "$serial_log_chirho")" \
                "$(count_pattern_chirho 'twm owns SubstructureRedirect' "$serial_log_chirho")" \
                "$(count_pattern_chirho '\[XTERM-PTY\]' "$serial_log_chirho")" \
                "$(count_pattern_chirho 'xgears-chirho:.*FPS' "$serial_log_chirho")"
        fi
    done

    if [ "$qemu_ended_early_chirho" -eq 0 ] && kill -0 "$QEMU_PID_CHIRHO" 2>/dev/null; then
        send_hmp_chirho "quit" "$attempt_dir_chirho/qemu-quit-monitor-chirho.log" || true
        local stop_wait_chirho=0
        while kill -0 "$QEMU_PID_CHIRHO" 2>/dev/null && [ "$stop_wait_chirho" -lt 30 ]; do
            sleep 0.1
            stop_wait_chirho=$((stop_wait_chirho + 1))
        done
        if kill -0 "$QEMU_PID_CHIRHO" 2>/dev/null; then
            kill "$QEMU_PID_CHIRHO" 2>/dev/null || true
        fi
    fi
    set +e
    wait "$QEMU_PID_CHIRHO" 2>/dev/null
    qemu_exit_chirho=$?
    set -e
    QEMU_PID_CHIRHO=""

    sync "$CURRENT_SCRATCH_CHIRHO" 2>/dev/null || true
    scratch_final_hash_chirho="$(hash_file_chirho "$CURRENT_SCRATCH_CHIRHO")"
    base_hash_after_chirho="$(hash_file_chirho "$BASE_ROOTFS_CHIRHO")"
    kernel_hash_after_chirho="$(hash_file_chirho "$KERNEL_IMAGE_CHIRHO")"
    if [ "$base_hash_after_chirho" != "$BASE_ROOTFS_HASH_CHIRHO" ] \
        && [ "$first_divergence_chirho" = "none_chirho" ]; then
        first_divergence_chirho="immutable_base_changed_chirho"
        fatal_line_chirho="base rootfs hash changed during attempt"
    fi
    if [ "$kernel_hash_after_chirho" != "$KERNEL_IMAGE_HASH_CHIRHO" ] \
        && [ "$first_divergence_chirho" = "none_chirho" ]; then
        first_divergence_chirho="immutable_kernel_image_changed_chirho"
        fatal_line_chirho="kernel image hash changed during attempt"
    fi

    launcher_count_chirho="$(count_pattern_chirho '\[EXEC\] pid=[0-9]+ path="/usr/local/sbin/start-lineluya-desktop-chirho.sh"' "$serial_log_chirho")"
    xorg_launch_count_chirho="$(count_pattern_chirho '\[DESKTOP\] Xorg launched' "$serial_log_chirho")"
    client_launch_count_chirho="$(count_pattern_chirho '\[DESKTOP\] clients launched:' "$serial_log_chirho")"
    synthesis_count_chirho="$(count_pattern_chirho 'X11-INJECT|X11-EVENT-INJECT|X11-BATCH|X11-REASSEM' "$serial_log_chirho")"
    trace_count_chirho="$(temporary_trace_count_chirho "$serial_log_chirho")"

    if [ -f "$before_raw_chirho" ]; then
        before_raw_hash_chirho="$(hash_file_chirho "$before_raw_chirho")"
    fi
    if [ -f "$after_raw_chirho" ]; then
        after_raw_hash_chirho="$(hash_file_chirho "$after_raw_chirho")"
        after_nonzero_bytes_chirho="$(LC_ALL=C tr -d '\000' <"$after_raw_chirho" | wc -c | tr -d ' ')"
    fi
    if [ -f "$before_ppm_chirho" ]; then
        before_ppm_hash_chirho="$(hash_file_chirho "$before_ppm_chirho")"
    fi
    if [ -f "$after_ppm_chirho" ]; then
        after_ppm_hash_chirho="$(hash_file_chirho "$after_ppm_chirho")"
    fi
    if [ -f "$before_raw_chirho" ] && [ -f "$after_raw_chirho" ]; then
        changed_raw_bytes_chirho="$({ cmp -l "$before_raw_chirho" "$after_raw_chirho" || true; } | wc -l | tr -d ' ')"
    fi

    if [ "$first_divergence_chirho" = "none_chirho" ]; then
        fatal_line_chirho="$(fatal_serial_line_chirho "$serial_log_chirho")"
        if [ -n "$fatal_line_chirho" ]; then
            first_divergence_chirho="fatal_guest_signal_chirho"
        fi
    fi
    if [ "$first_divergence_chirho" = "none_chirho" ]; then
        if [ -n "${MILESTONE_SECONDS_CHIRHO[framebuffer_after_chirho]+present_chirho}" ] \
            && [ $((elapsed_chirho - post_success_start_chirho)) -lt "$POST_SUCCESS_OBSERVE_CHIRHO" ]; then
            first_divergence_chirho="post_success_observation_timeout_chirho"
        fi
    fi
    if [ "$first_divergence_chirho" = "none_chirho" ]; then
        first_divergence_chirho="$(first_missing_milestone_chirho)"
    fi
    if [ "$first_divergence_chirho" = "none_chirho" ] \
        && [ "$qemu_exit_chirho" -ne 0 ]; then
        first_divergence_chirho="qemu_exit_${qemu_exit_chirho}_chirho"
    fi
    if [ "$first_divergence_chirho" = "none_chirho" ] \
        && [ -s "$qemu_stderr_chirho" ]; then
        first_divergence_chirho="qemu_stderr_chirho"
        fatal_line_chirho="$(head -n 1 "$qemu_stderr_chirho" || true)"
    fi
    if [ "$first_divergence_chirho" = "none_chirho" ] \
        && [ "$launcher_count_chirho" -ne 1 ]; then
        first_divergence_chirho="launcher_count_${launcher_count_chirho}_chirho"
    fi
    if [ "$first_divergence_chirho" = "none_chirho" ] \
        && [ "$xorg_launch_count_chirho" -ne 1 ]; then
        first_divergence_chirho="xorg_launch_count_${xorg_launch_count_chirho}_chirho"
    fi
    if [ "$first_divergence_chirho" = "none_chirho" ] \
        && [ "$client_launch_count_chirho" -ne 1 ]; then
        first_divergence_chirho="client_launch_count_${client_launch_count_chirho}_chirho"
    fi
    if [ "$first_divergence_chirho" = "none_chirho" ] \
        && [ "$synthesis_count_chirho" -ne 0 ]; then
        first_divergence_chirho="kernel_x11_synthesis_chirho"
    fi
    if [ "$first_divergence_chirho" = "none_chirho" ] \
        && [ "$REQUIRE_TRACE_FREE_CHIRHO" = "1" ] \
        && [ "$trace_count_chirho" -ne 0 ]; then
        first_divergence_chirho="temporary_trace_present_chirho"
    fi
    if [ "$first_divergence_chirho" = "none_chirho" ] \
        && { [ "$changed_raw_bytes_chirho" -eq 0 ] || [ "$after_nonzero_bytes_chirho" -eq 0 ]; }; then
        first_divergence_chirho="physical_framebuffer_unchanged_or_blank_chirho"
    fi
    if [ "$first_divergence_chirho" = "none_chirho" ]; then
        result_chirho="pass_chirho"
    fi

    framebuffer_width_chirho="$(grep -m1 '^framebuffer_before_width_chirho=' "$metadata_file_chirho" | cut -d= -f2 || true)"
    framebuffer_height_chirho="$(grep -m1 '^framebuffer_before_height_chirho=' "$metadata_file_chirho" | cut -d= -f2 || true)"
    if [ -n "$framebuffer_width_chirho" ] && [ -n "$framebuffer_height_chirho" ]; then
        convert_raw_framebuffer_chirho \
            "$before_raw_chirho" "$attempt_dir_chirho/framebuffer-before-chirho.png" \
            "$framebuffer_width_chirho" "$framebuffer_height_chirho" || true
        convert_raw_framebuffer_chirho \
            "$after_raw_chirho" "$attempt_dir_chirho/framebuffer-after-chirho.png" \
            "$framebuffer_width_chirho" "$framebuffer_height_chirho" || true
    fi

    write_milestones_chirho "$metadata_file_chirho"
    append_metadata_chirho "$metadata_file_chirho" "elapsed_seconds_chirho" "$elapsed_chirho"
    append_metadata_chirho "$metadata_file_chirho" "qemu_exit_chirho" "$qemu_exit_chirho"
    append_metadata_chirho "$metadata_file_chirho" "qemu_ended_early_chirho" "$qemu_ended_early_chirho"
    append_metadata_chirho "$metadata_file_chirho" "scratch_final_sha256_chirho" "$scratch_final_hash_chirho"
    append_metadata_chirho "$metadata_file_chirho" "base_rootfs_after_sha256_chirho" "$base_hash_after_chirho"
    append_metadata_chirho "$metadata_file_chirho" "kernel_image_after_sha256_chirho" "$kernel_hash_after_chirho"
    append_metadata_chirho "$metadata_file_chirho" "serial_sha256_chirho" "$(hash_file_chirho "$serial_log_chirho")"
    append_metadata_chirho "$metadata_file_chirho" "serial_lines_chirho" "$(wc -l <"$serial_log_chirho" | tr -d ' ')"
    append_metadata_chirho "$metadata_file_chirho" "qemu_stderr_sha256_chirho" "$(hash_file_chirho "$qemu_stderr_chirho")"
    append_metadata_chirho "$metadata_file_chirho" "launcher_count_chirho" "$launcher_count_chirho"
    append_metadata_chirho "$metadata_file_chirho" "xorg_launch_count_chirho" "$xorg_launch_count_chirho"
    append_metadata_chirho "$metadata_file_chirho" "client_launch_count_chirho" "$client_launch_count_chirho"
    append_metadata_chirho "$metadata_file_chirho" "synthesis_count_chirho" "$synthesis_count_chirho"
    append_metadata_chirho "$metadata_file_chirho" "temporary_trace_count_chirho" "$trace_count_chirho"
    append_metadata_chirho "$metadata_file_chirho" "framebuffer_before_raw_sha256_chirho" "$before_raw_hash_chirho"
    append_metadata_chirho "$metadata_file_chirho" "framebuffer_after_raw_sha256_chirho" "$after_raw_hash_chirho"
    append_metadata_chirho "$metadata_file_chirho" "framebuffer_before_ppm_sha256_chirho" "$before_ppm_hash_chirho"
    append_metadata_chirho "$metadata_file_chirho" "framebuffer_after_ppm_sha256_chirho" "$after_ppm_hash_chirho"
    append_metadata_chirho "$metadata_file_chirho" "framebuffer_changed_bytes_chirho" "$changed_raw_bytes_chirho"
    append_metadata_chirho "$metadata_file_chirho" "framebuffer_after_nonzero_bytes_chirho" "$after_nonzero_bytes_chirho"
    append_metadata_chirho "$metadata_file_chirho" "fatal_serial_line_chirho" "${fatal_line_chirho:-none_chirho}"
    append_metadata_chirho "$metadata_file_chirho" "first_divergence_chirho" "$first_divergence_chirho"
    append_metadata_chirho "$metadata_file_chirho" "result_chirho" "$result_chirho"

    if [ "$KEEP_SCRATCH_CHIRHO" != "1" ]; then
        rm -f "$CURRENT_SCRATCH_CHIRHO"
    fi
    rm -f "$CURRENT_MONITOR_CHIRHO"
    CURRENT_SCRATCH_CHIRHO=""
    CURRENT_MONITOR_CHIRHO=""

    echo "[$attempt_label_chirho] result=$result_chirho divergence=$first_divergence_chirho elapsed=${elapsed_chirho}s"
    [ "$result_chirho" = "pass_chirho" ]
}

main_chirho() {
    local required_command_chirho
    local source_status_chirho
    local source_short_chirho
    local cohort_metadata_chirho
    local forbidden_source_file_chirho
    local forbidden_source_count_chirho
    local forbidden_source_scan_status_chirho
    local dirty_patch_chirho
    local dirty_patch_hash_chirho="clean_chirho"
    local attempt_number_chirho
    local passed_attempts_chirho=0

    for required_command_chirho in \
        qemu-system-x86_64 socat sha256sum stat cp ss git sed grep cut cmp tr sync date head wc
    do
        require_command_chirho "$required_command_chirho"
    done
    [ -c /dev/kvm ] && [ -r /dev/kvm ] && [ -w /dev/kvm ] \
        || fatal_chirho "/dev/kvm is unavailable; native KVM is authoritative"
    [ "$CPU_MODEL_CHIRHO" = "qemu64" ] \
        || fatal_chirho "Gate D requires CPU_MODEL_CHIRHO=qemu64"

    require_unsigned_integer_chirho "RUN_COUNT_CHIRHO" "$RUN_COUNT_CHIRHO"
    require_unsigned_integer_chirho "BASE_SSH_FWD_PORT_CHIRHO" "$BASE_SSH_FWD_PORT_CHIRHO"
    require_unsigned_integer_chirho "TIMEOUT_CHIRHO" "$TIMEOUT_CHIRHO"
    require_unsigned_integer_chirho "POST_SUCCESS_OBSERVE_CHIRHO" "$POST_SUCCESS_OBSERVE_CHIRHO"
    require_unsigned_integer_chirho "FRAMEBUFFER_SETTLE_SECONDS_CHIRHO" "$FRAMEBUFFER_SETTLE_SECONDS_CHIRHO"
    require_unsigned_integer_chirho "SMP_CHIRHO" "$SMP_CHIRHO"
    [ "$RUN_COUNT_CHIRHO" -ge 1 ] || fatal_chirho "RUN_COUNT_CHIRHO must be at least 1"
    [ "$BASE_SSH_FWD_PORT_CHIRHO" -ge 1 ] || fatal_chirho "BASE_SSH_FWD_PORT_CHIRHO must be at least 1"
    [ "$TIMEOUT_CHIRHO" -ge 1 ] || fatal_chirho "TIMEOUT_CHIRHO must be at least 1"
    [ "$SMP_CHIRHO" -ge 1 ] || fatal_chirho "SMP_CHIRHO must be at least 1"
    [ "$TIMEOUT_CHIRHO" -le 400 ] || fatal_chirho "Gate D timeout may not exceed 400 seconds"
    [ $((BASE_SSH_FWD_PORT_CHIRHO + RUN_COUNT_CHIRHO - 1)) -le 65535 ] \
        || fatal_chirho "requested cohort port range exceeds 65535"
    [ -f "$KERNEL_IMAGE_CHIRHO" ] || fatal_chirho "missing kernel image: $KERNEL_IMAGE_CHIRHO"
    [ -f "$BASE_ROOTFS_CHIRHO" ] || fatal_chirho "missing base rootfs: $BASE_ROOTFS_CHIRHO"

    cd "$PROJECT_DIR_CHIRHO"
    git_chirho rev-parse --is-inside-work-tree >/dev/null 2>&1 \
        || fatal_chirho "PROJECT_DIR_CHIRHO is not a Git worktree"
    SOURCE_REVISION_CHIRHO="$(git_chirho rev-parse HEAD)"
    source_short_chirho="$(git_chirho rev-parse --short=12 HEAD)"
    source_status_chirho="$(git_chirho status --porcelain=v1 --untracked-files=all)"
    if [ "$REQUIRE_CLEAN_SOURCE_CHIRHO" = "1" ] && [ -n "$source_status_chirho" ]; then
        printf '%s\n' "$source_status_chirho" >&2
        fatal_chirho "acceptance evidence requires a clean identified revision"
    fi

    if [ -z "$COHORT_ID_CHIRHO" ]; then
        COHORT_ID_CHIRHO="$(date -u +%Y%m%dT%H%M%SZ)-${source_short_chirho}-chirho"
    fi
    case "$COHORT_ID_CHIRHO" in
        *[!A-Za-z0-9._-]*) fatal_chirho "COHORT_ID_CHIRHO contains unsafe characters" ;;
    esac
    COHORT_DIR_CHIRHO="$EVIDENCE_ROOT_CHIRHO/$COHORT_ID_CHIRHO"
    [ ! -e "$COHORT_DIR_CHIRHO" ] \
        || fatal_chirho "evidence directory already exists: $COHORT_DIR_CHIRHO"
    mkdir -p "$COHORT_DIR_CHIRHO"

    printf '%s\n' "$source_status_chirho" >"$COHORT_DIR_CHIRHO/source-status-chirho.txt"
    dirty_patch_chirho="$COHORT_DIR_CHIRHO/source-dirty-chirho.patch"
    git_chirho diff --binary HEAD >"$dirty_patch_chirho"
    if [ -s "$dirty_patch_chirho" ]; then
        dirty_patch_hash_chirho="$(hash_file_chirho "$dirty_patch_chirho")"
    else
        rm -f "$dirty_patch_chirho"
    fi

    cohort_metadata_chirho="$COHORT_DIR_CHIRHO/cohort-metadata-chirho.txt"
    : >"$cohort_metadata_chirho"
    forbidden_source_file_chirho="$COHORT_DIR_CHIRHO/forbidden-source-markers-chirho.txt"
    if grep -R -nE --include='*.rs' \
        "(\"\\[($TEMPORARY_TRACE_NAMES_CHIRHO|$FORBIDDEN_POLICY_MARKER_NAMES_CHIRHO)\\]|$FORBIDDEN_SOURCE_LITERAL_PATTERN_CHIRHO)" \
        "$PROJECT_DIR_CHIRHO/kernel-chirho/src" \
        >"$forbidden_source_file_chirho"; then
        forbidden_source_scan_status_chirho=0
    else
        forbidden_source_scan_status_chirho=$?
        [ "$forbidden_source_scan_status_chirho" -eq 1 ] \
            || fatal_chirho "forbidden-source scan failed with status $forbidden_source_scan_status_chirho"
    fi
    forbidden_source_count_chirho="$(wc -l <"$forbidden_source_file_chirho" | tr -d ' ')"
    append_metadata_chirho "$cohort_metadata_chirho" "source_revision_chirho" "$SOURCE_REVISION_CHIRHO"
    append_metadata_chirho "$cohort_metadata_chirho" "forbidden_source_marker_count_chirho" "$forbidden_source_count_chirho"
    append_metadata_chirho "$cohort_metadata_chirho" "forbidden_source_marker_sha256_chirho" "$(hash_file_chirho "$forbidden_source_file_chirho")"
    if [ "$REQUIRE_TRACE_FREE_CHIRHO" = "1" ] && [ "$forbidden_source_count_chirho" -ne 0 ]; then
        append_metadata_chirho "$cohort_metadata_chirho" "cohort_result_chirho" "fail_chirho"
        fatal_chirho "trace-free source gate found $forbidden_source_count_chirho forbidden marker(s); see $forbidden_source_file_chirho"
    fi
    BASE_ROOTFS_HASH_CHIRHO="$(hash_file_chirho "$BASE_ROOTFS_CHIRHO")"
    KERNEL_IMAGE_HASH_CHIRHO="$(hash_file_chirho "$KERNEL_IMAGE_CHIRHO")"
    append_metadata_chirho "$cohort_metadata_chirho" "cohort_id_chirho" "$COHORT_ID_CHIRHO"
    append_metadata_chirho "$cohort_metadata_chirho" "source_dirty_patch_sha256_chirho" "$dirty_patch_hash_chirho"
    append_metadata_chirho "$cohort_metadata_chirho" "acceptance_source_eligible_chirho" \
        "$([ -z "$source_status_chirho" ] \
            && [ "$REQUIRE_TRACE_FREE_CHIRHO" = "1" ] \
            && [ "$forbidden_source_count_chirho" -eq 0 ] \
            && echo 1 || echo 0)"
    append_metadata_chirho "$cohort_metadata_chirho" "run_count_chirho" "$RUN_COUNT_CHIRHO"
    append_metadata_chirho "$cohort_metadata_chirho" "cpu_model_chirho" "$CPU_MODEL_CHIRHO"
    append_metadata_chirho "$cohort_metadata_chirho" "memory_chirho" "$MEMORY_CHIRHO"
    append_metadata_chirho "$cohort_metadata_chirho" "smp_chirho" "$SMP_CHIRHO"
    append_metadata_chirho "$cohort_metadata_chirho" "timeout_seconds_chirho" "$TIMEOUT_CHIRHO"
    append_metadata_chirho "$cohort_metadata_chirho" "require_trace_free_chirho" "$REQUIRE_TRACE_FREE_CHIRHO"
    append_metadata_chirho "$cohort_metadata_chirho" "qemu_version_chirho" "$(qemu-system-x86_64 --version | head -n 1)"
    append_metadata_chirho "$cohort_metadata_chirho" "base_rootfs_sha256_chirho" "$BASE_ROOTFS_HASH_CHIRHO"
    record_hash_if_present_chirho "$cohort_metadata_chirho" "kernel_image" "$KERNEL_IMAGE_CHIRHO"
    record_hash_if_present_chirho "$cohort_metadata_chirho" "kernel_elf" "$KERNEL_ELF_CHIRHO"
    record_hash_if_present_chirho "$cohort_metadata_chirho" "rootfs_provisioner" "$PROJECT_DIR_CHIRHO/scripts-chirho/make-alpine-disk-chirho.sh"
    record_hash_if_present_chirho "$cohort_metadata_chirho" "desktop_launcher" "$PROJECT_DIR_CHIRHO/scripts-chirho/rootfs-chirho/start-lineluya-desktop-chirho.sh"
    record_hash_if_present_chirho "$cohort_metadata_chirho" "xorg_config" "$PROJECT_DIR_CHIRHO/scripts-chirho/rootfs-chirho/xorg-chirho.conf"
    record_hash_if_present_chirho "$cohort_metadata_chirho" "xgears_source" "$PROJECT_DIR_CHIRHO/userspace-chirho/x11-chirho/xgears_chirho.c"

    for ((attempt_number_chirho = 1; attempt_number_chirho <= RUN_COUNT_CHIRHO; attempt_number_chirho++)); do
        if run_attempt_chirho "$attempt_number_chirho"; then
            passed_attempts_chirho=$((passed_attempts_chirho + 1))
        else
            append_metadata_chirho "$cohort_metadata_chirho" "passed_attempts_chirho" "$passed_attempts_chirho"
            append_metadata_chirho "$cohort_metadata_chirho" "cohort_result_chirho" "fail_chirho"
            append_metadata_chirho "$cohort_metadata_chirho" "base_rootfs_final_sha256_chirho" "$(hash_file_chirho "$BASE_ROOTFS_CHIRHO")"
            append_metadata_chirho "$cohort_metadata_chirho" "kernel_image_final_sha256_chirho" "$(hash_file_chirho "$KERNEL_IMAGE_CHIRHO")"
            echo "cohort failed on attempt $attempt_number_chirho; no later attempts were selected" >&2
            exit 1
        fi
    done

    append_metadata_chirho "$cohort_metadata_chirho" "passed_attempts_chirho" "$passed_attempts_chirho"
    append_metadata_chirho "$cohort_metadata_chirho" "cohort_result_chirho" "pass_chirho"
    append_metadata_chirho "$cohort_metadata_chirho" "base_rootfs_final_sha256_chirho" "$(hash_file_chirho "$BASE_ROOTFS_CHIRHO")"
    append_metadata_chirho "$cohort_metadata_chirho" "kernel_image_final_sha256_chirho" "$(hash_file_chirho "$KERNEL_IMAGE_CHIRHO")"
    echo "cohort passed: $passed_attempts_chirho/$RUN_COUNT_CHIRHO"
    echo "$COHORT_DIR_CHIRHO"
}

main_chirho "$@"
