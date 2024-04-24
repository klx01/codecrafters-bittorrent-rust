use std::net::{Ipv4Addr, SocketAddrV4};
use std::time::Duration;
use anyhow::{bail, Context};
use reqwest::blocking::Client;
use reqwest::Url;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_bytes::ByteBuf;
use crate::torrent::Torrent;

pub(crate) const MY_PEER_ID: &str = "00112233445566778899";
const MY_PORT: u16 = 6881;
const PEER_LENGTH: usize = 6;

#[derive(Serialize)]
struct PeersQueryData<'a> {
    #[serde(with = "serde_bytes")]
    info_hash: &'a [u8; 20],
    peer_id: &'static str,
    port: u16,
    uploaded: usize,
    downloaded: usize,
    left: usize,
    #[serde(serialize_with = "bool_to_int")]
    compact: bool,
}
fn bool_to_int<S: Serializer>(v: &bool, ser: S) -> Result<S::Ok, S::Error> {
    ser.serialize_u8(*v as u8)
}

#[derive(Deserialize)]
#[serde(untagged)]
enum PeersResponseType {
    Success(PeersResponse),
    Fail{
        #[serde(rename = "failure reason")]
        reason: String,
    },
}
#[derive(Deserialize)]
pub(crate) struct PeersResponse {
    pub complete: usize,
    pub incomplete: usize,
    pub interval: usize,
    #[serde(rename = "min interval")]
    pub min_interval: usize,
    #[serde(deserialize_with = "deserialize_peers")]
    pub peers: Vec<SocketAddrV4>,
}
fn deserialize_peers<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<SocketAddrV4>, D::Error> {
    let peers = ByteBuf::deserialize(deserializer)?;
    let peers_len = peers.len();
    if (peers_len % PEER_LENGTH) != 0 {
        return Err(serde::de::Error::custom(format!("peers of total length {peers_len} can not be divided into socket addresses of length {PEER_LENGTH}")));
    }
    let peers = peers
        .chunks(PEER_LENGTH)
        .map(
            |peer|
                SocketAddrV4::new(
                    Ipv4Addr::new(peer[0], peer[1], peer[2], peer[3]),
                    u16::from_be_bytes([peer[4], peer[5]]),
                )
        )
        .collect();
    Ok(peers)
}

pub(crate) fn request_peers(torrent: &Torrent) -> anyhow::Result<PeersResponse> {
    let Torrent{ announce, info } = torrent;

    let info_hash = info.get_info_hash()?;
    let query = PeersQueryData {
        info_hash: &info_hash,
        peer_id: MY_PEER_ID,
        port: MY_PORT,
        uploaded: 0,
        downloaded: 0,
        left: info.get_length(),
        compact: true,
    };
    let query_string = serde_qs::to_string(&query)?;
    let mut url = Url::parse(&announce).context("failed to parse announce url")?;
    url.set_query(Some(&query_string));

    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .context("failed to build client")?;
    let request = client.get(url);

    let response = request.send().context("request failed")?;
    let response = response.bytes().context("failed to get response bytes")?;
    let response = serde_bencode::from_bytes::<PeersResponseType>(&response).context("failed to parse response into structure")?;
    let response = match response {
        PeersResponseType::Success(res) => res,
        PeersResponseType::Fail{reason} => bail!("got error response {reason}"),
    };

    Ok(response)
}