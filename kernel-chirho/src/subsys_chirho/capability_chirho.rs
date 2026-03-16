// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! POSIX capabilities for the Lineluya kernel.

// Capability constants (Linux-compatible)
pub const CAP_CHOWN_CHIRHO: u32 = 0;
pub const CAP_DAC_OVERRIDE_CHIRHO: u32 = 1;
pub const CAP_DAC_READ_SEARCH_CHIRHO: u32 = 2;
pub const CAP_FOWNER_CHIRHO: u32 = 3;
pub const CAP_FSETID_CHIRHO: u32 = 4;
pub const CAP_KILL_CHIRHO: u32 = 5;
pub const CAP_SETGID_CHIRHO: u32 = 6;
pub const CAP_SETUID_CHIRHO: u32 = 7;
pub const CAP_SETPCAP_CHIRHO: u32 = 8;
pub const CAP_NET_BIND_SERVICE_CHIRHO: u32 = 10;
pub const CAP_NET_BROADCAST_CHIRHO: u32 = 11;
pub const CAP_NET_ADMIN_CHIRHO: u32 = 12;
pub const CAP_NET_RAW_CHIRHO: u32 = 13;
pub const CAP_SYS_MODULE_CHIRHO: u32 = 16;
pub const CAP_SYS_RAWIO_CHIRHO: u32 = 17;
pub const CAP_SYS_CHROOT_CHIRHO: u32 = 18;
pub const CAP_SYS_PTRACE_CHIRHO: u32 = 19;
pub const CAP_SYS_ADMIN_CHIRHO: u32 = 21;
pub const CAP_SYS_BOOT_CHIRHO: u32 = 22;
pub const CAP_SYS_NICE_CHIRHO: u32 = 23;
pub const CAP_SYS_RESOURCE_CHIRHO: u32 = 24;
pub const CAP_MKNOD_CHIRHO: u32 = 27;
pub const CAP_LAST_CAP_CHIRHO: u32 = 40;

/// Per-process capability sets.
#[derive(Debug, Clone, Copy)]
pub struct CapabilitySetChirho {
    pub effective_chirho: u64,
    pub permitted_chirho: u64,
    pub inheritable_chirho: u64,
    pub bounding_chirho: u64,
    pub ambient_chirho: u64,
}

impl Default for CapabilitySetChirho {
    fn default() -> Self {
        // Root (UID 0) gets all capabilities by default
        let all_caps_chirho = (1u64 << (CAP_LAST_CAP_CHIRHO + 1)) - 1;
        Self {
            effective_chirho: all_caps_chirho,
            permitted_chirho: all_caps_chirho,
            inheritable_chirho: 0,
            bounding_chirho: all_caps_chirho,
            ambient_chirho: 0,
        }
    }
}

impl CapabilitySetChirho {
    /// Check if the effective set has a specific capability.
    pub fn has_cap_chirho(&self, cap_chirho: u32) -> bool {
        if cap_chirho > CAP_LAST_CAP_CHIRHO {
            return false;
        }
        self.effective_chirho & (1u64 << cap_chirho) != 0
    }

    /// Drop a capability from the effective set.
    pub fn drop_cap_chirho(&mut self, cap_chirho: u32) {
        if cap_chirho <= CAP_LAST_CAP_CHIRHO {
            self.effective_chirho &= !(1u64 << cap_chirho);
        }
    }

    /// Raise a capability in the effective set (must be in permitted).
    pub fn raise_cap_chirho(&mut self, cap_chirho: u32) -> bool {
        if cap_chirho > CAP_LAST_CAP_CHIRHO {
            return false;
        }
        if self.permitted_chirho & (1u64 << cap_chirho) == 0 {
            return false;
        }
        self.effective_chirho |= 1u64 << cap_chirho;
        true
    }

    /// Create an unprivileged capability set (no capabilities).
    pub fn unprivileged_chirho() -> Self {
        Self {
            effective_chirho: 0,
            permitted_chirho: 0,
            inheritable_chirho: 0,
            bounding_chirho: (1u64 << (CAP_LAST_CAP_CHIRHO + 1)) - 1,
            ambient_chirho: 0,
        }
    }

    /// Apply capability transformation for execve (Linux semantics).
    pub fn exec_transform_chirho(&mut self, file_caps_chirho: Option<&CapabilitySetChirho>) {
        if let Some(fc_chirho) = file_caps_chirho {
            self.permitted_chirho = (fc_chirho.permitted_chirho & self.bounding_chirho)
                | (self.inheritable_chirho & fc_chirho.inheritable_chirho);
            self.effective_chirho = self.permitted_chirho & fc_chirho.effective_chirho;
        } else {
            // No file caps: keep ambient, clear effective
            self.permitted_chirho = self.inheritable_chirho | self.ambient_chirho;
            self.effective_chirho = self.ambient_chirho;
        }
    }
}
