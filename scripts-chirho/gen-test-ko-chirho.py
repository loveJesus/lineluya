# For God so loved the world that he gave his only begotten Son,
# that whoever believes in him should not perish but have eternal life. - John 3:16
#
# Generate a minimal x86_64 ET_REL (.ko) ELF that calls printk when loaded.
# This creates a valid kernel module for testing Lineluya's ko_loader.
#
# The module has:
#   - init_module: calls printk("Hello from .ko module!\n") and returns 0
#   - cleanup_module: calls printk("Goodbye from .ko module!\n")
#
# Usage: python3 scripts-chirho/gen-test-ko-chirho.py > test-module-chirho.ko

import struct
import sys

# x86_64 machine code for init_module:
#   lea rdi, [rip + message]   ; load address of string
#   call printk                ; will be relocated
#   xor eax, eax               ; return 0
#   ret
#
# x86_64 machine code for cleanup_module:
#   lea rdi, [rip + bye_message]
#   call printk
#   ret

# Strings
init_msg_chirho = b"<6>Hello from .ko module! Lineluya ko_loader works! Hallelujah!\n\0"
cleanup_msg_chirho = b"<6>Goodbye from .ko module! Praise Jesus!\n\0"

# .text section: init_module + cleanup_module
# init_module:
#   48 8d 3d XX XX XX XX    lea rdi, [rip+offset_to_init_msg]
#   e8 XX XX XX XX          call printk (R_X86_64_PLT32 relocation)
#   31 c0                   xor eax, eax
#   c3                      ret
# cleanup_module:
#   48 8d 3d XX XX XX XX    lea rdi, [rip+offset_to_cleanup_msg]
#   e8 XX XX XX XX          call printk
#   c3                      ret

# We'll compute the RIP-relative offsets after placing .rodata

text_chirho = bytearray()

# init_module at offset 0
init_module_off_chirho = 0
# lea rdi, [rip + ???]  - will patch after .rodata placement
text_chirho += b"\x48\x8d\x3d\x00\x00\x00\x00"  # 7 bytes, offset at [3:7]
# call printk - R_X86_64_PLT32 relocation at offset 8
text_chirho += b"\xe8\x00\x00\x00\x00"            # 5 bytes, offset at [8:12]
# xor eax, eax; ret
text_chirho += b"\x31\xc0\xc3"                     # 3 bytes

# cleanup_module at offset 15
cleanup_module_off_chirho = len(text_chirho)
# lea rdi, [rip + ???]
text_chirho += b"\x48\x8d\x3d\x00\x00\x00\x00"  # 7 bytes
# call printk
text_chirho += b"\xe8\x00\x00\x00\x00"            # 5 bytes
# ret
text_chirho += b"\xc3"                              # 1 byte

text_size_chirho = len(text_chirho)

# .rodata section
rodata_chirho = init_msg_chirho + cleanup_msg_chirho
rodata_size_chirho = len(rodata_chirho)
init_msg_rodata_off_chirho = 0
cleanup_msg_rodata_off_chirho = len(init_msg_chirho)

# Patch RIP-relative LEA offsets (pointing from .text to .rodata)
# For init_module lea at offset 3: target = .rodata base + init_msg_offset
# RIP at end of lea = text_offset + 7
# Relative = rodata_section_offset + init_msg_offset - (text_section_offset + 7)
# We'll set these as relocations instead (R_X86_64_PC32 to section .rodata)

# .strtab: \0, .text\0, .rodata\0, .symtab\0, .strtab\0, .rela.text\0,
#          init_module\0, cleanup_module\0, printk\0, test_module\0
strtab_chirho = b"\0"
idx_text_chirho = len(strtab_chirho)
strtab_chirho += b".text\0"
idx_rodata_chirho = len(strtab_chirho)
strtab_chirho += b".rodata\0"
idx_symtab_chirho = len(strtab_chirho)
strtab_chirho += b".symtab\0"
idx_strtab_chirho = len(strtab_chirho)
strtab_chirho += b".strtab\0"
idx_rela_text_chirho = len(strtab_chirho)
strtab_chirho += b".rela.text\0"
idx_init_module_chirho = len(strtab_chirho)
strtab_chirho += b"init_module\0"
idx_cleanup_module_chirho = len(strtab_chirho)
strtab_chirho += b"cleanup_module\0"
idx_printk_chirho = len(strtab_chirho)
strtab_chirho += b"printk\0"
idx_modname_chirho = len(strtab_chirho)
strtab_chirho += b"test_ko_chirho\0"
strtab_size_chirho = len(strtab_chirho)

# Symbol table entries (Elf64_Sym = 24 bytes each)
# [0] null symbol
# [1] .text section symbol (for relocations)
# [2] .rodata section symbol
# [3] init_module (STB_GLOBAL, STT_FUNC)
# [4] cleanup_module (STB_GLOBAL, STT_FUNC)
# [5] printk (STB_GLOBAL, STT_NOTYPE, SHN_UNDEF - external)

def elf64_sym_chirho(name_chirho, value_chirho, size_chirho, info_chirho, other_chirho, shndx_chirho):
    return struct.pack("<IBBHQQ", name_chirho, info_chirho, other_chirho, shndx_chirho, value_chirho, size_chirho)

STB_LOCAL_CHIRHO = 0
STB_GLOBAL_CHIRHO = 1
STT_NOTYPE_CHIRHO = 0
STT_FUNC_CHIRHO = 2
STT_SECTION_CHIRHO = 3
SHN_UNDEF_CHIRHO = 0

def st_info_chirho(bind_chirho, type_chirho):
    return (bind_chirho << 4) | type_chirho

symtab_chirho = b""
symtab_chirho += elf64_sym_chirho(0, 0, 0, 0, 0, 0)  # [0] null
symtab_chirho += elf64_sym_chirho(0, 0, 0, st_info_chirho(STB_LOCAL_CHIRHO, STT_SECTION_CHIRHO), 0, 1)  # [1] .text section
symtab_chirho += elf64_sym_chirho(0, 0, 0, st_info_chirho(STB_LOCAL_CHIRHO, STT_SECTION_CHIRHO), 0, 2)  # [2] .rodata section
symtab_chirho += elf64_sym_chirho(idx_init_module_chirho, 0, text_size_chirho, st_info_chirho(STB_GLOBAL_CHIRHO, STT_FUNC_CHIRHO), 0, 1)  # [3] init_module
symtab_chirho += elf64_sym_chirho(idx_cleanup_module_chirho, cleanup_module_off_chirho, 13, st_info_chirho(STB_GLOBAL_CHIRHO, STT_FUNC_CHIRHO), 0, 1)  # [4] cleanup_module
symtab_chirho += elf64_sym_chirho(idx_printk_chirho, 0, 0, st_info_chirho(STB_GLOBAL_CHIRHO, STT_NOTYPE_CHIRHO), 0, SHN_UNDEF_CHIRHO)  # [5] printk (external)
symtab_size_chirho = len(symtab_chirho)

# RELA entries for .text (Elf64_Rela = 24 bytes each)
# We need 4 relocations:
# 1. init_module lea [offset 3]: R_X86_64_PC32 to .rodata+0 (init_msg)
# 2. init_module call [offset 8]: R_X86_64_PLT32 to printk
# 3. cleanup_module lea [offset cleanup+3]: R_X86_64_PC32 to .rodata+len(init_msg)
# 4. cleanup_module call [offset cleanup+8]: R_X86_64_PLT32 to printk

R_X86_64_PC32_CHIRHO = 2
R_X86_64_PLT32_CHIRHO = 4

def elf64_rela_chirho(offset_chirho, sym_chirho, type_chirho, addend_chirho):
    info_chirho = (sym_chirho << 32) | type_chirho
    return struct.pack("<QqQ" if addend_chirho >= 0 else "<Qqq", offset_chirho, info_chirho, addend_chirho)

# Actually use little-endian properly
def elf64_rela_chirho(offset_chirho, sym_chirho, type_chirho, addend_chirho):
    info_chirho = (sym_chirho << 32) | type_chirho
    return struct.pack("<QQq", offset_chirho, info_chirho, addend_chirho)

rela_chirho = b""
# LEA in init_module at offset 3, target .rodata section sym [2], addend = init_msg offset - 4
rela_chirho += elf64_rela_chirho(3, 2, R_X86_64_PC32_CHIRHO, init_msg_rodata_off_chirho - 4)
# CALL in init_module at offset 8, target printk sym [5], addend = -4
rela_chirho += elf64_rela_chirho(8, 5, R_X86_64_PLT32_CHIRHO, -4)
# LEA in cleanup_module at offset cleanup+3, target .rodata section sym [2]
rela_chirho += elf64_rela_chirho(cleanup_module_off_chirho + 3, 2, R_X86_64_PC32_CHIRHO, cleanup_msg_rodata_off_chirho - 4)
# CALL in cleanup_module at offset cleanup+8, target printk sym [5]
rela_chirho += elf64_rela_chirho(cleanup_module_off_chirho + 8, 5, R_X86_64_PLT32_CHIRHO, -4)
rela_size_chirho = len(rela_chirho)

# Layout sections:
# ELF header: 64 bytes
# Section 0 (null): no data
# Section 1 (.text): text_chirho
# Section 2 (.rodata): rodata_chirho
# Section 3 (.symtab): symtab_chirho
# Section 4 (.strtab): strtab_chirho
# Section 5 (.rela.text): rela_chirho
# Section headers: 6 * 64 bytes

ehdr_size_chirho = 64
shdr_size_chirho = 64
num_sections_chirho = 6

# Data starts after ELF header
data_start_chirho = ehdr_size_chirho

text_off_chirho = data_start_chirho
rodata_off_chirho = text_off_chirho + text_size_chirho
symtab_off_chirho = rodata_off_chirho + rodata_size_chirho
strtab_off_chirho = symtab_off_chirho + symtab_size_chirho
rela_off_chirho = strtab_off_chirho + strtab_size_chirho
shdr_off_chirho = rela_off_chirho + rela_size_chirho

# Build ELF header
ehdr_chirho = struct.pack("<4sBBBBBxxxxxxx",
    b"\x7fELF",
    2,    # ELFCLASS64
    1,    # ELFDATA2LSB
    1,    # EV_CURRENT
    0,    # ELFOSABI_NONE
    0,    # ABI version
)
ehdr_chirho += struct.pack("<HHIQQQIHHHHHH",
    1,              # ET_REL
    0x3E,           # EM_X86_64
    1,              # EV_CURRENT
    0,              # e_entry
    0,              # e_phoff (no program headers)
    shdr_off_chirho, # e_shoff
    0,              # e_flags
    ehdr_size_chirho, # e_ehsize
    0,              # e_phentsize
    0,              # e_phnum
    shdr_size_chirho, # e_shentsize
    num_sections_chirho, # e_shnum
    4,              # e_shstrndx (.strtab)
)

# Build section headers
def shdr_chirho(name_chirho, type_chirho, flags_chirho, addr_chirho, offset_chirho, size_chirho, link_chirho, info_chirho, align_chirho, entsize_chirho):
    return struct.pack("<IIQQQQIIQQQ",
        name_chirho, type_chirho, flags_chirho, addr_chirho, offset_chirho, size_chirho,
        link_chirho, info_chirho, align_chirho, entsize_chirho, 0)[:64]

# Actually section header is 64 bytes:
def shdr_chirho(name_chirho, sh_type_chirho, flags_chirho, addr_chirho, offset_chirho, size_chirho, link_chirho, info_chirho, align_chirho, entsize_chirho):
    return struct.pack("<IIQQQQIIQQ",
        name_chirho, sh_type_chirho, flags_chirho, addr_chirho, offset_chirho, size_chirho,
        link_chirho, info_chirho, align_chirho, entsize_chirho)

SHT_NULL_CHIRHO = 0
SHT_PROGBITS_CHIRHO = 1
SHT_SYMTAB_CHIRHO = 2
SHT_STRTAB_CHIRHO = 3
SHT_RELA_CHIRHO = 4
SHF_ALLOC_CHIRHO = 2
SHF_EXECINSTR_CHIRHO = 4

shdrs_chirho = b""
shdrs_chirho += shdr_chirho(0, SHT_NULL_CHIRHO, 0, 0, 0, 0, 0, 0, 0, 0)  # [0] null
shdrs_chirho += shdr_chirho(idx_text_chirho, SHT_PROGBITS_CHIRHO, SHF_ALLOC_CHIRHO | SHF_EXECINSTR_CHIRHO, 0, text_off_chirho, text_size_chirho, 0, 0, 16, 0)  # [1] .text
shdrs_chirho += shdr_chirho(idx_rodata_chirho, SHT_PROGBITS_CHIRHO, SHF_ALLOC_CHIRHO, 0, rodata_off_chirho, rodata_size_chirho, 0, 0, 1, 0)  # [2] .rodata
shdrs_chirho += shdr_chirho(idx_symtab_chirho, SHT_SYMTAB_CHIRHO, 0, 0, symtab_off_chirho, symtab_size_chirho, 4, 3, 8, 24)  # [3] .symtab, link=strtab, info=first global
shdrs_chirho += shdr_chirho(idx_strtab_chirho, SHT_STRTAB_CHIRHO, 0, 0, strtab_off_chirho, strtab_size_chirho, 0, 0, 1, 0)  # [4] .strtab
shdrs_chirho += shdr_chirho(idx_rela_text_chirho, SHT_RELA_CHIRHO, 0, 0, rela_off_chirho, rela_size_chirho, 3, 1, 8, 24)  # [5] .rela.text, link=symtab, info=.text

# Assemble the ELF
elf_chirho = ehdr_chirho + text_chirho + rodata_chirho + symtab_chirho + strtab_chirho + rela_chirho + shdrs_chirho

sys.stdout.buffer.write(elf_chirho)
sys.stderr.write(f"Generated test .ko: {len(elf_chirho)} bytes\n")
sys.stderr.write(f"  .text:      {text_size_chirho} bytes at offset {text_off_chirho}\n")
sys.stderr.write(f"  .rodata:    {rodata_size_chirho} bytes at offset {rodata_off_chirho}\n")
sys.stderr.write(f"  .symtab:    {symtab_size_chirho} bytes ({symtab_size_chirho // 24} symbols)\n")
sys.stderr.write(f"  .rela.text: {rela_size_chirho} bytes ({rela_size_chirho // 24} relocations)\n")
