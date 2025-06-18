use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use std::io::{self, Read, Write};
use tokio::io::AsyncRead;

use crate::error::ApplicationError;

/// The BitTorrent protocol identifier string
pub const PROTOCOL_STR: &str = "BitTorrent protocol";

/// Length of the full handshake message (always 68 bytes)
pub const HANDSHAKE_LEN: usize = 68;

/// Represents a BitTorrent handshake message.
///
/// A handshake is the first message sent in a connection and is always 68 bytes.
/// It identifies the torrent being requested (`info_hash`) and the client (`peer_id`).
pub struct Handshake {
    /// SHA-1 hash of the info dictionary from the .torrent file
    pub info_hash: [u8; 20],
    /// 20-byte string used to identify the client
    pub peer_id: [u8; 20],
}

impl Handshake {
    /// Creates a new `Handshake` with the given `info_hash` and `peer_id`.
    pub fn new(info_hash: [u8; 20], peer_id: [u8; 20]) -> Self {
        Self { info_hash, peer_id }
    }

    /// Encodes the handshake into a 68-byte array.
    ///
    /// This array can be written directly to a TCP stream.
    pub fn encode(&self) -> [u8; HANDSHAKE_LEN] {
        let mut buf = [0u8; HANDSHAKE_LEN];
        buf[0] = PROTOCOL_STR.len() as u8;
        buf[1..1 + PROTOCOL_STR.len()].copy_from_slice(PROTOCOL_STR.as_bytes());
        // reserved bytes [1+len..1+len+8] are zero by default
        buf[28..48].copy_from_slice(&self.info_hash);
        buf[48..68].copy_from_slice(&self.peer_id);
        buf
    }

    /// Decodes a 68-byte handshake message.
    ///
    /// Returns a `Handshake` or an error if the format is invalid.
    pub fn decode(buf: &[u8]) -> Result<Self, ApplicationError> {
        if buf.len() != HANDSHAKE_LEN {
            return Err(ApplicationError::ParserError(
                "invalid handshake length".into(),
            ));
        }

        let pstrlen = buf[0] as usize;
        if pstrlen != PROTOCOL_STR.len() {
            return Err(ApplicationError::ParserError(
                "invalid protocol string length".into(),
            ));
        }

        if &buf[1..1 + pstrlen] != PROTOCOL_STR.as_bytes() {
            return Err(ApplicationError::ParserError(
                "invalid protocol string".into(),
            ));
        }

        let mut info_hash = [0u8; 20];
        info_hash.copy_from_slice(&buf[28..48]);

        let mut peer_id = [0u8; 20];
        peer_id.copy_from_slice(&buf[48..68]);

        Ok(Self { info_hash, peer_id })
    }
}

/// Represents a protocol message exchanged after the handshake.
///
/// These messages follow the BitTorrent peer wire protocol.
#[derive(Debug)]
pub enum Message {
    /// `choke` message: tells the peer it will not receive requests
    Choke,
    /// `unchoke` message: peer is allowed to request blocks
    Unchoke,
    /// `interested` message: client is interested in pieces from peer
    Interested,
    /// `not interested` message: client is not interested
    NotInterested,
    /// `have` message: peer has a specific piece
    Have(u32),
    /// `bitfield` message: bitmap of pieces the peer has
    Bitfield(Vec<u8>),
    /// `request` message: request a block of data
    Request { index: u32, begin: u32, length: u32 },
    /// `piece` message: sends a block of a piece
    Piece {
        index: u32,
        begin: u32,
        block: Vec<u8>,
    },
    /// `cancel` message: cancels a previously sent request
    Cancel { index: u32, begin: u32, length: u32 },
}

impl Message {
    /// Serializes a `Message` into a byte vector for transmission.
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        match self {
            Message::Choke => {
                buf.write_u32::<BigEndian>(1).unwrap();
                buf.write_u8(0).unwrap();
            }
            Message::Unchoke => {
                buf.write_u32::<BigEndian>(1).unwrap();
                buf.write_u8(1).unwrap();
            }
            Message::Interested => {
                buf.write_u32::<BigEndian>(1).unwrap();
                buf.write_u8(2).unwrap();
            }
            Message::NotInterested => {
                buf.write_u32::<BigEndian>(1).unwrap();
                buf.write_u8(3).unwrap();
            }
            Message::Have(index) => {
                buf.write_u32::<BigEndian>(5).unwrap();
                buf.write_u8(4).unwrap();
                buf.write_u32::<BigEndian>(*index).unwrap();
            }
            Message::Bitfield(bitfield) => {
                buf.write_u32::<BigEndian>((1 + bitfield.len()) as u32)
                    .unwrap();
                buf.write_u8(5).unwrap();
                buf.extend_from_slice(bitfield);
            }
            Message::Request {
                index,
                begin,
                length,
            } => {
                buf.write_u32::<BigEndian>(13).unwrap();
                buf.write_u8(6).unwrap();
                buf.write_u32::<BigEndian>(*index).unwrap();
                buf.write_u32::<BigEndian>(*begin).unwrap();
                buf.write_u32::<BigEndian>(*length).unwrap();
            }
            Message::Piece {
                index,
                begin,
                block,
            } => {
                buf.write_u32::<BigEndian>((9 + block.len()) as u32)
                    .unwrap();
                buf.write_u8(7).unwrap();
                buf.write_u32::<BigEndian>(*index).unwrap();
                buf.write_u32::<BigEndian>(*begin).unwrap();
                buf.extend_from_slice(block);
            }
            Message::Cancel {
                index,
                begin,
                length,
            } => {
                buf.write_u32::<BigEndian>(13).unwrap();
                buf.write_u8(8).unwrap();
                buf.write_u32::<BigEndian>(*index).unwrap();
                buf.write_u32::<BigEndian>(*begin).unwrap();
                buf.write_u32::<BigEndian>(*length).unwrap();
            }
        }
        buf
    }

    /// Parses a buffer into a `Message`.
    ///
    /// Returns `Ok(None)` if the message is a keep-alive (length 0).
    pub fn decode(mut buf: &[u8]) -> Result<Option<Self>, ApplicationError> {
        if buf.len() < 4 {
            return Err(ApplicationError::ParserError(
                "buffer too short to read length".into(),
            ));
        }

        let len = buf
            .read_u32::<BigEndian>()
            .map_err(|e| ApplicationError::ParserError(format!("protocol: {}", e)))?;

        if len == 0 {
            // Keep-alive message
            return Ok(None);
        }

        if buf.len() < len as usize {
            return Err(ApplicationError::ParserError(
                "incomplete message data".into(),
            ));
        }

        let id = buf
            .read_u8()
            .map_err(|e| ApplicationError::ParserError(format!("protocol: {}", e)))?;

        let payload_len = len as usize - 1;

        match id {
            0 => Ok(Some(Message::Choke)),
            1 => Ok(Some(Message::Unchoke)),
            2 => Ok(Some(Message::Interested)),
            3 => Ok(Some(Message::NotInterested)),
            4 => {
                if payload_len != 4 {
                    return Err(ApplicationError::ParserError(
                        "invalid have message length".into(),
                    ));
                }
                let index = buf
                    .read_u32::<BigEndian>()
                    .map_err(|e| ApplicationError::ParserError(format!("protocol: {}", e)))?;
                Ok(Some(Message::Have(index)))
            }
            5 => {
                let mut bitfield = vec![0u8; payload_len];
                buf.read_exact(&mut bitfield)
                    .map_err(|e| ApplicationError::ParserError(format!("protocol: {}", e)))?;
                Ok(Some(Message::Bitfield(bitfield)))
            }
            6 => {
                if payload_len != 12 {
                    return Err(ApplicationError::ParserError(
                        "invalid request message length".into(),
                    ));
                }
                let index = buf
                    .read_u32::<BigEndian>()
                    .map_err(|e| ApplicationError::ParserError(format!("protocol: {}", e)))?;
                let begin = buf
                    .read_u32::<BigEndian>()
                    .map_err(|e| ApplicationError::ParserError(format!("protocol: {}", e)))?;
                let length = buf
                    .read_u32::<BigEndian>()
                    .map_err(|e| ApplicationError::ParserError(format!("protocol: {}", e)))?;
                Ok(Some(Message::Request {
                    index,
                    begin,
                    length,
                }))
            }
            7 => {
                if payload_len < 8 {
                    return Err(ApplicationError::ParserError(
                        "invalid piece message length".into(),
                    ));
                }
                let index = buf
                    .read_u32::<BigEndian>()
                    .map_err(|e| ApplicationError::ParserError(format!("protocol: {}", e)))?;
                let begin = buf
                    .read_u32::<BigEndian>()
                    .map_err(|e| ApplicationError::ParserError(format!("protocol: {}", e)))?;
                let block_len = payload_len - 8;
                let mut block = vec![0u8; block_len];
                buf.read_exact(&mut block).map_err(|e| {
                    ApplicationError::ParserError(format!("failed to read piece block: {}", e))
                })?;
                Ok(Some(Message::Piece {
                    index,
                    begin,
                    block,
                }))
            }
            8 => {
                if payload_len != 12 {
                    return Err(ApplicationError::ParserError(
                        "invalid cancel message length".into(),
                    ));
                }
                let index = buf
                    .read_u32::<BigEndian>()
                    .map_err(|e| ApplicationError::ParserError(format!("protocol: {}", e)))?;
                let begin = buf
                    .read_u32::<BigEndian>()
                    .map_err(|e| ApplicationError::ParserError(format!("protocol: {}", e)))?;
                let length = buf
                    .read_u32::<BigEndian>()
                    .map_err(|e| ApplicationError::ParserError(format!("protocol: {}", e)))?;
                Ok(Some(Message::Cancel {
                    index,
                    begin,
                    length,
                }))
            }
            _ => Err(ApplicationError::ParserError(format!(
                "unknown message id: {}",
                id
            ))),
        }
    }
}
