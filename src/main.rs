use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc::{channel, Receiver};
use torrent::{peer::PeerSession, queues::WorkResult, request_peer_info, Torrent};
use tracing::{debug, info};
use tracing_subscriber::prelude::*;

use structopt::StructOpt;

const PEER_ID: &[u8; 20] = b"-TR2940-k8hj0wgej6ch";
const PORT: u16 = 6881;

#[derive(Debug, StructOpt)]
struct Opt {
    torrent: PathBuf,
}

fn init_tracing() {
    tracing_subscriber::fmt::init();
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();
    let opt = Opt::from_args();

    let file = tokio::fs::read(opt.torrent).await?;
    let torrent = Torrent::from_bytes(&file)?;

    let details = request_peer_info(&torrent, PEER_ID, PORT).await?;

    let mut handles = Vec::new();

    let (save_tx, save_rx) = channel(50);

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

    let save_handle = tokio::spawn(save_results(save_rx, piece_count));

    for handle in handles {
        handle.await??;
    }
    save_handle.await?;

    Ok(())
}

#[tracing::instrument]
async fn save_results(mut save_rx: Receiver<WorkResult>, piece_count: usize) {
    let mut downloaded_count = 0;
    let mut total_bytes = 0;
    while let Some(result) = save_rx.recv().await {
        downloaded_count += 1;
        total_bytes += result.bytes.len();
        info!(
            "downloaded piece {} of {}: {} total bytes",
            downloaded_count, piece_count, total_bytes
        );
        if downloaded_count >= piece_count {
            info!("Download complete!");
            break;
        }
    }
}
