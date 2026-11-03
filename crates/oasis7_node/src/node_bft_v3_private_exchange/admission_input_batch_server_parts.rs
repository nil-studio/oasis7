use super::*;
use serde::Serialize;
#[derive(Serialize)]
struct BorrowedBytes<'a>(#[serde(with = "raw_bytes")] &'a [u8]);
mod raw_bytes {
    pub fn serialize<S: serde::Serializer>(
        bytes: &&[u8],
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.serialize_bytes(bytes)
    }
}
#[derive(Serialize)]
struct BorrowedPart<'a> {
    index: u32,
    bytes: BorrowedBytes<'a>,
}
#[derive(Serialize)]
struct BorrowedResponse<'a> {
    domain: &'static str,
    request_digest: String,
    reference: &'a InputRef,
    offset_bytes: u64,
    parts: Vec<BorrowedPart<'a>>,
}
impl Server {
    pub(super) fn material(
        &self,
        transport: &AuthenticatedPeerRequestContext,
        request: &BatchRequestV2,
        subject: &VerifiedBatchSubjectV2,
        base: NodeBftAdmissionInputReadViewV1<'_>,
        mut deadline: Instant,
    ) -> Result<Vec<u8>, String> {
        let read = &request.read;
        let ranges = read.ranges()?;
        let mut original: Option<Arc<NodeBftAdmissionInputBytesV1>> = None;
        // The sole retained payload is the original shared buffered immutable
        // encoding. Parts retain coordinates, never another payload Vec.
        for range in &ranges {
            if !subject.authorizes_part(range.index) {
                return Err("admission signed batch does not cover part".into());
            }
            let part_deadline = deadline;
            let mut calls = 0usize;
            let mut inspect = |until: Instant| {
                calls += 1;
                if calls != 1 {
                    return Err("admission part callback repeated".into());
                }
                deadline = deadline.min(until);
                transport
                    .request_context()
                    .check_cancelled(|| "admission part cancelled".to_owned())?;
                if Instant::now() >= deadline {
                    return Err("admission part expired".into());
                }
                let current = self.private.admission_original(
                    &read.requester_peer_id,
                    &read.nonce,
                    &read.reference,
                    self.source.as_ref(),
                    deadline,
                )?;
                self.check_original(&current, read)?;
                if original
                    .as_ref()
                    .is_some_and(|first| !Arc::ptr_eq(&first.bytes, &current.bytes))
                {
                    return Err("admission original owner changed between parts".into());
                }
                let start = usize::try_from(range.offset).map_err(|_| "admission part offset")?;
                let end = start
                    .checked_add(range.length as usize)
                    .ok_or("admission part end")?;
                if current.bytes.get(start..end).is_none() {
                    return Err("admission part missing".into());
                }
                original.get_or_insert(current);
                Ok(())
            };
            self.fresh(base, part_deadline, |current| {
                self.gate.authorize_part(
                    transport,
                    &NodeBftAdmissionInputPartScopeV2 {
                        view: current,
                        subject,
                        part_index: range.index,
                    },
                    &mut inspect,
                )
            })?;
            drop(inspect);
            if calls != 1 {
                return Err("admission gate skipped part".into());
            }
        }
        // Manifest is authorized as its own signed operation, not a fabricated
        // part. It still verifies current original existence and buffer owner.
        if original.is_none() {
            let current = self.private.admission_original(
                &read.requester_peer_id,
                &read.nonce,
                &read.reference,
                self.source.as_ref(),
                deadline,
            )?;
            self.check_original(&current, read)?;
            original = Some(current);
        }
        transport
            .restrict_reply_until(deadline)
            .map_err(|_| "admission part deadline")?;
        let original = original.ok_or("admission original unavailable")?;
        let encode_final = || {
            if Instant::now() >= deadline
                || !self.source.revalidate_original(&original, deadline)?
            {
                return Err("admission original absent/changed at final release".into());
            }
            self.check_original(&original, read)?;
            let mut parts = Vec::with_capacity(ranges.len());
            for range in &ranges {
                let start = usize::try_from(range.offset).map_err(|_| "admission final offset")?;
                let end = start
                    .checked_add(range.length as usize)
                    .ok_or("admission final end")?;
                parts.push(BorrowedPart {
                    index: range.index,
                    bytes: BorrowedBytes(
                        original
                            .bytes
                            .get(start..end)
                            .ok_or("admission final part missing")?,
                    ),
                });
            }
            encode_frame(&BorrowedResponse {
                domain: DOMAIN_V2,
                request_digest: request.digest()?,
                reference: &read.reference,
                offset_bytes: match read.operation {
                    BatchOperationV2::Manifest => 0,
                    BatchOperationV2::Batch { offset_bytes, .. } => offset_bytes,
                },
                parts,
            })
        };
        match (&self.view, base) {
            (ViewOwner::Provider(service), NodeBftAdmissionInputReadViewV1::Provider(old)) => {
                service
                    .with_charged_original_input_response(deadline, |current| {
                        if !super::flow::same_provider(old, current) {
                            return Err(BftSignerErrorV3(
                                "admission base changed before batch charge".into(),
                            ));
                        }
                        encode_final().map_err(BftSignerErrorV3)
                    })
                    .map_err(|e| e.to_string())
            }
            (ViewOwner::Node(_), NodeBftAdmissionInputReadViewV1::Node(_)) => {
                self.fresh(base, deadline, |_| encode_final())
            }
            _ => Err("admission final owner differs".into()),
        }
    }
    fn check_original(
        &self,
        original: &NodeBftAdmissionInputBytesV1,
        read: &BatchReadV2,
    ) -> Result<(), String> {
        if original.reference != read.reference
            || !original.bytes.belongs_to(&self.private.buffers())
        {
            return Err("admission original exact reference/buffer differs".into());
        }
        Ok(())
    }
}
