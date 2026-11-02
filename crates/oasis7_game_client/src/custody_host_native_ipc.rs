//! Local N socket only; M Control and ordinary Executor broker are separate.
use super::super::native_model::{NativePeer, PendingReply};
use super::*;
use oasis7_custody_core::{
    native_model_proxy_query_v2::SignedNativeModelReservationQueryV2,
    provisioning::ModelBudgetApprovalContractV1,
};
const PROFILE: &str = "native-model-custody-v2";
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeRequest {
    api_version: u8,
    resource_id: String,
    query: SignedNativeModelReservationQueryV2,
}
#[derive(Serialize, Deserialize)]
#[serde(
    tag = "status",
    content = "result",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum NativeReply {
    Pending(PendingReply),
    Unavailable,
}
pub(crate) fn binding(host: &Host, socket: &Path) -> Result<PublicBinding, String> {
    host.vault.read(|s| {
        let session = s.session.as_ref().ok_or_else(denied)?;
        let HostSessionPlan::V2(plan) = &session.binding.plan else {
            return Err(denied());
        };
        let budget = plan.model_budget.as_ref().ok_or_else(denied)?;
        let NativePlacement::Local { .. } = &s.identity.isolation.native else {
            return Err(denied());
        };
        if session.closing
            || vault::now()? >= plan.write_deadline.0
            || budget.approval_contract != ModelBudgetApprovalContractV1::NativeCustodyModelProxyV2
        {
            return Err(denied());
        }
        session.credential(SessionPurposeV1::Execution)?;
        verify_runtime_pins(&s.identity)?;
        Ok(PublicBinding {
            api_version: 1,
            transport_profile: PROFILE.into(),
            socket_path: socket.to_str().ok_or_else(denied)?.into(),
            identity_digest: s.identity.digest()?,
            descriptor_digest: s.identity.descriptor.digest().map_err(|_| denied())?,
            session_digest: session.binding.digest()?,
            resource_id: session.binding.resource_id.clone(),
            restricted_runtime_identity: s.identity.descriptor.restricted_runtime_identity.clone(),
            isolation: s.identity.isolation.clone(),
        })
    })
}
pub(crate) fn call(
    socket: &Path,
    query: SignedNativeModelReservationQueryV2,
) -> Result<PendingReply, String> {
    let b = read_binding_for(socket, PROFILE)?;
    super::super::registry::check_native(&b)?;
    let NativePlacement::Local { uid } = &b.isolation.native else {
        return Err(denied());
    };
    if vault::actual_uid() != *uid {
        return Err(denied());
    }
    let parent = socket.parent().ok_or_else(denied)?;
    let held = directory(parent, b.isolation.host_uid)?;
    verify_parent(parent, &held)?;
    let before = socket_metadata(socket, b.isolation.host_uid)?;
    let mut stream = UnixStream::connect(socket).map_err(|_| denied())?;
    if peer_uid(&stream)? != b.isolation.host_uid {
        return Err(denied());
    }
    let after = socket_metadata(socket, b.isolation.host_uid)?;
    if before.dev() != after.dev() || before.ino() != after.ino() {
        return Err(denied());
    }
    verify_parent(parent, &held)?;
    super::super::registry::check_native(&b)?;
    stream
        .set_read_timeout(Some(Duration::from_secs(15)))
        .map_err(|_| denied())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(3)))
        .map_err(|_| denied())?;
    write_frame(
        &mut stream,
        &NativeRequest {
            api_version: 2,
            resource_id: b.resource_id,
            query: query.clone(),
        },
    )?;
    match read_frame::<NativeReply>(&mut stream)? {
        NativeReply::Pending(reply) => {
            reply.validate(&query)?;
            Ok(reply)
        }
        NativeReply::Unavailable => Err(denied()),
    }
}
struct Retirement<'a> {
    held: &'a Dir,
    socket: std::path::PathBuf,
    own: std::fs::Metadata,
    side: Option<cap_std::fs::Metadata>,
    armed: bool,
}
impl Retirement<'_> {
    fn retire(&mut self) -> Result<(), String> {
        let mut files = vec![(self.socket.clone(), self.own.dev(), self.own.ino())];
        if let Some(side) = &self.side {
            files.push((sidecar(&self.socket)?, side.dev(), side.ino()));
        }
        for (path, dev, ino) in files {
            let name = path.file_name().ok_or_else(denied)?;
            if let Ok(now) = self.held.symlink_metadata(name)
                && now.dev() == dev
                && now.ino() == ino
            {
                self.held.remove_file(name).map_err(|_| denied())?;
            }
        }
        self.armed = false;
        Ok(())
    }
}
impl Drop for Retirement<'_> {
    fn drop(&mut self) {
        if self.armed {
            let _ = self.retire();
        }
    }
}

pub(crate) fn serve(host: Host, socket: &Path) -> Result<(), String> {
    let b = binding(&host, socket)?;
    super::super::registry::check_native(&b)?;
    if vault::actual_uid() != b.isolation.host_uid {
        return Err(denied());
    }
    let parent = socket.parent().ok_or_else(denied)?;
    let held = directory(parent, b.isolation.host_uid)?;
    verify_parent(parent, &held)?;
    let listener = UnixListener::bind(socket).map_err(|_| denied())?;
    std::fs::set_permissions(socket, std::fs::Permissions::from_mode(0o660))
        .map_err(|_| denied())?;
    let own = socket_metadata(socket, b.isolation.host_uid)?;
    let mut retirement = Retirement {
        held: &held,
        socket: socket.into(),
        own,
        side: None,
        armed: true,
    };
    publish_binding(&held, socket, &b)?;
    retirement.side = Some(
        held.symlink_metadata(sidecar(socket)?.file_name().ok_or_else(denied)?)
            .map_err(|_| denied())?,
    );
    let result = (|| {
        verify_parent(parent, &held)?;
        listener.set_nonblocking(true).map_err(|_| denied())?;
        loop {
            super::super::registry::check_native(&b)?;
            if binding(&host, socket).is_err() {
                break;
            }
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let peer = match host
                        .vault
                        .read(|s| NativePeer::observe(&stream, &s.identity, &b))
                    {
                        Ok(p) => p,
                        Err(_) => continue,
                    };
                    stream
                        .set_read_timeout(Some(Duration::from_secs(3)))
                        .map_err(|_| denied())?;
                    stream
                        .set_write_timeout(Some(Duration::from_secs(3)))
                        .map_err(|_| denied())?;
                    let reply = match read_frame::<NativeRequest>(&mut stream) {
                        Ok(q) if q.api_version == 2 && q.resource_id == b.resource_id => {
                            super::super::registry::check_native(&b)?;
                            host.reserve_native_model(peer, &q.query)
                                .map(NativeReply::Pending)
                                .unwrap_or(NativeReply::Unavailable)
                        }
                        _ => NativeReply::Unavailable,
                    };
                    let _ = write_frame(&mut stream, &reply);
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(100))
                }
                Err(_) => return Err(denied()),
            }
        }
        Ok(())
    })();
    drop(listener);
    result.and(retirement.retire())
}
