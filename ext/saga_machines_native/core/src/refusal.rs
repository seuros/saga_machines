use core::fmt;

/// Why the saga did not honour a call. A refused call changed nothing: not
/// the progress, not the pending action, not the settlement, not an attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// The saga has issued every attempt of its scope (2^48 - 1); it is not
    /// finished, it can only no longer hand out work.
    Exhausted,
    /// The action's attempt names nothing this saga can act on: it carries
    /// another scope or was never issued (answered before anything is looked
    /// up), or it is an earlier attempt of this saga that is neither the
    /// pending one nor the last one resolved. No history is kept beyond that.
    Stale,
    /// The attempt is the pending one or the last one resolved, but the call
    /// disagrees with what the saga recorded for it: another action key, or,
    /// for a repeat, another outcome. The record stands.
    Mismatch,
    /// A forward action is unresolved; its effect cannot be forgotten, so the
    /// saga cannot be cancelled until it is resolved.
    Busy,
    /// Every forward step succeeded; a complete saga cannot be cancelled.
    Concluded,
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Exhausted => "every attempt of this scope has been issued",
            Self::Stale => "the attempt is not one this saga can act on",
            Self::Mismatch => "the call disagrees with what is recorded for this attempt",
            Self::Busy => "a forward action is unresolved",
            Self::Concluded => "the saga is complete",
        })
    }
}

impl core::error::Error for Refusal {}
