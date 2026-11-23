//! Real RecoveryIssuer files and an independently persisted software witness.
//! The latter models an external monotonic service, not protected hardware.
use super::*;
use crate::controlled_authority::formal::issuer::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct WitnessState {
    grants: std::collections::BTreeMap<String, ExistingIssuerGrantV1>,
    anchor: IssuerAnchorV1,
}
#[derive(Clone, Copy)]
enum Failure {
    BeforeGrant,
    AfterGrant,
    None,
}
struct Witness {
    path: PathBuf,
    policy: RecoveryPolicyV1,
    failure: Failure,
}
impl Witness {
    fn read(&self) -> Result<WitnessState, FormalError> {
        if self.path.exists() {
            serde_json::from_slice(
                &fs::read(&self.path).map_err(|e| FormalError::Io(e.to_string()))?,
            )
            .map_err(|e| FormalError::Invalid(e.to_string()))
        } else {
            Ok(WitnessState {
                grants: std::collections::BTreeMap::new(),
                anchor: IssuerAnchorV1::initial(&self.policy)?,
            })
        }
    }
    fn persist(&self, state: &WitnessState) -> Result<(), FormalError> {
        let bytes = serde_json::to_vec(state).map_err(|e| FormalError::Invalid(e.to_string()))?;
        let temporary = self.path.with_extension("next");
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&temporary)
            .map_err(|e| FormalError::Io(e.to_string()))?;
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|e| FormalError::Io(e.to_string()))?;
        fs::rename(&temporary, &self.path).map_err(|e| FormalError::Io(e.to_string()))?;
        fs::File::open(self.path.parent().unwrap())
            .and_then(|f| f.sync_all())
            .map_err(|e| FormalError::Io(e.to_string()))
    }
    fn floor(&self, minimum: &IssuerAnchorV1, current: &WitnessState) -> Result<(), FormalError> {
        if minimum != &current.anchor && minimum != &IssuerAnchorV1::initial(&self.policy)? {
            return Err(FormalError::Rollback(
                "software witness original minimum changed".into(),
            ));
        }
        Ok(())
    }
}
impl IssuanceWitness for Witness {
    fn query_predecessor(
        &mut self,
        query: &SignedIssuerGrantQueryV1,
    ) -> Result<SignedIssuerGrantQueryResultV1, FormalError> {
        query.verify()?;
        let state = self.read()?;
        self.floor(&query.body.minimum_anchor, &state)?;
        if query.body.issuer_public_key != self.policy.recovery_issuer_public_key
            || query.body.policy_binding != policy_binding(&self.policy)?
        {
            return Err(FormalError::IssuerUnproven);
        }
        SignedIssuerGrantQueryResultV1::sign(
            IssuerGrantQueryResultBodyV1 {
                schema_version: 1,
                capability: IssuerWitnessCapabilityV1::NonrollbackUniqueSuccessorV1,
                query: query.body.clone(),
                current_anchor: state.anchor,
                grant: state
                    .grants
                    .get(&query.body.predecessor_authority_id)
                    .cloned(),
            },
            &key(2),
        )
    }
    fn authorize_or_query(
        &mut self,
        request: &SignedIssuanceRequestV1,
        certificate: &SignedRecoveryCertificateV1,
    ) -> Result<SignedIssuanceWitnessV1, FormalError> {
        request.verify()?;
        let mut state = self.read()?;
        self.floor(&request.body.minimum_anchor, &state)?;
        if request.body.issuer_public_key != self.policy.recovery_issuer_public_key
            || request.body.policy_binding != policy_binding(&self.policy)?
            || request.body.certificate_digest != recovery_certificate_digest(certificate)?
            || request.body.predecessor_authority_id != certificate.body.predecessor_authority_id
        {
            return Err(FormalError::IssuerUnproven);
        }
        if matches!(self.failure, Failure::BeforeGrant) {
            self.failure = Failure::None;
            return Err(FormalError::Io(
                "fixture interrupted after issuer intent persistence".into(),
            ));
        }
        if let Some(original) = state.grants.get(&certificate.body.predecessor_authority_id) {
            if &original.certificate != certificate {
                return Err(FormalError::Conflict(
                    "software witness retains the original successor".into(),
                ));
            }
        } else {
            let revision = state.anchor.revision.checked_add(1).ok_or_else(|| {
                FormalError::Invalid("software witness revision exhausted".into())
            })?;
            let digest = hash(
                "oasis7.controlled_software_issuer_grant.v1",
                &(
                    &state.anchor,
                    &certificate.body.predecessor_authority_id,
                    recovery_certificate_digest(certificate)?,
                    revision,
                ),
            )?;
            state.grants.insert(
                certificate.body.predecessor_authority_id.clone(),
                ExistingIssuerGrantV1 {
                    certificate: certificate.clone(),
                    grant_revision: revision,
                },
            );
            state.anchor = IssuerAnchorV1 { revision, digest };
            self.persist(&state)?;
        }
        if matches!(self.failure, Failure::AfterGrant) {
            self.failure = Failure::None;
            return Err(FormalError::Io(
                "fixture interrupted after witness grant persistence".into(),
            ));
        }
        SignedIssuanceWitnessV1::sign(
            IssuanceWitnessBodyV1 {
                schema_version: 1,
                capability: IssuerWitnessCapabilityV1::NonrollbackUniqueSuccessorV1,
                request: request.body.clone(),
                grant_revision: state
                    .grants
                    .get(&certificate.body.predecessor_authority_id)
                    .ok_or(FormalError::IssuerUnproven)?
                    .grant_revision,
                current_anchor: state.anchor,
            },
            &key(2),
        )
    }
}

pub(in super::super) fn issue_after_interruptions(
    fixture: &Fixture,
    intent: &VerifiedRecoveryIntent,
) -> (
    SignedRecoveryCertificateV1,
    SignedIssuanceWitnessV1,
    VerifiedIssuance,
) {
    let path = fixture.path.join("original-recovery-issuer");
    let witness_path = fixture.path.join("independent-original-issuer-grant.json");
    let minimum = IssuerAnchorV1::initial(fixture.authority.policy()).unwrap();
    let open = |floor: IssuerAnchorV1| {
        RecoveryIssuer::open(&path, fixture.authority.policy().clone(), key(1), floor).unwrap()
    };
    let mut issuer = open(minimum.clone());
    let mut witness = Witness {
        path: witness_path.clone(),
        policy: fixture.authority.policy().clone(),
        failure: Failure::BeforeGrant,
    };
    let previous_grants = fs::read(&witness_path).ok();
    assert!(matches!(
        issuer.issue(intent, &mut witness),
        Err(FormalError::Io(_))
    ));
    let original_intent = fs::read(path.join(format!(
        "{}.intent.json",
        intent.body().predecessor_authority_id
    )))
    .unwrap();
    assert_eq!(fs::read(&witness_path).ok(), previous_grants);
    drop(issuer);
    let mut issuer = open(minimum.clone());
    witness.failure = Failure::AfterGrant;
    assert!(matches!(
        issuer.issue(intent, &mut witness),
        Err(FormalError::Io(_))
    ));
    assert_eq!(
        fs::read(path.join(format!(
            "{}.intent.json",
            intent.body().predecessor_authority_id
        )))
        .unwrap(),
        original_intent
    );
    assert!(
        !path
            .join(format!(
                "{}.result.json",
                intent.body().predecessor_authority_id
            ))
            .exists()
    );
    let grant_bytes = fs::read(&witness_path).unwrap();
    drop(issuer);
    drop(witness);
    let mut issuer = open(minimum);
    let mut witness = Witness {
        path: witness_path,
        policy: fixture.authority.policy().clone(),
        failure: Failure::None,
    };
    let (certificate, receipt, _) = issuer.issue(intent, &mut witness).unwrap();
    assert_eq!(&certificate.body, intent.body());
    let floor = issuer.minimum_anchor().clone();
    assert!(
        path.join(format!(
            "{}.result.json",
            intent.body().predecessor_authority_id
        ))
        .is_file()
    );
    drop(issuer);
    let mut issuer = open(floor);
    let (again, cold_receipt, checked) = issuer.issue(intent, &mut witness).unwrap();
    assert_eq!(again, certificate);
    assert_eq!(
        cold_receipt.body.grant_revision,
        receipt.body.grant_revision
    );
    assert_eq!(fs::read(&witness.path).unwrap(), grant_bytes);
    eprintln!(
        "CA-F2 issuer original-intent/witness-grant/result cold reconciliation passed operation={}",
        intent.body().operation_id
    );
    (again, cold_receipt, checked)
}
