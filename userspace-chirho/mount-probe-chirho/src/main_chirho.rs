// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

#![no_std]
#![no_main]

use core::arch::asm;

const NR_READ_CHIRHO: u64 = 0;
const NR_WRITE_CHIRHO: u64 = 1;
const NR_OPENAT_CHIRHO: u64 = 257;
const NR_EXIT_GROUP_CHIRHO: u64 = 231;

const AT_FDCWD_CHIRHO: i64 = -100;
const O_RDONLY_CHIRHO: u64 = 0;

const TARGET_PATH_CHIRHO: &[u8] = b"/mnt/matthew712_chirho.txt\0";
const START_MSG_CHIRHO: &[u8] = b"[mount-probe] start\n";
const OPEN_MSG_CHIRHO: &[u8] = b"[mount-probe] openat ok fd=";
const READ_MSG_CHIRHO: &[u8] = b"[mount-probe] read bytes=";
const ERR_OPEN_MSG_CHIRHO: &[u8] = b"[mount-probe] openat errno=";
const ERR_READ_MSG_CHIRHO: &[u8] = b"[mount-probe] read errno=";
const DONE_MSG_CHIRHO: &[u8] = b"\n[mount-probe] done\n";
const NEWLINE_CHIRHO: &[u8] = b"\n";

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
unsafe fn syscall3_chirho(
    nr_chirho: u64,
    arg0_chirho: u64,
    arg1_chirho: u64,
    arg2_chirho: u64,
) -> i64 {
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

unsafe fn write_all_chirho(buf_chirho: &[u8]) {
    let mut offset_chirho = 0usize;
    while offset_chirho < buf_chirho.len() {
        let write_ret_chirho = syscall3_chirho(
            NR_WRITE_CHIRHO,
            1,
            buf_chirho[offset_chirho..].as_ptr() as u64,
            (buf_chirho.len() - offset_chirho) as u64,
        );
        if write_ret_chirho <= 0 {
            break;
        }
        offset_chirho += write_ret_chirho as usize;
    }
}

unsafe fn exit_group_chirho(status_chirho: u64) -> ! {
    syscall1_chirho(NR_EXIT_GROUP_CHIRHO, status_chirho);
    loop {}
}

fn itoa_chirho(value_chirho: i64, digits_buf_chirho: &mut [u8; 32]) -> &[u8] {
    if value_chirho == 0 {
        digits_buf_chirho[31] = b'0';
        return &digits_buf_chirho[31..32];
    }

    let negative_chirho = value_chirho < 0;
    let mut magnitude_chirho: u64 = if negative_chirho {
        (-(value_chirho as i128)) as u64
    } else {
        value_chirho as u64
    };

    let mut pos_chirho = digits_buf_chirho.len();
    while magnitude_chirho > 0 {
        pos_chirho -= 1;
        digits_buf_chirho[pos_chirho] = b'0' + (magnitude_chirho % 10) as u8;
        magnitude_chirho /= 10;
    }

    if negative_chirho {
        pos_chirho -= 1;
        digits_buf_chirho[pos_chirho] = b'-';
    }

    &digits_buf_chirho[pos_chirho..]
}

#[no_mangle]
pub extern "C" fn _start() -> ! {
    unsafe {
        write_all_chirho(START_MSG_CHIRHO);

        let fd_chirho = syscall4_chirho(
            NR_OPENAT_CHIRHO,
            AT_FDCWD_CHIRHO as u64,
            TARGET_PATH_CHIRHO.as_ptr() as u64,
            O_RDONLY_CHIRHO,
            0,
        );

        let mut digits_buf_chirho = [0u8; 32];
        if fd_chirho < 0 {
            write_all_chirho(ERR_OPEN_MSG_CHIRHO);
            write_all_chirho(itoa_chirho(-fd_chirho, &mut digits_buf_chirho));
            write_all_chirho(NEWLINE_CHIRHO);
            exit_group_chirho(2);
        }

        write_all_chirho(OPEN_MSG_CHIRHO);
        write_all_chirho(itoa_chirho(fd_chirho, &mut digits_buf_chirho));
        write_all_chirho(NEWLINE_CHIRHO);

        let mut read_buf_chirho = [0u8; 256];
        loop {
            let read_ret_chirho = syscall3_chirho(
                NR_READ_CHIRHO,
                fd_chirho as u64,
                read_buf_chirho.as_mut_ptr() as u64,
                read_buf_chirho.len() as u64,
            );

            if read_ret_chirho == 0 {
                break;
            }

            if read_ret_chirho < 0 {
                write_all_chirho(ERR_READ_MSG_CHIRHO);
                write_all_chirho(itoa_chirho(-read_ret_chirho, &mut digits_buf_chirho));
                write_all_chirho(NEWLINE_CHIRHO);
                exit_group_chirho(3);
            }

            write_all_chirho(READ_MSG_CHIRHO);
            write_all_chirho(itoa_chirho(read_ret_chirho, &mut digits_buf_chirho));
            write_all_chirho(NEWLINE_CHIRHO);
            write_all_chirho(&read_buf_chirho[..read_ret_chirho as usize]);
        }

        write_all_chirho(DONE_MSG_CHIRHO);
        exit_group_chirho(0);
    }
}

#[panic_handler]
fn panic_handler_chirho(_panic_info_chirho: &core::panic::PanicInfo) -> ! {
    loop {}
}
