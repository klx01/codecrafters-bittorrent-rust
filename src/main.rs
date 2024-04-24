use std::fs::File;
use std::io::Read;
use anyhow::{bail, Context};
use clap::{Parser, Subcommand};
use sha1::{Sha1, Digest};
use crate::bdecode::{decode_value, decode_value_str};
use crate::bencode::{bencode_value, json_encode_value, Value};

mod bdecode;
mod bencode;


#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Decode {
        value: String,
    },
    Info {
        path: String,
    }
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Decode { value } => decode_command(value),
        Command::Info { path } => info_command(path),
    }
}

fn decode_command(value: String) -> anyhow::Result<()> {
    let value = decode_value_str(&value)?;
    let json = json_encode_value(value)?;
    println!("{json}");
    Ok(())
}

fn info_command(path: String) -> anyhow::Result<()> {
    let res = get_info(&path)?;
    println!("{res}");
    Ok(())
}

const HASH_RAW_LENGTH: usize = 20;

fn get_info(path: &str) -> anyhow::Result<String> {
    let mut file = File::open(path).context("failed to open file")?;
    let mut contents = vec![];
    file.read_to_end(&mut contents).context("failed to read file")?;

    let value = decode_value(&contents)?;
    let Value::Dict(dict) = value else {
        bail!("expected dictionary, got {}", value.get_variant_name());
    };
    let Some(announce) = dict.get("announce") else {
        bail!("did not find announce field");
    };
    let Value::Str(announce) = announce else {
        bail!("expected announce to be a string, got {}", announce.get_variant_name());
    };
    let announce = std::str::from_utf8(announce).context("announce is not a valid utf8")?;
    let Some(info_value) = dict.get("info") else {
        bail!("did not find info field");
    };
    let Value::Dict(info) = info_value else {
        bail!("expected info to be dictionary, got {}", info_value.get_variant_name());
    };
    let Some(length) = info.get("length") else {
        bail!("did not find length field in info");
    };
    let Value::Int(length) = length else {
        bail!("expected length field to be an int, got {}", length.get_variant_name());
    };
    if *length < 1 {
        bail!("expected length to be positive, got {length}");
    }
    let length = *length as usize;

    let mut hasher = Sha1::new();
    hasher.update(bencode_value(info_value));
    let output = hasher.finalize();
    let info_hash = hex::encode(output);

    let Some(piece_length) = info.get("piece length") else {
        bail!("did not find piece length field in info");
    };
    let Value::Int(piece_length) = piece_length else {
        bail!("expected piece length to be an int, got {}", piece_length.get_variant_name());
    };
    if *piece_length < 1 {
        bail!("expected piece length to be positive, got {piece_length}");
    }
    let piece_length = *piece_length as usize;
    if piece_length > length {
        bail!("piece length {piece_length} is larger than total length {length}");
    }
    let expected_piece_count = (length + piece_length - 1) / piece_length; // integer division with a ceil

    let Some(pieces) = info.get("pieces") else {
        bail!("did not find pieces field in info");
    };
    let Value::Str(pieces) = pieces else {
        bail!("expected pieces to be a string, got {}", pieces.get_variant_name());
    };
    let pieces_len = pieces.len();
    let actual_piece_count = pieces_len / HASH_RAW_LENGTH;
    let remainder = pieces_len % HASH_RAW_LENGTH;
    if remainder > 0 {
        bail!("pieces of total length {pieces_len} can not be divided into hashes of length {HASH_RAW_LENGTH}");
    }
    if actual_piece_count != expected_piece_count {
        bail!("count of hashes {actual_piece_count} does not match the count that is based on the piece length {expected_piece_count}");
    }
    let piece_hashes: Vec<[u8; HASH_RAW_LENGTH]> = pieces
        .chunks(HASH_RAW_LENGTH)
        .map(|chunk| chunk.try_into().expect("size validation was incorrect"))
        .collect();
    assert_eq!(actual_piece_count, piece_hashes.len(), "size validation was incorrect");
    
    let piece_hashes = piece_hashes
        .into_iter()
        .map(|hash| hex::encode(hash))
        .collect::<Vec<_>>();

    let res = format!(
"Tracker URL: {announce}
Length: {length}
Info Hash: {info_hash}
Piece Hashes:
{}", piece_hashes.join("\n"));
    Ok(res)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_info() -> anyhow::Result<()> {
        let info = get_info("sample.torrent")?;
        let expected =
"Tracker URL: http://bittorrent-test-tracker.codecrafters.io/announce
Length: 92063
Info Hash: d69f91e6b2ae4c542468d1073a71d4ea13879a7f
Piece Hashes:
e876f67a2a8886e8f36b136726c30fa29703022d
6e2275e604a0766656736e81ff10b55204ad8d35
f00d937a0213df1982bc8d097227ad9e909acc17";
        assert_eq!(expected, info);
        Ok(())
    }
}