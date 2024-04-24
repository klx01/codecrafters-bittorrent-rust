use std::net::{SocketAddrV4, TcpStream};
use std::io::{Read, Write};
use std::time::Duration;
use anyhow::{bail, Context};
use crate::torrent::{HASH_RAW_LENGTH, Torrent};
use crate::tracker::MY_PEER_ID;

const MESSAGE_HEADER: &str = "\x13BitTorrent protocol";
const PADDING: &[u8] = &[0; 8];

pub(crate) fn handshake(torrent: &Torrent, socket: &SocketAddrV4) -> anyhow::Result<[u8; MY_PEER_ID.len()]> {
    let info_hash = torrent.info.get_info_hash()?;

    const HANDSHAKE_MESSAGE_LEN: usize = MESSAGE_HEADER.len() + PADDING.len() + HASH_RAW_LENGTH + MY_PEER_ID.len();
    let mut handshake_message = Vec::with_capacity(HANDSHAKE_MESSAGE_LEN);
    handshake_message.extend_from_slice(MESSAGE_HEADER.as_bytes());
    handshake_message.extend_from_slice(PADDING);
    handshake_message.extend_from_slice(&info_hash);
    handshake_message.extend_from_slice(MY_PEER_ID.as_bytes());

    let mut tcp_stream = TcpStream::connect(socket).context("failed to connect")?;
    let timeout = Some(Duration::from_secs(10));
    tcp_stream.set_write_timeout(timeout).context("failed to set write timeout")?;
    tcp_stream.set_read_timeout(timeout).context("failed to set write timeout")?;
    tcp_stream.write_all(&handshake_message).context("failed to write to socket")?;
    tcp_stream.flush().context("failed to flush")?;

    let mut response = [0; HANDSHAKE_MESSAGE_LEN];
    tcp_stream.read_exact(&mut response).context("failed to read response")?;
    let (header, tail) = response.split_at(MESSAGE_HEADER.len());
    if header != MESSAGE_HEADER.as_bytes() {
        bail!("received invalid header, str value {:?}", std::str::from_utf8(header));
    }
    let (_padding, tail) = tail.split_at(PADDING.len());
    let (hash, tail) = tail.split_at(info_hash.len());
    if hash != info_hash {
        bail!("received invalid hash hex {} expected {}", hex::encode(hash), hex::encode(info_hash));
    }
    let (peer_id, _tail) = tail.split_at(MY_PEER_ID.len());
    let peer_id: [u8; MY_PEER_ID.len()] = peer_id.try_into().expect("incorrect peer length");
    Ok(peer_id)
}