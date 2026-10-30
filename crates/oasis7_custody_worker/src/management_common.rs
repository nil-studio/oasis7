//! Shared clock and canonical digest primitives for management roles.
use oasis7_custody_core::approval::bytes_digest;
use serde::Serialize;
use worker::{Error, Result};

pub(crate) fn denied() -> Error {
    Error::RustError("custody management unavailable or rejected".into())
}
pub(crate) fn clock() -> Result<u64> {
    let n = js_sys::Date::now();
    if !n.is_finite() || n < 0.0 || n.fract() != 0.0 || n > 9_007_199_254_740_991.0 {
        return Err(denied());
    }
    Ok(n as u64)
}
pub(crate) fn digest<T: Serialize>(domain: &str, value: &T) -> Result<String> {
    let raw = oasis7_wasm_abi::encode_canonical_cbor(&(domain, value)).map_err(|_| denied())?;
    Ok(bytes_digest(&raw))
}
