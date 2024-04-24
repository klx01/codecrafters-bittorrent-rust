use clap::{Parser, Subcommand};
use crate::custom_bdecode::{decode_value_str};
use crate::custom_bencode::{json_encode_value};
use crate::torrent::{parse_torrent_from_file, Torrent, TorrentType};

mod custom_bdecode;
mod custom_bencode;
mod torrent;


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

fn get_info(path: &str) -> anyhow::Result<String> {
    let torrent = parse_torrent_from_file(path)?;
    let Torrent{ announce, info } = torrent;
    let length = match &info.torrent_type {
        TorrentType::SingleFile { length } => length,
        TorrentType::MultiFile { .. } => todo!("multi file torrents are not implemented yet")
    };
    let info_hash = info.get_info_hash()?;
    let piece_hashes = info.get_piece_hashes().collect::<Vec<_>>();
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
