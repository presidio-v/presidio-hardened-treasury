//! Canonical JSON encoding (spec v2 §3.3 "canonicalization spec").
//!
//! Rules — chosen so the same logical value hashes identically on every
//! toolchain an auditor might use:
//! - Object keys sorted by UTF-8 byte order; no duplicate keys can exist
//!   (`serde_json` maps collapse them at parse time).
//! - No insignificant whitespace; separators are `,` and `:` only.
//! - Strings escaped exactly as `serde_json` emits them (stable, minimal
//!   escaping; non-ASCII passes through as UTF-8).
//! - **Floats are rejected.** Only integers within `i64`/`u64` are legal
//!   JSON numbers here. Monetary atoms and timestamps serialize as decimal
//!   strings upstream (see `treasury-core`), so nothing in the accounting
//!   path ever needs a float. A value that cannot hash deterministically
//!   is not evidence.
//! - Nesting depth is capped to keep canonicalization total (no stack
//!   overflow on adversarial input).

use serde_json::Value;

/// Maximum permitted nesting depth for canonicalized values.
pub const MAX_DEPTH: usize = 128;

/// Encode a JSON value to its single canonical byte representation.
///
/// # Errors
/// [`CanonError::FloatRejected`] for any non-integer number;
/// [`CanonError::TooDeep`] beyond [`MAX_DEPTH`] nesting;
/// [`CanonError::Encode`] if string encoding fails (not expected in practice).
pub fn canonical_bytes(value: &Value) -> Result<Vec<u8>, CanonError> {
    let mut out = Vec::with_capacity(256);
    write_value(value, &mut out, 0)?;
    Ok(out)
}

fn write_value(value: &Value, out: &mut Vec<u8>, depth: usize) -> Result<(), CanonError> {
    if depth > MAX_DEPTH {
        return Err(CanonError::TooDeep);
    }
    match value {
        Value::Null => out.extend_from_slice(b"null"),
        Value::Bool(true) => out.extend_from_slice(b"true"),
        Value::Bool(false) => out.extend_from_slice(b"false"),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                out.extend_from_slice(i.to_string().as_bytes());
            } else if let Some(u) = n.as_u64() {
                out.extend_from_slice(u.to_string().as_bytes());
            } else {
                return Err(CanonError::FloatRejected(n.to_string()));
            }
        }
        Value::String(s) => write_string(s, out)?,
        Value::Array(items) => {
            out.push(b'[');
            let mut first = true;
            for item in items {
                if !first {
                    out.push(b',');
                }
                first = false;
                write_value(item, out, depth.saturating_add(1))?;
            }
            out.push(b']');
        }
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort_unstable();
            out.push(b'{');
            let mut first = true;
            for key in keys {
                if !first {
                    out.push(b',');
                }
                first = false;
                write_string(key, out)?;
                out.push(b':');
                if let Some(v) = map.get(key) {
                    write_value(v, out, depth.saturating_add(1))?;
                }
            }
            out.push(b'}');
        }
    }
    Ok(())
}

fn write_string(s: &str, out: &mut Vec<u8>) -> Result<(), CanonError> {
    let encoded = serde_json::to_string(s).map_err(|e| CanonError::Encode(e.to_string()))?;
    out.extend_from_slice(encoded.as_bytes());
    Ok(())
}

/// Errors from canonicalization.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CanonError {
    /// A non-integer JSON number was encountered; floats are not evidence.
    #[error("float rejected in canonical encoding: {0}")]
    FloatRejected(String),
    /// Nesting exceeded [`MAX_DEPTH`].
    #[error("value nesting exceeds maximum depth")]
    TooDeep,
    /// String encoding failed.
    #[error("encode error: {0}")]
    Encode(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sha256;
    use serde_json::json;

    fn canon_str(v: &Value) -> String {
        String::from_utf8(canonical_bytes(v).unwrap_or_default()).unwrap_or_default()
    }

    #[test]
    fn keys_sorted_and_compact() {
        let v = json!({"b": 1, "a": [true, null, "x"]});
        assert_eq!(canon_str(&v), r#"{"a":[true,null,"x"],"b":1}"#);
    }

    #[test]
    fn floats_rejected() {
        let v = json!({"price": 1.5});
        assert!(matches!(
            canonical_bytes(&v),
            Err(CanonError::FloatRejected(_))
        ));
    }

    #[test]
    fn depth_capped() {
        let mut v = json!(1);
        for _ in 0..=MAX_DEPTH {
            v = json!([v]);
        }
        assert_eq!(canonical_bytes(&v), Err(CanonError::TooDeep));
    }

    /// Golden vector — independently computed:
    /// `python3 -c 'import hashlib,json; print(hashlib.sha256(json.dumps(
    ///   {"asset":"BTC","atoms":"1000","venue":"coinbase-prime"},
    ///   separators=(",",":"),sort_keys=True).encode()).hexdigest())'`
    #[test]
    fn golden_hash_matches_independent_implementation() {
        let v = json!({"venue": "coinbase-prime", "atoms": "1000", "asset": "BTC"});
        let bytes = canonical_bytes(&v).unwrap_or_default();
        assert_eq!(
            String::from_utf8_lossy(&bytes),
            r#"{"asset":"BTC","atoms":"1000","venue":"coinbase-prime"}"#
        );
        assert_eq!(
            sha256(&bytes).to_hex(),
            "ee0b6cb0840399dad594502e03785d984baf56118ec317cfae7c4bb781489f45"
        );
    }
}
