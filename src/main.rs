use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc::channel;
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
    pretty_env_logger::init();
    let opt = Opt::from_args();

    let file = tokio::fs::read(opt.torrent).await?;
    let torrent = Torrent::from_bytes(&file)?;

    let details = request_peer_info(&torrent, PEER_ID, PORT).await?;

    let mut handles = Vec::new();

    let (save_tx, mut save_rx) = channel(50);

    let work_queue = torrent.work_queue().await?;

    let torrent = Arc::new(torrent);
    let piece_count = torrent.file.info.hash_pieces().len();

    for peer_data in details.peers.into_iter() {
        let torrent = Arc::clone(&torrent);
        let work_queue = work_queue.clone();
        let save_tx = save_tx.clone();
        let handle = tokio::spawn(async move {
            let mut session = PeerSession::new(peer_data, torrent, work_queue, save_tx, PEER_ID)
                .await?
                .connect()
                .await?;
            session.start_download().await?;

            Ok(()) as anyhow::Result<()>
        });

        handles.push(handle);
    }

    let save_handle = tokio::spawn(async move {
        let mut downloaded_count = 0_usize;
        while let Some(result) = save_rx.recv().await {
            println!(
                "Got work result: idx {}, len {} bytes",
                result.idx,
                result.bytes.len()
            );
            downloaded_count += 1;
            log::debug!("downloaded piece {} of {}", downloaded_count, piece_count);
            if downloaded_count >= piece_count {
                log::info!("Download complete!");
                break;
            }
        }
    });

    for handle in handles {
        handle.await??;
    }
    save_handle.await?;

    Ok(())
}
