# For God so loved the world that he gave his only begotten Son,
# that whoever believes in him should not perish but have eternal life. - John 3:16

.section .text
.global _start
_start:
    # sys_write(1, message, length)
    mov $1, %rax        # syscall number: write
    mov $1, %rdi        # fd: stdout
    lea message(%rip), %rsi   # buf: message address
    mov $msg_len, %rdx  # count: message length
    syscall

    # sys_exit_group(0)
    mov $231, %rax      # syscall number: exit_group
    xor %rdi, %rdi      # status: 0
    syscall

.section .rodata
message:
    .ascii "Hello from Lineluya userspace! John 3:16\n"
    .set msg_len, . - message
