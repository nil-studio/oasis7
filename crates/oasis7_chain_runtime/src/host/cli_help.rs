use super::{
    CliOptions,
    cli_contract::{OFFLINE_COMMANDS, OPTIONS, OptionKind},
};

pub(crate) fn render_help() -> String {
    let defaults = CliOptions::default();
    let mut text = String::from(
        "Usage: oasis7_chain_runtime [options]\n\nStarts standalone chain/node runtime with status HTTP endpoints.\n\nUtility commands (before local Node/services):\n",
    );
    for command in OFFLINE_COMMANDS {
        text.push_str(&format!("  {} {}\n", command.name, command.usage));
    }
    text.push_str(
        "  initialize-preview-world --root <fresh-root> --world-id <id> --node-id <id> --key-dir <secure-config> --genesis-nonce <64-hex> --status-bind <loopback:port>\n    Creates a fresh local preview world from genuine Runtime and NodeRuntime commits; it does not activate formal finality, provider funding, or test grants.\n",
    );
    text.push_str(
        "  world init prepare --config <private.json> --request <private.cbor>\n  world init quote --config <private.json> --request <private.cbor> --certificate <signed-certificate.json>\n  world init activate --config <private.json> --request <private.cbor> --certificate <signed-certificate.json> --writer-key <private.key> --primary <loopback:port> --replica <loopback:port> --timeout-ms <1..60000> --output <private.cbor>\n    Config supplies the independent issuer public-key pin; the certificate contains only the externally signed approval. Uses an explicit staged World/Journal/Node boundary and profile v2.\n    Never creates a World, identity, issuer trust pin, directories, or keys.\n",
    );
    text.push_str("\nOptions (scalars occur once; opposite switches conflict; repeatable lists retain entries):\n  Omitted relay/probe hints use live detection; explicit hints override it.\n  Escape a value beginning with --: --node-id -- --literal.\n");
    text.push_str(&format!(
        "  Initial hint fields: relay={}, probe={}.\n",
        defaults.p2p_detected_relay_available, defaults.p2p_detected_probe_stable
    ));
    for spec in OPTIONS {
        text.push_str(&format!(
            "  {} {}\n    {}",
            spec.name, spec.value, spec.description
        ));
        if spec.kind == OptionKind::List {
            text.push_str(" (repeatable)");
        }
        if let Some(default) = spec.default {
            text.push_str(&format!(" (default: {})", default(&defaults)));
        }
        if let Some(alias) = spec.alias {
            text.push_str(&format!(
                " (deprecated alias: {alias}; fetch permission only)"
            ));
        }
        text.push('\n');
    }
    text.push_str("  -h, --help    show help\n");
    text
}

pub fn print_help() {
    print!("{}", render_help());
}
