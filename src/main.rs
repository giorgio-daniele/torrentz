use crate::{
    error::ApplicationError,
    manager::PieceManager,
    peer::{Peer, PeerConnection},
    piece::Piece,
    torrent::Torrent,
    tracker::Tracker,
};

use std::sync::Arc;
use tokio::{
    sync::{Mutex, Semaphore},
    task,
};

mod error;
mod manager;
mod peer;
mod piece;
mod protocol;
mod torrent;
mod tracker;

const BLOCK_SIZE: usize     = 16 * 1024;
const CONCURRENCY: usize    = 10;
const BATCH_SIZE: usize     = 20;
const PEER_ID: [u8; 20]    = *b"-RU0001-123456789010";

#[tokio::main]
async fn main() -> Result<(), ApplicationError> {
    // Load torrent file and fetch the peers
    let torrent = Torrent::from_file("test.torrent")?;
    let tracker = Tracker;
    let peers   = tracker.announce(&torrent).await?;

    // Log the torrent info
    torrent.log_info();

    if peers.is_empty() {
        return Err(ApplicationError::ProtocolError("no peers".into()));
    }

    // Initialize piece manager
    let manager  = PieceManager::new(&torrent, BLOCK_SIZE);
    let pieces   = Arc::new(Mutex::new(manager.pieces));
    let peers    = Arc::new(peers);
    let sem      = Arc::new(Semaphore::new(CONCURRENCY));
    let peer_idx = Arc::new(Mutex::new(0));
    let info_hash= torrent.info_hash();

    // Start the main download loop
    download_loop(pieces, peers, sem, peer_idx, info_hash).await;

    println!("Download complete!");
    Ok(())
}

async fn download_loop(
    pieces:   Arc<Mutex<Vec<Piece>>>,
    peers:    Arc<Vec<Peer>>,
    sem:      Arc<Semaphore>,
    peer_idx: Arc<Mutex<usize>>,
    info_hash:[u8; 20],
) {
    loop {
        // Get a batch of pieces to download
        let batch = get_batch(&pieces).await;
        if batch.is_empty() {
            break; // no more pieces to download
        }

        let permit         = sem.clone().acquire_owned().await.unwrap();
        let peers_clone    = peers.clone();
        let peer_idx_clone = peer_idx.clone();
        let batch_clone    = batch.clone();

        // Spawn a new task to handle the peer download
        task::spawn(async move {
            let peer = select_peer(&peers_clone, &peer_idx_clone).await;
            let _    = runtime(&peer, &batch_clone, info_hash, PEER_ID).await;
            drop(permit);
        });
    }

    // Wait for all ongoing downloads to finish by acquiring all permits
    for _ in 0..CONCURRENCY {
        sem.acquire().await.unwrap().forget();
    }
}

async fn get_batch(pieces: &Arc<Mutex<Vec<Piece>>>) -> Vec<Piece> {
    let mut lock = pieces.lock().await;
    if lock.is_empty() {
        vec![]
    } else {
        let count = BATCH_SIZE.min(lock.len());
        lock.drain(0..count).collect()
    }
}

async fn select_peer(peers: &Arc<Vec<Peer>>, peer_idx: &Arc<Mutex<usize>>) -> Peer {
    let mut idx = peer_idx.lock().await;
    let peer    = peers[*idx].clone();
    *idx       = (*idx + 1) % peers.len();
    peer
}

/// Handles a single peer connection: connect, handshake, interested, and read messages.
async fn runtime(
    peer:      &Peer,
    pieces:    &[Piece],
    info_hash: [u8; 20],
    peer_id:   [u8; 20],
) -> Result<(), ApplicationError> {
    let mut conn = PeerConnection::connect(peer, info_hash, peer_id).await?;

    println!(
        "Connected to {}:{}, downloading pieces from {} to {}",
        peer.ip,
        peer.port,
        pieces.first().unwrap().index,
        pieces.last().unwrap().index,
    );

    conn.send_interested().await?;

    // // Print pieces that peer has available
    // let available: Vec<_> = conn.available_pieces().iter().cloned().collect();
    // println!("Peer {} has pieces {:?}", peer.ip, available);

    Ok(())
}
