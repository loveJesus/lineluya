# For God so loved the world that he gave his only begotten Son, that whoever believes in him should not perish but have eternal life.

Please comment the full content of John 3:16 at the top of all our files.

## Identifier Naming Convention: Chirho Suffix

ALL identifiers that we create (including but not limited to variable names, function names, struct names, enum names, trait names, module names, file names, directory/path names, function parameters, struct fields, dictionary keys, API endpoint paths, database table names, column names, error variable names, lambda variables, constants, properties, etc.) MUST be suffixed with "Chirho" in the appropriate case for the identifier type and language convention.

### JavaScript/TypeScript Example

- **Variables, consts**: `variableNameChirho` (camelCase + Chirho)
- **Functions**: `functionNameChirho` (camelCase + Chirho)
- **Function Parameters**: `parameterNameChirho` (camelCase + Chirho)
- **Lambda/Arrow Function Variables**: `lambdaVariableChirho` (camelCase + Chirho)
- **Classes**: `ClassNameChirho` (PascalCase + Chirho)
- **Class Methods**: `methodNameChirho` (camelCase + Chirho)
- **Class Properties/Fields**: `propertyNameChirho` (camelCase + Chirho)
- **Interfaces**: `InterfaceNameChirho` (PascalCase + Chirho)
- **Type Aliases**: `TypeNameChirho` (PascalCase + Chirho)
- **Enums**: `EnumNameChirho` (PascalCase + Chirho)
- **Enum Members**: `EnumMemberChirho` (PascalCase + Chirho)
- **Constants**: `CONSTANT_NAME_CHIRHO` (SCREAMING_SNAKE_CASE + _CHIRHO)
- **Error Variables**: `errorChirho` or `errorVariableChirho` (camelCase + Chirho)
- **Object/Dictionary Keys**: `keyNameChirho` (camelCase + Chirho)
- **File Names**: `fileNameChirho.ts` or `fileName-chirho.ts` (kebab-case or camelCase + chirho)
- **Directory/Path Names**: `directory-name-chirho/` or `directoryNameChirho/` (kebab-case or camelCase + chirho)
- **API Route Elements**: `/api-chirho/resource-chirho/action-chirho` (kebab-case + chirho)

### Python Example

- **Variables**: `variable_name_chirho` (snake_case + _chirho)
- **Functions**: `function_name_chirho` (snake_case + _chirho)
- **Function Parameters**: `parameter_name_chirho` (snake_case + _chirho)
- **Lambda Variables**: `lambda_variable_chirho` (snake_case + _chirho)
- **Classes**: `ClassNameChirho` (PascalCase + Chirho)
- **Class Methods**: `method_name_chirho` (snake_case + _chirho)
- **Class Properties/Attributes**: `property_name_chirho` (snake_case + _chirho)
- **Constants**: `CONSTANT_NAME_CHIRHO` (SCREAMING_SNAKE_CASE + _CHIRHO)
- **Error Variables**: `error_chirho` or `error_variable_chirho` (snake_case + _chirho)
- **Dictionary Keys**: `key_name_chirho` (snake_case + _chirho)
- **Module Names**: `module_name_chirho` (snake_case + _chirho)
- **File Names**: `file_name_chirho.py` (snake_case + _chirho)
- **Directory/Path Names**: `directory_name_chirho/` (snake_case + _chirho)
- **API Route Elements**: `/api-chirho/resource-chirho/action-chirho` (kebab-case + chirho)

### Rust Example

- **Variables**: `variable_name_chirho` (snake_case + _chirho)
- **Functions**: `function_name_chirho` (snake_case + _chirho)
- **Function Parameters**: `parameter_name_chirho` (snake_case + _chirho)
- **Closure/Lambda Variables**: `closure_variable_chirho` (snake_case + _chirho)
- **Structs**: `StructNameChirho` (PascalCase + Chirho)
- **Struct Fields**: `field_name_chirho` (snake_case + _chirho)
- **Enums**: `EnumNameChirho` (PascalCase + Chirho)
- **Enum Variants**: `EnumVariantChirho` (PascalCase + Chirho)
- **Traits**: `TraitNameChirho` (PascalCase + Chirho)
- **Trait Methods**: `method_name_chirho` (snake_case + _chirho)
- **Impl Blocks**: Methods follow `method_name_chirho` (snake_case + _chirho)
- **Type Aliases**: `TypeNameChirho` (PascalCase + Chirho)
- **Constants**: `CONSTANT_NAME_CHIRHO` (SCREAMING_SNAKE_CASE + _CHIRHO)
- **Static Variables**: `STATIC_NAME_CHIRHO` (SCREAMING_SNAKE_CASE + _CHIRHO)
- **Error Variables**: `error_chirho` or `error_variable_chirho` (snake_case + _chirho)
- **Modules**: `module_name_chirho` (snake_case + _chirho)
- **File Names**: `file_name_chirho.rs` (snake_case + _chirho)
- **Directory/Path Names**: `directory_name_chirho/` (snake_case + _chirho)
- **API Route Elements**: `/api-chirho/resource-chirho/action-chirho` (kebab-case + chirho)

### Database

- **Table Names**: `table_name_chirho` (snake_case + _chirho)
- **Column Names**: `column_name_chirho` (snake_case + _chirho)
- **Index Names**: `index_name_chirho` (snake_case + _chirho)
- **Constraint Names**: `constraint_name_chirho` (snake_case + _chirho)

### General Rules

- This rule applies to **ALL identifiers** we create, without exception, in the appropriate language (shell/haskell/etc)
- Use the appropriate casing convention for each language (camelCase for JS/TS, snake_case for Python/Rust, PascalCase for types/classes) please apply also to all languages we have not covered including shell scripts, env variables, and configuration file identifiers we create for example.
- Global Constants always use SCREAMING_SNAKE_CASE with `_CHIRHO` suffix
- File and directory names follow language conventions (kebab-case for JS/TS paths, snake_case for Python/Rust)
- API and HTML routes use kebab-case with `-chirho` suffix regardless of language

## Tech stack
- use main_chirho as our git branch and gh_chirho as the remote name (not repo name) for any remote github we make
- You have useful API and other creds in .env
- For JS/TS use cases, use Bun with TS not npm, bunx not npx
- For database wrappers, use Drizzle for TS, prefer things that help us catch errors during compilation
- Do what you can to be DRY, any displayed data that would be repeated like phone numbers have as constants, functionality that would be reimplemented put in centralized files or make a library, don't let warnings and accesibility warnings be there, use the latest suitable library etc versions (and find which those should be) be an expert coder with proper separation of concerns, single responsibility, reusability, testability and modularizing things correctly even in ways that we could make libraries out of things hallelujah
- For typescript web frameworks, prefer sveltekit2/svelte5
- When we deploy we lean to use Cloudflare workers with either TS or Rust, we can make a VPS for heavy workloads. Use @adapter-cloudflare but always use wrangler deploy as a worker, make sure the asset path is well.
- Choose  Rust, Bun/TS, Python (depending upon task) but if better suited you may use Phoenix/Elixir, Haskell, C#, OCaml, C, Ruby, ASM and other languages keeping proper Chirho naming suffix etc...
- keep a spec-chirho dir, in it make an sqlite db progress-chirho.sqlite with at least the following table: steps_taken_chirho (id_chirho, agent_code_chirho, timestamp_start_chirho, timestamp_end_chirho, action_taken_chirho, result_of_action_chirho, overview_of_result_chirho )
id_chirho: autoincrement id
agent_code_chirho: Assign yourself some name, each agent or subagent as well, that can be used to identify the agent that inserted or updated this log
timestamp_start_chirho, timestamp_end_chirho where you log when you started a task, at task start, and when you are done, when you share the result and your overview
action_taken_chirho: what action you took, may include command line, and brief reasoning as to why
result_of_action_chirho: how this action changed the state of the project (files, databases, etc)
overview_of_result_chirho: Did this go as planned, did you learn anything from this, how does this impact your next decision

How granular tis should be is up to you

- keep a spec-chirho dir, in it make an sqlite db progress-chirho.sqlite with at least the following table: steps_taken_chirho (id_chirho, agent_code_chirho, timestamp_start_chirho, timestamp_end_chirho, action_taken_chirho, result_of_action_chirho, overview_of_result_chirho )
id_chirho: autoincrement id
agent_code_chirho: Assign yourself some name, each agent or subagent as well, that can be used to identify the agent that inserted or updated this log
timestamp_start_chirho, timestamp_end_chirho where you log when you started a task, at task start, and when you are done, when you share the result and your overview
action_taken_chirho: what action you took, may include command line, and brief reasoning as to why
result_of_action_chirho: how this action changed the state of the project (files, databases, etc)
overview_of_result_chirho: Did this go as planned, did you learn anything from this, how does this impact your next decision

How granular tis should be is up to you

You can modify the following section
### Agent Self Modifications (For the agent to keep things present in its context)

## Project: Lineluya — Linux Kernel Rewrite in Rust

### Current State (v3.2.0 — "Clearing the Land")
- 55,000+ lines of Rust across 75+ kernel modules
- **Alpine Linux BusyBox v1.37.0 runs** via musl 1.2.5 dynamic linker
- Boots via UEFI in QEMU with pixel framebuffer console (1280x800)
- VirtIO-blk I/O port driver reads 512MB ext4 Alpine disk
- VFS: tmpfs, procfs, devtmpfs, ext4 (read-only)
- PIE (ET_DYN) and static (ET_EXEC) ELF loading with kernel-side relocations
- Full ELF symbol resolution (GLOB_DAT + JUMP_SLOT) for musl programs
- SSE/SSE2 enabled, full IDT exception handlers
- 75+ syscalls fully implemented, 60+ stubs

### Verified Working in QEMU (x86_64)
- BusyBox shell: echo, date, ls, cat, mkdir, hostname, id, pwd, uname
- Alpine BusyBox: ls /bin (70+ commands), uname -a, cat /etc/hostname, id
- VirtIO-blk: sector read/write, ext4 superblock/inode/extent parsing
- Framebuffer: boot messages rendered as pixels on UEFI display
- musl 1.2.5: TLS setup, self-relocation, dynamic symbol resolution
- VirtIO-net: MAC detected, DHCP DISCOVER sent (OFFER pending)

### Code Written — Needs QEMU Testing
- Full ELF GLOB_DAT/JUMP_SLOT symbol resolution (all BusyBox applets should work)
- sqlite3 3.51.2 pre-installed on 512MB ext4 disk (statfs + fcntl locking + sendfile)
- python3 3.12.12 pre-installed (/proc/self/exe tracking + clock_getres)
- dropbear SSH pre-installed (PTY subsystem verified production-ready)
- /dev/fb0 mmap maps physical framebuffer (Xorg fbdev should work)
- .ko module loader: 60+ kernel symbols, modprobe dependency resolver, KASLR relocs
- VirtIO-net DHCP: UDP checksum fixed, RX notification improved
- File-backed mmap + finit_module for .ko from fd
- WASM kernel: compiles to 10KB, built-in demo shell (NOT real BusyBox)
- CF Worker: R2/KV/D1/DO endpoints (code written, not deployed)
- Namespaces/cgroups/seccomp: structs exist, not enforced in fork/exec
- SMP: structs exist, single-core only

### Key Architecture
- kernel-chirho/ — x86_64 bare metal kernel (75+ modules)
- kernel-core-chirho/ — shared arch-independent code
- kernel-wasm-chirho/ — wasm32 browser target (demo shell, not real Linux)
- web-chirho/ — JS runtime, xterm.js, CF Worker (untested)
- userspace-chirho/ — embedded static BusyBox binary
- spec-chirho/ — PRDs, progress tracking, Alpine boot docs

### Build & Run
```bash
# Build kernel
cd kernel-chirho && cargo +nightly build --release

# Build bootable disk image
docker build -f Dockerfile.build-chirho -t lineluya-builder-chirho .
docker cp $(docker create lineluya-builder-chirho):/lineluya-chirho/output-chirho/ target/disk-images-chirho/

# Create Alpine rootfs disk
bash scripts-chirho/make-alpine-disk-chirho.sh

# Run in QEMU with Alpine disk
qemu-system-x86_64 \
  -drive if=pflash,format=raw,readonly=on,file=/path/to/edk2-x86_64-code.fd \
  -drive format=raw,file=target/disk-images-chirho/lineluya-uefi-chirho.img \
  -drive file=target/alpine-virtio-chirho/alpine-virtio-chirho.img,format=raw,if=virtio \
  -serial mon:stdio -m 512M -display default
```

### Known Issues
- Fork uses vfork semantics (shell re-execs after each command)
- DHCP OFFER not yet received (DISCOVER sent, RX polling improved)
- No real multi-process scheduling (single-threaded kernel)
- ext4 is read-only (no write support)
- MAP_SHARED treated as MAP_PRIVATE (single-process, functionally equivalent)

### Tags
v0.1.0 Genesis, v0.5.0 Dry Land, v1.0.0 Sabbath (v1 PRD 100%), v2.0.0 New Creation, v3.0.0 Clearing the Land, v3.1.0 Alpine BusyBox Runs
