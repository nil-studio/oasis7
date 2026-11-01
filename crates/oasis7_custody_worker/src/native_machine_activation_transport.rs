//! Only an Account-owned persisted Native1 dispatch token can reach this lane.
use worker::{Error, Headers, Method, Request, RequestInit, RequestRedirect, Result};
fn rejected() -> Error {
    Error::RustError("native machine activation rejected".into())
}
fn checked(value: bool) -> Result<()> {
    if value { Ok(()) } else { Err(rejected()) }
}
fn clock() -> Result<u64> {
    let n = js_sys::Date::now();
    checked(n.is_finite() && n >= 0.0 && n.fract() == 0.0 && n <= 9_007_199_254_740_991.0)?;
    Ok(n as u64)
}
const MAX_BYTES: usize = 32 * 1024;
use oasis7_custody_core::{
    native_cleanup_v2::{NATIVE_ACTIVATE_V2_PATH, SignedNativeActivationAckV2},
    provisioning::NativeMachineActivationDispatchV2,
};
pub(crate) async fn send(
    token: NativeMachineActivationDispatchV2,
) -> Result<SignedNativeActivationAckV2> {
    let now = clock()?;
    let remaining = token
        .dispatch_until()
        .get()
        .checked_sub(now)
        .filter(|n| *n > 0)
        .ok_or_else(rejected)?;
    let original = token.request();
    original.validate_shape().map_err(|_| rejected())?;
    let origin = &original.active.certificate.issuer.native_audience_origin;
    oasis7_custody_core::http_pop::CanonicalHttpTarget::new(
        origin,
        oasis7_custody_core::http_pop::HttpPopRoute::ProofChallenges,
    )
    .map_err(|_| rejected())?;
    let body = zeroize::Zeroizing::new(serde_json::to_string(original).map_err(|_| rejected())?);
    checked(body.len() <= MAX_BYTES)?;
    let headers = Headers::new();
    headers.set("Content-Type", "application/json")?;
    let mut init = RequestInit::new();
    init.with_method(Method::Post)
        .with_redirect(RequestRedirect::Manual)
        .with_headers(headers)
        .with_body(Some(body.as_str().into()));
    let request = Request::new_with_init(&format!("{origin}{NATIVE_ACTIVATE_V2_PATH}"), &init)?;
    // This is the one prepaid physical fetch: no challenge, redirect or retry.
    let response =
        crate::native_http_bounded::send_status(request, false, remaining.min(5_000)).await?;
    let ack = crate::native_http_bounded::read_body(response, MAX_BYTES).await?;
    checked(clock()? < token.dispatch_until().get())?;
    Ok(ack)
}
