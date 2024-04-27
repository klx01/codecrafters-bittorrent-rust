use std::cmp;
use std::io::SeekFrom;
use std::net::SocketAddrV4;
use std::ops::Deref;
use std::str::FromStr;
use std::path::Path;
use std::sync::Arc;
use anyhow::{bail, Context};
use clap::{Parser, Subcommand};
use tokio::fs::File;
use tokio::io::{AsyncWriteExt, AsyncSeekExt};
use tokio::sync::Mutex;
use tokio::task::JoinSet;
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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let output = match cli.command {
        Command::Decode { value } => decode_command(value),
        Command::Info { path } => info_command(&path).await,
        Command::Peers { path } => peers_command(&path).await,
        Command::Handshake { torrent_path, peer_socket } => handshake_command(&torrent_path, &peer_socket).await,
        Command::DownloadPiece { save_location, torrent_path, piece } => download_piece_command(&torrent_path, piece, &save_location).await,
        Command::Download { save_location, torrent_path } => download_command(&torrent_path, &save_location).await,
    }?;
    println!("{output}");
    Ok(())
}

fn decode_command(value: String) -> anyhow::Result<String> {
    let value = decode_value_str(&value)?;
    let json = json_encode_value(value)?;
    Ok(json)
}

async fn info_command(path: &str) -> anyhow::Result<String> {
    let torrent = parse_torrent_from_file(path).await?;
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

async fn peers_command(path: &str) -> anyhow::Result<String> {
    let torrent = parse_torrent_from_file(path).await?;
    let peers = request_peers(&torrent).await?;
    let peers = peers.peers.iter().map(|addr| addr.to_string()).collect::<Vec<_>>();
    let res = format!("{}", peers.join("\n"));
    Ok(res)
}

async fn handshake_command(path: &str, socket: &str) -> anyhow::Result<String> {
    let socket = SocketAddrV4::from_str(socket).context("failed to parse socket addr")?;
    let torrent = parse_torrent_from_file(path).await?;
    let info_hash = torrent.info.get_info_hash()?;
    let peer = init_peer(info_hash, &socket).await?;
    let peer_id = hex::encode(peer.peer_id);
    let output = format!("Peer ID: {peer_id}");
    Ok(output)
}

async fn download_piece_command(torrent_path: &str, piece: u32, save_location: &str) -> anyhow::Result<String> {
    let torrent = parse_torrent_from_file(torrent_path).await?;
    let piece_info = torrent.info.get_piece_info(piece)?;
    let peers = request_peers(&torrent).await?;
    let info_hash = torrent.info.get_info_hash()?;
    let mut peer = init_peer(info_hash, &peers.peers[0]).await?;
    let piece_data = peer.download_piece(piece_info).await?;
    let mut save_file = File::create(save_location).await.context("failed to create file")?;
    save_file.write(&piece_data).await?;
    let ret = format!("Piece {piece} downloaded to {save_location}");
    Ok(ret)
}

async fn download_command(torrent_path: &str, save_location: &str) -> anyhow::Result<String> {
    let torrent = parse_torrent_from_file(torrent_path).await?;
    if !torrent.info.is_single_file() {
        bail!("only single file torrents are supported");
    }
    let info_hash = torrent.info.get_info_hash()?;
    let peers = request_peers(&torrent).await?;

    let file_length = torrent.info.get_length(); // this is correct only for single-file torrents!!!
    let file = create_file_with_reserved_size(save_location, file_length as u64).await?;
    let file = Arc::new(Mutex::new(file));

    let pieces = torrent.info.get_all_pieces_info().collect::<Vec<_>>();
    let pieces = Arc::new(std::sync::Mutex::new(pieces));
    let pieces_count = torrent.info.pieces.len();
    let peers_count = peers.peers.len();
    let threads_count = cmp::min(pieces_count, peers_count);
    let mut join_set = JoinSet::new();
    for thread_no in 0..threads_count {
        let socket = peers.peers[thread_no];
        let file = file.clone();
        let pieces = pieces.clone();
        join_set.spawn(async move {
            let mut peer = init_peer(info_hash, &socket).await?;
            while let Some(piece_info) = pop_mutex_vec(pieces.deref()) {
                let file_start_pos = piece_info.file_start_pos;
                // todo: maybe download blocks of the same piece in parallel too
                let piece_data = peer.download_piece(piece_info).await?;
                // todo: maybe dump data into multiple files, and then assemble
                let mut file_guard = file.lock().await;
                file_guard.seek(SeekFrom::Start(file_start_pos as u64)).await.context("failed to seek file for write")?;
                file_guard.write(&piece_data).await.context("failed to write data to file")?;
                drop(file_guard);
            }
            Ok(())
        });
    }

    while let Some(result) = join_set.join_next().await {
        // all futures should be dropped when JoinSet is dropped, so it's ok to just exit
        let result: anyhow::Result<()> = result.context("join error")?;
        result?
    }

    let ret = format!("Downloaded {torrent_path} to {save_location}");
    Ok(ret)
}

async fn create_file_with_reserved_size(path: impl AsRef<Path>, file_size: u64) -> anyhow::Result<File> {
    let mut file = File::create(path).await?;
    file.seek(SeekFrom::Start(file_size - 1)).await.context("failed to seek file for reserve")?;
    file.write(&[0]).await.context("failed to write file for reserve")?;
    Ok(file)
}

fn pop_mutex_vec<T>(mutex_vec: &std::sync::Mutex<Vec<T>>) -> Option<T> {
    mutex_vec.lock().expect("poisoned lock").pop()
}

#[cfg(test)]
mod test {
    use std::io;
    use sha1::{Sha1, Digest};
    use super::*;

    #[tokio::test]
    async fn test_info() -> anyhow::Result<()> {
        let info = info_command("sample.torrent").await?;
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

    #[tokio::test]
    async fn test_peers() -> anyhow::Result<()> {
        let peers = peers_command("sample.torrent").await?;
        let expected =
"165.232.33.77:51467
178.62.85.20:51489
178.62.82.89:51448";
        assert_eq!(expected, peers);
        Ok(())
    }

    #[tokio::test]
    async fn test_handshake() -> anyhow::Result<()> {
        let peers = handshake_command("sample.torrent", "165.232.33.77:51467").await?;
        let expected = "Peer ID: 2d524e302e302e302d5af5c2cf488815c4a2fa7f";
        assert_eq!(expected, peers);
        Ok(())
    }

    #[tokio::test]
    async fn test_download_piece() -> anyhow::Result<()> {
        let output = download_piece_command("sample.torrent", 0, "download/test-piece-0").await?;
        let expected = "Piece 0 downloaded to download/test-piece-0";
        assert_eq!(expected, output);
        Ok(())
    }

    #[tokio::test]
    async fn test_download() -> anyhow::Result<()> {
        // tests are configured to be run in 1 thread, because there are errors when communicating with the same peer in parallel
        let file_path = "download/test";
        let output = download_command("sample.torrent", file_path).await?;
        let expected = "Downloaded sample.torrent to download/test";
        assert_eq!(expected, output);

        let mut file = std::fs::File::open(file_path)?;
        let mut hasher = Sha1::new();
        io::copy(&mut file, &mut hasher)?;
        let actual_hash = hasher.finalize();
        let actual_hash = hex::encode(actual_hash);
        assert_eq!("1577533193d6eaf67fa97e1d5bc9d1dfbe4f82e3", actual_hash);

        Ok(())
    }
}
