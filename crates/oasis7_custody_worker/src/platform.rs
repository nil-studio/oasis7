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
    object_id: String,
    env: Env,
}
#[cfg(feature = "recovery-fence")]
impl DurableObject for RecoveryFence {
    fn new(state: State, env: Env) -> Self {
        let object_id = state.id().to_string();
        Self {
            state: state._inner().into(),
            initial_epoch: env
                .var("CUSTODY_INITIAL_SERVICE_EPOCH")
                .map(|v| v.to_string())
                .unwrap_or_else(|_| "0".into()),
            object_id,
            env,
        }
    }
    async fn fetch(&self, request: Request) -> Result<Response> {
        let url = request.url()?;
        if url.scheme() != "https"
            || url.host_str() != Some("fence.internal")
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Response::error("not found", 404);
        }
        let store = crate::recovery_fence_store::FenceStore::new(
            &self.state,
            &self.env,
            &self.object_id,
            &self.initial_epoch,
        );
        if request.method() == Method::Post {
            if !crate::recovery_fence_store::control_enabled(&self.env)
                || request.headers().get("Content-Type")?.as_deref() != Some("application/json")
            {
                return unavailable();
            }
            return match url.path() {
                "/internal/recovery-describe" => match store.describe() {
                    Ok(v) => Response::from_json(&v),
                    Err(_) => Response::error("recovery pending", 409),
                },
                "/internal/recovery-disable" => match store.disable() {
                    Ok(v) => Response::from_json(&v),
                    Err(_) => Response::error("recovery pending", 409),
                },
                "/internal/recovery-activate" => {
                    let raw = crate::recovery_fence_control::bounded(request).await?;
                    let signed = serde_json::from_str(&raw).map_err(|_| {
                        worker::Error::RustError("invalid recovery manifest".into())
                    })?;
                    match store.activate(signed).await {
                        Ok(v) => Response::from_json(&v),
                        Err(_) => Response::error("recovery pending", 409),
                    }
                }
                _ => Response::error("not found", 404),
            };
        }
        if request.method() != Method::Get
            || url.path() != "/internal/recovery-fence"
            || url.query().is_some()
        {
            return Ok(
                Response::from_json(&ErrorResponse::new(ErrorCode::NotFound))?.with_status(404),
            );
        }
        // The read binding never reaches the separate independently signed control lane.
        match store.read() {
            Ok(fence) => Response::from_json(&fence),
            Err(_) => unavailable(),
        }
    }
}

#[cfg(any(feature = "executor", feature = "recovery-fence"))]
fn unavailable() -> Result<Response> {
    Ok(Response::from_json(&ErrorResponse::new(ErrorCode::NotReady))?.with_status(503))
}
