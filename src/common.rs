use std::collections::BTreeMap;
use anyhow::Context;

#[derive(PartialEq, Debug, Clone)]
pub(crate) enum Value<'a> {
    Int(i64),
    Str(&'a [u8]),
    List(Vec<Value<'a>>),
    Dict(BTreeMap<&'a str, Value<'a>>),
}
impl<'a> Value<'a> {
    pub fn get_variant_name(&self) -> &str {
        match self {
            Value::Int(_) => "int",
            Value::Str(_) => "string",
            Value::List(_) => "list",
            Value::Dict(_) => "dictionary",
        }
    } 
}

pub(crate) fn json_encode_value(value: Value) -> anyhow::Result<String> {
    /*
    implement custom serialization to json instead of using serde, 
    because i don't know how to easily serialize bytes as string via serde without a wrapper type, 
    and because it's really easy to do a custom one 
     */
    match value {
        Value::Int(int) => Ok(int.to_string()),
        Value::Str(str) => {
            let str = std::str::from_utf8(str).context("string is not a valid utf8")?;
            Ok(format!("\"{str}\""))
        }
        Value::List(list) => {
            let mut res = String::from("[");
            for value in list {
                res.push_str(&json_encode_value(value)?);
                res.push(',');
            }
            if res.len() > 1 {
                // remove the trailing comma
                res.pop();
            }
            res.push(']');
            Ok(res)
        }
        Value::Dict(dict) => {
            let mut res = String::from("{");
            for (key, value) in dict {
                res.push_str(&format!("\"{key}\":"));
                res.push_str(&json_encode_value(value)?);
                res.push(',');
            }
            if res.len() > 1 {
                // remove the trailing comma
                res.pop();
            }
            res.push('}');
            Ok(res)
        }
    }
}