//! Visibility-aware commitments let light readers authenticate their projection
//! without downloading private snapshots or other Agents' views.
use serde::{Deserialize, Serialize};
use serde_json::Value;
pub const MAX_PROJECTION_SCOPES: usize = 16_384;
const MAX_PROJECTION_BYTES: usize = 4 * 1024 * 1024;
fn hash(domain: &str, value: &impl Serialize) -> Result<String, String> {
    let digest = crate::world_service::request_digest(domain, value)?;
    digest
        .strip_prefix("blake3:")
        .map(str::to_owned)
        .ok_or_else(|| "formal projection digest encoding".into())
}
fn hash_valid(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn scope_valid(s: &str) -> bool {
    s == "public"
        || s.strip_prefix("agent:")
            .is_some_and(|id| !id.trim().is_empty() && id.len() <= 256)
}
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectionAudienceV1 {
    pub scope_id: String,
    /// Owner and delegate views of one Agent have different visibility. Their
    /// leaves must bind the actual authenticated reader, not just Agent scope.
    pub reader_public_key: Option<String>,
}
impl ProjectionAudienceV1 {
    pub fn public() -> Self {
        Self {
            scope_id: "public".into(),
            reader_public_key: None,
        }
    }
    pub fn agent(agent: &str, reader_public_key: String) -> Result<Self, String> {
        let audience = Self {
            scope_id: format!("agent:{agent}"),
            reader_public_key: Some(reader_public_key),
        };
        audience.validate()?;
        Ok(audience)
    }
    fn validate(&self) -> Result<(), String> {
        if !scope_valid(&self.scope_id)
            || (self.scope_id == "public") != self.reader_public_key.is_none()
            || self
                .reader_public_key
                .as_ref()
                .is_some_and(|key| !hash_valid(key))
        {
            return Err("formal projection audience".into());
        }
        Ok(())
    }
}
/// Explicit recursive UTF-8 key order and JSON scalar encoding. This contract
/// is independent of serde_json's optional insertion-order map feature.
fn canonical_json(value: &Value, out: &mut Vec<u8>) -> Result<(), String> {
    match value {
        Value::Object(map) => {
            out.push(b'{');
            let mut keys = map.keys().collect::<Vec<_>>();
            keys.sort();
            for (index, key) in keys.iter().enumerate() {
                if index > 0 {
                    out.push(b',');
                }
                serde_json::to_writer(&mut *out, key).map_err(|e| e.to_string())?;
                out.push(b':');
                canonical_json(&map[*key], out)?;
            }
            out.push(b'}');
        }
        Value::Array(array) => {
            out.push(b'[');
            for (index, item) in array.iter().enumerate() {
                if index > 0 {
                    out.push(b',');
                }
                canonical_json(item, out)?;
            }
            out.push(b']');
        }
        scalar => serde_json::to_writer(&mut *out, scalar).map_err(|e| e.to_string())?,
    }
    if out.len() > MAX_PROJECTION_BYTES {
        return Err("formal projection byte bound".into());
    }
    Ok(())
}
pub fn projection_leaf(
    audience: &ProjectionAudienceV1,
    value: &impl Serialize,
) -> Result<String, String> {
    audience.validate()?;
    let value = serde_json::to_value(value).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    canonical_json(&value, &mut bytes)?;
    hash("formal-projection-leaf-v1", &(audience, bytes))
}
fn parent(left: &str, right: &str) -> Result<String, String> {
    hash("formal-projection-node-v1", &(left, right))
}
fn bound_root(count: usize, head: &str) -> Result<String, String> {
    hash("formal-projection-root-v1", &(count as u64, head))
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectionInclusionV1 {
    pub schema_version: u32,
    pub audience: ProjectionAudienceV1,
    pub leaf_index: u64,
    pub leaf_count: u64,
    pub siblings: Vec<String>,
}
impl ProjectionInclusionV1 {
    pub fn verify(
        &self,
        audience: &ProjectionAudienceV1,
        value: &impl Serialize,
        root: &str,
    ) -> Result<(), String> {
        audience.validate()?;
        if self.schema_version != 1
            || &self.audience != audience
            || self.leaf_count == 0
            || self.leaf_count > MAX_PROJECTION_SCOPES as u64
            || self.leaf_index >= self.leaf_count
            || !hash_valid(root)
            || self.siblings.len() > 14
        {
            return Err("formal projection proof bounds/scope".into());
        }
        let mut width = self.leaf_count;
        let mut index = self.leaf_index;
        let mut current = projection_leaf(audience, value)?;
        for sibling in &self.siblings {
            if width == 1 || !hash_valid(sibling) {
                return Err("formal projection proof shape".into());
            }
            if index.is_multiple_of(2) {
                if index + 1 == width && sibling != &current {
                    return Err("formal projection odd leaf padding".into());
                }
                current = parent(&current, sibling)?;
            } else {
                current = parent(sibling, &current)?;
            }
            index /= 2;
            width = width.div_ceil(2);
        }
        if width != 1 || bound_root(self.leaf_count as usize, &current)? != root {
            return Err("formal projection root mismatch".into());
        }
        Ok(())
    }
}
/// Holds only bounded scope IDs and digests. Callers compute one authorized
/// projection at a time rather than retaining all private projection bodies.
pub struct ProjectionCommitmentsV1 {
    scopes: Vec<ProjectionAudienceV1>,
    levels: Vec<Vec<String>>,
}
impl ProjectionCommitmentsV1 {
    pub fn from_leaves(mut leaves: Vec<(ProjectionAudienceV1, String)>) -> Result<Self, String> {
        if leaves.is_empty() || leaves.len() > MAX_PROJECTION_SCOPES {
            return Err("formal projection scope count".into());
        }
        leaves.sort_by(|a, b| a.0.cmp(&b.0));
        if !leaves.iter().any(|l| l.0 == ProjectionAudienceV1::public())
            || leaves
                .iter()
                .any(|l| l.0.validate().is_err() || !hash_valid(&l.1))
            || leaves.windows(2).any(|l| l[0].0 == l[1].0)
        {
            return Err("formal projection duplicate/invalid scope".into());
        }
        let (scopes, hashes): (Vec<_>, Vec<_>) = leaves.into_iter().unzip();
        let mut levels = vec![hashes];
        while levels.last().expect("leaf level").len() > 1 {
            let next = levels
                .last()
                .expect("level")
                .chunks(2)
                .map(|pair| parent(&pair[0], pair.get(1).unwrap_or(&pair[0])))
                .collect::<Result<Vec<_>, _>>()?;
            levels.push(next);
        }
        Ok(Self { scopes, levels })
    }
    pub fn root(&self) -> Result<String, String> {
        bound_root(self.scopes.len(), &self.levels.last().expect("root")[0])
    }
    pub fn inclusion(
        &self,
        audience: &ProjectionAudienceV1,
    ) -> Result<ProjectionInclusionV1, String> {
        let index = self
            .scopes
            .binary_search(audience)
            .map_err(|_| "formal projection audience absent")?;
        let mut at = index;
        let mut siblings = Vec::new();
        for level in self.levels.iter().take(self.levels.len() - 1) {
            siblings.push(level.get(at ^ 1).unwrap_or(&level[at]).clone());
            at /= 2;
        }
        Ok(ProjectionInclusionV1 {
            schema_version: 1,
            audience: audience.clone(),
            leaf_index: index as u64,
            leaf_count: self.scopes.len() as u64,
            siblings,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn public_and_owner_proofs_do_not_need_other_private_views() {
        let views = [
            (
                ProjectionAudienceV1::public(),
                serde_json::json!({"tick":7}),
            ),
            (
                ProjectionAudienceV1::agent("a", hex::encode([1u8; 32])).unwrap(),
                serde_json::json!({"tick":7,"private":"memory-a"}),
            ),
            (
                ProjectionAudienceV1::agent("b", hex::encode([2u8; 32])).unwrap(),
                serde_json::json!({"tick":7,"private":"memory-b"}),
            ),
        ];
        let tree = ProjectionCommitmentsV1::from_leaves(
            views
                .iter()
                .map(|(scope, v)| (scope.clone(), projection_leaf(scope, v).unwrap()))
                .collect(),
        )
        .unwrap();
        for (scope, value) in &views {
            let proof = tree.inclusion(scope).unwrap();
            proof.verify(scope, value, &tree.root().unwrap()).unwrap();
            assert!(!serde_json::to_string(&proof).unwrap().contains("memory"));
            assert!(
                proof
                    .verify(scope, &serde_json::json!({"tick":8}), &tree.root().unwrap())
                    .is_err()
            );
            assert!(
                proof
                    .verify(
                        &ProjectionAudienceV1::agent("other", hex::encode([3u8; 32])).unwrap(),
                        value,
                        &tree.root().unwrap()
                    )
                    .is_err()
            );
            let mut changed = proof;
            changed.leaf_count += 1;
            assert!(changed.verify(scope, value, &tree.root().unwrap()).is_err());
        }
    }
    #[test]
    fn key_order_is_canonical_and_single_leaf_root_is_bound_to_count() {
        let a: Value = serde_json::from_str(r#"{"b":{"z":2,"a":1},"a":[3,4]}"#).unwrap();
        let b: Value = serde_json::from_str(r#"{"a":[3,4],"b":{"a":1,"z":2}}"#).unwrap();
        let audience = ProjectionAudienceV1::public();
        let leaf = projection_leaf(&audience, &a).unwrap();
        assert_eq!(leaf, projection_leaf(&audience, &b).unwrap());
        let tree =
            ProjectionCommitmentsV1::from_leaves(vec![(audience.clone(), leaf.clone())]).unwrap();
        let proof = tree.inclusion(&audience).unwrap();
        assert!(proof.siblings.is_empty());
        proof.verify(&audience, &a, &tree.root().unwrap()).unwrap();
        assert!(proof.verify(&audience, &a, &leaf).is_err());
        assert!(
            ProjectionCommitmentsV1::from_leaves(vec![
                (audience.clone(), leaf.clone()),
                (audience, leaf)
            ])
            .is_err()
        );
    }
    #[test]
    fn owner_proof_cannot_be_used_for_delegate_of_same_agent() {
        let owner = ProjectionAudienceV1::agent("a", hex::encode([1u8; 32])).unwrap();
        let delegate = ProjectionAudienceV1::agent("a", hex::encode([2u8; 32])).unwrap();
        let owner_view = serde_json::json!({"owner_chat":"private"});
        let delegated_view = serde_json::json!({"tick":7});
        let public = ProjectionAudienceV1::public();
        let tree = ProjectionCommitmentsV1::from_leaves(vec![
            (
                public.clone(),
                projection_leaf(&public, &serde_json::json!({})).unwrap(),
            ),
            (owner.clone(), projection_leaf(&owner, &owner_view).unwrap()),
            (
                delegate.clone(),
                projection_leaf(&delegate, &delegated_view).unwrap(),
            ),
        ])
        .unwrap();
        let proof = tree.inclusion(&owner).unwrap();
        proof
            .verify(&owner, &owner_view, &tree.root().unwrap())
            .unwrap();
        assert!(
            proof
                .verify(&delegate, &owner_view, &tree.root().unwrap())
                .is_err()
        );
        assert!(
            tree.inclusion(&delegate)
                .unwrap()
                .verify(&delegate, &owner_view, &tree.root().unwrap())
                .is_err()
        );
    }
}

#[path = "formal_projection_terminal_v2.rs"]
mod terminal_v2;
pub use terminal_v2::ClosedTaskFormalProjectionV2;
