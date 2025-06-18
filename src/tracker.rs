use crate::error::ApplicationError;
use crate::peer::Peer;
use crate::torrent::Torrent;
use reqwest::Client;
use serde::Deserialize;
use serde_bencode::de;
use serde_bencode::value::{Value};
use std::net::{IpAddr, Ipv4Addr};
use url::Url;

/// Handles communication with a BitTorrent tracker
pub struct Tracker;

/// Represents the response returned by a tracker announce request
#[derive(Debug, Deserialize)]
pub struct AnnounceResponse {
    #[serde(rename = "peers")]
    pub peers_data: Value,
    pub interval:   Option<i64>,
}

impl AnnounceResponse {

    pub fn peers(&self) -> Vec<Peer> {
        let mut result = Vec::new();

        match &self.peers_data {

            Value::Bytes(data) => {

                /*
                 * This block handles the "compact" peer list format.
                 * In compact mode, peers are represented as a single 
                 * byte string.
                 * 
                 * Each peer entry is 6 bytes long:
                 * - The first 4 bytes represent the IPv4 address.
                 * - The next  2 bytes represent the port number (in 
                 *   network byte order, big-endian).
                 *
                 * Example:
                 * A peer with IP 192.168.1.1 and port 6881 would be represented as:
                 * [192, 168, 1, 1, 26, 225]
                 * (where 26 * 256 + 225 = 6881)
                 *
                 * The code iterates through the `data` byte string in chunks 
                 * of 6 bytes. For each valid 6-byte chunk, it extracts the 
                 * IPv4 address and port, then creates a `Peer` struct and adds 
                 * it to the `result` vector.
                 */


                for chunk in data.chunks(6) {
                    if chunk.len() == 6 {
                        let ip   = Ipv4Addr::new(chunk[0], chunk[1], chunk[2], chunk[3]);
                        let port = u16::from_be_bytes([chunk[4], chunk[5]]);
                        result.push(Peer {
                            ip:   IpAddr::V4(ip),
                            port,
                        });
                    }
                }
            }
            Value::List(list) => {


                /*
                 * This block handles the "non-compact" (or dictionary) 
                 * peer list format.
                 * In this mode, peers are represented as a Bencoded list 
                 * of Bencoded dictionaries.
                 * 
                 * Each dictionary typically contains the following keys:
                 * - "ip": (Byte String) The peer's IPv4 address as a 
                 *    dotted-decimal string (e.g., "192.168.1.1").
                 * - "port": (Integer) The peer's port number.
                 *
                 * The code iterates through each 'item' in the 'list'.
                 * It expects each 'item' to be a 'Value::Dict'.
                 * Inside each dictionary, it attempts to extract the 
                 * "ip" and "port" values.
                 * 
                 * - "ip" is parsed from a byte string to a UTF-8 string, 
                 *    then to an Ipv4Addr.
                 * - "port" is cast from an integer, with a range check 
                 *    to ensure it fits in u16.
                 * 
                 * If both IP and port are successfully extracted, a 'Peer' 
                 * struct is created and added to the 'result' vector.
                 */

                for item in list {
                    if let Value::Dict(dict) = item {

                        // Get the IP string
                        let ip = dict.get(&b"ip".to_vec())
                            .and_then(|v| match v {
                                Value::Bytes(b) => String::from_utf8(b.clone()).ok(),
                                           _    => None,
                            })
                            .and_then(|s| s.parse::<Ipv4Addr>().ok())
                            .map(IpAddr::V4);
                        
                        // Get the port string
                        let port = dict.get(&b"port".to_vec())
                            .and_then(|v| match v {
                                Value::Int(n)   => Some(*n as u16),
                                           _    => None,
                            });
                        
                        // Add the result
                        if let (Some(ip), Some(port)) = (ip, port) {
                            result.push(Peer { 
                                ip, 
                                port 
                            });
                        }
                    }
                }
            }
            _ => {}
        }
        result
    }
}

impl Tracker {
    /// A fixed peer ID used to identify the client
    const PEER_ID: [u8; 20] = *b"-RU0001-123456789010";

    fn percent_encode(bytes: &[u8; 20]) -> String {
        bytes.iter().map(|b| format!("%{:02X}", b)).collect()
    }

    /// Sends an announce request to the tracker and returns the list of peers
    pub async fn announce(&self, torrent: &Torrent) -> Result<Vec<Peer>, ApplicationError> {
        let announce   = &torrent.announce;
        let info_hash  = &torrent.info_hash();
        let peer_id    = &Self::PEER_ID;
        let uploaded   = 0u64;
        let downloaded = 0u64;
        let left       = torrent.total_size() as u64;
        let port       = 6881u16;

        let base_url = Url::parse(announce)
            .map_err(|e| ApplicationError::TrackerError(format!("{}", e)))?;

        let params = [
            ("info_hash",  Tracker::percent_encode(info_hash)),
            ("peer_id",    Tracker::percent_encode(peer_id)),
            ("port",       port.to_string()),
            ("uploaded",   uploaded.to_string()),
            ("downloaded", downloaded.to_string()),
            ("left",       left.to_string()),
            ("event",      "started".to_string()),
        ];

        let query = params
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect::<Vec<_>>()
            .join("&");

        let url = format!("{}?{}", base_url, query);

        let client = Client::new();
        let raw = client
            .get(&url)
            .send()
            .await
            .map_err(|e| ApplicationError::TrackerError(format!("{}", e)))?
            .bytes()
            .await
            .map_err(|e| ApplicationError::TrackerError(format!("{}", e)))?;

        let resp: AnnounceResponse = de::from_bytes(&raw)
            .map_err(|e| ApplicationError::TrackerError(format!("{}", e)))?;

        Ok(resp.peers())
    }
}
