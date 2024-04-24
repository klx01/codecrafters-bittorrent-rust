use std::fs::File;
use std::io::Read;
use anyhow::{bail, Context};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_bytes::ByteBuf;
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
    torrent_type: TorrentType,
    #[serde(rename = "piece length")]
    pub piece_length: usize,
    #[serde(deserialize_with = "deserialize_pieces", serialize_with = "serialize_pieces")]
    pub pieces: Vec<[u8; HASH_RAW_LENGTH]>,
}

fn deserialize_pieces<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<[u8; HASH_RAW_LENGTH]>, D::Error> {
    let pieces = ByteBuf::deserialize(deserializer)?;
    let pieces_len = pieces.len();
    if (pieces_len % HASH_RAW_LENGTH) != 0 {
        return Err(serde::de::Error::custom(format!("pieces of total length {pieces_len} can not be divided into hashes of length {HASH_RAW_LENGTH}")));
    }
    let pieces = pieces
        .chunks(HASH_RAW_LENGTH)
        .map(|x| x.try_into().expect("chunks should be of a correct length"))
        .collect();
    Ok(pieces)
}

fn serialize_pieces<S: Serializer>(v: &Vec<[u8; HASH_RAW_LENGTH]>, ser: S) -> Result<S::Ok, S::Error> {
    let bytes = v.iter().copied().flatten().collect::<Vec<_>>();
    ser.serialize_bytes(&bytes)
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
    pub fn get_info_hash(&self) -> anyhow::Result<[u8; 20]> {
        let info_encoded = serde_bencode::to_bytes(self).context("failed to encode info")?;
        let mut hasher = Sha1::new();
        hasher.update(info_encoded);
        let output = hasher.finalize();
        let output = output.try_into().context("failed to convert hash to slice")?;
        Ok(output)
    }

    pub fn get_encoded_piece_hashes(&self) -> impl Iterator<Item = String> + '_ {
        self.pieces
            .iter()
            .map(|hash| hex::encode(hash))
    }

    pub fn get_length(&self) -> usize {
        match self.torrent_type {
            TorrentType::SingleFile { length } => length,
            TorrentType::MultiFile { .. } => todo!("multi file is not implemented yet")
        }
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

    let length = info.get_length();
    if piece_length > length {
        bail!("piece length {piece_length} is larger than total length {length}");
    }
    let expected_piece_count = (length + piece_length - 1) / piece_length; // integer division with a ceil

    if info.pieces.len() != expected_piece_count {
        bail!("count of hashes {} does not match the count that is based on the piece length {expected_piece_count}", info.pieces.len());
    }
    Ok(torrent)
}
