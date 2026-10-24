#[cfg(unix)]
use super::*;

#[cfg(unix)]
#[test]
fn startup_requires_durable_receiver_for_transition_and_withdrawn_publication() {
    use crate::libp2p_net::config::PeerRecordPublicationTransition;
    use crate::libp2p_net::durable_peer_record_publisher::{
        DurablePeerRecordPublisher, LocalPeerRecordPublisher,
    };
    use std::os::unix::fs::PermissionsExt;

    let physical_temp_dir =
        std::fs::canonicalize(std::env::temp_dir()).expect("resolve temporary directory");
    let root = tempfile::tempdir_in(physical_temp_dir).expect("private startup test root");
    let publication_directory = root.path().join("publication");
    std::fs::create_dir(&publication_directory).expect("create publication directory");
    std::fs::set_permissions(
        &publication_directory,
        std::fs::Permissions::from_mode(0o700),
    )
    .expect("restrict publication directory");

    let keypair = Keypair::generate_ed25519();
    let template =
        super::signed_discovery_peer_record(&keypair, Vec::new(), super::now_ms()).record;
    let mut config = Libp2pNetworkConfig {
        keypair: Some(keypair.clone()),
        peer_record: Some(template.clone()),
        peer_record_publication_transition: Some(PeerRecordPublicationTransition::Withdraw),
        ..Libp2pNetworkConfig::default()
    };
    let error = match Libp2pNetwork::try_new(config.clone()) {
        Err(error) => error,
        Ok(network) => {
            drop(network);
            panic!("a withdrawal transition without a durable publisher must fail closed")
        }
    };
    assert!(format!("{error:?}").contains("require durable V2 publisher and receiver"));

    config.peer_record_publication_directory = Some(publication_directory);
    Libp2pNetwork::initialize_peer_record_publication(&config)
        .expect("initialize durable publisher");
    let error = match Libp2pNetwork::try_new(config.clone()) {
        Err(error) => error,
        Ok(network) => {
            drop(network);
            panic!("a withdrawal transition without a durable receiver must fail closed")
        }
    };
    assert!(format!("{error:?}").contains("require durable V2 publisher and receiver"));

    let publisher = DurablePeerRecordPublisher::open(
        config
            .peer_record_publication_directory
            .as_deref()
            .expect("publication directory"),
        &keypair,
        &template,
    )
    .expect("open publisher");
    let mut publisher = LocalPeerRecordPublisher::Durable(Box::new(publisher));
    publisher
        .apply_transition(
            PeerRecordPublicationTransition::Withdraw,
            &template,
            &keypair,
        )
        .expect("persist withdrawn publication state");
    drop(publisher);

    config.peer_record_publication_transition = None;
    let error = match Libp2pNetwork::try_new(config) {
        Err(error) => error,
        Ok(network) => {
            drop(network);
            panic!("a withdrawn durable publisher cannot start with a legacy receiver")
        }
    };
    assert!(
        format!("{error:?}").contains("withdrawn peer publication requires a durable V2 receiver")
    );
}
