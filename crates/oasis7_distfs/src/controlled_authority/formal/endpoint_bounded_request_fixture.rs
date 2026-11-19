//! Fresh original-protocol signed states for data-loader tests. This does not
//! produce an IWS accepted GEN, execution permit, or serving qualification.
use super::*;
pub(in super::super) fn with_signed_request_states_fixture(
    mut run: impl FnMut(&str, &mut FileFormalEndpoint, &str, &FormalRequestStateV1),
) {
    fn visit(
        run: &mut impl FnMut(&str, &mut FileFormalEndpoint, &str, &FormalRequestStateV1),
        stage: &str,
        endpoint: &mut FileFormalEndpoint,
        request: &str,
    ) {
        let original = endpoint.load(request).unwrap().unwrap();
        run(stage, endpoint, request, &original);
    }
    let fixture = Fixture::new();
    let mut coordinator = fixture.coordinator();
    coordinator.submit(fixture.initial.clone()).unwrap();
    let (input, material) = fixture.frozen(coordinator.parent().unwrap(), 91);
    let request = input.body.request_digest.clone();
    let token = coordinator.freeze(input, material).unwrap();
    let candidate = fixture.candidate(&token);
    let (primary, replica) = coordinator.endpoints_mut();
    visit(&mut run, "Frozen", primary, &request);
    let p = primary.prepare(candidate.clone()).unwrap();
    let r = replica.prepare(candidate.clone()).unwrap();
    visit(&mut run, "Prepared", primary, &request);
    let p_final = primary.decide(&request, p.clone(), r.clone()).unwrap();
    let r_final = replica.decide(&request, p, r).unwrap();
    visit(&mut run, "Decided", primary, &request);
    let decision = FormalDecisionProofV1 {
        schema_version: 1,
        record: candidate.candidate.record.clone(),
        branch: candidate.branch,
        writer_signature_hex: candidate.candidate.writer_signature_hex.clone(),
        frozen: candidate.frozen.clone(),
        primary: p_final,
        replica: r_final,
    };
    let p_complete = primary.install_proof(decision.clone()).unwrap();
    let r_complete = replica.install_proof(decision.clone()).unwrap();
    visit(&mut run, "Ready", primary, &request);
    let proof = FormalQualifiedCommitProofV1 {
        schema_version: 1,
        decision,
        primary_complete: p_complete,
        replica_complete: r_complete,
    };
    primary.publish_qualified(proof.clone()).unwrap();
    replica.publish_qualified(proof).unwrap();
    visit(&mut run, "Qualified", primary, &request);
}

pub(in super::super) fn with_fresh_endpoint_for_bounded_request_fixture<T>(
    run: impl FnOnce(&mut FileFormalEndpoint, &Path) -> T,
) -> T {
    let fixture = Fixture::new();
    let mut endpoint = fixture.endpoint(EndpointRole::Primary);
    run(&mut endpoint, &fixture.pdir)
}
