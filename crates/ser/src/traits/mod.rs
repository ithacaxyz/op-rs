//! Traits for serialization.

pub mod compressor;
pub use compressor::Compressor;

pub mod providers;
pub use providers::{L1Provider, L2Provider};

mod rollup;
pub use rollup::RollupNode;

pub mod stages;
pub use stages::{FramePublished, NextTransaction, OriginReceiver};
