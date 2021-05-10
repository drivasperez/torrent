use std::path::PathBuf;
use torrent::{peer::Handshake, request_peer_info, Torrent};

use structopt::StructOpt;

const PEER_ID: &[u8] = b"-TR2940-k8hj0wgej6ch";
const PORT: u16 = 6881;

#[derive(Debug, StructOpt)]
struct Opt {
    torrent: PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opt = Opt::from_args();

    let file = tokio::fs::read(opt.torrent).await?;
    let torrent: Torrent = serde_bencode::from_bytes(&file)?;

    let details = request_peer_info(&torrent, PEER_ID, PORT).await?;
    let hash = torrent.info.hash()?;
    let handshake = Handshake::new(PEER_ID, &hash)?;

    let peer = details.peers.first().unwrap();

    println!("Peer: {}", peer);

    let mut handles = Vec::new();

    for peer in details.peers {
        let handshake = handshake.clone();
        let handle = tokio::spawn(async move {
            peer.connect(handshake).await?;
            Ok(()) as anyhow::Result<()>
        });

        handles.push(handle);
    }

    for handle in handles {
        handle.await??;
    }

    Ok(())
}
