use std::sync::Arc;

use ed25519_dalek::SigningKey;
use oasis7_proto::distributed_dht::{DistributedDht, MembershipDirectorySnapshot};

use crate::{
    MembershipDirectorySigner, MembershipDirectorySignerKeyring, MembershipSnapshotAuditOutcome,
    MembershipSnapshotRestorePolicy, MembershipSyncClient,
    distributed_dht::InMemoryDht,
    distributed_net::InMemoryNetwork,
    quorum::{ConsensusConfig, QuorumConsensus},
};

fn snapshot() -> MembershipDirectorySnapshot {
    MembershipDirectorySnapshot {
        world_id: "w1".into(),
        requester_id: "seq-1".into(),
        requested_at_ms: 400,
        reason: None,
        validators: vec![
            "seq-1".into(),
            "seq-2".into(),
            "seq-3".into(),
            "seq-4".into(),
        ],
        quorum_threshold: 3,
        signature_key_id: None,
        signature: None,
    }
}

#[test]
fn membership_restore_authentication_policy_matrix() {
    let key = SigningKey::from_bytes(&[51; 32]);
    let public_key = hex::encode(key.verifying_key().to_bytes());
    let private_key = hex::encode([51; 32]);
    let signer = MembershipDirectorySigner::ed25519(&private_key, &public_key).unwrap();
    let mut keyring = MembershipDirectorySignerKeyring::new();
    keyring
        .add_ed25519_key("key-1", &private_key, &public_key)
        .unwrap();
    let empty_keyring = MembershipDirectorySignerKeyring::new();
    let client = MembershipSyncClient::new(Arc::new(InMemoryNetwork::new()));

    // Every combination of the four authentication constraints must fail closed
    // without a usable verifier. A matching claimed signer is not authentication.
    for flags in [8_u8, 2, 0, 1, 3, 4, 5, 6, 7, 9, 10, 11, 12, 13, 14, 15] {
        let policy = MembershipSnapshotRestorePolicy {
            require_signature: flags & 1 != 0,
            require_signature_key_id: flags & 2 != 0,
            accepted_signature_key_ids: if flags & 4 != 0 {
                vec!["key-1".into()]
            } else {
                vec![]
            },
            accepted_signature_signer_public_keys: if flags & 8 != 0 {
                vec![public_key.clone()]
            } else {
                vec![]
            },
            ..Default::default()
        };
        for signed in [false, true] {
            for tampered in [false, true] {
                if !signed && tampered {
                    continue;
                }
                let mut snapshot = snapshot();
                if signed {
                    snapshot.signature_key_id = Some("key-1".into());
                    snapshot.signature = Some(signer.sign_snapshot(&snapshot).unwrap());
                    if tampered {
                        snapshot.reason = Some("modified after signing".into());
                    }
                }
                for verifier in 0..4 {
                    let dht = InMemoryDht::new();
                    dht.put_membership_directory("w1", &snapshot).unwrap();
                    let mut consensus = QuorumConsensus::new(ConsensusConfig::majority(vec![
                        "seq-1".into(),
                        "seq-2".into(),
                        "seq-3".into(),
                    ]))
                    .unwrap();
                    let before_validators = consensus.validators();
                    let before_threshold = consensus.quorum_threshold();
                    let before_records = consensus.export_records();
                    let report = client
                        .restore_membership_from_dht_verified_with_audit(
                            "w1",
                            &mut consensus,
                            &dht,
                            (verifier == 1).then_some(&signer),
                            match verifier {
                                2 => Some(&keyring),
                                3 => Some(&empty_keyring),
                                _ => None,
                            },
                            &policy,
                        )
                        .unwrap();
                    // Empty keyrings are present but unusable. Unsigned,
                    // unconstrained compatibility does not claim authentication.
                    let accepted = if signed {
                        if verifier == 0 {
                            flags == 0
                        } else {
                            verifier != 3 && !tampered
                        }
                    } else {
                        flags == 0
                    };
                    let context = format!(
                        "flags={flags}, signed={signed}, tampered={tampered}, verifier={verifier}: {}",
                        report.audit.reason
                    );
                    assert_eq!(
                        report.audit.outcome,
                        if accepted {
                            MembershipSnapshotAuditOutcome::Applied
                        } else {
                            MembershipSnapshotAuditOutcome::Rejected
                        },
                        "{context}"
                    );
                    if accepted {
                        assert_eq!(consensus.validators().len(), 4, "{context}");
                        assert_eq!(consensus.quorum_threshold(), 3, "{context}");
                        assert!(report.restored.unwrap().applied, "{context}");
                    } else {
                        assert!(report.restored.is_none(), "{context}");
                        assert_eq!(consensus.validators(), before_validators, "{context}");
                        assert_eq!(consensus.quorum_threshold(), before_threshold, "{context}");
                        assert_eq!(consensus.export_records(), before_records, "{context}");
                    }
                }
            }
        }
    }
}
