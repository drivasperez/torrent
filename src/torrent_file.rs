use anyhow::anyhow;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
use sha1::{Digest, Sha1};
use std::convert::TryFrom;
use std::{borrow::Cow, convert::TryInto};

use crate::queues::{PieceOfWork, WorkQueue};

#[derive(Debug, Deserialize)]
pub struct Node(String, i64);

#[derive(Debug, Deserialize, Serialize)]
pub struct File {
    pub path: Vec<String>,
    pub length: i64,
    #[serde(default)]
    pub md5sum: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Info {
    pub name: String,
    pub pieces: ByteBuf,
    #[serde(rename = "piece length")]
    pub piece_length: i64,
    #[serde(default)]
    pub md5sum: Option<String>,
    #[serde(default)]
    pub length: Option<i64>,
    #[serde(default)]
    pub files: Option<Vec<File>>,
    #[serde(default)]
    pub private: Option<u8>,
    #[serde(default)]
    pub path: Option<Vec<String>>,
    #[serde(default)]
    #[serde(rename = "root hash")]
    pub root_hash: Option<String>,
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

    pub fn piece_bounds(&self, index: usize) -> (usize, usize) {
        let length = self.piece_length as usize;
        let begin = index * length;
        let mut end = begin + length;

        if end > self.length.unwrap() as usize {
            end = self.length.unwrap() as usize;
        }

        (begin, end)
    }

    pub fn piece_length(&self, index: usize) -> usize {
        let (begin, end) = self.piece_bounds(index);
        end - begin
    }
}

#[derive(Debug, Deserialize)]
pub struct TorrentFile {
    pub info: Info,
    #[serde(default)]
    pub announce: Option<String>,
    #[serde(default)]
    pub nodes: Option<Vec<Node>>,
    #[serde(default)]
    pub encoding: Option<String>,
    #[serde(default)]
    pub httpseeds: Option<Vec<String>>,
    #[serde(default)]
    #[serde(rename = "announce-list")]
    pub announce_list: Option<Vec<Vec<String>>>,
    #[serde(default)]
    #[serde(rename = "creation date")]
    pub creation_date: Option<i64>,
    #[serde(rename = "comment")]
    pub comment: Option<String>,
    #[serde(default)]
    #[serde(rename = "created by")]
    pub created_by: Option<String>,
}

#[derive(Debug)]
pub struct Torrent {
    pub file: TorrentFile,
    pub info_hash: [u8; 20],
}

impl Torrent {
    pub fn build_tracker_url(&self, peer_id: &[u8], port: u16) -> anyhow::Result<Url> {
        let announce = self
            .file
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
                &(self.file.info.length.expect("No length given").to_string()),
            )
            .encoding_override(Some(&iso_8859_1_encode))
            .append_pair("info_hash", &iso_8859_1_decode(&self.info_hash))
            .append_pair("peer_id", &iso_8859_1_decode(peer_id));

        Ok(base)
    }

    pub fn from_bytes(bytes: &[u8]) -> anyhow::Result<Torrent> {
        let torrent: TorrentFile = serde_bencode::from_bytes(bytes)?;
        Ok(torrent.into())
    }

    pub async fn work_queue(&self) -> anyhow::Result<WorkQueue> {
        let pieces = self.file.info.hash_pieces();
        let (tx, rx) = async_channel::bounded(pieces.len());

        for (idx, hash) in pieces.into_iter().enumerate() {
            let length = self.file.info.piece_length(idx);
            tx.send(PieceOfWork {
                idx,
                hash: hash.try_into()?,
                length,
            })
            .await?;
        }

        Ok(WorkQueue { tx, rx })
    }
}

impl From<TorrentFile> for Torrent {
    fn from(file: TorrentFile) -> Self {
        let info_hash = file
            .info
            .hash()
            .expect("Couldn't get SHA1 hash for torrent info");
        Self { file, info_hash }
    }
}

fn iso_8859_1_decode(bytes: &[u8]) -> String {
    bytes.iter().map(|&byte| char::from(byte)).collect()
}

fn iso_8859_1_encode(string: &str) -> Cow<[u8]> {
    string
        .chars()
        .map(|c| u8::try_from(u32::from(c)).unwrap())
        .collect()
}
