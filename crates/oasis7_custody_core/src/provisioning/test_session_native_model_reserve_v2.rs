//! First durable whole-session reservation. Paid pending is not a floor/grant.
use super::*;
use crate::native_model_proxy_query_v2::SignedNativeModelReservationQueryV2;
use crate::time_units::UnixMillis;
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::provisioning) struct NativeModelReservationRecordV2 {
    pub query: SignedNativeModelReservationQueryV2,
    pub host_ingress_attempt_id: String,
    pub host_signed_attempt_digest: String,
    pub credential_generation: DecimalU64,
    pub claimed_at: UnixMillis,
    pub physical_units: u64,
    pub port_ids: Vec<String>,
    pub accepted_ordinal: u64,
    pub maximum_tokens: u64,
    pub maximum_micro_units: u64,
    pub floor: u64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeModelReserveOutcomeV2 {
    RecordedPending,
    ExactHistory,
}
impl AccountProvisionApprovalState {
    /// Invoked only by the fixed internal model-reservations Account handler.
    /// The surrounding Worker SQL must commit before returning any paid token.
    pub fn reserve_test_session_v2_native_model_graph(
        &mut self,
        id: &str,
        proof: SessionProofRequest<'_>,
        now: UnixMillis,
    ) -> Result<NativeModelReserveOutcomeV2> {
        let request: SignedNativeModelReservationQueryV2 = lifecycle::original(&proof)?;
        let s = self.test_session_v2_record(id)?;
        super::v2_credential_http::native_registration(s)?;
        request
            .verify_input_binding(&s.input, now.get())
            .map_err(|_| invalid())?;
        let query = &request.query;
        let possession = proof.possession;
        let root = s
            .roots
            .get(&SessionPurposeV1::Execution)
            .ok_or_else(invalid)?;
        let credential = root.credential.as_ref().ok_or_else(invalid)?;
        let pair = &s.header()?.1.client_pairing;
        let target = crate::http_pop::CanonicalHttpTarget::new(
            &s.enrollment.registration.public_origin,
            crate::http_pop::HttpPopRoute::ScopedClient {
                provision_id: self.draft.entry.provision_id.clone(),
                custody_account_id: self.draft.entry.custody_account_id.clone(),
                route: Box::new(crate::http_pop::HttpPopRoute::ModelReservations(id.into())),
            },
        )
        .map_err(|_| invalid())?;
        if root.revoked
            || root.approved_input_digest != s.input_digest()?
            || possession.profile() != HttpPopProfileV1::NativeModelCustodyV2
            || possession.binding_id() != id
            || possession.key_id() != pair.client_key_id
            || possession.public_key_hex() != pair.client_public_key
            || possession.request_id() != query.request_id
            || possession.target_uri() != target.uri()
            || possession.method() != target.method()
            || super::hex_bytes(&credential.digest)? != *possession.token_digest()
            || possession
                .expires()
                .checked_millis()
                .map_err(|_| invalid())?
                .get()
                <= now.get()
        {
            return Err(invalid());
        }
        let model = s.native_model_budget.as_ref().ok_or_else(invalid)?;
        // The first five observed routes were committed by the fee-only SQL.
        // This second transaction cannot manufacture that settlement or refund it.
        let paid = model
            .http_admissions
            .get(proof.ingress_attempt_id)
            .ok_or_else(invalid)?;
        if paid.query != request
            || paid.signed_attempt_digest != possession.transport_attempt_digest()
            || paid.credential_generation != credential.generation
            || paid.charged_at.get() > now.get()
        {
            return Err(invalid());
        }
        if let Some(old) = model.reservations.get(&query.reservation_id) {
            // Read-only exact N original, authenticated with this paid HTTP
            // receipt. A new ingress pays five but never replaces the first
            // Host original, graph, ordinal, reservation or dispatch authority.
            return if old.query == request {
                Ok(NativeModelReserveOutcomeV2::ExactHistory)
            } else {
                Err(ProvisionApprovalError::Conflict)
            };
        }
        self.test_session_v2_local(id, SessionPurposeV1::Execution, now)?;
        // Request and query nonce belong to the exact already-paid admission;
        // only the new reservation identifier enters the namespace here.
        self.test_session_v2_transport_id_available(id, &query.reservation_id)?;

        let (plan, _) = s.header()?;
        let budget = plan.model_budget.as_ref().ok_or_else(invalid)?;
        // Unknown previous calls retain their maximum concurrency reservation. This
        // first slice never reconstructs an attempt or accepts caller usage totals.
        if !model.reservations.is_empty()
            || query.expected_floor.0 != model.calls_reserved
            || query.ordinal.0 != model.calls_reserved.checked_add(1).ok_or_else(invalid)?
            || model
                .reservations
                .values()
                .any(|r| r.query.query.query_nonce == query.query_nonce)
        {
            return Err(ProvisionApprovalError::Conflict);
        }
        let ports = query.physical_graph.ports(query.call_kind);
        let remaining_ports = ports.get(5..).ok_or_else(invalid)?;
        let units = u64::try_from(remaining_ports.len()).map_err(|_| invalid())?;
        let mut port_ids = Vec::with_capacity(remaining_ports.len());
        for index in 5..ports.len() {
            let key = super::native_model_ports_v2::port_id(
                &request,
                proof.ingress_attempt_id,
                &possession.transport_attempt_digest(),
                index,
            )?;
            self.test_session_v2_transport_id_available(id, &key)?;
            port_ids.push(key);
        }
        let accepted_ordinal = super::next_session_ordinal_v2(s)?;
        let tokens = budget
            .max_input_tokens
            .0
            .checked_add(budget.max_output_tokens.0)
            .ok_or_else(invalid)?;
        let money = budget.maximum_call_price().map_err(|_| invalid())?;
        let calls = model.calls_reserved.checked_add(1).ok_or_else(invalid)?;
        let total_tokens = model
            .tokens_reserved
            .checked_add(tokens)
            .ok_or_else(invalid)?;
        let total_money = model
            .micro_units_reserved
            .checked_add(money)
            .ok_or_else(invalid)?;
        if units == 0
            || calls > budget.max_calls.0
            || total_tokens > budget.max_total_tokens.0
            || total_money > budget.price_policy.max_total_micro_units.0
        {
            return Err(invalid());
        }
        let record = NativeModelReservationRecordV2 {
            query: request.clone(),
            host_ingress_attempt_id: proof.ingress_attempt_id.into(),
            host_signed_attempt_digest: possession.transport_attempt_digest(),
            credential_generation: credential.generation,
            claimed_at: now,
            physical_units: units,
            port_ids: port_ids.clone(),
            accepted_ordinal,
            maximum_tokens: tokens,
            maximum_micro_units: money,
            floor: calls,
        };
        let mut next = self.clone();
        let saved = next.test_session_v2_record_mut(id)?;
        saved
            .roots
            .get_mut(&SessionPurposeV1::Execution)
            .ok_or_else(invalid)?
            .budgets
            .transport
            .charge(units)
            .map_err(|_| invalid())?;
        let model = saved.native_model_budget.as_mut().ok_or_else(invalid)?;
        for (offset, key) in port_ids.into_iter().enumerate() {
            let index = offset.checked_add(5).ok_or_else(invalid)?;
            model.graph_ports.insert(
                key,
                NativeModelReservedPortV2 {
                    reservation_id: query.reservation_id.clone(),
                    graph_index: u16::try_from(index).map_err(|_| invalid())?,
                    class: ports[index],
                    reserved_at: now,
                },
            );
        }
        model.calls_reserved = calls;
        model.tokens_reserved = total_tokens;
        model.micro_units_reserved = total_money;
        model
            .reservations
            .insert(query.reservation_id.clone(), record);
        append_session_event_v2(
            saved,
            SessionEventKindV2::NativeModelGraphReserved {
                reservation_id: query.reservation_id.clone(),
            },
            now,
        )?;
        // No Current/floor/provider permission is returned. The later strong factory
        // must reference the original ordinal and its explicit paid physical graph.
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
        Ok(NativeModelReserveOutcomeV2::RecordedPending)
    }
}
