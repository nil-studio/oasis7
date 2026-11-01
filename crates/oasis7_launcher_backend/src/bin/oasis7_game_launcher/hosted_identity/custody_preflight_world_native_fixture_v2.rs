//! Native component fixture uses actual human WebAuthn and actual managed-key
//! registration/login. Its Account issuer key is a fixture, not a paid Account.
use super::*;
use identity::custody::managed_binding::{ManagedBindingConfig,ManagedBindingConfigState};
use oasis7_custody_core::native_binding::*;
pub(super) struct Fixture {
    pub f:identity::model_session::model_session_tests::Fixture,
    pub human_key:openssl::pkey::PKey<openssl::pkey::Private>,
    pub sid:String,
    pub binding:NativeBindingReceiptV1,
    pub managed:ManagedNativeSessionStatusV1,
    pub registration:String,
}
pub(super) fn managed_fixture()->Fixture {
    let (mut f,human_key)=fixture();
    let context:ApprovalOptionsRequestV1=serde_json::from_value(session_options(now_ms())).unwrap();
    let epoch=context.context.service_epoch;
    let binding_owner=SigningKey::from_bytes(&[72;32]);
    f.service.config.registration_enabled=true;
    f.service.config.machine_registration_enabled=true;
    let custody=f.service.custody.as_mut().unwrap();
    custody.full_session_v2_summary_contract_enabled=true;
    custody.managed_binding_state=ManagedBindingConfigState::Ready;
    custody.managed_binding=ManagedBindingConfig::from_env().unwrap();
    let configured=custody.managed_binding.as_ref().unwrap().fixture_pin();
    assert_eq!(configured.service_epoch,epoch);
    assert_eq!(configured.environment,f.service.config.namespace.environment_id);
    assert_eq!(configured.public_key,hex::encode(binding_owner.verifying_key().to_bytes()));
    let login=SigningKey::from_bytes(&[71;32]);
    let preauth=URL_SAFE_NO_PAD.encode([75u8;32]);
    let pre=digest(&f.service.config.session_key,"preauth",&preauth);
    let registration=random_token();
    let options=f.service.machine_registration_route("POST","/machine-registration/options",&pre,
        &json!({"operation_id":registration,"public_key":hex::encode(login.verifying_key().to_bytes()),"display_name":"managed-world3"}),
        "source","https://localhost",now_ms()).unwrap().unwrap();
    let bytes=URL_SAFE_NO_PAD.decode(options["options"]["signed_bytes"].as_str().unwrap()).unwrap();
    f.service.machine_registration_route("POST","/machine-registration/complete",&pre,
        &json!({"operation_id":registration,"ceremony_id":registration,"signature":URL_SAFE_NO_PAD.encode(login.sign(&bytes).to_bytes())}),
        "source","https://localhost",now_ms()).unwrap().unwrap();
    let challenge:SignedNativeBindingChallengeV1=serde_json::from_value(f.service.managed_binding_route("POST",
        "/custody/managed-bindings/options",&pre,&json!({"registration_operation_id":registration,"binding_operation_id":"bind-world3"}),
        "https://localhost",now_ms()).unwrap()).unwrap();
    let observed_at=now_ms();
    let statement=OwnerNativeBindingStatementV1 {challenge:challenge.challenge.clone(),
        environment:f.service.config.namespace.environment_id.clone(),custody_account_id:"custody-a".into(),provision_id:"provision".into(),
        approved_plan_digest:"11".repeat(32),login_key_ref:"identity-key".into(),binding_operation_id:challenge.challenge.challenge_id.clone(),
        permit_id:"binding-permit".into(),source_revision:"22".repeat(32),account_authority_revision:"33".repeat(32),
        service_epoch:epoch,service_identity:"owner-service".into(),signer_key_id:"owner-binding".into(),
        authority_observed_at:DecimalU64(observed_at),expires_at:DecimalU64(challenge.challenge.expires_at.0.min(observed_at+5000))};
    let owner=SignedOwnerNativeBindingStatementV1 {signature:URL_SAFE_NO_PAD.encode(binding_owner.sign(&statement.signing_bytes().unwrap()).to_bytes()),statement};
    let possession=URL_SAFE_NO_PAD.encode(login.sign(&owner.statement.possession_signing_bytes().unwrap()).to_bytes());
    f.service.managed_binding_route("POST","/custody/managed-bindings/complete",&pre,
        &serde_json::to_value(NativeBindingCompleteV1 {api_version:1,statement:owner,login_key_signature:possession}).unwrap(),
        "https://localhost",now_ms()).unwrap();
    let options=f.service.native_key_route("POST","/native-key/options",None,&pre,
        &json!({"public_key":hex::encode(login.verifying_key().to_bytes()),"intent":"login"}),"source","https://localhost",now_ms()).unwrap().unwrap().0;
    let bytes=URL_SAFE_NO_PAD.decode(options["options"]["signed_bytes"].as_str().unwrap()).unwrap();
    let (_,sid)=f.service.native_key_route("POST","/native-key/complete",None,&pre,
        &json!({"operation_id":options["operation_id"],"ceremony_id":options["ceremony_id"],"signature":URL_SAFE_NO_PAD.encode(login.sign(&bytes).to_bytes())}),
        "source","https://localhost",now_ms()).unwrap().unwrap();
    let sid=sid.unwrap();
    let status:SignedManagedNativeSessionStatusV1=serde_json::from_value(f.service.managed_session_status(Some(&sid),
        &json!({"nonce":"44".repeat(32)}),now_ms()).unwrap()).unwrap();
    assert!(status.status.active);
    let binding=status.status.receipt.clone();
    assert!(status.status.session_ref!=f.service.session(Some(&f.token),now_ms()).unwrap().1.session_id);
    Fixture {f,human_key,sid,binding,managed:status.status,registration}
}
