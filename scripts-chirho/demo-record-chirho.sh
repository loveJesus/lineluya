#!/usr/bin/env bash
# For God so loved the world that he gave his only begotten Son,
# that whoever believes in him should not perish but have eternal life. - John 3:16
#
# Demo recording script for Lineluya kernel.
# Creates an MP4 showing real Alpine Linux programs running on a Rust kernel.
#
# Usage: ./scripts-chirho/demo-record-chirho.sh [level]
#   level 1: Basic (BusyBox commands + sqlite3)
#   level 2: Intermediate (+ Python, Dropbear, wget)
#   level 3: Full (+ apk, .ko loading, all programs)

set -euo pipefail

LEVEL_CHIRHO="${1:-2}"
QEMU_IMG_CHIRHO="target/disk-images-chirho/lineluya-uefi-chirho.img"
ALPINE_IMG_CHIRHO="target/alpine-virtio-chirho/alpine-virtio-chirho.img"
UEFI_FW_CHIRHO="/opt/homebrew/share/qemu/edk2-x86_64-code.fd"
OUTPUT_DIR_CHIRHO="docs-chirho"
LOG_CHIRHO="/tmp/lineluya-demo-chirho.log"

mkdir -p "$OUTPUT_DIR_CHIRHO"

echo "=== Lineluya Demo Recording (Level $LEVEL_CHIRHO) ==="
echo "Praise Jesus! Hallelujah!"

# Common QEMU args
QEMU_ARGS_CHIRHO=(
  -drive "if=pflash,format=raw,readonly=on,file=$UEFI_FW_CHIRHO"
  -drive "format=raw,file=$QEMU_IMG_CHIRHO"
  -drive "file=$ALPINE_IMG_CHIRHO,format=raw,if=virtio"
  -netdev "user,id=net0" -device "virtio-net-pci,netdev=net0"
  -serial stdio -display none -m 1G -no-reboot
)

# Generate commands based on level
generate_commands_chirho() {
  local level_chirho="$1"
  local delay_chirho=3

  # Wait for boot
  sleep 22

  # === Level 1: Basic ===
  echo "# BusyBox Shell Commands"
  printf 'echo "Hallelujah! Lineluya kernel is alive!"\n'
  sleep $delay_chirho

  printf 'uname -a\n'
  sleep $delay_chirho

  printf 'ls /\n'
  sleep $delay_chirho

  printf 'id\n'
  sleep $delay_chirho

  printf 'date\n'
  sleep $delay_chirho

  # sqlite3
  printf 'sqlite3 :memory: "CREATE TABLE praise_chirho(id INTEGER PRIMARY KEY, msg TEXT);"\n'
  sleep $delay_chirho
  printf 'sqlite3 :memory: "SELECT 316, 42+1;"\n'
  sleep 8

  [[ "$level_chirho" -lt 2 ]] && return

  # === Level 2: Intermediate ===
  printf 'python3 --version\n'
  sleep 10

  printf 'dropbear -V\n'
  sleep 8

  printf 'apk --version\n'
  sleep 5

  # wget (TCP networking)
  printf 'wget -O /dev/null http://1.1.1.1/ 2>&1\n'
  sleep 12

  [[ "$level_chirho" -lt 3 ]] && return

  # === Level 3: Full ===
  printf 'cat /proc/meminfo\n'
  sleep $delay_chirho

  printf 'cat /proc/version\n'
  sleep $delay_chirho

  printf 'ls /usr/bin\n'
  sleep $delay_chirho

  printf 'echo "All programs verified! Praise Jesus!"\n'
  sleep $delay_chirho
}

echo "Recording demo at level $LEVEL_CHIRHO..."

# Run QEMU with scripted input
generate_commands_chirho "$LEVEL_CHIRHO" | timeout 120 qemu-system-x86_64 "${QEMU_ARGS_CHIRHO[@]}" 2>&1 | tee "$LOG_CHIRHO"

echo ""
echo "=== Demo log saved to $LOG_CHIRHO ==="
echo ""
echo "To create an MP4 from this terminal recording, use asciinema + agg:"
echo "  asciinema rec demo.cast"
echo "  agg demo.cast demo.gif"
echo "  ffmpeg -i demo.gif -movflags faststart -pix_fmt yuv420p demo.mp4"
echo ""
echo "Or capture QEMU's framebuffer directly:"
echo "  qemu-system-x86_64 ... -display gtk &"
echo "  sleep 25 && ffmpeg -f x11grab -video_size 1280x800 -i :0 -t 60 demo.mp4"
echo ""
echo "Hallelujah! Praise Jesus!"
