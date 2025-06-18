use std::{collections::HashSet, net::IpAddr};

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, BufReader, BufWriter, ReadHalf, WriteHalf},
    net::TcpStream,
};

use crate::{
    error::ApplicationError,
    protocol::{HANDSHAKE_LEN, Handshake, Message},
};

/// Represents a peer in the BitTorrent network
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Peer {
    pub ip:   IpAddr,
    pub port: u16,
}

/// Manages the connection to a peer, including reading and writing
pub struct PeerConnection<'a> {
    peer:             &'a Peer,
    choked:           bool,
    reader:           BufReader<ReadHalf<TcpStream>>,
    writer:           BufWriter<WriteHalf<TcpStream>>,
    available_pieces: HashSet<usize>,
}

impl<'a> PeerConnection<'a> {
    pub async fn connect(
        peer:      &'a Peer,
        info_hash: [u8; 20],
        peer_id:   [u8; 20],
    ) -> Result<Self, ApplicationError> {
        let stream = TcpStream::connect(format!("{}:{}", peer.ip, peer.port))
            .await
            .map_err(|e| ApplicationError::PeerError(e.to_string()))?;

        let (rh, wh) = tokio::io::split(stream);
        let reader   = BufReader::new(rh);
        let writer   = BufWriter::new(wh);

        let mut conn = PeerConnection {
            choked: true,
            peer,
            reader,
            writer,
            available_pieces: HashSet::new(),
        };

        conn.writer
            .write_all(&Handshake::new(info_hash, peer_id).encode())
            .await
            .map_err(|e| ApplicationError::PeerError(e.to_string()))?;

        conn.writer
            .flush()
            .await
            .map_err(|e| ApplicationError::PeerError(e.to_string()))?;

        let mut buf = [0u8; HANDSHAKE_LEN];
        conn.reader
            .read_exact(&mut buf)
            .await
            .map_err(|e| ApplicationError::PeerError(e.to_string()))?;

        let handshake = Handshake::decode(&buf)?;
        if handshake.info_hash != info_hash {
            return Err(ApplicationError::ProtocolError("invalid info_hash".into()));
        }

        Ok(conn)
    }

    pub fn available_pieces(&self) -> &HashSet<usize> {
        &self.available_pieces
    }

    pub async fn send_interested(&mut self) -> Result<(), ApplicationError> {
        self.writer
            .write_all(&Message::Interested.encode())
            .await
            .map_err(|e| ApplicationError::PeerError(e.to_string()))?;

        self.writer
            .flush()
            .await
            .map_err(|e| ApplicationError::PeerError(e.to_string()))
    }

    pub async fn read_messages(&mut self) -> Result<(), ApplicationError> {
        while let Some(msg) = Self::read_message(&mut self.reader).await? {

            /*
             * 
             * 
             * Read incoming messages
             * 
             * 
             */


            match msg {
                Message::Choke => {
                    return Err(ApplicationError::ProtocolError("peer choked us".into()));
                }
                Message::Unchoke => {
                    self.choked = false;
                }
                Message::Bitfield(bytes) => {
                    for (i, byte) in bytes.iter().enumerate() {
                        for bit in 0..8 {
                            if byte & (0b1000_0000 >> bit) != 0 {
                                self.available_pieces.insert(i * 8 + bit);
                            }
                        }
                    }
                }
                Message::Have(index) => {
                    self.available_pieces.insert(index as usize);
                }
                Message::Piece { index, begin, block } => {
                    println!(
                        "Received piece {} (offset {}), {} bytes",
                        index,
                        begin,
                        block.len()
                    );
                }
                _ => {}
            }
        }
        Ok(())
    }

    async fn read_message(
        reader: &mut BufReader<ReadHalf<TcpStream>>,
    ) -> Result<Option<Message>, ApplicationError> {
        let mut length = [0u8; 4];
        if reader.read_exact(&mut length).await.is_err() {
            return Ok(None);
        }

        let size = u32::from_be_bytes(length);
        if size == 0 {
            return Ok(None);
        }

        let mut msg_buf = vec![0u8; size as usize];
        reader
            .read_exact(&mut msg_buf)
            .await
            .map_err(|e| ApplicationError::PeerError(e.to_string()))?;

        let mut full_buf = length.to_vec();
        full_buf.extend_from_slice(&msg_buf);

        Message::decode(&full_buf)
    }
}
