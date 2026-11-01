//! Trusted H-only commands and a separate closed model client. Never Executor CLI.
use super::*;
use serde::Deserialize;
use std::{
    io::{IsTerminal, Read},
    path::Path,
};
use zeroize::{Zeroize, Zeroizing};
pub(crate) const HELP: &str = "custody-host-init|custody-host-enroll|custody-host-bind|custody-host-claim --vault <private-root>\ncustody-host-serve|custody-host-profile --vault <private-root> --socket <preprovisioned-public-socket>\nNative H-only custody-host-native-serve|custody-host-native-profile use the same private vault/socket options; N-only custody-native-reserve-call --socket <approved-native-socket> accepts a public signed query. It does not enable provider dispatch.\nTrusted commands require private bounded JSON stdin with wrapping_key; no credentials or keys in argv/env/stdout. Model calls only custody-control-call --socket <approved-custody-socket> with closed Control JSON. This component does not establish deployment isolation acceptance or enable business actions.";
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Init {
    wrapping_key: String,
    identity: IdentityBinding,
    request_id: String,
}
impl Drop for Init {
    fn drop(&mut self) {
        self.wrapping_key.zeroize();
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Unlock {
    wrapping_key: String,
}
impl Drop for Unlock {
    fn drop(&mut self) {
        self.wrapping_key.zeroize();
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Bind {
    wrapping_key: String,
    binding: SessionBinding,
}
impl Drop for Bind {
    fn drop(&mut self) {
        self.wrapping_key.zeroize();
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Claim {
    wrapping_key: String,
    request: oasis7_custody_core::client_enrollment::ClientCredentialClaimRequestV1,
}
impl Drop for Claim {
    fn drop(&mut self) {
        self.wrapping_key.zeroize();
    }
}
fn input<T: serde::de::DeserializeOwned>() -> Result<T, String> {
    if std::io::stdin().is_terminal() {
        return Err("action_required: trusted private JSON stdin required".into());
    }
    let mut bytes = Zeroizing::new(Vec::new());
    std::io::stdin()
        .take(64 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| denied())?;
    if bytes.len() > 64 * 1024 {
        return Err(denied());
    }
    serde_json::from_slice(&bytes).map_err(|_| denied())
}
pub(crate) fn model_call(socket: &Path) -> Result<(), String> {
    let mut bytes = Vec::new();
    std::io::stdin()
        .take(32769)
        .read_to_end(&mut bytes)
        .map_err(|_| denied())?;
    if bytes.len() > 32768 {
        return Err(denied());
    }
    let request: control::ModelRequest = serde_json::from_slice(&bytes).map_err(|_| denied())?;
    crate::device_client::print(&ipc::call(socket, request)?)
}
/// N-only fixed client; stdin is a public signed query, never Host unlock data.
pub(crate) fn native_reserve_call(socket: &Path) -> Result<(), String> {
    let mut bytes = Vec::new();
    std::io::stdin()
        .take(32769)
        .read_to_end(&mut bytes)
        .map_err(|_| denied())?;
    if bytes.len() > 32768 {
        return Err(denied());
    }
    let query = serde_json::from_slice(&bytes).map_err(|_| denied())?;
    crate::device_client::print(&ipc::native::call(socket, query)?)
}
pub(crate) fn run(args: &[String]) -> Result<(), String> {
    if args.len() != 3 && args.len() != 5 {
        return Err(HELP.into());
    }
    if args[1] != "--vault" || args.len() == 5 && args[3] != "--socket" {
        return Err(HELP.into());
    }
    let root = Path::new(&args[2]);
    match args[0].as_str() {
        "custody-host-init" if args.len() == 3 => {
            let request: Init = input()?;
            let v = vault::Vault::initialize(
                root,
                vault::secret32(&request.wrapping_key)?,
                request.identity.clone(),
                request.request_id.clone(),
            )?;
            let descriptor = v.read(|s| Ok(s.identity.descriptor.clone()))?;
            crate::device_client::print(
                &serde_json::json!({"api_version":1,"state":"host_identity_initialized","descriptor":descriptor,"deployment_accepted":false}),
            )
        }
        "custody-host-bind" if args.len() == 3 => {
            let request: Bind = input()?;
            let v = vault::Vault::open(root, vault::secret32(&request.wrapping_key)?)?;
            v.read(|s| ipc::verify_runtime_pins(&s.identity))?;
            v.bind_session(request.binding.clone())?;
            crate::device_client::print(
                &serde_json::json!({"api_version":1,"state":"public_session_binding_retained","authority_granted":false}),
            )
        }
        "custody-host-claim" if args.len() == 3 => {
            let request: Claim = input()?;
            let host = Host::open(vault::Vault::open(
                root,
                vault::secret32(&request.wrapping_key)?,
            )?)?;
            let fresh = host.claim(request.request.clone())?;
            crate::device_client::print(
                &serde_json::json!({"api_version":1,"state":if fresh{"credential_reply_retained"}else{"unknown_token_operator_replacement_required"}}),
            )
        }
        "custody-host-enroll" if args.len() == 3 => {
            let request: Unlock = input()?;
            let host = Host::open(vault::Vault::open(
                root,
                vault::secret32(&request.wrapping_key)?,
            )?)?;
            let proved = host.enroll()?;
            let public = host.vault.read(|s| {
                Ok((
                    s.identity.descriptor.clone(),
                    s.enrollment.response.clone().ok_or_else(denied)?,
                ))
            })?;
            crate::device_client::print(
                &serde_json::json!({"api_version":1,"state":"enrollment_proved_metadata_only","enrollment_id":proved.enrollment_id,
                "client_key_id":public.1.key_id,"descriptor_digest":public.1.descriptor_digest,"descriptor":public.0,"authority_granted":false}),
            )
        }
        "custody-host-serve"
        | "custody-host-profile"
        | "custody-host-native-serve"
        | "custody-host-native-profile"
            if args.len() == 5 =>
        {
            let request: Unlock = input()?;
            let host = Host::open(vault::Vault::open(
                root,
                vault::secret32(&request.wrapping_key)?,
            )?)?;
            let socket = Path::new(&args[4]);
            match args[0].as_str() {
                "custody-host-serve" => ipc::serve(host, socket),
                "custody-host-native-serve" => ipc::native::serve(host, socket),
                "custody-host-native-profile" => {
                    crate::device_client::print(&ipc::native::binding(&host, socket)?)
                }
                _ => crate::device_client::print(&ipc::binding(&host, socket)?),
            }
        }
        _ => Err(HELP.into()),
    }
}
