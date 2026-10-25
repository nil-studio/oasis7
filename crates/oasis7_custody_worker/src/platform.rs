use oasis7_custody_core::http::{self, ErrorCode, ErrorResponse, Role, ServiceMetadata};
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
    match http::route(
        &request.method().to_string(),
        url.path(),
        url.query().is_some(),
    ) {
        Ok(()) => Response::from_json(&ServiceMetadata::disabled(ROLE)),
        Err((status, code)) => {
            Ok(Response::from_json(&ErrorResponse::new(code))?.with_status(status))
        }
    }
}

// No grant/secret/network RPC is exported by these role-local storage classes.
// In this foundation slice they reject all requests, including service bindings.
#[cfg(feature = "owner")]
#[durable_object]
pub struct AccountAuthority {
    _state: State,
}
#[cfg(feature = "owner")]
impl DurableObject for AccountAuthority {
    fn new(state: State, _env: Env) -> Self {
        Self { _state: state }
    }
    async fn fetch(&self, _request: Request) -> Result<Response> {
        unavailable()
    }
}

#[cfg(feature = "identity")]
#[durable_object]
pub struct NativeSession {
    _state: State,
}
#[cfg(feature = "identity")]
impl DurableObject for NativeSession {
    fn new(state: State, _env: Env) -> Self {
        Self { _state: state }
    }
    async fn fetch(&self, _request: Request) -> Result<Response> {
        unavailable()
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
    _state: State,
}
#[cfg(feature = "recovery-fence")]
impl DurableObject for RecoveryFence {
    fn new(state: State, _env: Env) -> Self {
        Self { _state: state }
    }
    async fn fetch(&self, _request: Request) -> Result<Response> {
        unavailable()
    }
}

fn unavailable() -> Result<Response> {
    Ok(Response::from_json(&ErrorResponse::new(ErrorCode::NotReady))?.with_status(503))
}
