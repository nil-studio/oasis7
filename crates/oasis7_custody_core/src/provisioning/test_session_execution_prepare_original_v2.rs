//! Immutable exact public signed original: history only, never Current or send.
use super::*;
fn invalid() -> ProvisionApprovalError {
    ProvisionApprovalError::BindingMismatch
}
const MAX_ORIGINAL_BYTES: usize = 64 * 1024;
fn encode(value: &ExecutionPrepareSignedOriginalV2) -> Result<Vec<u8>> {
    let bytes = oasis7_wasm_abi::encode_canonical_cbor(value).map_err(|_| invalid())?;
    if bytes.is_empty() || bytes.len() > MAX_ORIGINAL_BYTES {
        return Err(invalid());
    }
    Ok(bytes)
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|v| format!("{v:02x}")).collect()
}
fn decode(text: &str) -> Result<ExecutionPrepareSignedOriginalV2> {
    if text.is_empty()
        || text.len() > MAX_ORIGINAL_BYTES * 2
        || text.len() % 2 != 0
        || !text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(invalid());
    }
    let bytes = text
        .as_bytes()
        .chunks_exact(2)
        .map(|b| {
            let digit = |v: u8| if v <= b'9' { v - b'0' } else { v - b'a' + 10 };
            (digit(b[0]) << 4) | digit(b[1])
        })
        .collect::<Vec<_>>();
    let value = serde_cbor::from_slice(&bytes).map_err(|_| invalid())?;
    if encode(&value)? != bytes {
        return Err(invalid());
    }
    Ok(value)
}
impl SessionExecutionBookV2 {
    pub(in crate::provisioning) fn prepare_original_recorded_at(&self) -> Option<UnixMillis> {
        self.stages
            .get(&MoveExecutionStageV2::Prepare)?
            .original
            .as_ref()
            .map(|o| o.recorded_at)
    }
    pub(super) fn validate_prepare_original(
        &self,
        s: &TestSessionAuthorityV2,
        bundle: &ExecutionSigningBundleV2,
    ) -> Result<()> {
        let stage = self
            .stages
            .get(&MoveExecutionStageV2::Prepare)
            .ok_or_else(invalid)?;
        let Some(original) = &stage.original else {
            return Ok(());
        };
        let permits = stage.service_record.as_ref().ok_or_else(invalid)?;
        if original.stage != MoveExecutionStageV2::Prepare
            || original.recorded_at.get() < permits.recorded_at.get()
            || super::super::test_session::session_event_ordinal_v2(
                s,
                &super::super::test_session::SessionEventKindV2::ExecutionPrepareOriginalArchived {
                    request_id: self.action_request_id.clone(),
                },
            )?
            .is_none()
        {
            return Err(invalid());
        }
        bundle.verify_prepare_original(&decode(&original.canonical_cbor_hex)?)
    }
}
impl AccountProvisionApprovalState {
    /// Exact replay is read-only. Even after deadline/Closing this can archive
    /// a genuinely signed original, but never construct a grant or dispatch.
    pub fn archive_test_session_v2_prepare_original(
        &mut self,
        id: &str,
        original: ExecutionPrepareSignedOriginalV2,
        now: UnixMillis,
    ) -> Result<()> {
        let s = self.test_session_v2_record(id)?;
        let b = s.execution_book.as_ref().ok_or_else(invalid)?;
        let stage = b
            .stages
            .get(&MoveExecutionStageV2::Prepare)
            .ok_or_else(invalid)?;
        let permits = stage.service_record.as_ref().ok_or_else(invalid)?;
        if now.get() < permits.recorded_at.get() {
            return Err(invalid());
        }
        let bundle = b.prepare_bundle(
            s,
            &stage.allocation,
            self.test_session_v2_world_read(&s.input)?,
        )?;
        bundle.verify_prepare_original(&original)?;
        let canonical_cbor_hex = hex(&encode(&original)?);
        if let Some(old) = &stage.original {
            return if old.canonical_cbor_hex == canonical_cbor_hex {
                Ok(())
            } else {
                Err(ProvisionApprovalError::Conflict)
            };
        }
        let request_id = b.action_request_id.clone();
        let mut next = self.clone();
        let s = next.test_session_v2_record_mut(id)?;
        s.execution_book
            .as_mut()
            .ok_or_else(invalid)?
            .stages
            .get_mut(&MoveExecutionStageV2::Prepare)
            .ok_or_else(invalid)?
            .original = Some(PublicStageOriginalV2 {
            recorded_at: now,
            stage: MoveExecutionStageV2::Prepare,
            canonical_cbor_hex,
        });
        super::super::test_session::append_session_event_v2(
            s,
            super::super::test_session::SessionEventKindV2::ExecutionPrepareOriginalArchived {
                request_id,
            },
            now,
        )?;
        next.validate_persistence()?;
        *self = next;
        Ok(())
    }
    pub fn test_session_v2_prepare_original_history(
        &self,
        id: &str,
    ) -> Result<Option<ExecutionPrepareSignedOriginalV2>> {
        let s = self.test_session_v2_record(id)?;
        let b = s.execution_book.as_ref().ok_or_else(invalid)?;
        let Some(stage) = b.stages.get(&MoveExecutionStageV2::Prepare) else {
            return Ok(None);
        };
        let Some(original) = &stage.original else {
            return Ok(None);
        };
        let bundle = b.prepare_bundle(
            s,
            &stage.allocation,
            self.test_session_v2_world_read(&s.input)?,
        )?;
        let value = decode(&original.canonical_cbor_hex)?;
        bundle.verify_prepare_original(&value)?;
        Ok(Some(value))
    }
}
