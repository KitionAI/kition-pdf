//! COS objects as KDM JSON values, losslessly (`model/schema.json`'s overlay entries).
//!
//! | Object | JSON |
//! | --- | --- |
//! | `null`, booleans | `null`, `true`/`false` |
//! | integer | a JSON integer; beyond ±2^53 (which not every KDM implementation reads exactly) `{"int": "<decimal>"}` |
//! | real | `{"real": <number>}`; a non-finite one `{"realBits": "<16 hex digits>"}` |
//! | string | `{"str": "<text>"}` when its bytes are UTF-8, else `{"bin": "<hex>"}`; `"hex": true` when written in hex |
//! | name | `{"name": "<text>"}` when UTF-8, else `{"nameBin": "<hex>"}` |
//! | array | a JSON array |
//! | dictionary | `{"dict": [[key, value], …]}` in the dictionary's own order; a key is its text, or `{"bin": "<hex>"}` |
//! | stream | `{"stream": [[key, value], …], "data": <blob>}` with its *encoded* bytes as a blob; up to [`INLINE`] bytes inline as `"inline": "<hex>"` |
//! | reference | `{"ref": "<key>", "gen": <generation>}`: the referenced object's overlay key ([`crate::Ids`]) |
//!
//! A reference names the object by its key, not its number: objects added since the base was
//! written have keys of their own (`<actor>.<n>`), so two peers adding objects at once never
//! collide, and each replica gives them numbers of its own when it reads them.

use std::sync::Arc;

use kition_kit::session::{BlobRef, BlobSink, BlobSource};
use kition_pdf_cos::{Dict, ObjRef, Object, PdfString, Stream};
use serde_json::{Map, Value, json};

/// Streams up to this many encoded bytes stay inline instead of becoming a blob.
pub const INLINE: usize = 256;

/// Deepest nesting written or read (the parser's own nesting is bounded far below this).
const MAX_DEPTH: usize = 512;

/// JavaScript's safe integers: what every KDM implementation reads exactly.
const MAX_SAFE: i64 = (1 << 53) - 1;

const STREAM_MIME: &str = "application/octet-stream";

pub(crate) fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(char::from(DIGITS[usize::from(byte >> 4)]));
        out.push(char::from(DIGITS[usize::from(byte & 15)]));
    }
    out
}

pub(crate) fn unhex(text: &str) -> Result<Vec<u8>, String> {
    let digit = |c: u8| match c {
        b'0'..=b'9' => Ok(c - b'0'),
        b'a'..=b'f' => Ok(c - b'a' + 10),
        b'A'..=b'F' => Ok(c - b'A' + 10),
        _ => Err(format!("`{}` is not a hex digit", char::from(c))),
    };
    let bytes = text.as_bytes();
    if !bytes.len().is_multiple_of(2) {
        return Err("hex text of odd length".into());
    }
    bytes.chunks(2).map(|pair| Ok(digit(pair[0])? << 4 | digit(pair[1])?)).collect()
}

/// Writes objects: references through `key` (an object number to its overlay key), stream
/// bytes into `blobs`.
pub(crate) struct Encoder<'a> {
    pub key: &'a mut dyn FnMut(u32) -> String,
    pub blobs: &'a mut dyn BlobSink,
}

impl Encoder<'_> {
    pub fn object(&mut self, object: &Object) -> Result<Value, String> {
        self.value(object, 0)
    }

    pub fn dict(&mut self, dict: &Dict) -> Result<Value, String> {
        Ok(json!({ "dict": self.entries(dict, 0)? }))
    }

    fn entries(&mut self, dict: &Dict, depth: usize) -> Result<Value, String> {
        let mut out = Vec::with_capacity(dict.len());
        for (key, value) in dict.iter() {
            let key = match std::str::from_utf8(key) {
                Ok(text) => Value::String(text.to_owned()),
                Err(_) => json!({ "bin": hex(key) }),
            };
            out.push(Value::Array(vec![key, self.value(value, depth + 1)?]));
        }
        Ok(Value::Array(out))
    }

    fn value(&mut self, object: &Object, depth: usize) -> Result<Value, String> {
        if depth > MAX_DEPTH {
            return Err(format!("an object nested more than {MAX_DEPTH} levels deep"));
        }
        Ok(match object {
            Object::Null => Value::Null,
            Object::Bool(b) => Value::Bool(*b),
            Object::Int(i) if i.unsigned_abs() <= MAX_SAFE as u64 => json!(i),
            Object::Int(i) => json!({ "int": i.to_string() }),
            Object::Real(r) => match serde_json::Number::from_f64(*r) {
                Some(number) => json!({ "real": number }),
                None => json!({ "realBits": format!("{:016x}", r.to_bits()) }),
            },
            Object::String(s) => {
                let mut out = Map::new();
                match std::str::from_utf8(&s.bytes) {
                    Ok(text) => out.insert("str".into(), Value::String(text.to_owned())),
                    Err(_) => out.insert("bin".into(), Value::String(hex(&s.bytes))),
                };
                if s.hex {
                    out.insert("hex".into(), Value::Bool(true));
                }
                Value::Object(out)
            }
            Object::Name(n) => match std::str::from_utf8(n) {
                Ok(text) => json!({ "name": text }),
                Err(_) => json!({ "nameBin": hex(n) }),
            },
            Object::Array(items) => Value::Array(items.iter().map(|item| self.value(item, depth + 1)).collect::<Result<_, _>>()?),
            Object::Dict(d) => json!({ "dict": self.entries(d, depth)? }),
            Object::Stream(s) => {
                let entries = self.entries(&s.dict, depth)?;
                if s.raw.len() <= INLINE {
                    json!({ "stream": entries, "inline": hex(&s.raw) })
                } else {
                    let blob = self.blobs.put(&s.raw, STREAM_MIME);
                    json!({ "stream": entries, "data": serde_json::to_value(blob).map_err(|e| e.to_string())? })
                }
            }
            Object::Ref(r) => json!({ "ref": (self.key)(r.num), "gen": r.generation }),
        })
    }
}

/// Reads objects back: references through `num` (an overlay key to this replica's object
/// number), stream bytes from `blobs`.
pub(crate) struct Decoder<'a> {
    pub num: &'a mut dyn FnMut(&str) -> Option<u32>,
    pub blobs: &'a dyn BlobSource,
}

impl Decoder<'_> {
    pub fn object(&mut self, value: &Value) -> Result<Object, String> {
        self.value(value, 0)
    }

    pub fn dict(&mut self, value: &Value) -> Result<Dict, String> {
        match value.get("dict") {
            Some(entries) => self.entries(entries, 0),
            None => Err("not a dictionary".into()),
        }
    }

    fn entries(&mut self, entries: &Value, depth: usize) -> Result<Dict, String> {
        let list = entries.as_array().ok_or("dictionary entries are not a list")?;
        let mut dict = Dict::new();
        for entry in list {
            let pair = entry.as_array().filter(|pair| pair.len() == 2).ok_or("a dictionary entry is not a [key, value] pair")?;
            let key = match &pair[0] {
                Value::String(text) => text.as_bytes().to_vec(),
                other => unhex(other.get("bin").and_then(Value::as_str).ok_or("a dictionary key is neither text nor bin")?)?,
            };
            dict.set(key, self.value(&pair[1], depth + 1)?);
        }
        Ok(dict)
    }

    fn value(&mut self, value: &Value, depth: usize) -> Result<Object, String> {
        if depth > MAX_DEPTH {
            return Err(format!("an object nested more than {MAX_DEPTH} levels deep"));
        }
        let text = |key: &str| value.get(key).and_then(Value::as_str);
        Ok(match value {
            Value::Null => Object::Null,
            Value::Bool(b) => Object::Bool(*b),
            Value::Number(n) => Object::Int(n.as_i64().ok_or_else(|| format!("{n} is not an integer"))?),
            Value::String(_) => return Err("a bare string is not an object".into()),
            Value::Array(items) => Object::Array(items.iter().map(|item| self.value(item, depth + 1)).collect::<Result<_, _>>()?),
            Value::Object(map) => {
                if let Some(entries) = map.get("dict") {
                    Object::Dict(self.entries(entries, depth)?)
                } else if let Some(entries) = map.get("stream") {
                    let dict = self.entries(entries, depth)?;
                    let raw = if let Some(inline) = text("inline") {
                        unhex(inline)?
                    } else {
                        let blob: BlobRef =
                            serde_json::from_value(map.get("data").cloned().unwrap_or(Value::Null)).map_err(|e| format!("stream data: {e}"))?;
                        let bytes: Arc<[u8]> = self.blobs.get(&blob.hash).ok_or_else(|| format!("stream data {} is missing", blob.hash))?;
                        bytes.to_vec()
                    };
                    Object::Stream(Stream::from_raw(dict, raw))
                } else if let Some(key) = text("ref") {
                    let generation = map.get("gen").and_then(Value::as_u64).and_then(|g| u16::try_from(g).ok()).unwrap_or(0);
                    match (self.num)(key) {
                        Some(num) => Object::Ref(ObjRef::new(num, generation)),
                        // A key that names no number (malformed) reads as a dangling reference
                        // would: null (ISO 32000-2 §7.3.10).
                        None => Object::Null,
                    }
                } else if let Some(name) = text("name") {
                    Object::Name(name.as_bytes().to_vec())
                } else if let Some(name) = text("nameBin") {
                    Object::Name(unhex(name)?)
                } else if map.contains_key("str") || map.contains_key("bin") {
                    let bytes = match (text("str"), text("bin")) {
                        (Some(s), _) => s.as_bytes().to_vec(),
                        (None, Some(b)) => unhex(b)?,
                        (None, None) => return Err("a string holds no text".into()),
                    };
                    Object::String(PdfString { bytes, hex: map.get("hex").and_then(Value::as_bool).unwrap_or(false) })
                } else if let Some(real) = map.get("real") {
                    Object::Real(real.as_f64().ok_or("a real is not a number")?)
                } else if let Some(bits) = text("realBits") {
                    Object::Real(f64::from_bits(u64::from_str_radix(bits, 16).map_err(|e| e.to_string())?))
                } else if let Some(int) = text("int") {
                    Object::Int(int.parse().map_err(|_| format!("{int} is not an integer"))?)
                } else {
                    return Err(format!("unknown object form {value}"));
                }
            }
        })
    }
}
