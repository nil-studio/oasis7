//! Memory-class compatibility for retained semantic qualifications.
use oasis7_proto::memory_budget::MemoryClass;

/// An Ordinary admission is at least as strict as a Recovery admission. A
/// Recovery-only qualification must be explicitly readmitted before use by an
/// Ordinary caller; an Ordinary qualification can also be used by Recovery.
pub(crate) const fn allows(minted: MemoryClass, requested: MemoryClass) -> bool {
    matches!(minted, MemoryClass::Ordinary) || matches!(requested, MemoryClass::Recovery)
}
