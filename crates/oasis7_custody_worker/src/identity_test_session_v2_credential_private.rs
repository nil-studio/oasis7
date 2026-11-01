//! V2 private envelope reader; no SID/CSRF leaves this Identity actor.
use super::{check, observation::Observation};
use crate::{
    identity_native_runtime::private_session_read_v2, owner_test_session_rpc::TestSessionSelectorV1,
};
pub(in crate::identity_native_runtime) use private_session_read_v2::SessionSecrets;
use wasm_bindgen::JsValue;
use worker::Env;
pub(in crate::identity_native_runtime) async fn read(
    state: JsValue,
    env: &Env,
    selector: &TestSessionSelectorV1,
    first: &Observation,
) -> Result<SessionSecrets, JsValue> {
    private_session_read_v2::read_with(
        state,
        env,
        selector,
        &first.descriptor.session,
        || check(env, selector, first),
        |_, _| Ok(()),
    )
    .await
}
