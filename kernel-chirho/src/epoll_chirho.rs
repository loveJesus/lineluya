// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! epoll — scalable I/O event notification for the Lineluya kernel.

extern crate alloc;

use alloc::vec::Vec;
use spin::Mutex;

// epoll events (Linux-compatible)
pub const EPOLLIN_CHIRHO: u32 = 0x001;
pub const EPOLLOUT_CHIRHO: u32 = 0x004;
pub const EPOLLERR_CHIRHO: u32 = 0x008;
pub const EPOLLHUP_CHIRHO: u32 = 0x010;
pub const EPOLLRDHUP_CHIRHO: u32 = 0x2000;
pub const EPOLLET_CHIRHO: u32 = 1 << 31;
pub const EPOLLONESHOT_CHIRHO: u32 = 1 << 30;

// epoll_ctl operations
pub const EPOLL_CTL_ADD_CHIRHO: i32 = 1;
pub const EPOLL_CTL_DEL_CHIRHO: i32 = 2;
pub const EPOLL_CTL_MOD_CHIRHO: i32 = 3;

/// An epoll interest entry.
#[derive(Debug, Clone)]
pub struct EpollEntryChirho {
    pub fd_chirho: i32,
    pub events_chirho: u32,
    pub data_chirho: u64,
}

/// An epoll event returned to userspace.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct EpollEventChirho {
    pub events_chirho: u32,
    pub data_chirho: u64,
}

/// An epoll instance.
pub struct EpollInstanceChirho {
    pub entries_chirho: Vec<EpollEntryChirho>,
}

impl EpollInstanceChirho {
    pub fn new_chirho() -> Self {
        Self { entries_chirho: Vec::new() }
    }

    pub fn ctl_chirho(&mut self, op_chirho: i32, fd_chirho: i32, events_chirho: u32, data_chirho: u64) -> i32 {
        match op_chirho {
            EPOLL_CTL_ADD_CHIRHO => {
                if self.entries_chirho.iter().any(|e_chirho| e_chirho.fd_chirho == fd_chirho) {
                    return -17; // EEXIST
                }
                self.entries_chirho.push(EpollEntryChirho { fd_chirho, events_chirho, data_chirho });
                0
            }
            EPOLL_CTL_DEL_CHIRHO => {
                let len_before_chirho = self.entries_chirho.len();
                self.entries_chirho.retain(|e_chirho| e_chirho.fd_chirho != fd_chirho);
                if self.entries_chirho.len() == len_before_chirho { -2 } else { 0 } // ENOENT
            }
            EPOLL_CTL_MOD_CHIRHO => {
                for e_chirho in &mut self.entries_chirho {
                    if e_chirho.fd_chirho == fd_chirho {
                        e_chirho.events_chirho = events_chirho;
                        e_chirho.data_chirho = data_chirho;
                        return 0;
                    }
                }
                -2 // ENOENT
            }
            _ => -22, // EINVAL
        }
    }

    /// Poll all registered fds (non-blocking check).
    /// Returns ready events. In a real kernel, this would check fd readiness.
    pub fn wait_chirho(&self, max_events_chirho: usize) -> Vec<EpollEventChirho> {
        let mut ready_chirho = Vec::new();
        for entry_chirho in &self.entries_chirho {
            if ready_chirho.len() >= max_events_chirho {
                break;
            }
            // Stub: report all fds as ready for EPOLLIN|EPOLLOUT
            let revents_chirho = entry_chirho.events_chirho & (EPOLLIN_CHIRHO | EPOLLOUT_CHIRHO);
            if revents_chirho != 0 {
                ready_chirho.push(EpollEventChirho {
                    events_chirho: revents_chirho,
                    data_chirho: entry_chirho.data_chirho,
                });
            }
        }
        ready_chirho
    }
}

/// Global epoll instances (indexed by epoll fd).
pub static EPOLL_INSTANCES_CHIRHO: Mutex<Vec<Option<EpollInstanceChirho>>> =
    Mutex::new(Vec::new());

/// Create a new epoll instance, return its fd.
pub fn sys_epoll_create_chirho() -> i32 {
    let mut instances_chirho = EPOLL_INSTANCES_CHIRHO.lock();
    let fd_chirho = instances_chirho.len() as i32 + 1000; // epoll fds start at 1000
    instances_chirho.push(Some(EpollInstanceChirho::new_chirho()));
    fd_chirho
}
