use std::fs::File;
use std::net::SocketAddrV4;
use std::str::FromStr;
use std::io::Write;
use anyhow::Context;
use clap::{Parser, Subcommand};
use crate::custom_bdecode::{decode_value_str};
use crate::custom_bencode::{json_encode_value};
use crate::peer::init_peer;
use crate::torrent::{parse_torrent_from_file, Torrent};
use crate::tracker::request_peers;

mod custom_bdecode;
mod custom_bencode;
mod torrent;
mod tracker;
mod peer;

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
        /// torrent file
        path: String,
    },
    Peers {
        /// torrent file
        path: String,
    },
    Handshake {
        /// torrent file
        torrent_path: String,
        /// <ipv4>:<port>
        peer_socket: String,
    },
    #[command(name = "download_piece")]
    DownloadPiece {
        /// save location
        #[arg(short = 'o')]
        save_location: String,
        /// torrent file
        torrent_path: String,
        piece: u32,
    }
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let output = match cli.command {
        Command::Decode { value } => decode_command(value),
        Command::Info { path } => info_command(&path),
        Command::Peers { path } => peers_command(&path),
        Command::Handshake { torrent_path, peer_socket } => handshake_command(&torrent_path, &peer_socket),
        Command::DownloadPiece { save_location, torrent_path, piece } => download_piece_command(&torrent_path, piece, &save_location),
    }?;
    println!("{output}");
    Ok(())
}

fn decode_command(value: String) -> anyhow::Result<String> {
    let value = decode_value_str(&value)?;
    let json = json_encode_value(value)?;
    Ok(json)
}

fn info_command(path: &str) -> anyhow::Result<String> {
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

fn peers_command(path: &str) -> anyhow::Result<String> {
    let torrent = parse_torrent_from_file(path)?;
    let peers = request_peers(&torrent)?;
    let peers = peers.peers.iter().map(|addr| addr.to_string()).collect::<Vec<_>>();
    let res = format!("{}", peers.join("\n"));
    Ok(res)
}

fn handshake_command(path: &str, socket: &str) -> anyhow::Result<String> {
    let socket = SocketAddrV4::from_str(socket).context("failed to parse socket addr")?;
    let torrent = parse_torrent_from_file(path)?;
    let peer = init_peer(&torrent, &socket)?;
    let peer_id = hex::encode(peer.peer_id);
    let output = format!("Peer ID: {peer_id}");
    Ok(output)
}

fn download_piece_command(torrent_path: &str, piece: u32, save_location: &str) -> anyhow::Result<String> {
    let torrent = parse_torrent_from_file(torrent_path)?;
    let piece_info = torrent.info.get_piece_info(piece)?; 
    let peers = request_peers(&torrent)?;
    let mut peer = init_peer(&torrent, &peers.peers[0])?;
    let piece_data = peer.download_piece(piece_info)?;
    let mut save_file = File::create(save_location).context("failed to create file")?;
    save_file.write(&piece_data)?;
    let ret = format!("Piece {piece} downloaded to {save_location}");
    Ok(ret)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_info() -> anyhow::Result<()> {
        let info = info_command("sample.torrent")?;
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
        let peers = peers_command("sample.torrent")?;
        let expected =
"165.232.33.77:51467
178.62.85.20:51489
178.62.82.89:51448";
        assert_eq!(expected, peers);
        Ok(())
    }

    #[test]
    fn test_handshake() -> anyhow::Result<()> {
        let peers = handshake_command("sample.torrent", "165.232.33.77:51467")?;
        let expected = "Peer ID: 2d524e302e302e302d5af5c2cf488815c4a2fa7f";
        assert_eq!(expected, peers);
        Ok(())
    }

    #[test]
    fn test_download_piece() -> anyhow::Result<()> {
        let output = download_piece_command("sample.torrent", 0, "download/test")?;
        let expected = "Piece 0 downloaded to download/test";
        assert_eq!(expected, output);
        Ok(())
    }
}
