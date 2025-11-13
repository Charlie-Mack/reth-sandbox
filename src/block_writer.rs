//! Minimal block file writer that stores RLP blobs alongside a tiny header.

use std::{
    fs::File,
    io::{BufWriter, Write},
    path::Path,
};

/// File format version for future compatibility
const FILE_FORMAT_VERSION: u8 = 1;

/// Magic bytes to identify the file format
const MAGIC_BYTES: &[u8] = b"RETH";

/// Distinguishes between raw Ethereum blocks and any future rollup variants.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
#[repr(u8)]
pub(crate) enum BlockType {
    Ethereum = 0,
    Optimism = 1,
}

/// Block file header
/// Metadata written once at the head of the block file so later tools can
/// verify compatibility.
#[derive(Debug)]
pub(crate) struct BlockFileHeader {
    version: u8,
    block_type: BlockType,
    from_block: u64,
    to_block: u64,
}

impl BlockFileHeader {
    /// Build a header describing the chain range contained in the file.
    pub fn new(is_optimism: bool, from_block: u64, to_block: u64) -> Self {
        Self {
            version: FILE_FORMAT_VERSION,
            block_type: if is_optimism {
                BlockType::Optimism
            } else {
                BlockType::Ethereum
            },
            from_block,
            to_block,
        }
    }

    /// Get the block type from the header
    pub(super) fn block_type(&self) -> BlockType {
        self.block_type
    }

    /// Serialize the header to the provided writer.
    fn write_to(&self, writer: &mut impl Write) -> eyre::Result<()> {
        writer.write_all(MAGIC_BYTES)?;
        writer.write_all(&[self.version])?;
        writer.write_all(&[self.block_type as u8])?;
        writer.write_all(&self.from_block.to_le_bytes())?;
        writer.write_all(&self.to_block.to_le_bytes())?;
        Ok(())
    }

    /// Read header from file (for the decoder)
    /// Parse a header from disk (used by decoders).
    fn read_from(reader: &mut impl std::io::Read) -> eyre::Result<Self> {
        let mut magic = [0u8; 4];
        reader.read_exact(&mut magic)?;
        if magic != MAGIC_BYTES {
            return Err(eyre::eyre!("Invalid file format"));
        }

        let mut version = [0u8; 1];
        reader.read_exact(&mut version)?;
        if version[0] != FILE_FORMAT_VERSION {
            return Err(eyre::eyre!("Unsupported file version: {}", version[0]));
        }

        let mut block_type = [0u8; 1];
        reader.read_exact(&mut block_type)?;

        let mut from_block = [0u8; 8];
        reader.read_exact(&mut from_block)?;
        let from_block = u64::from_le_bytes(from_block);

        let mut to_block = [0u8; 8];
        reader.read_exact(&mut to_block)?;
        let to_block = u64::from_le_bytes(to_block);

        Ok(Self {
            version: version[0],
            block_type: match block_type[0] {
                0 => BlockType::Ethereum,
                1 => BlockType::Optimism,
                _ => return Err(eyre::eyre!("Unknown block type: {}", block_type[0])),
            },
            from_block,
            to_block,
        })
    }
}

/// Streams block blobs to disk for later replay by `reth-bench`.
pub struct BlockFileWriter {
    writer: BufWriter<File>,
    blocks_written: usize,
}

impl BlockFileWriter {
    /// Create the file, write the header, and prepare buffered writes.
    pub fn new(path: &Path, header: BlockFileHeader) -> eyre::Result<Self> {
        let mut writer = BufWriter::new(File::create(path)?);
        header.write_to(&mut writer)?;

        Ok(Self {
            writer,
            blocks_written: 0,
        })
    }

    /// Write a single length-prefixed RLP blob to the output file.
    pub fn write_block(&mut self, rlp_data: &[u8]) -> eyre::Result<()> {
        self.writer
            .write_all(&(rlp_data.len() as u32).to_le_bytes())?;
        self.writer.write_all(rlp_data)?;
        self.blocks_written += 1;
        Ok(())
    }

    /// Flush the writer and return how many blocks were persisted.
    pub fn finish(mut self) -> eyre::Result<usize> {
        self.writer.flush()?;
        Ok(self.blocks_written)
    }
}
