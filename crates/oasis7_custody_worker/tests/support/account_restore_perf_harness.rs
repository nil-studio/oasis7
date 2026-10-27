//! Input remains in isolated fixture memory; only fixed non-secret metrics leave.
use serde::Deserialize;
use worker::{Error, Request, Response, Result};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Measure {
    api_version: u8,
    snapshot: String,
    repeats: u8,
    #[serde(default)]
    mode: crate::provision_approval_store::performance::MeasureMode,
}
pub(super) async fn handle(mut request: Request) -> Result<Response> {
    if request.method() != worker::Method::Post {
        return Response::error("method not allowed", 405);
    }
    let raw = request.text().await?;
    if raw.len() > 4 * 1024 * 1024 + 1024 {
        return Response::error("measurement input exceeds bound", 413);
    }
    let body: Measure = serde_json::from_str(&raw)
        .map_err(|_| Error::RustError("invalid measurement input".into()))?;
    if body.api_version != 1 {
        return Response::error("unsupported measurement version", 400);
    }
    match crate::provision_approval_store::performance::measure(
        &body.snapshot,
        body.repeats,
        body.mode,
    ) {
        Ok(result) => Response::from_json(&result),
        // Expected invalid-schema/state probes are an explicit fixed fixture
        // response, not a rejected service-binding promise. Never echo input or
        // deserialize/verification error details into the measurement report.
        Err(_) => Response::error("restore measurement rejected", 400),
    }
}
