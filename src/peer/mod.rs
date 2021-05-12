use crate::torrent_file::Torrent;
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_bytes::ByteBuf;
use std::net::Ipv4Addr;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio_util::codec::Framed;

mod handshake;
mod peermessage;

use handshake::{Handshake, HandshakeCodec};

#[derive(Debug, Deserialize)]
struct TrackerResponse {
    interval: u16,
    peers: ByteBuf,
}

#[derive(Debug, Clone)]
pub struct PeerSession {
    data: PeerData,
    torrent: Arc<Torrent>,
    peer_id: [u8; 20],
}

#[derive(Debug, Clone, Deserialize)]
pub struct PeerData {
    ip: Ipv4Addr,
    port: u16,
}

impl std::fmt::Display for PeerSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", &self.data.ip, &self.data.port)
    }
}

impl PeerData {
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let ip = Ipv4Addr::new(bytes[0], bytes[1], bytes[2], bytes[3]);
        let port = u16::from_be_bytes([bytes[4], bytes[5]]);

        Self { ip, port }
    }
}

impl PeerSession {
    pub fn new(data: PeerData, torrent: Arc<Torrent>, peer_id: &[u8; 20]) -> Self {
        Self {
            data,
            torrent,
            peer_id: peer_id.to_owned(),
        }
    }

    pub async fn connect(&self) -> anyhow::Result<()> {
        let stream = TcpStream::connect((self.data.ip, self.data.port)).await?;
        let mut stream = Framed::new(stream, HandshakeCodec);

        let handshake = Handshake::new(&self.torrent.info_hash, &self.peer_id);

        stream.send(handshake).await?;

        let n = stream.next().await;

        println!("{:?}", n);

        Ok(())
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
