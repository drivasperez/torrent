use super::handshake::{Handshake, HandshakeCodec};
use super::message::PeerMessage;
use super::stream::PeerStream;
use super::PeerData;
use crate::bitfield::BitfieldMut;
use crate::queues::{WorkQueue, WorkResult};
use crate::Torrent;
use anyhow::anyhow;
use futures::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::sync::mpsc::Sender;
use tokio::time::{self, Duration};
use tokio_util::codec::Framed;

#[derive(Debug)]
struct PeerSessionState {
    index: usize,
    choked: bool,
    interested: bool,
    downloaded: usize,
    requested: usize,
    backlog: usize,
    buf: Vec<u8>,
    bitfield: Vec<u8>,
}

struct PieceState {
    index: usize,
    downloaded: usize,
    requested: usize,
    backlog: usize,
    buf: Vec<u8>,
}

impl Default for PeerSessionState {
    fn default() -> Self {
        Self {
            index: 0,
            choked: true,
            interested: false,
            downloaded: 0,
            requested: 0,
            backlog: 0,
            buf: Vec::default(),
            bitfield: Default::default(),
        }
    }
}

#[derive(Debug)]
pub struct PeerSession {
    data: PeerData,
    state: PeerSessionState,
    torrent: Arc<Torrent>,
    work_queue: WorkQueue,
    save_tx: Sender<WorkResult>,
    peer_id: [u8; 20],
    stream: PeerStream,
}

impl std::fmt::Display for PeerSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", &self.data.ip, &self.data.port)
    }
}

impl PeerSession {
    pub async fn new(
        data: PeerData,
        torrent: Arc<Torrent>,
        work_queue: WorkQueue,
        save_tx: Sender<WorkResult>,
        peer_id: &[u8; 20],
    ) -> anyhow::Result<Self> {
        let stream = TcpStream::connect((data.ip, data.port)).await?;
        let stream = Framed::new(stream, HandshakeCodec);

        Ok(Self {
            data,
            torrent,
            work_queue,
            save_tx,
            peer_id: peer_id.to_owned(),
            stream: PeerStream::Handshake(Some(stream)),
            state: Default::default(),
        })
    }

    pub async fn connect(&mut self) -> anyhow::Result<()> {
        log::trace!("Connecting to peer {}", self.data.ip);
        let stream = self.stream.get_handshake_framed();

        let handshake = Handshake::new(&self.torrent.info_hash, &self.peer_id);

        stream.send(handshake).await?;

        loop {
            let n = stream.next().await;
            match n {
                None => continue,
                Some(peer_shake) => {
                    if peer_shake?.info_hash == self.torrent.info_hash {
                        return Ok(());
                    } else {
                        return Err(anyhow!("Not the same hash"));
                    }
                }
            }
        }
    }

    async fn send_message(&mut self, msg: PeerMessage) -> anyhow::Result<()> {
        log::trace!("Sending peer message: {}", &msg);
        let stream = self.stream.get_message_framed();

        stream.send(msg).await?;

        Ok(())
    }

    async fn recv_message(&mut self) -> anyhow::Result<PeerMessage> {
        let stream = self.stream.get_message_framed();
        loop {
            let timeout = time::sleep(Duration::from_secs(30));
            tokio::pin!(timeout);
            tokio::select! {
                _ = &mut timeout => {
                    log::error!("Timed out");
                    return Err(anyhow::anyhow!("Timed out while receiving message"));
                }
                n = stream.next() => match n {
                    None => continue,
                    Some(res) => {
                        let msg = res?;
                        if let PeerMessage::KeepAlive = msg {
                            continue;
                        }
                        log::trace!("Received peer message: {}", &msg);
                        return Ok(msg);
                    }
                }

            }
        }
    }

    /// Receive a message from the peer and adjust session state accordingly.
    async fn read_message(&mut self, state: &mut PieceState) -> anyhow::Result<()> {
        let msg = self.recv_message().await?;
        match msg {
            PeerMessage::Choke => self.state.choked = true,
            PeerMessage::Unchoke => self.state.choked = false,
            PeerMessage::Have(idx) => self.state.bitfield.set_piece(idx as usize),
            PeerMessage::Bitfield(field) => self.state.bitfield = field,
            // TODO: If we have the piece, send it when requested
            PeerMessage::Request(_idx, _offset, _length) => {}
            PeerMessage::Piece(idx, offset, data) => {
                // TODO make these usizes at the codex level.
                let idx = idx as usize;
                let offset = offset as usize;

                if idx != state.index {
                    return Err(anyhow::anyhow!("Incorrect piece index"));
                }
                let len = data.len();

                if offset > state.buf.len() {
                    return Err(anyhow::anyhow!("Piece offset longer than buffer"));
                }

                if offset + len > state.buf.len() {
                    return Err(anyhow::anyhow!("Data too long for piece"));
                }

                use std::io::Write;
                (&mut state.buf[offset..]).write_all(&data)?;
                state.downloaded += len;
                state.backlog -= 1;
            }
            _ => {}
        };

        Ok(())
    }

    pub async fn start_download(&mut self) -> anyhow::Result<()> {
        self.send_message(PeerMessage::Unchoke).await?;
        self.send_message(PeerMessage::Interested).await?;
        let msg = self.recv_message().await?;

        log::info!("Message: {}", msg);

        Ok(())
    }
}
