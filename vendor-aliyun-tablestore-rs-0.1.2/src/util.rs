use std::time::{SystemTime, UNIX_EPOCH};

#[path = "time_format.rs"]
mod time_format;
use hmac::{Hmac, Mac};
use sha1::Sha1;
use sha2::{Digest, Sha256};

/// Get UTC date time string for aliyun ots API.
/// e.g. 2023-12-03T12:12:12.123Z
pub(crate) fn get_iso8601_date_time_string() -> String {
    time_format::iso8601(time::OffsetDateTime::now_utc())
}

#[allow(dead_code)]
pub(crate) fn current_time_ms() -> u128 {
    let now = SystemTime::now();
    now.duration_since(UNIX_EPOCH).unwrap().as_millis()
}

#[allow(dead_code)]
pub(crate) fn get_http_date() -> String {
    time_format::http_date(time::OffsetDateTime::now_utc())
}

#[allow(dead_code)]
/// Hmac-SHA256 digest
pub(crate) fn hmac_sha256(key_data: &[u8], msg_data: &[u8]) -> Vec<u8> {
    type HmacSha256 = Hmac<Sha256>;
    let mut mac = HmacSha256::new_from_slice(key_data).unwrap();
    mac.update(msg_data);
    let ret = mac.finalize();
    ret.into_bytes().to_vec()
}

#[allow(dead_code)]
pub(crate) fn sha256(data: &[u8]) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let data = hasher.finalize();
    data.to_vec()
}

pub(crate) fn hmac_sha1(key_data: &[u8], msg_data: &[u8]) -> Vec<u8> {
    // Create the hasher with the key. We can use expect for Hmac algorithms as they allow arbitrary key sizes.
    let mut hasher: Hmac<Sha1> = Mac::new_from_slice(key_data).unwrap();

    // hash the message
    hasher.update(msg_data);

    // finalize the hash and convert to a static array
    hasher.finalize().into_bytes().to_vec()
}

#[allow(dead_code)]
pub(crate) fn debug_bytes(bytes: &[u8]) {
    // Print bytes in a hexadecimal format, 16 bytes per line, comma-separated
    log::debug!("Bytes dump:");
    for (i, chunk) in bytes.chunks(16).enumerate() {
        let hex_strings: Vec<String> = chunk.iter().map(|b| format!("{:02X}", b)).collect();
        log::debug!("Line {:02}: {}", i, hex_strings.join(", "));
    }
}
