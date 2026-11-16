//! Work-only execution of the original player signature/serde-CBOR grammar.
//! Returned material is verified request DATA, never Runtime admission.
use super::*;
use oasis7_wasm_abi::{CanonicalCborStreamError, CapabilityCanonicalWorkV1};
use std::{
    io::{self, Write},
    mem::size_of,
};
pub struct PaidPlayerAuthRefusalV1<C> {
    pub message: String,
    pub fee: Option<C>,
}
impl<C> std::fmt::Debug for PaidPlayerAuthRefusalV1<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.message.fmt(f)
    }
}
#[derive(Debug)]
pub enum PlayerAuthWorkErrorV1<C> {
    Work(CanonicalCborStreamError),
    Protocol(PaidPlayerAuthRefusalV1<C>),
}
impl<C> std::fmt::Display for PlayerAuthWorkErrorV1<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Work(e) => e.fmt(f),
            Self::Protocol(e) => e.message.fmt(f),
        }
    }
}
pub struct PaidVerifiedPlayerAuthDataV1<C> {
    value: VerifiedPlayerAuth,
    _fees: Vec<C>,
    _slots: Option<C>,
    _outer: Option<C>,
}
impl<C> PaidVerifiedPlayerAuthDataV1<C> {
    pub fn verified(&self) -> &VerifiedPlayerAuth {
        &self.value
    }
}
impl PaidVerifiedPlayerAuthDataV1<()> {
    pub(super) fn into_legacy(self) -> VerifiedPlayerAuth {
        debug_assert!(self._fees.is_empty() && self._slots.is_none() && self._outer.is_none());
        self.value
    }
}
pub(crate) fn legacy_error(error: PlayerAuthWorkErrorV1<()>) -> String {
    match error {
        PlayerAuthWorkErrorV1::Work(e) => e.to_string(),
        PlayerAuthWorkErrorV1::Protocol(e) => e.message,
    }
}
pub(crate) struct Phase<'a, A: CapabilityCanonicalWorkV1> {
    pub(crate) source: &'a A,
    fees: Vec<A::Charge>,
    slots: Option<A::Charge>,
    outer: Option<A::Charge>,
}
impl<'a, A: CapabilityCanonicalWorkV1> Phase<'a, A> {
    pub(crate) fn new(source: &'a A) -> Result<Self, PlayerAuthWorkErrorV1<A::Charge>> {
        source
            .charge_work(1, 0)
            .map_err(PlayerAuthWorkErrorV1::Work)?;
        let outer = if source.is_metered() {
            Some(
                source
                    .reserve(
                        size_of::<Self>() + size_of::<PaidVerifiedPlayerAuthDataV1<A::Charge>>(),
                    )
                    .map_err(PlayerAuthWorkErrorV1::Work)?,
            )
        } else {
            None
        };
        Ok(Self {
            source,
            fees: Vec::new(),
            slots: None,
            outer,
        })
    }
    pub(crate) fn text(&self, text: &str) -> Result<(), PlayerAuthWorkErrorV1<A::Charge>> {
        for part in text.as_bytes().chunks(8192) {
            self.source
                .charge_work(1 + (part.len() as u64).saturating_mul(4), part.len() as u64)
                .map_err(PlayerAuthWorkErrorV1::Work)?;
        }
        Ok(())
    }
    pub(crate) fn hold(&mut self, n: usize) -> Result<(), PlayerAuthWorkErrorV1<A::Charge>> {
        self.source
            .charge_work(1, n as u64)
            .map_err(PlayerAuthWorkErrorV1::Work)?;
        if !self.source.is_metered() {
            return Ok(());
        }
        if self.fees.len() == self.fees.capacity() {
            let cap = self.fees.capacity().saturating_mul(2).max(4);
            let bytes =
                cap.checked_mul(size_of::<A::Charge>())
                    .ok_or(PlayerAuthWorkErrorV1::Work(
                        CanonicalCborStreamError::ResourceLimited("player work fee layout"),
                    ))?;
            self.source
                .charge_work(1, bytes as u64)
                .map_err(PlayerAuthWorkErrorV1::Work)?;
            let fee = self
                .source
                .reserve(bytes)
                .map_err(PlayerAuthWorkErrorV1::Work)?;
            self.fees
                .try_reserve_exact(cap - self.fees.len())
                .map_err(|_| {
                    PlayerAuthWorkErrorV1::Work(CanonicalCborStreamError::ResourceLimited(
                        "player work fee slots",
                    ))
                })?;
            if self.fees.capacity() > cap {
                return Err(PlayerAuthWorkErrorV1::Work(
                    CanonicalCborStreamError::ResourceLimited("player work fee capacity"),
                ));
            }
            self.slots = Some(fee);
        }
        self.fees.push(
            self.source
                .reserve(n)
                .map_err(PlayerAuthWorkErrorV1::Work)?,
        );
        Ok(())
    }
    pub(crate) fn fail<T>(&self, message: &str) -> Result<T, PlayerAuthWorkErrorV1<A::Charge>> {
        self.fail_with(message.len(), || message.to_owned())
    }
    pub(crate) fn fail_with<T>(
        &self,
        bound: usize,
        build: impl FnOnce() -> String,
    ) -> Result<T, PlayerAuthWorkErrorV1<A::Charge>> {
        let n = size_of::<PaidPlayerAuthRefusalV1<A::Charge>>()
            .max(size_of::<PlayerAuthWorkErrorV1<A::Charge>>())
            .checked_add(bound)
            .ok_or(PlayerAuthWorkErrorV1::Work(
                CanonicalCborStreamError::ResourceLimited("player refusal layout"),
            ))?;
        self.source
            .charge_work(1, n as u64)
            .map_err(PlayerAuthWorkErrorV1::Work)?;
        let fee = if self.source.is_metered() {
            Some(
                self.source
                    .reserve(n)
                    .map_err(PlayerAuthWorkErrorV1::Work)?,
            )
        } else {
            None
        };
        let message = build();
        if message.len() > bound {
            return Err(PlayerAuthWorkErrorV1::Work(
                CanonicalCborStreamError::ResourceLimited("player refusal bound"),
            ));
        }
        Err(PlayerAuthWorkErrorV1::Protocol(PaidPlayerAuthRefusalV1 {
            message,
            fee,
        }))
    }
    pub(crate) fn copy(&mut self, text: &str) -> Result<String, PlayerAuthWorkErrorV1<A::Charge>> {
        self.hold(size_of::<String>() + text.len())?;
        let mut bytes = Vec::with_capacity(text.len());
        for part in text.as_bytes().chunks(8192) {
            self.source
                .charge_work(1, part.len() as u64)
                .map_err(PlayerAuthWorkErrorV1::Work)?;
            bytes.extend_from_slice(part)
        }
        Ok(String::from_utf8(bytes).expect("original UTF-8"))
    }
    pub(crate) fn required(
        &mut self,
        raw: &str,
        label: &str,
    ) -> Result<String, PlayerAuthWorkErrorV1<A::Charge>> {
        self.text(raw)?;
        let value = raw.trim();
        if value.is_empty() {
            return self.fail_with(label.len() + 16, || format!("{label} is empty"));
        }
        self.copy(value)
    }
    pub(crate) fn optional_required(
        &mut self,
        raw: Option<&str>,
        label: &str,
    ) -> Result<String, PlayerAuthWorkErrorV1<A::Charge>> {
        match raw {
            Some(v) => self.required(v, label),
            None => self.fail_with(label.len() + 20, || format!("{label} is required")),
        }
    }
    pub(crate) fn decode<const N: usize>(
        &mut self,
        raw: &str,
        label: &str,
    ) -> Result<[u8; N], PlayerAuthWorkErrorV1<A::Charge>> {
        self.text(raw)?;
        self.hold(raw.len() / 2 + size_of::<Vec<u8>>() + 512)?;
        let bytes = match hex::decode(raw) {
            Ok(v) => v,
            Err(error) => {
                return self.fail_with(label.len() + 512, || {
                    format!("decode {label} failed: {error}")
                });
            }
        };
        if bytes.len() != N {
            return self.fail_with(label.len() + 128, || {
                format!(
                    "{label} length mismatch: expected {N} bytes, got {}",
                    bytes.len()
                )
            });
        }
        let mut fixed = [0u8; N];
        fixed.copy_from_slice(&bytes);
        Ok(fixed)
    }
    pub(crate) fn hex(&mut self, bytes: &[u8]) -> Result<String, PlayerAuthWorkErrorV1<A::Charge>> {
        self.hold(size_of::<String>() + bytes.len() * 2)?;
        self.source
            .charge_work(1, bytes.len() as u64)
            .map_err(PlayerAuthWorkErrorV1::Work)?;
        Ok(hex::encode(bytes))
    }
    pub(crate) fn public_key(
        &mut self,
        raw: &str,
        label: &str,
    ) -> Result<String, PlayerAuthWorkErrorV1<A::Charge>> {
        let text = self.required(raw, label)?;
        let bytes = self.decode::<32>(&text, label)?;
        self.hex(&bytes)
    }
    pub(crate) fn optional_public_key(
        &mut self,
        raw: Option<&str>,
        label: &str,
    ) -> Result<String, PlayerAuthWorkErrorV1<A::Charge>> {
        match raw {
            Some(v) => self.public_key(v, label),
            None => self.fail_with(label.len() + 20, || format!("{label} is required")),
        }
    }
    pub(crate) fn finish(
        self,
        value: VerifiedPlayerAuth,
    ) -> PaidVerifiedPlayerAuthDataV1<A::Charge> {
        PaidVerifiedPlayerAuthDataV1 {
            value,
            _fees: self.fees,
            _slots: self.slots,
            _outer: self.outer,
        }
    }
    pub(crate) fn verify_bytes(
        &mut self,
        public: &[u8; 32],
        signature: &[u8; 64],
        payload: &[u8],
    ) -> Result<(), PlayerAuthWorkErrorV1<A::Charge>> {
        self.source
            .charge_work(1, 32)
            .map_err(PlayerAuthWorkErrorV1::Work)?;
        self.hold(512)?;
        let key = match VerifyingKey::from_bytes(public) {
            Ok(v) => v,
            Err(error) => {
                return self.fail_with(512, || format!("parse auth public key failed: {error}"));
            }
        };
        self.source
            .charge_work(
                1 + (payload.len() as u64).div_ceil(128),
                payload.len() as u64 + 96,
            )
            .map_err(PlayerAuthWorkErrorV1::Work)?;
        match key.verify(payload, &Signature::from_bytes(signature)) {
            Ok(()) => Ok(()),
            Err(error) => self.fail_with(512, || format!("verify auth signature failed: {error}")),
        }
    }
    pub(crate) fn verify_signature(
        &mut self,
        key: &str,
        signature: &str,
        payload: &[u8],
    ) -> Result<(), PlayerAuthWorkErrorV1<A::Charge>> {
        let public = self.decode::<32>(key, "auth public key")?;
        let Some(signature) = signature.strip_prefix(VIEWER_PLAYER_AUTH_SIGNATURE_V1_PREFIX) else {
            return self.fail("auth signature is not awviewauth:v1");
        };
        let signature = self.decode::<64>(signature, "auth signature")?;
        self.verify_bytes(&public, &signature, payload)
    }
}
struct CborOutput<'a, A> {
    source: &'a A,
    bytes: Option<Vec<u8>>,
    length: usize,
    bound: usize,
    first: Option<CanonicalCborStreamError>,
}
impl<A: CapabilityCanonicalWorkV1> Write for CborOutput<'_, A> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        for part in bytes.chunks(8192) {
            if let Err(error) = self.source.charge_work(1, part.len() as u64) {
                self.first = Some(error);
                return Err(io::ErrorKind::Other.into());
            }
            let Some(end) = self
                .length
                .checked_add(part.len())
                .filter(|v| *v <= self.bound)
            else {
                self.first = Some(CanonicalCborStreamError::ResourceLimited(
                    "player signing image length",
                ));
                return Err(io::ErrorKind::Other.into());
            };
            if let Some(bytes) = &mut self.bytes {
                bytes.extend_from_slice(part)
            }
            self.length = end;
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
pub(crate) fn encode_in_phase<T: Serialize, A: CapabilityCanonicalWorkV1>(
    value: &T,
    phase: &mut Phase<'_, A>,
) -> Result<Vec<u8>, PlayerAuthWorkErrorV1<A::Charge>> {
    phase.hold(4096)?;
    let mut count = CborOutput {
        source: phase.source,
        bytes: None,
        length: 0,
        bound: usize::MAX,
        first: None,
    };
    let result = serde_cbor::to_writer(&mut count, value);
    if let Some(error) = count.first {
        return Err(PlayerAuthWorkErrorV1::Work(error));
    }
    if let Err(error) = result {
        return phase.fail_with(4096, || format!("encode auth payload failed: {error}"));
    }
    phase.hold(size_of::<Vec<u8>>() + count.length)?;
    let bytes = Vec::with_capacity(count.length);
    let mut output = CborOutput {
        source: phase.source,
        bytes: Some(bytes),
        length: 0,
        bound: count.length,
        first: None,
    };
    let result = serde_cbor::to_writer(&mut output, value);
    if let Some(error) = output.first {
        return Err(PlayerAuthWorkErrorV1::Work(error));
    }
    if let Err(error) = result {
        return phase.fail_with(4096, || format!("encode auth payload failed: {error}"));
    }
    if output.length != count.length {
        return Err(PlayerAuthWorkErrorV1::Work(
            CanonicalCborStreamError::Invalid("player signing image changed"),
        ));
    }
    Ok(output.bytes.expect("output buffer"))
}

fn check_proof<A: CapabilityCanonicalWorkV1>(
    phase: &mut Phase<'_, A>,
    request_player: &str,
    request_key: &str,
    proof: &PlayerAuthProof,
) -> Result<(String, String), PlayerAuthWorkErrorV1<A::Charge>> {
    let player = phase.required(&proof.player_id, "auth proof player_id")?;
    let key = phase.public_key(&proof.public_key, "auth proof public key")?;
    phase.text(request_player)?;
    phase.text(&player)?;
    if request_player != player {
        return phase.fail("auth proof player_id does not match request player_id");
    }
    phase.text(request_key)?;
    phase.text(&key)?;
    if request_key != key {
        return phase.fail("auth proof public_key does not match request public_key");
    }
    if proof.nonce == 0 {
        return phase.fail("auth nonce must be greater than zero");
    }
    Ok((player, key))
}
pub(crate) fn gameplay_payload<A: CapabilityCanonicalWorkV1>(
    request: &GameplayActionRequest,
    player: &str,
    public: &str,
    nonce: u64,
    phase: &mut Phase<'_, A>,
) -> Result<Vec<u8>, PlayerAuthWorkErrorV1<A::Charge>> {
    let action = phase.required(&request.action_id, "gameplay_action action_id")?;
    let target = phase.required(&request.target_agent_id, "gameplay_action target_agent_id")?;
    if let Some(actor) = &request.actor_agent_id {
        phase.text(actor)?;
    }
    let actor = request
        .actor_agent_id
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty());
    encode_in_phase(
        &ViewerPlayerAuthSigningEnvelope {
            version: VIEWER_PLAYER_AUTH_PAYLOAD_VERSION,
            payload: GameplayActionSigningPayload {
                operation: "gameplay_action",
                action_id: &action,
                target_agent_id: &target,
                actor_agent_id: actor,
                player_id: player,
                public_key: public,
                nonce,
            },
            actor: None,
        },
        phase,
    )
}
pub(crate) fn chat_payload<A: CapabilityCanonicalWorkV1>(
    request: &AgentChatRequest,
    player: &str,
    public: &str,
    nonce: u64,
    phase: &mut Phase<'_, A>,
) -> Result<Vec<u8>, PlayerAuthWorkErrorV1<A::Charge>> {
    let intent_seq = match request.intent_seq {
        Some(0) => return phase.fail("agent_chat intent_seq must be greater than zero"),
        v => v,
    };
    let authority = match (
        request.world_id.as_deref(),
        request.reorg_epoch,
        request.authority_scope.as_deref(),
    ) {
        (None, None, None) => None,
        (Some(world), Some(epoch), Some(scope)) => Some((
            phase.required(world, "agent_chat world_id")?,
            epoch,
            phase.required(scope, "agent_chat authority_scope")?,
        )),
        _ => return phase.fail(
            "agent_chat authority envelope must include world_id, reorg_epoch, and authority_scope",
        ),
    };
    let replaces = request
        .replaces_intent_id
        .as_deref()
        .map(|v| phase.required(v, "agent_chat replaces_intent_id"))
        .transpose()?;
    encode_in_phase(
        &ViewerPlayerAuthSigningEnvelope {
            version: VIEWER_PLAYER_AUTH_PAYLOAD_VERSION,
            payload: AgentChatSigningPayload {
                operation: "agent_chat",
                agent_id: &request.agent_id,
                player_id: player,
                public_key: public,
                nonce,
                message: &request.message,
                intent_tick: request.intent_tick,
                intent_seq,
                world_id: authority.as_ref().map(|v| v.0.as_str()),
                reorg_epoch: authority.as_ref().map(|v| v.1),
                authority_scope: authority.as_ref().map(|v| v.2.as_str()),
                replaces_intent_id: replaces.as_deref(),
                canonical_authority: request.canonical_authority.as_ref(),
            },
            actor: None,
        },
        phase,
    )
}
pub fn verify_gameplay_action_auth_proof_with_work_v1<A: CapabilityCanonicalWorkV1>(
    request: &GameplayActionRequest,
    proof: &PlayerAuthProof,
    source: &A,
) -> Result<PaidVerifiedPlayerAuthDataV1<A::Charge>, PlayerAuthWorkErrorV1<A::Charge>> {
    let mut phase = Phase::new(source)?;
    source
        .charge_work(1, 0)
        .map_err(PlayerAuthWorkErrorV1::Work)?;
    if let Err(error) = verify_proof_scheme(proof) {
        return phase.fail(&error);
    }
    let request_player = phase.required(&request.player_id, "gameplay_action player_id")?;
    let request_key =
        phase.optional_public_key(request.public_key.as_deref(), "gameplay_action public_key")?;
    let (player, key) = check_proof(&mut phase, &request_player, &request_key, proof)?;
    let payload = gameplay_payload(request, &player, &key, proof.nonce, &mut phase)?;
    phase.verify_signature(&key, &proof.signature, &payload)?;
    source
        .charge_work(0, 0)
        .map_err(PlayerAuthWorkErrorV1::Work)?;
    Ok(phase.finish(VerifiedPlayerAuth {
        player_id: player,
        public_key: key,
        nonce: proof.nonce,
        hosted_registration_nonce: None,
    }))
}
pub fn verify_agent_chat_auth_proof_with_work_v1<A: CapabilityCanonicalWorkV1>(
    request: &AgentChatRequest,
    proof: &PlayerAuthProof,
    source: &A,
) -> Result<PaidVerifiedPlayerAuthDataV1<A::Charge>, PlayerAuthWorkErrorV1<A::Charge>> {
    let mut phase = Phase::new(source)?;
    source
        .charge_work(1, 0)
        .map_err(PlayerAuthWorkErrorV1::Work)?;
    if let Err(error) = verify_proof_scheme(proof) {
        return phase.fail(&error);
    }
    let request_player =
        phase.optional_required(request.player_id.as_deref(), "agent_chat player_id")?;
    let request_key =
        phase.optional_public_key(request.public_key.as_deref(), "agent_chat public_key")?;
    let (player, key) = check_proof(&mut phase, &request_player, &request_key, proof)?;
    let payload = chat_payload(request, &player, &key, proof.nonce, &mut phase)?;
    phase.verify_signature(&key, &proof.signature, &payload)?;
    source
        .charge_work(0, 0)
        .map_err(PlayerAuthWorkErrorV1::Work)?;
    Ok(phase.finish(VerifiedPlayerAuth {
        player_id: player,
        public_key: key,
        nonce: proof.nonce,
        hosted_registration_nonce: None,
    }))
}
pub fn verify_collect_data_auth_proof_with_work_v1<A: CapabilityCanonicalWorkV1>(
    command: &crate::viewer::CollectDataCommand,
    proof: &PlayerAuthProof,
    source: &A,
) -> Result<PaidVerifiedPlayerAuthDataV1<A::Charge>, PlayerAuthWorkErrorV1<A::Charge>> {
    let mut phase = Phase::new(source)?;
    source
        .charge_work(1, 0)
        .map_err(PlayerAuthWorkErrorV1::Work)?;
    if let Err(error) = verify_proof_scheme(proof) {
        return phase.fail(&error);
    }
    let (operation, request) = collect_data::collect_data_command_parts(command);
    let request_player = phase.required(&request.player_id, "collect_data player_id")?;
    let request_key =
        phase.optional_public_key(request.public_key.as_deref(), "collect_data public_key")?;
    let (player, key) = check_proof(&mut phase, &request_player, &request_key, proof)?;
    crate::collect_data_auth::verify_authorization_in_phase_v1(
        operation,
        request.electricity_cost,
        request.data_amount,
        &player,
        &key,
        proof.nonce,
        &proof.signature,
        &mut phase,
    )?;
    source
        .charge_work(0, 0)
        .map_err(PlayerAuthWorkErrorV1::Work)?;
    Ok(phase.finish(VerifiedPlayerAuth {
        player_id: player,
        public_key: key,
        nonce: proof.nonce,
        hosted_registration_nonce: None,
    }))
}
