use async_channel::{RecvError, SendError};
use sha1::{Digest, Sha1};

#[derive(Debug, Clone)]
pub struct PieceOfWork {
    pub idx: usize,
    pub hash: [u8; 20],
    pub length: usize,
}

impl PieceOfWork {
    pub fn verify_buf(&self, buf: &[u8]) -> bool {
        let digest: [u8; 20] = Sha1::digest(buf).into();

        self.hash == digest
    }
}

#[derive(Debug, Clone)]
pub struct WorkResult {
    pub idx: usize,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct WorkQueue {
    pub tx: async_channel::Sender<PieceOfWork>,
    pub rx: async_channel::Receiver<PieceOfWork>,
}

impl WorkQueue {
    pub async fn pop(&self) -> Result<PieceOfWork, RecvError> {
        self.rx.recv().await
    }

    pub async fn push(&self, msg: PieceOfWork) -> Result<(), SendError<PieceOfWork>> {
        self.tx.send(msg).await
    }
}
