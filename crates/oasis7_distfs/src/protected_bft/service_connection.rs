use std::io::{Read, Write};

use super::super::binding::{BftSignerError, invalid};
use super::super::validation::BFT_SIGNER_MAX_FRAME_BYTES_V1;
use super::super::wire::digest_hex;
use super::{BftSignerServiceV1, BftSignerSessionV1};

impl BftSignerServiceV1 {
    /// Serves requests until the peer closes the stream. Callers should set a
    /// read/write timeout and run this only on a protected provider endpoint.
    pub fn serve_connection<S: Read + Write>(&self, stream: &mut S) -> Result<(), BftSignerError> {
        let mut session = BftSignerSessionV1::default();
        let result = self.serve_session_loop(stream, &mut session);
        let release = self.release_session_lease(&mut session);
        match (result, release) {
            (Ok(()), Ok(())) => Ok(()),
            (Err(error), Ok(())) => Err(error),
            (_, Err(error)) => Err(error),
        }
    }

    fn serve_session_loop<S: Read + Write>(
        &self,
        stream: &mut S,
        session: &mut BftSignerSessionV1,
    ) -> Result<(), BftSignerError> {
        loop {
            let mut length = [0u8; 4];
            match stream.read(&mut length[..1]) {
                Ok(0) => return Ok(()),
                Ok(1) => {}
                Ok(_) => return Err(invalid("BFT signer stream read exceeded one-byte header")),
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(invalid(format!("BFT signer request header: {error}"))),
            }
            stream.read_exact(&mut length[1..]).map_err(|error| {
                invalid(format!("truncated BFT signer request header: {error}"))
            })?;
            let frame_len = usize::try_from(u32::from_be_bytes(length))
                .map_err(|_| invalid("BFT signer frame length conversion"))?;
            if frame_len == 0 || frame_len > BFT_SIGNER_MAX_FRAME_BYTES_V1 {
                return Err(invalid("BFT signer frame length bound"));
            }
            let mut frame = Vec::new();
            frame
                .try_reserve_exact(frame_len)
                .map_err(|_| invalid("BFT signer frame allocation failed"))?;
            frame.resize(frame_len, 0);
            stream
                .read_exact(&mut frame)
                .map_err(|error| invalid(format!("BFT signer request body: {error}")))?;
            let response = self.handle_session_frame(session, &frame)?;
            let response_len = u32::try_from(response.len())
                .map_err(|_| invalid("BFT signer response exceeds TCP frame range"))?;
            stream
                .write_all(&response_len.to_be_bytes())
                .and_then(|_| stream.write_all(&response))
                .map_err(|error| invalid(format!("BFT signer response write: {error}")))?;
        }
    }

    fn release_session_lease(
        &self,
        session: &mut BftSignerSessionV1,
    ) -> Result<(), BftSignerError> {
        let Some(lease) = session.lease.as_ref() else {
            return Ok(());
        };
        let token_digest = match digest_hex("oasis7.bft.protected-signer.v1.lease", &lease.token) {
            Ok(digest) => digest,
            Err(error) => {
                if let Ok(mut runtime) = self.runtime.lock() {
                    runtime.poisoned = true;
                }
                return Err(invalid(format!(
                    "BFT signer lease release is uncertain; service poisoned: {error}"
                )));
            }
        };
        let mut runtime = self
            .runtime
            .lock()
            .map_err(|_| invalid("BFT signer service lock poisoned"))?;
        if runtime.poisoned {
            return Err(invalid(
                "BFT signer service is poisoned after uncertain release",
            ));
        }
        let released_snapshot = runtime
            .backend
            .release_lease(lease.generation, &token_digest);
        let release = released_snapshot.and_then(|snapshot| {
            let schedule = self.validate_backend_snapshot(&snapshot)?;
            if snapshot.protected_time_floor_ms < runtime.observed_clock_floor_ms {
                return Err(invalid("BFT signer backend clock floor rollback"));
            }
            runtime.observed_clock_floor_ms = snapshot.protected_time_floor_ms;
            let next_generation = lease
                .generation
                .checked_add(1)
                .ok_or_else(|| invalid("BFT signer lease generation overflow"))?;
            if snapshot.lease_generation == next_generation
                && snapshot.lease_token_digest.is_empty()
            {
                let previous = runtime
                    .verified
                    .as_ref()
                    .ok_or_else(|| invalid("BFT signer verified state cache is unavailable"))?;
                let next = super::VerifiedBftSignerStateV1 {
                    state: snapshot.state.clone(),
                    schedule,
                };
                super::snapshot_validation::validate_monotonic_state_extension(
                    &self.binding,
                    previous,
                    &next,
                )?;
                runtime.verified = Some(next);
                clear_matching_active_lease(&mut runtime, lease.generation, &token_digest);
                return Ok(());
            }
            if snapshot.lease_generation > lease.generation
                && snapshot.lease_token_digest != token_digest
            {
                clear_matching_active_lease(&mut runtime, lease.generation, &token_digest);
                return Ok(());
            }
            Err(invalid(
                "BFT signer backend did not durably fence released lease",
            ))
        });
        if let Err(error) = release {
            runtime.poisoned = true;
            return Err(invalid(format!(
                "BFT signer lease release is uncertain; service poisoned: {error}"
            )));
        }
        session.lease = None;
        Ok(())
    }
}

fn clear_matching_active_lease(
    runtime: &mut super::BftSignerServiceRuntimeV1,
    generation: u64,
    token_digest: &str,
) {
    if runtime.active_lease.as_ref().is_some_and(|active| {
        active.generation == generation && active.token_digest == token_digest
    }) {
        runtime.active_lease = None;
    }
}
