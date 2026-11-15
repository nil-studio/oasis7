//! Fixed pre-extension CBOR oracle: optional local metadata must not re-sign
//! every existing checkpoint merely because the binary was upgraded.
use super::*;
#[test]
fn absent_local_data_closure_preserves_original_hot_body_bytes() {
    let body = NativeHotBodyV1 {
        version: 1,
        instance_digest: "a".into(),
        authority_id: "b".into(),
        activation_ref: "c".into(),
        epoch: 1,
        revision: 0,
        head: FormalHeadRefV1 {
            position: 0,
            record_id: "d".into(),
        },
        head_node: None,
        imported_head_node: None,
        import_digest: None,
        base_parent: None,
        index: None,
        pending: None,
        decision_head: None,
        seal: None,
        durable_set: None,
        data_closure: None,
    };
    let original = hex::decode("af6776657273696f6e016f696e7374616e63655f64696765737461616c617574686f726974795f696461626e61637469766174696f6e5f72656661636565706f636801687265766973696f6e006468656164a268706f736974696f6e00697265636f72645f6964616469686561645f6e6f6465f672696d706f727465645f686561645f6e6f6465f66d696d706f72745f646967657374f66b626173655f706172656e74f665696e646578f66770656e64696e67f66d6465636973696f6e5f68656164f6647365616cf6").unwrap();
    assert_eq!(serde_cbor::to_vec(&body).unwrap(), original);
    assert_eq!(
        serde_cbor::from_slice::<NativeHotBodyV1>(&original).unwrap(),
        body
    );
}

#[test]
fn signed_native_local_reference_requires_the_exact_qualified_entry_and_domain() {
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
            #[serde(skip_serializing_if = "Option::is_none")]
            data_closure: Option<&'a str>,
        },
    }
    let (_, authority, proof, commit) = initial();
    let reference = h(78);
    for (hot_ref, entry_ref, wrong_domain, accepted) in [
        (None, None, false, true),
        (
            Some(reference.as_str()),
            Some(reference.as_str()),
            false,
            true,
        ),
        (Some(reference.as_str()), None, false, false),
        (None, Some(reference.as_str()), false, false),
        (
            Some(reference.as_str()),
            Some(reference.as_str()),
            true,
            false,
        ),
    ] {
        let entry = serde_cbor::to_vec(&Entry::Qualified {
            proof: &proof,
            data_closure: entry_ref,
        })
        .unwrap();
        let entry_id = blake3::hash(&entry).to_hex().to_string();
        let node = NativeHistoryNodeV1 {
            head: commit.head(),
            request: proof.decision.record.request_digest.clone(),
            entry: entry_id.clone(),
            ancestors: vec![],
        };
        let node_bytes = serde_cbor::to_vec(&node).unwrap();
        let node_id = blake3::hash(&node_bytes).to_hex().to_string();
        let body = NativeHotBodyV1 {
            version: 1,
            instance_digest: if wrong_domain {
                h(99)
            } else {
                authority.definition().primary.instance.digest().unwrap()
            },
            authority_id: authority.authority_id().unwrap(),
            activation_ref: authority.activation_ref().into(),
            epoch: 1,
            revision: 7,
            head: commit.head(),
            head_node: Some(node_id.clone()),
            imported_head_node: None,
            import_digest: None,
            base_parent: None,
            index: None,
            pending: None,
            decision_head: None,
            seal: None,
            durable_set: None,
            data_closure: hot_ref.map(str::to_owned),
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
        let verdict = NativeCheckpointVerdictV1::verify(
            &bytes,
            &authority,
            &public(5),
            &checkpoint,
            &provider,
            &commit.head(),
            &commit.head(),
            &authority.authority_id().unwrap(),
            |id| {
                if id == entry_id {
                    Ok(entry.clone())
                } else if id == node_id {
                    Ok(node_bytes.clone())
                } else {
                    Err(invalid("local reference test missing authenticated object"))
                }
            },
        );
        assert_eq!(verdict.is_ok(), accepted);
        if accepted {
            assert_eq!(verdict.unwrap().local_data_closure_reference(), hot_ref);
        }
    }
}
