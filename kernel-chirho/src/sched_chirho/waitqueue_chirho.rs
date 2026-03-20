// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! Wait queue infrastructure for the Lineluya kernel.
//!
//! Wait queues allow tasks to sleep until a particular condition becomes true.
//! When another part of the kernel changes state (e.g., data becomes available
//! on a pipe, a child exits), it calls [`wake_up_chirho`] or
//! [`wake_up_one_chirho`] to make sleeping tasks runnable again.
//!
//! This module provides:
//! - [`WaitQueueChirho`] -- a list of sleeping task PIDs.
//! - [`wait_event_chirho`] -- block the current task until a condition holds.
//! - [`wake_up_chirho`] -- wake all tasks on a queue.
//! - [`wake_up_one_chirho`] -- wake exactly one task.

extern crate alloc;

use alloc::vec::Vec;
use spin::Mutex;

// ---------------------------------------------------------------------------
// WaitQueueChirho
// ---------------------------------------------------------------------------

/// A queue of tasks waiting for some event.
///
/// Tasks that call [`wait_event_chirho`] are added to the queue and removed
/// from the scheduler's run queue.  When the awaited condition becomes true,
/// [`wake_up_chirho`] or [`wake_up_one_chirho`] moves them back into the run
/// queue.
pub struct WaitQueueChirho {
    /// PIDs of tasks currently sleeping on this queue.
    waiters_chirho: Mutex<Vec<u64>>,
}

impl WaitQueueChirho {
    /// Create a new, empty wait queue.
    pub const fn new_chirho() -> Self {
        Self {
            waiters_chirho: Mutex::new(Vec::new()),
        }
    }

    /// Add a task PID to the wait queue (internal helper).
    fn add_waiter_chirho(&self, pid_chirho: u64) {
        let mut waiters_chirho = self.waiters_chirho.lock();
        if !waiters_chirho.contains(&pid_chirho) {
            waiters_chirho.push(pid_chirho);
        }
    }

    /// Remove a task PID from the wait queue (internal helper).
    fn remove_waiter_chirho(&self, pid_chirho: u64) {
        let mut waiters_chirho = self.waiters_chirho.lock();
        if let Some(pos_chirho) = waiters_chirho
            .iter()
            .position(|&p_chirho| p_chirho == pid_chirho)
        {
            waiters_chirho.remove(pos_chirho);
        }
    }

    /// Return the number of tasks currently waiting.
    pub fn len_chirho(&self) -> usize {
        self.waiters_chirho.lock().len()
    }

    /// Return `true` if no tasks are waiting.
    pub fn is_empty_chirho(&self) -> bool {
        self.waiters_chirho.lock().is_empty()
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Block the current task on `queue_chirho` until `condition_chirho` returns
/// `true`.
///
/// The task is removed from the scheduler run queue and added to the wait
/// queue.  Each time it is woken up it re-evaluates the condition; if the
/// condition is still false it goes back to sleep (spurious wake-up safety).
///
/// # Note
///
/// The condition closure is evaluated with interrupts enabled.  The caller
/// must ensure that the state the closure inspects is protected by appropriate
/// synchronisation (e.g., a spinlock).
pub fn wait_event_chirho<F>(queue_chirho: &WaitQueueChirho, mut condition_chirho: F)
where
    F: FnMut() -> bool,
{
    // Fast path: condition already true.
    if condition_chirho() {
        return;
    }

    // Obtain the current task PID.
    let pid_chirho = match crate::scheduler_chirho::current_pid_chirho() {
        Some(p_chirho) => p_chirho,
        None => return, // No current task (idle context) -- cannot block.
    };

    // Add ourselves to the wait queue before blocking.
    queue_chirho.add_waiter_chirho(pid_chirho);

    loop {
        // GPT-directed fix: make the condition check + block ATOMIC
        // with respect to interrupts. Without this, a wake can fire
        // between the condition check (false) and block_current,
        // causing a lost-wake race where PID 2 sleeps forever even
        // though the accept_queue has data.
        //
        // Disable interrupts, check condition, and if still false,
        // take current_pid (blocking it) before re-enabling interrupts.
        let should_block_chirho = x86_64::instructions::interrupts::without_interrupts(|| {
            if condition_chirho() {
                false // Condition became true — don't block
            } else {
                true // Still false — need to block
            }
        });

        if !should_block_chirho {
            queue_chirho.remove_waiter_chirho(pid_chirho);
            return;
        }

        // Block via the scheduler. This removes us from the run queue and
        // context-switches to another task. We will resume here when
        // someone calls wake_up_chirho / wake_up_one_chirho.
        crate::scheduler_chirho::block_current_chirho();

        // We have been woken up. Loop back and re-evaluate the condition.
    }
}

/// Wake all tasks sleeping on `queue_chirho`.
///
/// Every task in the queue is moved back into the scheduler's run queue.
/// The queue is left empty afterward.
pub fn wake_up_chirho(queue_chirho: &WaitQueueChirho) {
    let pids_chirho: Vec<u64> = {
        let mut waiters_chirho = queue_chirho.waiters_chirho.lock();
        let pids_chirho = waiters_chirho.clone();
        waiters_chirho.clear();
        pids_chirho
    };

    for pid_chirho in pids_chirho {
        crate::scheduler_chirho::unblock_task_chirho(pid_chirho);
    }
}

/// Wake exactly one task sleeping on `queue_chirho`.
///
/// The first (oldest) waiter is removed from the queue and placed back into
/// the scheduler's run queue.  If the queue is empty this is a no-op.
pub fn wake_up_one_chirho(queue_chirho: &WaitQueueChirho) {
    let pid_chirho = {
        let mut waiters_chirho = queue_chirho.waiters_chirho.lock();
        if waiters_chirho.is_empty() {
            return;
        }
        waiters_chirho.remove(0)
    };

    crate::scheduler_chirho::unblock_task_chirho(pid_chirho);
}
