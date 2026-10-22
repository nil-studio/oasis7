use super::{CliOptions, build_audit_report, parse_options, validate_audit_report};
use oasis7_runtime::{
    GovernanceExecutionPolicy, GovernanceFinalitySignerRegistry,
    GovernanceMainTokenControllerRegistry, GovernanceThresholdSignerPolicy,
    RollbackAuthorityRecord, RollbackAuthorityRegistry, RollbackAuthorityRole,
};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_DIR_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn temp_dir(prefix: &str) -> PathBuf {
    let sequence = TEMP_DIR_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    std::env::temp_dir().join(format!("oasis7-governance-audit-{prefix}-{pid}-{sequence}"))
}

fn rollback_registry(on_call_key: &str, governance_key: &str) -> RollbackAuthorityRegistry {
    RollbackAuthorityRegistry::new([
        RollbackAuthorityRecord {
            authority_id: "rollback-on-call-01".to_string(),
            role: RollbackAuthorityRole::OnCall,
            public_key_hex: on_call_key.to_string(),
            active: true,
        },
        RollbackAuthorityRecord {
            authority_id: "rollback-governance-01".to_string(),
            role: RollbackAuthorityRole::Governance,
            public_key_hex: governance_key.to_string(),
            active: true,
        },
    ])
    .expect("valid rollback registry")
}

fn provision_rollback_fixture(world_dir: &std::path::Path, manifest_path: &std::path::Path) {
    const ON_CALL: &str = "1111111111111111111111111111111111111111111111111111111111111111";
    const GOVERNANCE: &str = "2222222222222222222222222222222222222222222222222222222222222222";
    let mut world =
        oasis7::runtime_host::world::load_from_dir(world_dir).expect("load fixture world");
    let _ = std::fs::remove_dir_all(world_dir.join(".distfs-state"));
    world
        .set_rollback_authority_registry(rollback_registry(ON_CALL, GOVERNANCE))
        .expect("set rollback registry");
    world
        .save_to_dir(world_dir)
        .expect("save rollback registry");
    let mut manifest: Vec<serde_json::Value> = serde_json::from_slice(
        std::fs::read(manifest_path)
            .expect("read fixture manifest")
            .as_slice(),
    )
    .expect("decode fixture manifest");
    manifest.extend([
        serde_json::json!({
            "slot_id": "ops.rollback.on_call.v1", "signer_id": "rollback-on-call-01",
            "scheme": "ed25519", "threshold": 1, "public_key_hex": ON_CALL
        }),
        serde_json::json!({
            "slot_id": "governance.rollback.v1", "signer_id": "rollback-governance-01",
            "scheme": "ed25519", "threshold": 1, "public_key_hex": GOVERNANCE
        }),
    ]);
    std::fs::write(
        manifest_path,
        serde_json::to_vec_pretty(&manifest).expect("encode fixture manifest"),
    )
    .expect("write fixture manifest");
}

fn write_world_and_manifest() -> (PathBuf, PathBuf) {
    let root = temp_dir("fixture");
    std::fs::create_dir_all(&root).expect("create root");
    let world_dir = root.join("world");
    let manifest_path = root.join("public_manifest.json");
    std::fs::create_dir_all(&world_dir).expect("create world dir");
    let mut world = oasis7::runtime_host::world::new();
    world
        .set_governance_finality_signer_registry(GovernanceFinalitySignerRegistry {
            slot_id: "governance.finality.v1".to_string(),
            threshold: 2,
            threshold_bps: 0,
            signer_bindings: BTreeMap::from([
                (
                    "governance.finality.v1.signer01".to_string(),
                    "54e7a02919fff2d49a9c325def8cb0211ea7f7a75a9011b9d0678b9e2a7af6bc".to_string(),
                ),
                (
                    "governance.finality.v1.signer02".to_string(),
                    "38dac17ff403cc19de033e47be7cf7b5354635fbc5c1976d7c532e20494aace4".to_string(),
                ),
                (
                    "governance.finality.v1.signer03".to_string(),
                    "e22bd5029176296712fb1a477f91c15775e5ab858181cb4172839ced526f12c8".to_string(),
                ),
            ]),
            validator_stakes: BTreeMap::new(),
        })
        .expect("set finality registry");
    world
        .set_governance_main_token_controller_registry(GovernanceMainTokenControllerRegistry {
            genesis_controller_account_id: "msig.genesis.v1".to_string(),
            treasury_bucket_controller_slots: BTreeMap::from([
                (
                    "staking_reward_pool".to_string(),
                    "msig.staking_governance.v1".to_string(),
                ),
                (
                    "ecosystem_pool".to_string(),
                    "msig.ecosystem_governance.v1".to_string(),
                ),
                (
                    "security_reserve".to_string(),
                    "msig.security_council.v1".to_string(),
                ),
            ]),
            restricted_starter_claim_admin_account_ids: BTreeSet::from([
                "msig.ecosystem_governance.v1".to_string(),
            ]),
            controller_signer_policies: BTreeMap::from([
                (
                    "msig.genesis.v1".to_string(),
                    GovernanceThresholdSignerPolicy {
                        threshold: 2,
                        allowed_public_keys: BTreeSet::from([
                            "6249e5a58278dbc4e629a16b5d33f6b84c39e3ceeb10e963bb9ef64ea4daac30"
                                .to_string(),
                            "7014e88a6336ec91fc7e6ffb044b50232e4411ec403f90123fa8a202a3420a04"
                                .to_string(),
                            "f4ecbcb4cbff4acb76cd4bf80fd3b6589a5c7ca2ac2f812380acb7b2cfa4a27c"
                                .to_string(),
                        ]),
                    },
                ),
                (
                    "msig.staking_governance.v1".to_string(),
                    GovernanceThresholdSignerPolicy {
                        threshold: 2,
                        allowed_public_keys: BTreeSet::from([
                            "13c160fc0f516b9a5663aa00c2a5446be6467f68ce341fdd79cdb64224dffd20"
                                .to_string(),
                            "10fa4d90abf753ec1aa54aee3ea53bab25f43e7078897e1fb6a3777af2255bcb"
                                .to_string(),
                            "c5c478f1a86b1ecdfa2d09af65f673d2835ee65f35ebd237270295d3773c2ba4"
                                .to_string(),
                        ]),
                    },
                ),
                (
                    "msig.ecosystem_governance.v1".to_string(),
                    GovernanceThresholdSignerPolicy {
                        threshold: 2,
                        allowed_public_keys: BTreeSet::from([
                            "0241f2e23305407676f2a5cec6d154da74944b2a366b2b2b6913cb746d402d0e"
                                .to_string(),
                            "960137cd5d675a517daed5f14ea6bea460e196fda4310a581ecd448f3bcd20b4"
                                .to_string(),
                            "f01a2f8e033d38b369af6bb9a80814a97d749a89ac9d071cc2fdfde1b1010b8a"
                                .to_string(),
                        ]),
                    },
                ),
                (
                    "msig.security_council.v1".to_string(),
                    GovernanceThresholdSignerPolicy {
                        threshold: 2,
                        allowed_public_keys: BTreeSet::from([
                            "d09de9413371ae42f643e4f8f31e2139611d1617809375b1ad884df3fb089448"
                                .to_string(),
                            "aa738a832b0d3bf371d231a0bd8502fd411f2a9723246e5d7d215e8fb0ecbb7c"
                                .to_string(),
                            "f852493e575e33647c005fe2fb43eb15963c73e4213c411e3920f25b498a6980"
                                .to_string(),
                        ]),
                    },
                ),
            ]),
        })
        .expect("set controller registry");
    world.save_to_dir(&world_dir).expect("save world");
    std::fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&vec![
            serde_json::json!({"slot_id":"governance.finality.v1","signer_id":"signer01","scheme":"ed25519","public_key_hex":"54e7a02919fff2d49a9c325def8cb0211ea7f7a75a9011b9d0678b9e2a7af6bc"}),
            serde_json::json!({"slot_id":"governance.finality.v1","signer_id":"signer02","scheme":"ed25519","public_key_hex":"38dac17ff403cc19de033e47be7cf7b5354635fbc5c1976d7c532e20494aace4"}),
            serde_json::json!({"slot_id":"governance.finality.v1","signer_id":"signer03","scheme":"ed25519","public_key_hex":"e22bd5029176296712fb1a477f91c15775e5ab858181cb4172839ced526f12c8"}),
            serde_json::json!({"slot_id":"msig.genesis.v1","signer_id":"signer01","scheme":"ed25519","public_key_hex":"6249e5a58278dbc4e629a16b5d33f6b84c39e3ceeb10e963bb9ef64ea4daac30"}),
            serde_json::json!({"slot_id":"msig.genesis.v1","signer_id":"signer02","scheme":"ed25519","public_key_hex":"7014e88a6336ec91fc7e6ffb044b50232e4411ec403f90123fa8a202a3420a04"}),
            serde_json::json!({"slot_id":"msig.genesis.v1","signer_id":"signer03","scheme":"ed25519","public_key_hex":"f4ecbcb4cbff4acb76cd4bf80fd3b6589a5c7ca2ac2f812380acb7b2cfa4a27c"}),
            serde_json::json!({"slot_id":"msig.staking_governance.v1","signer_id":"signer01","scheme":"ed25519","public_key_hex":"13c160fc0f516b9a5663aa00c2a5446be6467f68ce341fdd79cdb64224dffd20"}),
            serde_json::json!({"slot_id":"msig.staking_governance.v1","signer_id":"signer02","scheme":"ed25519","public_key_hex":"10fa4d90abf753ec1aa54aee3ea53bab25f43e7078897e1fb6a3777af2255bcb"}),
            serde_json::json!({"slot_id":"msig.staking_governance.v1","signer_id":"signer03","scheme":"ed25519","public_key_hex":"c5c478f1a86b1ecdfa2d09af65f673d2835ee65f35ebd237270295d3773c2ba4"}),
            serde_json::json!({"slot_id":"msig.ecosystem_governance.v1","signer_id":"signer01","scheme":"ed25519","public_key_hex":"0241f2e23305407676f2a5cec6d154da74944b2a366b2b2b6913cb746d402d0e"}),
            serde_json::json!({"slot_id":"msig.ecosystem_governance.v1","signer_id":"signer02","scheme":"ed25519","public_key_hex":"960137cd5d675a517daed5f14ea6bea460e196fda4310a581ecd448f3bcd20b4"}),
            serde_json::json!({"slot_id":"msig.ecosystem_governance.v1","signer_id":"signer03","scheme":"ed25519","public_key_hex":"f01a2f8e033d38b369af6bb9a80814a97d749a89ac9d071cc2fdfde1b1010b8a"}),
            serde_json::json!({"slot_id":"msig.security_council.v1","signer_id":"signer01","scheme":"ed25519","public_key_hex":"d09de9413371ae42f643e4f8f31e2139611d1617809375b1ad884df3fb089448"}),
            serde_json::json!({"slot_id":"msig.security_council.v1","signer_id":"signer02","scheme":"ed25519","public_key_hex":"aa738a832b0d3bf371d231a0bd8502fd411f2a9723246e5d7d215e8fb0ecbb7c"}),
            serde_json::json!({"slot_id":"msig.security_council.v1","signer_id":"signer03","scheme":"ed25519","public_key_hex":"f852493e575e33647c005fe2fb43eb15963c73e4213c411e3920f25b498a6980"})
        ])
        .expect("encode manifest"),
    )
    .expect("write manifest");
    (world_dir, manifest_path)
}

#[test]
fn parse_options_reads_flags() {
    let options = parse_options(
        [
            "--world-dir",
            "output/world",
            "--public-manifest",
            "manifest.json",
            "--strict-manifest-match",
            "--require-single-failure-tolerance",
        ]
        .into_iter(),
    )
    .expect("parse options");
    assert!(options.strict_manifest_match);
    assert!(options.require_single_failure_tolerance);
    assert_eq!(options.default_expected_threshold, 2);
}

#[test]
fn audit_report_passes_for_matching_two_of_three_registry() {
    let (world_dir, manifest_path) = write_world_and_manifest();
    provision_rollback_fixture(&world_dir, &manifest_path);
    let options = CliOptions {
        world_dir,
        public_manifest: Some(manifest_path),
        finality_slot_id: "governance.finality.v1".to_string(),
        default_expected_threshold: 2,
        strict_manifest_match: true,
        require_single_failure_tolerance: true,
    };
    let report = build_audit_report(&options).expect("build report");
    assert!(report.overall_single_failure_tolerance_pass);
    assert_eq!(report.manifest_match_pass, Some(true));
    assert_eq!(report.overall_status, "ready_for_ops_drill");
    assert!(validate_audit_report(&options, &report).is_empty());
}

#[test]
fn audit_report_blocks_single_failure_when_threshold_equals_signer_count() {
    let (world_dir, _) = write_world_and_manifest();
    let options = CliOptions {
        world_dir,
        public_manifest: None,
        finality_slot_id: "governance.finality.v1".to_string(),
        default_expected_threshold: 3,
        strict_manifest_match: false,
        require_single_failure_tolerance: true,
    };
    let report = build_audit_report(&options).expect("build report");
    assert!(!report.finality.threshold_matches_expectation);
    assert!(!validate_audit_report(&options, &report).is_empty());
}

#[test]
fn audit_report_accepts_manifest_specific_liveops_threshold() {
    let root = temp_dir("liveops-threshold");
    std::fs::create_dir_all(&root).expect("create root");
    let world_dir = root.join("world");
    let manifest_path = root.join("public_manifest.json");
    std::fs::create_dir_all(&world_dir).expect("create world dir");
    let mut world = oasis7::runtime_host::world::new();
    world
        .set_governance_finality_signer_registry(GovernanceFinalitySignerRegistry {
            slot_id: "governance.finality.v1".to_string(),
            threshold: 2,
            threshold_bps: 0,
            signer_bindings: BTreeMap::from([
                (
                    "governance.finality.v1.signer01".to_string(),
                    "54e7a02919fff2d49a9c325def8cb0211ea7f7a75a9011b9d0678b9e2a7af6bc".to_string(),
                ),
                (
                    "governance.finality.v1.signer02".to_string(),
                    "38dac17ff403cc19de033e47be7cf7b5354635fbc5c1976d7c532e20494aace4".to_string(),
                ),
                (
                    "governance.finality.v1.signer03".to_string(),
                    "e22bd5029176296712fb1a477f91c15775e5ab858181cb4172839ced526f12c8".to_string(),
                ),
            ]),
            validator_stakes: BTreeMap::new(),
        })
        .expect("set finality registry");
    world
        .set_governance_main_token_controller_registry(GovernanceMainTokenControllerRegistry {
            genesis_controller_account_id: "msig.genesis.v1".to_string(),
            treasury_bucket_controller_slots: BTreeMap::from([
                (
                    "staking_reward_pool".to_string(),
                    "msig.staking_governance.v1".to_string(),
                ),
                (
                    "ecosystem_pool".to_string(),
                    "msig.ecosystem_governance.v1".to_string(),
                ),
                (
                    "security_reserve".to_string(),
                    "msig.security_council.v1".to_string(),
                ),
            ]),
            restricted_starter_claim_admin_account_ids: BTreeSet::from(["liveops".to_string()]),
            controller_signer_policies: BTreeMap::from([
                (
                    "msig.genesis.v1".to_string(),
                    GovernanceThresholdSignerPolicy {
                        threshold: 2,
                        allowed_public_keys: BTreeSet::from([
                            "6249e5a58278dbc4e629a16b5d33f6b84c39e3ceeb10e963bb9ef64ea4daac30"
                                .to_string(),
                            "7014e88a6336ec91fc7e6ffb044b50232e4411ec403f90123fa8a202a3420a04"
                                .to_string(),
                            "f4ecbcb4cbff4acb76cd4bf80fd3b6589a5c7ca2ac2f812380acb7b2cfa4a27c"
                                .to_string(),
                        ]),
                    },
                ),
                (
                    "msig.staking_governance.v1".to_string(),
                    GovernanceThresholdSignerPolicy {
                        threshold: 2,
                        allowed_public_keys: BTreeSet::from([
                            "13c160fc0f516b9a5663aa00c2a5446be6467f68ce341fdd79cdb64224dffd20"
                                .to_string(),
                            "10fa4d90abf753ec1aa54aee3ea53bab25f43e7078897e1fb6a3777af2255bcb"
                                .to_string(),
                            "c5c478f1a86b1ecdfa2d09af65f673d2835ee65f35ebd237270295d3773c2ba4"
                                .to_string(),
                        ]),
                    },
                ),
                (
                    "msig.ecosystem_governance.v1".to_string(),
                    GovernanceThresholdSignerPolicy {
                        threshold: 2,
                        allowed_public_keys: BTreeSet::from([
                            "0241f2e23305407676f2a5cec6d154da74944b2a366b2b2b6913cb746d402d0e"
                                .to_string(),
                            "960137cd5d675a517daed5f14ea6bea460e196fda4310a581ecd448f3bcd20b4"
                                .to_string(),
                            "f01a2f8e033d38b369af6bb9a80814a97d749a89ac9d071cc2fdfde1b1010b8a"
                                .to_string(),
                        ]),
                    },
                ),
                (
                    "msig.security_council.v1".to_string(),
                    GovernanceThresholdSignerPolicy {
                        threshold: 2,
                        allowed_public_keys: BTreeSet::from([
                            "d09de9413371ae42f643e4f8f31e2139611d1617809375b1ad884df3fb089448"
                                .to_string(),
                            "aa738a832b0d3bf371d231a0bd8502fd411f2a9723246e5d7d215e8fb0ecbb7c"
                                .to_string(),
                            "f852493e575e33647c005fe2fb43eb15963c73e4213c411e3920f25b498a6980"
                                .to_string(),
                        ]),
                    },
                ),
                (
                    "liveops".to_string(),
                    GovernanceThresholdSignerPolicy {
                        threshold: 1,
                        allowed_public_keys: BTreeSet::from([
                            "14699ee340994e43103490585a96671ec66a3280bc0f90518f29cd1866f0fa7d"
                                .to_string(),
                            "b6517819f923b8b25989042b03e00854673b5517be88e3f568141373105ca77f"
                                .to_string(),
                        ]),
                    },
                ),
            ]),
        })
        .expect("set controller registry");
    world.save_to_dir(&world_dir).expect("save world");
    std::fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&vec![
            serde_json::json!({"slot_id":"governance.finality.v1","signer_id":"signer01","scheme":"ed25519","public_key_hex":"54e7a02919fff2d49a9c325def8cb0211ea7f7a75a9011b9d0678b9e2a7af6bc"}),
            serde_json::json!({"slot_id":"governance.finality.v1","signer_id":"signer02","scheme":"ed25519","public_key_hex":"38dac17ff403cc19de033e47be7cf7b5354635fbc5c1976d7c532e20494aace4"}),
            serde_json::json!({"slot_id":"governance.finality.v1","signer_id":"signer03","scheme":"ed25519","public_key_hex":"e22bd5029176296712fb1a477f91c15775e5ab858181cb4172839ced526f12c8"}),
            serde_json::json!({"slot_id":"msig.genesis.v1","signer_id":"signer01","scheme":"ed25519","public_key_hex":"6249e5a58278dbc4e629a16b5d33f6b84c39e3ceeb10e963bb9ef64ea4daac30"}),
            serde_json::json!({"slot_id":"msig.genesis.v1","signer_id":"signer02","scheme":"ed25519","public_key_hex":"7014e88a6336ec91fc7e6ffb044b50232e4411ec403f90123fa8a202a3420a04"}),
            serde_json::json!({"slot_id":"msig.genesis.v1","signer_id":"signer03","scheme":"ed25519","public_key_hex":"f4ecbcb4cbff4acb76cd4bf80fd3b6589a5c7ca2ac2f812380acb7b2cfa4a27c"}),
            serde_json::json!({"slot_id":"msig.staking_governance.v1","signer_id":"signer01","scheme":"ed25519","public_key_hex":"13c160fc0f516b9a5663aa00c2a5446be6467f68ce341fdd79cdb64224dffd20"}),
            serde_json::json!({"slot_id":"msig.staking_governance.v1","signer_id":"signer02","scheme":"ed25519","public_key_hex":"10fa4d90abf753ec1aa54aee3ea53bab25f43e7078897e1fb6a3777af2255bcb"}),
            serde_json::json!({"slot_id":"msig.staking_governance.v1","signer_id":"signer03","scheme":"ed25519","public_key_hex":"c5c478f1a86b1ecdfa2d09af65f673d2835ee65f35ebd237270295d3773c2ba4"}),
            serde_json::json!({"slot_id":"msig.ecosystem_governance.v1","signer_id":"signer01","scheme":"ed25519","public_key_hex":"0241f2e23305407676f2a5cec6d154da74944b2a366b2b2b6913cb746d402d0e"}),
            serde_json::json!({"slot_id":"msig.ecosystem_governance.v1","signer_id":"signer02","scheme":"ed25519","public_key_hex":"960137cd5d675a517daed5f14ea6bea460e196fda4310a581ecd448f3bcd20b4"}),
            serde_json::json!({"slot_id":"msig.ecosystem_governance.v1","signer_id":"signer03","scheme":"ed25519","public_key_hex":"f01a2f8e033d38b369af6bb9a80814a97d749a89ac9d071cc2fdfde1b1010b8a"}),
            serde_json::json!({"slot_id":"msig.security_council.v1","signer_id":"signer01","scheme":"ed25519","public_key_hex":"d09de9413371ae42f643e4f8f31e2139611d1617809375b1ad884df3fb089448"}),
            serde_json::json!({"slot_id":"msig.security_council.v1","signer_id":"signer02","scheme":"ed25519","public_key_hex":"aa738a832b0d3bf371d231a0bd8502fd411f2a9723246e5d7d215e8fb0ecbb7c"}),
            serde_json::json!({"slot_id":"msig.security_council.v1","signer_id":"signer03","scheme":"ed25519","public_key_hex":"f852493e575e33647c005fe2fb43eb15963c73e4213c411e3920f25b498a6980"}),
            serde_json::json!({"slot_id":"liveops","signer_id":"signer01","scheme":"ed25519","threshold":1,"public_key_hex":"14699ee340994e43103490585a96671ec66a3280bc0f90518f29cd1866f0fa7d"}),
            serde_json::json!({"slot_id":"liveops","signer_id":"signer02","scheme":"ed25519","threshold":1,"public_key_hex":"b6517819f923b8b25989042b03e00854673b5517be88e3f568141373105ca77f"})
        ])
        .expect("encode manifest"),
    )
    .expect("write manifest");
    provision_rollback_fixture(&world_dir, &manifest_path);

    let options = CliOptions {
        world_dir,
        public_manifest: Some(manifest_path),
        finality_slot_id: "governance.finality.v1".to_string(),
        default_expected_threshold: 2,
        strict_manifest_match: true,
        require_single_failure_tolerance: true,
    };
    let report = build_audit_report(&options).expect("build report");
    let liveops = report
        .controllers
        .iter()
        .find(|row| row.slot_id == "liveops")
        .expect("liveops row");
    assert_eq!(liveops.threshold, 1);
    assert!(liveops.threshold_matches_expectation);
    assert!(liveops.single_failure_tolerant);
    assert_eq!(report.manifest_match_pass, Some(true));
    assert_eq!(report.overall_status, "ready_for_ops_drill");
    assert!(validate_audit_report(&options, &report).is_empty());
}

#[test]
fn audit_report_reads_effective_finality_registry_after_validator_activation() {
    let root = temp_dir("effective-finality");
    std::fs::create_dir_all(&root).expect("create root");
    let world_dir = root.join("world");
    let manifest_path = root.join("public_manifest.json");
    std::fs::create_dir_all(&world_dir).expect("create world dir");
    let mut world = oasis7::runtime_host::world::new();
    world
        .set_governance_execution_policy(GovernanceExecutionPolicy {
            epoch_length_ticks: 10,
            ..GovernanceExecutionPolicy::default()
        })
        .expect("set governance policy");
    world
        .set_governance_finality_signer_registry(GovernanceFinalitySignerRegistry {
            slot_id: "governance.finality.v1".to_string(),
            threshold: 2,
            threshold_bps: 0,
            signer_bindings: BTreeMap::from([
                (
                    "validator-a".to_string(),
                    "1111111111111111111111111111111111111111111111111111111111111111".to_string(),
                ),
                (
                    "validator-b".to_string(),
                    "2222222222222222222222222222222222222222222222222222222222222222".to_string(),
                ),
            ]),
            validator_stakes: BTreeMap::from([
                ("validator-a".to_string(), 100),
                ("validator-b".to_string(), 100),
            ]),
        })
        .expect("set finality registry");
    world
        .set_governance_main_token_controller_registry(GovernanceMainTokenControllerRegistry {
            genesis_controller_account_id: "msig.genesis.v1".to_string(),
            treasury_bucket_controller_slots: BTreeMap::from([(
                "ecosystem_pool".to_string(),
                "liveops".to_string(),
            )]),
            restricted_starter_claim_admin_account_ids: BTreeSet::from(["liveops".to_string()]),
            controller_signer_policies: BTreeMap::from([
                (
                    "msig.genesis.v1".to_string(),
                    GovernanceThresholdSignerPolicy {
                        threshold: 1,
                        allowed_public_keys: BTreeSet::from([
                            "6249e5a58278dbc4e629a16b5d33f6b84c39e3ceeb10e963bb9ef64ea4daac30"
                                .to_string(),
                        ]),
                    },
                ),
                (
                    "liveops".to_string(),
                    GovernanceThresholdSignerPolicy {
                        threshold: 1,
                        allowed_public_keys: BTreeSet::from([
                            "13c160fc0f516b9a5663aa00c2a5446be6467f68ce341fdd79cdb64224dffd20"
                                .to_string(),
                        ]),
                    },
                ),
            ]),
        })
        .expect("set controller registry");
    // This audit/projection fixture starts with established historical membership.
    // It does not submit a new admission or bypass the command authorization path.
    let mut snapshot = world.snapshot();
    snapshot.state.governance_validator_admissions.insert(
        "candidate-c".to_string(),
        oasis7_runtime::GovernanceValidatorAdmissionRecord {
            candidate_id: "candidate-c".to_string(),
            node_id: "validator-c".to_string(),
            finality_signer_public_key:
                "3333333333333333333333333333333333333333333333333333333333333333".to_string(),
            stake: 25,
            operator_owner: "ops.team".to_string(),
            public_manifest_hash: "manifest-c".to_string(),
            requested_at_epoch: 0,
            last_transition_tick: 0,
            approved_at_epoch: Some(0),
            activation_epoch: Some(0),
            status: oasis7_runtime::GovernanceValidatorAdmissionStatus::Active,
            revoked_at_epoch: None,
            revocation_reason: None,
        },
    );
    snapshot.state.node_identity_bindings.insert(
        "validator-c".to_string(),
        "3333333333333333333333333333333333333333333333333333333333333333".to_string(),
    );
    snapshot.state.node_main_token_account_bindings.insert(
        "validator-c".to_string(),
        oasis7_runtime::main_token_account_id_from_node_public_key(
            "3333333333333333333333333333333333333333333333333333333333333333",
        ),
    );
    let world = oasis7_runtime::World::from_snapshot(snapshot, world.journal().clone())
        .expect("restore established membership fixture");
    world.save_to_dir(&world_dir).expect("save world");
    std::fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&vec![
            serde_json::json!({"slot_id":"governance.finality.v1","signer_id":"signer01","scheme":"ed25519","threshold":2,"public_key_hex":"1111111111111111111111111111111111111111111111111111111111111111"}),
            serde_json::json!({"slot_id":"governance.finality.v1","signer_id":"signer02","scheme":"ed25519","threshold":2,"public_key_hex":"2222222222222222222222222222222222222222222222222222222222222222"}),
            serde_json::json!({"slot_id":"governance.finality.v1","signer_id":"signer03","scheme":"ed25519","threshold":2,"public_key_hex":"3333333333333333333333333333333333333333333333333333333333333333"}),
            serde_json::json!({"slot_id":"msig.genesis.v1","signer_id":"signer01","scheme":"ed25519","threshold":1,"public_key_hex":"6249e5a58278dbc4e629a16b5d33f6b84c39e3ceeb10e963bb9ef64ea4daac30"}),
            serde_json::json!({"slot_id":"liveops","signer_id":"signer01","scheme":"ed25519","threshold":1,"public_key_hex":"13c160fc0f516b9a5663aa00c2a5446be6467f68ce341fdd79cdb64224dffd20"})
        ])
        .expect("encode manifest"),
    )
    .expect("write manifest");

    let options = CliOptions {
        world_dir,
        public_manifest: Some(manifest_path),
        finality_slot_id: "governance.finality.v1".to_string(),
        default_expected_threshold: 2,
        strict_manifest_match: true,
        require_single_failure_tolerance: false,
    };
    let report = build_audit_report(&options).expect("build report");
    assert_eq!(report.finality.signer_count, 3);
    assert_eq!(report.finality.manifest_match, Some(true));
}

#[test]
fn strict_manifest_audit_requires_exact_rollback_authority_registry_match() {
    const ON_CALL: &str = "1111111111111111111111111111111111111111111111111111111111111111";
    const GOVERNANCE: &str = "2222222222222222222222222222222222222222222222222222222222222222";
    const DRIFTED_GOVERNANCE: &str =
        "3333333333333333333333333333333333333333333333333333333333333333";
    let (world_dir, manifest_path) = write_world_and_manifest();
    let mut world = oasis7::runtime_host::world::load_from_dir(world_dir.as_path())
        .expect("load fixture world");
    world
        .set_rollback_authority_registry(rollback_registry(ON_CALL, GOVERNANCE))
        .expect("set governed rollback registry");
    world
        .save_to_dir(world_dir.as_path())
        .expect("save governed world");
    let mut manifest: Vec<serde_json::Value> = serde_json::from_slice(
        std::fs::read(manifest_path.as_path())
            .expect("read manifest")
            .as_slice(),
    )
    .expect("decode manifest");
    manifest.extend([
        serde_json::json!({
            "slot_id": "ops.rollback.on_call.v1", "signer_id": "rollback-on-call-01",
            "scheme": "ed25519", "threshold": 1, "public_key_hex": ON_CALL
        }),
        serde_json::json!({
            "slot_id": "governance.rollback.v1", "signer_id": "rollback-governance-01",
            "scheme": "ed25519", "threshold": 1, "public_key_hex": GOVERNANCE
        }),
    ]);
    std::fs::write(
        manifest_path.as_path(),
        serde_json::to_vec_pretty(&manifest).expect("encode manifest"),
    )
    .expect("write manifest");
    let options = CliOptions {
        world_dir: world_dir.clone(),
        public_manifest: Some(manifest_path),
        finality_slot_id: "governance.finality.v1".to_string(),
        default_expected_threshold: 2,
        strict_manifest_match: true,
        require_single_failure_tolerance: false,
    };

    let exact = build_audit_report(&options).expect("audit exact governed registry");
    assert_eq!(exact.manifest_match_pass, Some(true));
    assert!(validate_audit_report(&options, &exact).is_empty());

    world
        .set_rollback_authority_registry(rollback_registry(ON_CALL, DRIFTED_GOVERNANCE))
        .expect("rotate world without manifest update");
    world.save_to_dir(world_dir).expect("save drifted world");
    let drift = build_audit_report(&options).expect("audit drifted governed registry");
    assert_eq!(drift.manifest_match_pass, Some(false));
    assert!(
        validate_audit_report(&options, &drift)
            .iter()
            .any(|error| error.contains("does not exactly match"))
    );
}

#[test]
fn strict_audit_emits_redacted_per_slot_rollback_rows_without_custody_material() {
    let (world_dir, manifest_path) = write_world_and_manifest();
    provision_rollback_fixture(&world_dir, &manifest_path);
    let options = CliOptions {
        world_dir,
        public_manifest: Some(manifest_path),
        finality_slot_id: "governance.finality.v1".to_string(),
        default_expected_threshold: 2,
        strict_manifest_match: true,
        require_single_failure_tolerance: false,
    };

    let report = build_audit_report(&options).expect("build strict audit report");
    assert_eq!(report.rollback_authorities.len(), 2);
    assert!(report.rollback_blockers.is_empty());
    let encoded = serde_json::to_string(&report).expect("encode audit report");
    assert!(encoded.contains("ops.rollback.on_call.v1"));
    assert!(encoded.contains("governance.rollback.v1"));
    assert!(!encoded.contains("rollback-on-call-01"));
    assert!(!encoded.contains("rollback-governance-01"));
    assert!(!encoded.contains("1111111111111111"));
    assert!(!encoded.contains("2222222222222222"));
}

#[test]
fn strict_manifest_audit_rejects_manifest_and_world_missing_both_rollback_slots() {
    let (world_dir, manifest_path) = write_world_and_manifest();
    let mut options = CliOptions {
        world_dir,
        public_manifest: Some(manifest_path),
        finality_slot_id: "governance.finality.v1".to_string(),
        default_expected_threshold: 2,
        strict_manifest_match: true,
        require_single_failure_tolerance: false,
    };

    let report = build_audit_report(&options).expect("build strict audit report");
    assert_eq!(report.manifest_match_pass, Some(false));
    assert!(
        validate_audit_report(&options, &report)
            .iter()
            .any(|error| error.contains("does not exactly match")),
        "strict audit must fail closed when both fixed rollback authority slots are absent"
    );
    options.public_manifest = None;
    options.strict_manifest_match = false;
    let report = build_audit_report(&options).expect("build non-manifest audit report");
    assert_ne!(report.overall_status, "ready_for_ops_drill");
    assert!(
        validate_audit_report(&options, &report)
            .iter()
            .any(|error| error.contains("rollback authority"))
    );
}

#[test]
fn strict_manifest_audit_accepts_the_same_entries_document_shape_as_import() {
    let (world_dir, manifest_path) = write_world_and_manifest();
    provision_rollback_fixture(&world_dir, &manifest_path);
    let entries: Vec<serde_json::Value> = serde_json::from_slice(
        std::fs::read(manifest_path.as_path())
            .expect("read array manifest")
            .as_slice(),
    )
    .expect("decode array manifest");
    std::fs::write(
        manifest_path.as_path(),
        serde_json::to_vec_pretty(&serde_json::json!({ "entries": entries }))
            .expect("encode document manifest"),
    )
    .expect("write document manifest");

    let options = CliOptions {
        world_dir,
        public_manifest: Some(manifest_path),
        finality_slot_id: "governance.finality.v1".to_string(),
        default_expected_threshold: 2,
        strict_manifest_match: true,
        require_single_failure_tolerance: false,
    };
    let report = build_audit_report(&options)
        .expect("strict audit must accept the importer-supported entries document shape");
    assert_eq!(report.manifest_match_pass, Some(true));
    assert!(validate_audit_report(&options, &report).is_empty());
}
