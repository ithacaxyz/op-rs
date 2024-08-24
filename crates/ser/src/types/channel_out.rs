//! Contains logic for constructing a channel.

use alloc::{sync::Arc, vec, vec::Vec};
use alloy_rlp::Encodable;
use eyre::{bail, Result};
use kona_derive::batch::SingleBatch;
use kona_primitives::{
    ChannelID, Frame, RollupConfig, FJORD_MAX_RLP_BYTES_PER_CHANNEL, MAX_RLP_BYTES_PER_CHANNEL,
};
use rand::{rngs::SmallRng, Rng, SeedableRng};
use tracing::{error, trace, warn};

use crate::traits::Compressor;

/// The absolute minimum size of a frame.
/// This is the fixed overhead frame size, calculated as specified
/// in the [Frame Format][ff] specs: 16 + 2 + 4 + 1 = 23 bytes.
///
/// [ff]: https://github.com/ethereum-optimism/specs/blob/main/specs/protocol/derivation.md#frame-format
pub(crate) const FRAME_V0_OVERHEAD_SIZE: u64 = 23;

/// The channel output type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelOut<C: Compressor> {
    /// The current channel id.
    pub id: ChannelID,
    /// The current frame number.
    pub frame: u16,
    /// The uncompressed size of the channel.
    /// This must be less than [kona_primitives::MAX_RLP_BYTES_PER_CHANNEL].
    pub rlp_length: u64,
    /// If the channel is closed.
    pub closed: bool,
    /// The configured max channel size.
    /// This should come from the Chain Spec.
    pub max_frame_size: u64,
    /// The rollup config.
    pub rollup_config: Arc<RollupConfig>,
    /// The compressor to use.
    pub compressor: C,
    /// The l1 block number that the sequencer window times out.
    pub sequencer_window_timeout: u64,
    /// Channel duration timeout.
    pub channel_timeout: u64,
    /// The sub safety margin.
    pub sub_safety_margin: u64,
}

impl<C: Compressor> ChannelOut<C> {
    /// Constructs a new [ChannelOut].
    pub fn new(cfg: Arc<RollupConfig>, c: C, epoch_num: u64, sub_safety_margin: u64) -> Self {
        let mut small_rng = SmallRng::from_entropy();
        let mut id = ChannelID::default();
        small_rng.fill(&mut id);
        let max_channel_duration = Self::max_channel_duration(&cfg);
        let sequencer_window_size = cfg.seq_window_size;
        Self {
            id,
            frame: 0,
            rlp_length: 0,
            closed: false,
            max_frame_size: 0,
            rollup_config: cfg,
            compressor: c,
            sequencer_window_timeout: epoch_num + sequencer_window_size - sub_safety_margin,
            channel_timeout: epoch_num + max_channel_duration,
            sub_safety_margin,
        }
    }

    /// Returns the max channel duration.
    pub fn max_channel_duration(_cfg: &RollupConfig) -> u64 {
        unimplemented!()
    }

    /// Returns the ready bytes from the channel.
    pub fn ready_bytes(&self) -> usize {
        self.compressor.len()
    }

    /// Max RLP Bytes per channel.
    /// This is retrieved from the Chain Spec since it changes after the Fjord Hardfork.
    /// Uses the batch timestamp to determine the max RLP bytes per channel.
    pub fn max_rlp_bytes_per_channel(&self, batch: &SingleBatch) -> u64 {
        if self.rollup_config.is_fjord_active(batch.timestamp) {
            return FJORD_MAX_RLP_BYTES_PER_CHANNEL;
        }
        MAX_RLP_BYTES_PER_CHANNEL
    }

    /// Checks if the batch is timed out.
    pub fn check_timed_out(&mut self, l1_block_num: u64) {
        if self.sequencer_window_timeout < l1_block_num || self.channel_timeout < l1_block_num {
            warn!(target: "channel-out", "Batch is timed out. Closing channel: {:?}", self.id);
            self.closed = true;
        }
    }

    /// Adds a batch to the [ChannelOut].
    pub fn add_batch(&mut self, batch: SingleBatch) {
        if self.closed {
            warn!(target: "channel-out", "Channel is closed. Not adding batch: {:?}", self.id);
            return;
        }

        // RLP encode the batch
        let mut buf = Vec::new();
        batch.encode(&mut buf);

        let max_per_channel = self.max_rlp_bytes_per_channel(&batch);
        if self.rlp_length + buf.len() as u64 > max_per_channel {
            warn!(target: "channel-out", "Batch exceeds max RLP bytes per channel ({}). Closing channel: {:?}", max_per_channel, self.id);
            self.closed = true;
            return;
        }

        self.rlp_length += buf.len() as u64;

        match self.compressor.write(&buf) {
            Ok(n) => trace!(target: "channel-out", "Wrote {} bytes to compressor", n),
            Err(e) => {
                error!(target: "channel-out", "Error writing batch to compressor: {:?}", e);
            }
        }
    }

    /// Updates the channel timeout when a frame is published.
    pub fn frame_published(&mut self, l1_block_num: u64) {
        let new_timeout =
            l1_block_num + self.rollup_config.channel_timeout - self.sub_safety_margin;
        if self.channel_timeout > new_timeout {
            self.channel_timeout = new_timeout;
        }
    }

    /// Checks if a frame has enough bytes to output.
    pub fn frame_ready(&self) -> bool {
        self.ready_bytes() as u64 + FRAME_V0_OVERHEAD_SIZE >= self.max_frame_size
    }

    /// Force compress the channel to produce frame bytes.
    pub fn flush(&mut self) -> Result<()> {
        self.compressor.flush()
    }

    /// Compresses the channel and reads the compressed data.
    pub fn compress(&mut self) -> Result<Vec<u8>> {
        let mut buf = vec![0; self.ready_bytes()];
        match self.compressor.read(&mut buf) {
            Ok(n) => trace!(target: "channel-out", "Read {} bytes from compressor", n),
            Err(e) => {
                warn!(target: "channel-out", "Error reading compressed data: {:?}", e);
                bail!("Error reading compressed data: {:?}", e);
            }
        }
        Ok(buf)
    }

    /// Outputs the next frame if available.
    pub fn output_frame(&mut self) -> Option<Frame> {
        if !self.frame_ready() {
            return None;
        }

        let data = match self.compress() {
            Ok(d) => d,
            Err(e) => {
                warn!(target: "channel-out", "Error compressing data: {:?}", e);
                return None;
            }
        };

        let frame = Frame { id: self.id, number: self.frame, data, is_last: false };
        self.frame += 1;

        // If the max frame number is reached,
        // the channel must be closed.
        if self.frame == u16::MAX {
            warn!(target: "channel-out", "Max frame number reached ({}). Closed channel: ({:?})", self.frame, self.id);
            self.closed = true;
        }

        Some(frame)
    }
}
