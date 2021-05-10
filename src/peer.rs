use crate::torrent_file::Torrent;
use bytes::buf::Buf;
use bytes::BytesMut;
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_bytes::ByteBuf;
use std::{convert::TryInto, net::Ipv4Addr};
use tokio::net::TcpStream;
use tokio_util::codec::{Decoder, Encoder, Framed};

const PROTOCOL_NAME: &[u8] = b"BitTorrent protocol";

#[derive(Debug, Deserialize)]
struct TrackerResponse {
    interval: u16,
    peers: ByteBuf,
}

#[derive(Debug, Deserialize)]
pub struct Peer {
    ip: Ipv4Addr,
    port: u16,
}

impl std::fmt::Display for Peer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", &self.ip, &self.port)
    }
}

impl Peer {
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let ip = Ipv4Addr::new(bytes[0], bytes[1], bytes[2], bytes[3]);
        let port = u16::from_be_bytes([bytes[4], bytes[5]]);

        Self { ip, port }
    }

    pub async fn connect(&self, handshake: Handshake) -> anyhow::Result<()> {
        let stream = TcpStream::connect((self.ip, self.port)).await?;
        let mut stream = Framed::new(stream, HandshakeCodec);

        stream.send(handshake).await?;

        let n = stream.next().await;

        println!("{:?}", n);

        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct Handshake {
    info_hash: [u8; 20],
    peer_id: [u8; 20],
}

struct HandshakeCodec;

impl Handshake {
    pub fn new(info_hash: &[u8], peer_id: &[u8]) -> anyhow::Result<Self> {
        let s = Self {
            info_hash: info_hash.try_into()?,
            peer_id: peer_id.try_into()?,
        };

        Ok(s)
    }
}

impl Encoder<Handshake> for HandshakeCodec {
    type Error = std::io::Error;

    fn encode(&mut self, item: Handshake, dst: &mut BytesMut) -> Result<(), Self::Error> {
        dst.extend_from_slice(b"\x13");
        dst.extend_from_slice(PROTOCOL_NAME);
        dst.extend_from_slice(&[0u8; 8]);
        dst.extend_from_slice(&item.info_hash);
        dst.extend_from_slice(&item.peer_id);

        Ok(())
    }
}

impl Decoder for HandshakeCodec {
    type Item = Handshake;

    type Error = std::io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if src.is_empty() {
            return Ok(None);
        }

        let mut tmp_buf = src.clone();
        let prot_len = tmp_buf.get_u8() as usize;
        if prot_len != PROTOCOL_NAME.len() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Handshake must have the string \"BitTorrent protocol\"",
            ));
        }

        let payload_len = prot_len + 8 + 20 + 20;
        if src.remaining() > payload_len {
            // we have the full message in the buffer so advance the buffer
            // cursor past the message length header
            src.advance(1);
        } else {
            return Ok(None);
        }

        // protocol string
        let mut prot = [0; 19];
        src.copy_to_slice(&mut prot);
        // reserved field
        let mut reserved = [0; 8];
        src.copy_to_slice(&mut reserved);
        // info hash
        let mut info_hash = [0; 20];
        src.copy_to_slice(&mut info_hash);
        // peer id
        let mut peer_id = [0; 20];
        src.copy_to_slice(&mut peer_id);

        Ok(Some(Handshake { info_hash, peer_id }))
    }
}

#[derive(Debug)]
pub struct PeersInfo {
    pub interval: u16,
    pub peers: Vec<Peer>,
}

impl From<TrackerResponse> for PeersInfo {
    fn from(res: TrackerResponse) -> Self {
        let peers = res.peers.chunks_exact(6).map(Peer::from_bytes).collect();

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

    let details: PeersInfo = tracker_response.into();
    Ok(details)
}
