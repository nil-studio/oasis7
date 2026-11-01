//! Historical genuine Core proof + synthetic approved Executor fixture key.
//! This proves real Rust WebCrypto/SQLite once, not online Current/paid graph.
use super::*;
use oasis7_custody_core::provisioning::{
    ExecutionPrepareRecipientV2, ExecutionPrepareSignerOperationV2, ExecutionSigningBundleV2,
    SignedAccountExecutionPreparePermitV2,
};
use worker::*;
#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Operation {
    Seed,
    Initialize,
    Sign,
    Lookup,
    Counts,
    ProductionExpired,
    Fault,
}
#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Fault {
    WrongKek,
    FrozenAfterOpen,
    CredentialGenerationChanged,
    ClaimUpdateCheck,
    PublicationCheck,
    RemoveState,
    CorruptHead,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    operation: Operation,
    proof: SignedAccountExecutionPreparePermitV2,
    bundle: ExecutionSigningBundleV2,
    fault: Option<Fault>,
}
#[wasm_bindgen(module = "/platform/execution_signer_fixture_v2.mjs")]
extern "C" {
    #[wasm_bindgen(js_name = executionFixtureStateV2)]
    fn wrap_state(state: JsValue) -> JsValue;
    #[wasm_bindgen(catch, js_name = executionFixtureSeedV2)]
    fn seed_sql(state: &JsValue, payload: &str) -> super::Result<()>;
    #[wasm_bindgen(catch, js_name = executionFixtureCountsV2)]
    fn counts_sql(state: &JsValue) -> super::Result<String>;
    #[wasm_bindgen(catch, js_name = executionFixtureFaultV2)]
    fn fault_sql(state: &JsValue, kind: &str) -> super::Result<()>;
}
#[durable_object]
pub struct ExecutionPrepareSignerHarness {
    state: JsValue,
    env: Env,
    physical_actor_calls: std::cell::Cell<u64>,
}
impl DurableObject for ExecutionPrepareSignerHarness {
    fn new(state: State, env: Env) -> Self {
        Self {
            state: wrap_state(state._inner().into()),
            env,
            physical_actor_calls: std::cell::Cell::new(0),
        }
    }
    async fn fetch(&self, mut request: Request) -> worker::Result<Response> {
        let url = request.url()?;
        if request.method() == Method::Post
            && matches!(
                url.path(),
                "/internal/role-key/execution-prepare-v2"
                    | "/internal/role-key/execution-prepare-initialize-v2"
            )
            && url.scheme() == "https"
            && url.host_str() == Some("role-key.internal")
            && url.query().is_none()
            && url.fragment().is_none()
            && request.headers().get("Content-Type")?.as_deref() == Some("application/json")
        {
            self.physical_actor_calls.set(
                self.physical_actor_calls
                    .get()
                    .checked_add(1)
                    .ok_or_else(|| worker::Error::RustError("fixture counter exhausted".into()))?,
            );
            let raw = request.text().await?;
            if raw.len() > 32768 {
                return Response::error("historical RPC too large", 409);
            }
            return match crate::execution_prepare_rpc_v2::historical_receive(
                self.state.clone(),
                &self.env,
                &raw,
                url.path(),
            )
            .await
            {
                Ok(value) => Response::ok(value),
                Err(_) => Response::error("historical fixed RPC rejected", 409),
            };
        }
        match self.run(&mut request).await {
            Ok(value) => Response::from_json(&value),
            Err(_) => Response::error("historical execution signer fixture rejected", 409),
        }
    }
}
impl ExecutionPrepareSignerHarness {
    async fn run(&self, request: &mut Request) -> super::Result<serde_json::Value> {
        let raw = request.text().await.map_err(|_| rejected())?;
        if raw.len() > 32768 {
            return Err(rejected());
        }
        let input: Input = serde_json::from_str(&raw).map_err(|_| rejected())?;
        if serde_json::to_value(&input).map_err(|_| rejected())?
            != serde_json::from_str::<serde_json::Value>(&raw).map_err(|_| rejected())?
        {
            return Err(rejected());
        }
        let scope = input.proof.permit.recipient_scope.clone();
        let public = input.proof.permit.recipient_key.clone();
        let expected_operation = if matches!(input.operation, Operation::Initialize) {
            ExecutionPrepareSignerOperationV2::InitializeSigner
        } else {
            ExecutionPrepareSignerOperationV2::SignPrepare
        };
        let verified = ExecutionIssuerConfigurationV2::load(&self.env)?.verify(
            input.proof,
            input.bundle,
            ExecutionPrepareRecipientV2 {
                scope: &scope,
                key: &public,
                operation: expected_operation,
            },
            14_008,
        )?;
        let operation = verified.once_operation_digest().map_err(|_| rejected())?;
        let store = RoleKeyStore::new(self.state.clone());
        let mut answer = serde_json::json!({"accepted":true});
        match input.operation {
            Operation::Seed => {
                // The fixed fixture secret matches participant_ready's actual
                // approved Executor public key; no seed comes from Node/input.
                if public.public_key != super::public(&[0x41; 32])? {
                    return Err(rejected());
                }
                if read(&self.state, KEY).is_err() {
                    let kek = Zeroizing::new([0x93; 32]);
                    let seed = Zeroizing::new([0x41; 32]);
                    let envelope =
                        envelope_crypto::seal(&scope.context, DecimalU64(1), &kek, seed.as_slice())
                            .await
                            .map_err(|_| rejected())?;
                    let slot =
                        RoleKeySlotV1::new(scope, public, envelope).map_err(|_| rejected())?;
                    let snapshot = Snapshot {
                        schema_version: 1,
                        slot: Some(slot),
                        audit_sequence: DecimalU64(1),
                    };
                    seed_sql(&self.state, &encode(&snapshot)?)?;
                }
            }
            Operation::Initialize => {
                store.initialize_execution_historical_harness_v2(verified, 14_008)?
            }
            Operation::Sign | Operation::ProductionExpired => {
                let expires = verified.proof().permit.expires_at.get();
                let binding = verified.proof().permit.clone();
                let service_proof_digest = verified.proof_digest().map_err(|_| rejected())?;
                let calls = std::cell::Cell::new(0u32);
                let frozen = matches!(input.fault, Some(Fault::FrozenAfterOpen));
                let current = || {
                    calls.set(calls.get().checked_add(1).ok_or_else(rejected)?);
                    Ok(ExecutionPrepareSigningCurrentV2 {
                        role: RoleKeyCurrent {
                            scope: scope.clone(),
                            approved: true,
                            frozen: frozen && calls.get() > 1,
                            cancelled: false,
                            expires_at: DecimalU64(expires),
                            authority_revision: "71".repeat(32),
                        },
                        session_id: binding.session_id.clone(),
                        approved_input_digest: binding.approved_input_digest.clone(),
                        action_request_id: binding.action_request_id.clone(),
                        stage_operation_id: binding.stage_operation_id.clone(),
                        source_operation_id: binding.source_operation_id.clone(),
                        source_nonce: binding.source_nonce.clone(),
                        credential_generation: DecimalU64(
                            binding
                                .credential_generation
                                .0
                                .checked_add(u64::from(matches!(
                                    input.fault,
                                    Some(Fault::CredentialGenerationChanged)
                                )))
                                .ok_or_else(rejected)?,
                        ),
                        bundle_digest: binding.bundle_digest.clone(),
                        service_proof_digest: service_proof_digest.clone(),
                    })
                };
                let kek = Zeroizing::new(if matches!(input.fault, Some(Fault::WrongKek)) {
                    [0x94; 32]
                } else {
                    [0x93; 32]
                });
                let key = ExecutionPrepareKeyV2 {
                    scope: &scope,
                    wrapping_version: DecimalU64(1),
                    kek: &kek,
                };
                let original = if matches!(input.operation, Operation::ProductionExpired) {
                    store
                        .sign_execution_prepare_once_v2(key, verified, current)
                        .await?
                } else {
                    store
                        .execution_prepare_historical_harness_v2(key, verified, current, || {
                            Ok(14_009)
                        })
                        .await?
                };
                let bytes =
                    oasis7_wasm_abi::encode_canonical_cbor(&original).map_err(|_| rejected())?;
                answer = serde_json::json!({"known":true,"original_digest":oasis7_custody_core::approval::bytes_digest(&bytes)});
            }
            Operation::Lookup => {
                let original = store.execution_prepare_original_v2(&scope, &operation)?;
                answer = match original {
                    Some(value) => {
                        serde_json::json!({"known":true,"original_digest":oasis7_custody_core::approval::bytes_digest(&oasis7_wasm_abi::encode_canonical_cbor(&value).map_err(|_| rejected())?)})
                    }
                    None => serde_json::json!({"known":false}),
                };
            }
            Operation::Counts => {
                let counts: serde_json::Value =
                    serde_json::from_str(&counts_sql(&self.state)?).map_err(|_| rejected())?;
                let slot_digest = read(&self.state, KEY)
                    .ok()
                    .map(|s| oasis7_custody_core::approval::bytes_digest(s.as_bytes()));
                answer = serde_json::json!({"counts":counts,"slot_digest":slot_digest});
            }
            Operation::Fault => {
                let kind = match input.fault {
                    Some(Fault::ClaimUpdateCheck) => "claim_update_check",
                    Some(Fault::PublicationCheck) => "publication_check",
                    Some(Fault::RemoveState) => "remove_state",
                    Some(Fault::CorruptHead) => "corrupt_head",
                    _ => return Err(rejected()),
                };
                fault_sql(&self.state, kind)?;
            }
        }
        if matches!(input.operation, Operation::Counts) {
            answer["physical_actor_calls"] = serde_json::json!(self.physical_actor_calls.get());
        }
        Ok(answer)
    }
}
