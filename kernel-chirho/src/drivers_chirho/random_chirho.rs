// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! Kernel entropy pool and /dev/random implementation for the Lineluya
//! kernel (A5 subsystem).
//!
//! Provides:
//! - A simple entropy pool seeded from RDRAND/RDSEED instructions
//! - `getrandom(2)` syscall implementation
//! - Entropy mixing via xorshift128+ PRNG
//! - Interrupt timing jitter collection
//!
//! This is not cryptographically secure — a real implementation would
//! use ChaCha20 or BLAKE2 as the CSPRNG.

use core::sync::atomic::{AtomicU64, AtomicBool, Ordering};
use spin::Mutex;

// ============================================================================
// Entropy pool
// ============================================================================

/// Size of the entropy pool in u64 words.
const POOL_SIZE_CHIRHO: usize = 32;

/// The kernel entropy pool.
struct EntropyPoolChirho {
    /// Pool state (xorshift128+ state words).
    state_chirho: [u64; POOL_SIZE_CHIRHO],
    /// Current mix index.
    mix_index_chirho: usize,
    /// Total entropy bits estimated.
    entropy_bits_chirho: u32,
}

impl EntropyPoolChirho {
    const fn new_chirho() -> Self {
        Self {
            state_chirho: [0; POOL_SIZE_CHIRHO],
            mix_index_chirho: 0,
            entropy_bits_chirho: 0,
        }
    }
}

/// Global entropy pool.
static ENTROPY_POOL_CHIRHO: Mutex<EntropyPoolChirho> =
    Mutex::new(EntropyPoolChirho::new_chirho());

/// Whether the pool has been seeded with hardware randomness.
static POOL_SEEDED_CHIRHO: AtomicBool = AtomicBool::new(false);

/// Interrupt timing counter for jitter entropy.
static INTERRUPT_COUNTER_CHIRHO: AtomicU64 = AtomicU64::new(0);

// ============================================================================
// Hardware random number generation (RDRAND / RDSEED)
// ============================================================================

/// Try to get a 64-bit random value from RDRAND.
///
/// Returns `None` if RDRAND is not available or fails.
#[allow(dead_code)]
fn rdrand64_chirho() -> Option<u64> {
    let mut val_chirho: u64;
    let success_chirho: u8;

    unsafe {
        core::arch::asm!(
            "rdrand {val}",
            "setc {ok}",
            val = out(reg) val_chirho,
            ok = out(reg_byte) success_chirho,
            options(nostack),
        );
    }

    if success_chirho != 0 {
        Some(val_chirho)
    } else {
        None
    }
}

/// Try to get a 64-bit random value from RDSEED.
///
/// Returns `None` if RDSEED is not available or fails.
#[allow(dead_code)]
fn rdseed64_chirho() -> Option<u64> {
    let mut val_chirho: u64;
    let success_chirho: u8;

    unsafe {
        core::arch::asm!(
            "rdseed {val}",
            "setc {ok}",
            val = out(reg) val_chirho,
            ok = out(reg_byte) success_chirho,
            options(nostack),
        );
    }

    if success_chirho != 0 {
        Some(val_chirho)
    } else {
        None
    }
}

/// Read the TSC (Time Stamp Counter) for timing jitter.
#[allow(dead_code)]
fn rdtsc_chirho() -> u64 {
    let lo_chirho: u32;
    let hi_chirho: u32;
    unsafe {
        core::arch::asm!(
            "rdtsc",
            out("eax") lo_chirho,
            out("edx") hi_chirho,
            options(nostack, nomem),
        );
    }
    ((hi_chirho as u64) << 32) | (lo_chirho as u64)
}

// ============================================================================
// Entropy mixing
// ============================================================================

/// Mix a 64-bit value into the entropy pool.
#[allow(dead_code)]
fn mix_entropy_chirho(value_chirho: u64) {
    let mut pool_chirho = ENTROPY_POOL_CHIRHO.lock();
    let idx_chirho = pool_chirho.mix_index_chirho % POOL_SIZE_CHIRHO;

    // xorshift mix
    let mut s_chirho = pool_chirho.state_chirho[idx_chirho] ^ value_chirho;
    s_chirho ^= s_chirho << 13;
    s_chirho ^= s_chirho >> 7;
    s_chirho ^= s_chirho << 17;
    pool_chirho.state_chirho[idx_chirho] = s_chirho;

    pool_chirho.mix_index_chirho = pool_chirho.mix_index_chirho.wrapping_add(1);
    pool_chirho.entropy_bits_chirho = pool_chirho.entropy_bits_chirho.saturating_add(1);
}

/// Extract a 64-bit random value from the pool.
#[allow(dead_code)]
fn extract_u64_chirho() -> u64 {
    let mut pool_chirho = ENTROPY_POOL_CHIRHO.lock();

    // xorshift128+ over two pool words
    let idx_a_chirho = pool_chirho.mix_index_chirho % POOL_SIZE_CHIRHO;
    let idx_b_chirho = (pool_chirho.mix_index_chirho + 1) % POOL_SIZE_CHIRHO;

    let mut s1_chirho = pool_chirho.state_chirho[idx_a_chirho];
    let s0_chirho = pool_chirho.state_chirho[idx_b_chirho];
    let result_chirho = s0_chirho.wrapping_add(s1_chirho);

    pool_chirho.state_chirho[idx_a_chirho] = s0_chirho;
    s1_chirho ^= s1_chirho << 23;
    s1_chirho ^= s1_chirho >> 17;
    s1_chirho ^= s0_chirho;
    s1_chirho ^= s0_chirho >> 26;
    pool_chirho.state_chirho[idx_b_chirho] = s1_chirho;

    pool_chirho.mix_index_chirho = pool_chirho.mix_index_chirho.wrapping_add(2);

    result_chirho
}

// ============================================================================
// Initialization
// ============================================================================

/// Initialize the entropy pool with hardware randomness.
///
/// Tries RDSEED first, falls back to RDRAND, then TSC.
#[allow(dead_code)]
pub fn init_random_chirho() {
    let mut seeded_count_chirho = 0u32;

    // Try RDSEED/RDRAND to seed the pool
    for _ in 0..POOL_SIZE_CHIRHO {
        if let Some(val_chirho) = rdseed64_chirho() {
            mix_entropy_chirho(val_chirho);
            seeded_count_chirho += 1;
        } else if let Some(val_chirho) = rdrand64_chirho() {
            mix_entropy_chirho(val_chirho);
            seeded_count_chirho += 1;
        }
    }

    // Always mix in TSC for additional entropy
    mix_entropy_chirho(rdtsc_chirho());

    if seeded_count_chirho > 0 {
        POOL_SEEDED_CHIRHO.store(true, Ordering::Relaxed);
        crate::serial_debug_chirho!(
            "[RANDOM] Entropy pool seeded with {} hardware random values",
            seeded_count_chirho
        );
    } else {
        crate::serial_debug_chirho!(
            "[RANDOM] WARNING: No hardware RNG available, using TSC-only entropy"
        );
        // Seed with multiple TSC reads
        for _ in 0..POOL_SIZE_CHIRHO {
            mix_entropy_chirho(rdtsc_chirho());
            // Spin briefly to get different TSC values
            for _ in 0..100 {
                core::hint::spin_loop();
            }
        }
        POOL_SEEDED_CHIRHO.store(true, Ordering::Relaxed);
    }
}

/// Called from interrupt handlers to collect timing jitter.
#[allow(dead_code)]
pub fn add_interrupt_entropy_chirho() {
    let count_chirho = INTERRUPT_COUNTER_CHIRHO.fetch_add(1, Ordering::Relaxed);
    // Mix every 64th interrupt to avoid lock contention
    if count_chirho & 63 == 0 {
        mix_entropy_chirho(rdtsc_chirho());
    }
}

// ============================================================================
// getrandom(2) syscall
// ============================================================================

/// `getrandom(2)` flags.
#[allow(dead_code)]
pub const GRND_NONBLOCK_CHIRHO: u32 = 0x01;
#[allow(dead_code)]
pub const GRND_RANDOM_CHIRHO: u32 = 0x02;
#[allow(dead_code)]
pub const GRND_INSECURE_CHIRHO: u32 = 0x04;

/// Implement `getrandom(2)`: fill a userspace buffer with random bytes.
///
/// # Arguments
/// * `buf_addr_chirho` — userspace destination buffer address
/// * `buflen_chirho` — number of bytes requested
/// * `flags_chirho` — GRND_NONBLOCK, GRND_RANDOM, etc.
///
/// # Returns
/// Number of bytes written, or negative errno.
#[allow(dead_code)]
pub fn sys_getrandom_chirho(
    buf_addr_chirho: u64,
    buflen_chirho: u64,
    _flags_chirho: u64,
) -> i64 {
    if buf_addr_chirho == 0 {
        return -(crate::syscall_chirho::EFAULT_CHIRHO);
    }

    let len_chirho = buflen_chirho as usize;
    if len_chirho == 0 {
        return 0;
    }

    // Cap at 256 bytes per call to avoid spending too long
    let actual_len_chirho = if len_chirho > 256 { 256 } else { len_chirho };

    let dest_chirho = buf_addr_chirho as *mut u8;

    let mut written_chirho = 0usize;
    while written_chirho < actual_len_chirho {
        let rand_val_chirho = extract_u64_chirho();
        let bytes_chirho = rand_val_chirho.to_ne_bytes();
        let remaining_chirho = actual_len_chirho - written_chirho;
        let chunk_chirho = if remaining_chirho >= 8 { 8 } else { remaining_chirho };

        for i_chirho in 0..chunk_chirho {
            unsafe {
                core::ptr::write_volatile(
                    dest_chirho.add(written_chirho + i_chirho),
                    bytes_chirho[i_chirho],
                );
            }
        }

        written_chirho += chunk_chirho;
    }

    actual_len_chirho as i64
}
