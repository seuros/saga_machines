use periscope_machines::scope_of;

/// Workflow identity in the dedup namespace: the caller's, never issued here.
///
/// It names the saga to the outside world and is part of every
/// [`ActionKey`](crate::ActionKey). Keeping it unique within one ledger is
/// the caller's job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SagaId(u64);

impl SagaId {
    /// Adopt a caller-owned identity.
    #[must_use]
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    /// Derive a stable identity from an external name, such as a manifest id
    /// (FNV-1a: the same bytes give the same identity in every process).
    #[must_use]
    pub const fn from_bytes(bytes: &[u8]) -> Self {
        let mut hash = 0xcbf2_9ce4_8422_2325_u64;
        let mut index = 0;
        while index < bytes.len() {
            hash ^= bytes[index] as u64;
            hash = hash.wrapping_mul(0x0100_0000_01b3);
            index += 1;
        }
        Self(hash)
    }

    /// Raw identity for persistence.
    #[must_use]
    pub const fn raw(self) -> u64 {
        self.0
    }
}

/// One issued attempt: the handle [`Saga::resolve`](crate::Saga::resolve)
/// checks.
///
/// Issued once per saga and never again: it carries the saga's scope in its
/// top sixteen bits above a sequence that starts at one. It is a plain
/// integer underneath ([`AttemptId::raw`]), so it can cross a queue or a
/// database column and be rebuilt with [`AttemptId::from_raw`]; a rebuilt id
/// the saga never issued is [`Refusal::Stale`](crate::Refusal::Stale).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AttemptId(u64);

impl AttemptId {
    /// Rebuild an id from its integer form. Any integer is accepted here; the
    /// saga decides whether it names an attempt.
    #[must_use]
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    /// The integer form: scope above, sequence below.
    #[must_use]
    pub const fn raw(self) -> u64 {
        self.0
    }

    /// The scope of the saga that issued this attempt.
    #[must_use]
    pub const fn scope(self) -> u16 {
        scope_of(self.0)
    }
}
