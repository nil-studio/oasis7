//! Fixed harness-only physical markers; stage is passed explicitly across awaits.
#[cfg(any(feature = "owner", feature = "identity"))]
use oasis7_custody_core::provisioning::OwnerSetupStageV1;
use wasm_bindgen::prelude::*;
#[cfg(any(feature = "owner", feature = "identity"))]
pub(crate) fn stage(stage: Option<OwnerSetupStageV1>) -> &'static str {
    match stage {
        None => "source",
        Some(OwnerSetupStageV1::CreateTask) => "create_task",
        Some(OwnerSetupStageV1::PrepareGoal) => "prepare_goal",
        Some(OwnerSetupStageV1::PublishGoal) => "publish_goal",
        Some(OwnerSetupStageV1::Delegation) => "delegation",
        Some(OwnerSetupStageV1::Bind) => "bind",
        Some(OwnerSetupStageV1::FinalObservation) => "final_observation",
    }
}
#[cfg(feature = "owner-setup-physical-trace-harness")]
fn emit(stage: Option<&'static str>, port: &'static str, phase: &'static str) {
    if let Some(stage) = stage {
        #[wasm_bindgen]
        extern "C" {
            #[wasm_bindgen(js_namespace = console, js_name = log)]
            fn physical_log(value: &str);
        }
        physical_log(&format!("oasis7.physical-edge.v1 {port} {stage} {phase}"));
    }
}
// success means the transport future returned Ok, not HTTP 200, authority admission or commit.
// Counting uses start events; cancellation and transport failure retain an unknown terminal.
pub(crate) async fn edge<T, E>(
    stage: Option<&'static str>,
    port: &'static str,
    future: impl std::future::Future<Output = Result<T, E>>,
) -> Result<T, E> {
    #[cfg(not(feature = "owner-setup-physical-trace-harness"))]
    {
        let _ = (stage, port);
        future.await
    }
    #[cfg(feature = "owner-setup-physical-trace-harness")]
    {
        struct Pending(Option<&'static str>, &'static str, bool);
        impl Drop for Pending {
            fn drop(&mut self) {
                if !self.2 {
                    emit(self.0, self.1, "unknown");
                }
            }
        }
        emit(stage, port, "start");
        let mut pending = Pending(stage, port, false);
        let result = future.await;
        emit(
            stage,
            port,
            if result.is_ok() { "success" } else { "unknown" },
        );
        pending.2 = true;
        result
    }
}

#[cfg(feature = "owner")]
#[wasm_bindgen(module = "/platform/provision_rpc.mjs")]
extern "C" {
    #[wasm_bindgen(catch, js_name = verifyNativeApproval)]
    #[cfg(not(feature = "owner-setup-physical-trace-harness"))]
    async fn normal_verify(env: &worker::Env, raw: &str) -> Result<JsValue, JsValue>;
    #[cfg(feature = "owner-setup-physical-trace-harness")]
    #[wasm_bindgen(catch, js_name = verifyNativeApprovalTraced)]
    async fn traced_verify(env: &worker::Env, raw: &str, stage: &str) -> Result<JsValue, JsValue>;
}
#[cfg(feature = "owner")]
pub(crate) async fn verify_source(
    env: &worker::Env,
    raw: &str,
    stage: &'static str,
) -> Result<JsValue, JsValue> {
    #[cfg(feature = "owner-setup-physical-trace-harness")]
    {
        edge(
            Some(stage),
            "source.verify.service",
            traced_verify(env, raw, stage),
        )
        .await
    }
    #[cfg(not(feature = "owner-setup-physical-trace-harness"))]
    {
        let _ = stage;
        normal_verify(env, raw).await
    }
}
#[cfg(all(
    feature = "owner-setup-physical-trace-harness",
    any(feature = "identity", feature = "recovery-fence")
))]
pub(crate) fn fixed_stage(stage: &str) -> Result<&'static str, JsValue> {
    match stage {
        "reconcile" => Ok("reconcile"),
        "source" => Ok("source"),
        "create_task" => Ok("create_task"),
        "prepare_goal" => Ok("prepare_goal"),
        "publish_goal" => Ok("publish_goal"),
        "delegation" => Ok("delegation"),
        "bind" => Ok("bind"),
        "final_observation" => Ok("final_observation"),
        _ => Err(JsValue::from_str("invalid trace stage")),
    }
}
#[cfg(any(feature = "owner", feature = "identity"))]
#[wasm_bindgen::prelude::wasm_bindgen(module = "/platform/role_provision_rpc.mjs")]
extern "C" {
    #[wasm_bindgen(catch, js_name = readRecoveryFence)]
    async fn normal_fence(
        env: &worker::Env,
    ) -> Result<wasm_bindgen::JsValue, wasm_bindgen::JsValue>;
    #[cfg(feature = "owner-setup-physical-trace-harness")]
    #[wasm_bindgen(catch, js_name = readRecoveryFenceTraced)]
    async fn traced_fence(
        env: &worker::Env,
        stage: &str,
    ) -> Result<wasm_bindgen::JsValue, wasm_bindgen::JsValue>;
}
#[cfg(any(feature = "owner", feature = "identity"))]
pub(crate) async fn read_fence(
    env: &worker::Env,
    trace: Option<&'static str>,
) -> Result<wasm_bindgen::JsValue, wasm_bindgen::JsValue> {
    #[cfg(feature = "owner-setup-physical-trace-harness")]
    if let Some(stage) = trace {
        return edge(trace, "fence.service", traced_fence(env, stage)).await;
    }
    let _ = trace;
    normal_fence(env).await
}

#[cfg(all(
    feature = "owner-setup-physical-trace-harness",
    feature = "recovery-fence"
))]
pub(crate) async fn read_actor(
    env: worker::Env,
    stage: String,
) -> std::result::Result<String, wasm_bindgen::JsValue> {
    use worker::{Method, Request};
    let stage = fixed_stage(&stage)?;
    let stub = env
        .durable_object("FENCES")
        .map_err(|_| wasm_bindgen::JsValue::from_str("independent recovery trace unavailable"))?
        .id_from_name("service-fence-v1")
        .map_err(|_| wasm_bindgen::JsValue::from_str("independent recovery trace unavailable"))?
        .get_stub()
        .map_err(|_| wasm_bindgen::JsValue::from_str("independent recovery trace unavailable"))?;
    let response = edge(
        Some(stage),
        "fence.actor",
        stub.fetch_with_request(
            Request::new(
                "https://fence.internal/internal/recovery-fence",
                Method::Get,
            )
            .map_err(|_| {
                wasm_bindgen::JsValue::from_str("independent recovery trace unavailable")
            })?,
        ),
    )
    .await
    .map_err(|_| wasm_bindgen::JsValue::from_str("independent recovery trace unavailable"))?;
    if response.status_code() != 200 {
        return Err(wasm_bindgen::JsValue::from_str(
            "independent recovery trace unavailable",
        ));
    }
    #[wasm_bindgen(module = "/platform/sql.mjs")]
    extern "C" {
        #[wasm_bindgen(catch, js_name = readBoundedText)]
        async fn read_body(
            body: &wasm_bindgen::JsValue,
            limit: u32,
        ) -> std::result::Result<wasm_bindgen::JsValue, wasm_bindgen::JsValue>;
    }
    let raw: web_sys::Response = response.into();
    let text = read_body(raw.as_ref(), 1024)
        .await?
        .as_string()
        .ok_or_else(|| wasm_bindgen::JsValue::from_str("independent recovery trace unavailable"))?;
    let _: oasis7_custody_core::recovery::RecoveryFence = serde_json::from_str(&text)
        .map_err(|_| wasm_bindgen::JsValue::from_str("independent recovery trace unavailable"))?;
    Ok(text)
}
