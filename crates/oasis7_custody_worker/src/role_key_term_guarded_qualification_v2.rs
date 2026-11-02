//! Fact-only reply on the fixed private CF namespace. Not a portable permit,
//! global Current, signing handle, or proof against whole-DO rollback.
use super::*;
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TermOwnerSlotQualificationRequestV2 {
    pub api_version: u8,
    pub paid_attempt_id: String,
    pub owner_scope: RoleKeySlotScopeV1,
    pub owner_key: PublicRoleKey,
    pub issued_at: DecimalU64,
    pub until: DecimalU64,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TermOwnerSlotQualificationReplyV2 {
    pub api_version: u8,
    pub paid_attempt_id: String,
    pub request_digest: String,
    pub owner_scope: RoleKeySlotScopeV1,
    pub owner_key: PublicRoleKey,
    pub installed_schema: u8,
    pub pin_floor_digest: String,
    pub observed_at: DecimalU64,
    pub until: DecimalU64,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Claim {
    pub(super) id: String,
    pub(super) request: TermOwnerSlotQualificationRequestV2,
    pub(super) claimed_at: DecimalU64,
    reply: TermOwnerSlotQualificationReplyV2,
}
fn request_digest(q: &TermOwnerSlotQualificationRequestV2) -> Result<String> {
    oasis7_wasm_abi::canonical_hash(&("oasis7.custody.term-owner-slot-qualification-request.v2", q))
        .map_err(|_| rejected())
}
fn identifier(q: &TermOwnerSlotQualificationRequestV2) -> Result<String> {
    oasis7_wasm_abi::canonical_hash(&(
        "oasis7.custody.term-owner-slot-qualification-once.v2",
        &q.owner_scope,
        &q.paid_attempt_id,
    ))
    .map_err(|_| rejected())
}
fn reply(
    q: &TermOwnerSlotQualificationRequestV2,
    slot: &RoleKeySlotV1,
    config: &TermGuardedSignerConfigurationV2,
    at: u64,
) -> Result<TermOwnerSlotQualificationReplyV2> {
    let (close_pin, close_floor) = config.close.as_ref().ok_or_else(rejected)?;
    if q.api_version != 2
        || q.paid_attempt_id.is_empty()
        || q.paid_attempt_id.len() > 256
        || q.paid_attempt_id.chars().any(char::is_control)
        || q.owner_scope != slot.scope
        || q.owner_key != slot.public_ref
        || q.issued_at.0 == 0
        || q.issued_at.0 > at
        || at >= q.until.0
        || q.until.0 > q.issued_at.0.checked_add(5000).ok_or_else(rejected)?
    {
        return Err(rejected());
    }
    Ok(TermOwnerSlotQualificationReplyV2 {
        api_version: 2,
        paid_attempt_id: q.paid_attempt_id.clone(),
        request_digest: request_digest(q)?,
        owner_scope: slot.scope.clone(),
        owner_key: slot.public_ref.clone(),
        installed_schema: 2,
        pin_floor_digest: oasis7_wasm_abi::canonical_hash(&(
            "oasis7.custody.term-owner-slot-dual-installation.v2",
            &config.pin,
            &config.floor,
            close_pin,
            close_floor,
        ))
        .map_err(|_| rejected())?,
        observed_at: DecimalU64(at),
        until: q.until,
    })
}
pub(super) fn check_claim(
    c: &Claim,
    slot: &RoleKeySlotV1,
    config: &TermGuardedSignerConfigurationV2,
) -> Result<()> {
    if c.id != identifier(&c.request)?
        || c.reply != reply(&c.request, slot, config, c.claimed_at.0)?
    {
        return Err(rejected());
    }
    Ok(())
}
#[wasm_bindgen(module = "/platform/term_guarded_signer_sql_v2.mjs")]
extern "C" {
    #[wasm_bindgen(catch,js_name=claimTermGuardedQualification)]
    fn claim_qualification(state: &JsValue, transition: &js_sys::Function) -> Result<String>;
}
impl RoleKeyStore {
    pub(crate) fn qualify_term_owner_slot_v2(
        &self,
        q: TermOwnerSlotQualificationRequestV2,
        config: &TermGuardedSignerConfigurationV2,
    ) -> Result<TermOwnerSlotQualificationReplyV2> {
        let mut transition = |raw: String| -> Result<String> {
            let checked = check_inventory(&raw, config)?;
            let at = clock()?;
            if at < checked.clock_floor
                || checked.qualifications.len() >= MAX_ROWS
                || checked
                    .claims
                    .iter()
                    .any(|c| c.permit.payload.paid_attempt_id == q.paid_attempt_id)
                || checked
                    .close_claims
                    .iter()
                    .any(|c| c.permit.payload.paid_attempt_id == q.paid_attempt_id)
                || checked
                    .qualifications
                    .iter()
                    .any(|c| c.request.paid_attempt_id == q.paid_attempt_id)
            {
                return Err(rejected());
            }
            json(&Claim {
                id: identifier(&q)?,
                request: q.clone(),
                claimed_at: DecimalU64(at),
                reply: reply(&q, &checked.slot, config, at)?,
            })
        };
        let function = Closure::borrow_mut(&mut transition);
        let saved = claim_qualification(&self.state, function.as_ref().unchecked_ref())?;
        drop(function);
        let c: Claim = strict(&saved)?;
        if c.request != q {
            return Err(rejected());
        }
        Ok(c.reply)
    }
}
