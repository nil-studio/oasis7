//! Routing selectors are signed as part of the actual URI, never authority.
pub(crate) enum ClientRoute {
    Challenge,
    Claim { enrollment_id: String },
    Initialize { session_id: String },
    ToolsList { session_id: String },
    Heartbeat { session_id: String },
    Finish { session_id: String },
    Abort { session_id: String },
}

pub(crate) struct ClientTarget {
    pub account_id: String,
    pub provision_id: String,
    pub route: ClientRoute,
}

fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-._~".contains(&b))
}

/// Parse only the closed POST lane. The caller separately rejects query and fragment,
/// compares the deployment origin, and retains the exact URI for Account verification.
pub(crate) fn parse(path: &str) -> Option<ClientTarget> {
    if path.len() > 2048 {
        return None;
    }
    let segments: Vec<_> = path.split('/').collect();
    let [
        "",
        "v1",
        "account-provisions",
        provision,
        "accounts",
        account,
        "client",
        tail @ ..,
    ] = segments.as_slice()
    else {
        return None;
    };
    if !identifier(provision) || !identifier(account) {
        return None;
    }
    let route = match tail {
        ["proof-challenges"] => ClientRoute::Challenge,
        ["client-enrollments", id, "claim"] if identifier(id) => ClientRoute::Claim {
            enrollment_id: (*id).into(),
        },
        ["test-sessions", id, operation] if identifier(id) => {
            let session_id = (*id).into();
            match *operation {
                "initialize" => ClientRoute::Initialize { session_id },
                "tools-list" => ClientRoute::ToolsList { session_id },
                "heartbeat" => ClientRoute::Heartbeat { session_id },
                "finish" => ClientRoute::Finish { session_id },
                "abort" => ClientRoute::Abort { session_id },
                _ => return None,
            }
        }
        _ => return None,
    };
    Some(ClientTarget {
        account_id: (*account).into(),
        provision_id: (*provision).into(),
        route,
    })
}
