//! The builder installs a workspace tool, so Cargo must see every tool member.
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
    let mut checked = 0;
    for token in members.split('"').skip(1).step_by(2) {
        if !token.starts_with("tools/") {
            continue;
        }
        assert!(
            DOCKERFILE
                .lines()
                .any(|line| line == format!("COPY {token} /opt/oasis7/{token}")),
            "builder must copy workspace member {token}"
        );
        assert!(
            CONTEXT.lines().any(|line| line == format!("!{token}/**")),
            "Docker context must include workspace member {token}"
        );
        checked += 1;
    }
    assert!(checked > 0, "must validate actual tool members");
    assert!(
        CONTEXT.lines().any(|line| line == "tools/**/target/**"),
        "tool build outputs must remain excluded"
    );
}
