// For God so loved the world, that he gave his only begotten Son,
// that whosoever believeth in him should not perish, but have everlasting life. — John 3:16 (KJV)

//! Allocation-free hot-path ownership accounting for physical leaf frames and
//! shared address-space handles.
//!
//! The physical-frame table is allocated once during boot. Every later retain
//! and release is one bounds check plus one atomic compare/exchange loop; COW
//! faults never grow a map or allocate metadata.

use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

const UNMANAGED_FRAME_COUNT_CHIRHO: u32 = u32::MAX;
const MAX_MANAGED_FRAME_COUNT_CHIRHO: u32 = u32::MAX - 1;

/// Half-open physical-frame-number range owned by the frame allocator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameRangeChirho {
    pub start_frame_chirho: usize,
    pub end_frame_exclusive_chirho: usize,
}

/// Boot-time construction failures for the flat frame table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameOwnershipInitErrorChirho {
    NoManagedFramesChirho,
    InvalidRangeChirho(FrameRangeChirho),
    AllocationFailedChirho,
}

/// A mapping retain failed without modifying the counter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameRetainErrorChirho {
    OverflowChirho { frame_number_chirho: usize },
}

/// Result of adding one PTE reference to a physical frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameRetainOutcomeChirho {
    UnmanagedChirho,
    ManagedChirho { references_chirho: u32 },
}

/// A mapping release failed without wrapping or modifying the counter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameReleaseErrorChirho {
    UnderflowChirho { frame_number_chirho: usize },
}

/// Result of removing one PTE reference from a physical frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameReleaseOutcomeChirho {
    UnmanagedChirho,
    StillReferencedChirho { references_chirho: u32 },
    LastReferenceChirho,
}

/// Flat O(1) mapping-reference table indexed directly by physical frame number.
///
/// Slots outside allocator-owned memory ranges carry a sentinel and are never
/// returned to the RAM frame allocator. This keeps framebuffer/MMIO mappings
/// out of COW ownership without device-specific address checks in the hot path.
pub struct LeafFrameOwnershipChirho {
    reference_counts_chirho: Vec<AtomicU32>,
    managed_frame_count_chirho: usize,
}

impl LeafFrameOwnershipChirho {
    /// Allocate and initialize the complete table before faults can occur.
    pub fn try_new_chirho(
        managed_ranges_chirho: &[FrameRangeChirho],
    ) -> Result<Self, FrameOwnershipInitErrorChirho> {
        let mut slot_count_chirho = 0usize;
        let mut managed_frame_count_chirho = 0usize;

        for range_chirho in managed_ranges_chirho {
            if range_chirho.start_frame_chirho >= range_chirho.end_frame_exclusive_chirho {
                return Err(FrameOwnershipInitErrorChirho::InvalidRangeChirho(
                    *range_chirho,
                ));
            }
            slot_count_chirho = slot_count_chirho.max(range_chirho.end_frame_exclusive_chirho);
            managed_frame_count_chirho = managed_frame_count_chirho.saturating_add(
                range_chirho
                    .end_frame_exclusive_chirho
                    .saturating_sub(range_chirho.start_frame_chirho),
            );
        }

        if slot_count_chirho == 0 || managed_frame_count_chirho == 0 {
            return Err(FrameOwnershipInitErrorChirho::NoManagedFramesChirho);
        }

        let mut reference_counts_chirho = Vec::new();
        reference_counts_chirho
            .try_reserve_exact(slot_count_chirho)
            .map_err(|_| FrameOwnershipInitErrorChirho::AllocationFailedChirho)?;
        for _slot_chirho in 0..slot_count_chirho {
            reference_counts_chirho.push(AtomicU32::new(UNMANAGED_FRAME_COUNT_CHIRHO));
        }

        for range_chirho in managed_ranges_chirho {
            for frame_number_chirho in
                range_chirho.start_frame_chirho..range_chirho.end_frame_exclusive_chirho
            {
                reference_counts_chirho[frame_number_chirho].store(0, Ordering::Relaxed);
            }
        }

        Ok(Self {
            reference_counts_chirho,
            managed_frame_count_chirho,
        })
    }

    /// Add one user-PTE reference. No allocation or lock is taken.
    pub fn retain_mapping_chirho(
        &self,
        frame_number_chirho: usize,
    ) -> Result<FrameRetainOutcomeChirho, FrameRetainErrorChirho> {
        let Some(reference_count_chirho) = self.reference_counts_chirho.get(frame_number_chirho)
        else {
            return Ok(FrameRetainOutcomeChirho::UnmanagedChirho);
        };

        let mut observed_chirho = reference_count_chirho.load(Ordering::Acquire);
        loop {
            if observed_chirho == UNMANAGED_FRAME_COUNT_CHIRHO {
                return Ok(FrameRetainOutcomeChirho::UnmanagedChirho);
            }
            if observed_chirho == MAX_MANAGED_FRAME_COUNT_CHIRHO {
                return Err(FrameRetainErrorChirho::OverflowChirho {
                    frame_number_chirho,
                });
            }
            match reference_count_chirho.compare_exchange_weak(
                observed_chirho,
                observed_chirho + 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    return Ok(FrameRetainOutcomeChirho::ManagedChirho {
                        references_chirho: observed_chirho + 1,
                    });
                }
                Err(actual_chirho) => observed_chirho = actual_chirho,
            }
        }
    }

    /// Remove one user-PTE reference. Underflow clamps at zero and is loud to
    /// the caller; it can never wrap into a false multi-billion reference.
    pub fn release_mapping_chirho(
        &self,
        frame_number_chirho: usize,
    ) -> Result<FrameReleaseOutcomeChirho, FrameReleaseErrorChirho> {
        let Some(reference_count_chirho) = self.reference_counts_chirho.get(frame_number_chirho)
        else {
            return Ok(FrameReleaseOutcomeChirho::UnmanagedChirho);
        };

        let mut observed_chirho = reference_count_chirho.load(Ordering::Acquire);
        loop {
            if observed_chirho == UNMANAGED_FRAME_COUNT_CHIRHO {
                return Ok(FrameReleaseOutcomeChirho::UnmanagedChirho);
            }
            if observed_chirho == 0 {
                return Err(FrameReleaseErrorChirho::UnderflowChirho {
                    frame_number_chirho,
                });
            }
            let remaining_chirho = observed_chirho - 1;
            match reference_count_chirho.compare_exchange_weak(
                observed_chirho,
                remaining_chirho,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) if remaining_chirho == 0 => {
                    return Ok(FrameReleaseOutcomeChirho::LastReferenceChirho);
                }
                Ok(_) => {
                    return Ok(FrameReleaseOutcomeChirho::StillReferencedChirho {
                        references_chirho: remaining_chirho,
                    });
                }
                Err(actual_chirho) => observed_chirho = actual_chirho,
            }
        }
    }

    /// Return the live mapping count, or `None` for device/hole/out-of-range frames.
    pub fn mapping_count_chirho(&self, frame_number_chirho: usize) -> Option<u32> {
        let count_chirho = self
            .reference_counts_chirho
            .get(frame_number_chirho)?
            .load(Ordering::Acquire);
        (count_chirho != UNMANAGED_FRAME_COUNT_CHIRHO).then_some(count_chirho)
    }

    pub fn slot_count_chirho(&self) -> usize {
        self.reference_counts_chirho.len()
    }

    pub fn managed_frame_count_chirho(&self) -> usize {
        self.managed_frame_count_chirho
    }
}

/// Atomic owner-count errors for an address-space control record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressSpaceOwnerErrorChirho {
    ReleasedChirho,
    OverflowChirho,
    UnderflowChirho,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressSpaceRetireGateErrorChirho {
    NotLiveChirho,
    SharedOwnersChirho { owners_chirho: u32 },
}

/// Result of dropping one handle reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressSpaceOwnerReleaseChirho {
    OwnersRemainChirho { owners_chirho: u32 },
    LastOwnerChirho,
}

/// Allocation-free owner counter used by the x86 address-space handle.
pub struct AddressSpaceOwnerCountChirho {
    owners_and_state_chirho: AtomicU64,
}

const ADDRESS_SPACE_RETIRING_BIT_CHIRHO: u64 = 1 << 63;
const ADDRESS_SPACE_OWNER_MASK_CHIRHO: u64 = u32::MAX as u64;

impl AddressSpaceOwnerCountChirho {
    pub const fn new_chirho() -> Self {
        Self {
            owners_and_state_chirho: AtomicU64::new(1),
        }
    }

    pub fn try_retain_chirho(&self) -> Result<u32, AddressSpaceOwnerErrorChirho> {
        let mut observed_chirho = self.owners_and_state_chirho.load(Ordering::Acquire);
        loop {
            if observed_chirho == 0 || observed_chirho & ADDRESS_SPACE_RETIRING_BIT_CHIRHO != 0 {
                return Err(AddressSpaceOwnerErrorChirho::ReleasedChirho);
            }
            let owner_count_chirho = observed_chirho & ADDRESS_SPACE_OWNER_MASK_CHIRHO;
            if owner_count_chirho == u32::MAX as u64 {
                return Err(AddressSpaceOwnerErrorChirho::OverflowChirho);
            }
            match self.owners_and_state_chirho.compare_exchange_weak(
                observed_chirho,
                observed_chirho + 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return Ok((owner_count_chirho + 1) as u32),
                Err(actual_chirho) => observed_chirho = actual_chirho,
            }
        }
    }

    pub fn release_chirho(
        &self,
    ) -> Result<AddressSpaceOwnerReleaseChirho, AddressSpaceOwnerErrorChirho> {
        let mut observed_chirho = self.owners_and_state_chirho.load(Ordering::Acquire);
        loop {
            if observed_chirho == 0 {
                return Err(AddressSpaceOwnerErrorChirho::UnderflowChirho);
            }
            if observed_chirho & ADDRESS_SPACE_RETIRING_BIT_CHIRHO != 0 {
                return Err(AddressSpaceOwnerErrorChirho::ReleasedChirho);
            }
            let owner_count_chirho = observed_chirho & ADDRESS_SPACE_OWNER_MASK_CHIRHO;
            if owner_count_chirho == 0 {
                return Err(AddressSpaceOwnerErrorChirho::UnderflowChirho);
            }
            let remaining_chirho = owner_count_chirho - 1;
            match self.owners_and_state_chirho.compare_exchange_weak(
                observed_chirho,
                remaining_chirho,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) if remaining_chirho == 0 => {
                    return Ok(AddressSpaceOwnerReleaseChirho::LastOwnerChirho);
                }
                Ok(_) => {
                    return Ok(AddressSpaceOwnerReleaseChirho::OwnersRemainChirho {
                        owners_chirho: remaining_chirho as u32,
                    });
                }
                Err(actual_chirho) => observed_chirho = actual_chirho,
            }
        }
    }

    /// Atomically reserve retirement for exactly one live owner. A concurrent
    /// clone and last-owner retirement cannot both succeed.
    pub fn try_begin_retirement_chirho(&self) -> Result<(), AddressSpaceRetireGateErrorChirho> {
        match self.owners_and_state_chirho.compare_exchange(
            1,
            ADDRESS_SPACE_RETIRING_BIT_CHIRHO | 1,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => Ok(()),
            Err(observed_chirho)
                if observed_chirho == 0
                    || observed_chirho & ADDRESS_SPACE_RETIRING_BIT_CHIRHO != 0 =>
            {
                Err(AddressSpaceRetireGateErrorChirho::NotLiveChirho)
            }
            Err(observed_chirho) => Err(AddressSpaceRetireGateErrorChirho::SharedOwnersChirho {
                owners_chirho: (observed_chirho & ADDRESS_SPACE_OWNER_MASK_CHIRHO) as u32,
            }),
        }
    }

    pub fn cancel_retirement_chirho(&self) -> Result<(), AddressSpaceOwnerErrorChirho> {
        self.owners_and_state_chirho
            .compare_exchange(
                ADDRESS_SPACE_RETIRING_BIT_CHIRHO | 1,
                1,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .map(|_| ())
            .map_err(|_| AddressSpaceOwnerErrorChirho::ReleasedChirho)
    }

    pub fn finish_retirement_chirho(&self) -> Result<(), AddressSpaceOwnerErrorChirho> {
        self.owners_and_state_chirho
            .compare_exchange(
                ADDRESS_SPACE_RETIRING_BIT_CHIRHO | 1,
                0,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .map(|_| ())
            .map_err(|_| AddressSpaceOwnerErrorChirho::ReleasedChirho)
    }

    pub fn owner_count_chirho(&self) -> u32 {
        (self.owners_and_state_chirho.load(Ordering::Acquire) & ADDRESS_SPACE_OWNER_MASK_CHIRHO)
            as u32
    }
}

#[cfg(test)]
mod tests_chirho {
    use super::*;

    fn ownership_fixture_chirho() -> LeafFrameOwnershipChirho {
        LeafFrameOwnershipChirho::try_new_chirho(&[
            FrameRangeChirho {
                start_frame_chirho: 2,
                end_frame_exclusive_chirho: 6,
            },
            FrameRangeChirho {
                start_frame_chirho: 9,
                end_frame_exclusive_chirho: 11,
            },
        ])
        .expect("fixture ranges must initialize")
    }

    #[test]
    fn duplicate_mapping_survives_original_release_chirho() {
        let ownership_chirho = ownership_fixture_chirho();
        assert_eq!(
            ownership_chirho.retain_mapping_chirho(3),
            Ok(FrameRetainOutcomeChirho::ManagedChirho {
                references_chirho: 1,
            })
        );
        assert_eq!(
            ownership_chirho.retain_mapping_chirho(3),
            Ok(FrameRetainOutcomeChirho::ManagedChirho {
                references_chirho: 2,
            })
        );
        assert_eq!(
            ownership_chirho.release_mapping_chirho(3),
            Ok(FrameReleaseOutcomeChirho::StillReferencedChirho {
                references_chirho: 1,
            })
        );
        assert_eq!(ownership_chirho.mapping_count_chirho(3), Some(1));
        assert_eq!(
            ownership_chirho.release_mapping_chirho(3),
            Ok(FrameReleaseOutcomeChirho::LastReferenceChirho)
        );
    }

    #[test]
    fn underflow_clamps_at_zero_chirho() {
        let ownership_chirho = ownership_fixture_chirho();
        assert_eq!(
            ownership_chirho.release_mapping_chirho(4),
            Err(FrameReleaseErrorChirho::UnderflowChirho {
                frame_number_chirho: 4,
            })
        );
        assert_eq!(ownership_chirho.mapping_count_chirho(4), Some(0));
    }

    #[test]
    fn holes_and_out_of_range_frames_are_unmanaged_chirho() {
        let ownership_chirho = ownership_fixture_chirho();
        for frame_number_chirho in [0usize, 7, 8, 11, 4096] {
            assert_eq!(
                ownership_chirho.retain_mapping_chirho(frame_number_chirho),
                Ok(FrameRetainOutcomeChirho::UnmanagedChirho)
            );
            assert_eq!(
                ownership_chirho.release_mapping_chirho(frame_number_chirho),
                Ok(FrameReleaseOutcomeChirho::UnmanagedChirho)
            );
        }
    }

    #[test]
    fn shared_address_space_retires_only_after_last_owner_chirho() {
        let owners_chirho = AddressSpaceOwnerCountChirho::new_chirho();
        assert_eq!(owners_chirho.try_retain_chirho(), Ok(2));
        assert_eq!(
            owners_chirho.release_chirho(),
            Ok(AddressSpaceOwnerReleaseChirho::OwnersRemainChirho { owners_chirho: 1 })
        );
        assert_eq!(owners_chirho.owner_count_chirho(), 1);
        assert_eq!(
            owners_chirho.release_chirho(),
            Ok(AddressSpaceOwnerReleaseChirho::LastOwnerChirho)
        );
        assert_eq!(owners_chirho.owner_count_chirho(), 0);
        assert_eq!(
            owners_chirho.release_chirho(),
            Err(AddressSpaceOwnerErrorChirho::UnderflowChirho)
        );
    }

    #[test]
    fn retirement_gate_excludes_concurrent_clone_chirho() {
        let owners_chirho = AddressSpaceOwnerCountChirho::new_chirho();
        assert_eq!(owners_chirho.try_begin_retirement_chirho(), Ok(()));
        assert_eq!(
            owners_chirho.try_retain_chirho(),
            Err(AddressSpaceOwnerErrorChirho::ReleasedChirho)
        );
        assert_eq!(owners_chirho.cancel_retirement_chirho(), Ok(()));
        assert_eq!(owners_chirho.try_retain_chirho(), Ok(2));
        assert_eq!(
            owners_chirho.try_begin_retirement_chirho(),
            Err(AddressSpaceRetireGateErrorChirho::SharedOwnersChirho { owners_chirho: 2 })
        );
    }

    #[test]
    fn fork_cow_unmap_and_exit_release_each_leaf_chirho() {
        let ownership_chirho = ownership_fixture_chirho();

        // Parent mapping, then the child PTE installed by fork.
        assert_eq!(
            ownership_chirho.retain_mapping_chirho(3),
            Ok(FrameRetainOutcomeChirho::ManagedChirho {
                references_chirho: 1,
            })
        );
        assert_eq!(
            ownership_chirho.retain_mapping_chirho(3),
            Ok(FrameRetainOutcomeChirho::ManagedChirho {
                references_chirho: 2,
            })
        );

        // Child COW publishes its new leaf before releasing the shared leaf.
        assert_eq!(
            ownership_chirho.retain_mapping_chirho(4),
            Ok(FrameRetainOutcomeChirho::ManagedChirho {
                references_chirho: 1,
            })
        );
        assert_eq!(
            ownership_chirho.release_mapping_chirho(3),
            Ok(FrameReleaseOutcomeChirho::StillReferencedChirho {
                references_chirho: 1,
            })
        );

        // Child munmap frees only its private COW leaf; parent exit then frees
        // the original. Neither transition underflows or strands a reference.
        assert_eq!(
            ownership_chirho.release_mapping_chirho(4),
            Ok(FrameReleaseOutcomeChirho::LastReferenceChirho)
        );
        assert_eq!(
            ownership_chirho.release_mapping_chirho(3),
            Ok(FrameReleaseOutcomeChirho::LastReferenceChirho)
        );
        assert_eq!(ownership_chirho.mapping_count_chirho(3), Some(0));
        assert_eq!(ownership_chirho.mapping_count_chirho(4), Some(0));
    }

    #[test]
    fn shared_exit_allows_only_last_owner_to_retire_chirho() {
        let owners_chirho = AddressSpaceOwnerCountChirho::new_chirho();
        assert_eq!(owners_chirho.try_retain_chirho(), Ok(2));
        assert_eq!(
            owners_chirho.try_begin_retirement_chirho(),
            Err(AddressSpaceRetireGateErrorChirho::SharedOwnersChirho { owners_chirho: 2 })
        );
        assert_eq!(
            owners_chirho.release_chirho(),
            Ok(AddressSpaceOwnerReleaseChirho::OwnersRemainChirho { owners_chirho: 1 })
        );
        assert_eq!(owners_chirho.try_begin_retirement_chirho(), Ok(()));
        assert_eq!(owners_chirho.finish_retirement_chirho(), Ok(()));
        assert_eq!(owners_chirho.owner_count_chirho(), 0);
        assert_eq!(
            owners_chirho.try_retain_chirho(),
            Err(AddressSpaceOwnerErrorChirho::ReleasedChirho)
        );
    }
}
