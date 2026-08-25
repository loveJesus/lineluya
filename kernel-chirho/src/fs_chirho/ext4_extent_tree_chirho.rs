// For God so loved the world, that he gave his only begotten Son,
// that whosoever believeth in him should not perish, but have everlasting life.
// — John 3:16 (KJV)

//! Bounded parsing for one ext4 extent-tree node.
//!
//! Inode-root nodes contain only 60 bytes, while child nodes occupy a full
//! filesystem block. Callers must pass the complete node so every declared
//! extent entry is either parsed or rejected as malformed; silently truncating
//! a child node turns mapped file blocks into apparent sparse holes.

extern crate alloc;

use alloc::vec::Vec;

const EXT4_EXTENT_MAGIC_CHIRHO: u16 = 0xF30A;
const EXT4_EXTENT_HEADER_SIZE_CHIRHO: usize = 12;
const EXT4_EXTENT_ENTRY_SIZE_CHIRHO: usize = 12;

/// The result of resolving one logical block within a single extent node.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ExtentNodeTargetChirho {
    /// A leaf extent directly maps the requested logical block.
    PhysicalBlockChirho(u64),
    /// An index entry names the next extent-tree block to inspect.
    ChildNodeChirho(u64),
    /// This well-formed node contains no mapping for the requested block.
    SparseHoleChirho,
}

/// A validated decision plus the node depth that produced it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ExtentNodeLookupChirho {
    pub depth_chirho: u16,
    pub target_chirho: ExtentNodeTargetChirho,
}

fn read_u16_chirho(data_chirho: &[u8], offset_chirho: usize) -> Option<u16> {
    Some(u16::from_le_bytes([
        *data_chirho.get(offset_chirho)?,
        *data_chirho.get(offset_chirho + 1)?,
    ]))
}

fn read_u32_chirho(data_chirho: &[u8], offset_chirho: usize) -> Option<u32> {
    Some(u32::from_le_bytes([
        *data_chirho.get(offset_chirho)?,
        *data_chirho.get(offset_chirho + 1)?,
        *data_chirho.get(offset_chirho + 2)?,
        *data_chirho.get(offset_chirho + 3)?,
    ]))
}

/// Select the leaf mapping or index child for `logical_block_chirho`.
///
/// `None` means the node is malformed or truncated. A valid node with no
/// matching entry returns `SparseHoleChirho`, keeping corruption distinct from
/// an intentional sparse block.
pub(crate) fn lookup_extent_node_chirho(
    node_data_chirho: &[u8],
    logical_block_chirho: u32,
) -> Option<ExtentNodeLookupChirho> {
    if node_data_chirho.len() < EXT4_EXTENT_HEADER_SIZE_CHIRHO {
        return None;
    }

    let magic_chirho = read_u16_chirho(node_data_chirho, 0)?;
    let entries_chirho = read_u16_chirho(node_data_chirho, 2)? as usize;
    let max_entries_chirho = read_u16_chirho(node_data_chirho, 4)? as usize;
    let depth_chirho = read_u16_chirho(node_data_chirho, 6)?;
    let available_entries_chirho = node_data_chirho
        .len()
        .saturating_sub(EXT4_EXTENT_HEADER_SIZE_CHIRHO)
        / EXT4_EXTENT_ENTRY_SIZE_CHIRHO;

    if magic_chirho != EXT4_EXTENT_MAGIC_CHIRHO
        || entries_chirho > max_entries_chirho
        || max_entries_chirho > available_entries_chirho
        || entries_chirho > available_entries_chirho
    {
        return None;
    }

    if depth_chirho == 0 {
        for entry_index_chirho in 0..entries_chirho {
            let entry_offset_chirho =
                EXT4_EXTENT_HEADER_SIZE_CHIRHO + entry_index_chirho * EXT4_EXTENT_ENTRY_SIZE_CHIRHO;
            let first_logical_chirho = read_u32_chirho(node_data_chirho, entry_offset_chirho)?;
            let encoded_length_chirho = read_u16_chirho(node_data_chirho, entry_offset_chirho + 4)?;
            let block_count_chirho = if encoded_length_chirho > 0x8000 {
                (encoded_length_chirho - 0x8000) as u32
            } else {
                encoded_length_chirho as u32
            };
            if block_count_chirho == 0 {
                return None;
            }

            let logical_end_chirho = first_logical_chirho.checked_add(block_count_chirho)?;
            if logical_block_chirho >= first_logical_chirho
                && logical_block_chirho < logical_end_chirho
            {
                let physical_high_chirho =
                    read_u16_chirho(node_data_chirho, entry_offset_chirho + 6)? as u64;
                let physical_low_chirho =
                    read_u32_chirho(node_data_chirho, entry_offset_chirho + 8)? as u64;
                let physical_start_chirho = (physical_high_chirho << 32) | physical_low_chirho;
                return Some(ExtentNodeLookupChirho {
                    depth_chirho,
                    target_chirho: ExtentNodeTargetChirho::PhysicalBlockChirho(
                        physical_start_chirho
                            + u64::from(logical_block_chirho - first_logical_chirho),
                    ),
                });
            }
        }

        return Some(ExtentNodeLookupChirho {
            depth_chirho,
            target_chirho: ExtentNodeTargetChirho::SparseHoleChirho,
        });
    }

    let mut selected_child_chirho = None;
    for entry_index_chirho in 0..entries_chirho {
        let entry_offset_chirho =
            EXT4_EXTENT_HEADER_SIZE_CHIRHO + entry_index_chirho * EXT4_EXTENT_ENTRY_SIZE_CHIRHO;
        let first_logical_chirho = read_u32_chirho(node_data_chirho, entry_offset_chirho)?;
        if logical_block_chirho < first_logical_chirho {
            break;
        }
        let child_low_chirho = read_u32_chirho(node_data_chirho, entry_offset_chirho + 4)? as u64;
        let child_high_chirho = read_u16_chirho(node_data_chirho, entry_offset_chirho + 8)? as u64;
        selected_child_chirho = Some((child_high_chirho << 32) | child_low_chirho);
    }

    Some(ExtentNodeLookupChirho {
        depth_chirho,
        target_chirho: selected_child_chirho
            .map(ExtentNodeTargetChirho::ChildNodeChirho)
            .unwrap_or(ExtentNodeTargetChirho::SparseHoleChirho),
    })
}

/// Resolve a logical block through a complete, bounded extent tree.
///
/// The root is the inode's 60-byte `i_block` field. Child nodes supplied by
/// `read_child_node_chirho` retain their full filesystem-block length.
pub(crate) fn find_physical_block_chirho<ReadChildNodeChirho>(
    root_node_chirho: &[u8],
    logical_block_chirho: u32,
    root_depth_chirho: u16,
    mut read_child_node_chirho: ReadChildNodeChirho,
) -> Option<u64>
where
    ReadChildNodeChirho: FnMut(u64) -> Option<Vec<u8>>,
{
    const MAX_EXTENT_TREE_DEPTH_CHIRHO: u16 = 5;

    fn descend_extent_tree_chirho<ReadChildNodeChirho>(
        node_data_chirho: &[u8],
        logical_block_chirho: u32,
        expected_depth_chirho: u16,
        read_child_node_chirho: &mut ReadChildNodeChirho,
    ) -> Option<u64>
    where
        ReadChildNodeChirho: FnMut(u64) -> Option<Vec<u8>>,
    {
        let lookup_chirho = lookup_extent_node_chirho(node_data_chirho, logical_block_chirho)?;
        if lookup_chirho.depth_chirho != expected_depth_chirho {
            return None;
        }
        match lookup_chirho.target_chirho {
            ExtentNodeTargetChirho::PhysicalBlockChirho(physical_block_chirho) => {
                Some(physical_block_chirho)
            }
            ExtentNodeTargetChirho::SparseHoleChirho => None,
            ExtentNodeTargetChirho::ChildNodeChirho(child_block_chirho) => {
                if expected_depth_chirho == 0 {
                    return None;
                }
                let child_node_chirho = read_child_node_chirho(child_block_chirho)?;
                descend_extent_tree_chirho(
                    &child_node_chirho,
                    logical_block_chirho,
                    expected_depth_chirho - 1,
                    read_child_node_chirho,
                )
            }
        }
    }

    if root_depth_chirho > MAX_EXTENT_TREE_DEPTH_CHIRHO {
        return None;
    }
    descend_extent_tree_chirho(
        root_node_chirho,
        logical_block_chirho,
        root_depth_chirho,
        &mut read_child_node_chirho,
    )
}

#[cfg(test)]
mod tests_chirho {
    use super::{
        find_physical_block_chirho, lookup_extent_node_chirho, ExtentNodeLookupChirho,
        ExtentNodeTargetChirho, EXT4_EXTENT_MAGIC_CHIRHO,
    };

    fn write_u16_chirho(data_chirho: &mut [u8], offset_chirho: usize, value_chirho: u16) {
        data_chirho[offset_chirho..offset_chirho + 2].copy_from_slice(&value_chirho.to_le_bytes());
    }

    fn write_u32_chirho(data_chirho: &mut [u8], offset_chirho: usize, value_chirho: u32) {
        data_chirho[offset_chirho..offset_chirho + 4].copy_from_slice(&value_chirho.to_le_bytes());
    }

    fn five_extent_leaf_chirho() -> [u8; 4096] {
        let mut node_chirho = [0u8; 4096];
        write_u16_chirho(&mut node_chirho, 0, EXT4_EXTENT_MAGIC_CHIRHO);
        write_u16_chirho(&mut node_chirho, 2, 5);
        write_u16_chirho(&mut node_chirho, 4, 340);
        write_u16_chirho(&mut node_chirho, 6, 0);

        for entry_index_chirho in 0..5usize {
            let entry_offset_chirho = 12 + entry_index_chirho * 12;
            write_u32_chirho(
                &mut node_chirho,
                entry_offset_chirho,
                entry_index_chirho as u32,
            );
            write_u16_chirho(&mut node_chirho, entry_offset_chirho + 4, 1);
            write_u16_chirho(&mut node_chirho, entry_offset_chirho + 6, 0);
            write_u32_chirho(
                &mut node_chirho,
                entry_offset_chirho + 8,
                100 + entry_index_chirho as u32,
            );
        }
        node_chirho
    }

    fn one_child_root_chirho(child_block_chirho: u32) -> [u8; 60] {
        let mut node_chirho = [0u8; 60];
        write_u16_chirho(&mut node_chirho, 0, EXT4_EXTENT_MAGIC_CHIRHO);
        write_u16_chirho(&mut node_chirho, 2, 1);
        write_u16_chirho(&mut node_chirho, 4, 4);
        write_u16_chirho(&mut node_chirho, 6, 1);
        write_u32_chirho(&mut node_chirho, 12, 0);
        write_u32_chirho(&mut node_chirho, 16, child_block_chirho);
        node_chirho
    }

    #[test]
    fn full_child_node_maps_fifth_extent_chirho() {
        let node_chirho = five_extent_leaf_chirho();
        assert_eq!(
            lookup_extent_node_chirho(&node_chirho, 4),
            Some(ExtentNodeLookupChirho {
                depth_chirho: 0,
                target_chirho: ExtentNodeTargetChirho::PhysicalBlockChirho(104),
            }),
        );
    }

    #[test]
    fn truncated_child_node_is_not_reported_as_sparse_chirho() {
        let node_chirho = five_extent_leaf_chirho();
        assert_eq!(lookup_extent_node_chirho(&node_chirho[..60], 4), None);
    }

    #[test]
    fn fragmented_file_reads_exact_fifth_extent_bytes_chirho() {
        let root_node_chirho = one_child_root_chirho(900);
        let child_node_chirho = five_extent_leaf_chirho();
        let physical_block_chirho =
            find_physical_block_chirho(&root_node_chirho, 4, 1, |requested_node_chirho| {
                (requested_node_chirho == 900).then(|| child_node_chirho.to_vec())
            })
            .expect("fragmented file logical block 4 was not mapped");

        let mut fifth_block_chirho = [0u8; 4096];
        let expected_payload_chirho = b"fragmented-file-fifth-extent-chirho";
        fifth_block_chirho[..expected_payload_chirho.len()]
            .copy_from_slice(expected_payload_chirho);
        let read_block_chirho = |requested_block_chirho: u64| {
            (requested_block_chirho == 104).then_some(fifth_block_chirho)
        };
        let actual_block_chirho = read_block_chirho(physical_block_chirho)
            .expect("extent walker selected the wrong physical block");
        assert_eq!(
            &actual_block_chirho[..expected_payload_chirho.len()],
            expected_payload_chirho,
        );
    }
}
