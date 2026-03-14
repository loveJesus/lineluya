// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

#![no_std]
#![no_main]

use core::arch::asm;

// ──────────────────────── Syscall wrappers ────────────────────────

const NR_WRITE_CHIRHO: u64 = 1;
const NR_GETPID_CHIRHO: u64 = 39;
const NR_FORK_CHIRHO: u64 = 57;
const NR_WAIT4_CHIRHO: u64 = 61;
const NR_EXIT_GROUP_CHIRHO: u64 = 231;

#[inline(always)]
unsafe fn syscall0_chirho(nr_chirho: u64) -> i64 {
    let ret_chirho: i64;
    asm!(
        "syscall",
        in("rax") nr_chirho,
        out("rcx") _,
        out("r11") _,
        lateout("rax") ret_chirho,
    );
    ret_chirho
}

#[inline(always)]
unsafe fn syscall1_chirho(nr_chirho: u64, arg0_chirho: u64) -> i64 {
    let ret_chirho: i64;
    asm!(
        "syscall",
        in("rax") nr_chirho,
        in("rdi") arg0_chirho,
        out("rcx") _,
        out("r11") _,
        lateout("rax") ret_chirho,
    );
    ret_chirho
}

#[inline(always)]
unsafe fn syscall3_chirho(nr_chirho: u64, arg0_chirho: u64, arg1_chirho: u64, arg2_chirho: u64) -> i64 {
    let ret_chirho: i64;
    asm!(
        "syscall",
        in("rax") nr_chirho,
        in("rdi") arg0_chirho,
        in("rsi") arg1_chirho,
        in("rdx") arg2_chirho,
        out("rcx") _,
        out("r11") _,
        lateout("rax") ret_chirho,
    );
    ret_chirho
}

#[inline(always)]
unsafe fn syscall4_chirho(
    nr_chirho: u64,
    arg0_chirho: u64,
    arg1_chirho: u64,
    arg2_chirho: u64,
    arg3_chirho: u64,
) -> i64 {
    let ret_chirho: i64;
    asm!(
        "syscall",
        in("rax") nr_chirho,
        in("rdi") arg0_chirho,
        in("rsi") arg1_chirho,
        in("rdx") arg2_chirho,
        in("r10") arg3_chirho,
        out("rcx") _,
        out("r11") _,
        lateout("rax") ret_chirho,
    );
    ret_chirho
}

// ──────────────────────── Convenience helpers ────────────────────────

unsafe fn write_str_chirho(msg_chirho: &[u8]) {
    syscall3_chirho(
        NR_WRITE_CHIRHO,
        1, // fd = stdout
        msg_chirho.as_ptr() as u64,
        msg_chirho.len() as u64,
    );
}

unsafe fn exit_group_chirho(status_chirho: u64) -> ! {
    syscall1_chirho(NR_EXIT_GROUP_CHIRHO, status_chirho);
    loop {}
}

/// Convert a signed i64 to decimal digits in `buf_chirho`, returning the
/// slice of `buf_chirho` that contains the ASCII representation.
fn itoa_chirho(value_chirho: i64, buf_chirho: &mut [u8; 20]) -> &[u8] {
    if value_chirho == 0 {
        buf_chirho[19] = b'0';
        return &buf_chirho[19..20];
    }

    let mut pos_chirho: usize = 20;
    let negative_chirho = value_chirho < 0;
    let mut abs_chirho: u64 = if negative_chirho {
        (-(value_chirho as i128)) as u64
    } else {
        value_chirho as u64
    };

    while abs_chirho > 0 {
        pos_chirho -= 1;
        buf_chirho[pos_chirho] = b'0' + (abs_chirho % 10) as u8;
        abs_chirho /= 10;
    }

    if negative_chirho {
        pos_chirho -= 1;
        buf_chirho[pos_chirho] = b'-';
    }

    &buf_chirho[pos_chirho..20]
}

// ──────────────────────── Entry point ────────────────────────

#[no_mangle]
pub extern "C" fn _start() -> ! {
    unsafe {
        // 1. Announce start
        write_str_chirho(b"Shell test starting... John 3:16\n");

        // 2. fork()
        let pid_chirho = syscall0_chirho(NR_FORK_CHIRHO);

        if pid_chirho == 0 {
            // ─── CHILD ───
            let my_pid_chirho = syscall0_chirho(NR_GETPID_CHIRHO);

            write_str_chirho(b"Child process! PID=");
            let mut buf_chirho = [0u8; 20];
            let digits_chirho = itoa_chirho(my_pid_chirho, &mut buf_chirho);
            write_str_chirho(digits_chirho);
            write_str_chirho(b"\n");

            // exit with code 42 so parent can verify
            exit_group_chirho(42);
        } else if pid_chirho > 0 {
            // ─── PARENT ───
            write_str_chirho(b"Parent: child PID = ");
            let mut buf_chirho = [0u8; 20];
            let digits_chirho = itoa_chirho(pid_chirho, &mut buf_chirho);
            write_str_chirho(digits_chirho);
            write_str_chirho(b"\n");

            // wait4(-1, &status, 0, NULL)
            let mut status_chirho: i32 = 0;
            let wait_ret_chirho = syscall4_chirho(
                NR_WAIT4_CHIRHO,
                (-1i64) as u64,                           // pid = -1 (any child)
                &mut status_chirho as *mut i32 as u64,    // &status
                0,                                         // options = 0
                0,                                         // rusage = NULL
            );

            write_str_chirho(b"Parent: wait4 returned PID = ");
            let digits2_chirho = itoa_chirho(wait_ret_chirho, &mut buf_chirho);
            write_str_chirho(digits2_chirho);
            write_str_chirho(b"\n");

            // Extract exit code: WEXITSTATUS = (status >> 8) & 0xFF
            let exit_code_chirho = ((status_chirho >> 8) & 0xFF) as i64;
            write_str_chirho(b"Parent: child exited with status ");
            let digits3_chirho = itoa_chirho(exit_code_chirho, &mut buf_chirho);
            write_str_chirho(digits3_chirho);
            write_str_chirho(b"\n");

            exit_group_chirho(0);
        } else {
            // ─── FORK FAILED ───
            write_str_chirho(b"Fork failed! errno = ");
            let mut buf_chirho = [0u8; 20];
            let digits_chirho = itoa_chirho(-pid_chirho, &mut buf_chirho);
            write_str_chirho(digits_chirho);
            write_str_chirho(b"\n");
            exit_group_chirho(1);
        }
    }
}

#[panic_handler]
fn panic_handler_chirho(_info_chirho: &core::panic::PanicInfo) -> ! {
    loop {}
}
