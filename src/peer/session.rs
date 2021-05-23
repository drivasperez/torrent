use super::message::PeerMessage;
use super::PeerData;
use super::{
    handshake::{Handshake, HandshakeCodec},
    stream::{make_message_stream, HandshakeStream, MessageStream},
};
use crate::queues::{WorkQueue, WorkResult};
use crate::Torrent;
use crate::{
    bitfield::{Bitfield, BitfieldMut},
    queues::PieceOfWork,
};
use anyhow::anyhow;
use futures::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::sync::mpsc::Sender;
use tokio::time::{self, Duration};
use tokio_util::codec::Framed;

const MAX_BLOCK_SIZE: usize = 16_384;
const MAX_BACKLOG: usize = 5;

#[derive(Debug)]
struct PieceState {
    index: usize,
    downloaded: usize,
    requested: usize,
    backlog: usize,
    buf: Vec<u8>,
}

impl PieceState {
    pub fn new(index: usize, len: usize) -> Self {
        Self {
            index,
            downloaded: 0,
            requested: 0,
            backlog: 0,
            buf: vec![0; len],
        }
    }
}

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
struct PeerConnection {
    data: PeerData,
    state: PeerSessionState,
    torrent: Arc<Torrent>,
    work_queue: WorkQueue,
    save_tx: Sender<WorkResult>,
    peer_id: [u8; 20],
    stream: HandshakeStream,
}

impl PeerConnection {
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
            stream,
            state: Default::default(),
        })
    }

    pub async fn connect(mut self) -> anyhow::Result<PeerSession> {
        log::debug!("Connecting to peer {}", self.data.ip);

        let handshake = Handshake::new(&self.torrent.info_hash, &self.peer_id);

        self.stream.send(handshake).await?;

        loop {
            let n = self.stream.next().await;
            match n {
                None => continue,
                Some(peer_shake) => {
                    if peer_shake?.info_hash == self.torrent.info_hash {
                        let Self {
                            data,
                            state,
                            torrent,
                            work_queue,
                            save_tx,
                            peer_id,
                            stream,
                        } = self;
                        return Ok(PeerSession {
                            data,
                            state,
                            torrent,
                            work_queue,
                            save_tx,
                            peer_id,
                            stream: make_message_stream(stream),
                        });
                    } else {
                        return Err(anyhow!("Not the same hash"));
                    }
                }
            }
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
    stream: MessageStream,
}

impl std::fmt::Display for PeerSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", &self.data.ip, &self.data.port)
    }
}

impl PeerSession {
    pub async fn connect(
        data: PeerData,
        torrent: Arc<Torrent>,
        work_queue: WorkQueue,
        save_tx: Sender<WorkResult>,
        peer_id: &[u8; 20],
    ) -> anyhow::Result<Self> {
        let connection = PeerConnection::new(data, torrent, work_queue, save_tx, peer_id).await?;
        let mut session = connection.connect().await?;

        if let PeerMessage::Bitfield(bitfield) = session.recv_message().await? {
            log::debug!("Got bitfield from peer, length 0x{:0x}", bitfield.len());
            session.state.bitfield = bitfield;

            Ok(session)
        } else {
            Err(anyhow!("Peer didn't send bitfield"))
        }
    }

    async fn send_message(&mut self, msg: PeerMessage) -> anyhow::Result<()> {
        log::debug!("Sending peer message: {}", &msg);

        self.stream.send(msg).await?;

        Ok(())
    }

    async fn recv_message(&mut self) -> anyhow::Result<PeerMessage> {
        loop {
            let timeout = time::sleep(Duration::from_secs(30));
            tokio::pin!(timeout);
            tokio::select! {
                _ = &mut timeout => {
                    log::error!("Timed out");
                    return Err(anyhow::anyhow!("Timed out while receiving message"));
                }
                n = self.stream.next() => match n {
                    None => continue,
                    Some(res) => {
                        let msg = res?;
                        if let PeerMessage::KeepAlive = msg {
                            continue;
                        }
                        log::debug!("Received peer message: {}", &msg);
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

        while let Ok(work) = self.work_queue.pop().await {
            if !self.state.bitfield.has_piece(work.idx) {
                self.work_queue.push(work).await?;
                continue;
            }

            let buf = self.attempt_download(&work).await?;

            // TODO: Make this a result?
            if !work.verify_buf(&buf) {
                log::warn!("Piece {} failed integrity check", work.idx);
                self.work_queue.push(work).await?;
                continue;
            }

            self.send_message(PeerMessage::Have(work.idx as u32))
                .await?;
            self.save_tx
                .send(WorkResult {
                    idx: work.idx,
                    bytes: buf,
                })
                .await?;
        }

        Ok(())
    }

    async fn send_request(
        &mut self,
        idx: usize,
        requested: usize,
        block_size: usize,
    ) -> anyhow::Result<()> {
        self.send_message(PeerMessage::Request(
            idx as u32,
            requested as u32,
            block_size as u32,
        ))
        .await
    }

    async fn attempt_download(&mut self, work: &PieceOfWork) -> anyhow::Result<Vec<u8>> {
        log::debug!(
            "Attempting download of piece {} from peer {}",
            work.idx,
            self.data.ip
        );
        let mut state = PieceState::new(work.idx, work.length);

        while state.downloaded < work.length {
            if !self.state.choked {
                while state.backlog < MAX_BACKLOG && state.requested < work.length {
                    let mut block_size = MAX_BLOCK_SIZE;

                    if work.length - state.requested < block_size {
                        block_size = work.length - state.requested;
                    }

                    self.send_request(work.idx, state.requested, block_size)
                        .await?;
                    state.backlog += 1;
                    state.requested += block_size;
                }
            }

            self.read_message(&mut state).await?;
        }

        Ok(state.buf)
    }
}
