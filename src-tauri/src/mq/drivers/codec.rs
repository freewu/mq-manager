//! Byte <-> JSON-string helpers shared by the queue based drivers.
//!
//! Payloads travel over the IPC boundary as JSON, so binary bodies must be
//! base64 encoded; [`PayloadEncoding`] tells the UI which one it received.

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;

use crate::error::{AppError, AppResult};
use crate::mq::types::PayloadEncoding;

/// Encode bytes as UTF-8 when they are valid printable text, base64 otherwise.
pub fn encode_bytes(bytes: &[u8]) -> (String, PayloadEncoding) {
    match std::str::from_utf8(bytes) {
        Ok(text)
            if !text
                .chars()
                .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t')) =>
        {
            (text.to_string(), PayloadEncoding::Utf8)
        }
        _ => (BASE64.encode(bytes), PayloadEncoding::Base64),
    }
}

/// Decode a payload/header value that carries its own encoding tag.
pub fn decode_bytes(value: Option<&str>, encoding: PayloadEncoding) -> AppResult<Vec<u8>> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    match encoding {
        PayloadEncoding::Utf8 => Ok(value.as_bytes().to_vec()),
        PayloadEncoding::Base64 => decode_base64(value, "payload"),
    }
}

/// Decode base64 with an error message naming the offending field.
pub fn decode_base64(value: &str, what: &str) -> AppResult<Vec<u8>> {
    BASE64
        .decode(value.trim())
        .map_err(|error| AppError::invalid(format!("{what} is not valid base64: {error}")))
}
