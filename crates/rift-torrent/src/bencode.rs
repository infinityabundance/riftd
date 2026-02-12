//! Bencode encoding and decoding (BEP-3 compatible).

use std::collections::BTreeMap;

use crate::{FileInfo, InfoHash, PieceHash, SrtExtension, TorrentError, TorrentMeta};

/// Bencode value representation.
#[derive(Clone, Debug, PartialEq)]
pub enum BValue {
    /// Byte string (may contain non-UTF8).
    Bytes(Vec<u8>),
    /// Integer.
    Int(i64),
    /// List of values.
    List(Vec<BValue>),
    /// Dictionary (sorted by key).
    Dict(BTreeMap<Vec<u8>, BValue>),
}

impl BValue {
    /// Get as bytes if Bytes variant.
    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            BValue::Bytes(b) => Some(b),
            _ => None,
        }
    }

    /// Get as string if valid UTF-8 Bytes.
    pub fn as_str(&self) -> Option<&str> {
        self.as_bytes().and_then(|b| std::str::from_utf8(b).ok())
    }

    /// Get as integer.
    pub fn as_int(&self) -> Option<i64> {
        match self {
            BValue::Int(i) => Some(*i),
            _ => None,
        }
    }

    /// Get as list.
    pub fn as_list(&self) -> Option<&[BValue]> {
        match self {
            BValue::List(l) => Some(l),
            _ => None,
        }
    }

    /// Get as dictionary.
    pub fn as_dict(&self) -> Option<&BTreeMap<Vec<u8>, BValue>> {
        match self {
            BValue::Dict(d) => Some(d),
            _ => None,
        }
    }

    /// Dictionary lookup by string key.
    pub fn get(&self, key: &str) -> Option<&BValue> {
        self.as_dict().and_then(|d| d.get(key.as_bytes()))
    }
}

/// Decode bencode from bytes.
pub fn decode(input: &[u8]) -> Result<(BValue, usize), TorrentError> {
    if input.is_empty() {
        return Err(TorrentError::Bencode("empty input".into()));
    }

    match input[0] {
        b'i' => decode_int(input),
        b'l' => decode_list(input),
        b'd' => decode_dict(input),
        b'0'..=b'9' => decode_bytes(input),
        c => Err(TorrentError::Bencode(format!("unexpected byte: {}", c as char))),
    }
}

fn decode_int(input: &[u8]) -> Result<(BValue, usize), TorrentError> {
    let end = input
        .iter()
        .position(|&b| b == b'e')
        .ok_or_else(|| TorrentError::Bencode("unterminated integer".into()))?;

    let num_str = std::str::from_utf8(&input[1..end])
        .map_err(|_| TorrentError::Bencode("invalid integer encoding".into()))?;

    let num: i64 = num_str
        .parse()
        .map_err(|_| TorrentError::Bencode("invalid integer value".into()))?;

    Ok((BValue::Int(num), end + 1))
}

fn decode_bytes(input: &[u8]) -> Result<(BValue, usize), TorrentError> {
    let colon = input
        .iter()
        .position(|&b| b == b':')
        .ok_or_else(|| TorrentError::Bencode("missing colon in byte string".into()))?;

    let len_str = std::str::from_utf8(&input[..colon])
        .map_err(|_| TorrentError::Bencode("invalid byte string length".into()))?;

    let len: usize = len_str
        .parse()
        .map_err(|_| TorrentError::Bencode("invalid byte string length value".into()))?;

    let start = colon + 1;
    let end = start + len;

    if end > input.len() {
        return Err(TorrentError::Bencode("byte string exceeds input".into()));
    }

    Ok((BValue::Bytes(input[start..end].to_vec()), end))
}

fn decode_list(input: &[u8]) -> Result<(BValue, usize), TorrentError> {
    let mut items = Vec::new();
    let mut pos = 1; // Skip 'l'

    while pos < input.len() && input[pos] != b'e' {
        let (value, consumed) = decode(&input[pos..])?;
        items.push(value);
        pos += consumed;
    }

    if pos >= input.len() {
        return Err(TorrentError::Bencode("unterminated list".into()));
    }

    Ok((BValue::List(items), pos + 1)) // +1 for 'e'
}

fn decode_dict(input: &[u8]) -> Result<(BValue, usize), TorrentError> {
    let mut map = BTreeMap::new();
    let mut pos = 1; // Skip 'd'

    while pos < input.len() && input[pos] != b'e' {
        // Key must be a byte string
        let (key_val, key_consumed) = decode(&input[pos..])?;
        let key = match key_val {
            BValue::Bytes(b) => b,
            _ => return Err(TorrentError::Bencode("dictionary key must be string".into())),
        };
        pos += key_consumed;

        // Value
        let (value, val_consumed) = decode(&input[pos..])?;
        pos += val_consumed;

        map.insert(key, value);
    }

    if pos >= input.len() {
        return Err(TorrentError::Bencode("unterminated dictionary".into()));
    }

    Ok((BValue::Dict(map), pos + 1)) // +1 for 'e'
}

/// Encode bencode to bytes.
pub fn encode(value: &BValue) -> Vec<u8> {
    let mut out = Vec::new();
    encode_into(value, &mut out);
    out
}

fn encode_into(value: &BValue, out: &mut Vec<u8>) {
    match value {
        BValue::Bytes(b) => {
            out.extend_from_slice(b.len().to_string().as_bytes());
            out.push(b':');
            out.extend_from_slice(b);
        }
        BValue::Int(i) => {
            out.push(b'i');
            out.extend_from_slice(i.to_string().as_bytes());
            out.push(b'e');
        }
        BValue::List(items) => {
            out.push(b'l');
            for item in items {
                encode_into(item, out);
            }
            out.push(b'e');
        }
        BValue::Dict(map) => {
            out.push(b'd');
            for (key, val) in map {
                // Key as byte string
                out.extend_from_slice(key.len().to_string().as_bytes());
                out.push(b':');
                out.extend_from_slice(key);
                // Value
                encode_into(val, out);
            }
            out.push(b'e');
        }
    }
}

/// Parse a .torrent file into TorrentMeta.
pub fn parse_torrent(data: &[u8]) -> Result<TorrentMeta, TorrentError> {
    let (root, _) = decode(data)?;
    let dict = root
        .as_dict()
        .ok_or(TorrentError::InvalidTorrent("root not dict"))?;

    // Extract info dictionary and compute infohash
    let info_bval = dict
        .get(b"info".as_slice())
        .ok_or(TorrentError::InvalidTorrent("missing info"))?;
    let info_bytes = encode(info_bval);
    let info_hash = compute_sha1_infohash(&info_bytes);

    let info = info_bval
        .as_dict()
        .ok_or(TorrentError::InvalidTorrent("info not dict"))?;

    // Name (required)
    let name = info
        .get(b"name".as_slice())
        .and_then(|v| v.as_str())
        .ok_or(TorrentError::InvalidTorrent("missing name"))?
        .to_string();

    // Piece length (required)
    let piece_length = info
        .get(b"piece length".as_slice())
        .and_then(|v| v.as_int())
        .ok_or(TorrentError::InvalidTorrent("missing piece length"))? as u64;

    // Pieces (required) - concatenated SHA1 hashes
    let pieces_bytes = info
        .get(b"pieces".as_slice())
        .and_then(|v| v.as_bytes())
        .ok_or(TorrentError::InvalidTorrent("missing pieces"))?;

    if pieces_bytes.len() % 20 != 0 {
        return Err(TorrentError::InvalidTorrent("pieces length not multiple of 20"));
    }

    let pieces: Vec<PieceHash> = pieces_bytes
        .chunks_exact(20)
        .map(|chunk| {
            let mut h = [0u8; 20];
            h.copy_from_slice(chunk);
            PieceHash(h)
        })
        .collect();

    // Files - either single file or multi-file
    let (files, total_length) = if let Some(length_val) = info.get(b"length".as_slice()) {
        // Single file mode
        let length = length_val
            .as_int()
            .ok_or(TorrentError::InvalidTorrent("invalid length"))? as u64;
        let file = FileInfo {
            path: name.clone().into(),
            length,
        };
        (vec![file], length)
    } else if let Some(files_val) = info.get(b"files".as_slice()) {
        // Multi-file mode
        let files_list = files_val
            .as_list()
            .ok_or(TorrentError::InvalidTorrent("files not list"))?;

        let mut files = Vec::new();
        let mut total = 0u64;

        for file_val in files_list {
            let file_dict = file_val
                .as_dict()
                .ok_or(TorrentError::InvalidTorrent("file entry not dict"))?;

            let length = file_dict
                .get(b"length".as_slice())
                .and_then(|v| v.as_int())
                .ok_or(TorrentError::InvalidTorrent("file missing length"))? as u64;

            let path_list = file_dict
                .get(b"path".as_slice())
                .and_then(|v| v.as_list())
                .ok_or(TorrentError::InvalidTorrent("file missing path"))?;

            let path: PathBuf = path_list
                .iter()
                .filter_map(|p| p.as_str())
                .collect();

            files.push(FileInfo { path, length });
            total += length;
        }

        (files, total)
    } else {
        return Err(TorrentError::InvalidTorrent("missing length or files"));
    };

    // Optional fields
    let announce = dict
        .get(b"announce".as_slice())
        .and_then(|v| v.as_str())
        .map(String::from);

    let announce_list = dict
        .get(b"announce-list".as_slice())
        .and_then(|v| v.as_list())
        .map(|tiers| {
            tiers
                .iter()
                .filter_map(|tier| {
                    tier.as_list().map(|urls| {
                        urls.iter()
                            .filter_map(|u| u.as_str().map(String::from))
                            .collect()
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let creation_date = dict
        .get(b"creation date".as_slice())
        .and_then(|v| v.as_int())
        .map(|i| i as u64);

    let comment = dict
        .get(b"comment".as_slice())
        .and_then(|v| v.as_str())
        .map(String::from);

    let created_by = dict
        .get(b"created by".as_slice())
        .and_then(|v| v.as_str())
        .map(String::from);

    // SRT extension (rift-specific)
    let srt_extension = dict
        .get(b"srt".as_slice())
        .and_then(|v| parse_srt_extension(v).ok());

    Ok(TorrentMeta {
        info_hash,
        name,
        piece_length,
        pieces,
        files,
        total_length,
        announce,
        announce_list,
        creation_date,
        srt_extension,
        comment,
        created_by,
    })
}

fn parse_srt_extension(value: &BValue) -> Result<SrtExtension, TorrentError> {
    let dict = value
        .as_dict()
        .ok_or(TorrentError::InvalidTorrent("srt not dict"))?;

    let version = dict
        .get(b"version".as_slice())
        .and_then(|v| v.as_int())
        .unwrap_or(1) as u8;

    let t0_offset = dict
        .get(b"t0_offset".as_slice())
        .and_then(|v| v.as_int())
        .unwrap_or(0) as u64;

    let window_secs = dict
        .get(b"window_secs".as_slice())
        .and_then(|v| v.as_int())
        .unwrap_or(300) as u64;

    let slot_ms = dict
        .get(b"slot_ms".as_slice())
        .and_then(|v| v.as_int())
        .unwrap_or(500) as u64;

    let salt = dict.get(b"salt".as_slice()).and_then(|v| {
        v.as_bytes().and_then(|b| {
            if b.len() == 16 {
                let mut arr = [0u8; 16];
                arr.copy_from_slice(b);
                Some(arr)
            } else {
                None
            }
        })
    });

    Ok(SrtExtension {
        version,
        t0_offset,
        window_secs,
        slot_ms,
        salt,
    })
}

fn compute_sha1_infohash(info_bytes: &[u8]) -> InfoHash {
    use sha1::{Digest, Sha1};
    let mut hasher = Sha1::new();
    hasher.update(info_bytes);
    let result = hasher.finalize();
    let mut h = [0u8; 20];
    h.copy_from_slice(&result);
    InfoHash::Sha1(h)
}

use std::path::PathBuf;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_int() {
        let (val, len) = decode(b"i42e").unwrap();
        assert_eq!(val, BValue::Int(42));
        assert_eq!(len, 4);
    }

    #[test]
    fn decode_negative_int() {
        let (val, _) = decode(b"i-123e").unwrap();
        assert_eq!(val, BValue::Int(-123));
    }

    #[test]
    fn decode_bytes() {
        let (val, len) = decode(b"5:hello").unwrap();
        assert_eq!(val, BValue::Bytes(b"hello".to_vec()));
        assert_eq!(len, 7);
    }

    #[test]
    fn decode_list() {
        let (val, _) = decode(b"li1ei2ei3ee").unwrap();
        assert_eq!(
            val,
            BValue::List(vec![BValue::Int(1), BValue::Int(2), BValue::Int(3)])
        );
    }

    #[test]
    fn decode_dict() {
        let (val, _) = decode(b"d3:fooi42ee").unwrap();
        let mut expected = BTreeMap::new();
        expected.insert(b"foo".to_vec(), BValue::Int(42));
        assert_eq!(val, BValue::Dict(expected));
    }

    #[test]
    fn encode_roundtrip() {
        let original = BValue::Dict({
            let mut m = BTreeMap::new();
            m.insert(b"name".to_vec(), BValue::Bytes(b"test".to_vec()));
            m.insert(b"size".to_vec(), BValue::Int(1024));
            m.insert(
                b"list".to_vec(),
                BValue::List(vec![BValue::Int(1), BValue::Int(2)]),
            );
            m
        });

        let encoded = encode(&original);
        let (decoded, _) = decode(&encoded).unwrap();
        assert_eq!(original, decoded);
    }
}
