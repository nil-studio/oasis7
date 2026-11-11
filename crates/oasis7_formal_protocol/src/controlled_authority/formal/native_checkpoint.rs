//! Signed native checkpoint provenance, independent of filesystem/provider actors.
//! A checked checkpoint authenticates archived parent context, never establishes
//! an ordinary reader's external pin or claims provider freshness by itself.
use super::super::{crypto, fencing};
use super::*;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
const LIMIT: usize = 64 * 1024 * 1024;
const DOMAIN: &str = "oasis7.formal.endpoint_checkpoint.sign.v1";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeHotBodyV1 {
    pub version: u32,
    pub instance_digest: String,
    pub authority_id: String,
    pub activation_ref: String,
    pub epoch: u64,
    pub revision: u64,
    pub head: FormalHeadRefV1,
    pub head_node: Option<String>,
    pub imported_head_node: Option<String>,
    pub import_digest: Option<String>,
    pub base_parent: Option<NativeCheckpointParentV1>,
    pub index: Option<String>,
    pub pending: Option<String>,
    pub decision_head: Option<FormalHeadRefV1>,
    pub seal: Option<fencing::EpochSealBodyV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub durable_set: Option<fencing::DurableSetNativeBindingV1>,
    /// Local-only complete-data commitment. No execution or consensus grant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_closure: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeCheckpointParentV1 {
    pub record: CanonicalRecordV1,
    pub finalizing_authority_id: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NativeHistoryNodeV1 {
    pub head: FormalHeadRefV1,
    pub request: String,
    pub entry: String,
    /// Link 0 is the immediate predecessor; link k is 2^k positions behind.
    pub ancestors: Vec<NativeHistoryLinkV1>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NativeHistoryLinkV1 {
    pub head: FormalHeadRefV1,
    pub node: String,
}

#[derive(Deserialize)]
struct SignedHot {
    body: NativeHotBodyV1,
    signature: String,
}
#[derive(Deserialize)]
enum ArchivedEntry {
    Qualified {
        proof: FormalQualifiedCommitProofV1,
        #[serde(default)]
        data_closure: Option<String>,
    },
}
/// Private checked provenance; callers cannot promote a raw parent record.
pub struct NativeCheckpointVerdictV1 {
    proof: FormalQualifiedCommitProofV1,
    parent: Option<NativeCheckpointParentV1>,
    head: FormalHeadRefV1,
    authority_id: String,
    minimum: FormalHeadRefV1,
    current_head: FormalHeadRefV1,
    data_closure: Option<String>,
}
fn object<T: DeserializeOwned>(
    id: &str,
    load: &mut impl FnMut(&str) -> Result<Vec<u8>, FormalError>,
) -> Result<T, FormalError> {
    crypto::hash_hex(id)?;
    let bytes = load(id)?;
    if bytes.len() > LIMIT || blake3::hash(&bytes).to_hex().as_str() != id {
        return Err(invalid("native provenance referenced CAS checksum/bound"));
    }
    serde_cbor::from_slice(&bytes).map_err(|e| invalid(e.to_string()))
}
fn descend(
    body: &NativeHotBodyV1,
    target: &FormalHeadRefV1,
    load: &mut impl FnMut(&str) -> Result<Vec<u8>, FormalError>,
) -> Result<NativeHistoryNodeV1, FormalError> {
    crypto::hash_hex(&target.record_id)?;
    if target.position == 0 || target.position > body.head.position {
        return Err(invalid("native provenance target position"));
    }
    let mut id = body
        .head_node
        .as_ref()
        .or(body.imported_head_node.as_ref())
        .ok_or_else(|| invalid("native provenance history root"))?
        .clone();
    let mut expected = body.head.clone();
    for _ in 0..=64 {
        let node: NativeHistoryNodeV1 = object(&id, load)?;
        if node.head != expected || node.ancestors.len() > 64 {
            return Err(invalid("native provenance node/head bound"));
        }
        if node.head.position == target.position {
            if node.head != *target {
                return Err(invalid("native provenance exact target fork"));
            }
            return Ok(node);
        }
        let distance = node
            .head
            .position
            .checked_sub(target.position)
            .ok_or_else(|| invalid("native provenance ancestry position"))?;
        let level = ((63 - distance.leading_zeros()) as usize).min(
            node.ancestors
                .len()
                .checked_sub(1)
                .ok_or_else(|| invalid("native provenance missing ancestor"))?,
        );
        let link = &node.ancestors[level];
        let step = 1u64
            .checked_shl(level as u32)
            .ok_or_else(|| invalid("native provenance skip overflow"))?;
        if node.head.position.checked_sub(step) != Some(link.head.position)
            || link.head.position < target.position
        {
            return Err(invalid("native provenance skip distance"));
        }
        id = link.node.clone();
        expected = link.head.clone();
    }
    Err(invalid("native provenance ancestry bound"))
}
impl NativeCheckpointVerdictV1 {
    /// Caller supplies a separately authenticated checkpoint/floor. This function
    /// verifies its exact signed byte commitment and the bounded CAS ancestry;
    /// it does not replace the native provider's monotonic/freshness checks.
    #[expect(
        clippy::too_many_arguments,
        reason = "native provenance binds checkpoint authority, endpoint, protected commitment, target and minimum"
    )]
    pub fn verify(
        signed_checkpoint: &[u8],
        checkpoint_authority: &VerifiedAuthorityV1,
        endpoint_public_key: &str,
        expected_checkpoint: &fencing::EndpointCheckpointBindingV1,
        provider: &fencing::MonotonicProviderEvidenceV1,
        target: &FormalHeadRefV1,
        minimum: &FormalHeadRefV1,
        authority_id: &str,
        mut load: impl FnMut(&str) -> Result<Vec<u8>, FormalError>,
    ) -> Result<Self, FormalError> {
        if signed_checkpoint.len() > LIMIT {
            return Err(invalid("native provenance checkpoint byte bound"));
        }
        let hot: SignedHot =
            serde_cbor::from_slice(signed_checkpoint).map_err(|e| invalid(e.to_string()))?;
        crypto::verify(DOMAIN, &hot.body, &hot.signature, endpoint_public_key)?;
        let body = &hot.body;
        let instance = [
            &checkpoint_authority.definition().primary.instance,
            &checkpoint_authority.definition().replica.instance,
        ]
        .into_iter()
        .find(|instance| instance.endpoint_public_key == endpoint_public_key)
        .ok_or_else(|| invalid("native provenance authorized endpoint key"))?;
        if body.version != 1
            || body.instance_digest != instance.digest().map_err(|e| invalid(e.to_string()))?
            || body.authority_id != checkpoint_authority.authority_id()?
            || body.activation_ref != checkpoint_authority.activation_ref()
            || body.epoch != checkpoint_authority.definition().epoch
            || expected_checkpoint.commitment != blake3::hash(signed_checkpoint).to_hex().as_str()
            || expected_checkpoint.revision != body.revision
        {
            return Err(invalid(
                "native provenance signed checkpoint authority/instance/commitment",
            ));
        }
        fencing::verify_monotonic_checkpoint_evidence(
            provider,
            instance,
            expected_checkpoint,
            &fencing::TrustedFencingPolicyV1 {
                provider_public_key: checkpoint_authority
                    .policy()
                    .fencing_provider_public_key
                    .clone(),
                authorized_resource_bindings: checkpoint_authority
                    .policy()
                    .authorized_fencing_resource_bindings
                    .clone(),
            },
        )
        .map_err(|error| {
            invalid(format!(
                "native provenance trusted monotonic provider: {error}"
            ))
        })?;
        let selected = body.decision_head.as_ref().unwrap_or(&body.head);
        if expected_checkpoint.head.position != selected.position
            || expected_checkpoint.head.decision_hash != selected.record_id
            || expected_checkpoint.head.qualified != body.decision_head.is_none()
        {
            return Err(invalid("native provenance protected checkpoint head"));
        }
        if let Some(binding) = &body.durable_set {
            binding.validate().map_err(|e| invalid(e.to_string()))?;
        }
        // Both roots are authenticated beneath the same signed history. A raw
        // caller-selected minimum never grants a new ancestry bridge.
        if minimum.position != 0 {
            descend(body, minimum, &mut load)?;
        } else if *minimum != checkpoint_authority.definition().world.genesis_head()? {
            return Err(invalid("native provenance genesis minimum"));
        }
        let node = descend(body, target, &mut load)?;
        let ArchivedEntry::Qualified {
            proof,
            data_closure: archived_data_closure,
        } = object(&node.entry, &mut load)?;
        if proof.decision.record.position != target.position
            || proof.decision.record.record_id()? != target.record_id
            || proof.decision.primary.body.finalizing_authority_id != authority_id
            || proof.decision.replica.body.finalizing_authority_id != authority_id
        {
            return Err(invalid(
                "native provenance exact archived proof/head/finalizer",
            ));
        }
        if let Some(id) = &archived_data_closure {
            crypto::hash_hex(id)?;
        }
        if let Some(id) = &body.data_closure {
            crypto::hash_hex(id)?;
            if body.head_node.is_none() {
                return Err(invalid("local closure requires a local Qualified node"));
            }
        }
        let data_closure = if *target == body.head {
            if body.head_node.is_some() && body.data_closure != archived_data_closure {
                return Err(invalid("local closure checkpoint/Qualified ref mismatch"));
            }
            body.data_closure.clone()
        } else {
            archived_data_closure
        };
        let parent = if let Some(link) = node.ancestors.first() {
            if link.head.position.checked_add(1) != Some(node.head.position) {
                return Err(invalid("native provenance immediate parent position"));
            }
            let parent_node: NativeHistoryNodeV1 = object(&link.node, &mut load)?;
            if parent_node.head != link.head {
                return Err(invalid("native provenance immediate parent node"));
            }
            let ArchivedEntry::Qualified { proof: parent, .. } =
                object(&parent_node.entry, &mut load)?;
            if parent.decision.record.position != link.head.position
                || parent.decision.record.record_id()? != link.head.record_id
                || parent.decision.primary.body.finalizing_authority_id
                    != parent.decision.replica.body.finalizing_authority_id
            {
                return Err(invalid("native provenance parent qualified context"));
            }
            Some(NativeCheckpointParentV1 {
                record: parent.decision.record,
                finalizing_authority_id: parent.decision.primary.body.finalizing_authority_id,
            })
        } else if target.position == 1 {
            // An imported checkpoint may retain this initial cut as its sealed
            // base_parent. That archive boundary is not a canonical predecessor
            // of the initial activation, which has no parent proof.
            None
        } else {
            body.base_parent.clone()
        };
        Ok(Self {
            proof,
            parent,
            head: target.clone(),
            authority_id: authority_id.into(),
            minimum: minimum.clone(),
            current_head: body.head.clone(),
            data_closure,
        })
    }
    /// Authenticated local reference only; callers must check its exact domain,
    /// namespace continuity and complete evidence. This grants no durability.
    pub fn local_data_closure_reference(&self) -> Option<&str> {
        self.data_closure.as_deref()
    }
    pub fn proof(&self) -> &FormalQualifiedCommitProofV1 {
        &self.proof
    }
    pub fn verified_parent_record(&self) -> Option<&CanonicalRecordV1> {
        self.parent.as_ref().map(|p| &p.record)
    }
    pub fn verified_parent_finalizing_authority_id(&self) -> Option<&str> {
        self.parent
            .as_ref()
            .map(|p| p.finalizing_authority_id.as_str())
    }
    pub fn head(&self) -> &FormalHeadRefV1 {
        &self.head
    }
    pub fn authority_id(&self) -> &str {
        &self.authority_id
    }
    pub(super) fn minimum(&self) -> &FormalHeadRefV1 {
        &self.minimum
    }
    pub(super) fn current_head(&self) -> &FormalHeadRefV1 {
        &self.current_head
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controlled_authority::formal::test_fixtures::*;
    #[derive(Serialize)]
    struct Signed<'a> {
        body: &'a NativeHotBodyV1,
        signature: String,
    }
    #[derive(Serialize)]
    enum Entry<'a> {
        Qualified {
            proof: &'a FormalQualifiedCommitProofV1,
        },
    }
    #[test]
    fn signed_native_provenance_checks_pin_hashes_and_exact_equal_cut_context() {
        let (certificate, authority, proof, commit) = initial();
        let mut objects = std::collections::BTreeMap::new();
        let mut put = |bytes: Vec<u8>| {
            let id = blake3::hash(&bytes).to_hex().to_string();
            objects.insert(id.clone(), bytes);
            id
        };
        let entry = put(serde_cbor::to_vec(&Entry::Qualified { proof: &proof }).unwrap());
        let node = NativeHistoryNodeV1 {
            head: commit.head(),
            request: proof.decision.record.request_digest.clone(),
            entry,
            ancestors: vec![],
        };
        let node = put(serde_cbor::to_vec(&node).unwrap());
        let body = NativeHotBodyV1 {
            version: 1,
            instance_digest: authority.definition().primary.instance.digest().unwrap(),
            authority_id: authority.authority_id().unwrap(),
            activation_ref: authority.activation_ref().into(),
            epoch: 1,
            revision: 7,
            head: commit.head(),
            head_node: Some(node),
            imported_head_node: None,
            import_digest: None,
            base_parent: None,
            index: None,
            pending: None,
            decision_head: None,
            seal: None,
            data_closure: None,
            durable_set: None,
        };
        let bytes = serde_cbor::to_vec(&Signed {
            body: &body,
            signature: sign(crypto::bytes(DOMAIN, &body).unwrap(), 5),
        })
        .unwrap();
        let checkpoint = fencing::EndpointCheckpointBindingV1 {
            revision: 7,
            commitment: blake3::hash(&bytes).to_hex().to_string(),
            head: crate::controlled_authority::replicated_protocol::HeadAnchor {
                position: commit.head().position,
                decision_hash: commit.head().record_id,
                qualified: true,
            },
        };
        let statement = fencing::MonotonicProviderStatementV1 {
            schema_version: 1,
            capability: fencing::ProviderCapabilityV1::MonotonicExclusiveSignerStorageV1,
            resource_binding: authority.policy().authorized_fencing_resource_bindings[0].clone(),
            instance_digest: body.instance_digest.clone(),
            checkpoint: checkpoint.clone(),
            epoch_floor: 1,
            seal_digest: None,
        };
        let provider = fencing::MonotonicProviderEvidenceV1 {
            signature: fencing::sign(fencing::PROVIDER_DOMAIN, &statement, &key(3)).unwrap(),
            statement,
        };
        let verify = |pin: &fencing::EndpointCheckpointBindingV1,
                      target: &FormalHeadRefV1,
                      corrupt: bool| {
            NativeCheckpointVerdictV1::verify(
                &bytes,
                &authority,
                &public(5),
                pin,
                &provider,
                target,
                &commit.head(),
                &authority.authority_id().unwrap(),
                |id| {
                    let mut bytes = objects
                        .get(id)
                        .cloned()
                        .ok_or_else(|| invalid("test missing object"))?;
                    if corrupt {
                        bytes.push(0);
                    }
                    Ok(bytes)
                },
            )
        };
        let check_provider = |provider: &fencing::MonotonicProviderEvidenceV1, key: &str| {
            NativeCheckpointVerdictV1::verify(
                &bytes,
                &authority,
                key,
                &checkpoint,
                provider,
                &commit.head(),
                &commit.head(),
                &authority.authority_id().unwrap(),
                |id| {
                    objects
                        .get(id)
                        .cloned()
                        .ok_or_else(|| invalid("test missing object"))
                },
            )
        };
        // Endpoint signatures and caller-created bindings cannot replace a
        // policy-authorized monotonic provider signature over this checkpoint.
        let mut endpoint_only = provider.clone();
        endpoint_only.signature =
            fencing::sign(fencing::PROVIDER_DOMAIN, &endpoint_only.statement, &key(5)).unwrap();
        assert!(check_provider(&endpoint_only, &public(5)).is_err());
        for field in 0..4 {
            let mut bad = provider.clone();
            match field {
                0 => bad.statement.checkpoint.revision += 1,
                1 => bad.statement.instance_digest = h(91),
                2 => bad.statement.resource_binding = h(92),
                _ => bad.statement.capability = fencing::ProviderCapabilityV1::ControlledLocalTest,
            }
            bad.signature =
                fencing::sign(fencing::PROVIDER_DOMAIN, &bad.statement, &key(3)).unwrap();
            assert!(check_provider(&bad, &public(5)).is_err());
        }
        let mut wrong_domain = provider.clone();
        wrong_domain.signature = fencing::sign(
            "ordinary-service-response",
            &wrong_domain.statement,
            &key(3),
        )
        .unwrap();
        assert!(check_provider(&wrong_domain, &public(5)).is_err());
        assert!(check_provider(&provider, &public(99)).is_err());
        let verdict = verify(&checkpoint, &commit.head(), false).unwrap();
        let restored =
            restore_formal_commit_from_checkpoint(&proof, &authority, None, &verdict).unwrap();
        let trust = InitialActivationTrustV1 {
            issuer_public_key: public(9),
            world: authority.definition().world.clone(),
            initial_state_root: certificate.body.initial_state_root.clone(),
            governing_manifest_digest: certificate.body.governing_manifest_digest.clone(),
            recovery_policy_digest: authority.policy().digest().unwrap(),
        };
        let trusted =
            VerifiedFormalReadAnchorV1::from_initial(&certificate, &trust, &proof).unwrap();
        let publication = VerifiedFormalReadAnchorV1::from_native_historical_service_restore(
            &trusted, &verdict, &authority, &restored,
        )
        .unwrap();
        assert_eq!(publication.checkpoint(), trusted.checkpoint());
        let mut stale_floor = checkpoint.clone();
        stale_floor.revision += 1;
        assert!(verify(&stale_floor, &commit.head(), false).is_err());
        let mut wrong_pin = checkpoint.clone();
        wrong_pin.commitment = h(99);
        assert!(verify(&wrong_pin, &commit.head(), false).is_err());
        let mut fork = commit.head();
        fork.record_id = h(98);
        assert!(verify(&checkpoint, &fork, false).is_err());
        assert!(verify(&checkpoint, &commit.head(), true).is_err());
    }
    /// Recovery imports retain the sealed cut as checkpoint base_parent, but
    /// that is an archive boundary, not the parent of the initial activation.
    #[test]
    fn imported_initial_cut_has_no_parent_despite_checkpoint_base_parent() {
        let (certificate, authority, proof, commit) = initial();
        let mut objects = std::collections::BTreeMap::new();
        let mut put = |bytes: Vec<u8>| {
            let id = blake3::hash(&bytes).to_hex().to_string();
            objects.insert(id.clone(), bytes);
            id
        };
        let entry = put(serde_cbor::to_vec(&Entry::Qualified { proof: &proof }).unwrap());
        let node = NativeHistoryNodeV1 {
            head: commit.head(),
            request: proof.decision.record.request_digest.clone(),
            entry,
            ancestors: vec![],
        };
        let node = put(serde_cbor::to_vec(&node).unwrap());
        let body = NativeHotBodyV1 {
            version: 1,
            instance_digest: authority.definition().primary.instance.digest().unwrap(),
            authority_id: authority.authority_id().unwrap(),
            activation_ref: authority.activation_ref().into(),
            epoch: 1,
            revision: 7,
            head: commit.head(),
            head_node: None,
            imported_head_node: Some(node),
            import_digest: Some(h(97)),
            base_parent: Some(NativeCheckpointParentV1 {
                record: commit.record().clone(),
                finalizing_authority_id: commit.finalizing_authority_id().into(),
            }),
            index: None,
            pending: None,
            decision_head: None,
            seal: None,
            data_closure: None,
            durable_set: None,
        };
        let bytes = serde_cbor::to_vec(&Signed {
            body: &body,
            signature: sign(crypto::bytes(DOMAIN, &body).unwrap(), 5),
        })
        .unwrap();
        let checkpoint = fencing::EndpointCheckpointBindingV1 {
            revision: 7,
            commitment: blake3::hash(&bytes).to_hex().to_string(),
            head: crate::controlled_authority::replicated_protocol::HeadAnchor {
                position: commit.head().position,
                decision_hash: commit.head().record_id,
                qualified: true,
            },
        };
        let statement = fencing::MonotonicProviderStatementV1 {
            schema_version: 1,
            capability: fencing::ProviderCapabilityV1::MonotonicExclusiveSignerStorageV1,
            resource_binding: authority.policy().authorized_fencing_resource_bindings[0].clone(),
            instance_digest: body.instance_digest.clone(),
            checkpoint: checkpoint.clone(),
            epoch_floor: 1,
            seal_digest: None,
        };
        let provider = fencing::MonotonicProviderEvidenceV1 {
            signature: fencing::sign(fencing::PROVIDER_DOMAIN, &statement, &key(3)).unwrap(),
            statement,
        };
        let evidence = NativeCheckpointVerdictV1::verify(
            &bytes,
            &authority,
            &public(5),
            &checkpoint,
            &provider,
            &commit.head(),
            &commit.head(),
            &authority.authority_id().unwrap(),
            |id| {
                objects
                    .get(id)
                    .cloned()
                    .ok_or_else(|| invalid("test missing imported object"))
            },
        )
        .unwrap();
        assert!(
            evidence.verified_parent_record().is_none(),
            "imported initial cut must not become its own historical parent"
        );
        assert!(evidence.verified_parent_finalizing_authority_id().is_none());
        let restored =
            restore_formal_commit_from_checkpoint(&proof, &authority, None, &evidence).unwrap();
        assert_eq!(restored.record(), commit.record());
        let trust = InitialActivationTrustV1 {
            issuer_public_key: public(9),
            world: authority.definition().world.clone(),
            initial_state_root: certificate.body.initial_state_root.clone(),
            governing_manifest_digest: certificate.body.governing_manifest_digest.clone(),
            recovery_policy_digest: authority.policy().digest().unwrap(),
        };
        let trusted =
            VerifiedFormalReadAnchorV1::from_initial(&certificate, &trust, &proof).unwrap();
        let publication = VerifiedFormalReadAnchorV1::from_native_historical_service_restore(
            &trusted, &evidence, &authority, &restored,
        )
        .unwrap();
        assert_eq!(publication.checkpoint(), trusted.checkpoint());
    }
}

#[cfg(test)]
#[path = "native_checkpoint_local_closure_tests.rs"]
mod local_closure_tests;
