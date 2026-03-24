# For God so loved the world that he gave his only begotten Son,
# that whoever believes in him should not perish but have eternal life. - John 3:16
#
# GDB script to debug the deterministic Xorg NULL-deref crash in Lineluya.
#
# The crash:
#   RIP  = 0x7f0000146150  (musl offset 0x46150, inside malloc chunk mgmt)
#   addr = 0x2b33d         (the faulting memory access — near-NULL pointer)
#   RSP  = 0x7ffffbffec38
#   Instruction: movzbl -0x3(%rdi),%edx  → RDI = 0x2b340 (corrupted chunk ptr)
#
# This is NOT strchrnul. It is musl's internal malloc group/bin management code
# (static function past aligned_alloc, file offset 0x4602a..0x464xx). The crash
# happens because a malloc chunk header pointer (RDI) is near-NULL (0x2b340),
# meaning heap metadata was corrupted — likely by a use-after-munmap or
# double-free in Xorg's initialization path.
#
# Usage:
#   1. Start QEMU with -s -S  (see qemu-xorg-debug-chirho.sh)
#   2. In another terminal:
#        gdb -x /path/to/gdb-xorg-crash-chirho.gdb
#   3. GDB connects, loads symbols, sets breakpoints, and continues.
#   4. When the crash triggers, the script prints the full backtrace.
#
# Prerequisites:
#   - Copy Xorg and musl from the Alpine rootfs to /tmp/:
#       sudo losetup --show -fP target/alpine-virtio-chirho/alpine-virtio-chirho.img
#       sudo mount -o ro /dev/loopN /mnt
#       cp /mnt/usr/libexec/Xorg /tmp/Xorg-chirho
#       cp /mnt/lib/ld-musl-x86_64.so.1 /tmp/ld-musl-chirho.so
#       sudo umount /mnt && sudo losetup -d /dev/loopN
#   - Install gdb:  sudo apt install gdb

# ============================================================================
# Connection
# ============================================================================
set confirm off
set pagination off
set print pretty on

# Connect to QEMU GDB stub
target remote localhost:1234

# ============================================================================
# Symbol Loading
# ============================================================================
# Lineluya loads PIE executables at fixed base addresses:
#   Xorg (PIE ET_DYN):  base = 0x555555550000
#   musl interpreter:   base = 0x7F0000100000
#
# add-symbol-file takes the .text section address.
# Xorg .text is at file offset 0x218c0, so .text VA = base + 0x218c0
# musl .text is at file offset 0x14000, so .text VA = base + 0x14000

# Load Xorg symbols (stripped, but has .dynsym + .eh_frame for unwinding)
add-symbol-file /tmp/Xorg-chirho 0x5555555718c0 \
    -s .rodata  0x5555556a3000 \
    -s .data    0x555555b15c60 \
    -s .bss     0x555555b1a760 \
    -s .plt     0x555555570010 \
    -s .got.plt 0x555555b14fe8

# Load musl symbols (has .dynsym — strchrnul, malloc, free, etc.)
add-symbol-file /tmp/ld-musl-chirho.so 0x7f0000114000 \
    -s .rodata  0x7f000016c000 \
    -s .data    0x7f00001a2aa0 \
    -s .bss     0x7f00001a3408 \
    -s .got.plt 0x7f00001a2f30

# ============================================================================
# Strategy 1: Break at the exact crash instruction
# ============================================================================
# The crash is deterministic at RIP=0x7f0000146150.
# This instruction is: movzbl -0x3(%rdi),%edx
# It faults when RDI is a near-NULL corrupted chunk pointer.
#
# We set a breakpoint there and check RDI. If RDI is sane (> 0x100000),
# we continue. When it's bad, we get the backtrace.

break *0x7f0000146150
commands
    silent
    # Check if RDI is the bad pointer (near-NULL, < 1MB)
    if $rdi < 0x100000
        printf "\n===== XORG CRASH: CORRUPTED MALLOC CHUNK PTR =====\n"
        printf "RDI (chunk ptr) = 0x%lx\n", $rdi
        printf "Access addr     = RDI-3 = 0x%lx\n", $rdi - 3
        printf "RIP             = 0x%lx\n", $rip
        printf "RSP             = 0x%lx\n", $rsp
        printf "\n--- Register State ---\n"
        info registers
        printf "\n--- Backtrace (DWARF .eh_frame unwinding) ---\n"
        backtrace 30
        printf "\n--- Stack Dump (16 qwords from RSP) ---\n"
        x/16gx $rsp
        printf "\n--- Disassembly at crash point ---\n"
        x/5i $rip
        printf "\n===== END CRASH DUMP =====\n"
        printf "\nThe backtrace above shows which Xorg function called\n"
        printf "malloc/realloc/free with a corrupted heap. Look for the\n"
        printf "first frame with an address in 0x555555xxxxxx range.\n\n"
    else
        # RDI is sane — this is a normal malloc call, continue
        continue
    end
end

# ============================================================================
# Strategy 2: Break on malloc/free entry to trace allocation patterns
# ============================================================================
# These are disabled by default. Uncomment to trace all malloc/free calls
# leading up to the crash. WARNING: extremely verbose.

# break *0x7f0000145cde
# commands
#     silent
#     printf "[malloc] size=%lu rsp=0x%lx\n", $rdi, $rsp
#     backtrace 5
#     continue
# end
#
# break *0x7f0000145cd4
# commands
#     silent
#     printf "[free] ptr=0x%lx rsp=0x%lx\n", $rdi, $rsp
#     backtrace 5
#     continue
# end

# ============================================================================
# Strategy 3: Catchpoint on page fault for addr < 0x100000
# ============================================================================
# QEMU GDB stub does not support catch signal, but we can set a hardware
# watchpoint on CR2 (not directly possible) or break on the kernel's page
# fault handler and check the faulting address.
#
# The kernel page_fault_handler_chirho is at symbol offset 0x155764 in the
# kernel binary. Since the kernel is a PIE loaded by bootloader_api,
# we can try to break by address if we know the kernel load base.
# For now, Strategy 1 (direct RIP break) is more reliable.

# ============================================================================
# Strategy 4: Watch the heap metadata region
# ============================================================================
# If the backtrace from Strategy 1 is unhelpful (all frames in musl),
# we can set a hardware watchpoint on the corrupted address to catch
# whatever WROTE the bad value.
#
# Uncomment after first run to catch the corruption:
# watch *(uint64_t*)0x7f00001a3b10
# commands
#     printf "\n===== HEAP METADATA WRITE =====\n"
#     info registers
#     backtrace 20
# end

# ============================================================================
# Let it run
# ============================================================================
printf "\n[gdb-xorg-crash-chirho] Breakpoint set at 0x7f0000146150\n"
printf "[gdb-xorg-crash-chirho] Waiting for Xorg to hit corrupted chunk...\n"
printf "[gdb-xorg-crash-chirho] Boot the kernel, then run: Xorg :0\n\n"
continue
