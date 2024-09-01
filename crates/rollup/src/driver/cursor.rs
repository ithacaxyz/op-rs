use std::collections::{BTreeMap, VecDeque};

use hashbrown::HashMap;
use kona_primitives::{BlockInfo, L2BlockInfo};

/// A cursor that keeps track of the L2 tip block for a given L1 origin block.
///
/// The cursor is used to advance the pipeline to the next L2 block, and to reset
/// the pipeline to a previous L2 block when a reorg happens.
#[derive(Debug)]
pub struct SyncCursor {
    /// The block cache capacity before evicting old entries
    /// (to avoid unbounded memory growth)
    capacity: usize,
    /// The L1 origin block numbers for which we have an L2 block in the cache.
    /// Used to keep track of the order of insertion and evict the oldest entry.
    l1_origin_key_order: VecDeque<u64>,
    /// The L1 origin block info for which we have an L2 block in the cache.
    l1_origin_block_info: HashMap<u64, BlockInfo>,
    /// Map of the L1 origin block number to its corresponding tip L2 block
    l1_origin_to_l2_blocks: BTreeMap<u64, L2BlockInfo>,
}

#[allow(unused)]
impl SyncCursor {
    /// Create a new cursor with the default cache capacity.
    pub fn new() -> Self {
        // NOTE: this value must be greater than the `CHANNEL_TIMEOUT` to allow
        // for derivation to proceed through a deep reorg. This value is set
        // to 300 blocks before the Granite hardfork and 50 blocks after it.
        // Ref: <https://specs.optimism.io/protocol/derivation.html#timeouts>
        Self::with_capacity(350)
    }

    /// Create a new cursor with the given cache capacity.
    fn with_capacity(capacity: usize) -> Self {
        Self {
            capacity,
            l1_origin_key_order: VecDeque::with_capacity(capacity),
            l1_origin_block_info: HashMap::with_capacity(capacity),
            l1_origin_to_l2_blocks: BTreeMap::new(),
        }
    }

    /// Get the current L2 tip and the corresponding L1 origin block info.
    pub fn tip(&self) -> (L2BlockInfo, BlockInfo) {
        if let Some((origin_number, l2_tip)) = self.l1_origin_to_l2_blocks.last_key_value() {
            let origin_block = self.l1_origin_block_info[origin_number];
            (*l2_tip, origin_block)
        } else {
            unreachable!("cursor must be initialized with one block before advancing")
        }
    }

    /// Advance the cursor to the provided L2 block, given the corresponding L1 origin block.
    ///
    /// If the cache is full, the oldest entry is evicted.
    pub fn advance(&mut self, l1_origin_block: BlockInfo, l2_tip_block: L2BlockInfo) {
        if self.l1_origin_to_l2_blocks.len() >= self.capacity {
            let key = self.l1_origin_key_order.pop_front().unwrap();
            self.l1_origin_to_l2_blocks.remove(&key);
        }

        self.l1_origin_key_order.push_back(l1_origin_block.number);
        self.l1_origin_block_info.insert(l1_origin_block.number, l1_origin_block);
        self.l1_origin_to_l2_blocks.insert(l1_origin_block.number, l2_tip_block);
    }

    /// When the L1 undergoes a reorg, we need to reset the cursor to the fork block.
    /// This is the last L1 block for which we have a corresponding L2 block in the cache.
    ///
    /// Returns the (L2 block info, L1 origin block info) tuple for the new cursor state.
    pub fn reset(&mut self, fork_block: u64) -> (BlockInfo, BlockInfo) {
        match self.l1_origin_to_l2_blocks.get(&fork_block) {
            Some(l2_safe_tip) => {
                // The fork block is in the cache, we can use it to reset the cursor.
                (l2_safe_tip.block_info, self.l1_origin_block_info[&fork_block])
            }
            None => {
                // If the fork block is not in the cache, we reset the cursor
                // to the last known L1 block for which we have a corresponding L2 block.
                let (last_l1_known_tip, l2_known_tip) = self
                    .l1_origin_to_l2_blocks
                    .range(..=fork_block)
                    .next_back()
                    .expect("walked back to genesis without finding anchor origin block");

                (l2_known_tip.block_info, self.l1_origin_block_info[last_l1_known_tip])
            }
        }
    }
}
