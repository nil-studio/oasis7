//! Five observed entry routes settle before any fallible current/outbound.
use super::*;
use crate::native_model_proxy_query_v2::SignedNativeModelReservationQueryV2;
use crate::time_units::UnixMillis;
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::provisioning) enum NativeModelEntryPortV2 {
    ProviderAbuseActor,
    RealmAbuseActor,
    IdentityOwnerService,
    OwnerAccountActor,
}
const PORTS: [NativeModelEntryPortV2; 4] = [
    NativeModelEntryPortV2::ProviderAbuseActor,
    NativeModelEntryPortV2::RealmAbuseActor,
    NativeModelEntryPortV2::IdentityOwnerService,
    NativeModelEntryPortV2::OwnerAccountActor,
];
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::provisioning) struct NativeModelHttpAdmissionV2 {
    pub query: SignedNativeModelReservationQueryV2,
    pub signed_attempt_digest: String,
    pub credential_generation: DecimalU64,
    pub charged_at: UnixMillis,
    pub port_ids: [String; 4],
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::provisioning) struct NativeModelEntryPortRecordV2 {
    pub parent_ingress: String,
    pub port: NativeModelEntryPortV2,
    pub query_digest: String,
    pub signed_attempt_digest: String,
    pub charged_at: UnixMillis,
}
fn port_id(
    s: &TestSessionAuthorityV2,
    ingress: &str,
    q: &SignedNativeModelReservationQueryV2,
    port: NativeModelEntryPortV2,
) -> Result<String> {
    Ok(format!(
        "native-model-entry-v2:{}:{}:{}:{}",
        s.input_digest()?,
        digest(&("model-http-ingress", ingress))?,
        digest(&("model-http-query", q))?,
        digest(&("model-http-route", port))?
    ))
}
impl AccountProvisionApprovalState {
    pub fn charge_test_session_v2_native_model_http_attempt(
        &mut self,
        id: &str,
        proof: SessionProofRequest<'_>,
        now: UnixMillis,
    ) -> Result<()> {
        let query: SignedNativeModelReservationQueryV2 = lifecycle::original(&proof)?;
        let s = self.test_session_v2_record(id)?;
        let credential = super::native_model_http_v2::authenticated_model_receipt_v2(
            self,
            s,
            &query,
            proof.possession,
            now,
        )?;
        let model = s.native_model_budget.as_ref().ok_or_else(invalid)?;
        let ingress = proof.ingress_attempt_id;
        let signed_attempt_digest = proof.possession.transport_attempt_digest();
        if ingress.is_empty() || ingress.len() > 256 || ingress.chars().any(char::is_control) {
            return Err(invalid());
        }
        if let Some(old) = model.http_admissions.get(ingress) {
            return if old.query == query
                && old.signed_attempt_digest == signed_attempt_digest
                && old.credential_generation == credential.generation
            {
                Ok(())
            } else {
                Err(ProvisionApprovalError::Conflict)
            };
        }
        let same_original = model
            .http_admissions
            .values()
            .filter(|old| {
                old.query.query.request_id == query.query.request_id
                    || old.query.query.query_nonce == query.query.query_nonce
            })
            .collect::<Vec<_>>();
        if same_original.iter().any(|old| old.query != query) {
            return Err(ProvisionApprovalError::Conflict);
        }
        if same_original.is_empty() {
            for id_value in [&query.query.request_id, &query.query.query_nonce] {
                self.test_session_v2_transport_id_available(id, id_value)?;
            }
        }
        self.test_session_v2_local(id, SessionPurposeV1::Execution, now)?;
        self.test_session_v2_transport_id_available(id, ingress)?;
        let port_ids = [
            port_id(s, ingress, &query, PORTS[0])?,
            port_id(s, ingress, &query, PORTS[1])?,
            port_id(s, ingress, &query, PORTS[2])?,
            port_id(s, ingress, &query, PORTS[3])?,
        ];
        for port in &port_ids {
            self.test_session_v2_transport_id_available(id, port)?;
        }
        let record = NativeModelHttpAdmissionV2 {
            query: query.clone(),
            signed_attempt_digest,
            credential_generation: credential.generation,
            charged_at: now,
            port_ids,
        };
        let mut next = self.clone();
        let saved = next.test_session_v2_record_mut(id)?;
        // 1 public HTTP + 2 CLIENT_ENROLLMENTS abuse Actor fetches + Owner service/Account Actor.
        // The mounted dispatcher must independently prove both routes with its spy.
        saved
            .roots
            .get_mut(&SessionPurposeV1::Execution)
            .ok_or_else(invalid)?
            .budgets
            .transport
            .charge(entry_units()?)
            .map_err(|_| invalid())?;
        let model = saved.native_model_budget.as_mut().ok_or_else(invalid)?;
        for (port, key) in PORTS.into_iter().zip(&record.port_ids) {
            model.entry_ports.insert(
                key.clone(),
                NativeModelEntryPortRecordV2 {
                    parent_ingress: ingress.into(),
                    port,
                    query_digest: digest(&("model-http-query", &record.query))?,
                    signed_attempt_digest: record.signed_attempt_digest.clone(),
                    charged_at: now,
                },
            );
        }
        model.http_admissions.insert(ingress.into(), record);
        append_session_event_v2(
            saved,
            SessionEventKindV2::NativeModelHttpSettled {
                ingress_attempt_id: ingress.into(),
            },
            now,
        )?;
        if saved
            .roots
            .get(&SessionPurposeV1::Execution)
            .ok_or_else(invalid)?
            .budgets
            .transport
            .remaining()
            .map_err(|_| invalid())?
            == 0
            && saved.closing.is_none()
        {
            saved.closing = Some((TestSessionClosingReasonV1::BudgetExhausted, now));
        }
        next.validate_persistence()?;
        *self = next;
        Ok(())
    }
}
pub(in crate::provisioning) fn validate_entry_fees(
    s: &TestSessionAuthorityV2,
    model: &SessionNativeModelBudgetV2,
) -> Result<()> {
    if model.http_admissions.len() > 400
        || model.entry_ports.len()
            != model
                .http_admissions
                .len()
                .checked_mul(PORTS.len())
                .ok_or_else(invalid)?
    {
        return Err(invalid());
    }
    let mut requests = BTreeMap::new();
    let mut nonces = BTreeMap::new();
    for (ingress, record) in &model.http_admissions {
        let original = digest(&record.query)?;
        for (inventory, key) in [
            (&mut requests, &record.query.query.request_id),
            (&mut nonces, &record.query.query.query_nonce),
        ] {
            if inventory
                .insert(key, original.clone())
                .is_some_and(|old| old != original)
            {
                return Err(invalid());
            }
        }
        record
            .query
            .verify_input_binding(&s.input, record.charged_at.get())
            .map_err(|_| invalid())?;
        if ingress.is_empty()
            || ingress.len() > 256
            || ingress.chars().any(char::is_control)
            || !hex64(&record.signed_attempt_digest)
            || record.credential_generation.0 == 0
        {
            return Err(invalid());
        }
        for (port, key) in PORTS.into_iter().zip(&record.port_ids) {
            if key != &port_id(s, ingress, &record.query, port)? {
                return Err(invalid());
            }
            let actual = model.entry_ports.get(key).ok_or_else(invalid)?;
            if actual.parent_ingress != *ingress
                || actual.port != port
                || actual.charged_at != record.charged_at
                || actual.query_digest != digest(&("model-http-query", &record.query))?
                || actual.signed_attempt_digest != record.signed_attempt_digest
            {
                return Err(invalid());
            }
        }
    }
    for (key, port) in &model.entry_ports {
        if !model
            .http_admissions
            .get(&port.parent_ingress)
            .is_some_and(|r| r.port_ids.iter().any(|id| id == key))
        {
            return Err(invalid());
        }
    }
    Ok(())
}

pub(super) fn entry_units() -> Result<u64> {
    1u64.checked_add(u64::try_from(PORTS.len()).map_err(|_| invalid())?)
        .ok_or_else(invalid)
}
