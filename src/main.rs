use std::collections::BTreeMap;
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
    Dict(BTreeMap<&'a str, Value<'a>>),
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

fn decode_value(input: &str) -> anyhow::Result<Value> {
    let (value, tail) = decode_value_inner(input)?;
    if tail.len() > 0 {
        bail!("invalid format, input is not completely consumed");
    }
    Ok(value)
}

fn decode_value_inner(input: &str) -> anyhow::Result<(Value, &str)> {
    let Some(first) = input.chars().next() else {
        bail!("empty input");
    };
    match first {
        'i' => decode_int(input),
        '0'..='9' => decode_string(input),
        'l' => decode_list(input),
        'd' => decode_dict(input),
        _ => bail!("invalid format, can't parse a value that starts with {first}"),
    }
}

fn decode_string(input: &str) -> anyhow::Result<(Value, &str)> {
    let (length_str, string) = input.split_once(':').context("delimiter not found")?;
    let length = length_str.parse::<usize>().context("length is not a valid number")?;
    let actual_len = string.len();
    if actual_len < length {
        bail!("actual len {actual_len} is smaller than expected {length}");
    }
    let string = &string[..length];
    let consumed_len = length_str.len() + 1 + length; // + 1 from delimiter
    let tail = &input[consumed_len..];
    Ok((Value::Str(string), tail))
}

fn decode_int(input: &str) -> anyhow::Result<(Value, &str)> {
    let (num_str, _) = input[1..].split_once('e').context("end of integer not found")?;
    let num = num_str.parse().context("invalid int")?;
    let consumed_len = num_str.len() + 2; // +2 from i at the start and e at the end
    let tail = &input[consumed_len..];
    Ok((Value::Int(num), tail))
}

fn decode_list(input: &str) -> anyhow::Result<(Value, &str)> {
    let mut input = &input[1..];
    let mut list = vec![];
    loop {
        let Some(next) = input.chars().next() else {
            bail!("invalid format, list does not have an end");
        };
        if next == 'e' {
            input = &input[1..];
            break;
        }
        let (value, tail) = decode_value_inner(input)?;
        input = tail;
        list.push(value);
    }
    Ok((Value::List(list), input))
}

fn decode_dict(input: &str) -> anyhow::Result<(Value, &str)> {
    let mut input = &input[1..];
    let mut dict = BTreeMap::new();
    loop {
        let Some(next) = input.chars().next() else {
            bail!("invalid format, dict does not have an end");
        };
        if next == 'e' {
            input = &input[1..];
            break;
        }
        let (key, tail) = decode_value_inner(input)?;
        input = tail;
        let Value::Str(key) = key else {
            bail!("invalid format, dict key is not a string");
        };
        let (value, tail) = decode_value_inner(input)?;
        input = tail;
        dict.insert(key, value);
    }
    Ok((Value::Dict(dict), input))
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

        let res = decode_value("d3:foo3:bar5:helloi52ee")?;
        let mut expected = BTreeMap::new();
        expected.insert("foo", Value::Str("bar"));
        expected.insert("hello", Value::Int(52));
        assert_eq!(Value::Dict(expected), res);
        assert_eq!("{\"foo\":\"bar\",\"hello\":52}", serialize_for_output(res)?);

        Ok(())
    }
}