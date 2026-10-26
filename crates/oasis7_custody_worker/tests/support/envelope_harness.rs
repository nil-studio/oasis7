//! Synthetic plaintext and KEKs only; compiled solely into the approval test artifact.
use oasis7_custody_core::{
    DecimalU64,
    envelope::{EnvelopeAlgorithmV1, EnvelopeContextV1, KeyEnvelopeV1, KeyIdentity},
};
use serde::Deserialize;
use serde_json::json;
use worker::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    identity: KeyIdentity,
    #[serde(default)]
    envelope: Option<KeyEnvelopeV1>,
    #[serde(default)]
    plaintext: Vec<u8>,
    #[serde(default)]
    key: u8,
}

pub async fn handle(mut request: Request) -> Result<Response> {
    let path = request.path();
    let input: Input = match request.json().await {
        Ok(value) => value,
        Err(_) => return Ok(Response::from_json(&json!({"rejected": true}))?.with_status(409)),
    };
    let context = EnvelopeContextV1 {
        schema_version: 1,
        identity: input.identity,
        algorithm: EnvelopeAlgorithmV1::Aes256Gcm,
    };
    // Public fixed interoperability fixtures, never environment/operator material.
    let key = [if input.key == 2 { 92_u8 } else { 91_u8 }; 32];
    let result = async {
        match path.as_str() {
            "/envelope/seal" => {
                let sealed =
                    crate::envelope_crypto::seal(&context, DecimalU64(1), &key, &input.plaintext)
                        .await?;
                Ok(json!({"envelope": sealed}))
            }
            "/envelope/open" => {
                let envelope = input
                    .envelope
                    .as_ref()
                    .ok_or(crate::envelope_crypto::EnvelopeRejected)?;
                let plaintext = crate::envelope_crypto::open(envelope, &context, &key).await?;
                Ok(json!({"plaintext": plaintext.as_bytes()}))
            }
            "/envelope/rewrap" => {
                let envelope = input
                    .envelope
                    .as_ref()
                    .ok_or(crate::envelope_crypto::EnvelopeRejected)?;
                let updated = crate::envelope_crypto::rewrap(
                    envelope,
                    &context,
                    &key,
                    DecimalU64(2),
                    &[92_u8; 32],
                )
                .await?;
                Ok(json!({"envelope": updated}))
            }
            _ => Err(crate::envelope_crypto::EnvelopeRejected),
        }
    }
    .await;
    match result {
        Ok(value) => Response::from_json(&value),
        Err(_) => Ok(Response::from_json(&json!({"rejected": true}))?.with_status(409)),
    }
}
