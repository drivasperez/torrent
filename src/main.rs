use std::path::PathBuf;
use std::sync::Arc;
use torrent::{peer::PeerSession, request_peer_info, Torrent};

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
            let session = PeerSession::new(peer_data, torrent, PEER_ID);
            session.connect().await?;
            Ok(()) as anyhow::Result<()>
        });

        handles.push(handle);
    }

    for handle in handles {
        handle.await??;
    }

    Ok(())
}
