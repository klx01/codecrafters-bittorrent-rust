use std::fs::File;
use std::net::SocketAddrV4;
use std::str::FromStr;
use std::io::{Seek, SeekFrom, Write};
use std::path::Path;
use anyhow::{bail, Context};
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
    },
    Download {
        /// save location
        #[arg(short = 'o')]
        save_location: String,
        /// torrent file
        torrent_path: String,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let output = match cli.command {
        Command::Decode { value } => decode_command(value),
        Command::Info { path } => info_command(&path),
        Command::Peers { path } => peers_command(&path),
        Command::Handshake { torrent_path, peer_socket } => handshake_command(&torrent_path, &peer_socket),
        Command::DownloadPiece { save_location, torrent_path, piece } => download_piece_command(&torrent_path, piece, &save_location),
        Command::Download { save_location, torrent_path } => download_command(&torrent_path, &save_location),
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

fn download_command(torrent_path: &str, save_location: &str) -> anyhow::Result<String> {
    let torrent = parse_torrent_from_file(torrent_path)?;
    if !torrent.info.is_single_file() {
        bail!("only single file torrents are supported");
    }
    let peers = request_peers(&torrent)?;

    let file_length = torrent.info.get_length(); // this is correct only for single-file torrents!!!
    let mut file = create_file_with_reserved_size(save_location, file_length as u64)?;
    let mut peer = init_peer(&torrent, &peers.peers[0])?;

    let pieces_count = torrent.info.pieces.len() as u32;
    for piece in 0..pieces_count {
        let piece_info = torrent.info.get_piece_info(piece)?;
        let piece_data = peer.download_piece(piece_info)?;
        let start = piece * torrent.info.piece_length;
        file.seek(SeekFrom::Start(start as u64)).context("failed to seek file for write")?;
        file.write(&piece_data).context("failed to write data to file")?;
    }

    let ret = format!("Downloaded {torrent_path} to {save_location}");
    Ok(ret)
}

fn create_file_with_reserved_size(path: impl AsRef<Path>, file_size: u64) -> anyhow::Result<File> {
    let mut file = File::create(path)?;
    file.seek(SeekFrom::Start(file_size - 1)).context("failed to seek file for reserve")?;
    file.write(&[0]).context("failed to write file for reserve")?;
    Ok(file)
}

#[cfg(test)]
mod test {
    use std::io;
    use sha1::{Sha1, Digest};
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
        let output = download_piece_command("sample.torrent", 0, "download/test-piece-0")?;
        let expected = "Piece 0 downloaded to download/test-piece-0";
        assert_eq!(expected, output);
        Ok(())
    }

    #[test]
    fn test_download() -> anyhow::Result<()> {
        // tests are configured to be run in 1 thread, because there are errors when communicating with the same peer in parallel
        let file_path = "download/test";
        let output = download_command("sample.torrent", file_path)?;
        let expected = "Downloaded sample.torrent to download/test";
        assert_eq!(expected, output);
        
        let mut file = File::open(file_path)?;
        let mut hasher = Sha1::new();
        io::copy(&mut file, &mut hasher)?;
        let actual_hash = hasher.finalize();
        let actual_hash = hex::encode(actual_hash);
        assert_eq!("1577533193d6eaf67fa97e1d5bc9d1dfbe4f82e3", actual_hash);
        
        Ok(())
    }
}
