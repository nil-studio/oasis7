//! Pure bounded HTTP I/O, with no Native identity configuration or credential.
use serde::de::DeserializeOwned;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use worker::{AbortSignal, Error, Fetch, Request, Result};
fn rejected() -> Error {
    Error::RustError("native bounded transport rejected".into())
}
fn checked(value: bool) -> Result<()> {
    if value { Ok(()) } else { Err(rejected()) }
}
pub(crate) async fn send_status(
    request: Request,
    allow_not_found: bool,
    timeout_ms: u64,
) -> Result<worker::Response> {
    checked((1..=5_000).contains(&timeout_ms))?;
    let constructor =
        js_sys::Reflect::get(&js_sys::global(), &"AbortSignal".into()).map_err(|_| rejected())?;
    let timeout: js_sys::Function = js_sys::Reflect::get(&constructor, &"timeout".into())
        .map_err(|_| rejected())?
        .dyn_into()
        .map_err(|_| rejected())?;
    let signal: web_sys::AbortSignal = timeout
        .call1(&constructor, &(timeout_ms as f64).into())
        .map_err(|_| rejected())?
        .dyn_into()
        .map_err(|_| rejected())?;
    let response = Fetch::Request(request)
        .send_with_signal(&AbortSignal::from(signal))
        .await
        .map_err(|_| rejected())?;
    checked(
        (response.status_code() == 200 || (allow_not_found && response.status_code() == 404))
            && response.headers().get("Content-Type")?.is_some_and(|v| {
                v.split(';')
                    .next()
                    .is_some_and(|v| v.eq_ignore_ascii_case("application/json"))
            }),
    )?;
    Ok(response)
}
pub(crate) async fn read_body<T: DeserializeOwned>(
    response: worker::Response,
    limit: usize,
) -> Result<T> {
    let bytes = read_body_bytes(response, limit).await?;
    serde_json::from_slice(&bytes).map_err(|_| rejected())
}
/// The same bounded physical read, allowing a fixed protocol to inspect its
/// original JSON before any Value conversion can erase duplicate/unknown keys.
pub(crate) async fn read_body_bytes(
    response: worker::Response,
    limit: usize,
) -> Result<zeroize::Zeroizing<Vec<u8>>> {
    checked((1..=65_536).contains(&limit))?;
    let response: web_sys::Response = response.into();
    let body = response.body().ok_or_else(rejected)?;
    let reader_fn: js_sys::Function = js_sys::Reflect::get(body.as_ref(), &"getReader".into())
        .map_err(|_| rejected())?
        .dyn_into()
        .map_err(|_| rejected())?;
    let reader = reader_fn.call0(body.as_ref()).map_err(|_| rejected())?;
    let read: js_sys::Function = js_sys::Reflect::get(&reader, &"read".into())
        .map_err(|_| rejected())?
        .dyn_into()
        .map_err(|_| rejected())?;
    let mut bytes = zeroize::Zeroizing::new(Vec::new());
    loop {
        let promise: js_sys::Promise = read
            .call0(&reader)
            .map_err(|_| rejected())?
            .dyn_into()
            .map_err(|_| rejected())?;
        let part = JsFuture::from(promise).await.map_err(|_| rejected())?;
        if js_sys::Reflect::get(&part, &"done".into())
            .map_err(|_| rejected())?
            .as_bool()
            == Some(true)
        {
            break;
        }
        let value: js_sys::Uint8Array = js_sys::Reflect::get(&part, &"value".into())
            .map_err(|_| rejected())?
            .dyn_into()
            .map_err(|_| rejected())?;
        if bytes.len().saturating_add(value.length() as usize) > limit {
            if let Ok(cancel) = js_sys::Reflect::get(&reader, &"cancel".into())
                && let Ok(cancel) = cancel.dyn_into::<js_sys::Function>()
                && let Ok(promise) = cancel.call0(&reader)
                && let Ok(promise) = promise.dyn_into::<js_sys::Promise>()
            {
                let _ = JsFuture::from(promise).await;
            }
            return Err(rejected());
        }
        let part_bytes = zeroize::Zeroizing::new(value.to_vec());
        bytes.extend_from_slice(part_bytes.as_slice());
        value.fill(0, 0, value.length());
    }
    Ok(bytes)
}
