// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! ELF loader and userspace execution module for the Lineluya kernel.
//!
//! This module ties together the ELF parser, memory manager, GDT selectors,
//! and IRETQ mechanism to load an embedded hello-world ELF binary into
//! user-space memory and jump to ring 3 for the first time.
//!
//! The flow is:
//! 1. Parse the embedded ELF binary via [`crate::elf_chirho::parse_elf_chirho`].
//! 2. For each `PT_LOAD` segment, allocate pages with user-accessible flags
//!    and copy the segment data (zeroing BSS).
//! 3. Allocate a user stack and build the initial stack layout (argc, argv,
//!    envp, auxiliary vector) that a Linux program expects.
//! 4. Use `iretq` to transition the CPU from ring 0 to ring 3 at the ELF
//!    entry point.
//!
//! When the userspace program issues a `syscall` (e.g. `sys_write`), the
//! kernel's SYSCALL trampoline handles it and the serial output should show
//! the hello-world message.

extern crate alloc;

use core::arch::asm;

use crate::elf_chirho::{
    self, ElfSegmentChirho, AT_ENTRY_CHIRHO, AT_GID_CHIRHO,
    AT_NULL_CHIRHO, AT_PAGESZ_CHIRHO, AT_PHDR_CHIRHO, AT_PHENT_CHIRHO, AT_PHNUM_CHIRHO,
    AT_RANDOM_CHIRHO, AT_UID_CHIRHO, PF_W_CHIRHO, PF_X_CHIRHO,
};
use crate::gdt_chirho::{USER_CS_CHIRHO, USER_DS_CHIRHO};
use crate::mm_chirho::{
    self, MmChirho, PROT_EXEC_CHIRHO, PROT_READ_CHIRHO, PROT_WRITE_CHIRHO,
    MAP_ANONYMOUS_CHIRHO, MAP_FIXED_CHIRHO, MAP_PRIVATE_CHIRHO,
};
use crate::serial_println_chirho;

// ============================================================================
// Constants
// ============================================================================

/// Page size (4 KiB).
const PAGE_SIZE_CHIRHO: u64 = 4096;

/// User stack top address. We place the stack just below the canonical user
/// space ceiling, at a well-known address matching typical Linux layouts.
/// The stack grows downward from this address.
const USER_STACK_TOP_CHIRHO: u64 = 0x7FFF_FFFF_F000;

/// User stack size (8 MiB).
const USER_STACK_SIZE_CHIRHO: u64 = 8 * 1024 * 1024;

/// The embedded hello-world ELF binary, compiled for x86_64-unknown-none.
/// `include_bytes!` embeds the file contents directly into the kernel image
/// at compile time.
pub static HELLO_ELF_CHIRHO: &[u8] = include_bytes!(
    "../../userspace-chirho/hello-chirho/target/x86_64-unknown-none/release/hello-chirho"
);

/// BusyBox static x86_64 binary — 1.1MB, 40+ commands including ash shell.
/// DISABLED: Embedding 1.1MB makes the kernel 8.3MB which the BIOS bootloader
/// can't handle. BusyBox should be loaded from initramfs instead.
pub static BUSYBOX_ELF_CHIRHO: &[u8] = b"";

// ============================================================================
// Error type
// ============================================================================

/// Errors that can occur during ELF loading and exec.
#[derive(Debug)]
pub enum ExecErrorChirho {
    /// ELF parsing/validation failed.
    ElfParseChirho(&'static str),
    /// Memory mapping failed.
    MmapFailedChirho(i64),
    /// No PT_LOAD segments found.
    NoSegmentsChirho,
}

// ============================================================================
// LoadedElfChirho — result of loading an ELF into memory
// ============================================================================

/// Information about a successfully loaded ELF binary.
pub struct LoadedElfChirho {
    /// Virtual address of the program entry point.
    pub entry_point_chirho: u64,
    /// Virtual address of the program header table in memory (for AT_PHDR).
    pub phdr_addr_chirho: u64,
    /// Number of program header entries (for AT_PHNUM).
    pub phdr_num_chirho: u16,
    /// Size of a single program header entry (for AT_PHENT).
    pub phdr_size_chirho: u16,
    /// Address just past the end of the last loaded segment (initial brk).
    pub brk_addr_chirho: u64,
}

// ============================================================================
// Step 1: Load ELF segments into memory
// ============================================================================

/// Parse an ELF binary and map its PT_LOAD segments into user-accessible
/// memory.
///
/// For each loadable segment:
/// - Allocates page-aligned anonymous memory with `MAP_FIXED`.
/// - Copies the initialised file data into the mapped region.
/// - Zeros the BSS portion (memsz - filesz).
///
/// Returns a [`LoadedElfChirho`] describing the loaded binary's entry point,
/// program header location, and initial break address.
pub fn load_elf_into_memory_chirho(
    elf_data_chirho: &[u8],
) -> Result<LoadedElfChirho, ExecErrorChirho> {
    // Parse the ELF header and program headers.
    let elf_info_chirho = elf_chirho::parse_elf_chirho(elf_data_chirho)
        .map_err(|_err_chirho| ExecErrorChirho::ElfParseChirho("ELF parse failed"))?;

    if elf_info_chirho.segments_chirho.is_empty() {
        return Err(ExecErrorChirho::NoSegmentsChirho);
    }

    serial_println_chirho!(
        "[EXEC] ELF parsed: entry={:#x}, {} PT_LOAD segments",
        elf_info_chirho.entry_point_chirho,
        elf_info_chirho.segments_chirho.len()
    );

    // Ensure the mm subsystem is initialised.
    let mm_lock_chirho = mm_chirho::get_or_init_mm_chirho();

    let mut brk_addr_chirho: u64 = 0;

    // Map each PT_LOAD segment.
    for seg_chirho in &elf_info_chirho.segments_chirho {
        load_segment_chirho(elf_data_chirho, seg_chirho, mm_lock_chirho)?;

        // Track the highest address for brk.
        let seg_end_chirho = seg_chirho.vaddr_chirho + seg_chirho.memsz_chirho;
        if seg_end_chirho > brk_addr_chirho {
            brk_addr_chirho = seg_end_chirho;
        }
    }

    // Page-align brk upward.
    brk_addr_chirho = align_up_chirho(brk_addr_chirho, PAGE_SIZE_CHIRHO);

    serial_println_chirho!(
        "[EXEC] All segments loaded. brk={:#x}",
        brk_addr_chirho
    );

    Ok(LoadedElfChirho {
        entry_point_chirho: elf_info_chirho.entry_point_chirho,
        phdr_addr_chirho: elf_info_chirho.phdr_addr_chirho,
        phdr_num_chirho: elf_info_chirho.phdr_num_chirho,
        phdr_size_chirho: elf_info_chirho.phdr_size_chirho,
        brk_addr_chirho,
    })
}

/// Map a single PT_LOAD segment into user memory.
fn load_segment_chirho(
    elf_data_chirho: &[u8],
    seg_chirho: &ElfSegmentChirho,
    mm_lock_chirho: &spin::Mutex<Option<MmChirho>>,
) -> Result<(), ExecErrorChirho> {
    // Page-align the segment start downward and end upward.
    let page_start_chirho = align_down_chirho(seg_chirho.vaddr_chirho, PAGE_SIZE_CHIRHO);
    let page_end_chirho =
        align_up_chirho(seg_chirho.vaddr_chirho + seg_chirho.memsz_chirho, PAGE_SIZE_CHIRHO);
    let map_len_chirho = page_end_chirho - page_start_chirho;

    // Convert ELF segment flags to Linux PROT_* flags.
    let prot_chirho = elf_flags_to_prot_chirho(seg_chirho.flags_chirho);

    serial_println_chirho!(
        "[EXEC]   Mapping segment: vaddr={:#x}, memsz={:#x}, filesz={:#x}, prot={:#x}, pages={:#x}..{:#x}",
        seg_chirho.vaddr_chirho,
        seg_chirho.memsz_chirho,
        seg_chirho.filesz_chirho,
        prot_chirho,
        page_start_chirho,
        page_end_chirho
    );

    // Allocate the pages. We use MAP_FIXED to place them at the exact vaddr
    // the ELF specifies. We always map writable initially so we can copy data
    // in, then mprotect to the correct permissions afterward.
    let alloc_prot_chirho = PROT_READ_CHIRHO | PROT_WRITE_CHIRHO | PROT_EXEC_CHIRHO;
    {
        let mut mm_chirho = mm_lock_chirho.lock();
        let mm_ref_chirho = mm_chirho.as_mut().expect("MM not initialised");

        mm_ref_chirho
            .mmap_chirho(
                page_start_chirho,
                map_len_chirho,
                alloc_prot_chirho,
                MAP_ANONYMOUS_CHIRHO | MAP_PRIVATE_CHIRHO | MAP_FIXED_CHIRHO,
                -1,
                0,
            )
            .map_err(|err_chirho| ExecErrorChirho::MmapFailedChirho(err_chirho))?;
    }

    // Copy the initialised data from the ELF file into the mapped region.
    if seg_chirho.filesz_chirho > 0 {
        if let Some(data_chirho) = elf_chirho::segment_data_chirho(elf_data_chirho, seg_chirho) {
            let dest_ptr_chirho = seg_chirho.vaddr_chirho as *mut u8;
            // SAFETY: We just mapped these pages as writable. The source data
            // is a valid slice from the ELF image.
            unsafe {
                core::ptr::copy_nonoverlapping(
                    data_chirho.as_ptr(),
                    dest_ptr_chirho,
                    data_chirho.len(),
                );
            }
        }
    }

    // Zero the BSS portion (memsz > filesz).
    let bss_start_chirho = seg_chirho.vaddr_chirho + seg_chirho.filesz_chirho;
    let bss_len_chirho = seg_chirho.memsz_chirho - seg_chirho.filesz_chirho;
    if bss_len_chirho > 0 {
        // SAFETY: The region [bss_start, bss_start + bss_len) was just mapped
        // and is within the segment's memsz.
        unsafe {
            core::ptr::write_bytes(bss_start_chirho as *mut u8, 0, bss_len_chirho as usize);
        }
    }

    // If the final permissions differ from what we mapped with, apply mprotect.
    // For simplicity in this first implementation, we leave the pages RWX since
    // the kernel shares page tables. A real implementation would mprotect here.

    Ok(())
}

// ============================================================================
// Step 2: Set up the user stack
// ============================================================================

/// Allocate a user stack and build the initial stack layout that a Linux
/// program expects: argc, argv pointers, NULL, envp NULL, auxiliary vector.
///
/// Returns the final RSP value (16-byte aligned, pointing to argc on the
/// stack).
pub fn setup_user_stack_chirho(
    loaded_chirho: &LoadedElfChirho,
) -> u64 {
    let mm_lock_chirho = mm_chirho::get_or_init_mm_chirho();

    // The stack occupies [STACK_TOP - STACK_SIZE, STACK_TOP).
    let stack_bottom_chirho = USER_STACK_TOP_CHIRHO - USER_STACK_SIZE_CHIRHO;

    serial_println_chirho!(
        "[EXEC] Allocating user stack: {:#x}..{:#x} ({} MiB)",
        stack_bottom_chirho,
        USER_STACK_TOP_CHIRHO,
        USER_STACK_SIZE_CHIRHO / (1024 * 1024)
    );

    // Map the stack pages.
    {
        let mut mm_chirho = mm_lock_chirho.lock();
        let mm_ref_chirho = mm_chirho.as_mut().expect("MM not initialised for stack");
        mm_ref_chirho
            .mmap_chirho(
                stack_bottom_chirho,
                USER_STACK_SIZE_CHIRHO,
                PROT_READ_CHIRHO | PROT_WRITE_CHIRHO,
                MAP_ANONYMOUS_CHIRHO | MAP_PRIVATE_CHIRHO | MAP_FIXED_CHIRHO,
                -1,
                0,
            )
            .expect("Failed to map user stack");
    }

    // Build the initial stack contents. The stack grows downward, so we start
    // at the top and work our way down.
    //
    // Layout (from high to low address):
    //   - Program name string: "hello-chirho\0"
    //   - 16 random bytes (for AT_RANDOM)
    //   - Padding for alignment
    //   - Auxiliary vector entries (pairs of u64)
    //   - NULL (envp terminator)
    //   - NULL (argv terminator)
    //   - argv[0] pointer -> program name string
    //   - argc = 1
    //
    // RSP points to argc at the bottom.

    let mut sp_chirho = USER_STACK_TOP_CHIRHO;

    // -- Write the program name string at the top of the stack --
    let prog_name_chirho = b"hello-chirho\0";
    sp_chirho -= prog_name_chirho.len() as u64;
    let prog_name_addr_chirho = sp_chirho;
    // SAFETY: We just mapped this region.
    unsafe {
        core::ptr::copy_nonoverlapping(
            prog_name_chirho.as_ptr(),
            sp_chirho as *mut u8,
            prog_name_chirho.len(),
        );
    }

    // -- Write 16 "random" bytes for AT_RANDOM --
    sp_chirho -= 16;
    let random_addr_chirho = sp_chirho;
    // Use a simple deterministic pattern as a placeholder for random bytes.
    let random_bytes_chirho: [u8; 16] = [
        0x4A, 0x6F, 0x68, 0x6E, // "John"
        0x33, 0x3A, 0x31, 0x36, // "3:16"
        0xDE, 0xAD, 0xBE, 0xEF,
        0xCA, 0xFE, 0xBA, 0xBE,
    ];
    unsafe {
        core::ptr::copy_nonoverlapping(
            random_bytes_chirho.as_ptr(),
            sp_chirho as *mut u8,
            16,
        );
    }

    // -- Align SP to 8 bytes for the u64 entries that follow --
    sp_chirho = sp_chirho & !7;

    // -- Build the auxiliary vector (pushed in reverse, so AT_NULL goes first) --
    // We push pairs of (type, value) as u64 values.
    // The auxv is read from low to high, so the first pair pushed (at lowest
    // address) will be read first.

    // Helper: push a u64 onto the stack.
    let push_u64_chirho = |sp_ref_chirho: &mut u64, val_chirho: u64| {
        *sp_ref_chirho -= 8;
        unsafe {
            core::ptr::write(*sp_ref_chirho as *mut u64, val_chirho);
        }
    };

    // We need to build auxv from the end (AT_NULL) so that when we push in
    // reverse the final layout in memory reads correctly. Actually, the auxv
    // goes in ascending address order, so we place it as a block. Let's
    // compute all the entries first, then write them.

    // Auxiliary vector entries (type, value):
    let auxv_entries_chirho: [(u64, u64); 9] = [
        (AT_PAGESZ_CHIRHO, PAGE_SIZE_CHIRHO),
        (AT_ENTRY_CHIRHO, loaded_chirho.entry_point_chirho),
        (AT_PHDR_CHIRHO, loaded_chirho.phdr_addr_chirho),
        (AT_PHNUM_CHIRHO, loaded_chirho.phdr_num_chirho as u64),
        (AT_PHENT_CHIRHO, loaded_chirho.phdr_size_chirho as u64),
        (AT_UID_CHIRHO, 0),
        (AT_GID_CHIRHO, 0),
        (AT_RANDOM_CHIRHO, random_addr_chirho),
        (AT_NULL_CHIRHO, 0),
    ];

    // Total size of auxv on stack: 9 entries * 2 u64s * 8 bytes = 144 bytes.
    // Plus: envp NULL (8), argv NULL (8), argv[0] (8), argc (8) = 32 bytes.
    // Total below current sp: 144 + 32 = 176 bytes.
    // We need the final sp (pointing to argc) to be 16-byte aligned.

    // Calculate total stack frame size.
    let auxv_size_chirho = auxv_entries_chirho.len() * 2 * 8; // 144
    let frame_size_chirho = 8 + 8 + 8 + 8 + auxv_size_chirho as u64; // argc + argv[0] + argv_null + envp_null + auxv
    // 8 + 8 + 8 + 8 + 144 = 176 bytes

    // Align sp so that (sp - frame_size) is 16-byte aligned.
    // frame_size = 176 = 11 * 16, already 16-byte aligned.
    // But to be safe:
    let target_sp_chirho = (sp_chirho - frame_size_chirho) & !0xF;
    sp_chirho = target_sp_chirho + frame_size_chirho;

    // Now push everything in reverse order (high to low address).
    // Auxv goes first (highest address within the frame), then envp NULL,
    // then argv NULL, then argv[0], then argc.

    // Push auxv entries in reverse order (AT_NULL at highest address).
    for idx_chirho in (0..auxv_entries_chirho.len()).rev() {
        let (type_chirho, val_chirho) = auxv_entries_chirho[idx_chirho];
        push_u64_chirho(&mut sp_chirho, val_chirho);
        push_u64_chirho(&mut sp_chirho, type_chirho);
    }

    // Push envp NULL terminator.
    push_u64_chirho(&mut sp_chirho, 0);

    // Push argv NULL terminator.
    push_u64_chirho(&mut sp_chirho, 0);

    // Push argv[0] = pointer to program name.
    push_u64_chirho(&mut sp_chirho, prog_name_addr_chirho);

    // Push argc = 1.
    push_u64_chirho(&mut sp_chirho, 1);

    serial_println_chirho!(
        "[EXEC] User stack set up. RSP={:#x} (16-byte aligned: {})",
        sp_chirho,
        sp_chirho % 16 == 0
    );

    // Verify alignment.
    debug_assert_eq!(sp_chirho % 16, 0, "User RSP must be 16-byte aligned");

    sp_chirho
}

// ============================================================================
// Step 2b: Set up the user stack with argv/envp (for execve)
// ============================================================================

/// Allocate a user stack and build the initial stack layout with the provided
/// argv and envp arrays, matching the standard Linux process stack layout.
///
/// Layout (high to low):
///   - Environment strings
///   - Argument strings
///   - Program name string (for AT_EXECFN)
///   - 16 random bytes (for AT_RANDOM)
///   - Padding for alignment
///   - Auxiliary vector entries
///   - NULL (envp terminator)
///   - envp[n-1] ... envp[0] pointers
///   - NULL (argv terminator)
///   - argv[argc-1] ... argv[0] pointers
///   - argc
///
/// Returns the final RSP value (16-byte aligned, pointing to argc).
pub fn setup_user_stack_with_args_chirho(
    loaded_chirho: &LoadedElfChirho,
    argv_chirho: &[alloc::string::String],
    envp_chirho: &[alloc::string::String],
) -> u64 {
    let mm_lock_chirho = mm_chirho::get_or_init_mm_chirho();

    let stack_bottom_chirho = USER_STACK_TOP_CHIRHO - USER_STACK_SIZE_CHIRHO;

    serial_println_chirho!(
        "[EXEC] Allocating user stack (execve): {:#x}..{:#x} ({} MiB)",
        stack_bottom_chirho,
        USER_STACK_TOP_CHIRHO,
        USER_STACK_SIZE_CHIRHO / (1024 * 1024)
    );

    // Map the stack pages.
    {
        let mut mm_guard_chirho = mm_lock_chirho.lock();
        let mm_ref_chirho = mm_guard_chirho.as_mut().expect("MM not initialised for stack");
        mm_ref_chirho
            .mmap_chirho(
                stack_bottom_chirho,
                USER_STACK_SIZE_CHIRHO,
                PROT_READ_CHIRHO | PROT_WRITE_CHIRHO,
                MAP_ANONYMOUS_CHIRHO | MAP_PRIVATE_CHIRHO | MAP_FIXED_CHIRHO,
                -1,
                0,
            )
            .expect("Failed to map user stack");
    }

    let mut sp_chirho = USER_STACK_TOP_CHIRHO;

    // Helper: push bytes onto the stack, return the address of the written data.
    let push_bytes_chirho = |sp_ref_chirho: &mut u64, data_chirho: &[u8]| -> u64 {
        *sp_ref_chirho -= data_chirho.len() as u64;
        let addr_chirho = *sp_ref_chirho;
        unsafe {
            core::ptr::copy_nonoverlapping(
                data_chirho.as_ptr(),
                addr_chirho as *mut u8,
                data_chirho.len(),
            );
        }
        addr_chirho
    };

    // -- Write environment strings onto the stack (high addresses) --
    let mut envp_addrs_chirho: alloc::vec::Vec<u64> = alloc::vec::Vec::new();
    for env_str_chirho in envp_chirho.iter().rev() {
        let mut bytes_chirho = env_str_chirho.as_bytes().to_vec();
        bytes_chirho.push(0); // NUL terminator
        let addr_chirho = push_bytes_chirho(&mut sp_chirho, &bytes_chirho);
        envp_addrs_chirho.push(addr_chirho);
    }
    envp_addrs_chirho.reverse();

    // -- Write argument strings onto the stack --
    let mut argv_addrs_chirho: alloc::vec::Vec<u64> = alloc::vec::Vec::new();
    for arg_str_chirho in argv_chirho.iter().rev() {
        let mut bytes_chirho = arg_str_chirho.as_bytes().to_vec();
        bytes_chirho.push(0); // NUL terminator
        let addr_chirho = push_bytes_chirho(&mut sp_chirho, &bytes_chirho);
        argv_addrs_chirho.push(addr_chirho);
    }
    argv_addrs_chirho.reverse();

    // -- Write 16 "random" bytes for AT_RANDOM --
    sp_chirho -= 16;
    let random_addr_chirho = sp_chirho;
    let random_bytes_chirho: [u8; 16] = [
        0x4A, 0x6F, 0x68, 0x6E, // "John"
        0x33, 0x3A, 0x31, 0x36, // "3:16"
        0xDE, 0xAD, 0xBE, 0xEF,
        0xCA, 0xFE, 0xBA, 0xBE,
    ];
    unsafe {
        core::ptr::copy_nonoverlapping(
            random_bytes_chirho.as_ptr(),
            sp_chirho as *mut u8,
            16,
        );
    }

    // -- Align SP to 8 bytes --
    sp_chirho = sp_chirho & !7;

    // Helper: push a u64 onto the stack.
    let push_u64_chirho = |sp_ref_chirho: &mut u64, val_chirho: u64| {
        *sp_ref_chirho -= 8;
        unsafe {
            core::ptr::write(*sp_ref_chirho as *mut u64, val_chirho);
        }
    };

    // -- Auxiliary vector entries --
    let auxv_entries_chirho: [(u64, u64); 9] = [
        (AT_PAGESZ_CHIRHO, PAGE_SIZE_CHIRHO),
        (AT_ENTRY_CHIRHO, loaded_chirho.entry_point_chirho),
        (AT_PHDR_CHIRHO, loaded_chirho.phdr_addr_chirho),
        (AT_PHNUM_CHIRHO, loaded_chirho.phdr_num_chirho as u64),
        (AT_PHENT_CHIRHO, loaded_chirho.phdr_size_chirho as u64),
        (AT_UID_CHIRHO, 0),
        (AT_GID_CHIRHO, 0),
        (AT_RANDOM_CHIRHO, random_addr_chirho),
        (AT_NULL_CHIRHO, 0),
    ];

    // Calculate total frame size for alignment.
    let argc_chirho = argv_chirho.len() as u64;
    let auxv_size_chirho = auxv_entries_chirho.len() * 2 * 8;
    // argc + argv pointers + argv NULL + envp pointers + envp NULL + auxv
    let frame_size_chirho = 8
        + (argv_chirho.len() * 8) as u64
        + 8
        + (envp_chirho.len() * 8) as u64
        + 8
        + auxv_size_chirho as u64;

    // Align sp so that (sp - frame_size) is 16-byte aligned.
    let target_sp_chirho = (sp_chirho - frame_size_chirho) & !0xF;
    sp_chirho = target_sp_chirho + frame_size_chirho;

    // Push auxv entries in reverse order.
    for idx_chirho in (0..auxv_entries_chirho.len()).rev() {
        let (type_chirho, val_chirho) = auxv_entries_chirho[idx_chirho];
        push_u64_chirho(&mut sp_chirho, val_chirho);
        push_u64_chirho(&mut sp_chirho, type_chirho);
    }

    // Push envp NULL terminator.
    push_u64_chirho(&mut sp_chirho, 0);

    // Push envp pointers in reverse order.
    for idx_chirho in (0..envp_addrs_chirho.len()).rev() {
        push_u64_chirho(&mut sp_chirho, envp_addrs_chirho[idx_chirho]);
    }

    // Push argv NULL terminator.
    push_u64_chirho(&mut sp_chirho, 0);

    // Push argv pointers in reverse order.
    for idx_chirho in (0..argv_addrs_chirho.len()).rev() {
        push_u64_chirho(&mut sp_chirho, argv_addrs_chirho[idx_chirho]);
    }

    // Push argc.
    push_u64_chirho(&mut sp_chirho, argc_chirho);

    serial_println_chirho!(
        "[EXEC] User stack set up (execve). RSP={:#x}, argc={}, envc={} (16-byte aligned: {})",
        sp_chirho,
        argc_chirho,
        envp_chirho.len(),
        sp_chirho % 16 == 0
    );

    debug_assert_eq!(sp_chirho % 16, 0, "User RSP must be 16-byte aligned");

    sp_chirho
}

// ============================================================================
// Step 3: Jump to userspace via IRETQ
// ============================================================================

/// Transition the CPU from ring 0 (kernel mode) to ring 3 (user mode) by
/// building an IRETQ frame on the kernel stack and executing `iretq`.
///
/// The IRETQ frame layout (from RSP upward, i.e. the CPU pops in this order):
///   - RIP   (user entry point)
///   - CS    (user code segment, USER_CS_CHIRHO = 0x2B)
///   - RFLAGS (with IF set to enable interrupts in userspace)
///   - RSP   (user stack pointer)
///   - SS    (user stack segment, USER_DS_CHIRHO = 0x23)
///
/// # Safety
///
/// This function never returns. It directly manipulates CPU registers and
/// stack state to perform a privilege-level transition.
///
/// # Diverging
///
/// Marked as `-> !` because the function never returns; execution continues
/// at the user-space entry point.
pub fn jump_to_userspace_chirho(entry_point_chirho: u64, user_rsp_chirho: u64) -> ! {
    serial_println_chirho!(
        "[EXEC] Jumping to userspace: entry={:#x}, rsp={:#x}",
        entry_point_chirho,
        user_rsp_chirho
    );
    serial_println_chirho!(
        "[EXEC] CS={:#x}, SS={:#x}",
        USER_CS_CHIRHO,
        USER_DS_CHIRHO
    );

    // RFLAGS with IF (Interrupt Flag, bit 9) set so the timer interrupt
    // can preempt userspace and syscalls can return correctly.
    let rflags_chirho: u64 = 0x200; // IF only

    // SAFETY: This inline assembly builds an IRETQ frame and transitions
    // to user mode. The user-space pages must be mapped with USER_ACCESSIBLE
    // flags, and the GDT must have valid user CS/SS descriptors.
    //
    // IMPORTANT: We push the IRETQ frame BEFORE zeroing registers, because
    // the compiler assigns `in(reg)` operands to GPRs that would be
    // destroyed by the zeroing sequence.
    unsafe {
        asm!(
            // Push the IRETQ frame FIRST (before zeroing registers).
            // The CPU pops: RIP, CS, RFLAGS, RSP, SS (bottom to top).
            // We push in reverse: SS, RSP, RFLAGS, CS, RIP.
            "push {ss}",       // SS = USER_DS_CHIRHO (0x23)
            "push {rsp_user}", // RSP = user stack pointer
            "push {rflags}",   // RFLAGS (with IF set)
            "push {cs}",       // CS = USER_CS_CHIRHO (0x2B)
            "push {rip}",      // RIP = entry point

            // NOW zero all general-purpose registers to prevent leaking
            // kernel data to userspace. The IRETQ frame is safely on the
            // stack and won't be affected.
            "xor rax, rax",
            "xor rbx, rbx",
            "xor rcx, rcx",
            "xor rdx, rdx",
            "xor rdi, rdi",
            "xor rsi, rsi",
            "xor rbp, rbp",
            "xor r8, r8",
            "xor r9, r9",
            "xor r10, r10",
            "xor r11, r11",
            "xor r12, r12",
            "xor r13, r13",
            "xor r14, r14",
            "xor r15, r15",

            // Transition to ring 3.
            "iretq",

            ss = in(reg) USER_DS_CHIRHO as u64,
            rsp_user = in(reg) user_rsp_chirho,
            rflags = in(reg) rflags_chirho,
            cs = in(reg) USER_CS_CHIRHO as u64,
            rip = in(reg) entry_point_chirho,
            options(noreturn),
        );
    }
}

// ============================================================================
// Step 4: exec_init_chirho — top-level entry point
// ============================================================================

/// Load the embedded hello-world ELF binary and execute it in userspace.
///
/// This is the main entry point called from `kernel_main_chirho` to bring
/// up the first userspace process. It:
/// 1. Parses the embedded ELF binary.
/// 2. Maps all PT_LOAD segments into user-accessible memory.
/// 3. Sets up the user stack with argc/argv/auxv.
/// 4. Transitions to ring 3 via IRETQ.
///
/// This function never returns — execution continues in userspace, and
/// the kernel regains control only through syscalls or interrupts.
pub fn exec_init_chirho() {
    // Try BusyBox first, fall back to hello-chirho
    let (elf_name_chirho, elf_data_chirho) = if BUSYBOX_ELF_CHIRHO.len() > 100 {
        ("busybox (ash shell)", BUSYBOX_ELF_CHIRHO)
    } else {
        ("hello-chirho", HELLO_ELF_CHIRHO)
    };

    serial_println_chirho!("[EXEC] === Loading {} ELF binary ===", elf_name_chirho);
    serial_println_chirho!(
        "[EXEC] Embedded ELF size: {} bytes",
        elf_data_chirho.len()
    );

    // Step 1: Parse and load the ELF into memory.
    let loaded_chirho = match load_elf_into_memory_chirho(elf_data_chirho) {
        Ok(info_chirho) => info_chirho,
        Err(err_chirho) => {
            serial_println_chirho!("[EXEC] FATAL: Failed to load ELF: {:?}", err_chirho);
            return;
        }
    };

    serial_println_chirho!(
        "[EXEC] ELF loaded: entry={:#x}, phdr={:#x}, brk={:#x}",
        loaded_chirho.entry_point_chirho,
        loaded_chirho.phdr_addr_chirho,
        loaded_chirho.brk_addr_chirho
    );

    // Set the initial program break from the ELF's highest loaded segment.
    crate::syscall_chirho::set_brk_chirho(loaded_chirho.brk_addr_chirho);

    // Step 2: Set up the user stack with proper argv.
    // BusyBox uses argv[0] to determine which applet to run.
    // Pass "sh" so it launches the ash shell.
    let argv_chirho = if elf_data_chirho.len() > 100_000 {
        // BusyBox — launch as shell
        alloc::vec![
            alloc::string::String::from("/bin/sh"),
        ]
    } else {
        alloc::vec![
            alloc::string::String::from("hello-chirho"),
        ]
    };
    let envp_chirho = alloc::vec![
        alloc::string::String::from("HOME=/root"),
        alloc::string::String::from("PATH=/bin:/sbin"),
        alloc::string::String::from("TERM=linux"),
        alloc::string::String::from("PS1=lineluya# "),
    ];
    let user_rsp_chirho = setup_user_stack_with_args_chirho(
        &loaded_chirho,
        &argv_chirho,
        &envp_chirho,
    );

    serial_println_chirho!(
        "[EXEC] Ready to enter userspace. entry={:#x}, rsp={:#x}",
        loaded_chirho.entry_point_chirho,
        user_rsp_chirho
    );

    // Step 3: Jump to userspace. This never returns.
    jump_to_userspace_chirho(loaded_chirho.entry_point_chirho, user_rsp_chirho);
}

// ============================================================================
// Internal helpers
// ============================================================================

/// Align `val_chirho` upward to the next multiple of `align_chirho`.
/// `align_chirho` must be a power of two.
const fn align_up_chirho(val_chirho: u64, align_chirho: u64) -> u64 {
    (val_chirho + align_chirho - 1) & !(align_chirho - 1)
}

/// Align `val_chirho` downward to the nearest multiple of `align_chirho`.
/// `align_chirho` must be a power of two.
const fn align_down_chirho(val_chirho: u64, align_chirho: u64) -> u64 {
    val_chirho & !(align_chirho - 1)
}

/// Convert ELF segment flags (PF_R, PF_W, PF_X) to Linux PROT_* flags.
fn elf_flags_to_prot_chirho(flags_chirho: u32) -> u32 {
    let mut prot_chirho: u32 = PROT_READ_CHIRHO; // Always readable

    if flags_chirho & PF_W_CHIRHO != 0 {
        prot_chirho |= PROT_WRITE_CHIRHO;
    }
    if flags_chirho & PF_X_CHIRHO != 0 {
        prot_chirho |= PROT_EXEC_CHIRHO;
    }

    prot_chirho
}
