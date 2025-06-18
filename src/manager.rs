use crate::piece::{Block, BlockState, Piece};
use crate::torrent::Torrent;

pub struct PieceManager {
    pub pieces: Vec<Piece>,
    pub len: usize,
    pub last_len: usize,
    pub block_size: usize,
}

impl PieceManager {
    pub fn new(torrent: &Torrent, block_size: usize) -> Self {
        let len = torrent.piece_length() as usize;
        let tot = torrent.total_size() as usize;
        let cnt = torrent.pieces_count();
        let last_len = if tot % len == 0 { len } else { tot % len };

        let pieces = (0..cnt)
            .map(|i| {
                let piece_size = if i == cnt - 1 { last_len } else { len };
                let blks = (0..piece_size)
                    .step_by(block_size)
                    .map(|off| {
                        let blen = std::cmp::min(block_size, piece_size - off);
                        Block {
                            offset: off,
                            length: blen,
                            state: BlockState::NotRequested,
                        }
                    })
                    .collect();

                Piece {
                    index: i,
                    blocks: blks,
                }
            })
            .collect();

        Self {
            pieces,
            len,
            last_len,
            block_size,
        }
    }

    pub fn mark_block_requested(&mut self, pidx: usize, boff: usize) {
        self.pieces
            .get_mut(pidx)
            .and_then(|p| p.blocks.iter_mut().find(|b| b.offset == boff))
            .filter(|b| matches!(b.state, BlockState::NotRequested))
            .map(|b| b.state = BlockState::Requested);
    }

    pub fn mark_block_downloaded(&mut self, pidx: usize, boff: usize) {
        self.pieces
            .get_mut(pidx)
            .and_then(|p| p.blocks.iter_mut().find(|b| b.offset == boff))
            .map(|b| b.state = BlockState::Downloaded);
    }

    pub fn is_piece_complete(&self, pidx: usize) -> bool {
        self.pieces
            .get(pidx)
            .map(|p| {
                p.blocks
                    .iter()
                    .all(|b| matches!(b.state, BlockState::Downloaded))
            })
            .unwrap_or(false)
    }

    pub fn needed_blocks(&self) -> Vec<(usize, usize)> {
        self.pieces
            .iter()
            .flat_map(|p| {
                p.blocks
                    .iter()
                    .filter(|b| matches!(b.state, BlockState::NotRequested))
                    .map(move |b| (p.index, b.offset))
            })
            .collect()
    }
}
