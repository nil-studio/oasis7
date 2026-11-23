use super::*;
// One authenticated traversal feeds independently buffered provisional destinations.
pub(super) struct MirroredPages<'a, A, B> {
    pub accepted: &'a mut A,
    pub cancelled: &'a mut B,
    pub memory: &'a MemoryBudget,
}
impl<A: ValueNodeWriter, B: ValueNodeWriter> ValueNodeWriter for MirroredPages<'_, A, B> {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        let charge = self.memory.reserve(bytes.len(), MemoryClass::Ordinary)?;
        let copy = charge.retain(bytes.to_vec());
        self.accepted.put_node(id, bytes)?;
        self.cancelled.put_node(id, copy)
    }
}
