#[cfg(any(feature = "executor", feature = "recovery-fence"))]
use oasis7_custody_core::http::ErrorCode;
use oasis7_custody_core::http::{self, ErrorResponse, Role, ServiceMetadata};
use worker::*;

#[cfg(feature = "identity")]
const ROLE: Role = Role::Identity;
#[cfg(feature = "owner")]
const ROLE: Role = Role::Owner;
#[cfg(feature = "executor")]
const ROLE: Role = Role::Executor;
#[cfg(feature = "recovery-fence")]
const ROLE: Role = Role::RecoveryFence;

#[event(fetch)]
async fn main(request: Request, _env: Env, _ctx: Context) -> Result<Response> {
    let url = request.url()?;
    #[cfg(feature = "identity")]
    if crate::management::is_management_path(url.path()) {
        return crate::management::handle(request, _env).await;
    }
    match http::route(request.method().as_ref(), url.path(), url.query().is_some()) {
        Ok(()) => Response::from_json(&ServiceMetadata::disabled(ROLE)),
        Err((status, code)) => {
            Ok(Response::from_json(&ErrorResponse::new(code))?.with_status(status))
        }
    }
}

#[cfg(feature = "executor")]
#[durable_object]
pub struct ExecutorKeys {
    _state: State,
}
#[cfg(feature = "executor")]
impl DurableObject for ExecutorKeys {
    fn new(state: State, _env: Env) -> Self {
        Self { _state: state }
    }
    async fn fetch(&self, _request: Request) -> Result<Response> {
        unavailable()
    }
}

#[cfg(feature = "recovery-fence")]
#[durable_object]
pub struct RecoveryFence {
    state: wasm_bindgen::JsValue,
    initial_epoch: String,
}
#[cfg(feature = "recovery-fence")]
impl DurableObject for RecoveryFence {
    fn new(state: State, env: Env) -> Self {
        Self {
            state: state._inner().into(),
            initial_epoch: env
                .var("CUSTODY_INITIAL_SERVICE_EPOCH")
                .map(|v| v.to_string())
                .unwrap_or_else(|_| "0".into()),
        }
    }
    async fn fetch(&self, request: Request) -> Result<Response> {
        let url = request.url()?;
        if request.method() != Method::Get
            || url.path() != "/internal/recovery-fence"
            || url.query().is_some()
        {
            return Ok(
                Response::from_json(&ErrorResponse::new(ErrorCode::NotFound))?.with_status(404),
            );
        }
        // Read-only: no public route or binding can activate or roll back this fence.
        // Fresh namespaces initialize frozen, independent of AccountAuthority data.
        match crate::sql_store::read_frozen_fence(&self.state, &self.initial_epoch) {
            Ok(fence) => Response::from_json(&fence),
            Err(_) => unavailable(),
        }
    }
}

#[cfg(any(feature = "executor", feature = "recovery-fence"))]
fn unavailable() -> Result<Response> {
    Ok(Response::from_json(&ErrorResponse::new(ErrorCode::NotReady))?.with_status(503))
}
