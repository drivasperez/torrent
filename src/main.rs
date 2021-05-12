use std::path::PathBuf;
use std::sync::Arc;
use torrent::{
    peer::{PeerMessage, PeerSession},
    request_peer_info, Torrent,
};

use structopt::StructOpt;

const PEER_ID: &[u8; 20] = b"-TR2940-k8hj0wgej6ch";
const PORT: u16 = 6881;

#[derive(Debug, StructOpt)]
struct Opt {
    torrent: PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opt = Opt::from_args();

    let file = tokio::fs::read(opt.torrent).await?;
    let torrent = Torrent::from_bytes(&file)?;

    let details = request_peer_info(&torrent, PEER_ID, PORT).await?;

    let mut handles = Vec::new();

    let torrent = Arc::new(torrent);
    for peer_data in details.peers {
        let torrent = Arc::clone(&torrent);
        let handle = tokio::spawn(async move {
            let mut session = PeerSession::new(peer_data, torrent, PEER_ID).await?;
            session.connect().await?;
            session.send_message(PeerMessage::Unchoke).await?;
            session.send_message(PeerMessage::Interested).await?;
            let msg = session.recv_message().await?;
            println!("Message: {:#?}", msg);
            Ok(()) as anyhow::Result<()>
        });

        handles.push(handle);
    }

    for handle in handles {
        handle.await??;
    }

    Ok(())
}
