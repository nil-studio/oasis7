use oasis7_custody_core::client_enrollment::*;
use oasis7_custody_core::{DecimalU64, time_units::UnixMillis};
fn descriptor() -> HostDeploymentDescriptorV1 {
    HostDeploymentDescriptorV1 {
        api_version: 1,
        environment: "test".into(),
        transport_profile: HostTransportProfileV1::NativeBrokerCustodyV1,
        tool_profile: "custody-native-v1".into(),
        host_instance_id: "host-one".into(),
        client_public_key: "ab".repeat(32),
        build_digest: "bc".repeat(32),
        skill_manifest_digest: "cd".repeat(32),
        tool_manifest_digest: "de".repeat(32),
        https_origin: "https://host.example".into(),
        restricted_runtime_identity: "test-only-different-principal".into(),
        isolation_mechanism: HostIsolationMechanismV1::SeparateOsPrincipal,
        isolation_acceptance_refs: vec!["synthetic-fixture-not-CT008-pass".into()],
    }
}
#[test]
fn host_descriptor_is_closed_metadata_and_default_allowlist_is_empty() {
    let d = descriptor();
    d.validate().unwrap();
    assert!(
        HostDeploymentAllowlist::default()
            .configured_metadata(&d)
            .unwrap()
            .is_none()
    );
    let pin = HostDeploymentPinV1 {
        environment: d.environment.clone(),
        host_instance_id: d.host_instance_id.clone(),
        descriptor_digest: d.digest().unwrap(),
        client_public_key: d.client_public_key.clone(),
    };
    let configured = HostDeploymentAllowlist::from_fixed_config(vec![pin.clone()]).unwrap();
    assert_eq!(configured.configured_metadata(&d).unwrap(), Some(&pin));
    for changed in 0..4 {
        let mut v = d.clone();
        match changed {
            0 => v.https_origin = "https://other.example".into(),
            1 => v.client_public_key = "aa".repeat(32),
            2 => v.skill_manifest_digest = "aa".repeat(32),
            _ => v.host_instance_id = "other-host".into(),
        };
        assert!(configured.configured_metadata(&v).unwrap().is_none());
    }
    let mut raw = serde_json::to_value(&d).unwrap();
    raw["approvedHost"] = true.into();
    assert!(serde_json::from_value::<HostDeploymentDescriptorV1>(raw).is_err());
    assert!(HostDeploymentAllowlist::from_fixed_config(vec![pin.clone(), pin]).is_err());
}
#[test]
fn claim_is_only_execute_read_and_explicit_generation_replacement() {
    let mut r = ClientCredentialClaimRequestV1 {
        api_version: 1,
        enrollment_id: "enrollment-one".into(),
        test_session_id: "session-one".into(),
        request_id: "claim-one".into(),
        purposes: vec![TestSessionPurposeV1::Execute, TestSessionPurposeV1::Read],
        replace: false,
        expected_generation: None,
    };
    r.validate().unwrap();
    r.purposes.push(TestSessionPurposeV1::Cleanup);
    assert!(r.validate().is_err());
    r.purposes.pop();
    r.replace = true;
    assert!(r.validate().is_err());
    r.expected_generation = Some(DecimalU64(u64::MAX));
    r.validate().unwrap();
    let limit = |purpose| TestSessionPurposeLimitsV1 {
        purpose,
        not_before: UnixMillis::new(1000),
        deadline: UnixMillis::new(2000),
        request_limit: DecimalU64(1),
        transport_limit: DecimalU64(1),
        business_limit: DecimalU64(0),
    };
    let mut three = ThreePurposeLimitsV1 {
        execute: limit(TestSessionPurposeV1::Execute),
        read: limit(TestSessionPurposeV1::Read),
        cleanup: limit(TestSessionPurposeV1::Cleanup),
    };
    three.validate().unwrap();
    three.read.business_limit = DecimalU64(1);
    assert!(three.validate().is_err());
}
