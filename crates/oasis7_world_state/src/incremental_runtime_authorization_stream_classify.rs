use super::*;
pub(crate) fn classify<R: ValueNodeReader, C: FnMut() -> bool>(
    kind: AuthorizationBodyKindV1,
    entry: &StateEntryV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<ChangedColumn, ValueTreeError> {
    use ChangedColumn as K;
    use MemberPayloadCodecV1 as V;
    let key = WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
    let canonical = WorldFieldKeyV1::new(kind.descriptor(), key.kind, key.semantic_key)
        .map_err(ValueTreeError::Invalid)?;
    if entry.namespace != StateNamespaceV1::Authorization
        || key.kind != WorldFieldRecordKindV1::Entry
        || canonical.as_bytes() != entry.key
    {
        return Err(invalid());
    }
    macro_rules! names {
        ($names:expr,$make:expr) => {
            for name in $names {
                if records
                    .key(kind.descriptor(), WorldFieldRecordKindV1::Entry, &name)?
                    .0
                    .as_bytes()
                    == entry.key
                {
                    return Ok(($make)(name));
                }
            }
        };
    }
    macro_rules! options {
        ($names:expr,$codec:expr) => {
            for name in $names {
                for tag in ["present", "value"] {
                    if records
                        .key(
                            kind.descriptor(),
                            WorldFieldRecordKindV1::Entry,
                            &(name, tag),
                        )?
                        .0
                        .as_bytes()
                        == entry.key
                    {
                        return Ok(K::Optional(name, $codec));
                    }
                }
            }
        };
    }
    match kind {
        AuthorizationBodyKindV1::Audit => {
            names!(
                [
                    "receipt_id",
                    "scope_hash",
                    "decision",
                    "branch_id",
                    "finality_status",
                    "state_hash_before",
                    "canonical_request_hash",
                    "canonical_result_hash"
                ],
                |name| K::Required(name, V::Text)
            );
            names!(["world_head_before", "finality_epoch"], |name| K::Required(
                name,
                V::U64
            ));
            names!(["budget_before"], |name| K::Required(name, V::I64));
            options!(
                [
                    "root_receipt_id",
                    "grant_id",
                    "module_id",
                    "module_version",
                    "manifest_hash",
                    "catalog_snapshot_id",
                    "response_nonce",
                    "authorization_nonce_key_hash",
                    "denial_code",
                    "finality_block_hash",
                    "state_hash_after",
                    "committed_effect_receipt_id"
                ],
                V::Text
            );
            options!(["budget_after"], V::I64);
            options!(["world_head_after"], V::U64);
            names!(["subject", "audience"], K::Json);
            if records
                .key(
                    kind.descriptor(),
                    WorldFieldRecordKindV1::Entry,
                    &"presenter",
                )?
                .0
                .as_bytes()
                == entry.key
                || records
                    .key(
                        kind.descriptor(),
                        WorldFieldRecordKindV1::Entry,
                        &("presenter", "present"),
                    )?
                    .0
                    .as_bytes()
                    == entry.key
            {
                return Ok(K::Presenter);
            }
            if records
                .key(
                    kind.descriptor(),
                    WorldFieldRecordKindV1::Entry,
                    &("committed_effect_receipt_ids", "count"),
                )?
                .0
                .as_bytes()
                == entry.key
            {
                return Ok(K::Effects);
            }
            Ok(K::EffectMember)
        }
        AuthorizationBodyKindV1::Invocation => {
            names!(
                [
                    "grant_id",
                    "presenter.presenter_id",
                    "presenter.presenter_kind",
                    "audience.world_id",
                    "audience.branch_id",
                    "audience.target_kind",
                    "catalog_snapshot_id",
                    "module_id",
                    "module_version",
                    "response_nonce"
                ],
                |name| K::Required(name, V::Text)
            );
            names!(["audience.finality_epoch"], |name| K::Required(
                name,
                V::U64
            ));
            options!(
                [
                    "presenter.session_id",
                    "presenter.attestation_ref",
                    "audience.target_id"
                ],
                V::Text
            );
            names!(
                [
                    "subject.kind",
                    "subject.agent_id",
                    "subject.owner_binding",
                    "subject.generation",
                    "subject.module_id",
                    "subject.module_version",
                    "subject.instance_id",
                    "subject.system_id",
                    "subject.epoch"
                ],
                |_| K::Subject
            );
            Err(invalid())
        }
    }
}
