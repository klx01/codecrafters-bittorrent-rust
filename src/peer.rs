use std::net::{SocketAddrV4, TcpStream};
use std::io::{Read, Write};
use std::{cmp, mem, slice};
use std::time::Duration;
use anyhow::{bail, Context};
use sha1::{Digest, Sha1};
use crate::torrent::{HASH_RAW_LENGTH, PieceInfo, Torrent};
use crate::tracker::{MY_PEER_ID, PEER_ID_LEN};

const PROTOCOL_HEADER: &str = "BitTorrent protocol";
const PADDING: &[u8] = &[0; 8];
const BLOCK_SIZE: u32 = 16 * 1024;
const BLOCK_HEADER_LENGTH: usize = 8;
const MAX_LENGTH: u32 = BLOCK_SIZE + (BLOCK_HEADER_LENGTH as u32);

#[repr(C)]
struct HandshakeMessage {
    length: u8,
    header: [u8; PROTOCOL_HEADER.len()],
    padding: [u8; PADDING.len()],
    info_hash: [u8; HASH_RAW_LENGTH],
    peer_id: [u8; PEER_ID_LEN],
}

#[allow(dead_code)]
#[derive(Debug, Copy, Clone)]
enum MessageType {
    Choke = 0,
    Unchoke = 1,
    Interested = 2,
    NotInterested = 3,
    Have = 4,
    PiecesBitfield = 5,
    Block = 6,
    Piece = 7,
    Cancel = 8,
}

#[repr(C)]
struct BlockRequestRaw {
    index: [u8; 4],
    begin: [u8; 4],
    length: [u8; 4],
}
impl BlockRequestRaw {
    fn new(piece_index: u32, begin: u32, length: u32) -> Self {
        Self {
            index: piece_index.to_be_bytes(),
            begin: begin.to_be_bytes(),
            length: length.to_be_bytes(),
        }
    }
}

pub(crate) struct Peer {
    tcp: TcpStream,
    pub peer_id: [u8; PEER_ID_LEN],
    pub has_pieces: Vec<u8>,
}
impl Peer {
    fn read_message(&mut self, expect_type: MessageType) -> anyhow::Result<Vec<u8>> {
        read_message(&mut self.tcp, expect_type)
    }
    fn write_message(&mut self, msg_type: MessageType, data: &[u8]) -> anyhow::Result<()> {
        write_message(&mut self.tcp, msg_type, data)
    }

    pub fn has_piece(&self, piece_index: u32) -> bool {
        piece_exists(piece_index, &self.has_pieces)
    }

    pub fn download_piece(&mut self, piece_info: PieceInfo) -> anyhow::Result<Vec<u8>> {
        let PieceInfo{ index: piece_index, length: piece_size, hash: piece_hash } = piece_info;

        if !self.has_piece(piece_index) {
            bail!("peer does not have piece {piece_index}");
        }

        let mut full_piece = Vec::with_capacity(piece_size as usize);
        let mut block_no = 0;
        while let Some((block_start, block_length)) = Self::next_block_params(block_no, piece_size) {
            block_no += 1;
            let block_request = BlockRequestRaw::new(piece_index, block_start, block_length);
            self.write_message(MessageType::Block, unsafe { get_bytes_ref_of_struct(&block_request) })?;

            let block_response = self.read_message(MessageType::Piece)?;
            let block = Self::extract_block_from_response(&block_response, piece_index, block_no, block_start, block_length)?;
            full_piece.extend_from_slice(block);
        }

        let mut hasher = Sha1::new();
        hasher.update(&full_piece);
        let actual_hash = hasher.finalize();
        let actual_hash: [u8; HASH_RAW_LENGTH] = actual_hash.try_into().context("failed to calc hash")?;
        if actual_hash != piece_hash {
            bail!("hash does not match, expected {}, actual {}", hex::encode(piece_hash), hex::encode(actual_hash));
        }
        Ok(full_piece)
    }

    fn next_block_params(block_no: u32, piece_size: u32) -> Option<(u32, u32)> {
        let block_start = block_no * BLOCK_SIZE;
        if block_start >= piece_size {
            return None;
        }
        let left_size = piece_size - block_start;
        let length = cmp::min(left_size, BLOCK_SIZE);
        Some((block_start, length))
    }

    fn extract_block_from_response(block_response: &[u8], piece_index: u32, block_no: u32, block_start: u32, block_length: u32) -> anyhow::Result<&[u8]> {
        let expected_response_length = block_length as usize + BLOCK_HEADER_LENGTH;
        if block_response.len() != expected_response_length {
            bail!("unexpected response length for block {block_no} expected {expected_response_length} got {}", block_response.len());
        }
        let (header, block) = block_response.split_at(BLOCK_HEADER_LENGTH);
        let (res_piece_index, res_block_start) = header.split_at(4);
        let res_piece_index = u32::from_be_bytes(res_piece_index.try_into().unwrap());
        if res_piece_index != piece_index {
            bail!("unexpected response length for block {block_no} expected {expected_response_length} got {}", block_response.len());
        }
        let res_block_start = u32::from_be_bytes(res_block_start.try_into().unwrap());
        if res_block_start != block_start {
            bail!("unexpected start value in response for block {block_no} expected {block_start} got {res_block_start}");
        }
        Ok(block)
    }
}

pub(crate) fn init_peer(torrent: &Torrent, socket: &SocketAddrV4) -> anyhow::Result<Peer> {
    let info_hash = torrent.info.get_info_hash()?;
    let mut tcp = create_connection(socket)?;
    let peer_id = handshake(&mut tcp, &info_hash)?;
    let has_pieces = read_message(&mut tcp, MessageType::PiecesBitfield)?;
    if has_pieces.iter().all(|x| *x == 0) {
        bail!("peer has no pieces");
    }
    write_message(&mut tcp, MessageType::Interested, &[])?;
    let _ = read_message(&mut tcp, MessageType::Unchoke)?;
    let peer = Peer{ tcp, peer_id, has_pieces };
    Ok(peer)
}

fn create_connection(socket: &SocketAddrV4) -> anyhow::Result<TcpStream> {
    let tcp_stream = TcpStream::connect(socket).context("failed to connect")?;
    let timeout = Some(Duration::from_secs(2));
    tcp_stream.set_write_timeout(timeout).context("failed to set write timeout")?;
    tcp_stream.set_read_timeout(timeout).context("failed to set write timeout")?;
    Ok(tcp_stream)
}

fn handshake(tcp: &mut TcpStream, info_hash: &[u8; 20]) -> anyhow::Result<[u8; PEER_ID_LEN]> {
    let mut handshake_message = HandshakeMessage {
        length: PROTOCOL_HEADER.len() as u8,
        header: PROTOCOL_HEADER.as_bytes().try_into().unwrap(),
        padding: PADDING.try_into().unwrap(),
        info_hash: info_hash.clone(),
        peer_id: MY_PEER_ID.as_bytes().try_into().unwrap(),
    };
    let handshake_bytes = unsafe { get_bytes_ref_of_struct_mut(&mut handshake_message) };

    tcp.write_all(handshake_bytes).context("failed to send handshake")?;
    tcp.flush().context("failed to flush handshake")?;

    tcp.read_exact(handshake_bytes).context("failed to read handshake")?;
    validate_handshake(info_hash, &handshake_message)?;

    let peer_id = handshake_message.peer_id;
    Ok(peer_id)
}

unsafe fn get_bytes_ref_of_struct_mut<T: Sized>(struct_ref: &mut T) -> &mut [u8] {
    slice::from_raw_parts_mut(
        struct_ref as *mut T as *mut u8,
        mem::size_of::<T>()
    )
}
unsafe fn get_bytes_ref_of_struct<T: Sized>(struct_ref: &T) -> &[u8] {
    slice::from_raw_parts(
        struct_ref as *const T as *const u8,
        mem::size_of::<T>()
    )
}

fn validate_handshake(info_hash: &[u8; 20], handshake_message: &HandshakeMessage) -> anyhow::Result<()> {
    if handshake_message.length != (PROTOCOL_HEADER.len() as u8) {
        bail!("received invalid header length {}", handshake_message.length);
    }
    if handshake_message.header != PROTOCOL_HEADER.as_bytes() {
        bail!("received invalid header {:?}", std::str::from_utf8(&handshake_message.header));
    }
    if handshake_message.info_hash != *info_hash {
        bail!("received invalid hash hex {} expected {}", hex::encode(handshake_message.info_hash), hex::encode(info_hash));
    }
    Ok(())
}

fn read_message(tcp: &mut TcpStream, expect_type: MessageType) -> anyhow::Result<Vec<u8>> {
    let mut message_length_bytes = [0u8; 4];
    tcp.read_exact(&mut message_length_bytes).context(format!("failed to read message length for {expect_type:?}"))?;
    let message_length = u32::from_be_bytes(message_length_bytes);
    assert!(message_length > 0, "got a heartbeat message, not prepared for that");
    let data_length = message_length - 1;
    if data_length > MAX_LENGTH {
        bail!("received too large message length {data_length} for {expect_type:?} {message_length_bytes:?}");
    }

    let mut msg_type = [0u8];
    tcp.read_exact(&mut msg_type).context(format!("failed to read message type for {expect_type:?}"))?;
    let msg_type = msg_type[0];
    if msg_type != (expect_type as u8) {
        bail!("got message of type {msg_type} instead of expected {expect_type:?}");
    }

    let mut data = vec![0u8; data_length as usize];
    if data_length > 0 {
        tcp.read_exact(&mut data).context(format!("failed to read message data for {expect_type:?}"))?;
    }
    Ok(data)
}

fn write_message(tcp: &mut TcpStream, msg_type: MessageType, data: &[u8]) -> anyhow::Result<()> {
    let length = (data.len() + 1) as u32; // length of the whole message, including the type, not just data
    tcp.write_all(&length.to_be_bytes()).context("failed to write message length")?;
    tcp.write_all(&[msg_type as u8]).context("failed to write message type")?;
    if length > 0 {
        tcp.write_all(&data).context("failed to write message data")?;
    }
    tcp.flush().context("failed to flush message")?;
    Ok(())
}

fn piece_exists(piece_index: u32, pieces_bitmap: &[u8]) -> bool {
    // extracted to a separate function for easy testing. The struct requires a TcpStream
    let byte_key = (piece_index / 8) as usize;
    let bit_no = piece_index % 8;
    let Some(bitmap_byte) = pieces_bitmap.get(byte_key) else {
        return false;
    };
    let bit_no = 7 - bit_no;
    let bitmask = 1u8 << bit_no;
    let has_bit = (*bitmap_byte & bitmask) > 0;
    has_bit
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_next_block_params() {
        let params = Peer::next_block_params(0, 300).expect("block 0 should exist");
        assert_eq!((0, 300), params);
        let params = Peer::next_block_params(1, 300);
        assert!(params.is_none(), "block 1 should not exist");

        let params = Peer::next_block_params(0, BLOCK_SIZE).expect("block 0 should exist");
        assert_eq!((0, BLOCK_SIZE), params);
        let params = Peer::next_block_params(1, BLOCK_SIZE);
        assert!(params.is_none(), "block 1 should not exist");

        let params = Peer::next_block_params(0, BLOCK_SIZE + 1).expect("block 0 should exist");
        assert_eq!((0, BLOCK_SIZE), params);
        let params = Peer::next_block_params(1, BLOCK_SIZE + 1).expect("block 1 should exist");
        assert_eq!((BLOCK_SIZE, 1), params);
        let params = Peer::next_block_params(2, BLOCK_SIZE + 1);
        assert!(params.is_none(), "block 2 should not exist");
    }

    #[test]
    fn test_has_piece() {
        let pieces = [0b11100000, 0b10010000];
        assert!(piece_exists(0, &pieces));
        assert!(piece_exists(1, &pieces));
        assert!(piece_exists(2, &pieces));
        assert!(!piece_exists(3, &pieces));
        assert!(!piece_exists(4, &pieces));
        assert!(!piece_exists(7, &pieces));
        assert!(piece_exists(8, &pieces));
        assert!(!piece_exists(9, &pieces));
        assert!(!piece_exists(10, &pieces));
        assert!(piece_exists(11, &pieces));
        assert!(!piece_exists(15, &pieces));
        assert!(!piece_exists(16, &pieces));
        assert!(!piece_exists(100, &pieces));
    }
}