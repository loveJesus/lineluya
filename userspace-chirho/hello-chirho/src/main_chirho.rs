// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

#![no_std]
#![no_main]

use core::arch::asm;

const MSG_CHIRHO: &[u8] = b"Hello from Lineluya userspace! John 3:16\n";

#[no_mangle]
pub extern "C" fn _start() -> ! {
    let msg_ptr_chirho = MSG_CHIRHO.as_ptr();
    let msg_len_chirho = MSG_CHIRHO.len();

    // sys_write(1, msg, len)
    unsafe {
        asm!(
            "syscall",
            in("rax") 1u64,                    // __NR_write
            in("rdi") 1u64,                    // fd = stdout
            in("rsi") msg_ptr_chirho as u64,
            in("rdx") msg_len_chirho as u64,
            out("rcx") _,
            out("r11") _,
        );
    }

    // sys_exit_group(0)
    unsafe {
        asm!(
            "syscall",
            in("rax") 231u64,    // __NR_exit_group
            in("rdi") 0u64,      // status = 0
            options(noreturn),
        );
    }
}

#[panic_handler]
fn panic_handler_chirho(_info_chirho: &core::panic::PanicInfo) -> ! {
    loop {}
}
