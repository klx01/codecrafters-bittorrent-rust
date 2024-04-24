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

    let mut hasher = Sha1::new();
    hasher.update(bencode_value(info_value));
    let output = hasher.finalize();
    let hash = hex::encode(output);
    
    Ok(format!("Tracker URL: {announce}\nLength: {length}\nInfo Hash: {hash}"))
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
Info Hash: d69f91e6b2ae4c542468d1073a71d4ea13879a7f";
        assert_eq!(expected, info);
        Ok(())
    }
}