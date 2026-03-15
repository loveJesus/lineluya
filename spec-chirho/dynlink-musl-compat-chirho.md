# For God so loved the world that he gave his only begotten Son, that whoever believes in him should not perish but have eternal life. - John 3:16

# G1-002: Dynamic Linker (ld-musl) Compatibility Assessment

This document assesses whether Lineluya's dynamic linker support in
`kernel-chirho/src/dynlink_chirho.rs` can correctly load and execute musl's
`ld-musl-x86_64.so.1`, which is the dynamic linker/interpreter used by all
dynamically linked Alpine Linux binaries.

## musl's ld-musl-x86_64.so.1 Architecture

Unlike glibc's `ld-linux-x86-64.so.2`, musl's dynamic linker is **the libc
itself**. The file `/lib/ld-musl-x86_64.so.1` is a symlink (or hardlink) to
`/lib/libc.musl-x86_64.so.1`. When the kernel loads a dynamically linked ELF
binary, it:

1. Reads the `PT_INTERP` segment to find `/lib/ld-musl-x86_64.so.1`
2. Loads the interpreter as an `ET_DYN` ELF at a separate base address
3. Sets `AT_BASE` in the auxiliary vector to the interpreter's load address
4. Jumps to the interpreter's entry point (not the main executable's)
5. The interpreter self-relocates, loads shared libraries, then jumps to the
   main executable's entry point (from `AT_ENTRY`)

## Lineluya Compatibility Checklist

### 1. PT_INTERP Extraction

**Status: READY**

`dynlink_chirho.rs` provides:
- `find_interp_in_phdrs_chirho()` -- scans program headers for PT_INTERP
- `extract_interp_path_chirho()` -- reads the NUL-terminated path string

musl binaries will have `PT_INTERP` set to `/lib/ld-musl-x86_64.so.1`.
This path is correctly extracted as a UTF-8 string.

**Requirement:** The VFS must be able to resolve `/lib/ld-musl-x86_64.so.1`
from the Alpine rootfs (either via initramfs, ext4, or a mounted filesystem).

### 2. ET_DYN Loading at Arbitrary Base

**Status: READY**

`load_elf_at_base_chirho()` correctly handles `ET_DYN` binaries:
- Computes `load_bias = base_addr - first_vaddr` for PIE/shared objects
- Maps all `PT_LOAD` segments with the bias applied
- Copies `.text`/`.data` from the ELF file into mapped memory
- Zeroes BSS regions (memsz > filesz)
- Returns adjusted entry point, phdr address, and brk

musl's `ld-musl-x86_64.so.1` is an `ET_DYN` ELF with first `PT_LOAD` vaddr
at 0, which is the standard case. The `INTERP_LOAD_BASE_CHIRHO` constant
(`0x7F00_0010_0000`) provides a suitable non-colliding base address.

### 3. AT_BASE Auxiliary Vector Entry

**Status: READY**

`build_dynlink_auxv_chirho()` sets `AT_BASE` to the interpreter's load base
address. This is critical because musl's `__dls2()` initialization function
uses `AT_BASE` to:
- Find its own ELF headers in memory
- Locate its `.dynamic` section
- Perform self-relocation (R_X86_64_RELATIVE entries)

The auxiliary vector also correctly includes:
- `AT_PHDR` -- main executable's program header table address
- `AT_PHENT` -- program header entry size
- `AT_PHNUM` -- number of program headers
- `AT_ENTRY` -- main executable's entry point
- `AT_PAGESZ` -- 4096
- `AT_UID`/`AT_EUID`/`AT_GID`/`AT_EGID` -- credential info
- `AT_RANDOM` -- pointer to 16 random bytes (currently using entry point as placeholder)
- `AT_NULL` -- terminator

### 4. Self-Relocation Support

**Status: READY (with caveats)**

`apply_relative_relocs_chirho()` handles `R_X86_64_RELATIVE` relocations,
which are the primary relocation type musl needs during self-bootstrap.

However, musl's dynamic linker performs its own self-relocation during
`__dls2()` and `__dls3()`, so the kernel does NOT need to apply relocations
for the interpreter. The kernel only needs to:
1. Load the interpreter's PT_LOAD segments at the chosen base
2. Copy file data and zero BSS
3. Jump to the interpreter's entry point

musl will handle all relocations (both its own and the main executable's)
internally. The kernel's relocation support is useful for loading statically
PIE executables that have no interpreter.

### 5. Stack Layout for musl Entry

**Status: NEEDS VERIFICATION**

musl's entry point (`_dlstart`) expects the standard Linux stack layout:

```
[top of stack]
  argc          (8 bytes)
  argv[0]       (pointer)
  argv[1]       (pointer)
  ...
  argv[argc]    (NULL)
  envp[0]       (pointer)
  envp[1]       (pointer)
  ...
  envp[n]       (NULL)
  auxv[0].type  (8 bytes)
  auxv[0].value (8 bytes)
  ...
  AT_NULL, 0
[bottom of stack data]
```

**Action item:** Verify that `exec_chirho.rs` builds the user stack in exactly
this layout. The `rsp` register must point to `argc` when control transfers
to the interpreter entry point.

### 6. AT_RANDOM Requirement

**Status: NEEDS FIX**

musl reads 16 bytes from the address pointed to by `AT_RANDOM` for stack
canary initialization (`__stack_chk_guard`). Currently, `AT_RANDOM` points
to `original_entry_chirho` which is a code address, not a buffer of random
bytes.

**Fix:** Allocate 16 bytes on the user stack (before argv strings) and fill
them with random data from `getrandom()`, then point `AT_RANDOM` to that
buffer.

### 7. Required Filesystem Entries

For musl's dynamic linker to work, the following must be accessible:

| Path | Purpose | Status |
|------|---------|--------|
| `/lib/ld-musl-x86_64.so.1` | The interpreter itself | VFS must resolve |
| `/lib/libc.musl-x86_64.so.1` | musl libc (same file) | Symlink or hardlink |
| `/etc/ld-musl-x86_64.path` | Library search paths | Optional (fallback: /lib:/usr/local/lib:/usr/lib) |

### 8. Syscalls During musl Initialization

musl's dynamic linker initialization (`__dls2` -> `__dls3`) makes these
syscalls before reaching `main()`:

1. **`mmap`** -- Map shared libraries (DONE)
2. **`mprotect`** -- Set segment permissions (DONE)
3. **`brk`** -- Initialize heap (DONE)
4. **`arch_prctl(ARCH_SET_FS)`** -- Set TLS base (DONE)
5. **`set_tid_address`** -- Thread setup (DONE)
6. **`set_robust_list`** -- Thread robustness (STUB, OK)
7. **`rt_sigaction`** -- Install signal handlers (DONE)
8. **`rt_sigprocmask`** -- Unblock signals (DONE)
9. **`prlimit64`** -- Query resource limits (DONE)
10. **`getrandom`** -- Stack canary seed (DONE)
11. **`clock_gettime`** -- Time initialization (DONE)
12. **`openat`** -- Open shared libraries (DONE)
13. **`read`** -- Read ELF headers of libraries (DONE)
14. **`close`** -- Close library fds (DONE)
15. **`fcntl`** -- F_DUPFD_CLOEXEC (DONE)

All critical syscalls for musl initialization are implemented.

## Verdict

**Lineluya's dynamic linker support is largely ready for musl.** The core
mechanisms (ET_DYN loading, AT_BASE, PT_INTERP extraction, auxiliary vector)
are correctly implemented.

### Remaining Action Items

| Priority | Item | Effort |
|----------|------|--------|
| HIGH | Fix AT_RANDOM to point to real random bytes on user stack | Small |
| HIGH | Verify user stack layout matches Linux convention exactly | Small |
| MEDIUM | Ensure VFS can resolve `/lib/ld-musl-x86_64.so.1` from rootfs | Depends on rootfs mount |
| LOW | Test with actual Alpine binary end-to-end | Integration test |
