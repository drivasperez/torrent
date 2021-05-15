use async_channel::{RecvError, SendError};

#[derive(Debug, Clone)]
pub struct PieceOfWork {
    pub idx: usize,
    pub hash: [u8; 20],
    pub length: usize,
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
