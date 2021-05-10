use std::path::PathBuf;
use torrent::{request_peer_info, Torrent};

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

    println!("{:?}", &details);

    Ok(())
}
