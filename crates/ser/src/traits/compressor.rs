//! Compression Trait

use eyre::Result;

/// The compressor trait is used to compress channel data.
pub trait Compressor {
    /// Returns if the compressor is empty.
    fn is_empty(&self) -> bool;

    /// Returns an estimate of the current length of the compressed data.
    /// Flushing will increase the accuracy at the expense of a poorer compression ratio.
    fn len(&self) -> usize;

    /// Flushes any uncompressed data to the compression buffer.
    /// This will result in a non-optimal compression ratio.
    fn flush(&mut self) -> Result<()>;

    /// Returns if the compressor is full.
    fn is_full(&self) -> bool;

    /// Writes data to the compressor.
    /// Returns the number of bytes written.
    fn write(&mut self, data: &[u8]) -> Result<usize>;

    /// Reads data from the compressor.
    /// This should only be called after the compressor has been closed.
    /// Returns the number of bytes read.
    fn read(&mut self, data: &mut [u8]) -> Result<usize>;

    /// Closes the compressor.
    /// This should be called before reading any data.
    fn close(&mut self) -> Result<()>;
}
