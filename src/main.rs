use clap::{Parser, Subcommand};
use crate::custom_bdecode::{decode_value_str};
use crate::custom_bencode::{json_encode_value};
use crate::torrent::{parse_torrent_from_file, Torrent};
use crate::tracker::request_peers;

mod custom_bdecode;
mod custom_bencode;
mod torrent;
mod tracker;


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
    },
    Peers {
        path: String,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Decode { value } => decode_command(value),
        Command::Info { path } => info_command(path),
        Command::Peers { path } => peers_command(path),
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
    let length = info.get_length();
    let piece_length = info.piece_length;
    let info_hash = info.get_info_hash()?;
    let info_hash = hex::encode(info_hash);
    let piece_hashes = info.get_encoded_piece_hashes().collect::<Vec<_>>();
    let res = format!(
"Tracker URL: {announce}
Length: {length}
Info Hash: {info_hash}
Piece Length: {piece_length}
Piece Hashes:
{}", piece_hashes.join("\n"));
    Ok(res)
}

fn peers_command(path: String) -> anyhow::Result<()> {
    let res = get_peers(&path)?;
    println!("{res}");
    Ok(())
}

fn get_peers(path: &str) -> anyhow::Result<String> {
    let torrent = parse_torrent_from_file(path)?;
    let peers = request_peers(&torrent)?;
    let peers = peers.peers.iter().map(|addr| addr.to_string()).collect::<Vec<_>>();
    let res = format!("{}", peers.join("\n"));
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
Piece Length: 32768
Piece Hashes:
e876f67a2a8886e8f36b136726c30fa29703022d
6e2275e604a0766656736e81ff10b55204ad8d35
f00d937a0213df1982bc8d097227ad9e909acc17";
        assert_eq!(expected, info);
        Ok(())
    }

    #[test]
    fn test_peers() -> anyhow::Result<()> {
        let peers = get_peers("sample.torrent")?;
        let expected =
"165.232.33.77:51467
178.62.85.20:51489
178.62.82.89:51448";
        assert_eq!(expected, peers);
        Ok(())
    }
}
