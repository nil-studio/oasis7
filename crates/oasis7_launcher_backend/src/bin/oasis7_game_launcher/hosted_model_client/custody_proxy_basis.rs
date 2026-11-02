//! Native-owned fixed catalog, actual account/session key owner and instance.
//! Missing actual paid Current driver is Unsupported; catalog JSON cannot grant it.
use super::*;
use ed25519_dalek::SigningKey;
use oasis7_custody_core::{DecimalU64, approval::*, provisioning::*};
use oasis7_model_provider::connection::{CustodyConnectionMetadata, WeakConnectionManager};
use serde::Deserialize;
use std::{collections::BTreeSet, io::Read, path::Path, sync::RwLock};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Policy {
    profiles: Vec<Profile>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Profile {
    preset_id: String,
    preset_revision: u64,
    runtime_digest: String,
    skill_digest: String,
    tools_digest: String,
    context_policy_digest: String,
    price_policy: ModelPricePolicyV1,
}
/// Produced only by the actual Native paid-current driver, never deserialized
/// from policy/input or replaced by a ready/verified flag.
pub(super) struct ModelGraphDescription {
    pub physical_graph: oasis7_custody_core::native_model_graph_v2::NativeModelPhysicalGraphV2,
    pub digest: String,
    pub probes: [u64; 2],
    pub inference: u64,
    pub endpoint_policy_digest: String,
}
/// Produced by a vendor-specific authenticated all-in ceiling adapter, never
/// by parsing the local operator policy. No supported adapter exists yet.
/// The private fields deliberately have no JSON or public constructor.
pub(super) struct QualifiedProviderPriceCeiling {
    policy_digest: String,
    valid_until: u64,
}
pub(super) trait ModelProxyCurrentGraph: Send + Sync {
    fn describe(
        &self,
        request: &ApprovalOptionsRequestV1,
        preset: &oasis7_model_provider::preset::ProviderPreset,
    ) -> Result<ModelGraphDescription, String>;
    // Pure cached qualification only: an additional HTTP here would be an
    // unreserved graph edge. The current provider adapters have token usage,
    // not an authenticated vendor all-in money ceiling, so they stay closed.
    fn qualified_price(
        &self,
        _preset: &oasis7_model_provider::preset::ProviderPreset,
        _budget: &oasis7_custody_core::provisioning::ModelInferenceBudgetV1,
        _now: u64,
    ) -> Result<QualifiedProviderPriceCeiling, String> {
        Err(failure())
    }
}
pub(super) struct NativeProxyBasisAuthority {
    manager: WeakConnectionManager,
    local: Option<super::custody_proxy_local::LocalRuntime>,
    policy: Policy,
    instance: SigningKey,
    // None until an actual mounted paid Context/LiveRead driver is supplied.
    graph: RwLock<Option<Arc<dyn ModelProxyCurrentGraph>>>,
}
fn failure() -> String {
    "Native model proxy basis unavailable".into()
}
fn digest<T: serde::Serialize>(domain: &str, value: &T) -> Result<String, String> {
    let digest = oasis7_agent_api::h_v1(domain, value);
    digest
        .as_str()
        .strip_prefix("blake3:")
        .map(str::to_owned)
        .ok_or_else(failure)
}
impl NativeProxyBasisAuthority {
    pub(super) fn from_env(
        manager: WeakConnectionManager,
        presets: &[oasis7_model_provider::preset::ProviderPreset],
    ) -> Result<Option<Self>, String> {
        let Some(path) = std::env::var_os("OASIS7_CUSTODY_MODEL_PROXY_POLICY") else {
            return Ok(None);
        };
        let path = Path::new(&path);
        let meta = std::fs::symlink_metadata(path).map_err(|_| failure())?;
        if !path.is_absolute()
            || !meta.is_file()
            || meta.file_type().is_symlink()
            || meta.len() > 32768
        {
            return Err(failure());
        }
        let file = std::fs::File::open(path).map_err(|_| failure())?;
        let opened = file.metadata().map_err(|_| failure())?;
        if !opened.is_file() || opened.len() > 32768 {
            return Err(failure());
        }
        let mut raw = Vec::new();
        file.take(32769)
            .read_to_end(&mut raw)
            .map_err(|_| failure())?;
        if raw.len() > 32768 {
            return Err(failure());
        }
        let policy: Policy = serde_json::from_slice(&raw).map_err(|_| failure())?;
        let mut seen = BTreeSet::new();
        if policy.profiles.is_empty()
            || policy.profiles.len() > 16
            || policy.profiles.iter().any(|p| {
                !seen.insert((p.preset_id.clone(), p.preset_revision))
                    || !presets.iter().any(|preset| {
                        preset.id == p.preset_id && preset.revision == p.preset_revision
                    })
            })
        {
            return Err(failure());
        }
        let mut seed = [0u8; 32];
        getrandom::fill(&mut seed).map_err(|_| failure())?;
        let instance = SigningKey::from_bytes(&seed);
        use zeroize::Zeroize;
        seed.zeroize();
        Ok(Some(Self {
            manager,
            local: None,
            policy,
            instance,
            graph: RwLock::new(None),
        }))
    }
    pub(super) fn with_local_runtime(mut self, identity: &Identity) -> Result<Self, String> {
        self.local = super::custody_proxy_local::LocalRuntime::from_env(identity)?;
        Ok(self)
    }
    fn instance_public_key(&self) -> String {
        hex::encode(self.instance.verifying_key().as_bytes())
    }
    pub(super) fn resolve(
        &self,
        request: &ApprovalOptionsRequestV1,
        account: &str,
        session: &str,
        now: u64,
    ) -> Result<(CustodyConnectionMetadata, ModelGraphDescription), String> {
        let CustodyIntentV1::TestSessionV2 { plan, .. } = &request.intent else {
            return Err(failure());
        };
        let budget = plan.model_budget.as_ref().ok_or_else(failure)?;
        if budget.approval_contract != ModelBudgetApprovalContractV1::NativeCustodyModelProxyV2
            || budget.native_account_id != account
            || budget.native_instance_public_key != self.instance_public_key()
            || now >= budget.deadline.0
        {
            return Err(failure());
        }
        let profile = self
            .policy
            .profiles
            .iter()
            .find(|p| {
                p.preset_id == budget.preset_id && p.preset_revision == budget.preset_revision.0
            })
            .ok_or_else(failure)?;
        let manager = self.manager.upgrade().ok_or_else(failure)?;
        let meta = manager
            .custody_connection_for_preset(account, session, &profile.preset_id)
            .map_err(|_| failure())?;
        if meta.preset.model != budget.model
            || meta.preset.revision != budget.preset_revision.0
            || budget.deadline.0 > meta.expires_at
            || budget.runtime_digest != profile.runtime_digest
            || budget.skill_digest != profile.skill_digest
            || budget.tools_digest != profile.tools_digest
            || budget.context_policy_digest != profile.context_policy_digest
            || budget.price_policy.currency != profile.price_policy.currency
            || budget.price_policy.input_micro_units_per_million
                != profile.price_policy.input_micro_units_per_million
            || budget.price_policy.output_micro_units_per_million
                != profile.price_policy.output_micro_units_per_million
            || budget.price_policy.quote_digest != profile.price_policy.quote_digest
            || budget.price_policy.valid_until.0 > profile.price_policy.valid_until.0
            || budget.price_policy.max_total_micro_units.0
                > profile.price_policy.max_total_micro_units.0
        {
            return Err(failure());
        }
        let graph = self.graph.read().map_err(|_| failure())?;
        let driver = graph.as_ref().ok_or_else(failure)?;
        let price = driver.qualified_price(&meta.preset, budget, now)?;
        if price.valid_until < budget.deadline.0
            || price.policy_digest
                != oasis7_custody_core::native_model_proxy_v2::native_model_price_policy_digest(
                    &budget.price_policy,
                )?
        {
            return Err(failure());
        }
        let graph = graph
            .as_ref()
            .ok_or_else(failure)?
            .describe(request, &meta.preset)?;
        graph.physical_graph.validate_budget(budget)?;
        if graph.digest != graph.physical_graph.digest()?
            || graph.digest != budget.physical_graph_digest
            || graph.probes != budget.probe_physical_http.map(|v| v.0)
            || graph.inference != budget.inference_physical_http.0
        {
            return Err(failure());
        }
        Ok((meta, graph))
    }
    fn connection_binding(
        &self,
        meta: &CustodyConnectionMetadata,
        graph: &ModelGraphDescription,
    ) -> Result<NativeModelProxyConnectionBindingV2, String> {
        Ok(NativeModelProxyConnectionBindingV2 {
            native_account_id: meta.account_id.clone(),
            native_instance_public_key: self.instance_public_key(),
            connection_ref_digest: digest(
                "oasis7.custody.native-model-connection.v2",
                &(
                    &meta.connection_id,
                    &meta.account_id,
                    &meta.session_id,
                    meta.version,
                    meta.generation,
                    self.instance_public_key(),
                ),
            )?,
            connection_generation: DecimalU64(meta.generation),
            endpoint_policy_digest: graph.endpoint_policy_digest.clone(),
        })
    }
}
impl super::super::hosted_identity::ModelProxyBasisAuthority for NativeProxyBasisAuthority {
    fn active_committed(&self, signal: super::super::hosted_identity::ActiveModelSignal) {
        if let Some(local) = &self.local {
            local.notify(signal);
        }
    }
    fn first_reservation_query(
        &self,
        input: &oasis7_custody_core::approval_admission::ApprovalAdmissionInput,
        owner: &super::super::hosted_identity::ModelProxyOwnerBinding,
        now: u64,
    ) -> Result<
        oasis7_custody_core::native_model_proxy_query_v2::SignedNativeModelReservationQueryV2,
        String,
    > {
        use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
        use ed25519_dalek::Signer;
        use oasis7_custody_core::native_model_proxy_query_v2::*;
        use oasis7_custody_core::native_model_proxy_v2::NativeModelCallKindV2;
        // Cached, actual local connection + qualified vendor ceiling + fixed
        // graph only. This has no network call and never probes a provider.
        let (metadata, graph) =
            self.resolve(&input.request, owner.account_id(), owner.session_ref(), now)?;
        let CustodyIntentV1::TestSessionV2 { plan, scope } = &input.request.intent else {
            return Err(failure());
        };
        let Some(NativeCustodySummaryContractV1::FullTestSessionProxyV2(contract)) =
            &input.approval.proof.model_budget_summary_contract
        else {
            return Err(failure());
        };
        let budget = plan.model_budget.as_ref().ok_or_else(failure)?;
        let mut conversation = oasis7_model_provider::protocol::probe_conversation();
        conversation.max_output_tokens = conversation
            .max_output_tokens
            .min(u32::try_from(budget.max_output_tokens.0).unwrap_or(u32::MAX));
        let encoded = zeroize::Zeroizing::new(
            oasis7_model_provider::protocol::encode_request(&metadata.preset, &conversation)
                .map_err(|_| failure())?,
        );
        let provider_request_digest = oasis7_custody_core::approval::bytes_digest(&encoded);
        let mut nonce = [0u8; 32];
        getrandom::fill(&mut nonce).map_err(|_| failure())?;
        let reservation_id = digest(
            "oasis7.custody.native-model-first-reservation.v2",
            &owner.approved_input_digest(),
        )?;
        let query = NativeModelReservationQueryV2 {
            api_version: 2,
            request_id: format!("model-query-{reservation_id}"),
            reservation_id,
            provision_id: scope.client_pairing.target.provision_id.clone(),
            custody_account_id: scope.client_pairing.target.custody_account_id.clone(),
            session_id: scope.session_id.clone(),
            approved_input_digest: owner.approved_input_digest().to_owned(),
            budget_digest: budget.digest(plan.write_deadline.0)?,
            basis_digest: contract.basis_digest.clone(),
            request_commitment: NativeModelRequestCommitmentV2::EncodedRequestV2 {
                provider_request_digest,
            },
            physical_graph: graph.physical_graph,
            call_kind: NativeModelCallKindV2::ProbeFirst,
            ordinal: DecimalU64(1),
            expected_floor: DecimalU64(0),
            query_nonce: URL_SAFE_NO_PAD.encode(nonce),
            issued_at: DecimalU64(now),
            admit_until: DecimalU64(
                now.checked_add(5000)
                    .ok_or_else(failure)?
                    .min(budget.deadline.0)
                    .min(plan.write_deadline.0),
            ),
        };
        let bytes = query.signing_bytes()?;
        let signed = SignedNativeModelReservationQueryV2 {
            query,
            signature_hex: hex::encode(self.instance.sign(&bytes).to_bytes()),
        };
        signed.verify_input_binding(input, now)?;
        Ok(signed)
    }
    fn public_metadata(&self) -> Value {
        json!({"api_version":2,"available":false,
            "contract":"native_custody_model_proxy_v2","native_instance_public_key":self.instance_public_key(),
            "profiles":self.policy.profiles.iter().map(|p|json!({"preset_id":p.preset_id,
                "preset_revision":p.preset_revision,"runtime_digest":p.runtime_digest,
                "skill_digest":p.skill_digest,"tools_digest":p.tools_digest,
                "context_policy_digest":p.context_policy_digest,"price_policy":p.price_policy})).collect::<Vec<_>>()})
    }
    fn prepare(
        &self,
        request: &ApprovalOptionsRequestV1,
        account: &str,
        session: &str,
        now: u64,
    ) -> Result<NativeModelProxyBasisV2, String> {
        let (meta, graph) = self.resolve(request, account, session, now)?;
        let CustodyIntentV1::TestSessionV2 { plan, .. } = &request.intent else {
            return Err(failure());
        };
        let budget = plan.model_budget.as_ref().ok_or_else(failure)?;
        NativeModelProxyBasisV2::for_request(
            request,
            &self.connection_binding(&meta, &graph)?,
            now,
            budget
                .deadline
                .0
                .min(meta.expires_at)
                .min(budget.price_policy.valid_until.0),
        )
    }
    fn validate_saved(
        &self,
        basis: &NativeModelProxyBasisV2,
        request: &ApprovalOptionsRequestV1,
        account: &str,
        session: &str,
        now: u64,
    ) -> Result<(), String> {
        let (meta, graph) = self.resolve(request, account, session, now)?;
        let current = self.connection_binding(&meta, &graph)?;
        basis.validate_request(request, account, now)?;
        if basis.connection_ref_digest != current.connection_ref_digest
            || basis.connection_generation != current.connection_generation
            || basis.native_instance_public_key != current.native_instance_public_key
            || basis.endpoint_policy_digest != current.endpoint_policy_digest
        {
            return Err(failure());
        }
        Ok(())
    }
}
