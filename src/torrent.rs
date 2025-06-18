use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
use sha1::{Digest, Sha1};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use crate::error::ApplicationError;

/// Represents a parsed .torrent file
#[derive(Debug, Serialize, Deserialize)]
pub struct Torrent {
    pub announce: String,
    pub info:     Info,
    #[serde(skip)]
    pub info_raw_bytes: Vec<u8>,
}

/// Fields inside the 'info' dictionary of a .torrent file
#[derive(Debug, Serialize, Deserialize)]
pub struct Info {
    pub name: String,
    #[serde(rename = "piece length")]
    pub piece_length: i64,
    pub pieces: ByteBuf,
    pub length: Option<i64>,
    pub files:  Option<Vec<TorrentFile>>,
}

/// A file entry in a multi-file torrent
#[derive(Debug, Serialize, Deserialize)]
pub struct TorrentFile {
    pub length: i64,
    pub path:   Vec<String>,
}

/// Represents a file with its full path and length
#[derive(Debug)]
pub struct FileEntry {
    pub length: i64,
    pub path:   PathBuf,
}

impl Torrent {
    /// Reads a `.torrent` file from disk and parses it into a [`Torrent`] struct
    pub fn from_file(path: &str) -> Result<Self, ApplicationError> {

        // Read into buffer from file
        let data = fs::read(path)
            .map_err(|e| ApplicationError::TrackerError(format!("{}", e)))?;

        // Generate the map
        let bencoded_map: BTreeMap<String, serde_bencode::value::Value> =
            serde_bencode::from_bytes(&data)
                .map_err(|e| ApplicationError::TrackerError(format!("{}", e)))?;

        // Get the info
        let info_value = bencoded_map.get("info").ok_or_else(|| {
            ApplicationError::ParserError(format!("missing info"))
        })?;

        // Convert the info bytes and encode to bencode
        let info_raw_bytes = serde_bencode::to_bytes(info_value)
            .map_err(|e| ApplicationError::TrackerError(format!("{}", e)))?;

        // Geneerate the torrent object
        let torrent: Torrent = serde_bencode::from_bytes(&data)
            .map_err(|e| ApplicationError::TrackerError(format!("{}", e)))?;

        Ok(Torrent {
            info_raw_bytes,
            ..torrent
        })
    }

    /// Computes the SHA1 hash of the bencoded `info` dictionary
    pub fn info_hash(&self) -> [u8; 20] {
        let digest = Sha1::digest(&self.info_raw_bytes);
        let mut arr = [0u8; 20];
        arr.copy_from_slice(&digest);
        arr
    }

    // /// Returns the SHA1 info hash as a hexadecimal string
    // pub fn info_hash_hex(&self) -> String {
    //     hex::encode(self.info_hash())
    // }

    // /// Returns the name of the torrent (from the `info.name` field)
    // pub fn name(&self) -> &str {
    //     &self.info.name
    // }

    /// Calculates the total size of all files described by the torrent
    pub fn total_size(&self) -> i64 {
        self.files().iter().map(|f| f.length).sum()
    }

    // /// Returns `true` if the torrent is a multi-file torrent
    // pub fn is_multi_file(&self) -> bool {
    //     self.info.files.is_some()
    // }

    /// Returns all files in the torrent with their full paths and sizes
    pub fn files(&self) -> Vec<FileEntry> {
        if let Some(files) = &self.info.files {
            files
                .iter()
                .map(|f| FileEntry {
                    length: f.length,
                    path:   {
                        let mut pb = PathBuf::from(&self.info.name);
                        for p in &f.path {
                            pb.push(p);
                        }
                        pb
                    },
                })
                .collect()
        } else {
            vec![FileEntry {
                length: self.info.length.unwrap_or(0),
                path:   PathBuf::from(&self.info.name),
            }]
        }
    }

    /// Returns the number of pieces the torrent is divided into
    pub fn pieces_count(&self) -> usize {
        self.info.pieces.len() / 20
    }

    /// Returns the declared length of each piece (in bytes)
    ///
    /// The last piece may be shorter.
    pub fn piece_length(&self) -> i64 {
        self.info.piece_length
    }

    // /// Returns the SHA1 hash of each piece as a vector of `[u8; 20]`
    // pub fn piece_hashes(&self) -> Vec<[u8; 20]> {
    //     self.info
    //         .pieces
    //         .chunks(20)
    //         .filter_map(|chunk| {
    //             if chunk.len() == 20 {
    //                 let mut arr = [0u8; 20];
    //                 arr.copy_from_slice(chunk);
    //                 Some(arr)
    //             } else {
    //                 None
    //             }
    //         })
    //         .collect()
    // }

    // /// Maps each file in the torrent to the set of piece indices it spans
    // ///
    // /// This is useful for determining which pieces need to be downloaded
    // /// for each file.
    // pub fn file_piece_map(&self) -> Vec<(FileEntry, Vec<usize>)> {
    //     let files = self.files();
    //     let piece_len = self.piece_length() as usize;
    //     let mut offset = 0;

    //     files
    //         .into_iter()
    //         .map(|file| {
    //             let start = offset;
    //             let end = offset + file.length as usize;
    //             let first_piece = start / piece_len;
    //             let last_piece = (end.saturating_sub(1)) / piece_len;
    //             offset = end;

    //             let pieces: Vec<usize> = (first_piece..=last_piece).collect();
    //             (file, pieces)
    //         })
    //         .collect()
    // }

    pub fn log_info(&self) {
        println!("Torrent Info:");
        println!("  Name: {}", self.info.name);
        println!("  Announce URL: {}", self.announce);
        println!("  Piece Length: {} bytes", self.piece_length());
        println!("  Total Pieces: {}", self.pieces_count());
        println!("  Total Size: {} bytes", self.total_size());

        let files = self.files();
        println!("  Files ({}):", files.len());
        for file in files {
            println!("    - {} ({} bytes)", file.path.display(), file.length);
        }
    }
}

