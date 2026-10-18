use super::*;
use crate::runtime::MajorWorldEventVisibilityPermission;

#[test]
fn runtime_live_server_config_play_interval_defaults_and_clamps() {
    let config = ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal);
    assert_eq!(config.play_step_interval, Duration::from_millis(800));
    assert_eq!(
        config.major_world_event_visibility,
        MajorWorldEventVisibilityPermission::Unknown
    );

    let clamped = config.with_play_step_interval(Duration::from_millis(10));
    assert_eq!(clamped.play_step_interval, Duration::from_millis(50));
}

#[test]
fn runtime_live_server_config_requires_explicit_major_event_audience_policy() {
    let config = ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
        .with_major_world_event_visibility(MajorWorldEventVisibilityPermission::Public);
    assert_eq!(
        config.major_world_event_visibility,
        MajorWorldEventVisibilityPermission::Public
    );

    let restricted = ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
        .with_major_world_event_visibility(MajorWorldEventVisibilityPermission::Restricted);
    assert_eq!(
        restricted.major_world_event_visibility,
        MajorWorldEventVisibilityPermission::Restricted
    );
}
