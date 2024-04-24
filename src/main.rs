use std::fs::File;
use std::io::Read;
use anyhow::{bail, Context};
use clap::{Parser, Subcommand};
use crate::bdecode::{decode_value, decode_value_str};
use crate::common::{json_encode_value, Value};

mod bdecode;
mod common;


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
    let mut file = File::open(&path).context("failed to open file")?;
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
    let Some(info) = dict.get("info") else {
        bail!("did not find info field");
    };
    let Value::Dict(info) = info else {
        bail!("expected info to be dictionary, got {}", info.get_variant_name());
    };
    let Some(length) = info.get("length") else {
        bail!("did not find length field in info");
    };
    let Value::Int(length) = length else {
        bail!("expected length field to be an int, got {}", length.get_variant_name());
    };
    
    println!("Tracker URL: {announce}\nLength: {length}");
    
    Ok(())
}
