use anyhow::anyhow;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
use sha1::{Digest, Sha1};
use std::borrow::Cow;
use std::convert::TryFrom;

#[derive(Debug, Deserialize)]
pub struct Node(String, i64);

#[derive(Debug, Deserialize, Serialize)]
pub struct File {
    path: Vec<String>,
    length: i64,
    #[serde(default)]
    md5sum: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Info {
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
pub struct Torrent {
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
            )
            .encoding_override(Some(&iso_8859_1_encode))
            .append_pair("info_hash", &iso_8859_1_decode(&self.info.hash()?))
            .append_pair("peer_id", &iso_8859_1_decode(peer_id));

        Ok(base)
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
