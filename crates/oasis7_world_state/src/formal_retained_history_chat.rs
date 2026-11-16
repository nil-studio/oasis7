//! Fixed original chat-scope equality and matching for retained Current results.
use super::*;

pub fn chat_result_matches(
    result: &CanonicalIntentResultV1,
    lookup: &AgentChatIntentLookupV1,
) -> Result<bool, RetainedHistoryReadError> {
    let Some(parts) =
        retained_chat_scope_data_with_work_v1(result, &lookup.intent_id, &mut |_, _| {
            Ok::<(), std::convert::Infallible>(())
        })
        .unwrap()
    else {
        return Ok(false);
    };
    let digest = digest_text("owner-public-key", parts.public_key())
        .map_err(RetainedHistoryReadError::Invalid)?;
    Ok(parts
        .matches_with_digest_data_v1(lookup, &digest, &mut |_, _| {
            Ok::<(), std::convert::Infallible>(())
        })
        .unwrap())
}
/// Borrowed original scope operands only. This does not authenticate a result,
/// select a current owner, or grant archive/audience permission.
pub struct RetainedChatScopeDataV1<'a> {
    request: &'a oasis7_proto::viewer::AgentChatRequest,
    auth: &'a oasis7_proto::viewer::PlayerAuthProof,
    authority: &'a oasis7_proto::viewer::CanonicalAgentChatAuthorityV1,
}
fn chat_equal<E>(
    a: &str,
    b: &str,
    work: &mut impl FnMut(u64, u64) -> Result<(), E>,
) -> Result<bool, E> {
    work(1, 0)?;
    if a.len() != b.len() {
        return Ok(false);
    }
    for (a, b) in a.as_bytes().chunks(8192).zip(b.as_bytes().chunks(8192)) {
        work(1, (a.len() + b.len()) as u64)?;
        if a != b {
            return Ok(false);
        }
    }
    Ok(true)
}
pub fn retained_chat_scope_data_with_work_v1<'a, E>(
    result: &'a CanonicalIntentResultV1,
    intent: &str,
    work: &mut impl FnMut(u64, u64) -> Result<(), E>,
) -> Result<Option<RetainedChatScopeDataV1<'a>>, E> {
    work(1, 0)?;
    if result.rejected.is_some() {
        return Ok(None);
    }
    let count = result.receipt.as_object().map_or(0, |v| v.len());
    let levels = (count + 1).ilog2() as u64 + 2;
    work(levels * 4, levels * 32)?;
    let Some(id) = result
        .receipt
        .get("intent_id")
        .and_then(serde_json::Value::as_str)
    else {
        return Ok(None);
    };
    if !chat_equal(id, intent, work)? {
        return Ok(None);
    }
    let crate::service_admission::WorldServicePayloadV1::AgentChat(request) =
        &result.request.signed_payload
    else {
        return Ok(None);
    };
    let Some(auth) = request.auth.as_ref() else {
        return Ok(None);
    };
    let Some(authority) = request.canonical_authority.as_ref() else {
        return Ok(None);
    };
    Ok(Some(RetainedChatScopeDataV1 {
        request,
        auth,
        authority,
    }))
}
impl RetainedChatScopeDataV1<'_> {
    pub fn public_key(&self) -> &str {
        &self.auth.public_key
    }
    /// Original ordered field predicate. The fixed reader computes the digest
    /// with the original ABI tuple/BLAKE3 kernel, never from a caller verdict.
    pub fn matches_with_digest_data_v1<E>(
        &self,
        lookup: &AgentChatIntentLookupV1,
        digest: &str,
        work: &mut impl FnMut(u64, u64) -> Result<(), E>,
    ) -> Result<bool, E> {
        if !chat_equal(&self.request.agent_id, &lookup.agent_id, work)?
            || !chat_equal(digest, &lookup.owner_public_key_digest, work)?
            || !chat_equal(&self.auth.player_id, &lookup.player_id, work)?
        {
            return Ok(false);
        }
        let Some(world) = self.request.world_id.as_deref() else {
            return Ok(false);
        };
        if !chat_equal(world, &lookup.world_id, work)? {
            return Ok(false);
        }
        work(1, 0)?;
        if self.request.reorg_epoch != Some(lookup.reorg_epoch) {
            return Ok(false);
        }
        let Some(scope) = self.request.authority_scope.as_deref() else {
            return Ok(false);
        };
        if !chat_equal(scope, &lookup.authority_scope, work)?
            || !chat_equal(&self.authority.branch_id, &lookup.branch_id, work)?
        {
            return Ok(false);
        }
        work(1, 0)?;
        Ok(self.authority.agent_identity_generation == lookup.identity_generation)
    }
}
