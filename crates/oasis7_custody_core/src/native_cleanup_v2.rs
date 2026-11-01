//! Explicit machine-phase certificates. Signature validation alone does not
//! attest payment, live Account authority, Native state or an external floor.
use crate::DecimalU64;
use serde::{Deserialize, Serialize};
#[path = "native_cleanup_v2_pin.rs"]
mod pin;
pub use pin::*;
#[path = "native_cleanup_v2_phase.rs"]
mod phase;
pub use phase::*;
#[path = "native_cleanup_v2_floor.rs"]
mod floor;
pub use floor::*;
#[path = "native_cleanup_v2_registration.rs"]
mod registration;
pub use registration::*;
#[path = "native_cleanup_v2_request.rs"]
mod request;
pub use request::*;
#[path = "native_cleanup_v2_pending_reference.rs"]
mod pending_reference;
pub use pending_reference::*;
#[path = "native_cleanup_v2_activation.rs"]
mod activation;
pub use activation::*;
pub const NATIVE_REGISTER_V2_PATH: &str =
    "/api/public/hosted-account/v2/entry/read-and-register-test-session-v2";
pub const NATIVE_ACTIVATE_V2_PATH: &str =
    "/api/public/hosted-account/v2/custody/cleanup/activate-v2";
pub const NATIVE_CURRENT_V2_PATH: &str = "/api/public/hosted-account/v2/custody/cleanup/current-v2";
pub const ACCOUNT_NATIVE_CURRENT_V2_PATH: &str = "/api/custody/cleanup/native-current-fact-v2";
pub(crate) fn id(s: &str) -> bool {
    !s.is_empty() && s.len() <= 256 && s.trim() == s && !s.chars().any(char::is_control)
}
pub(crate) fn hex64(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub(crate) fn hash<T: Serialize>(value: &T) -> Result<String, String> {
    oasis7_wasm_abi::canonical_hash(value).map_err(|e| e.to_string())
}
pub(crate) fn invalid() -> String {
    "native cleanup v2 binding unavailable".into()
}
pub(crate) fn bytes<T: Serialize>(domain: &str, value: &T) -> Result<Vec<u8>, String> {
    oasis7_wasm_abi::encode_canonical_cbor(&(domain, value)).map_err(|e| e.to_string())
}
pub(crate) fn decode<const N: usize>(s: &str) -> Result<[u8; N], String> {
    if s.len() != N * 2
        || !s
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(invalid());
    }
    let mut out = [0u8; N];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).map_err(|_| invalid())?;
    }
    Ok(out)
}
pub(crate) fn verify<T: Serialize>(
    domain: &str,
    payload: &T,
    signature: &str,
    key: &str,
) -> Result<(), String> {
    ed25519_dalek::VerifyingKey::from_bytes(&decode::<32>(key)?)
        .map_err(|_| invalid())?
        .verify_strict(
            &bytes(domain, payload)?,
            &ed25519_dalek::Signature::from_bytes(&decode::<64>(signature)?),
        )
        .map_err(|_| invalid())
}

#[path = "native_cleanup_v2_account_floor.rs"]
mod account_floor;
pub use account_floor::*;

#[path = "native_cleanup_v2_term_request.rs"]
mod term_request;
pub use term_request::*;

#[path = "native_cleanup_v2_term_fact.rs"]
mod term_fact;
pub use term_fact::*;

#[path = "native_cleanup_v2_preflight_access.rs"]
mod preflight_access;
pub use preflight_access::*;
#[path = "native_cleanup_v2_preflight_receipt.rs"]
mod preflight_receipt;
pub use preflight_receipt::*;
