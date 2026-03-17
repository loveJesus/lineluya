// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]
#![feature(custom_test_frameworks)]
#![feature(alloc_error_handler)]
#![allow(dead_code, unused_imports, unused_variables, unused_mut)]
#![allow(unused_parens, unused_braces, unused_unsafe, unused_assignments)]
#![allow(unused_doc_comments, unreachable_code, private_interfaces)]

extern crate alloc;

// ============================================================================
// Console & output — serial, framebuffer, VGA, dmesg, tty, pty
// ============================================================================
#[path = "console_chirho/serial_chirho.rs"]
mod serial_chirho;
#[path = "console_chirho/vga_buffer_chirho.rs"]
mod vga_buffer_chirho;
#[path = "console_chirho/fbconsole_chirho.rs"]
mod fbconsole_chirho;
#[path = "console_chirho/dmesg_chirho.rs"]
mod dmesg_chirho;
#[path = "console_chirho/tty_chirho.rs"]
mod tty_chirho;
#[path = "console_chirho/pty_chirho.rs"]
mod pty_chirho;
#[path = "console_chirho/cmdline_chirho.rs"]
mod cmdline_chirho;

// ============================================================================
// Architecture — GDT, IDT, APIC, ACPI, SMP, syscall entry, context switch
// ============================================================================
#[path = "arch_chirho/gdt_chirho.rs"]
mod gdt_chirho;
#[path = "arch_chirho/interrupts_chirho.rs"]
mod interrupts_chirho;
#[path = "arch_chirho/syscall_entry_chirho.rs"]
mod syscall_entry_chirho;
#[path = "arch_chirho/context_switch_chirho.rs"]
mod context_switch_chirho;
#[path = "arch_chirho/apic_chirho.rs"]
mod apic_chirho;
#[path = "arch_chirho/acpi_chirho.rs"]
mod acpi_chirho;
#[path = "arch_chirho/smp_chirho.rs"]
mod smp_chirho;
#[path = "arch_chirho/hpet_chirho.rs"]
mod hpet_chirho;
#[path = "arch_chirho/msi_chirho.rs"]
mod msi_chirho;
#[path = "arch_chirho/boot_protocol_chirho.rs"]
mod boot_protocol_chirho;
#[path = "arch_chirho/multiboot2_header_chirho.rs"]
mod multiboot2_header_chirho;

// ============================================================================
// Memory management — heap allocator, frame allocator, page tables, uaccess
// ============================================================================
#[path = "mm_chirho/allocator_chirho.rs"]
mod allocator_chirho;
#[allow(dead_code)]
#[path = "mm_chirho/buddy_chirho.rs"]
mod buddy_chirho;
#[path = "mm_chirho/memory_chirho.rs"]
mod memory_chirho;
#[path = "mm_chirho/pagetable_chirho.rs"]
mod pagetable_chirho;
#[path = "mm_chirho/uaccess_chirho.rs"]
mod uaccess_chirho;
#[path = "mm_chirho/mmap_chirho.rs"]
mod mm_chirho;

// ============================================================================
// Filesystem — VFS, tmpfs, ext4, procfs, sysfs, devtmpfs, pipes
// ============================================================================
#[path = "fs_chirho/vfs_chirho.rs"]
mod vfs_chirho;
#[path = "fs_chirho/tmpfs_chirho.rs"]
mod tmpfs_chirho;
#[path = "fs_chirho/devtmpfs_chirho.rs"]
mod devtmpfs_chirho;
#[path = "fs_chirho/procfs_chirho.rs"]
mod procfs_chirho;
#[path = "fs_chirho/sysfs_chirho.rs"]
mod sysfs_chirho;
#[path = "fs_chirho/vfs_ops_chirho.rs"]
mod fs_chirho;
#[path = "fs_chirho/ext4_chirho.rs"]
mod ext4_chirho;
#[path = "fs_chirho/gpt_chirho.rs"]
mod gpt_chirho;
#[path = "fs_chirho/pipe_chirho.rs"]
mod pipe_chirho;
#[path = "fs_chirho/initramfs_chirho.rs"]
mod initramfs_chirho;
#[path = "fs_chirho/overlayfs_chirho.rs"]
#[allow(dead_code)]
mod overlayfs_chirho;

// ============================================================================
// Drivers — VirtIO, PCI, AHCI, NVMe, e1000, USB, block I/O, framebuffer
// ============================================================================
#[path = "drivers_chirho/block_chirho.rs"]
mod block_chirho;
#[path = "drivers_chirho/bio_chirho.rs"]
mod bio_chirho;
#[path = "drivers_chirho/virtio_chirho.rs"]
mod virtio_chirho;
#[path = "drivers_chirho/pci_chirho.rs"]
mod pci_chirho;
#[path = "drivers_chirho/ahci_chirho.rs"]
mod ahci_chirho;
#[path = "drivers_chirho/nvme_chirho.rs"]
mod nvme_chirho;
#[path = "drivers_chirho/e1000_chirho.rs"]
mod e1000_chirho;
#[path = "drivers_chirho/usb_chirho.rs"]
mod usb_chirho;
#[path = "drivers_chirho/fb_device_chirho.rs"]
mod fb_device_chirho;
#[path = "drivers_chirho/evdev_chirho.rs"]
mod evdev_chirho;
#[path = "drivers_chirho/random_chirho.rs"]
mod random_chirho;
#[path = "drivers_chirho/sound_chirho.rs"]
mod sound_chirho;
#[path = "drivers_chirho/loop_device_chirho.rs"]
mod loop_device_chirho;

// ============================================================================
// Networking — TCP/IP stack, sockets, DHCP, DNS
// ============================================================================
#[path = "net_chirho/net_core_chirho.rs"]
mod net_chirho;

// ============================================================================
// Scheduling — task management, scheduler, wait queues, futex
// ============================================================================
#[path = "sched_chirho/task_chirho.rs"]
mod task_chirho;
#[path = "sched_chirho/scheduler_chirho.rs"]
mod scheduler_chirho;
#[path = "sched_chirho/waitqueue_chirho.rs"]
mod waitqueue_chirho;
#[path = "sched_chirho/futex_chirho.rs"]
mod futex_chirho;

// ============================================================================
// Process management — fork, exec, ELF loading, dynamic linking, signals
// ============================================================================
#[path = "process_chirho/process_core_chirho.rs"]
mod process_chirho;
#[path = "process_chirho/exec_chirho.rs"]
mod exec_chirho;
#[path = "process_chirho/elf_chirho.rs"]
mod elf_chirho;
#[path = "process_chirho/dynlink_chirho.rs"]
mod dynlink_chirho;
#[path = "process_chirho/signal_chirho.rs"]
mod signal_chirho;

// ============================================================================
// Syscall dispatch (top-level — routes to all subsystems)
// ============================================================================
mod syscall_chirho;

// ============================================================================
// Advanced subsystem stubs — io_uring, BPF, epoll, cgroups, namespaces, etc.
// ============================================================================
#[path = "subsys_chirho/io_uring_chirho.rs"]
mod io_uring_chirho;
#[path = "subsys_chirho/bpf_chirho.rs"]
mod bpf_chirho;
#[path = "subsys_chirho/vdso_chirho.rs"]
mod vdso_chirho;
#[path = "subsys_chirho/epoll_chirho.rs"]
mod epoll_chirho;
#[path = "subsys_chirho/cgroup_chirho.rs"]
mod cgroup_chirho;
#[path = "subsys_chirho/namespace_chirho.rs"]
mod namespace_chirho;
#[path = "subsys_chirho/seccomp_chirho.rs"]
mod seccomp_chirho;
#[path = "subsys_chirho/capability_chirho.rs"]
mod capability_chirho;
#[path = "subsys_chirho/module_chirho.rs"]
mod module_chirho;
#[path = "subsys_chirho/ko_loader_chirho.rs"]
mod ko_loader_chirho;
#[path = "subsys_chirho/power_chirho.rs"]
mod power_chirho;
#[path = "subsys_chirho/trace_chirho.rs"]
mod trace_chirho;
#[path = "subsys_chirho/eventfd_chirho.rs"]
mod eventfd_chirho;
#[path = "subsys_chirho/inotify_chirho.rs"]
mod inotify_chirho;
#[path = "subsys_chirho/busybox_chirho.rs"]
pub mod busybox_chirho;
#[path = "subsys_chirho/ioctl_chirho.rs"]
pub mod ioctl_chirho;

use bootloader_api::{entry_point, BootInfo, BootloaderConfig};
use bootloader_api::config::Mapping;
use core::panic::PanicInfo;

/// Bootloader configuration: map all physical memory so the kernel can access it.
pub static BOOTLOADER_CONFIG_CHIRHO: BootloaderConfig = {
    let mut config_chirho = BootloaderConfig::new_default();
    config_chirho.mappings.physical_memory = Some(Mapping::Dynamic);
    config_chirho
};

entry_point!(kernel_main_chirho, config = &BOOTLOADER_CONFIG_CHIRHO);

/// Main kernel entry point. Called by the bootloader after setting up long mode,
/// paging, and providing boot information.
fn kernel_main_chirho(boot_info_chirho: &'static mut BootInfo) -> ! {
    serial_println_chirho!("Lineluya kernel booting...");
    serial_println_chirho!("For God so loved the world that he gave his only begotten Son,");
    serial_println_chirho!("that whoever believes in him should not perish but have eternal life.");
    serial_println_chirho!("- John 3:16");
    serial_println_chirho!();

    // Enable SSE/SSE2 — required by musl and all modern x86_64 code.
    // Clear CR0.EM, set CR0.MP, set CR4.OSFXSR + CR4.OSXMMEXCPT.
    unsafe {
        core::arch::asm!(
            "mov rax, cr0",
            "and ax, 0xFFFB", // clear EM (bit 2)
            "or ax, 0x2",     // set MP (bit 1)
            "mov cr0, rax",
            "mov rax, cr4",
            "or ax, 0x600",   // set OSFXSR (bit 9) + OSXMMEXCPT (bit 10)
            "mov cr4, rax",
            out("rax") _,
            options(nomem, nostack)
        );
    }
    serial_println_chirho!("[OK] SSE/SSE2 enabled");

    // Initialize the Global Descriptor Table
    gdt_chirho::init_chirho();
    serial_println_chirho!("[OK] GDT initialized");

    // Initialize the Interrupt Descriptor Table
    interrupts_chirho::init_idt_chirho();
    serial_println_chirho!("[OK] IDT initialized");

    // Initialize the PICs (Programmable Interrupt Controllers)
    interrupts_chirho::init_pics_chirho();
    serial_println_chirho!("[OK] PICs initialized");

    // Initialize memory management
    let physical_memory_offset_chirho = boot_info_chirho
        .physical_memory_offset
        .into_option()
        .expect("Physical memory offset not provided by bootloader");

    let memory_regions_chirho = &boot_info_chirho.memory_regions;

    // Store the physical memory offset globally for the page table module.
    pagetable_chirho::set_phys_mem_offset_chirho(physical_memory_offset_chirho);
    // Save the boot PML4 address for lazy page migration.
    pagetable_chirho::save_boot_pml4_chirho();

    // Initialize the frame allocator with the memory map from the bootloader
    let mut frame_allocator_chirho =
        memory_chirho::BootInfoFrameAllocatorChirho::init_chirho(memory_regions_chirho);
    serial_println_chirho!("[OK] Frame allocator initialized");

    // Initialize the page mapper
    let mut mapper_chirho =
        unsafe { memory_chirho::init_mapper_chirho(physical_memory_offset_chirho) };
    serial_println_chirho!("[OK] Page mapper initialized");

    // Initialize the kernel heap
    allocator_chirho::init_heap_chirho(&mut mapper_chirho, &mut frame_allocator_chirho)
        .expect("Heap initialization failed");
    serial_println_chirho!("[OK] Heap allocator initialized");

    // Initialize the framebuffer console (pixel-based text on UEFI screen)
    if let Some(fb_chirho) = boot_info_chirho.framebuffer.as_mut() {
        let fb_info_chirho = fb_chirho.info();
        let fb_buf_chirho = fb_chirho.buffer_mut();
        let is_bgr_chirho = matches!(fb_info_chirho.pixel_format, bootloader_api::info::PixelFormat::Bgr);
        fbconsole_chirho::FB_CONSOLE_CHIRHO.lock().init_chirho(
            fb_buf_chirho.as_mut_ptr(),
            fb_buf_chirho.len(),
            fb_info_chirho.width,
            fb_info_chirho.height,
            fb_info_chirho.bytes_per_pixel,
            fb_info_chirho.stride,
            is_bgr_chirho,
        );
        fb_println_chirho!("Lineluya kernel booting...");
        fb_println_chirho!("For God so loved the world that he gave his only begotten Son,");
        fb_println_chirho!("that whoever believes in him should not perish but have eternal life.");
        fb_println_chirho!("- John 3:16");
        fb_println_chirho!();
        fb_println_chirho!("[OK] Framebuffer console initialized ({}x{}, {}bpp)",
            fb_info_chirho.width, fb_info_chirho.height, fb_info_chirho.bytes_per_pixel * 8);

        // Configure /dev/fb0 device with actual framebuffer parameters
        // The physical address is derived from the virtual buffer pointer
        // and the physical memory offset.
        let fb_virt_addr_chirho = fb_buf_chirho.as_ptr() as u64;
        let fb_phys_addr_chirho = fb_virt_addr_chirho.wrapping_sub(physical_memory_offset_chirho);
        fb_device_chirho::set_fb_params_chirho(
            fb_phys_addr_chirho,
            fb_info_chirho.width as u32,
            fb_info_chirho.height as u32,
            (fb_info_chirho.stride * fb_info_chirho.bytes_per_pixel) as u32,
            (fb_info_chirho.bytes_per_pixel * 8) as u32,
            is_bgr_chirho,
        );
    }

    // Initialize the mm subsystem with a second mapper and a frame allocator
    // that starts where the boot allocator left off (to avoid double-allocating
    // frames used for the heap and page tables).
    {
        let mm_mapper_chirho =
            unsafe { memory_chirho::init_mapper_chirho(physical_memory_offset_chirho) };
        let mm_frame_alloc_chirho = mm_chirho::GlobalFrameAllocatorChirho::new_chirho(
            frame_allocator_chirho.memory_regions_chirho(),
            frame_allocator_chirho.next_index_chirho(),
        );
        unsafe {
            mm_chirho::init_mm_chirho(mm_mapper_chirho, mm_frame_alloc_chirho);
        }
    }

    // Initialize kernel command line from bootloader (E1-014).
    cmdline_chirho::init_cmdline_from_bootinfo_chirho();

    // Initialize dmesg ring buffer (E1-015).
    dmesg_chirho::init_dmesg_chirho();

    // Phase 2: Initialize process management
    task_chirho::init_tasking_chirho();
    fb_println_chirho!("[OK] Task system initialized");

    scheduler_chirho::init_scheduler_chirho();
    // Set PID 0 as the scheduler's current task so it participates
    // in scheduling (gets pushed to run queue when yielding).
    scheduler_chirho::set_current_pid_chirho(0);
    fb_println_chirho!("[OK] Scheduler initialized");

    unsafe { syscall_chirho::init_syscalls_chirho() };
    fb_println_chirho!("[OK] Syscall interface initialized");

    unsafe { syscall_entry_chirho::init_syscall_entry_chirho() };
    // Sync PID 0's kernel_stack with the boot syscall stack so the
    // scheduler sets the correct KERNEL_STACK_TOP when switching back.
    unsafe {
        let boot_kstack_chirho = syscall_entry_chirho::KERNEL_STACK_TOP_CHIRHO;
        if let Some(task_arc_chirho) = task_chirho::current_task_chirho() {
            task_arc_chirho.lock().kernel_stack_chirho = boot_kstack_chirho;
        }
    }
    fb_println_chirho!("[OK] Syscall entry trampoline initialized");

    fs_chirho::init_fs_chirho();
    fb_println_chirho!("[OK] Filesystem layer initialized");

    // Initialize PTY subsystem (needed for SSH, screen, tmux, xterm)
    pty_chirho::init_pty_chirho();
    fb_println_chirho!("[OK] PTY subsystem initialized");

    // Initialize kernel symbol table for .ko module loading
    ko_loader_chirho::init_kernel_symbols_chirho();

    // Phase A4: VirtIO device discovery — scan PCI bus, probe any VirtIO-blk,
    // and attempt to read sector 0 as a smoke test (P2-001 / P2-002).
    virtio_chirho::init_virtio_chirho();
    fb_println_chirho!("[OK] VirtIO subsystem initialized");

    // Phase A2: Sound card PCI detection (A2-SOUND-001).
    sound_chirho::detect_sound_cards_chirho();
    fb_println_chirho!("[OK] Sound subsystem initialized");

    net_chirho::init_networking_chirho();
    fb_println_chirho!("[OK] Networking initialized");

    x86_64::instructions::interrupts::enable();
    fb_println_chirho!("[OK] Interrupts enabled");

    fb_println_chirho!();
    fb_println_chirho!("=== Lineluya Kernel v2.0.0 ===");
    fb_println_chirho!("Linux-compatible kernel written in Rust");
    fb_println_chirho!("All subsystems initialized.");
    fb_println_chirho!();

    // Test heap allocation
    {
        use alloc::boxed::Box;
        use alloc::vec;
        use alloc::string::String;

        let heap_value_chirho = Box::new(42);
        serial_println_chirho!("[TEST] Heap allocation: Box<i32> = {}", heap_value_chirho);

        let vec_chirho = vec![1, 2, 3, 4, 5];
        serial_println_chirho!("[TEST] Vec allocation: {:?}", vec_chirho);

        let string_chirho = String::from("Hallelujah! Lineluya kernel is alive!");
        serial_println_chirho!("[TEST] String allocation: {}", string_chirho);
    }

    // Test syscall dispatch (kernel-mode test)
    {
        let mut test_frame_chirho = syscall_chirho::SyscallFrameChirho::zeroed_chirho();
        // Test sys_uname (syscall 63)
        test_frame_chirho.rax_chirho = 63;
        let result_chirho = syscall_chirho::syscall_dispatch_chirho(&mut test_frame_chirho);
        serial_println_chirho!("[TEST] sys_uname dispatch returned: {}", result_chirho);

        // Test sys_getpid (syscall 39)
        test_frame_chirho.rax_chirho = 39;
        let result_chirho = syscall_chirho::syscall_dispatch_chirho(&mut test_frame_chirho);
        serial_println_chirho!("[TEST] sys_getpid returned: {}", result_chirho);
    }

    // Create /etc/profile on tmpfs to auto-start dropbear SSH server.
    // Uses the tmpfs write API directly since ext4 disk may not have this file.
    {
        let profile_content_chirho = "# Auto-start dropbear SSH on port 2222\ndropbear -p 2222 -B -R 2>/dev/null &\n";
        tmpfs_chirho::write_tmpfs_file_chirho(
            "/etc/profile",
            profile_content_chirho.as_bytes(),
        );
        serial_println_chirho!("[INIT] Created /etc/profile (dropbear auto-start)");
    }

    // Load and execute the hello world binary
    serial_println_chirho!();
    serial_println_chirho!("Loading hello world ELF into userspace...");
    exec_chirho::exec_init_chirho();
    // If we return here, userspace exited or failed to load
    serial_println_chirho!("Returned from exec_init_chirho (userspace exited or load failed).");

    serial_println_chirho!();
    serial_println_chirho!("Entering idle loop. Press keys to test keyboard interrupt.");

    // Enter the idle loop - halt until next interrupt
    hlt_loop_chirho();
}

/// Halt the CPU until the next interrupt, in a loop.
/// This is more power-efficient than a busy spin loop.
pub fn hlt_loop_chirho() -> ! {
    loop {
        x86_64::instructions::interrupts::enable_and_hlt();
    }
}

/// Panic handler - prints the panic info to serial and halts.
#[panic_handler]
fn panic_handler_chirho(info_chirho: &PanicInfo) -> ! {
    serial_println_chirho!("!!! KERNEL PANIC !!!");
    serial_println_chirho!("{}", info_chirho);
    loop {
        x86_64::instructions::hlt();
    }
}
