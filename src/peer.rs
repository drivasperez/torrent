use crate::torrent_file::Torrent;
use bytes::{Buf, BufMut, BytesMut};
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_bytes::ByteBuf;
use std::sync::Arc;
use std::{convert::TryInto, net::Ipv4Addr};
use tokio::net::TcpStream;
use tokio_util::codec::{Decoder, Encoder, Framed};

const PROTOCOL_NAME: [u8; 19] = *b"BitTorrent protocol";

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

#[derive(Debug, Clone)]
pub struct Handshake {
    info_hash: [u8; 20],
    peer_id: [u8; 20],
    protocol_name: [u8; 19],
    reserved: [u8; 8],
}

struct HandshakeCodec;

impl Handshake {
    pub fn new(info_hash: &[u8; 20], peer_id: &[u8; 20]) -> Self {
        Self {
            info_hash: info_hash.to_owned(),
            peer_id: peer_id.to_owned(),
            protocol_name: PROTOCOL_NAME,
            reserved: [0_u8; 8],
        }
    }
}

impl Encoder<Handshake> for HandshakeCodec {
    type Error = std::io::Error;

    fn encode(&mut self, item: Handshake, dst: &mut BytesMut) -> Result<(), Self::Error> {
        dst.put_u8(item.protocol_name.len().try_into().unwrap());
        dst.extend_from_slice(&item.protocol_name);
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
        let mut protocol_name = [0; 19];
        src.copy_to_slice(&mut protocol_name);
        // reserved field
        let mut reserved = [0; 8];
        src.copy_to_slice(&mut reserved);
        // info hash
        let mut info_hash = [0; 20];
        src.copy_to_slice(&mut info_hash);
        // peer id
        let mut peer_id = [0; 20];
        src.copy_to_slice(&mut peer_id);

        Ok(Some(Handshake {
            info_hash,
            peer_id,
            protocol_name,
            reserved,
        }))
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

#[cfg(test)]
mod test {
    use super::*;
    use tokio_util::codec::Encoder;

    #[test]
    fn encode_handshake() {
        let handshake = Handshake::new(&[1u8; 20], b"Daniel Rivas12345678");
        let mut codec = HandshakeCodec;

        let mut bytes = BytesMut::with_capacity(50);
        codec.encode(handshake, &mut bytes).unwrap();

        assert_eq!(bytes.len(), 68);
    }
}
