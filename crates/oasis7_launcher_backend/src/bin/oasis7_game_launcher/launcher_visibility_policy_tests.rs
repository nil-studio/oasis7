use std::path::Path;

use super::{CliOptions, build_oasis7_viewer_live_command, parse_options};
use oasis7_runtime::MajorWorldEventVisibilityPermission;

#[test]
fn parse_options_defaults_major_world_event_visibility_to_unknown() {
    let options = parse_options(std::iter::empty()).expect("parse should succeed");
    assert_eq!(
        options.major_world_event_visibility,
        MajorWorldEventVisibilityPermission::Unknown
    );
}

#[test]
fn parse_options_accepts_major_world_event_visibility() {
    for (raw, expected) in [
        ("unknown", MajorWorldEventVisibilityPermission::Unknown),
        ("public", MajorWorldEventVisibilityPermission::Public),
        (
            "restricted",
            MajorWorldEventVisibilityPermission::Restricted,
        ),
        ("denied", MajorWorldEventVisibilityPermission::Denied),
    ] {
        let options = parse_options(["--major-world-event-visibility", raw].into_iter())
            .expect("visibility policy should parse");
        assert_eq!(options.major_world_event_visibility, expected);
    }
}

#[test]
fn parse_options_rejects_invalid_major_world_event_visibility() {
    let err = parse_options(["--major-world-event-visibility", "internal"].into_iter())
        .expect_err("invalid visibility policy should fail");
    assert!(err.contains("unknown|public|restricted|denied"));
}

#[test]
fn build_viewer_live_command_wires_major_world_event_visibility() {
    if !crate::hosted_test_env::with_issuer(None) {
        return;
    }
    for (visibility, expected) in [
        (
            MajorWorldEventVisibilityPermission::Restricted,
            "restricted",
        ),
        (MajorWorldEventVisibilityPermission::Unknown, "unknown"),
    ] {
        let options = CliOptions {
            major_world_event_visibility: visibility,
            ..CliOptions::default()
        };
        let command =
            build_oasis7_viewer_live_command(Path::new("/bin/echo"), &options, false, false);
        let args: Vec<String> = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        let visibility_index = args
            .iter()
            .position(|arg| arg == "--major-world-event-visibility")
            .expect("visibility flag should be forwarded");
        assert_eq!(
            args.get(visibility_index + 1).map(String::as_str),
            Some(expected)
        );
    }
}
