use std::fs::File;
use std::io::Read;
use anyhow::{bail, Context};
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};

pub(crate) const HASH_RAW_LENGTH: usize = 20;

#[derive(Deserialize)]
pub(crate) struct Torrent {
    pub announce: String,
    pub info: TorrentInfo,
}
#[derive(Deserialize, Serialize)]
pub(crate) struct TorrentInfo {
    pub name: String,
    #[serde(flatten)]
    pub torrent_type: TorrentType,
    #[serde(rename = "piece length")]
    pub piece_length: usize,
    #[serde(with = "serde_bytes")]
    pieces: Vec<u8>,
}

#[derive(Deserialize, Serialize)]
#[serde(untagged)]
pub(crate) enum TorrentType {
    SingleFile{
        length: usize,
    },
    MultiFile{
        files: TorrentFile,
    },
}

#[derive(Deserialize, Serialize)]
pub(crate) struct TorrentFile {
    length: usize,
    path: String,
}

impl TorrentInfo {
    pub fn get_info_hash(&self) -> anyhow::Result<String> {
        let info_encoded = serde_bencode::to_bytes(self).context("failed to encode info")?;
        let mut hasher = Sha1::new();
        hasher.update(info_encoded);
        let output = hasher.finalize();
        let info_hash = hex::encode(output);
        Ok(info_hash)
    }

    pub fn get_piece_hashes(&self) -> impl Iterator<Item = String> + '_ {
        self.pieces
            .chunks(HASH_RAW_LENGTH)
            .map(|hash| hex::encode(hash))
    }
}

pub(crate) fn parse_torrent_from_file(path: &str) -> anyhow::Result<Torrent> {
    let mut file = File::open(path).context("failed to open file")?;
    let mut contents = vec![];
    file.read_to_end(&mut contents).context("failed to read file")?;
    parse_torrent(&contents)
}

pub(crate) fn parse_torrent(data: &[u8]) -> anyhow::Result<Torrent> {
    let torrent: Torrent = serde_bencode::from_bytes(&data).context("failed to decode torrent struct")?;
    let info = &torrent.info;
    let piece_length = info.piece_length;
    
    let expected_piece_count = match &info.torrent_type {
        TorrentType::SingleFile { length } => {
            let length = *length;
            if piece_length > length {
                bail!("piece length {piece_length} is larger than total length {length}");
            }
            (length + piece_length - 1) / piece_length // integer division with a ceil
        }
        TorrentType::MultiFile { .. } => todo!("multi file is not implemented yet")
    };

    let pieces = &info.pieces;
    let pieces_len = pieces.len();
    let actual_piece_count = pieces_len / HASH_RAW_LENGTH;
    let remainder = pieces_len % HASH_RAW_LENGTH;
    if remainder > 0 {
        bail!("pieces of total length {pieces_len} can not be divided into hashes of length {HASH_RAW_LENGTH}");
    }
    if actual_piece_count != expected_piece_count {
        bail!("count of hashes {actual_piece_count} does not match the count that is based on the piece length {expected_piece_count}");
    }
    Ok(torrent)
}
