use anyhow::{bail, Context};
use clap::{Parser, Subcommand};

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Decode {
        value: String,
    }
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Decode { value } => decode_value(value),
    }
}

fn decode_value(value: String) -> anyhow::Result<()> {
    let Some(first) = value.chars().next() else {
        bail!("empty input");
    };
    match first {
        'i' => decode_int(value),
        '0'..='9' => decode_string(value),
        _ => bail!("invalid format, can't parse a value that starts with {first}"),
    }
}

fn decode_string(value: String) -> anyhow::Result<()> {
    let (length, string) = value.split_once(':').context("delimiter not found")?;
    let length = length.parse::<usize>().context("length is not a valid number")?;
    let actual_len = string.len();
    if actual_len < length {
        bail!("actual len {actual_len} is smaller than expected {length}");
    }
    let string = &string[..length];
    println!("\"{string}\"");
    Ok(())
}

fn decode_int(value: String) -> anyhow::Result<()> {
    let (num, _) = value[1..].split_once('e').context("end of integer not found")?;
    let num = num.parse::<i64>().context("invalid int")?;
    println!("{num}");
    Ok(())
}
