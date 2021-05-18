use crate::torrent_file::Torrent;
use serde::Deserialize;
use serde_bytes::ByteBuf;
use std::net::Ipv4Addr;

mod handshake;
mod message;
mod session;
mod stream;

pub use handshake::*;
pub use message::*;
pub use session::*;

#[derive(Debug, Deserialize)]
struct TrackerResponse {
    interval: u16,
    peers: ByteBuf,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PeerData {
    ip: Ipv4Addr,
    port: u16,
}

impl PeerData {
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let ip = Ipv4Addr::new(bytes[0], bytes[1], bytes[2], bytes[3]);
        let port = u16::from_be_bytes([bytes[4], bytes[5]]);

        Self { ip, port }
    }
}

#[derive(Debug)]
pub struct PeersInfo {
    pub interval: u16,
    pub peers: Vec<PeerData>,
}

impl From<TrackerResponse> for PeersInfo {
    fn from(res: TrackerResponse) -> Self {
        let peers = res
            .peers
            .chunks_exact(6)
            .map(PeerData::from_bytes)
            .collect();

        Self {
            interval: res.interval,
            peers,
        }
    }
}

pub async fn request_peer_info(
    torrent: &Torrent,
    peer_id: &[u8],
    port: u16,
) -> anyhow::Result<PeersInfo> {
    let client: reqwest::Client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()?;
    let url = torrent.build_tracker_url(peer_id, port)?;

    let req = client.get(url).build()?;

    let tracker_response = client.execute(req).await?;

    let bytes = tracker_response.bytes().await?;
    let tracker_response: TrackerResponse = serde_bencode::from_bytes(&bytes)?;

    let details = tracker_response.into();
    Ok(details)
}
