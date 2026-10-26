//! Scheduled, fixed authority-to-authority delivery; never a browser or Executor capability.
use super::*;
use oasis7_custody_core::source_revocation::*;
use serde::Serialize;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(module = "/platform/directory_rpc.mjs")]
extern "C" {
    #[wasm_bindgen(catch, js_name = closeSourceEvent)]
    async fn deliver(env: &JsValue, raw: &str) -> std::result::Result<JsValue, JsValue>;
}

async fn post<T: Serialize, R: DeserializeOwned>(
    config: &NativeIdentityConfig,
    path: &str,
    secret: &str,
    value: &T,
) -> Result<R> {
    let headers = Headers::new();
    headers.set("Content-Type", "application/json")?;
    headers.set("Authorization", &format!("Bearer {secret}"))?;
    let mut init = RequestInit::new();
    init.with_method(Method::Post)
        .with_headers(headers)
        .with_redirect(RequestRedirect::Manual)
        .with_body(Some(serde_json::to_string(value)?.into()));
    read_native(Request::new_with_init(
        &format!("{}{PREFIX}{path}", config.origin),
        &init,
    )?)
    .await
}

pub(crate) async fn pump(env: &Env) -> Result<()> {
    let names = [
        "CUSTODY_NATIVE_REVOCATION_POLL_SECRET",
        "CUSTODY_NATIVE_REVOCATION_ACK_SECRET",
    ];
    if names.iter().all(|name| env.secret(name).is_err()) {
        return Ok(());
    }
    let config = NativeIdentityConfig::from_env(env)?.ok_or_else(rejected)?;
    let secrets = names
        .iter()
        .map(|name| env.secret(name).map(|v| v.to_string()))
        .collect::<Result<Vec<_>>>()?;
    for secret in &secrets {
        let value: [u8; 32] = hex::decode(secret)
            .map_err(|_| rejected())?
            .try_into()
            .map_err(|_| rejected())?;
        checked(hex::encode(value) == *secret && *secret != config.adapter_secret)?;
    }
    checked(secrets[0] != secrets[1])?;
    let batch: SourceRevocationsPollResponseV1 = post(
        &config,
        "/revocations/poll",
        &secrets[0],
        &SourceRevocationsPollRequestV1 {
            api_version: 1,
            limit: 8,
        },
    )
    .await?;
    checked(batch.api_version == 1 && batch.events.len() <= 8)?;
    let mut unavailable = false;
    for signed in batch.events {
        let event = &signed.event;
        event.validate().map_err(|_| rejected())?;
        config.identity(&event.identity_namespace, event.identity_instance_epoch)?;
        checked(
            event.environment == config.environment
                && event.owner_audience == config.audience
                && event.key_id == config.key_id
                && event.observed_at.0 <= clock()?.saturating_add(MAX_SKEW_MS),
        )?;
        crate::approval_crypto::verify_ed25519(
            &config.public_key,
            &signed.signature,
            &event.signing_bytes().map_err(|_| rejected())?,
        )
        .await
        .map_err(|_| rejected())?;
        let digest = signed.digest().map_err(|_| rejected())?;
        let delivered = deliver(env.as_ref(), &serde_json::to_string(&signed)?).await;
        let Ok(delivered) = delivered else {
            unavailable = true;
            continue;
        };
        let Some(raw) = delivered.as_string() else {
            unavailable = true;
            continue;
        };
        if raw.len() > MAX_BYTES {
            unavailable = true;
            continue;
        }
        let receipt = match serde_json::from_str::<Option<SignedSourceClosureAckV1>>(&raw) {
            Ok(Some(receipt)) => receipt,
            Ok(None) => continue,
            Err(_) => {
                unavailable = true;
                continue;
            }
        };
        let ack = &receipt.ack;
        // The native authority verifies the fixed Owner signature and full source binding.
        // Identity's binding check also prevents unrelated receipts reaching that write endpoint.
        if ack.event_id != event.event_id
            || ack.event_digest != digest
            || ack.identity_namespace != event.identity_namespace
            || ack.environment != event.environment
            || ack.identity_instance_epoch != event.identity_instance_epoch
            || ack.manager_account_id != event.manager_account_id
            || ack.owner_audience != event.owner_audience
        {
            unavailable = true;
            continue;
        }
        let result: Result<SourceRevocationAckResultV1> =
            post(&config, "/revocations/ack", &secrets[1], &receipt).await;
        if let Ok(result) = result {
            checked(
                result.api_version == 1
                    && result.event_id == event.event_id
                    && result.event_digest == digest,
            )?;
        } else {
            unavailable = true;
        }
    }
    checked(!unavailable)
}
