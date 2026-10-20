use super::*;
use crate::parse_options;

#[test]
fn clear_reachability_and_omitted_hints_follow_changing_live_signals() {
    let options = parse_options(
        [
            "--node-role",
            "observer",
            "--p2p-clear-detected-reachability",
        ]
        .into_iter(),
    )
    .unwrap();
    assert!(!options.p2p_detected_reachability_explicit);
    let public = Libp2pReachabilitySnapshot {
        autonat_status: LiveAutoNatStatus::Public,
        relay_reservation_active: true,
        ..Default::default()
    };
    let private = Libp2pReachabilitySnapshot {
        autonat_status: LiveAutoNatStatus::Private,
        ..Default::default()
    };
    let (_, first) =
        build_live_node_network_policy_recommendation(&options, Some(&public)).unwrap();
    let (_, next) =
        build_live_node_network_policy_recommendation(&options, Some(&private)).unwrap();
    assert_eq!(
        first.observed_reachability,
        Some(PeerReachabilityClass::Public)
    );
    assert_eq!(
        next.observed_reachability,
        Some(PeerReachabilityClass::Private)
    );
    assert!(first.relay_available);
    assert!(!next.relay_available);
    let (_, unknown) = build_live_node_network_policy_recommendation(
        &options,
        Some(&Libp2pReachabilitySnapshot::default()),
    )
    .unwrap();
    assert!(first.probe_stable);
    assert!(!unknown.probe_stable);
    assert_eq!(unknown.observed_reachability, None);
}

#[test]
fn explicit_true_and_false_hints_survive_live_signal_changes() {
    for (flags, expected) in [
        (
            [
                "--p2p-detected-relay-available",
                "--p2p-detected-probe-stable",
            ],
            true,
        ),
        (
            [
                "--p2p-detected-relay-unavailable",
                "--p2p-detected-probe-unstable",
            ],
            false,
        ),
    ] {
        let options = parse_options(flags.into_iter()).unwrap();
        assert!(options.p2p_detected_relay_available_explicit);
        assert!(options.p2p_detected_probe_stable_explicit);
        for active in [false, true, false] {
            let live = Libp2pReachabilitySnapshot {
                relay_reservation_active: active,
                active_relay_path_count: usize::from(active),
                ..Default::default()
            };
            let (_, detection) =
                build_live_node_network_policy_recommendation(&options, Some(&live)).unwrap();
            assert_eq!(detection.relay_available, expected);
            assert_eq!(detection.probe_stable, expected);
        }
    }
}
