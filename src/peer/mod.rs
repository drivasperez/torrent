use crate::torrent_file::Torrent;
use anyhow::anyhow;
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_bytes::ByteBuf;
use std::net::Ipv4Addr;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio_util::codec::{Framed, FramedParts};

mod handshake;
mod peermessage;

use handshake::{Handshake, HandshakeCodec};

use self::peermessage::PeerMessageCodec;

#[derive(Debug, Deserialize)]
struct TrackerResponse {
    interval: u16,
    peers: ByteBuf,
}

#[derive(Debug)]
enum PeerStream {
    Handshake(Option<Framed<TcpStream, HandshakeCodec>>),
    Message(Option<Framed<TcpStream, PeerMessageCodec>>),
}

impl PeerStream {
    pub fn get_handshake_framed(&mut self) -> &mut Framed<TcpStream, HandshakeCodec> {
        match self {
            Self::Handshake(stream) => {
                return stream.as_mut().unwrap();
            }
            Self::Message(stream) => {
                let stream = stream.take().unwrap();
                *self = Self::to_handshake(stream);
                if let Self::Handshake(Some(stream)) = self {
                    return stream;
                } else {
                    unreachable!();
                }
            }
        }
    }

    pub fn get_message_framed(&mut self) -> &mut Framed<TcpStream, PeerMessageCodec> {
        match self {
            Self::Message(stream) => {
                return stream.as_mut().unwrap();
            }
            Self::Handshake(stream) => {
                let stream = stream.take().unwrap();
                *self = Self::to_message(stream);
                if let Self::Message(Some(stream)) = self {
                    return stream;
                } else {
                    unreachable!();
                }
            }
        }
    }

    fn to_handshake<T>(stream: Framed<TcpStream, T>) -> Self {
        let old_parts = stream.into_parts();
        let mut new_parts = FramedParts::new(old_parts.io, HandshakeCodec);
        // reuse buffers of previous codec
        new_parts.read_buf = old_parts.read_buf;
        new_parts.write_buf = old_parts.write_buf;
        let stream = Framed::from_parts(new_parts);

        Self::Handshake(Some(stream))
    }

    fn to_message<T>(stream: Framed<TcpStream, T>) -> Self {
        let old_parts = stream.into_parts();
        let mut new_parts = FramedParts::new(old_parts.io, PeerMessageCodec);
        // reuse buffers of previous codec
        new_parts.read_buf = old_parts.read_buf;
        new_parts.write_buf = old_parts.write_buf;
        let stream = Framed::from_parts(new_parts);

        Self::Message(Some(stream))
    }
}

#[derive(Debug)]
pub struct PeerSession {
    data: PeerData,
    torrent: Arc<Torrent>,
    peer_id: [u8; 20],
    stream: PeerStream,
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
    pub async fn new(
        data: PeerData,
        torrent: Arc<Torrent>,
        peer_id: &[u8; 20],
    ) -> anyhow::Result<Self> {
        let stream = TcpStream::connect((data.ip, data.port)).await?;
        let stream = Framed::new(stream, HandshakeCodec);

        Ok(Self {
            data,
            torrent,
            peer_id: peer_id.to_owned(),
            stream: PeerStream::Handshake(Some(stream)),
        })
    }

    pub async fn connect(&mut self) -> anyhow::Result<()> {
        let stream = self.stream.get_handshake_framed();

        let handshake = Handshake::new(&self.torrent.info_hash, &self.peer_id);

        stream.send(handshake).await?;

        loop {
            let n = stream.next().await;
            match n {
                None => continue,
                Some(peer_shake) => {
                    if peer_shake?.info_hash == self.torrent.info_hash {
                        println!("They want the same thing");
                        return Ok(());
                    } else {
                        return Err(anyhow!("Not the same hash"));
                    }
                }
            }
        }
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
