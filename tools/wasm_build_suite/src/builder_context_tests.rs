//! Cargo must parse every original tool manifest; only the actual producer
//! contributes its complete source tree to the builder image.
const WORKSPACE: &str = include_str!("../../../Cargo.toml");
const DOCKERFILE: &str = include_str!("../../../docker/wasm-builder/Dockerfile");
const CONTEXT: &str = include_str!("../../../.dockerignore");

#[test]
fn builder_context_contains_every_workspace_tool() {
    let members = WORKSPACE
        .split("members = [")
        .nth(1)
        .expect("workspace members")
        .split(']')
        .next()
        .expect("members list");
    let context: Vec<_> = CONTEXT.lines().collect();
    let excluded = context
        .iter()
        .position(|line| *line == "tools/*/*")
        .expect("non-producer tool contents must be excluded");
    let manifests = context
        .iter()
        .position(|line| *line == "!tools/*/Cargo.toml")
        .expect("all actual tool manifests must be included");
    assert!(excluded < manifests, "manifest exception follows exclusion");
    assert!(!context.contains(&"!tools/**"));
    let mut checked = 0;
    for token in members.split('"').skip(1).step_by(2) {
        if !token.starts_with("tools/") {
            continue;
        }
        assert!(
            DOCKERFILE
                .lines()
                .any(|line| line == format!("COPY --chmod=0755 {token} /opt/oasis7/{token}")),
            "builder must copy workspace member {token} with readable permissions"
        );
        if token == "tools/wasm_build_suite" {
            let sources = context
                .iter()
                .position(|line| *line == format!("!{token}/**"))
                .expect("actual producer's complete source tree must be included");
            assert!(sources > excluded, "producer exception follows exclusion");
        } else {
            assert!(
                !context.iter().any(|line| {
                    line.starts_with(&format!("!{token}/"))
                        && **line != format!("!{token}/Cargo.toml")
                }),
                "non-producer {token} must remain manifest-only"
            );
        }
        checked += 1;
    }
    assert!(checked > 0, "must validate actual tool members");
    for excluded_target in ["tools/**/target", "tools/**/target/**"] {
        let position = context
            .iter()
            .position(|line| *line == excluded_target)
            .expect("tool build outputs must remain excluded");
        assert!(position > manifests);
    }
}
