// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! Symmetric Multi-Processing (SMP) for the Lineluya kernel.
//!
//! Provides CPU information tracking, per-CPU run queues (A5-005),
//! and load balancing infrastructure for distributing tasks across cores.
//!
//! ## Phase milestones
//!
//! - **A5-004** (done): AP startup via INIT-SIPI-SIPI.
//! - **A5-005**: SMP scheduler with per-CPU run queues and load balancing.

extern crate alloc;

use alloc::collections::VecDeque;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::Mutex;

// ============================================================================
// Constants
// ============================================================================

/// Maximum number of CPUs supported.
pub const MAX_CPUS_CHIRHO: usize = 256;

// ============================================================================
// CPU bookkeeping
// ============================================================================

/// Number of CPUs detected (including the BSP).
pub static CPU_COUNT_CHIRHO: AtomicU32 = AtomicU32::new(1);

/// Describes a single logical CPU.
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub struct CpuInfoChirho {
    /// Kernel-assigned CPU index (0 = BSP).
    pub cpu_id_chirho: u32,
    /// Local APIC ID for this CPU.
    pub apic_id_chirho: u32,
    /// `true` if this is the Bootstrap Processor.
    pub is_bsp_chirho: bool,
    /// Whether this CPU is online and scheduling tasks.
    pub online_chirho: bool,
}

impl CpuInfoChirho {
    /// Create a new CPU info entry.
    #[allow(dead_code)]
    pub const fn new_chirho(cpu_id_chirho: u32, apic_id_chirho: u32, is_bsp_chirho: bool) -> Self {
        Self {
            cpu_id_chirho,
            apic_id_chirho,
            is_bsp_chirho,
            online_chirho: false,
        }
    }
}

/// Per-CPU information array.
static CPU_INFO_CHIRHO: Mutex<[CpuInfoChirho; MAX_CPUS_CHIRHO]> = Mutex::new(
    [CpuInfoChirho {
        cpu_id_chirho: 0,
        apic_id_chirho: 0,
        is_bsp_chirho: false,
        online_chirho: false,
    }; MAX_CPUS_CHIRHO],
);

// ============================================================================
// A5-005: Per-CPU Run Queues
// ============================================================================

/// A per-CPU run queue holding task PIDs waiting to be scheduled on this CPU.
pub struct PerCpuRunQueueChirho {
    /// FIFO queue of runnable task PIDs.
    queue_chirho: VecDeque<u64>,
    /// Number of tasks currently in this queue.
    len_chirho: usize,
    /// Total load metric (sum of task weights, for balancing).
    load_chirho: u64,
}

impl PerCpuRunQueueChirho {
    /// Create an empty per-CPU run queue.
    pub const fn new_chirho() -> Self {
        Self {
            queue_chirho: VecDeque::new(),
            len_chirho: 0,
            load_chirho: 0,
        }
    }

    /// Enqueue a task PID.
    pub fn enqueue_chirho(&mut self, pid_chirho: u64) {
        self.queue_chirho.push_back(pid_chirho);
        self.len_chirho += 1;
        self.load_chirho += 1; // Each task contributes weight 1 for now.
    }

    /// Dequeue the next task PID (FIFO order).
    pub fn dequeue_chirho(&mut self) -> Option<u64> {
        if let Some(pid_chirho) = self.queue_chirho.pop_front() {
            self.len_chirho -= 1;
            if self.load_chirho > 0 {
                self.load_chirho -= 1;
            }
            Some(pid_chirho)
        } else {
            None
        }
    }

    /// Return the number of tasks in this queue.
    pub fn len_chirho(&self) -> usize {
        self.len_chirho
    }

    /// Whether this run queue is empty.
    pub fn is_empty_chirho(&self) -> bool {
        self.len_chirho == 0
    }

    /// Get the current load metric.
    #[allow(dead_code)]
    pub fn load_chirho(&self) -> u64 {
        self.load_chirho
    }
}

/// Array of per-CPU run queues (one per possible CPU).
///
/// Protected by individual spinlocks so CPUs don't contend on a single lock.
static PER_CPU_RQ_CHIRHO: [Mutex<PerCpuRunQueueChirho>; MAX_CPUS_CHIRHO] = {
    // const initializer for the array.
    const INIT_CHIRHO: Mutex<PerCpuRunQueueChirho> =
        Mutex::new(PerCpuRunQueueChirho::new_chirho());
    [INIT_CHIRHO; MAX_CPUS_CHIRHO]
};

// ============================================================================
// SMP scheduler operations
// ============================================================================

/// Assign a task to the CPU with the lowest load (simple load balancing).
///
/// Returns the CPU index the task was assigned to.
#[allow(dead_code)]
pub fn enqueue_task_balanced_chirho(pid_chirho: u64) -> u32 {
    let num_cpus_chirho = CPU_COUNT_CHIRHO.load(Ordering::Relaxed) as usize;
    let num_cpus_chirho = if num_cpus_chirho == 0 { 1 } else { num_cpus_chirho };

    // Find the CPU with the fewest queued tasks.
    let mut min_load_chirho = u64::MAX;
    let mut target_cpu_chirho: usize = 0;

    for cpu_chirho in 0..num_cpus_chirho {
        let rq_chirho = PER_CPU_RQ_CHIRHO[cpu_chirho].lock();
        let load_val_chirho = rq_chirho.load_chirho;
        if load_val_chirho < min_load_chirho {
            min_load_chirho = load_val_chirho;
            target_cpu_chirho = cpu_chirho;
        }
    }

    PER_CPU_RQ_CHIRHO[target_cpu_chirho]
        .lock()
        .enqueue_chirho(pid_chirho);

    target_cpu_chirho as u32
}

/// Pick the next task for the given CPU from its per-CPU run queue.
#[allow(dead_code)]
pub fn pick_next_task_chirho(cpu_chirho: u32) -> Option<u64> {
    PER_CPU_RQ_CHIRHO[cpu_chirho as usize].lock().dequeue_chirho()
}

/// Attempt to steal a task from the busiest CPU to the given (idle) CPU.
///
/// This implements basic work-stealing for load balancing.
/// Returns the stolen PID, or `None` if no stealing opportunity exists.
#[allow(dead_code)]
pub fn try_steal_task_chirho(idle_cpu_chirho: u32) -> Option<u64> {
    let num_cpus_chirho = CPU_COUNT_CHIRHO.load(Ordering::Relaxed) as usize;

    // Find the busiest CPU (that isn't us).
    let mut max_load_chirho: u64 = 0;
    let mut busiest_cpu_chirho: Option<usize> = None;

    for cpu_chirho in 0..num_cpus_chirho {
        if cpu_chirho == idle_cpu_chirho as usize {
            continue;
        }
        let rq_chirho = PER_CPU_RQ_CHIRHO[cpu_chirho].lock();
        let load_val_chirho = rq_chirho.load_chirho;
        if load_val_chirho > max_load_chirho && load_val_chirho > 1 {
            max_load_chirho = load_val_chirho;
            busiest_cpu_chirho = Some(cpu_chirho);
        }
    }

    // Steal one task from the busiest CPU.
    if let Some(src_cpu_chirho) = busiest_cpu_chirho {
        let mut src_rq_chirho = PER_CPU_RQ_CHIRHO[src_cpu_chirho].lock();
        if let Some(pid_chirho) = src_rq_chirho.dequeue_chirho() {
            // Enqueue on the idle CPU.
            PER_CPU_RQ_CHIRHO[idle_cpu_chirho as usize]
                .lock()
                .enqueue_chirho(pid_chirho);
            return Some(pid_chirho);
        }
    }

    None
}

/// Get the run queue length for a given CPU.
#[allow(dead_code)]
pub fn rq_len_chirho(cpu_chirho: u32) -> usize {
    PER_CPU_RQ_CHIRHO[cpu_chirho as usize].lock().len_chirho()
}

// ============================================================================
// SMP initialisation
// ============================================================================

/// Initialise SMP subsystem.
///
/// Sets up CPU info for the BSP and marks it online.
/// A full implementation will parse the MADT, prepare trampoline code,
/// and send INIT/SIPI to each AP.
#[allow(dead_code)]
pub fn init_smp_chirho() {
    crate::serial_println_chirho!("SMP: initialising per-CPU run queues...");

    // Mark BSP as online.
    {
        let mut info_chirho = CPU_INFO_CHIRHO.lock();
        info_chirho[0] = CpuInfoChirho {
            cpu_id_chirho: 0,
            apic_id_chirho: 0,
            is_bsp_chirho: true,
            online_chirho: true,
        };
    }

    CPU_COUNT_CHIRHO.store(1, Ordering::SeqCst);
    crate::serial_println_chirho!(
        "SMP: BSP online, {} per-CPU run queues ready",
        MAX_CPUS_CHIRHO
    );
}

/// Register an Application Processor that has been started via INIT-SIPI.
#[allow(dead_code)]
pub fn register_ap_chirho(cpu_id_chirho: u32, apic_id_chirho: u32) {
    if (cpu_id_chirho as usize) >= MAX_CPUS_CHIRHO {
        return;
    }

    {
        let mut info_chirho = CPU_INFO_CHIRHO.lock();
        info_chirho[cpu_id_chirho as usize] = CpuInfoChirho {
            cpu_id_chirho,
            apic_id_chirho,
            is_bsp_chirho: false,
            online_chirho: true,
        };
    }

    let new_count_chirho = CPU_COUNT_CHIRHO.fetch_add(1, Ordering::SeqCst) + 1;
    crate::serial_println_chirho!(
        "SMP: AP {} online (APIC ID {}), total CPUs = {}",
        cpu_id_chirho,
        apic_id_chirho,
        new_count_chirho
    );
}

/// Return the kernel CPU ID of the calling processor.
///
/// Currently returns 0 (BSP). With full SMP support this would read
/// the local APIC ID and map it to a kernel CPU index.
#[allow(dead_code)]
pub fn cpu_id_chirho() -> u32 {
    0
}
