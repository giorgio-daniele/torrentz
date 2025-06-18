/// Represents the current state of a block within a piece
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockState {
    /// The block has not been requested from any peer yet
    NotRequested,
    /// The block has been requested but not yet received
    Requested,
    /// The block has been successfully downloaded
    Downloaded,
}

/// A contiguous block of data within a piece
#[derive(Debug, Clone)]
pub struct Block {
    /// Offset (in bytes) from the start of the piece
    pub offset: usize,
    /// Length of the block in bytes
    pub length: usize,
    /// Current state of the block (not requested, requested, or downloaded)
    pub state: BlockState,
}

/// A piece of the torrent file, composed of one or more blocks
#[derive(Debug, Clone)]
pub struct Piece {
    /// Index of the piece (0-based)
    pub index: usize,
    /// List of blocks that make up this piece
    pub blocks: Vec<Block>,
}
