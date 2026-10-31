//! A point record scope inherited from an exact already-qualified family.
use super::*;
use oasis7_formal_protocol::incremental_execution::RootSetV1;
use oasis7_proto::memory_budget::ReservedValue;
/// Opens only the original outer reference. It inherits the record closure
/// from the exact family roots instead of rescanning the entire Case.
pub fn open_qualified_organization_record_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    owner: OrganizationRootKeyV1<'_>,
    roots: &RootSetV1,
    parent: &ReservedValue<OrganizationFamilyQualificationV1>,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<Option<ReservedValue<OrganizationRecordQualificationV1>>, ValueTreeError> {
    context.check()?;
    if !parent.uses_budget(context.values.memory) {
        return Err(ValueTreeError::Invalid("organization open family pool"));
    }
    parent.validate_binding(
        &roots
            .digest()
            .map_err(|_| ValueTreeError::Invalid("organization open roots"))?,
    )?;
    let auth = roots
        .state
        .iter()
        .find(|root| root.namespace == StateNamespaceV1::Authorization)
        .ok_or(ValueTreeError::Invalid("organization open namespace"))?;
    if context.root != auth {
        return Err(ValueTreeError::Invalid("organization open root binding"));
    }
    let Some(child) = inspect_organization_root_v1(owner, context)? else {
        context.check()?;
        return Ok(None);
    };
    let (key, _) = owner.outer().fixed_path(RecordPathV1::Record, context)?;
    let digest = WorldFieldKeyV1::from_bytes(key.as_bytes())
        .map_err(ValueTreeError::Invalid)?
        .semantic_key
        .ok_or(ValueTreeError::Invalid("organization open owner digest"))?;
    let proof = closure::from_family(
        owner,
        digest,
        &child,
        context.values.memory,
        context.values.class,
    )?;
    context.check()?;
    Ok(Some(proof))
}
