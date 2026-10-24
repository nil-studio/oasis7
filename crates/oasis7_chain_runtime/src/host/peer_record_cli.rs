use oasis7::development_node_config::{
    DevelopmentPeerRecordPublicationModeV1, DevelopmentPeerRecordReceptionModeV1,
};

use super::cli::CliOptions;

pub(super) fn validate_options(options: &CliOptions) -> Result<(), String> {
    if options.p2p_peer_record_reception != DevelopmentPeerRecordReceptionModeV1::LegacyCompatible
        && (options.formal_live_config.is_some() || options.guarded_initial_config.is_some())
    {
        return Err(
            "non-legacy --p2p-peer-record-reception requires the standard development host runtime with libp2p"
                .into(),
        );
    }
    if options.p2p_peer_record_reception == DevelopmentPeerRecordReceptionModeV1::V2Only
        && options.p2p_peer_record_publication == DevelopmentPeerRecordPublicationModeV1::V1Legacy
        && !options.initialize_peer_record_receiver
    {
        return Err(
            "V2-only peer-record reception requires durable V2 peer-record publication for normal host startup"
                .into(),
        );
    }
    if options.initialize_peer_record_publication
        && options.p2p_peer_record_publication != DevelopmentPeerRecordPublicationModeV1::V2Durable
    {
        return Err(
            "--initialize-peer-record-publication requires --p2p-peer-record-publication v2_durable"
                .into(),
        );
    }
    if options.initialize_peer_record_publication
        && (options.formal_live_config.is_some() || options.guarded_initial_config.is_some())
    {
        return Err(
            "--initialize-peer-record-publication applies only to the development host runtime"
                .into(),
        );
    }
    if options.initialize_peer_record_receiver
        && options.p2p_peer_record_reception
            == DevelopmentPeerRecordReceptionModeV1::LegacyCompatible
    {
        return Err(
            "--initialize-peer-record-receiver requires --p2p-peer-record-reception v2_durable_compatible or v2_only"
                .into(),
        );
    }
    if options.initialize_peer_record_receiver
        && (options.formal_live_config.is_some() || options.guarded_initial_config.is_some())
    {
        return Err(
            "--initialize-peer-record-receiver applies only to the development host runtime".into(),
        );
    }
    if options.initialize_peer_record_publication && options.initialize_peer_record_receiver {
        return Err(
            "peer-record publication and receiver initialization are separate one-time operations"
                .into(),
        );
    }
    Ok(())
}
