//! Synthetic historical signed-body storage only. No production route/profile.
use oasis7_client_api::world_service::task_context_projection::*;
use oasis7_custody_core::{
    approval::{ApprovedWorldV1, bytes_digest},
    context_body_v2::*,
};
use serde::Deserialize;
use wasm_bindgen::prelude::*;
use worker::*;
#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
enum Input {
    Prepared {
        scope: Box<ContextBodyScopeV2>,
        request: Box<TaskContextPrepareAttestationRequestV2>,
        delivery: Box<WorldContextAttestationV2<TaskContextPreparedDeliveryV2>>,
    },
    Captured {
        scope: Box<ContextBodyScopeV2>,
        prior: Box<AttestedSlotRefV2>,
        request: Box<TaskContextGetAttestationRequestV2>,
        delivery: Box<WorldContextAttestationV2<TaskContextCapturedDeliveryV2>>,
    },
    Open {
        reference: Box<AttestedSlotRefV2>,
        record: Box<ContextBodyRecordV2>,
    },
}
#[durable_object]
pub struct ContextFullSignedV2Harness {
    env: Env,
}
impl DurableObject for ContextFullSignedV2Harness {
    fn new(_state: State, env: Env) -> Self {
        Self { env }
    }
    async fn fetch(&self, mut request: Request) -> Result<Response> {
        let result=async {
            // Pin comes from the fixture's deployment configuration, not input.
            let world:ApprovedWorldV1=serde_json::from_str(&self.env.var("SYNTHETIC_CONTEXT_WORLD_PIN").map_err(|_|())?.to_string()).map_err(|_|())?;
            let raw=request.text().await.map_err(|_|())?;
            let input:Input=serde_json::from_str(&raw).map_err(|_|())?;
            let crypto=crate::context_body_crypto::ContextBodyCrypto::load(&self.env).map_err(|_|())?;
            let value=match input {
                Input::Prepared {scope,request,delivery}=>{
                    let sealed=crypto.prepare_fullsigned_slot_v2(*scope,&world,&request,&delivery).await.map_err(|_|())?;
                    serde_json::json!({"reference":sealed.reference(),"record":sealed.original_record(),
                        "actor_name":sealed.reference().scope.derived_actor_name().map_err(|_|())?})
                }
                Input::Captured {scope,prior,request,delivery}=>{
                    let sealed=crypto.capture_fullsigned_slot_v2(*scope,&world,&prior,&request,&delivery).await.map_err(|_|())?;
                    serde_json::json!({"reference":sealed.reference(),"record":sealed.original_record(),
                        "actor_name":sealed.reference().scope.derived_actor_name().map_err(|_|())?})
                }
                Input::Open {reference,record}=>{
                    let body=crypto.open_fullsigned_v2(&reference,&record,&world).await.map_err(|_|())?;
                    serde_json::json!({"bytes":body.canonical_bytes().len(),"digest":bytes_digest(body.canonical_bytes())})
                }
            };
            Ok::<_,()>(value)
        }.await;
        match result {
            Ok(v) => Response::from_json(&v),
            Err(()) => Response::error("synthetic fullsigned storage rejected", 409),
        }
    }
}
