//! One original business update kernel. Source adapters are private: Current
//! must be minted from the actual Runtime delta owner, never a caller map/root.
use super::*;
pub(super) trait CanonicalOps {
    fn canonical<T: Serialize>(&mut self, value: &T) -> Result<Vec<u8>, String> {
        oasis7_wasm_abi::encode_canonical_cbor(value).map_err(|e| e.to_string())
    }
    fn hash_work(&mut self, _bytes: usize) -> Result<(), String> {
        Ok(())
    }
}
pub(super) struct LegacyCanonical;
impl CanonicalOps for LegacyCanonical {}
impl CanonicalOps for LegacyFacts<'_> {}
pub(super) trait Facts: CanonicalOps {
    fn has_world_results(&self) -> bool;
    fn key_digest(&mut self, result: &CanonicalIntentResultV1) -> Result<String, String> {
        correlation::key_digest(&result.request.correlation.key)
    }
    fn check_result(&mut self, key: &str, result: &CanonicalIntentResultV1) -> Result<(), String>;
    fn scope(
        &mut self,
        request: &SubmitIntentRequest<WorldServicePayloadV1>,
    ) -> Result<String, String>;
}
pub(super) struct LegacyFacts<'a>(pub(super) &'a World);
impl Facts for LegacyFacts<'_> {
    fn has_world_results(&self) -> bool {
        !self
            .0
            .capability_revocation_state()
            .world_service_results
            .is_empty()
    }
    fn check_result(&mut self, key: &str, result: &CanonicalIntentResultV1) -> Result<(), String> {
        let world = self.0;
        let ledger_result: CanonicalIntentResultV1 = if let Some(stored_value) = world
            .capability_revocation_state()
            .world_service_results
            .get(key)
        {
            correlation::validate_result(key, stored_value)?;
            serde_json::from_value(stored_value.clone()).map_err(|error| error.to_string())?
        } else if world.state().retained_history_root.is_some() {
            world
                .read_retained_world_service_result(key)
                .map_err(|error| error.to_string())?
                .ok_or("formal result candidate is absent from authenticated World history")?
        } else {
            return Err("formal result candidate is absent from the World ledger".into());
        };
        if &ledger_result != result {
            return Err("formal result candidate differs from the World ledger".into());
        }
        Ok(())
    }
    fn scope(
        &mut self,
        request: &SubmitIntentRequest<WorldServicePayloadV1>,
    ) -> Result<String, String> {
        scope::for_request(self.0, request)
    }
}
pub(super) trait Index {
    type Delta;
    fn empty_root(&mut self) -> Result<BusinessIndexRootV1, String> {
        BusinessIndexRootV1::empty()
    }
    fn validate_tree(&mut self, tree: &FormalBusinessResultTreeV1) -> Result<(), String> {
        tree.validate()
    }
    fn get(
        &mut self,
        base: &BusinessIndexRootV1,
        key: &str,
    ) -> Result<Option<BusinessIndexLeafV1>, String>;
    fn build(
        &mut self,
        base: BusinessIndexRootV1,
        updates: Vec<BusinessIndexLeafV1>,
    ) -> Result<Self::Delta, String>;
    fn new_root(delta: &Self::Delta) -> &BusinessIndexRootV1;
}
struct LegacyIndex<'a, R>(&'a R);
impl<R: BusinessIndexNodeReader> Index for LegacyIndex<'_, R> {
    type Delta = BusinessIndexDeltaV1;
    fn get(
        &mut self,
        base: &BusinessIndexRootV1,
        key: &str,
    ) -> Result<Option<BusinessIndexLeafV1>, String> {
        base.get(key, self.0)
    }
    fn build(
        &mut self,
        base: BusinessIndexRootV1,
        updates: Vec<BusinessIndexLeafV1>,
    ) -> Result<Self::Delta, String> {
        BusinessIndexDeltaV1::build_v2(base, updates, self.0)
    }
    fn new_root(delta: &Self::Delta) -> &BusinessIndexRootV1 {
        &delta.new
    }
}
pub(super) fn extend(
    facts: &mut impl Facts,
    metadata: FormalBusinessCommitMetadataV1,
    previous: Option<&FormalBusinessResultTreeV1>,
    new_results: &[CanonicalIntentResultV1],
    reader: &impl BusinessIndexNodeReader,
    updates: Vec<BusinessIndexLeafV1>,
) -> Result<FormalBusinessResultTreeUpdateV1, String> {
    let (tree, delta) = extend_with_index(
        facts,
        metadata,
        previous,
        new_results.iter(),
        &mut LegacyIndex(reader),
        updates,
    )?;
    Ok(FormalBusinessResultTreeUpdateV1 { tree, delta })
}
pub(super) fn extend_with_index<'r, I: Index>(
    facts: &mut impl Facts,
    metadata: FormalBusinessCommitMetadataV1,
    previous: Option<&FormalBusinessResultTreeV1>,
    new_results: impl ExactSizeIterator<Item = &'r CanonicalIntentResultV1>,
    index: &mut I,
    mut updates: Vec<BusinessIndexLeafV1>,
) -> Result<(FormalBusinessResultTreeV1, I::Delta), String> {
    if let Some(previous) = previous {
        index.validate_tree(previous)?;
        if previous.metadata.world_id != metadata.world_id
            || previous.metadata.chain_id != metadata.chain_id
            || previous.metadata.genesis_digest != metadata.genesis_digest
            || previous.metadata.formal_position >= metadata.formal_position
        {
            return Err("formal result tree predecessor identity/position mismatch".into());
        }
    } else if facts.has_world_results() {
        return Err(
            "fresh formal activation cannot upgrade unindexed legacy intent results".into(),
        );
    }

    let base = match previous {
        Some(tree) => tree.index_root.clone(),
        None => index.empty_root()?,
    };
    if let Some(previous) = previous {
        let old_metadata = index
            .get(&base, &metadata_key_digest())?
            .ok_or("formal result predecessor metadata leaf missing")?;
        if old_metadata.kind != BusinessIndexLeafKindV1::CommitMetadata
            || decode_index_leaf(&old_metadata)?
                != TreeLeafV1::CommitMetadata(previous.metadata.clone())
        {
            return Err("formal result predecessor metadata leaf mismatch".into());
        }
    }
    updates.reserve(new_results.len() + 1);
    updates.push(index_leaf_with(
        facts,
        metadata_key_digest(),
        BusinessIndexLeafKindV1::CommitMetadata,
        &TreeLeafV1::CommitMetadata(metadata.clone()),
    )?);
    for result in new_results {
        result
            .request
            .validate()
            .map_err(|error| error.to_string())?;
        let key = facts.key_digest(result)?;
        facts.check_result(&key, result)?;
        let index_key = facts
            .key_digest(result)?
            .strip_prefix("blake3:")
            .map(str::to_owned)
            .ok_or_else(|| "formal result key digest encoding".to_owned())?;
        let prior = index
            .get(&base, &index_key)?
            .map(|leaf| decode_index_leaf(&leaf))
            .transpose()?;
        let first_position = match prior {
            Some(TreeLeafV1::Outcome(old_leaf)) => {
                let candidate =
                    outcome_leaf_with_facts(facts, result, old_leaf.first_formal_position)?;
                if immutable_outcome_with(facts, &old_leaf)?
                    != immutable_outcome_with(facts, &candidate)?
                {
                    return Err("formal result ledger rewrote an immutable outcome".into());
                }
                old_leaf.first_formal_position
            }
            Some(TreeLeafV1::CommitMetadata(_)) => {
                return Err("formal result key collided with metadata key".into());
            }
            None => metadata.formal_position,
        };
        let outcome = outcome_leaf_with_facts(facts, result, first_position)?;
        updates.push(index_leaf_with(
            facts,
            index_key,
            BusinessIndexLeafKindV1::Outcome,
            &TreeLeafV1::Outcome(outcome),
        )?);
    }
    updates.sort_by(|left, right| left.key_digest.cmp(&right.key_digest));
    let delta = index.build(base, updates)?;
    let tree = FormalBusinessResultTreeV1 {
        schema_version: FORMAL_BUSINESS_RESULT_SCHEMA_V1,
        metadata,
        index_root: I::new_root(&delta).clone(),
    };
    index.validate_tree(&tree)?;
    Ok((tree, delta))
}
