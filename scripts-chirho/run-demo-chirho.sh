#!/bin/bash
# For God so loved the world that he gave his only begotten Son,
# that whoever believes in him should not perish but have eternal life. - John 3:16
#
# Lineluya Kernel Demo — v5.0 "Thank You Jesus Christ"
# A Linux kernel rewrite in Rust — 70,000+ lines
#
# Usage:
#   VM=hallelujah@10.211.55.7 DELAY=12 bash scripts-chirho/run-demo-chirho.sh  # Parallels
#   VM=ubuntu@54.208.176.168 KEY=~/.ssh/fpga-benchmark-chirho.pem DELAY=12 bash scripts-chirho/run-demo-chirho.sh  # EC2

set -euo pipefail

VM_CHIRHO="${VM:-ubuntu@54.208.176.168}"
KEY_CHIRHO="${KEY:-${HOME}/.ssh/fpga-benchmark-chirho.pem}"
FIFO_CHIRHO="${FIFO:-/tmp/dffi}"
LOG_CHIRHO="${LOG:-/tmp/df.log}"
DELAY_CHIRHO="${DELAY:-12}"

SSH_CHIRHO="ssh -i $KEY_CHIRHO -o StrictHostKeyChecking=no $VM_CHIRHO"

send_chirho() {
    echo "  lineluya# $1"
    $SSH_CHIRHO "echo '$1' > $FIFO_CHIRHO"
    sleep "${2:-$DELAY_CHIRHO}"
}

result_chirho() {
    local pattern_chirho="$1"
    local label_chirho="$2"
    local val_chirho
    val_chirho=$($SSH_CHIRHO "grep '$pattern_chirho' $LOG_CHIRHO 2>/dev/null | grep -v '\\[' | head -1" 2>/dev/null || true)
    if [ -n "$val_chirho" ]; then
        echo "  ✓ $label_chirho: $val_chirho"
    else
        echo "  ✗ $label_chirho: not found"
    fi
}

echo ""
echo "============================================================"
echo "  Lineluya — A Linux Kernel Rewrite in Rust"
echo "  v5.0 Thank You Jesus Christ — 70,000+ lines"
echo "============================================================"
echo ""

send_chirho "uname -a" "$DELAY_CHIRHO"
send_chirho "sqlite3 :memory: 'SELECT 316, 42+1;'" "$DELAY_CHIRHO"
send_chirho "python3 -c 'print(42)'" 18
send_chirho "losetup /dev/loop0 /root/loop_demo_chirho.img" 6
send_chirho "mount -t ext4 /dev/loop0 /mnt" 15
send_chirho "cat /mnt/matthew712_chirho.txt" "$DELAY_CHIRHO"
send_chirho "echo Hallelujah Thank You Jesus Christ" 5

echo ""
echo "============================================================"
echo "  Results"
echo "============================================================"
echo ""

result_chirho "Lineluya lineluya" "uname -a"
result_chirho "316" "sqlite3 SELECT"
result_chirho "^42$" "python3 print(42)"
result_chirho "whatever.*do to you" "Matthew 7:12"

MOUNT_CHIRHO=$($SSH_CHIRHO "grep 'MOUNT.*read back' $LOG_CHIRHO | head -1" 2>/dev/null || true)
if [ -n "$MOUNT_CHIRHO" ]; then
    echo "  ✓ ext4 write+readback: $MOUNT_CHIRHO"
else
    echo "  ✗ ext4 write+readback: not found"
fi

PROMPTS_CHIRHO=$($SSH_CHIRHO "grep -c 'lineluya#' $LOG_CHIRHO" 2>/dev/null || echo 0)
PANICS_CHIRHO=$($SSH_CHIRHO "grep -c 'panic' $LOG_CHIRHO" 2>/dev/null || echo 0)
echo ""
echo "  Prompts returned: $PROMPTS_CHIRHO"
echo "  Kernel panics: $PANICS_CHIRHO"
echo ""
echo "============================================================"
echo "  Hallelujah! Thank You Jesus Christ!"
echo "============================================================"
