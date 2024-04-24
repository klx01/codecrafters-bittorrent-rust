use anyhow::{bail, Context};
use clap::{Parser, Subcommand};
use serde::Serialize;

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

#[derive(Serialize, PartialEq, Debug, Clone)]
#[serde(untagged)]
enum Value<'a> {
    Int(i64),
    Str(&'a str),
    List(Vec<Value<'a>>),
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Decode { value } => decode_command(value),
    }
}

fn decode_command(value: String) -> anyhow::Result<()> {
    let value = decode_value(&value)?;
    let json = serialize_for_output(value)?;
    println!("{json}");
    Ok(())
}

fn serialize_for_output(value: Value) -> anyhow::Result<String> {
    serde_json::to_string(&value).context("failed to serialize to json")
}

fn decode_value(value: &str) -> anyhow::Result<Value> {
    let (value, tail) = decode_value_inner(value)?;
    if tail.len() > 0 {
        bail!("invalid format, input is not completely consumed");
    }
    Ok(value)
}

fn decode_value_inner(value: &str) -> anyhow::Result<(Value, &str)> {
    let Some(first) = value.chars().next() else {
        bail!("empty input");
    };
    match first {
        'i' => decode_int(value),
        '0'..='9' => decode_string(value),
        'l' => decode_list(value),
        _ => bail!("invalid format, can't parse a value that starts with {first}"),
    }
}

fn decode_string(value: &str) -> anyhow::Result<(Value, &str)> {
    let (length_str, string) = value.split_once(':').context("delimiter not found")?;
    let length = length_str.parse::<usize>().context("length is not a valid number")?;
    let actual_len = string.len();
    if actual_len < length {
        bail!("actual len {actual_len} is smaller than expected {length}");
    }
    let string = &string[..length];
    let consumed_len = length_str.len() + 1 + length; // + 1 from delimiter
    let tail = &value[consumed_len..];
    Ok((Value::Str(string), tail))
}

fn decode_int(value: &str) -> anyhow::Result<(Value, &str)> {
    let (num_str, _) = value[1..].split_once('e').context("end of integer not found")?;
    let num = num_str.parse().context("invalid int")?;
    let consumed_len = num_str.len() + 2; // +2 from i at the start and e at the end
    let tail = &value[consumed_len..];
    Ok((Value::Int(num), tail))
}

fn decode_list(value: &str) -> anyhow::Result<(Value, &str)> {
    let mut value = &value[1..];
    let mut list = vec![];
    loop {
        let Some(next) = value.chars().next() else {
            bail!("invalid format, list does not have an end");
        };
        if next == 'e' {
            value = &value[1..];
            break;
        }
        let (next, tail) = decode_value_inner(value)?;
        value = tail;
        list.push(next);
    }
    Ok((Value::List(list), value))
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_parse() -> anyhow::Result<()> {
        let res = decode_value("5:hello")?;
        assert_eq!(Value::Str("hello"), res);
        assert_eq!("\"hello\"", serialize_for_output(res)?);
        let res = decode_value("i52e")?;
        assert_eq!(Value::Int(52), res);
        assert_eq!("52", serialize_for_output(res)?);
        let res = decode_value("i-52e")?;
        assert_eq!(Value::Int(-52), res);
        assert_eq!("-52", serialize_for_output(res)?);
        let res = decode_value("l5:helloi52ee")?;
        let expected = Value::List([Value::Str("hello"), Value::Int(52)].to_vec());
        assert_eq!(expected, res);
        assert_eq!("[\"hello\",52]", serialize_for_output(res)?);
        let res = decode_value("ll5:helloi52eee")?;
        assert_eq!(Value::List([expected].to_vec()), res);
        assert_eq!("[[\"hello\",52]]", serialize_for_output(res)?);
        Ok(())
    }
}