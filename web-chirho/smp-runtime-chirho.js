// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

/**
 * Lineluya SMP Runtime — Web Workers as Virtual CPUs (B5)
 *
 * This module implements symmetric multiprocessing on the browser
 * using Web Workers. Each Worker represents a virtual CPU core
 * that can execute WASM kernel code independently.
 *
 * Architecture:
 *   Main thread    → CPU 0 (scheduler, I/O, rendering)
 *   Web Worker 1   → CPU 1 (process execution)
 *   Web Worker 2   → CPU 2 (process execution)
 *   Web Worker N   → CPU N (process execution)
 *
 * SharedArrayBuffer → Shared kernel memory (process table, mutexes)
 * Atomics.wait/notify → futex-like synchronization
 * MessagePort → IPC between Workers (syscall forwarding)
 *
 * B5-001: Web Workers as virtual CPUs
 * B5-002: SharedArrayBuffer shared memory
 * B5-003: Atomics.wait/notify as futex
 * B5-004: Process-per-Worker scheduling
 * B5-005: Mutex and semaphore primitives
 * B5-006: Cooperative scheduling with yield points
 * B5-008: Message passing between Workers
 */

// ── Constants ───────────────────────────────────────────────────────────────

const MAX_CPUS_CHIRHO = 8;
const FUTEX_TABLE_SIZE_CHIRHO = 256; // Number of futex slots
const SHARED_MEM_SIZE_CHIRHO = 1024 * 1024; // 1MB shared memory

// Futex operations (Linux-compatible)
const FUTEX_WAIT_CHIRHO = 0;
const FUTEX_WAKE_CHIRHO = 1;
const FUTEX_PRIVATE_FLAG_CHIRHO = 128;

// CPU states
const CPU_STATE_IDLE_CHIRHO = 0;
const CPU_STATE_RUNNING_CHIRHO = 1;
const CPU_STATE_HALTED_CHIRHO = 2;

// ── Shared Memory Layout ────────────────────────────────────────────────────

/**
 * SharedArrayBuffer layout (B5-002):
 *   [0..31]       CPU status flags (4 bytes each, up to 8 CPUs)
 *   [32..1055]    Futex table (4 bytes × 256 slots)
 *   [1056..2079]  Process run queue (4 bytes × 256 PIDs)
 *   [2080..2083]  Run queue head index
 *   [2084..2087]  Run queue tail index
 *   [2088..2091]  Global PID counter
 *   [2092..2095]  Scheduler lock
 *   [4096..]      General shared data area
 */

const OFFSET_CPU_STATUS_CHIRHO = 0;
const OFFSET_FUTEX_TABLE_CHIRHO = 32;
const OFFSET_RUN_QUEUE_CHIRHO = 1056;
const OFFSET_RQ_HEAD_CHIRHO = 2080;
const OFFSET_RQ_TAIL_CHIRHO = 2084;
const OFFSET_PID_COUNTER_CHIRHO = 2088;
const OFFSET_SCHED_LOCK_CHIRHO = 2092;
const OFFSET_SHARED_DATA_CHIRHO = 4096;

// ── SMP Manager (Main Thread) ───────────────────────────────────────────────

class SmpManagerChirho {
  constructor() {
    /** @type {Worker[]} */
    this.workersChirho = [];
    /** @type {SharedArrayBuffer|null} */
    this.sharedMemChirho = null;
    /** @type {Int32Array|null} */
    this.sharedViewChirho = null;
    this.cpuCountChirho = 0;
    this.bootedChirho = false;
  }

  /**
   * Initialize the SMP subsystem with N virtual CPUs (B5-001).
   * @param {number} numCpusChirho Number of virtual CPUs (1-8)
   * @param {string} workerScriptChirho URL to the Worker script
   */
  async initChirho(numCpusChirho, workerScriptChirho) {
    this.cpuCountChirho = Math.min(numCpusChirho, MAX_CPUS_CHIRHO);

    // Allocate shared memory (B5-002)
    this.sharedMemChirho = new SharedArrayBuffer(SHARED_MEM_SIZE_CHIRHO);
    this.sharedViewChirho = new Int32Array(this.sharedMemChirho);

    // Initialize shared memory
    this.sharedViewChirho[OFFSET_PID_COUNTER_CHIRHO / 4] = 2; // PID 1 = init
    this.sharedViewChirho[OFFSET_SCHED_LOCK_CHIRHO / 4] = 0;
    this.sharedViewChirho[OFFSET_RQ_HEAD_CHIRHO / 4] = 0;
    this.sharedViewChirho[OFFSET_RQ_TAIL_CHIRHO / 4] = 0;

    // CPU 0 is the main thread (always RUNNING)
    this.sharedViewChirho[OFFSET_CPU_STATUS_CHIRHO / 4] = CPU_STATE_RUNNING_CHIRHO;

    // Spawn Worker threads for CPUs 1..N (B5-001)
    for (let iChirho = 1; iChirho < this.cpuCountChirho; iChirho++) {
      const workerChirho = new Worker(workerScriptChirho, { type: 'module' });

      // Send shared memory and CPU ID to the worker
      workerChirho.postMessage({
        type_chirho: 'init_chirho',
        cpuIdChirho: iChirho,
        sharedMemChirho: this.sharedMemChirho,
      });

      // Handle messages from worker
      workerChirho.addEventListener('message', (evChirho) => {
        this.handleWorkerMessageChirho(iChirho, evChirho.data);
      });

      workerChirho.addEventListener('error', (evChirho) => {
        console.error(`[SMP] CPU ${iChirho} error:`, evChirho);
        this.sharedViewChirho[(OFFSET_CPU_STATUS_CHIRHO / 4) + iChirho] = CPU_STATE_HALTED_CHIRHO;
      });

      this.workersChirho.push(workerChirho);
    }

    this.bootedChirho = true;
    console.log(`[SMP] Initialized ${this.cpuCountChirho} virtual CPUs - John 3:16`);
  }

  /**
   * Handle a message from a Worker CPU.
   */
  handleWorkerMessageChirho(cpuIdChirho, msgChirho) {
    switch (msgChirho.type_chirho) {
      case 'ready_chirho':
        console.log(`[SMP] CPU ${cpuIdChirho} online`);
        this.sharedViewChirho[(OFFSET_CPU_STATUS_CHIRHO / 4) + cpuIdChirho] = CPU_STATE_IDLE_CHIRHO;
        break;

      case 'syscall_chirho':
        // Forward syscall to main thread for I/O
        this.handleRemoteSyscallChirho(cpuIdChirho, msgChirho);
        break;

      case 'halted_chirho':
        this.sharedViewChirho[(OFFSET_CPU_STATUS_CHIRHO / 4) + cpuIdChirho] = CPU_STATE_HALTED_CHIRHO;
        break;
    }
  }

  /**
   * Handle a syscall forwarded from a Worker CPU.
   * I/O syscalls (write, read) must execute on the main thread.
   */
  handleRemoteSyscallChirho(cpuIdChirho, msgChirho) {
    const { nrChirho, argsChirho, requestIdChirho } = msgChirho;

    // Process the syscall on the main thread
    let resultChirho = -38; // ENOSYS default

    // Send result back to the worker
    this.workersChirho[cpuIdChirho - 1]?.postMessage({
      type_chirho: 'syscall_result_chirho',
      requestIdChirho,
      resultChirho,
    });
  }

  // ── Futex operations (B5-003) ─────────────────────────────────────────

  /**
   * Futex wait — block until the value at address changes (B5-003).
   * Maps to Atomics.wait() on SharedArrayBuffer.
   *
   * @param {number} addrChirho Address in shared memory (4-byte aligned)
   * @param {number} expectedChirho Expected value
   * @param {number} timeoutMsChirho Timeout in milliseconds (-1 = infinite)
   * @returns {number} 0 = woken, -EAGAIN = value changed, -ETIMEDOUT = timed out
   */
  futexWaitChirho(addrChirho, expectedChirho, timeoutMsChirho) {
    const indexChirho = addrChirho / 4;
    if (indexChirho < 0 || indexChirho >= this.sharedViewChirho.length) {
      return -14; // EFAULT
    }

    const resultChirho = Atomics.wait(
      this.sharedViewChirho,
      indexChirho,
      expectedChirho,
      timeoutMsChirho < 0 ? Infinity : timeoutMsChirho
    );

    switch (resultChirho) {
      case 'ok': return 0;           // Woken by futex_wake
      case 'not-equal': return -11;  // EAGAIN — value already changed
      case 'timed-out': return -110; // ETIMEDOUT
      default: return -22;           // EINVAL
    }
  }

  /**
   * Futex wake — wake up to N threads waiting on an address (B5-003).
   * Maps to Atomics.notify() on SharedArrayBuffer.
   *
   * @param {number} addrChirho Address in shared memory
   * @param {number} countChirho Maximum threads to wake
   * @returns {number} Number of threads woken
   */
  futexWakeChirho(addrChirho, countChirho) {
    const indexChirho = addrChirho / 4;
    if (indexChirho < 0 || indexChirho >= this.sharedViewChirho.length) {
      return -14; // EFAULT
    }
    return Atomics.notify(this.sharedViewChirho, indexChirho, countChirho);
  }

  // ── Mutex primitives (B5-005) ─────────────────────────────────────────

  /**
   * Acquire a spinlock (try-lock with Atomics.compareExchange).
   * @param {number} lockAddrChirho Address of the lock word
   * @returns {boolean} true if lock acquired
   */
  mutexTryLockChirho(lockAddrChirho) {
    const indexChirho = lockAddrChirho / 4;
    return Atomics.compareExchange(this.sharedViewChirho, indexChirho, 0, 1) === 0;
  }

  /**
   * Release a spinlock.
   * @param {number} lockAddrChirho Address of the lock word
   */
  mutexUnlockChirho(lockAddrChirho) {
    const indexChirho = lockAddrChirho / 4;
    Atomics.store(this.sharedViewChirho, indexChirho, 0);
    Atomics.notify(this.sharedViewChirho, indexChirho, 1);
  }

  /**
   * Acquire a mutex, blocking if necessary.
   * Uses futex wait for efficient blocking.
   * @param {number} lockAddrChirho Address of the lock word
   */
  mutexLockChirho(lockAddrChirho) {
    while (!this.mutexTryLockChirho(lockAddrChirho)) {
      this.futexWaitChirho(lockAddrChirho, 1, 1); // Wait briefly
    }
  }

  // ── Scheduler (B5-004/B5-006) ─────────────────────────────────────────

  /**
   * Enqueue a PID to the run queue.
   */
  enqueueProcessChirho(pidChirho) {
    this.mutexLockChirho(OFFSET_SCHED_LOCK_CHIRHO);
    const tailChirho = Atomics.load(this.sharedViewChirho, OFFSET_RQ_TAIL_CHIRHO / 4);
    const queueIndexChirho = OFFSET_RUN_QUEUE_CHIRHO / 4 + (tailChirho % 256);
    Atomics.store(this.sharedViewChirho, queueIndexChirho, pidChirho);
    Atomics.store(this.sharedViewChirho, OFFSET_RQ_TAIL_CHIRHO / 4, tailChirho + 1);
    this.mutexUnlockChirho(OFFSET_SCHED_LOCK_CHIRHO);
  }

  /**
   * Dequeue a PID from the run queue.
   * @returns {number} PID or -1 if empty
   */
  dequeueProcessChirho() {
    this.mutexLockChirho(OFFSET_SCHED_LOCK_CHIRHO);
    const headChirho = Atomics.load(this.sharedViewChirho, OFFSET_RQ_HEAD_CHIRHO / 4);
    const tailChirho = Atomics.load(this.sharedViewChirho, OFFSET_RQ_TAIL_CHIRHO / 4);
    if (headChirho >= tailChirho) {
      this.mutexUnlockChirho(OFFSET_SCHED_LOCK_CHIRHO);
      return -1;
    }
    const queueIndexChirho = OFFSET_RUN_QUEUE_CHIRHO / 4 + (headChirho % 256);
    const pidChirho = Atomics.load(this.sharedViewChirho, queueIndexChirho);
    Atomics.store(this.sharedViewChirho, OFFSET_RQ_HEAD_CHIRHO / 4, headChirho + 1);
    this.mutexUnlockChirho(OFFSET_SCHED_LOCK_CHIRHO);
    return pidChirho;
  }

  /**
   * Allocate a new PID (atomic increment).
   */
  allocPidChirho() {
    return Atomics.add(this.sharedViewChirho, OFFSET_PID_COUNTER_CHIRHO / 4, 1);
  }

  // ── Message passing (B5-008) ──────────────────────────────────────────

  /**
   * Send a message to a specific CPU.
   */
  sendToCpuChirho(cpuIdChirho, msgChirho) {
    if (cpuIdChirho === 0) {
      // Main thread handles directly
      this.handleWorkerMessageChirho(0, msgChirho);
    } else if (cpuIdChirho > 0 && cpuIdChirho <= this.workersChirho.length) {
      this.workersChirho[cpuIdChirho - 1].postMessage(msgChirho);
    }
  }

  /**
   * Broadcast a message to all CPUs.
   */
  broadcastChirho(msgChirho) {
    for (let iChirho = 0; iChirho < this.workersChirho.length; iChirho++) {
      this.workersChirho[iChirho].postMessage(msgChirho);
    }
  }

  /**
   * Get CPU status summary.
   */
  statusChirho() {
    const resultChirho = [];
    for (let iChirho = 0; iChirho < this.cpuCountChirho; iChirho++) {
      const stateChirho = this.sharedViewChirho[(OFFSET_CPU_STATUS_CHIRHO / 4) + iChirho];
      const stateNameChirho = ['IDLE', 'RUNNING', 'HALTED'][stateChirho] || 'UNKNOWN';
      resultChirho.push({ cpuChirho: iChirho, stateChirho: stateNameChirho });
    }
    return resultChirho;
  }

  /**
   * Shutdown all worker CPUs.
   */
  shutdownChirho() {
    for (const workerChirho of this.workersChirho) {
      workerChirho.postMessage({ type_chirho: 'shutdown_chirho' });
      workerChirho.terminate();
    }
    this.workersChirho = [];
    this.bootedChirho = false;
    console.log('[SMP] All CPUs halted');
  }
}

// ── Worker CPU Script (runs inside Web Worker) ──────────────────────────────

/**
 * This code runs inside each Web Worker (B5-001).
 * It receives a SharedArrayBuffer for shared memory,
 * loads the WASM kernel, and executes processes.
 */
const WORKER_SCRIPT_CHIRHO = `
// For God so loved the world - John 3:16
let cpuIdChirho = -1;
let sharedViewChirho = null;
let runningChirho = true;

self.addEventListener('message', (evChirho) => {
  const msgChirho = evChirho.data;

  switch (msgChirho.type_chirho) {
    case 'init_chirho':
      cpuIdChirho = msgChirho.cpuIdChirho;
      sharedViewChirho = new Int32Array(msgChirho.sharedMemChirho);
      self.postMessage({ type_chirho: 'ready_chirho' });
      break;

    case 'run_process_chirho':
      // Execute a process on this CPU
      runProcessChirho(msgChirho.pidChirho);
      break;

    case 'syscall_result_chirho':
      // Handle syscall result from main thread
      if (pendingResolveChirho) {
        pendingResolveChirho(msgChirho.resultChirho);
        pendingResolveChirho = null;
      }
      break;

    case 'shutdown_chirho':
      runningChirho = false;
      self.close();
      break;
  }
});

let pendingResolveChirho = null;

function runProcessChirho(pidChirho) {
  // Update CPU status
  const cpuStatusIdxChirho = ${OFFSET_CPU_STATUS_CHIRHO} / 4 + cpuIdChirho;
  Atomics.store(sharedViewChirho, cpuStatusIdxChirho, ${CPU_STATE_RUNNING_CHIRHO});

  // Process execution would happen here:
  // Load WASM module, set up imports, call process entry point

  // When done, go back to idle
  Atomics.store(sharedViewChirho, cpuStatusIdxChirho, ${CPU_STATE_IDLE_CHIRHO});
  self.postMessage({ type_chirho: 'process_done_chirho', pidChirho });
}

// Futex wait (B5-003) - available in Workers
function futexWaitWorkerChirho(addrChirho, expectedChirho, timeoutChirho) {
  return Atomics.wait(sharedViewChirho, addrChirho / 4, expectedChirho, timeoutChirho);
}

// Futex wake (B5-003)
function futexWakeWorkerChirho(addrChirho, countChirho) {
  return Atomics.notify(sharedViewChirho, addrChirho / 4, countChirho);
}
`;

// Export for use in runtime
if (typeof window !== 'undefined') {
  window.SmpManagerChirho = SmpManagerChirho;
}

export { SmpManagerChirho, WORKER_SCRIPT_CHIRHO };
