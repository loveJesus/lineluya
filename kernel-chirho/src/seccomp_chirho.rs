// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! seccomp-BPF (Secure Computing with BPF) for the Lineluya kernel.
//!
//! ## Track F — Container Runtime (F1-006)
//!
//! Implements:
//! - `seccomp(2)` syscall with SECCOMP_SET_MODE_STRICT and
//!   SECCOMP_SET_MODE_FILTER
//! - BPF filter program storage per-task
//! - **Full BPF interpreter** that evaluates classic BPF programs against
//!   `struct seccomp_data` at syscall entry time
//! - Return actions: ALLOW, KILL, ERRNO, TRAP, TRACE, LOG
//!
//! The BPF interpreter supports the following instruction classes:
//! - LD (load word/half/byte from seccomp_data)
//! - LDX (load into index register)
//! - ALU (add, sub, mul, div, and, or, xor, lsh, rsh, neg)
//! - JMP (ja, jeq, jgt, jge, jset — with K or X operands)
//! - RET (return action code)
//! - MISC (tax, txa)
//! - ST/STX (store to scratch memory)
//!
//! Reference: seccomp(2), Linux kernel/seccomp.c, net/core/filter.c

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use spin::Mutex;

// ============================================================================
// seccomp constants (matching Linux uapi)
// ============================================================================

/// seccomp(2) operations.
pub const SECCOMP_SET_MODE_STRICT_CHIRHO: u32 = 0;
pub const SECCOMP_SET_MODE_FILTER_CHIRHO: u32 = 1;
pub const SECCOMP_GET_ACTION_AVAIL_CHIRHO: u32 = 2;
#[allow(dead_code)]
pub const SECCOMP_GET_NOTIF_SIZES_CHIRHO: u32 = 3;

/// seccomp filter return action values (upper 16 bits).
pub const SECCOMP_RET_KILL_PROCESS_CHIRHO: u32 = 0x8000_0000;
pub const SECCOMP_RET_KILL_THREAD_CHIRHO: u32 = 0x0000_0000;
pub const SECCOMP_RET_TRAP_CHIRHO: u32 = 0x0003_0000;
pub const SECCOMP_RET_ERRNO_CHIRHO: u32 = 0x0005_0000;
#[allow(dead_code)]
pub const SECCOMP_RET_USER_NOTIF_CHIRHO: u32 = 0x7FC0_0000;
pub const SECCOMP_RET_TRACE_CHIRHO: u32 = 0x7FF0_0000;
pub const SECCOMP_RET_LOG_CHIRHO: u32 = 0x7FFC_0000;
pub const SECCOMP_RET_ALLOW_CHIRHO: u32 = 0x7FFF_0000;

/// Mask for the return data (lower 16 bits — errno value for RET_ERRNO).
pub const SECCOMP_RET_DATA_MASK_CHIRHO: u32 = 0x0000_FFFF;
/// Mask for the return action (upper 16 bits).
pub const SECCOMP_RET_ACTION_MASK_CHIRHO: u32 = 0xFFFF_0000;

/// seccomp filter flags.
#[allow(dead_code)]
pub const SECCOMP_FILTER_FLAG_TSYNC_CHIRHO: u32 = 1 << 0;
#[allow(dead_code)]
pub const SECCOMP_FILTER_FLAG_LOG_CHIRHO: u32 = 1 << 1;
#[allow(dead_code)]
pub const SECCOMP_FILTER_FLAG_SPEC_ALLOW_CHIRHO: u32 = 1 << 2;
#[allow(dead_code)]
pub const SECCOMP_FILTER_FLAG_NEW_LISTENER_CHIRHO: u32 = 1 << 3;

// ============================================================================
// BPF instruction (classic BPF for seccomp)
// ============================================================================

/// Classic BPF instruction (8 bytes).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct BpfInsnChirho {
    /// Opcode.
    pub code_chirho: u16,
    /// Jump if true.
    pub jt_chirho: u8,
    /// Jump if false.
    pub jf_chirho: u8,
    /// Generic multi-use field (constant, offset, etc.).
    pub k_chirho: u32,
}

// ============================================================================
// Classic BPF opcode encoding (matching Linux include/uapi/linux/filter.h)
// ============================================================================

// Instruction classes (bits 2:0)
const BPF_CLASS_LD_CHIRHO: u16 = 0x00;
const BPF_CLASS_LDX_CHIRHO: u16 = 0x01;
const BPF_CLASS_ST_CHIRHO: u16 = 0x02;
const BPF_CLASS_STX_CHIRHO: u16 = 0x03;
const BPF_CLASS_ALU_CHIRHO: u16 = 0x04;
const BPF_CLASS_JMP_CHIRHO: u16 = 0x05;
const BPF_CLASS_RET_CHIRHO: u16 = 0x06;
const BPF_CLASS_MISC_CHIRHO: u16 = 0x07;

// LD/LDX size (bits 4:3)
const BPF_SIZE_W_CHIRHO: u16 = 0x00;  // 32-bit word
const BPF_SIZE_H_CHIRHO: u16 = 0x08;  // 16-bit halfword
const BPF_SIZE_B_CHIRHO: u16 = 0x10;  // 8-bit byte

// LD/LDX mode (bits 7:5)
const BPF_MODE_IMM_CHIRHO: u16 = 0x00;  // Immediate
const BPF_MODE_ABS_CHIRHO: u16 = 0x20;  // Absolute offset into data
const BPF_MODE_IND_CHIRHO: u16 = 0x40;  // Indirect (X + k)
const BPF_MODE_MEM_CHIRHO: u16 = 0x60;  // Scratch memory

// ALU operations (bits 7:4)
const BPF_ALU_ADD_CHIRHO: u16 = 0x00;
const BPF_ALU_SUB_CHIRHO: u16 = 0x10;
const BPF_ALU_MUL_CHIRHO: u16 = 0x20;
const BPF_ALU_DIV_CHIRHO: u16 = 0x30;
const BPF_ALU_OR_CHIRHO: u16 = 0x40;
const BPF_ALU_AND_CHIRHO: u16 = 0x50;
const BPF_ALU_LSH_CHIRHO: u16 = 0x60;
const BPF_ALU_RSH_CHIRHO: u16 = 0x70;
const BPF_ALU_NEG_CHIRHO: u16 = 0x80;
#[allow(dead_code)]
const BPF_ALU_MOD_CHIRHO: u16 = 0x90;
const BPF_ALU_XOR_CHIRHO: u16 = 0xA0;

// ALU/JMP source (bit 3)
const BPF_SRC_K_CHIRHO: u16 = 0x00;  // K immediate
const BPF_SRC_X_CHIRHO: u16 = 0x08;  // X register

// JMP operations (bits 7:4)
const BPF_JMP_OP_JA_CHIRHO: u16 = 0x00;
const BPF_JMP_JEQ_CHIRHO: u16 = 0x10;
const BPF_JMP_JGT_CHIRHO: u16 = 0x20;
const BPF_JMP_JGE_CHIRHO: u16 = 0x30;
const BPF_JMP_JSET_CHIRHO: u16 = 0x40;

// MISC operations
const BPF_MISC_TAX_CHIRHO: u16 = 0x00;  // A -> X
const BPF_MISC_TXA_CHIRHO: u16 = 0x80;  // X -> A

// Convenience composed opcodes
pub const BPF_LD_W_ABS_CHIRHO: u16 = BPF_CLASS_LD_CHIRHO | BPF_SIZE_W_CHIRHO | BPF_MODE_ABS_CHIRHO; // 0x20
pub const BPF_JMP_JEQ_K_CHIRHO: u16 = BPF_CLASS_JMP_CHIRHO | BPF_JMP_JEQ_CHIRHO | BPF_SRC_K_CHIRHO; // 0x15
pub const BPF_JMP_JGE_K_CHIRHO: u16 = BPF_CLASS_JMP_CHIRHO | BPF_JMP_JGE_CHIRHO | BPF_SRC_K_CHIRHO; // 0x35
pub const BPF_JMP_JA_CHIRHO: u16 = BPF_CLASS_JMP_CHIRHO | BPF_JMP_OP_JA_CHIRHO; // 0x05
pub const BPF_RET_K_CHIRHO: u16 = BPF_CLASS_RET_CHIRHO | BPF_SRC_K_CHIRHO; // 0x06

/// `struct seccomp_data` offsets (the input to the BPF filter).
pub const SECCOMP_DATA_NR_CHIRHO: u32 = 0;
pub const SECCOMP_DATA_ARCH_CHIRHO: u32 = 4;
pub const SECCOMP_DATA_IP_CHIRHO: u32 = 8;
pub const SECCOMP_DATA_ARGS_CHIRHO: u32 = 16;

/// x86_64 audit architecture constant.
pub const AUDIT_ARCH_X86_64_CHIRHO: u32 = 0xC000_003E;

/// Size of `struct seccomp_data` (64 bytes).
const SECCOMP_DATA_SIZE_CHIRHO: usize = 64;

/// Number of BPF scratch memory slots (M[0..15]).
const BPF_MEMWORDS_CHIRHO: usize = 16;

// ============================================================================
// seccomp_data — the data structure BPF filters operate on
// ============================================================================

/// The `struct seccomp_data` that BPF filters evaluate against.
///
/// Layout (64 bytes):
/// ```text
///  0: nr          (u32) — syscall number
///  4: arch        (u32) — audit architecture
///  8: instruction_pointer (u64)
/// 16: args[0]     (u64)
/// 24: args[1]     (u64)
/// 32: args[2]     (u64)
/// 40: args[3]     (u64)
/// 48: args[4]     (u64)
/// 56: args[5]     (u64)
/// ```
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct SeccompDataChirho {
    /// Syscall number.
    pub nr_chirho: u32,
    /// Audit architecture (e.g. AUDIT_ARCH_X86_64).
    pub arch_chirho: u32,
    /// Instruction pointer at time of syscall.
    pub instruction_pointer_chirho: u64,
    /// Syscall arguments.
    pub args_chirho: [u64; 6],
}

impl SeccompDataChirho {
    /// Read a byte at the given offset from this structure, treating it
    /// as a raw byte array.
    fn read_byte_chirho(&self, offset_chirho: u32) -> Option<u8> {
        if (offset_chirho as usize) >= SECCOMP_DATA_SIZE_CHIRHO {
            return None;
        }
        let ptr_chirho = self as *const Self as *const u8;
        Some(unsafe { *ptr_chirho.add(offset_chirho as usize) })
    }

    /// Read a 16-bit halfword at the given offset (little-endian).
    fn read_half_chirho(&self, offset_chirho: u32) -> Option<u16> {
        if (offset_chirho as usize + 2) > SECCOMP_DATA_SIZE_CHIRHO {
            return None;
        }
        let ptr_chirho = self as *const Self as *const u8;
        let val_chirho = unsafe {
            let p_chirho = ptr_chirho.add(offset_chirho as usize);
            (*(p_chirho as *const u16)).to_le()
        };
        Some(val_chirho)
    }

    /// Read a 32-bit word at the given offset (little-endian).
    fn read_word_chirho(&self, offset_chirho: u32) -> Option<u32> {
        if (offset_chirho as usize + 4) > SECCOMP_DATA_SIZE_CHIRHO {
            return None;
        }
        let ptr_chirho = self as *const Self as *const u8;
        let val_chirho = unsafe {
            let p_chirho = ptr_chirho.add(offset_chirho as usize);
            (*(p_chirho as *const u32)).to_le()
        };
        Some(val_chirho)
    }
}

// ============================================================================
// Seccomp mode
// ============================================================================

/// Seccomp mode for a process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeccompModeChirho {
    /// No seccomp filtering.
    DisabledChirho,
    /// Strict mode: only read, write, exit, sigreturn allowed.
    StrictChirho,
    /// Filter mode: BPF programs evaluate each syscall.
    FilterChirho,
}

// ============================================================================
// Per-process seccomp state
// ============================================================================

/// Seccomp state for one process.
#[derive(Debug, Clone)]
pub struct SeccompStateChirho {
    /// Current mode.
    pub mode_chirho: SeccompModeChirho,
    /// BPF filter programs (evaluated in order; most restrictive wins).
    pub filters_chirho: Vec<Vec<BpfInsnChirho>>,
}

impl Default for SeccompStateChirho {
    fn default() -> Self {
        Self {
            mode_chirho: SeccompModeChirho::DisabledChirho,
            filters_chirho: Vec::new(),
        }
    }
}

// ============================================================================
// Global seccomp registry
// ============================================================================

static SECCOMP_STATE_CHIRHO: Mutex<BTreeMap<u64, SeccompStateChirho>> =
    Mutex::new(BTreeMap::new());

// ============================================================================
// F1-006: BPF Interpreter — evaluate classic BPF programs
// ============================================================================

/// Execute a single classic BPF filter program against the given seccomp_data.
///
/// Returns the BPF return value (a SECCOMP_RET_* action code).
/// On invalid instructions or out-of-bounds access, returns SECCOMP_RET_KILL.
fn run_bpf_filter_chirho(
    filter_chirho: &[BpfInsnChirho],
    data_chirho: &SeccompDataChirho,
) -> u32 {
    // BPF registers
    let mut a_chirho: u32 = 0;   // Accumulator
    let mut x_chirho: u32 = 0;   // Index register
    let mut pc_chirho: usize = 0; // Program counter

    // Scratch memory (M[0..15])
    let mut mem_chirho = [0u32; BPF_MEMWORDS_CHIRHO];

    // Safety limit: prevent infinite loops
    let max_insns_chirho: usize = 4096;
    let mut insn_count_chirho: usize = 0;

    while pc_chirho < filter_chirho.len() {
        insn_count_chirho += 1;
        if insn_count_chirho > max_insns_chirho {
            // Runaway program — kill the process
            return SECCOMP_RET_KILL_PROCESS_CHIRHO;
        }

        let insn_chirho = &filter_chirho[pc_chirho];
        let class_chirho = insn_chirho.code_chirho & 0x07;
        let size_chirho = insn_chirho.code_chirho & 0x18;
        let mode_chirho = insn_chirho.code_chirho & 0xE0;
        let op_chirho = insn_chirho.code_chirho & 0xF0;
        let src_chirho = insn_chirho.code_chirho & 0x08;

        match class_chirho {
            // ================================================================
            // LD — load into accumulator
            // ================================================================
            c_chirho if c_chirho == BPF_CLASS_LD_CHIRHO => {
                let val_chirho = match mode_chirho {
                    m_chirho if m_chirho == BPF_MODE_IMM_CHIRHO => {
                        // LD #k — load immediate
                        Some(insn_chirho.k_chirho)
                    }
                    m_chirho if m_chirho == BPF_MODE_ABS_CHIRHO => {
                        // LD [k] — load from seccomp_data at absolute offset
                        match size_chirho {
                            s_chirho if s_chirho == BPF_SIZE_W_CHIRHO => {
                                data_chirho.read_word_chirho(insn_chirho.k_chirho)
                            }
                            s_chirho if s_chirho == BPF_SIZE_H_CHIRHO => {
                                data_chirho.read_half_chirho(insn_chirho.k_chirho)
                                    .map(|v_chirho| v_chirho as u32)
                            }
                            s_chirho if s_chirho == BPF_SIZE_B_CHIRHO => {
                                data_chirho.read_byte_chirho(insn_chirho.k_chirho)
                                    .map(|v_chirho| v_chirho as u32)
                            }
                            _ => None,
                        }
                    }
                    m_chirho if m_chirho == BPF_MODE_IND_CHIRHO => {
                        // LD [X+k] — indirect load
                        let off_chirho = x_chirho.wrapping_add(insn_chirho.k_chirho);
                        match size_chirho {
                            s_chirho if s_chirho == BPF_SIZE_W_CHIRHO => {
                                data_chirho.read_word_chirho(off_chirho)
                            }
                            s_chirho if s_chirho == BPF_SIZE_H_CHIRHO => {
                                data_chirho.read_half_chirho(off_chirho)
                                    .map(|v_chirho| v_chirho as u32)
                            }
                            s_chirho if s_chirho == BPF_SIZE_B_CHIRHO => {
                                data_chirho.read_byte_chirho(off_chirho)
                                    .map(|v_chirho| v_chirho as u32)
                            }
                            _ => None,
                        }
                    }
                    m_chirho if m_chirho == BPF_MODE_MEM_CHIRHO => {
                        // LD M[k] — load from scratch memory
                        let idx_chirho = insn_chirho.k_chirho as usize;
                        if idx_chirho < BPF_MEMWORDS_CHIRHO {
                            Some(mem_chirho[idx_chirho])
                        } else {
                            None
                        }
                    }
                    _ => None,
                };
                match val_chirho {
                    Some(v_chirho) => a_chirho = v_chirho,
                    None => return SECCOMP_RET_KILL_PROCESS_CHIRHO,
                }
                pc_chirho += 1;
            }

            // ================================================================
            // LDX — load into index register
            // ================================================================
            c_chirho if c_chirho == BPF_CLASS_LDX_CHIRHO => {
                let val_chirho = match mode_chirho {
                    m_chirho if m_chirho == BPF_MODE_IMM_CHIRHO => {
                        Some(insn_chirho.k_chirho)
                    }
                    m_chirho if m_chirho == BPF_MODE_MEM_CHIRHO => {
                        let idx_chirho = insn_chirho.k_chirho as usize;
                        if idx_chirho < BPF_MEMWORDS_CHIRHO {
                            Some(mem_chirho[idx_chirho])
                        } else {
                            None
                        }
                    }
                    _ => None,
                };
                match val_chirho {
                    Some(v_chirho) => x_chirho = v_chirho,
                    None => return SECCOMP_RET_KILL_PROCESS_CHIRHO,
                }
                pc_chirho += 1;
            }

            // ================================================================
            // ST — store accumulator to scratch memory
            // ================================================================
            c_chirho if c_chirho == BPF_CLASS_ST_CHIRHO => {
                let idx_chirho = insn_chirho.k_chirho as usize;
                if idx_chirho < BPF_MEMWORDS_CHIRHO {
                    mem_chirho[idx_chirho] = a_chirho;
                } else {
                    return SECCOMP_RET_KILL_PROCESS_CHIRHO;
                }
                pc_chirho += 1;
            }

            // ================================================================
            // STX — store index register to scratch memory
            // ================================================================
            c_chirho if c_chirho == BPF_CLASS_STX_CHIRHO => {
                let idx_chirho = insn_chirho.k_chirho as usize;
                if idx_chirho < BPF_MEMWORDS_CHIRHO {
                    mem_chirho[idx_chirho] = x_chirho;
                } else {
                    return SECCOMP_RET_KILL_PROCESS_CHIRHO;
                }
                pc_chirho += 1;
            }

            // ================================================================
            // ALU — arithmetic/logic operations
            // ================================================================
            c_chirho if c_chirho == BPF_CLASS_ALU_CHIRHO => {
                let operand_chirho = if src_chirho == BPF_SRC_X_CHIRHO {
                    x_chirho
                } else {
                    insn_chirho.k_chirho
                };

                match op_chirho {
                    o_chirho if o_chirho == BPF_ALU_ADD_CHIRHO => {
                        a_chirho = a_chirho.wrapping_add(operand_chirho);
                    }
                    o_chirho if o_chirho == BPF_ALU_SUB_CHIRHO => {
                        a_chirho = a_chirho.wrapping_sub(operand_chirho);
                    }
                    o_chirho if o_chirho == BPF_ALU_MUL_CHIRHO => {
                        a_chirho = a_chirho.wrapping_mul(operand_chirho);
                    }
                    o_chirho if o_chirho == BPF_ALU_DIV_CHIRHO => {
                        if operand_chirho == 0 {
                            return SECCOMP_RET_KILL_PROCESS_CHIRHO;
                        }
                        a_chirho /= operand_chirho;
                    }
                    o_chirho if o_chirho == BPF_ALU_OR_CHIRHO => {
                        a_chirho |= operand_chirho;
                    }
                    o_chirho if o_chirho == BPF_ALU_AND_CHIRHO => {
                        a_chirho &= operand_chirho;
                    }
                    o_chirho if o_chirho == BPF_ALU_LSH_CHIRHO => {
                        a_chirho = a_chirho.wrapping_shl(operand_chirho);
                    }
                    o_chirho if o_chirho == BPF_ALU_RSH_CHIRHO => {
                        a_chirho = a_chirho.wrapping_shr(operand_chirho);
                    }
                    o_chirho if o_chirho == BPF_ALU_NEG_CHIRHO => {
                        a_chirho = (-(a_chirho as i32)) as u32;
                    }
                    o_chirho if o_chirho == BPF_ALU_XOR_CHIRHO => {
                        a_chirho ^= operand_chirho;
                    }
                    _ => {
                        return SECCOMP_RET_KILL_PROCESS_CHIRHO;
                    }
                }
                pc_chirho += 1;
            }

            // ================================================================
            // JMP — conditional and unconditional jumps
            // ================================================================
            c_chirho if c_chirho == BPF_CLASS_JMP_CHIRHO => {
                let operand_chirho = if src_chirho == BPF_SRC_X_CHIRHO {
                    x_chirho
                } else {
                    insn_chirho.k_chirho
                };

                match op_chirho {
                    o_chirho if o_chirho == BPF_JMP_JA_CHIRHO => {
                        // Unconditional jump
                        pc_chirho += 1 + insn_chirho.k_chirho as usize;
                    }
                    o_chirho if o_chirho == BPF_JMP_JEQ_CHIRHO => {
                        if a_chirho == operand_chirho {
                            pc_chirho += 1 + insn_chirho.jt_chirho as usize;
                        } else {
                            pc_chirho += 1 + insn_chirho.jf_chirho as usize;
                        }
                    }
                    o_chirho if o_chirho == BPF_JMP_JGT_CHIRHO => {
                        if a_chirho > operand_chirho {
                            pc_chirho += 1 + insn_chirho.jt_chirho as usize;
                        } else {
                            pc_chirho += 1 + insn_chirho.jf_chirho as usize;
                        }
                    }
                    o_chirho if o_chirho == BPF_JMP_JGE_CHIRHO => {
                        if a_chirho >= operand_chirho {
                            pc_chirho += 1 + insn_chirho.jt_chirho as usize;
                        } else {
                            pc_chirho += 1 + insn_chirho.jf_chirho as usize;
                        }
                    }
                    o_chirho if o_chirho == BPF_JMP_JSET_CHIRHO => {
                        if (a_chirho & operand_chirho) != 0 {
                            pc_chirho += 1 + insn_chirho.jt_chirho as usize;
                        } else {
                            pc_chirho += 1 + insn_chirho.jf_chirho as usize;
                        }
                    }
                    _ => {
                        return SECCOMP_RET_KILL_PROCESS_CHIRHO;
                    }
                }
            }

            // ================================================================
            // RET — return a value
            // ================================================================
            c_chirho if c_chirho == BPF_CLASS_RET_CHIRHO => {
                if src_chirho == BPF_SRC_K_CHIRHO {
                    return insn_chirho.k_chirho;
                } else {
                    return a_chirho;
                }
            }

            // ================================================================
            // MISC — TAX (A->X) and TXA (X->A)
            // ================================================================
            c_chirho if c_chirho == BPF_CLASS_MISC_CHIRHO => {
                let misc_op_chirho = insn_chirho.code_chirho & 0xF8;
                if misc_op_chirho == BPF_MISC_TAX_CHIRHO {
                    x_chirho = a_chirho;
                } else if misc_op_chirho == BPF_MISC_TXA_CHIRHO {
                    a_chirho = x_chirho;
                } else {
                    return SECCOMP_RET_KILL_PROCESS_CHIRHO;
                }
                pc_chirho += 1;
            }

            // Unknown instruction class
            _ => {
                return SECCOMP_RET_KILL_PROCESS_CHIRHO;
            }
        }
    }

    // Fell off the end without a RET — kill
    SECCOMP_RET_KILL_PROCESS_CHIRHO
}

// ============================================================================
// Syscall implementation
// ============================================================================

/// `seccomp(2)` syscall.
///
/// # Arguments
/// * `op_chirho` — SECCOMP_SET_MODE_STRICT, SECCOMP_SET_MODE_FILTER, etc.
/// * `flags_chirho` — filter flags
/// * `args_ptr_chirho` — pointer to `struct sock_fprog` (for FILTER mode)
///
/// # Returns
/// 0 on success, or negative errno.
pub fn sys_seccomp_chirho(op_chirho: u64, flags_chirho: u64, args_ptr_chirho: u64) -> i64 {
    let pid_chirho = crate::scheduler_chirho::current_pid_chirho().unwrap_or(1);

    match op_chirho as u32 {
        SECCOMP_SET_MODE_STRICT_CHIRHO => {
            let mut states_chirho = SECCOMP_STATE_CHIRHO.lock();
            let state_chirho = states_chirho
                .entry(pid_chirho)
                .or_insert_with(SeccompStateChirho::default);

            if state_chirho.mode_chirho != SeccompModeChirho::DisabledChirho {
                return -(crate::syscall_chirho::EINVAL_CHIRHO);
            }

            state_chirho.mode_chirho = SeccompModeChirho::StrictChirho;
            crate::serial_println_chirho!(
                "[SECCOMP] PID {} entered strict mode",
                pid_chirho
            );
            0
        }
        SECCOMP_SET_MODE_FILTER_CHIRHO => {
            if args_ptr_chirho == 0 {
                return -(crate::syscall_chirho::EFAULT_CHIRHO);
            }

            // Read sock_fprog: { u16 len; BpfInsn* filter }
            let len_chirho = unsafe {
                core::ptr::read_unaligned(args_ptr_chirho as *const u16)
            } as usize;
            let filter_ptr_chirho = unsafe {
                core::ptr::read_unaligned((args_ptr_chirho + 8) as *const u64)
            };

            if len_chirho == 0 || len_chirho > 4096 || filter_ptr_chirho == 0 {
                return -(crate::syscall_chirho::EINVAL_CHIRHO);
            }

            // Read the BPF instructions
            let mut filter_chirho = Vec::with_capacity(len_chirho);
            for i_chirho in 0..len_chirho {
                let insn_chirho = unsafe {
                    core::ptr::read_unaligned(
                        (filter_ptr_chirho as *const BpfInsnChirho).add(i_chirho),
                    )
                };
                filter_chirho.push(insn_chirho);
            }

            let mut states_chirho = SECCOMP_STATE_CHIRHO.lock();
            let state_chirho = states_chirho
                .entry(pid_chirho)
                .or_insert_with(SeccompStateChirho::default);

            state_chirho.mode_chirho = SeccompModeChirho::FilterChirho;
            state_chirho.filters_chirho.push(filter_chirho);

            crate::serial_println_chirho!(
                "[SECCOMP] PID {} installed BPF filter ({} insns, flags={:#x})",
                pid_chirho,
                len_chirho,
                flags_chirho,
            );
            0
        }
        SECCOMP_GET_ACTION_AVAIL_CHIRHO => {
            0 // We support all actions
        }
        _ => {
            crate::serial_println_chirho!(
                "[SECCOMP] Unknown op {} for PID {}",
                op_chirho,
                pid_chirho,
            );
            -(crate::syscall_chirho::EINVAL_CHIRHO)
        }
    }
}

/// Check if a syscall is allowed under the current process's seccomp policy.
///
/// Called at syscall entry time. Returns `SECCOMP_RET_ALLOW` if the syscall
/// should proceed, or another action code.
///
/// **F1-006**: Now runs the full BPF interpreter for filter mode instead of
/// always allowing.
pub fn check_seccomp_chirho(pid_chirho: u64, syscall_nr_chirho: u64) -> u32 {
    let states_chirho = SECCOMP_STATE_CHIRHO.lock();
    let state_chirho = match states_chirho.get(&pid_chirho) {
        Some(s_chirho) => s_chirho,
        None => return SECCOMP_RET_ALLOW_CHIRHO,
    };

    match state_chirho.mode_chirho {
        SeccompModeChirho::DisabledChirho => SECCOMP_RET_ALLOW_CHIRHO,
        SeccompModeChirho::StrictChirho => {
            // Strict mode: only allow read(0), write(1), exit(60),
            // rt_sigreturn(15), exit_group(231)
            match syscall_nr_chirho {
                0 | 1 | 60 | 15 | 231 => SECCOMP_RET_ALLOW_CHIRHO,
                _ => SECCOMP_RET_KILL_PROCESS_CHIRHO,
            }
        }
        SeccompModeChirho::FilterChirho => {
            // Build the seccomp_data structure for the BPF programs
            let data_chirho = SeccompDataChirho {
                nr_chirho: syscall_nr_chirho as u32,
                arch_chirho: AUDIT_ARCH_X86_64_CHIRHO,
                instruction_pointer_chirho: 0, // Not available at this point
                args_chirho: [0; 6],            // Args not passed to check yet
            };

            // Run all filters; the most restrictive result wins.
            // Lower action values are more restrictive.
            let mut result_chirho = SECCOMP_RET_ALLOW_CHIRHO;

            for filter_chirho in &state_chirho.filters_chirho {
                let action_chirho = run_bpf_filter_chirho(filter_chirho, &data_chirho);

                // The action with the lowest value wins (most restrictive)
                if action_chirho < result_chirho {
                    result_chirho = action_chirho;
                }
            }

            if (result_chirho & SECCOMP_RET_ACTION_MASK_CHIRHO) != SECCOMP_RET_ALLOW_CHIRHO {
                crate::serial_println_chirho!(
                    "[SECCOMP] PID {} syscall {} => action {:#x}",
                    pid_chirho,
                    syscall_nr_chirho,
                    result_chirho
                );
            }

            result_chirho
        }
    }
}

/// Extended check that passes full syscall arguments to the BPF filters.
///
/// This version is used when the caller has access to all six arguments.
pub fn check_seccomp_with_args_chirho(
    pid_chirho: u64,
    syscall_nr_chirho: u64,
    args_chirho: &[u64; 6],
) -> u32 {
    let states_chirho = SECCOMP_STATE_CHIRHO.lock();
    let state_chirho = match states_chirho.get(&pid_chirho) {
        Some(s_chirho) => s_chirho,
        None => return SECCOMP_RET_ALLOW_CHIRHO,
    };

    match state_chirho.mode_chirho {
        SeccompModeChirho::DisabledChirho => SECCOMP_RET_ALLOW_CHIRHO,
        SeccompModeChirho::StrictChirho => {
            match syscall_nr_chirho {
                0 | 1 | 60 | 15 | 231 => SECCOMP_RET_ALLOW_CHIRHO,
                _ => SECCOMP_RET_KILL_PROCESS_CHIRHO,
            }
        }
        SeccompModeChirho::FilterChirho => {
            let data_chirho = SeccompDataChirho {
                nr_chirho: syscall_nr_chirho as u32,
                arch_chirho: AUDIT_ARCH_X86_64_CHIRHO,
                instruction_pointer_chirho: 0,
                args_chirho: *args_chirho,
            };

            let mut result_chirho = SECCOMP_RET_ALLOW_CHIRHO;
            for filter_chirho in &state_chirho.filters_chirho {
                let action_chirho = run_bpf_filter_chirho(filter_chirho, &data_chirho);
                if action_chirho < result_chirho {
                    result_chirho = action_chirho;
                }
            }

            result_chirho
        }
    }
}
