use std::collections::BTreeMap;
use anyhow::{bail, Context};
use crate::common::Value;

pub(crate) fn decode_value_str(input: &str) -> anyhow::Result<Value> {
    decode_value(input.as_bytes())
}

pub(crate) fn decode_value(input: &[u8]) -> anyhow::Result<Value> {
    let (value, tail) = decode_value_inner(input)?;
    if tail.len() > 0 {
        bail!("invalid format, input is not completely consumed");
    }
    Ok(value)
}

fn decode_value_inner(input: &[u8]) -> anyhow::Result<(Value, &[u8])> {
    let Some(&first) = input.get(0) else {
        bail!("empty input");
    };
    match first {
        b'i' => decode_int(input),
        b'0'..=b'9' => decode_string(input),
        b'l' => decode_list(input),
        b'd' => decode_dict(input),
        _ => bail!("invalid format, can't parse a value that starts with {first}"),
    }
}

fn decode_string(input: &[u8]) -> anyhow::Result<(Value, &[u8])> {
    let mut iter = input.splitn(2, |x| *x == b':');
    let Some(length) = iter.next() else {
        bail!("failed to get string length");
    };
    let Some(tail) = iter.next() else {
        bail!("failed to get string, delimiter not found");
    };
    debug_assert!(iter.next().is_none());
    let length = std::str::from_utf8(length).context("string length is not valid utf8")?;
    let length = length.parse::<usize>().context("length is not a valid number")?;
    let tail_len = tail.len();
    if length > tail_len {
        bail!("expected string length {length} is larger than remaining length {tail_len}");
    }
    let (string, tail) = tail.split_at(length);
    Ok((Value::Str(string), tail))
}

fn decode_int(input: &[u8]) -> anyhow::Result<(Value, &[u8])> {
    let mut iter = input[1..].splitn(2, |x| *x == b'e');
    let Some(num) = iter.next() else {
        bail!("failed to get integer");
    };
    let Some(tail) = iter.next() else {
        bail!("end of integer not found");
    };
    debug_assert!(iter.next().is_none());
    let num = std::str::from_utf8(num).context("integer is not valid utf8")?;
    let num = num.parse().context("invalid int")?;
    Ok((Value::Int(num), tail))
}

fn decode_list(input: &[u8]) -> anyhow::Result<(Value, &[u8])> {
    let mut input = &input[1..];
    let mut list = vec![];
    loop {
        let Some(&next) = input.get(0) else {
            bail!("invalid format, list does not have an end");
        };
        if next == b'e' {
            input = &input[1..];
            break;
        }
        let (value, tail) = decode_value_inner(input)?;
        input = tail;
        list.push(value);
    }
    Ok((Value::List(list), input))
}

fn decode_dict(input: &[u8]) -> anyhow::Result<(Value, &[u8])> {
    let mut input = &input[1..];
    let mut dict = BTreeMap::new();
    loop {
        let Some(&next) = input.get(0) else {
            bail!("invalid format, dict does not have an end");
        };
        if next == b'e' {
            input = &input[1..];
            break;
        }
        let (key, tail) = decode_value_inner(input)?;
        input = tail;
        let Value::Str(key) = key else {
            bail!("invalid format, dict key is not a string");
        };
        let key = std::str::from_utf8(key).context("dict key is not a valid utf8")?;
        let (value, tail) = decode_value_inner(input)?;
        input = tail;
        dict.insert(key, value);
    }
    Ok((Value::Dict(dict), input))
}

#[cfg(test)]
mod test {
    use crate::common::json_encode_value;
    use super::*;

    #[test]
    fn test_parse() -> anyhow::Result<()> {
        let res = decode_value_str("6:hello:")?;
        assert_eq!(Value::Str("hello:".as_bytes()), res);
        assert_eq!("\"hello:\"", json_encode_value(res)?);

        let res = decode_value_str("i52e")?;
        assert_eq!(Value::Int(52), res);
        assert_eq!("52", json_encode_value(res)?);
        let res = decode_value_str("i-52e")?;
        assert_eq!(Value::Int(-52), res);
        assert_eq!("-52", json_encode_value(res)?);

        let res = decode_value_str("l5:helloi52ee")?;
        let expected = Value::List([Value::Str("hello".as_bytes()), Value::Int(52)].to_vec());
        assert_eq!(expected, res);
        assert_eq!("[\"hello\",52]", json_encode_value(res)?);
        let res = decode_value_str("ll5:helloi52eee")?;
        assert_eq!(Value::List([expected].to_vec()), res);
        assert_eq!("[[\"hello\",52]]", json_encode_value(res)?);

        let res = decode_value_str("d3:foo3:bar5:helloi52ee")?;
        let mut expected = BTreeMap::new();
        expected.insert("foo", Value::Str("bar".as_bytes()));
        expected.insert("hello", Value::Int(52));
        assert_eq!(Value::Dict(expected), res);
        assert_eq!("{\"foo\":\"bar\",\"hello\":52}", json_encode_value(res)?);

        Ok(())
    }
}
