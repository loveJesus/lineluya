<!-- For God so loved the world, that he gave his only begotten Son, that whosoever believeth in him should not perish, but have everlasting life. — John 3:16 (KJV) -->

# Address-space lifecycle Chirho

This workflow owns the type and accounting boundary between physical RAM,
user PTEs, fork/COW, `CLONE_VM`, exec, and exit. A raw physical PML4 address is
not an ownership token. `AddressSpaceHandleChirho` is the token, and its `Drop`
path performs atomic accounting only; page-table walks and frame reclamation
are explicit cold-path work.

```mermaid
flowchart TD
    boot_init_chirho[Boot: preallocate flat per-frame counters] --> boot_scan_chirho[Register pre-existing user PTEs]
    boot_scan_chirho --> map_leaf_chirho[Map: retain new managed leaf before publishing PTE]
    map_leaf_chirho --> replace_leaf_chirho{Displaced user leaf?}
    replace_leaf_chirho -->|yes| release_replaced_chirho[Release old leaf; free only on last reference]
    replace_leaf_chirho -->|no| live_space_chirho[Live owned address space]
    release_replaced_chirho --> live_space_chirho

    boot_scan_chirho --> boot_parent_chirho[Early init runs on raw boot PML4]
    boot_parent_chirho --> boot_clone_chirho[Boot-only constructor clones into a fresh owned handle]
    boot_clone_chirho --> clone_tree_chirho
    live_space_chirho --> fork_kind_chirho{Clone operation}
    fork_kind_chirho -->|CLONE_VM| share_handle_chirho[Atomically clone address-space handle]
    fork_kind_chirho -->|fork| clone_tree_chirho[Copy page-table levels and retain every shared leaf]
    clone_tree_chirho --> cow_mark_chirho[Managed writable leaves become read-only COW]
    share_handle_chirho --> live_space_chirho
    cow_mark_chirho --> live_space_chirho

    live_space_chirho --> cow_fault_chirho{COW write fault}
    cow_fault_chirho -->|count = 1| cow_exclusive_chirho[Clear COW and make writable in place]
    cow_fault_chirho -->|count > 1| cow_split_chirho[Allocate and retain new leaf, publish, release old leaf]
    cow_fault_chirho -->|count = 0 or unmanaged| cow_reject_chirho[Reject loudly; never invent ownership]
    cow_exclusive_chirho --> live_space_chirho
    cow_split_chirho --> live_space_chirho

    live_space_chirho --> unmap_chirho[munmap: clear PTE and release leaf]
    unmap_chirho --> unmap_last_chirho{Last mapping?}
    unmap_last_chirho -->|yes| recycle_leaf_chirho[Zero and return frame to intrusive O(1) free list]
    unmap_last_chirho -->|no| live_space_chirho

    live_space_chirho --> lifecycle_chirho{exec or exit}
    live_space_chirho --> mapper_acquire_chirho[Mapper acquisition compares bound root with CR3]
    mapper_acquire_chirho -->|different| mapper_rebind_chirho[Rebind under mapper lock before use]
    mapper_acquire_chirho -->|same| live_space_chirho
    mapper_rebind_chirho --> live_space_chirho
    lifecycle_chirho -->|exec| switch_root_chirho[Install new CR3 before retiring old handle]
    lifecycle_chirho -->|exit| detach_handle_chirho[Detach handle outside task/list/global locks]
    switch_root_chirho --> retire_owner_gate_chirho[Atomic last-owner retirement gate]
    detach_handle_chirho --> retire_owner_gate_chirho
    retire_owner_gate_chirho -->|shared owners| atomic_drop_chirho[Drop only decrements; no walk or lock]
    retire_owner_gate_chirho -->|last owner| retire_mapper_sync_chirho[Acquire current-root mapper guard and release stale borrow]
    retire_mapper_sync_chirho --> retire_root_gate_chirho{Root still live?}
    retire_root_gate_chirho -->|root equals active CR3| refuse_active_chirho[Return handle + ActiveCr3 error]
    retire_root_gate_chirho -->|root is borrowed by global mapper| refuse_mapper_chirho[Return handle + ActiveMapperRoot error]
    retire_root_gate_chirho -->|inactive| retire_tree_chirho[Release leaves, then recycle table levels and PML4]
```

Enforced invariants:

- one atomic word arbitrates handle clone versus last-owner retirement;
- only the boot-specific constructor accepts the unowned boot source, and it
  exposes no raw-root parameter to lifecycle callers;
- `retire_chirho` refuses the PML4 currently installed in CR3;
- `retire_chirho` also refuses the PML4 borrowed by the global mapper, so a
  missed post-switch rebind cannot leave a mutable reference into a recycled
  page-table tree; its cold path first acquires the current-root mapper guard
  so an ordinary stale binding is repaired instead of becoming a permanent
  retirement refusal;
- the raw global mapper is private, and its sole acquisition API lazily rebinds
  to the current CR3 before returning a guard; direct assembly CR3 switches do
  not run Rust with a task's restored user FS base;
- mapping, fork, COW, unmap, and retirement all use the same flat leaf counter;
- managed-counter underflow clamps at zero and returns an error;
- device/MMIO frames are outside allocator-owned ranges and are never returned
  to the RAM frame allocator;
- the frame allocator's recycled-frame list is intrusive, so releasing the last
  COW leaf never allocates while handling a fault;
- implicit handle `Drop` never walks page tables or acquires VFS, task-list, or
  frame-allocation locks.
