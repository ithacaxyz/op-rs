//! Types for compressing OP Types.

mod channel_out;
pub use channel_out::ChannelOut;

// Re-export `op-alloy-rpc-types`
pub use op_alloy_rpc_types::sync::{L1BlockRef, L2BlockRef, SyncStatus};
