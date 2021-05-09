use anyhow::anyhow;
use percent_encoding::{percent_encode, NON_ALPHANUMERIC};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
use sha1::{Digest, Sha1};
use std::path::PathBuf;

use structopt::StructOpt;

// const PEER_ID: &str = "DANCOOLGUYTORRENT012";
const PEER_ID: &[u8] = b"-TR2940-k8hj0wgej6ch";
const PORT: u16 = 6881;

#[derive(Debug, StructOpt)]
struct Opt {
    torrent: PathBuf,
}

#[derive(Debug, Deserialize)]
struct TrackerResponse {
    interval: u16,
    peers: Vec<Peer>,
}

#[derive(Debug, Deserialize)]
struct Node(String, i64);

#[derive(Debug, Deserialize, Serialize)]
struct File {
    path: Vec<String>,
    length: i64,
    #[serde(default)]
    md5sum: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Peer {
    ip: std::net::Ipv4Addr,
    port: u16,
}

#[derive(Debug, Deserialize, Serialize)]
struct Info {
    name: String,
    pieces: ByteBuf,
    #[serde(rename = "piece length")]
    piece_length: i64,
    #[serde(default)]
    md5sum: Option<String>,
    #[serde(default)]
    length: Option<i64>,
    #[serde(default)]
    files: Option<Vec<File>>,
    #[serde(default)]
    private: Option<u8>,
    #[serde(default)]
    path: Option<Vec<String>>,
    #[serde(default)]
    #[serde(rename = "root hash")]
    root_hash: Option<String>,
}

impl Info {
    pub fn hash(&self) -> anyhow::Result<[u8; 20]> {
        let bytes = serde_bencode::ser::to_bytes(self)?;
        let result = Sha1::digest(&bytes);

        Ok(result.into())
    }

    pub fn hash_pieces(&self) -> std::slice::ChunksExact<u8> {
        self.pieces.chunks_exact(20)
    }
}

#[derive(Debug, Deserialize)]
struct Torrent {
    info: Info,
    #[serde(default)]
    announce: Option<String>,
    #[serde(default)]
    nodes: Option<Vec<Node>>,
    #[serde(default)]
    encoding: Option<String>,
    #[serde(default)]
    httpseeds: Option<Vec<String>>,
    #[serde(default)]
    #[serde(rename = "announce-list")]
    announce_list: Option<Vec<Vec<String>>>,
    #[serde(default)]
    #[serde(rename = "creation date")]
    creation_date: Option<i64>,
    #[serde(rename = "comment")]
    comment: Option<String>,
    #[serde(default)]
    #[serde(rename = "created by")]
    created_by: Option<String>,
}

impl Torrent {
    pub fn build_tracker_url(&self, peer_id: &[u8], port: u16) -> anyhow::Result<Url> {
        let announce = self
            .announce
            .as_ref()
            .ok_or_else(|| anyhow!("No announce found"))?;
        let mut base = Url::parse(&announce)?;

        base.query_pairs_mut()
            .append_pair("port", &format!("{}", port))
            .append_pair("uploaded", "0")
            .append_pair("downloaded", "0")
            .append_pair("compact", "1")
            .append_pair(
                "left",
                &(self.info.length.expect("No length given").to_string()),
            );

        let mut url_string = base.to_string();
        url_string.push_str(&format!(
            "&info_hash={}",
            percent_encode(&self.info.hash()?, NON_ALPHANUMERIC)
        ));
        url_string.push_str(&format!(
            "&peer_id={}",
            percent_encode(peer_id, NON_ALPHANUMERIC)
        ));

        Ok(Url::parse(&url_string)?)
    }
}

pub fn bytes_to_hex_string(bytes: &[u8]) -> Result<String, std::fmt::Error> {
    use core::fmt::Write;
    let mut s = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(s, "{:02x}", byte)?;
    }

    Ok(s)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opt = Opt::from_args();

    let file = tokio::fs::read(opt.torrent).await?;
    let torrent: Torrent = serde_bencode::from_bytes(&file)?;

    let client: reqwest::Client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()?;

    let url = torrent.build_tracker_url(PEER_ID, PORT)?;
    println!("URL: {}", &url);

    let req = client.get(url).build()?;

    let tracker_response = client.execute(req).await?;

    let bytes = tracker_response.bytes().await?;
    let tracker_response: TrackerResponse = serde_bencode::from_bytes(&bytes)?;

    println!("{:?}", &tracker_response);

    Ok(())
}
