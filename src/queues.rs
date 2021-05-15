use async_channel::{Recv, Send};

#[derive(Debug, Clone)]
pub struct PieceOfWork {
    pub idx: usize,
    pub hash: [u8; 20],
    pub length: usize,
}

#[derive(Debug, Clone)]
pub struct WorkResult {
    pub idx: u32,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct WorkQueue {
    pub tx: async_channel::Sender<PieceOfWork>,
    pub rx: async_channel::Receiver<PieceOfWork>,
}

impl WorkQueue {
    pub async fn pop(&self) -> Recv<'_, PieceOfWork> {
        self.rx.recv()
    }

    pub async fn push(&self, msg: PieceOfWork) -> Send<'_, PieceOfWork> {
        self.tx.send(msg)
    }
}
