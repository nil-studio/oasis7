//! Closed Resource V2 contracts only. No state migration, paid issuer or I/O.
use crate::DecimalU64;
use crate::approval::*;
use oasis7_client_api::world_service::resource_basis as wire;
use serde::{Deserialize, Serialize};
#[path = "owner_setup_v2_plan.rs"]
mod owner_plan;
pub use owner_plan::*;
#[path = "test_session_v2_plan.rs"]
mod session_plan;
pub use session_plan::*;
#[path = "prepared_resource_basis.rs"]
mod prepared;
pub use prepared::*;
#[path = "paid_resource_read_proof.rs"]
mod paid;
pub use paid::*;
#[path = "resource_v2_scope.rs"]
mod scope;
pub use scope::*;
#[cfg(test)]
#[path = "resource_v2_schema.rs"]
pub(crate) mod schema;
fn invalid() -> String {
    "invalid resource v2 contract".into()
}
fn id(s: &str) -> bool {
    !s.trim().is_empty() && s.len() <= 256 && !s.chars().any(char::is_control)
}
fn hex(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn digest<T: Serialize>(domain: &str, v: &T) -> Result<String, String> {
    oasis7_wasm_abi::encode_canonical_cbor(&(domain, v))
        .map_err(|e| e.to_string())
        .map(|v| crate::approval::bytes_digest(&v))
}
fn required_option<'de, D, T>(d: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(d)
}
